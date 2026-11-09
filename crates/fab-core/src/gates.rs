//! Which checks cover which paths, kept in `.github/fabrial-gates.toml`.
//! `fab gates` reads it to run only the checks a change touches. The
//! repository owns the file: `fab init` writes a starter when there is none,
//! and `fab update` and `fab check` leave it alone.

use serde::Deserialize;

use crate::Error;
use crate::keys::Key;

/// Where the gates live, relative to the repository root.
pub const GATES_PATH: &str = ".github/fabrial-gates.toml";
/// The top-level keys of [`Gates`], as the reference pages describe them.
pub const KEYS: &[Key] = &[
    Key {
        name: "full",
        kind: "string",
        default: "required",
        about: "Command that runs every check. `fab gates --full` runs it, and so does a changed file no \
                scope names when `unmatched` is `full`.",
    },
    Key {
        name: "unmatched",
        kind: "`full` or `skip`",
        default: "`full`",
        about: "What a changed file that no scope names runs: the `full` command, or nothing.",
    },
    Key {
        name: "scoped",
        kind: "array of tables",
        default: "none",
        about: "Checks for parts of the repository, as `[[scoped]]` tables, in the order they run.",
    },
];
/// The keys of each `[[scoped]]` table, [`Scope`], as the reference pages
/// describe them.
pub const SCOPE_KEYS: &[Key] = &[
    Key {
        name: "name",
        kind: "string",
        default: "required",
        about: "Scope's name, as `fab gates` prints it. No two scopes share one.",
    },
    Key {
        name: "paths",
        kind: "list of strings",
        default: "required",
        about: "Globs over paths from the repository root. `*` and `?` match within one path segment, and a \
                `**` segment matches any number of segments.",
    },
    Key {
        name: "run",
        kind: "string",
        default: "required",
        about: "Command, run with `sh -c` at the repository root.",
    },
];

/// What a changed file that no scope names runs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Unmatched {
    /// The full gate command.
    #[default]
    Full,
    /// Nothing.
    Skip,
}

/// The gates file.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Gates {
    /// The command that runs every check, for `fab gates --full`.
    pub full: String,
    /// What a changed file that no scope names runs.
    #[serde(default)]
    pub unmatched: Unmatched,
    /// The checks for parts of the repository, in the order they run.
    #[serde(default)]
    pub scoped: Vec<Scope>,
}

/// One part of the repository and the command that checks it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// The scope's name, as `fab gates` prints it.
    pub name: String,
    /// Globs over paths from the repository root: `*` and `?` match within
    /// one path segment, and a `**` segment matches any number of segments.
    pub paths: Vec<String>,
    /// The command, run with `sh -c` at the repository root.
    pub run: String,
}

impl Gates {
    /// Parses the gates file's text and checks it: a command for `full` and
    /// each scope, at least one glob per scope, and no two scopes with one
    /// name.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let gates: Self = toml::from_str(text).map_err(|e| Error::Gates(e.to_string()))?;
        if gates.full.trim().is_empty() {
            return Err(Error::Gates("`full` is empty".into()));
        }
        for (n, scope) in gates.scoped.iter().enumerate() {
            if scope.name.trim().is_empty() {
                return Err(Error::Gates(format!("scope {} has no name", n + 1)));
            }
            if gates.scoped[..n].iter().any(|s| s.name == scope.name) {
                return Err(Error::Gates(format!("two scopes are named `{}`", scope.name)));
            }
            if scope.run.trim().is_empty() {
                return Err(Error::Gates(format!("scope `{}` has an empty `run`", scope.name)));
            }
            if scope.paths.iter().all(|p| p.trim().is_empty()) {
                return Err(Error::Gates(format!("scope `{}` names no paths", scope.name)));
            }
        }
        Ok(gates)
    }

    /// The starter `fab init` writes, running `full` for every change until
    /// the repository adds scopes.
    pub fn starter(full: &str) -> String {
        format!(
            "# The checks `fab gates` runs, by the paths a change touches. It runs each\n\
             # scope whose `paths` match a changed file. Documentation and planning\n\
             # records run nothing unless a scope names them. `unmatched` says what a\n\
             # changed file no scope names runs: \"full\" or \"skip\". `fab gates --full`\n\
             # runs `full`. Each command runs with `sh -c` at the repository root. This\n\
             # repository owns the file: `fab update` and `fab check` leave it alone.\n\
             \n\
             full = \"{full}\"\n\
             unmatched = \"full\"\n\
             \n\
             # [[scoped]]\n\
             # name = \"rust\"\n\
             # paths = [\"src/**\", \"Cargo.toml\", \"Cargo.lock\"]\n\
             # run = \"cargo test\"\n"
        )
    }
}

