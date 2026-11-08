//! A repository's fabrial settings, kept in `.github/fabrial.toml`.

use serde::Deserialize;

use crate::Error;

/// Where the settings live, relative to the repository root.
pub const SETTINGS_PATH: &str = ".github/fabrial.toml";

/// The values the workflows, the gate and the skills read.
///
/// The workflows read each value with `sed`, so every value is one line of
/// the form `key = "value"` and may not hold a quote, a backslash or a line
/// break. `disposable` is the one list, `key = ["a", "b"]` on one line, and
/// no workflow reads it. [`Settings::validate`] enforces that.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// GitHub login of the person who approves pull requests.
    pub owner: String,
    /// The `Co-authored-by` value on every squash commit.
    pub trailer: String,
    /// The command agents run before pushing.
    pub gates: String,
    /// The `Signed-off-by` value that ends every squash commit, for
    /// repositories that use the Developer Certificate of Origin. `None`
    /// leaves the line out.
    #[serde(default)]
    pub signoff: Option<String>,
    /// `"on"` adds an `Assisted-by: <model>` line to the squash commit for
    /// each agent model the pull request's commits name; `"off"` or no value
    /// keeps model names off `main`.
    #[serde(default)]
    pub assisted_by: Option<String>,
    /// Ignored paths, relative to a worktree's root, that `fab cleanup` may
    /// delete with a merged worktree beside Cargo's `target/`: caches a tool
    /// rebuilds, such as a tracker's local database. A path ending in `/`
    /// names a directory.
    #[serde(default)]
    pub disposable: Option<Vec<String>>,
    /// The fabrial version that last wrote the installed files.
    #[serde(default)]
    pub version: Option<String>,
    /// The commands `fab run` uses to reach the repository's task source.
    /// `None` leaves a run to GitHub issues and description files.
    #[serde(default)]
    pub tasks: Option<Tasks>,
}

/// The commands for each step of a tracked task, in a `[tasks]` table. Each
/// is split on whitespace and run without a shell in the task's worktree,
/// after `{id}`, `{evidence}` and `{file}` in a word are replaced. A
/// replacement stays one argument, whatever it holds.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tasks {
    /// Prints the task, given `{id}`.
    pub show: String,
    /// Starts the task, given `{id}`.
    pub start: String,
    /// Completes the task, given `{id}` and `{evidence}`.
    pub complete: String,
    /// Records a handoff note, given `{id}` and the note's `{file}`.
    pub handoff: String,
}

