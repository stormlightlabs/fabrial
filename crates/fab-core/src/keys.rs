//! What each key of fabrial's TOML files holds, written for the reference
//! pages `fab manual` generates. Each file's parser keeps its table of
//! [`Key`]s beside its struct, and a test ties the table to the struct with
//! [`assert_keys_match`], so a key cannot be added without its entry or
//! change from required to optional without the table saying so.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde::de::{self, Deserializer, Visitor};

/// One key of a TOML file, as its reference page describes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// The key's name in the file.
    pub name: &'static str,
    /// What the value is, such as `string` or `list of strings`.
    pub kind: &'static str,
    /// What an absent key means: `required`, `unset`, or the default value.
    pub default: &'static str,
    /// What the key does, in Markdown. Text outside backticks holds no `<`,
    /// `>`, `{` or `}`, which the docs site would read as markup.
    pub about: &'static str,
}

/// Records the field names a derived [`Deserialize`] passes to
/// `deserialize_struct`, then stops.
struct Fields(&'static [&'static str]);

impl<'de> Deserializer<'de> for &mut Fields {
    type Error = de::value::Error;

    fn deserialize_struct<V: Visitor<'de>>(
        self, _name: &'static str, fields: &'static [&'static str], _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0 = fields;
        Err(de::Error::custom("fields recorded"))
    }

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(de::Error::custom("not a struct"))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit
        unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
    }
}

/// The keys a struct with a derived [`Deserialize`] reads, in declaration
/// order, or none for a type that is not a struct.
pub fn fields<'de, T: Deserialize<'de>>() -> &'static [&'static str] {
    let mut fields = Fields(&[]);
    let _ = T::deserialize(&mut fields);
    fields.0
}

/// The names in a table of keys, for comparing with [`fields`].
pub fn names(keys: &[Key]) -> Vec<&'static str> {
    keys.iter().map(|k| k.name).collect()
}

/// Panics when `keys` names other fields than `T`, in another order, or
/// marks a key `required` that `T` does not require, or the reverse. For
/// tests. `entry` gives the TOML for one key with a valid value, such as
/// `name = "x"` or a `[[stage]]` table; the required keys' entries are
/// joined in table order, so a table entry goes after the plain keys.
pub fn assert_keys_match<T: DeserializeOwned>(keys: &[Key], entry: impl Fn(&str) -> String) {
    assert_eq!(
        names(keys),
        fields::<T>(),
        "the table of keys and the struct's fields differ"
    );
    let required: Vec<&Key> = keys.iter().filter(|k| k.default == "required").collect();
    let all: String = required.iter().map(|k| entry(k.name)).collect();
    if let Err(e) = toml::from_str::<T>(&all) {
        panic!("the required keys alone do not parse, so a key the table calls optional is required: {e}");
    }
    for key in &required {
        let without: String = required
            .iter()
            .filter(|k| k.name != key.name)
            .map(|k| entry(k.name))
            .collect();
        assert!(toml::from_str::<T>(&without).is_err(), "`{}` is not required", key.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct Sample {
        first: String,
        #[serde(default)]
        second: Option<u8>,
    }

    const SAMPLE_KEYS: &[Key] = &[
        Key { name: "first", kind: "string", default: "required", about: "" },
        Key { name: "second", kind: "integer", default: "unset", about: "" },
    ];

    fn entry(key: &str) -> String {
        match key {
            "second" => "second = 1\n".into(),
            _ => format!("{key} = \"x\"\n"),
        }
    }

    #[test]
    fn fields_reads_a_structs_keys_in_order() {
        assert_eq!(fields::<Sample>(), ["first", "second"]);
        assert!(fields::<String>().is_empty());
    }

    #[test]
    fn a_matching_table_passes() {
        assert_keys_match::<Sample>(SAMPLE_KEYS, entry);
    }

    #[test]
    #[should_panic(expected = "a key the table calls optional is required")]
    fn a_required_key_the_table_calls_optional_fails() {
        assert_keys_match::<Sample>(&[Key { default: "unset", ..SAMPLE_KEYS[0] }, SAMPLE_KEYS[1]], entry);
    }

    #[test]
    #[should_panic(expected = "`second` is not required")]
    fn an_optional_key_the_table_calls_required_fails() {
        assert_keys_match::<Sample>(&[SAMPLE_KEYS[0], Key { default: "required", ..SAMPLE_KEYS[1] }], entry);
    }
}
