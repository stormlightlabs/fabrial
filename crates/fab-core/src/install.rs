//! Installs fabrial into a repository by copying files, and reports where an
//! installed copy has drifted from this build.
//!
//! Most files are built into the binary from this repository. `CODEOWNERS`,
//! the settings file and a section of `AGENTS.md` are written from the
//! repository's [`Settings`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::settings::{SETTINGS_PATH, Settings};
use crate::{Error, version};

/// The line that opens fabrial's section of `AGENTS.md`.
pub const AGENTS_BEGIN: &str = "<!-- fabrial:begin -->";
/// The line that closes fabrial's section of `AGENTS.md`.
pub const AGENTS_END: &str = "<!-- fabrial:end -->";
/// The symlink Codex and Pi read skills through.
pub const SKILLS_LINK: &str = ".agents/skills";

const SKILLS_TARGET: &str = "../.claude/skills";
const AGENTS_SECTION: &str = include_str!("agents-section.md");

/// The files copied as they are, with whether each is executable.
const EMBEDDED: &[(&str, &str, bool)] = &[
    (
        ".github/workflows/branch-pr.yml",
        include_str!("../../../.github/workflows/branch-pr.yml"),
        false,
    ),
    (
        ".github/workflows/review.yml",
        include_str!("../../../.github/workflows/review.yml"),
        false,
    ),
    (
        ".github/workflows/comment.yml",
        include_str!("../../../.github/workflows/comment.yml"),
        false,
    ),
    (
        ".github/storm/gate.sh",
        include_str!("../../../.github/storm/gate.sh"),
        true,
    ),
    (
        ".claude/agents/adversarial-reviewer.md",
        include_str!("../../../.claude/agents/adversarial-reviewer.md"),
        false,
    ),
    (
        ".claude/agents/implementer.md",
        include_str!("../../../.claude/agents/implementer.md"),
        false,
    ),
    (
        ".claude/agents/reviewer.md",
        include_str!("../../../.claude/agents/reviewer.md"),
        false,
    ),
    (
        ".claude/agents/reviser.md",
        include_str!("../../../.claude/agents/reviser.md"),
        false,
    ),
    (
        ".claude/agents/verifier.md",
        include_str!("../../../.claude/agents/verifier.md"),
        false,
    ),
    (
        ".claude/skills/adv-rev/SKILL.md",
        include_str!("../../../.claude/skills/adv-rev/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/handoff/SKILL.md",
        include_str!("../../../.claude/skills/handoff/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/implement/SKILL.md",
        include_str!("../../../.claude/skills/implement/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/resync/SKILL.md",
        include_str!("../../../.claude/skills/resync/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/review/SKILL.md",
        include_str!("../../../.claude/skills/review/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/review/references/reading-the-diff.md",
        include_str!("../../../.claude/skills/review/references/reading-the-diff.md"),
        false,
    ),
    (
        ".claude/skills/revise/SKILL.md",
        include_str!("../../../.claude/skills/revise/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/storm/SKILL.md",
        include_str!("../../../.claude/skills/storm/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/writing-docs/SKILL.md",
        include_str!("../../../.claude/skills/writing-docs/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/writing-docs/references/tells.md",
        include_str!("../../../.claude/skills/writing-docs/references/tells.md"),
        false,
    ),
];

/// One way an installed copy differs from this build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drift {
    /// A managed file is not there.
    Missing(PathBuf),
    /// A managed file's contents differ from this build's.
    Differs(PathBuf),
    /// `AGENTS.md` has no fabrial section, or its section differs.
    AgentsSection,
    /// `.agents/skills` is not a symlink to `../.claude/skills`.
    SkillsLink,
    /// The settings name another fabrial version, or none.
    Version { installed: Option<String>, running: String },
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Drift::Missing(path) => write!(f, "missing: {}", path.display()),
            Drift::Differs(path) => write!(f, "differs: {}", path.display()),
            Drift::AgentsSection => write!(f, "differs: AGENTS.md's fabrial section"),
            Drift::SkillsLink => write!(f, "differs: {SKILLS_LINK} is not a link to {SKILLS_TARGET}"),
            Drift::Version { installed, running } => match installed {
                Some(installed) => write!(f, "version: installed {installed}, this build {running}"),
                None => write!(f, "version: none recorded, this build {running}"),
            },
        }
    }
}

/// What [`init`] wrote.
#[derive(Debug, Default)]
pub struct Report {
    /// Files created or replaced, relative to the repository root.
    pub written: Vec<PathBuf>,
    /// Files already as this build would write them.
    pub unchanged: Vec<PathBuf>,
}

/// Reads a repository's settings, or `None` when it has none.
pub fn read_settings(root: &Path) -> Result<Option<Settings>, Error> {
    let path = root.join(SETTINGS_PATH);
    match fs::read_to_string(&path) {
        Ok(text) => Settings::parse(&text).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path, source }),
    }
}

/// Writes fabrial's files into the repository at `root`.
///
/// On a first install (no settings file yet), a managed file that already
/// exists with other contents is someone else's, so `init` refuses unless
/// `force` is set. Once the settings file exists, the managed files are
/// fabrial's and are replaced.
pub fn init(root: &Path, settings: &Settings, force: bool) -> Result<Report, Error> {
    settings.validate()?;
    let first_install = !root.join(SETTINGS_PATH).exists();
    let files = expected_files(settings, &version());

    if first_install && !force {
        let conflicts: Vec<PathBuf> = files
            .iter()
            .filter(|(path, contents, _)| {
                fs::read_to_string(root.join(path)).is_ok_and(|existing| existing != *contents)
            })
            .map(|(path, _, _)| path.clone())
            .collect();
        if !conflicts.is_empty() {
            return Err(Error::Conflict(conflicts));
        }
    }

    let mut report = Report::default();
    for (path, contents, executable) in &files {
        let full = root.join(path);
        if fs::read_to_string(&full).is_ok_and(|existing| existing == *contents) {
            report.unchanged.push(path.clone());
            continue;
        }
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io { path: parent.into(), source })?;
        }
        fs::write(&full, contents).map_err(|source| Error::Io { path: full.clone(), source })?;
        if *executable {
            make_executable(&full)?;
        }
        report.written.push(path.clone());
    }

    let agents = root.join("AGENTS.md");
    let current = read_optional(&agents)?;
    let updated = with_agents_section(current.as_deref().unwrap_or(""));
    if current.as_deref() == Some(updated.as_str()) {
        report.unchanged.push("AGENTS.md".into());
    } else {
        fs::write(&agents, updated).map_err(|source| Error::Io { path: agents.clone(), source })?;
        report.written.push("AGENTS.md".into());
    }

    if skills_link_ok(root) {
        report.unchanged.push(SKILLS_LINK.into());
    } else {
        write_skills_link(root)?;
        report.written.push(SKILLS_LINK.into());
    }
    Ok(report)
}

/// Lists where the repository at `root` differs from what this build's
/// [`init`] would write. An empty list means no drift.
pub fn check(root: &Path) -> Result<Vec<Drift>, Error> {
    let running = version();
    let Some(settings) = read_settings(root)? else {
        return Ok(vec![Drift::Missing(SETTINGS_PATH.into())]);
    };

    let mut drift = Vec::new();
    if settings.version.as_deref().map(release) != Some(release(&running)) {
        drift.push(Drift::Version { installed: settings.version.clone(), running: running.clone() });
    }
    for (path, contents, _) in expected_files(&settings, &running) {
        if path == Path::new(SETTINGS_PATH) {
            continue;
        }
        match read_optional(&root.join(&path))? {
            None => drift.push(Drift::Missing(path)),
            Some(existing) if existing != contents => drift.push(Drift::Differs(path)),
            Some(_) => {}
        }
    }
    let agents = read_optional(&root.join("AGENTS.md"))?.unwrap_or_default();
    if agents_section(&agents) != Some(AGENTS_SECTION) {
        drift.push(Drift::AgentsSection);
    }
    if !skills_link_ok(root) {
        drift.push(Drift::SkillsLink);
    }
    Ok(drift)
}

/// Every file `init` writes, with its contents and whether it is executable.
fn expected_files(settings: &Settings, version: &str) -> Vec<(PathBuf, String, bool)> {
    let mut files: Vec<(PathBuf, String, bool)> = EMBEDDED
        .iter()
        .map(|(path, contents, executable)| ((*path).into(), (*contents).into(), *executable))
        .collect();
    files.push((".github/CODEOWNERS".into(), codeowners(&settings.owner), false));
    files.push((SETTINGS_PATH.into(), settings.render(version), false));
    files
}

fn codeowners(owner: &str) -> String {
    format!(
        "# Every pull request requests a review from the owner. Pull requests are\n\
         # opened by stormlight-thunderbot, so the owner is never the author and can\n\
         # approve them. `fab init` writes this file from `.github/fabrial.toml`.\n\
         * @{owner}\n"
    )
}

/// The part of a version before any `+build` suffix, which is what `check`
/// compares: a new commit with the same files is not drift.
fn release(version: &str) -> &str {
    version.split('+').next().unwrap_or(version)
}

/// The text between the section markers, or `None` when there is no section.
fn agents_section(agents: &str) -> Option<&str> {
    let start = agents.find(AGENTS_BEGIN)? + AGENTS_BEGIN.len();
    let end = start + agents[start..].find(AGENTS_END)?;
    agents[start..end].strip_prefix('\n')
}

/// `agents` with fabrial's section replaced, or added at the end.
fn with_agents_section(agents: &str) -> String {
    let block = format!("{AGENTS_BEGIN}\n{AGENTS_SECTION}{AGENTS_END}\n");
    if let (Some(start), Some(end)) = (agents.find(AGENTS_BEGIN), agents.find(AGENTS_END))
        && start < end
    {
        let mut tail = &agents[end + AGENTS_END.len()..];
        tail = tail.strip_prefix('\n').unwrap_or(tail);
        return format!("{}{block}{tail}", &agents[..start]);
    }
    if agents.is_empty() {
        return format!("# AGENTS.md\n\n{block}");
    }
    let sep = if agents.ends_with("\n\n") {
        ""
    } else if agents.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{agents}{sep}{block}")
}

fn read_optional(path: &Path) -> Result<Option<String>, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path: path.into(), source }),
    }
}