impl Tasks {
    /// Each step's name, command and the placeholders it must hold.
    pub fn steps(&self) -> [(&'static str, &str, &'static [&'static str]); 4] {
        [
            ("show", &self.show, &["{id}"]),
            ("start", &self.start, &["{id}"]),
            ("complete", &self.complete, &["{id}", "{evidence}"]),
            ("handoff", &self.handoff, &["{id}", "{file}"]),
        ]
    }

    /// The argument list for a command, with each placeholder replaced.
    pub fn argv(command: &str, values: &[(&str, &str)]) -> Vec<String> {
        command
            .split_whitespace()
            .map(|word| values.iter().fold(word.to_string(), |w, (k, v)| w.replace(k, v)))
            .collect()
    }
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
        if let Some(value) = &self.assisted_by
            && value != "on"
            && value != "off"
        {
            return Err(Error::Settings(format!(
                "`assisted_by` is {value:?}, not \"on\" or \"off\""
            )));
        }
        for path in self.disposable.iter().flatten() {
            let bad = path.is_empty()
                || path.starts_with('/')
                || path.split('/').any(|part| part == "..")
                || path.contains(['"', '\\', '\n', '\r']);
            if bad {
                return Err(Error::Settings(format!(
                    "`disposable` holds {path:?}; each path is relative to the worktree, without `..`, quotes or backslashes"
                )));
            }
        }
        let signoff = self.signoff.as_ref().map(|s| ("signoff", s));
        for (key, value) in [
            ("owner", &self.owner),
            ("trailer", &self.trailer),
            ("gates", &self.gates),
        ]
        .into_iter()
        .chain(signoff)
        {
            if value.is_empty() {
                return Err(Error::Settings(format!("`{key}` is empty")));
            }
            if value.contains(['"', '\\', '\n', '\r']) {
                return Err(Error::Settings(format!(
                    "`{key}` holds a quote, a backslash or a line break, which the workflows cannot read"
                )));
            }
        }
        for (step, command, needs) in self.tasks.iter().flat_map(Tasks::steps) {
            if command.contains(['"', '\\', '\n', '\r']) {
                return Err(Error::Settings(format!(
                    "`tasks.{step}` holds a quote, a backslash or a line break"
                )));
            }
            if let Some(missing) = needs.iter().find(|p| !command.contains(*p)) {
                return Err(Error::Settings(format!("`tasks.{step}` does not use {missing}")));
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

    /// Whether the squash commit names the agent models that worked on it.
    pub fn assisted(&self) -> bool {
        self.assisted_by.as_deref() == Some("on")
    }

    /// Renders the settings file, recording `version` as the installing
    /// fabrial version. `signoff` is written only when it is set.
    pub fn render(&self, version: &str) -> String {
        let signoff = self.signoff.as_ref().map_or_else(String::new, |s| {
            format!(
                "\n# The Signed-off-by trailer that ends every squash commit; the owner\n\
                 # certifies agent work at merge.\n\
                 signoff = \"{s}\"\n"
            )
        });
        let assisted = self.assisted_by.as_ref().map_or_else(String::new, |a| {
            format!(
                "\n# \"on\" adds an Assisted-by line to every squash commit for each agent\n\
                 # model its pull request's commits name.\n\
                 assisted_by = \"{a}\"\n"
            )
        });
        let disposable = self.disposable.as_ref().map_or_else(String::new, |paths| {
            let list = paths.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
            format!(
                "\n# Ignored paths fab cleanup may delete with a merged worktree, beside\n\
                 # target/: caches a tool rebuilds.\n\
                 disposable = [{list}]\n"
            )
        });
        let tasks = self.tasks.as_ref().map_or_else(String::new, |t| {
            format!(
                "\n# The commands fab run uses for each step of a tracked task. Each is\n\
                 # split on spaces and run in the task's worktree; {{id}}, {{evidence}}\n\
                 # and {{file}} are replaced.\n\
                 [tasks]\n\
                 show = \"{}\"\n\
                 start = \"{}\"\n\
                 complete = \"{}\"\n\
                 handoff = \"{}\"\n",
                t.show, t.start, t.complete, t.handoff
            )
        });
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
             {signoff}\
             {assisted}\
             {disposable}\
             \n\
             # The fabrial version that wrote the installed files. `fab check`\n\
             # compares it with its own.\n\
             version = \"{}\"\n\
             {tasks}",
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
            signoff: None,
            assisted_by: None,
            disposable: None,
            version: None,
            tasks: None,
        }
    }

    fn tasks() -> Tasks {
        Tasks {
            show: "arcl task show {id}".into(),
            start: "arcl task start {id}".into(),
            complete: "arcl task complete {id} --evidence {evidence}".into(),
            handoff: "arcl task handoff {id} --note-file {file}".into(),
        }
    }

    #[test]
    fn task_commands_render_parse_back_and_keep_each_value_one_argument() {
        let with = Settings { tasks: Some(tasks()), ..settings() };
        let parsed = Settings::parse(&with.render("0.9.0")).unwrap();
        assert_eq!(parsed.tasks, Some(tasks()));
        assert_eq!(parsed.version.as_deref(), Some("0.9.0"));
        let argv = Tasks::argv(
            &tasks().complete,
            &[("{id}", "t-1"), ("{evidence}", "Done in #3 at abc")],
        );
        assert_eq!(
            argv,
            ["arcl", "task", "complete", "t-1", "--evidence", "Done in #3 at abc"]
        );
    }

    #[test]
    fn task_commands_need_their_placeholders() {
        let missing = Tasks { complete: "arcl task complete {id}".into(), ..tasks() };
        let err = Settings { tasks: Some(missing), ..settings() }.validate().unwrap_err();
        assert!(err.to_string().contains("tasks.complete"), "{err}");
        let text = format!("{}[tasks]\nshow = \"x {{id}}\"\n", settings().render("0.9.0"));
        assert!(Settings::parse(&text).is_err());
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
    fn signoff_is_written_only_when_set_and_parses_back() {
        let text = settings().render("0.1.0");
        assert!(!text.contains("signoff"));
        assert_eq!(Settings::parse(&text).unwrap().signoff, None);
        let mut signed = settings();
        signed.signoff = Some("Owais <desertthunder.dev@gmail.com>".into());
        let text = signed.render("0.1.0");
        assert!(
            text.lines()
                .any(|l| l == r#"signoff = "Owais <desertthunder.dev@gmail.com>""#)
        );
        assert_eq!(
            Settings::parse(&text).unwrap(),
            Settings { version: Some("0.1.0".into()), ..signed }
        );
    }

    #[test]
    fn assisted_by_is_on_or_off_and_written_only_when_set() {
        assert!(!settings().assisted());
        assert!(!settings().render("0.1.0").contains("assisted_by"));
        let mut on = settings();
        on.assisted_by = Some("on".into());
        let parsed = Settings::parse(&on.render("0.1.0")).unwrap();
        assert!(parsed.assisted());
        let mut bad = settings();
        bad.assisted_by = Some("yes".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn disposable_paths_round_trip_and_stay_inside_the_worktree() {
        assert!(!settings().render("0.1.0").contains("disposable"));
        let mut s = settings();
        s.disposable = Some(vec![".cache/db".into(), "tmp/".into()]);
        let text = s.render("0.1.0");
        assert!(
            text.lines().any(|l| l == r#"disposable = [".cache/db", "tmp/"]"#),
            "{text}"
        );
        assert_eq!(Settings::parse(&text).unwrap().disposable, s.disposable);
        for bad in ["", "/etc/passwd", "../up", "a/../../b", "a\"b"] {
            let mut s = settings();
            s.disposable = Some(vec![bad.into()]);
            assert!(s.validate().is_err(), "{bad:?}");
        }
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
        let mut bad = settings();
        bad.signoff = Some("A <a@b>\nSigned-off-by: B <b@c>".into());
        assert!(bad.validate().is_err());
        let mut bad = settings();
        bad.signoff = Some(String::new());
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