impl Scope {
    /// Whether any of the scope's globs matches `path`.
    pub fn matches(&self, path: &str) -> bool {
        self.paths.iter().any(|glob| glob_match(glob, path))
    }
}

/// Whether `glob` matches the whole of `path`, both `/`-separated from the
/// repository root. `*` matches any run of characters within a segment, `?`
/// one character, and a segment of `**` any number of whole segments,
/// including none.
pub fn glob_match(glob: &str, path: &str) -> bool {
    let globs: Vec<&str> = glob.split('/').collect();
    let parts: Vec<&str> = path.split('/').collect();
    segments_match(&globs, &parts)
}

fn segments_match(globs: &[&str], parts: &[&str]) -> bool {
    match globs.split_first() {
        None => parts.is_empty(),
        Some((&"**", rest)) => (0..=parts.len()).any(|n| segments_match(rest, &parts[n..])),
        Some((glob, rest)) => match parts.split_first() {
            Some((part, others)) => segment_match(glob.as_bytes(), part.as_bytes()) && segments_match(rest, others),
            None => false,
        },
    }
}

fn segment_match(glob: &[u8], text: &[u8]) -> bool {
    match glob.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|n| segment_match(rest, &text[n..])),
        Some((b'?', rest)) => !text.is_empty() && segment_match(rest, &text[1..]),
        Some((c, rest)) => text.first() == Some(c) && segment_match(rest, &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{fields, names};

    #[test]
    fn the_key_tables_match_the_gates_structs() {
        assert_eq!(names(KEYS), fields::<Gates>());
        assert_eq!(names(SCOPE_KEYS), fields::<Scope>());
    }

    #[test]
    fn globs_match_whole_paths_by_segment() {
        for (glob, path) in [
            ("crates/**", "crates/fab-cli/src/main.rs"),
            ("crates/**/*.rs", "crates/a.rs"),
            ("crates/**/*.rs", "crates/fab-cli/src/main.rs"),
            ("Cargo.toml", "Cargo.toml"),
            ("packages/pi/*.test.mjs", "packages/pi/extensions.test.mjs"),
            ("src/?.rs", "src/a.rs"),
            ("**", "a/b"),
        ] {
            assert!(glob_match(glob, path), "{glob} {path}");
        }
        for (glob, path) in [
            ("crates/*", "crates/fab-cli/src/main.rs"),
            ("Cargo.toml", "crates/fab-cli/Cargo.toml"),
            ("*.rs", "src/main.rs"),
            ("src/?.rs", "src/ab.rs"),
            ("crates/**", "cratesx/a"),
        ] {
            assert!(!glob_match(glob, path), "{glob} {path}");
        }
    }

    #[test]
    fn the_starter_parses_and_runs_full_for_everything() {
        let gates = Gates::parse(&Gates::starter("just check")).unwrap();
        assert_eq!(gates.full, "just check");
        assert_eq!(gates.unmatched, Unmatched::Full);
        assert!(gates.scoped.is_empty());
    }

    #[test]
    fn a_bad_file_is_refused() {
        let scope = |name: &str, paths: &str, run: &str| {
            format!("[[scoped]]\nname = \"{name}\"\npaths = [{paths}]\nrun = \"{run}\"\n")
        };
        let full = "full = \"just check\"\n";
        for (text, says) in [
            ("full = \"\"\n".to_string(), "`full` is empty"),
            (format!("{full}unmatched = \"never\"\n"), "unknown variant"),
            (format!("{full}gates = \"x\"\n"), "unknown field"),
            (
                format!("{full}{}{}", scope("a", "\"x\"", "t"), scope("a", "\"y\"", "t")),
                "two scopes",
            ),
            (format!("{full}{}", scope("a", "", "t")), "names no paths"),
            (format!("{full}{}", scope("a", "\"x\"", " ")), "empty `run`"),
            (format!("{full}{}", scope("", "\"x\"", "t")), "no name"),
        ] {
            let err = Gates::parse(&text).unwrap_err().to_string();
            assert!(err.contains(says), "{text}: {err}");
        }
        let ok = format!("{full}unmatched = \"skip\"\n{}", scope("a", "\"x/**\"", "t"));
        let gates = Gates::parse(&ok).unwrap();
        assert_eq!(gates.unmatched, Unmatched::Skip);
        assert!(gates.scoped[0].matches("x/y"));
    }
}