fn skills_link_ok(root: &Path) -> bool {
    fs::read_link(root.join(SKILLS_LINK)).is_ok_and(|target| target == Path::new(SKILLS_TARGET))
}

#[cfg(unix)]
fn write_skills_link(root: &Path) -> Result<(), Error> {
    let link = root.join(SKILLS_LINK);
    if let Ok(meta) = fs::symlink_metadata(&link) {
        if !meta.file_type().is_symlink() {
            return Err(Error::Conflict(vec![SKILLS_LINK.into()]));
        }
        fs::remove_file(&link).map_err(|source| Error::Io { path: link.clone(), source })?;
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io { path: parent.into(), source })?;
    }
    std::os::unix::fs::symlink(SKILLS_TARGET, &link).map_err(|source| Error::Io { path: link, source })
}

#[cfg(not(unix))]
fn write_skills_link(_root: &Path) -> Result<(), Error> {
    Err(Error::Unsupported("creating the .agents/skills symlink"))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|source| Error::Io { path: path.into(), source })?
        .permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms).map_err(|source| Error::Io { path: path.into(), source })
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            owner: "desertthunder".into(),
            trailer: "Owais <desertthunder.dev@gmail.com>".into(),
            gates: "npm test".into(),
            version: None,
        }
    }

    #[test]
    fn init_then_check_reports_no_drift() {
        let dir = tempfile::tempdir().unwrap();
        let report = init(dir.path(), &settings(), false).unwrap();
        assert!(report.written.iter().any(|p| p == Path::new(".github/storm/gate.sh")));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        let toml = fs::read_to_string(dir.path().join(SETTINGS_PATH)).unwrap();
        assert!(toml.contains(r#"gates = "npm test""#));
        let owners = fs::read_to_string(dir.path().join(".github/CODEOWNERS")).unwrap();
        assert!(owners.ends_with("* @desertthunder\n"));
    }

    #[test]
    fn init_twice_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let again = init(dir.path(), &settings(), false).unwrap();
        assert_eq!(again.written, Vec::<PathBuf>::new());
    }

    #[test]
    fn check_reports_an_edited_and_a_deleted_file() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        fs::write(dir.path().join(".github/storm/gate.sh"), "#!/bin/sh\n").unwrap();
        fs::remove_file(dir.path().join(".claude/agents/verifier.md")).unwrap();
        let drift = check(dir.path()).unwrap();
        assert!(drift.contains(&Drift::Differs(".github/storm/gate.sh".into())));
        assert!(drift.contains(&Drift::Missing(".claude/agents/verifier.md".into())));
    }

    #[test]
    fn check_reports_an_older_installed_version() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let text = fs::read_to_string(&path).unwrap().replace(&version(), "0.0.1+old");
        fs::write(&path, text).unwrap();
        let drift = check(dir.path()).unwrap();
        assert!(matches!(&drift[..], [Drift::Version { installed: Some(v), .. }] if v == "0.0.1+old"));
    }

    #[test]
    fn check_ignores_a_new_commit_of_the_same_release() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace(&version(), &format!("{}+other", crate::VERSION));
        fs::write(&path, text).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn first_install_refuses_to_replace_other_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(".github/CODEOWNERS"), "* @someone\n").unwrap();
        let err = init(dir.path(), &settings(), false).unwrap_err();
        assert!(matches!(err, Error::Conflict(paths) if paths == vec![PathBuf::from(".github/CODEOWNERS")]));
        assert!(init(dir.path(), &settings(), true).is_ok());
    }

    #[test]
    fn agents_section_is_added_once_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "# Project\n\nOwn rules.\n").unwrap();
        init(dir.path(), &settings(), false).unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let agents = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# Project\n\nOwn rules.\n\n<!-- fabrial:begin -->\n"));
        assert_eq!(agents.matches(AGENTS_BEGIN).count(), 1);
        assert!(agents.ends_with("<!-- fabrial:end -->\n"));
    }

    #[test]
    fn agents_section_is_replaced_in_place() {
        let text = format!("# P\n\n{AGENTS_BEGIN}\nold\n{AGENTS_END}\n\n## After\n");
        let updated = with_agents_section(&text);
        assert!(updated.starts_with(&format!("# P\n\n{AGENTS_BEGIN}\n## Branches")));
        assert!(updated.ends_with(&format!("{AGENTS_END}\n\n## After\n")));
        assert_eq!(agents_section(&updated), Some(AGENTS_SECTION));
    }

    #[cfg(unix)]
    #[test]
    fn gate_script_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let mode = fs::metadata(dir.path().join(".github/storm/gate.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}
