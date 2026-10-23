//! A repository's fabrial settings, kept in `.github/fabrial.toml`.

use serde::Deserialize;

use crate::Error;

/// Where the settings live, relative to the repository root.
pub const SETTINGS_PATH: &str = ".github/fabrial.toml";

/// The values the workflows, the gate and the skills read.
///
/// The workflows read each value with `sed`, so every value is one line of
/// the form `key = "value"` and may not hold a quote, a backslash or a line
/// break. [`Settings::validate`] enforces that.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// GitHub login of the person who approves pull requests.
    pub owner: String,
    /// The `Co-authored-by` value on every squash commit.
    pub trailer: String,
    /// The command agents run before pushing.
    pub gates: String,
    /// The fabrial version that last wrote the installed files.
    #[serde(default)]
    pub version: Option<String>,
}

impl Settings {
    /// Parses the settings file's text.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let settings: Self = toml::from_str(text).map_err(|e| Error::Settings(e.to_string()))?;
        settings.validate()?;
        Ok(settings)
    }

    /// Checks that every value can be written in the one-line form the
    /// workflows read.
    pub fn validate(&self) -> Result<(), Error> {
        for (key, value) in [
            ("owner", &self.owner),
            ("trailer", &self.trailer),
            ("gates", &self.gates),
        ] {
            if value.is_empty() {
                return Err(Error::Settings(format!("`{key}` is empty")));
            }
            if value.contains(['"', '\\', '\n', '\r']) {
                return Err(Error::Settings(format!(
                    "`{key}` holds a quote, a backslash or a line break, which the workflows cannot read"
                )));
            }
        }
        if self.owner.starts_with('@') {
            return Err(Error::Settings("`owner` is a login without the `@`".into()));
        }
        if !is_login(&self.owner) {
            return Err(Error::Settings(format!(
                "`owner` is not a GitHub login: {:?} (letters, digits and single hyphens, \
                 not at either end, up to 39 characters)",
                self.owner
            )));
        }
        Ok(())
    }

    /// Renders the settings file, recording `version` as the installing
    /// fabrial version.
    pub fn render(&self, version: &str) -> String {
        format!(
            "# fabrial's settings for this repository. The Branch PR workflow, the Review\n\
             # gate and the skills read them. Each value is one line: key = \"value\".\n\
             \n\
             # GitHub login of the person who approves pull requests; `fab init` writes\n\
             # CODEOWNERS from it.\n\
             owner = \"{}\"\n\
             \n\
             # The Co-authored-by trailer on every squash commit.\n\
             trailer = \"{}\"\n\
             \n\
             # The command agents run before pushing.\n\
             gates = \"{}\"\n\
             \n\
             # The fabrial version that wrote the installed files. `fab init --check`\n\
             # compares it with its own.\n\
             version = \"{}\"\n",
            self.owner, self.trailer, self.gates, version
        )
    }
}

/// Whether `owner` is a GitHub login: ASCII letters, digits and single
/// hyphens, not starting or ending with a hyphen, up to 39 characters.
fn is_login(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 39
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && !owner.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            owner: "desertthunder".into(),
            trailer: "Owais <desertthunder.dev@gmail.com>".into(),
            gates: "just check".into(),
            version: None,
        }
    }

    #[test]
    fn render_parses_back() {
        let parsed = Settings::parse(&settings().render("0.1.0+abc1234")).unwrap();
        assert_eq!(parsed.trailer, "Owais <desertthunder.dev@gmail.com>");
        assert_eq!(parsed.version.as_deref(), Some("0.1.0+abc1234"));
    }

    #[test]
    fn render_keeps_the_one_line_form_the_workflows_read() {
        let text = settings().render("0.1.0");
        assert!(
            text.lines()
                .any(|l| l == r#"trailer = "Owais <desertthunder.dev@gmail.com>""#)
        );
    }

    #[test]
    fn rejects_values_the_workflows_cannot_read() {
        let mut bad = settings();
        bad.trailer = r#"O "Q" <q@x>"#.into();
        assert!(bad.validate().is_err());
        let mut bad = settings();
        bad.owner = "@desertthunder".into();
        assert!(bad.validate().is_err());
        let mut bad = settings();
        bad.gates = String::new();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn owner_must_be_a_github_login() {
        for good in ["desertthunder", "a", "a-b-c", "A1", &"x".repeat(39)] {
            let mut s = settings();
            s.owner = good.to_string();
            assert!(s.validate().is_ok(), "{good}");
        }
        for bad in ["a b", "-a", "a-", "a--b", "a_b", "ä", &"x".repeat(40)] {
            let mut s = settings();
            s.owner = bad.to_string();
            assert!(s.validate().is_err(), "{bad}");
        }
    }
}
