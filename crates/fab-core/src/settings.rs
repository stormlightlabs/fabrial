//! A repository's fabrial settings, kept in `.github/fabrial.toml`.

use serde::Deserialize;

use crate::Error;

/// Where the settings live, relative to the repository root.
pub const SETTINGS_PATH: &str = ".github/fabrial.toml";
/// Directories that hold workflows, settings or what agents read, in
/// lowercase. Every file under one is configuration, so none can hold
/// planning records.
pub const CONFIG_DIRS: &[&str] = &[".github", ".claude", ".agents", ".codex"];
/// The secret prefix `fab init` writes when none is given.
pub const DEFAULT_SECRET_PREFIX: &str = "FABRIAL";
/// The secret prefix an install without the setting reads: the names
/// fabrial 0.9.0 and earlier read, which `fab update` writes into it.
pub const LEGACY_SECRET_PREFIX: &str = "THUNDERBOT";

/// The values the workflows, the gate and the skills read.
///
/// The workflows read each value with `sed`, so every value is one line of
/// the form `key = "value"` and may not hold a quote, a backslash or a line
/// break. `disposable` and `planning` are lists, `key = ["a", "b"]` on one
/// line, and no workflow reads them. [`Settings::validate`] enforces that.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// GitHub login of the person who approves pull requests.
    pub owner: String,
    /// The slug of the repository's GitHub App, the one the workflows act
    /// as: `fabrial-bot` in `github.com/apps/fabrial-bot`. Its bot login is
    /// [`Settings::bot`]. `fab init` and `fab update` need it; the file
    /// still parses without it so `fab update --app` can add it.
    #[serde(default)]
    pub app: Option<String>,
    /// The prefix of the secrets that hold the App's ID and key: the
    /// workflows read `<prefix>_APP_ID` and `<prefix>_PRIVATE_KEY`. `None`
    /// reads [`LEGACY_SECRET_PREFIX`]; see [`Settings::secret_prefix`].
    #[serde(default)]
    pub secret_prefix: Option<String>,
    /// The GitHub environment every job that mints the App's token names,
    /// an opt-in guard. Kept as environment secrets with a deployment rule
    /// of `main`, the App's secrets reach no job on another ref. `None`
    /// names no environment, and the jobs read repository and organization
    /// secrets.
    #[serde(default)]
    pub environment: Option<String>,
    /// The `Co-authored-by` value on every squash commit.
    pub trailer: String,
    /// The command agents run before pushing. `fab gates` runs it when the
    /// repository has no `.github/fabrial-gates.toml`, and `fab init` writes
    /// it into that file's starter as `full`.
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
    /// Directories, relative to the repository root and each ending in `/`,
    /// whose Markdown and TOML files are planning records, such as a
    /// tracker's task files. `fab review-depth` gives a pull request that
    /// changes only those, documentation and tests the standard pass. None
    /// may sit under a directory [`CONFIG_DIRS`] names, and configuration
    /// under one stays code.
    #[serde(default)]
    pub planning: Option<Vec<String>>,
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
    /// Prepares a new worktree for the other commands, run once before
    /// `start`, such as a tracker that keeps a local cache per checkout.
    /// Takes no placeholders. `None` skips it.
    #[serde(default)]
    pub init: Option<String>,
    /// The skill under `.claude/skills` that `fab dispatch` gives an
    /// implementer for working the tracker, such as `arclightning`. A
    /// plain directory name. `None` gives none.
    #[serde(default)]
    pub skill: Option<String>,
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
        if let Some(app) = &self.app
            && !is_app_slug(app)
        {
            let hint = if app.ends_with("[bot]") { "; give the slug without `[bot]`" } else { "" };
            return Err(Error::Settings(format!(
                "`app` is {app:?}, not a GitHub App slug (lowercase letters, digits and hyphens, \
                 not at either end, up to 34 characters){hint}"
            )));
        }
        if let Some(prefix) = &self.secret_prefix
            && !is_secret_prefix(prefix)
        {
            return Err(Error::Settings(format!(
                "`secret_prefix` is {prefix:?}, not a secret name prefix (uppercase letters, digits and \
                 underscores, not starting with a digit or GITHUB_)"
            )));
        }
        if let Some(name) = &self.environment
            && !(is_name(name) && name.len() <= 255)
        {
            return Err(Error::Settings(format!(
                "`environment` is {name:?}, not an environment name (ASCII letters, digits, `-` and `_`, \
                 up to 255 characters)"
            )));
        }
        if let Some(value) = &self.assisted_by
            && value != "on"
            && value != "off"
        {
            return Err(Error::Settings(format!(
                "`assisted_by` is {value:?}, not \"on\" or \"off\""
            )));
        }
        for path in self.disposable.iter().flatten() {
            if !is_relative(path) {
                return Err(Error::Settings(format!(
                    "`disposable` holds {path:?}; each path is relative to the worktree, without `..`, quotes or backslashes"
                )));
            }
        }
        for path in self.planning.iter().flatten() {
            let parts = || path.strip_suffix('/').unwrap_or(path).split('/');
            if !is_relative(path) || !path.ends_with('/') || parts().any(|part| part.is_empty() || part == ".") {
                return Err(Error::Settings(format!(
                    "`planning` holds {path:?}; each is a directory relative to the repository, ending in `/`, \
                     without `.`, `..`, quotes or backslashes"
                )));
            }
            if parts().any(|part| CONFIG_DIRS.contains(&part.to_ascii_lowercase().as_str())) {
                return Err(Error::Settings(format!(
                    "`planning` holds {path:?}; a directory under {} holds configuration, not planning records",
                    CONFIG_DIRS.join(", ")
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
            if value.trim().is_empty() {
                return Err(Error::Settings(format!("`{key}` is empty")));
            }
            if value.contains(['"', '\\', '\n', '\r']) {
                return Err(Error::Settings(format!(
                    "`{key}` holds a quote, a backslash or a line break, which the workflows cannot read"
                )));
            }
        }
        let init = self.tasks.iter().filter_map(|t| t.init.as_deref());
        for command in init {
            if command.trim().is_empty() || command.contains(['"', '\\', '\n', '\r']) {
                return Err(Error::Settings(
                    "`tasks.init` is empty or holds a quote, a backslash or a line break".into(),
                ));
            }
        }
        for skill in self.tasks.iter().filter_map(|t| t.skill.as_deref()) {
            if !is_name(skill) {
                return Err(Error::Settings(format!(
                    "`tasks.skill` is {skill:?}; it names a skill directory: ASCII letters, digits, `-` and `_`"
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

    /// The login the App's comments and commits carry, `<app>[bot]`, or
    /// `None` when the settings name no App.
    pub fn bot(&self) -> Option<String> {
        self.app.as_ref().map(|app| format!("{app}[bot]"))
    }

    /// The prefix the workflows' secret names take: the setting, or
    /// [`LEGACY_SECRET_PREFIX`] when it is not set.
    pub fn secret_prefix(&self) -> &str {
        self.secret_prefix.as_deref().unwrap_or(LEGACY_SECRET_PREFIX)
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
        let app = self.app.as_ref().map_or_else(String::new, |a| {
            format!(
                "\n# The slug of the GitHub App the workflows act as, from its page\n\
                 # github.com/apps/<slug>. Its comments come from <slug>[bot].\n\
                 app = \"{a}\"\n"
            )
        });
        let prefix = self.secret_prefix.as_ref().map_or_else(String::new, |p| {
            format!(
                "\n# The prefix of the secrets holding the App's ID and key: the workflows\n\
                 # read {p}_APP_ID and {p}_PRIVATE_KEY.\n\
                 secret_prefix = \"{p}\"\n"
            )
        });
        let environment = self.environment.as_ref().map_or_else(String::new, |e| {
            format!(
                "\n# The GitHub environment the jobs that mint the App's token name. Keep\n\
                 # the secrets in it, restricted to main, so no other ref reads them.\n\
                 environment = \"{e}\"\n"
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
        let planning = self.planning.as_ref().map_or_else(String::new, |paths| {
            let list = paths.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
            format!(
                "\n# Directories whose Markdown and TOML files are planning records, which\n\
                 # fab review-depth reviews as it does documentation.\n\
                 planning = [{list}]\n"
            )
        });
        let tasks = self.tasks.as_ref().map_or_else(String::new, |t| {
            let init = t
                .init
                .as_ref()
                .map_or_else(String::new, |i| format!("init = \"{i}\"\n"));
            let skill = t
                .skill
                .as_ref()
                .map_or_else(String::new, |k| format!("skill = \"{k}\"\n"));
            format!(
                "\n# The commands fab run uses for each step of a tracked task. Each is\n\
                 # split on spaces and run in the task's worktree; {{id}}, {{evidence}}\n\
                 # and {{file}} are replaced. init, when set, runs once in the new\n\
                 # worktree before start. skill names the skill fab dispatch gives\n\
                 # an implementer.\n\
                 [tasks]\n\
                 {init}\
                 {skill}\
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
             {app}\
             {prefix}\
             {environment}\
             \n\
             # The Co-authored-by trailer on every squash commit.\n\
             trailer = \"{}\"\n\
             \n\
             # The command agents run before pushing.\n\
             gates = \"{}\"\n\
             {signoff}\
             {assisted}\
             {disposable}\
             {planning}\
             \n\
             # The fabrial version that wrote the installed files. `fab check`\n\
             # compares it with its own.\n\
             version = \"{}\"\n\
             {tasks}",
            self.owner, self.trailer, self.gates, version
        )
    }
}

/// Whether a listed path is relative and stays inside the repository, with
/// nothing the one-line form cannot hold.
fn is_relative(path: &str) -> bool {
    !(path.is_empty()
        || path.starts_with('/')
        || path.split('/').any(|part| part == "..")
        || path.contains(['"', '\\', '\n', '\r']))
}

/// Whether `app` is a GitHub App slug: lowercase ASCII letters, digits and
/// hyphens, not starting or ending with a hyphen, up to the 34 characters an
/// App's name may hold.
fn is_app_slug(app: &str) -> bool {
    !app.is_empty()
        && app.len() <= 34
        && app
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !app.starts_with('-')
        && !app.ends_with('-')
}

/// Whether `prefix` can start a secret name GitHub accepts: uppercase ASCII
/// letters, digits and underscores, not starting with a digit or with the
/// `GITHUB_` GitHub reserves.
fn is_secret_prefix(prefix: &str) -> bool {
    !prefix.is_empty()
        && prefix
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !prefix.starts_with(|c: char| c.is_ascii_digit())
        && prefix != "GITHUB"
        && !prefix.starts_with("GITHUB_")
}

/// Whether a skill name is a plain directory name: ASCII letters, digits,
/// `-` and `_`, so it cannot leave `.claude/skills`.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
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
            app: Some("fabrial-bot".into()),
            secret_prefix: Some("FABRIAL".into()),
            environment: Some("fabrial".into()),
            trailer: "Owais <desertthunder.dev@gmail.com>".into(),
            gates: "just check".into(),
            signoff: None,
            assisted_by: None,
            disposable: None,
            planning: None,
            version: None,
            tasks: None,
        }
    }

    fn tasks() -> Tasks {
        Tasks {
            init: None,
            skill: None,
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
        let init = Tasks { init: Some("arcl init".into()), ..tasks() };
        let text = Settings { tasks: Some(init.clone()), ..settings() }.render("0.9.0");
        assert!(text.lines().any(|l| l == r#"init = "arcl init""#), "{text}");
        assert_eq!(Settings::parse(&text).unwrap().tasks, Some(init));
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
    fn the_task_skill_renders_parses_back_and_is_a_plain_name() {
        let with = Tasks { skill: Some("arclightning".into()), ..tasks() };
        let text = Settings { tasks: Some(with.clone()), ..settings() }.render("0.9.0");
        assert!(text.lines().any(|l| l == r#"skill = "arclightning""#), "{text}");
        assert_eq!(Settings::parse(&text).unwrap().tasks, Some(with));
        for bad in ["", "../x", "a/b", ".", "a b", "a\"b"] {
            let tasks = Tasks { skill: Some(bad.into()), ..tasks() };
            let err = Settings { tasks: Some(tasks), ..settings() }.validate().unwrap_err();
            assert!(err.to_string().contains("tasks.skill"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn task_commands_need_their_placeholders() {
        let missing = Tasks { complete: "arcl task complete {id}".into(), ..tasks() };
        let err = Settings { tasks: Some(missing), ..settings() }.validate().unwrap_err();
        assert!(err.to_string().contains("tasks.complete"), "{err}");
        let text = format!("{}[tasks]\nshow = \"x {{id}}\"\n", settings().render("0.9.0"));
        assert!(Settings::parse(&text).is_err());
        let quoted = Tasks { init: Some("arcl \\ init".into()), ..tasks() };
        let err = Settings { tasks: Some(quoted), ..settings() }.validate().unwrap_err();
        assert!(err.to_string().contains("tasks.init"), "{err}");
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
    fn planning_directories_round_trip_and_stay_inside_the_repository() {
        assert!(!settings().render("0.1.0").contains("planning"));
        let mut s = settings();
        s.planning = Some(vec!["plans/".into(), ".tracker/tasks/".into()]);
        let text = s.render("0.1.0");
        assert!(
            text.lines().any(|l| l == r#"planning = ["plans/", ".tracker/tasks/"]"#),
            "{text}"
        );
        assert_eq!(Settings::parse(&text).unwrap().planning, s.planning);
        for bad in [
            "",
            "/",
            "plans",
            "/plans/",
            "../plans/",
            "a/../b/",
            "./plans/",
            "plans//",
            "a//b/",
            "a\"b/",
            ".github/",
            ".GitHub/",
            ".github/workflows/",
            ".claude/",
            "docs/.claude/",
            ".agents/skills/",
        ] {
            let mut s = settings();
            s.planning = Some(vec![bad.into()]);
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
        bad.gates = " ".into();
        assert!(bad.validate().is_err());
        let mut bad = settings();
        bad.signoff = Some("A <a@b>\nSigned-off-by: B <b@c>".into());
        assert!(bad.validate().is_err());
        let mut bad = settings();
        bad.signoff = Some(String::new());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn the_app_renders_parses_back_and_names_its_bot() {
        let text = settings().render("0.1.0");
        assert!(text.lines().any(|l| l == r#"app = "fabrial-bot""#), "{text}");
        let parsed = Settings::parse(&text).unwrap();
        assert_eq!(parsed.app.as_deref(), Some("fabrial-bot"));
        assert_eq!(parsed.bot().as_deref(), Some("fabrial-bot[bot]"));
        let none = Settings { app: None, ..settings() };
        assert!(!none.render("0.1.0").contains("app ="));
        assert_eq!(Settings::parse(&none.render("0.1.0")).unwrap().bot(), None);
    }

    #[test]
    fn the_app_must_be_a_slug() {
        for good in ["stormlight-thunderbot", "a", "bot2", &"x".repeat(34)] {
            let s = Settings { app: Some(good.to_string()), ..settings() };
            assert!(s.validate().is_ok(), "{good}");
        }
        for bad in ["", "-a", "a-", "My-App", "a b", "a_b", "a\"b", &"x".repeat(35)] {
            let s = Settings { app: Some(bad.to_string()), ..settings() };
            let err = s.validate().unwrap_err().to_string();
            assert!(err.contains("`app`"), "{bad}: {err}");
        }
        let s = Settings { app: Some("fabrial-bot[bot]".into()), ..settings() };
        assert!(s.validate().unwrap_err().to_string().contains("without `[bot]`"));
    }

    #[test]
    fn the_secret_prefix_renders_and_parses_back() {
        let text = Settings { secret_prefix: Some("THUNDERBOT".into()), ..settings() }.render("0.1.0");
        assert!(text.lines().any(|l| l == r#"secret_prefix = "THUNDERBOT""#), "{text}");
        let parsed = Settings::parse(&text).unwrap();
        assert_eq!(parsed.secret_prefix(), "THUNDERBOT");
        let none = Settings { secret_prefix: None, ..settings() };
        assert!(!none.render("0.1.0").contains("secret_prefix"));
        assert_eq!(
            Settings::parse(&none.render("0.1.0")).unwrap().secret_prefix(),
            LEGACY_SECRET_PREFIX
        );
    }

    #[test]
    fn the_environment_renders_and_parses_back_only_when_set() {
        let text = Settings { environment: Some("own_env".into()), ..settings() }.render("0.1.0");
        assert!(text.lines().any(|l| l == r#"environment = "own_env""#), "{text}");
        assert_eq!(Settings::parse(&text).unwrap().environment.as_deref(), Some("own_env"));
        let none = Settings { environment: None, ..settings() };
        assert!(!none.render("0.1.0").contains("environment ="));
        assert_eq!(Settings::parse(&none.render("0.1.0")).unwrap().environment, None);
    }

    #[test]
    fn the_environment_must_be_a_plain_name() {
        for good in ["fabrial", "Bot-Secrets_2", &"x".repeat(255)] {
            let s = Settings { environment: Some(good.to_string()), ..settings() };
            assert!(s.validate().is_ok(), "{good}");
        }
        for bad in ["", "a b", "a/b", "a.b", "a\"b", "a:b", &"x".repeat(256)] {
            let s = Settings { environment: Some(bad.to_string()), ..settings() };
            let err = s.validate().unwrap_err().to_string();
            assert!(err.contains("`environment`"), "{bad}: {err}");
        }
    }

    #[test]
    fn the_secret_prefix_must_start_a_secret_name() {
        for good in ["FABRIAL", "THUNDERBOT", "A", "_X", "MY_BOT2"] {
            let s = Settings { secret_prefix: Some(good.into()), ..settings() };
            assert!(s.validate().is_ok(), "{good}");
        }
        for bad in [
            "",
            "fabrial",
            "My_Bot",
            "2BOT",
            "GITHUB",
            "GITHUB_BOT",
            "A-B",
            "A B",
            "A\"B",
        ] {
            let s = Settings { secret_prefix: Some(bad.into()), ..settings() };
            let err = s.validate().unwrap_err().to_string();
            assert!(err.contains("`secret_prefix`"), "{bad}: {err}");
        }
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
