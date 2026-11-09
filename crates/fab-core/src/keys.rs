//! What each key of fabrial's TOML files holds, written for the reference
//! pages `fab manual` generates. Each file's parser keeps its table of
//! [`Key`]s beside its struct, and a test ties the table to the struct's
//! fields with [`fields`], so a key cannot be added without its entry.

use serde::Deserialize;
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

    #[test]
    fn fields_reads_a_structs_keys_in_order() {
        assert_eq!(fields::<Sample>(), ["first", "second"]);
        assert!(fields::<String>().is_empty());
    }
}
