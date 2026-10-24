//! Installs fabrial into a repository by copying files, and reports where an
//! installed copy has drifted from this build.
//!
//! Most files are built into the binary from this repository. `CODEOWNERS`,
//! the settings file and a section of `AGENTS.md` are written from the
//! repository's [`Settings`].

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::settings::{SETTINGS_PATH, Settings};
use crate::{Error, version};

/// The line that opens fabrial's section of `AGENTS.md`.
pub const AGENTS_BEGIN: &str = "<!-- fabrial:begin -->";
/// The line that closes fabrial's section of `AGENTS.md`.
pub const AGENTS_END: &str = "<!-- fabrial:end -->";
/// The symlink Codex and Pi read skills through.
pub const SKILLS_LINK: &str = ".agents/skills";

const AGENTS_PATH: &str = "AGENTS.md";
const CODEOWNERS_PATH: &str = ".github/CODEOWNERS";
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
    /// A managed file's contents differ from this build's, or the path is a
    /// symlink or an entry of another kind, which `init` refuses.
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

/// What sits at `.agents/skills`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkState {
    /// Nothing.
    Absent,
    /// A symlink to `../.claude/skills`.
    Ok,
    /// A real directory, which `init` never deletes.
    Directory,
    /// A file, or a symlink to somewhere else.
    Other,
}

/// The kind of entry `init` writes at a managed path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
}

/// What [`init`] wrote.
#[derive(Debug, Default)]
pub struct Report {
    /// Files created or replaced, relative to the repository root.
    pub written: Vec<PathBuf>,
    /// Files already as this build would write them.
    pub unchanged: Vec<PathBuf>,
}

/// Refuses a `root` with no `.git` entry. A worktree's `.git` is a file, so
/// any kind of entry counts.
pub fn require_repo_root(root: &Path) -> Result<(), Error> {
    let git = root.join(".git");
    match fs::symlink_metadata(&git) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(Error::NotRepoRoot(root.into())),
        Err(source) => Err(Error::Io { path: git, source }),
    }
}

/// Reads a repository's settings, or `None` when it has none. A settings
/// path that is a symlink, or sits under one, is refused rather than read.
pub fn read_settings(root: &Path) -> Result<Option<Settings>, Error> {
    let bad = unmanageable(root, [(PathBuf::from(SETTINGS_PATH), Kind::File)])?;
    if !bad.is_empty() {
        return Err(Error::Unmanageable(bad));
    }
    let path = root.join(SETTINGS_PATH);
    match fs::read_to_string(&path) {
        Ok(text) => Settings::parse(&text).map(Some),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path, source }),
    }
}

/// Installs fabrial in a repository that does not have it yet: `fab init`.
/// A repository with a settings file is refused; [`update`] refreshes it.
/// `force` replaces managed files someone else wrote, as in [`init`].
pub fn install(root: &Path, settings: &Settings, force: bool) -> Result<Report, Error> {
    if read_settings(root)?.is_some() {
        return Err(Error::Installed);
    }
    init(root, settings, force)
}

/// Replaces an install's files with this build's: `fab update`. `change`
/// edits the installed settings first, for a new owner, trailer or gate
/// command. An install that records a newer release than this build is
/// refused unless `downgrade` is set; one with no recorded version is not.
pub fn update(root: &Path, change: impl FnOnce(&mut Settings), downgrade: bool) -> Result<Report, Error> {
    let mut settings = read_settings(root)?.ok_or(Error::NotInstalled)?;
    if let Some(installed) = settings.version.take()
        && !downgrade
        && crate::release(&installed) > crate::release(crate::VERSION)
    {
        return Err(Error::Downgrade { installed, this: crate::version() });
    }
    change(&mut settings);
    init(root, &settings, false)
}

/// Writes fabrial's files into the repository at `root`.
///
/// On a first install (no settings file yet), a managed file that already
/// exists with other contents is someone else's, so `init` refuses unless
/// `force` is set. Once the settings file exists, the managed files are
/// fabrial's and are replaced.
///
/// A symlink at a managed path or at a directory above one, or a directory
/// where a managed file goes, is refused on every run, with or without
/// `force`, and is never followed or deleted.
pub fn init(root: &Path, settings: &Settings, force: bool) -> Result<Report, Error> {
    settings.validate()?;
    // Everything that can refuse is decided before the first write.
    let bad = unmanageable(root, managed_paths())?;
    if !bad.is_empty() {
        return Err(Error::Unmanageable(bad));
    }
    let first_install = !root.join(SETTINGS_PATH).exists();
    let files = expected_files(settings, &version());

    let agents = root.join(AGENTS_PATH);
    let current = read_optional(&agents)?;
    let updated = with_agents_section(&normalize(current.as_deref().unwrap_or("")))?;
    let link = skills_link_state(root);
    if link == LinkState::Directory {
        return Err(Error::SkillsDir);
    }

    if first_install && !force {
        let mut conflicts: Vec<PathBuf> = files
            .iter()
            .filter(|(path, contents, _)| {
                fs::read(root.join(path)).is_ok_and(|existing| existing != contents.as_bytes())
            })
            .map(|(path, _, _)| path.clone())
            .collect();
        if link == LinkState::Other {
            conflicts.push(SKILLS_LINK.into());
        }
        if !conflicts.is_empty() {
            return Err(Error::Conflict(conflicts));
        }
    }

    let mut report = Report::default();
    for (path, contents, executable) in &files {
        let full = root.join(path);
        if fs::read(&full).is_ok_and(|existing| existing == contents.as_bytes()) {
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

    if current.as_deref() == Some(updated.as_str()) {
        report.unchanged.push(AGENTS_PATH.into());
    } else {
        fs::write(&agents, updated).map_err(|source| Error::Io { path: agents.clone(), source })?;
        report.written.push(AGENTS_PATH.into());
    }

    if link == LinkState::Ok {
        report.unchanged.push(SKILLS_LINK.into());
    } else {
        write_skills_link(root)?;
        report.written.push(SKILLS_LINK.into());
    }
    Ok(report)
}

/// Lists where the repository at `root` differs from what this build's
/// [`init`] would write. An empty list means no drift.
///
/// A path [`init`] would refuse is reported as [`Drift::Differs`], and
/// nothing is read through it.
pub fn check(root: &Path) -> Result<Vec<Drift>, Error> {
    let running = version();
    let bad = unmanageable(root, managed_paths())?;
    let blocked = |path: &Path| bad.iter().any(|b| path.starts_with(b));
    let mut drift: Vec<Drift> = bad.iter().cloned().map(Drift::Differs).collect();
    if blocked(Path::new(SETTINGS_PATH)) {
        return Ok(drift);
    }
    let Some(settings) = read_settings(root)? else {
        drift.push(Drift::Missing(SETTINGS_PATH.into()));
        return Ok(drift);
    };

    if settings.version.as_deref().map(release) != Some(release(&running)) {
        drift.push(Drift::Version { installed: settings.version.clone(), running: running.clone() });
    }
    // The settings file is compared with the render of its own values, so a
    // line the workflows' `sed` readers cannot read shows as drift.
    let recorded = settings.version.as_deref().unwrap_or(&running);
    for (path, contents, _) in expected_files(&settings, recorded) {
        if blocked(&path) {
            continue;
        }
        match read_bytes(&root.join(&path))? {
            None => drift.push(Drift::Missing(path)),
            Some(existing) if existing != contents.as_bytes() => drift.push(Drift::Differs(path)),
            Some(_) => {}
        }
    }
    if !blocked(Path::new(AGENTS_PATH)) {
        // An AGENTS.md that is not UTF-8 has no section this build wrote.
        let agents = match fs::read(root.join(AGENTS_PATH)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|text| normalize(&text))
                .unwrap_or_default(),
            Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
            Err(source) => return Err(Error::Io { path: root.join(AGENTS_PATH), source }),
        };
        if !matches!(agents_section(&agents), Ok(Some(section)) if section == AGENTS_SECTION) {
            drift.push(Drift::AgentsSection);
        }
    }
    if !blocked(Path::new(SKILLS_LINK)) && skills_link_state(root) != LinkState::Ok {
        drift.push(Drift::SkillsLink);
    }
    Ok(drift)
}

/// Every path [`init`] writes through, with the kind of entry it writes
/// there. `.agents/skills` itself is left to [`skills_link_state`], so only
/// the directory above it is listed.
fn managed_paths() -> Vec<(PathBuf, Kind)> {
    let mut paths: Vec<(PathBuf, Kind)> = EMBEDDED
        .iter()
        .map(|(path, _, _)| ((*path).into(), Kind::File))
        .collect();
    for path in [CODEOWNERS_PATH, SETTINGS_PATH, AGENTS_PATH] {
        paths.push((path.into(), Kind::File));
    }
    if let Some(parent) = Path::new(SKILLS_LINK).parent() {
        paths.push((parent.into(), Kind::Dir));
    }
    paths
}

/// The entries among `paths`, and the directories above them, that `init`
/// must not write through: a symlink, or an entry of the wrong kind. Each
/// path is walked from the root down and stops at its first missing or
/// refused entry, so nothing is looked up through a symlink.
fn unmanageable(root: &Path, paths: impl IntoIterator<Item = (PathBuf, Kind)>) -> Result<Vec<PathBuf>, Error> {
    let mut found = BTreeSet::new();
    for (path, kind) in paths {
        let parts: Vec<_> = path.components().collect();
        let mut rel = PathBuf::new();
        for (i, part) in parts.iter().enumerate() {
            rel.push(part);
            let want = if i + 1 == parts.len() { kind } else { Kind::Dir };
            let full = root.join(&rel);
            let meta = match fs::symlink_metadata(&full) {
                Ok(meta) => meta,
                Err(e) if e.kind() == ErrorKind::NotFound => break,
                Err(source) => return Err(Error::Io { path: full, source }),
            };
            let fits = match want {
                Kind::File => meta.file_type().is_file(),
                Kind::Dir => meta.file_type().is_dir(),
            };
            if !fits {
                found.insert(rel);
                break;
            }
        }
    }
    Ok(found.into_iter().collect())
}

/// Every file `init` writes, with its contents and whether it is executable.
fn expected_files(settings: &Settings, version: &str) -> Vec<(PathBuf, String, bool)> {
    let mut files: Vec<(PathBuf, String, bool)> = EMBEDDED
        .iter()
        .map(|(path, contents, executable)| ((*path).into(), (*contents).into(), *executable))
        .collect();
    files.push((CODEOWNERS_PATH.into(), codeowners(&settings.owner), false));
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

/// `text` with CRLF line endings turned into LF, which is what `init` writes.
fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Where fabrial's section sits: the byte offsets of the first begin marker
/// and of the first end marker after it, or `None` when there is no begin
/// marker. A begin marker with no end after it is an error, since the end of
/// the section is unknown.
fn section_bounds(agents: &str) -> Result<Option<(usize, usize)>, Error> {
    let Some(start) = agents.find(AGENTS_BEGIN) else {
        return Ok(None);
    };
    let after = start + AGENTS_BEGIN.len();
    match agents[after..].find(AGENTS_END) {
        Some(end) => Ok(Some((start, after + end))),
        None => Err(Error::Agents(format!(
            "{AGENTS_BEGIN} has no {AGENTS_END} after it; add the end marker or remove the begin marker"
        ))),
    }
}

/// The text between the section markers, or `None` when there is no section.
fn agents_section(agents: &str) -> Result<Option<&str>, Error> {
    Ok(section_bounds(agents)?.and_then(|(start, end)| agents[start + AGENTS_BEGIN.len()..end].strip_prefix('\n')))
}

/// `agents` with fabrial's section replaced, or added at the end.
fn with_agents_section(agents: &str) -> Result<String, Error> {
    let block = format!("{AGENTS_BEGIN}\n{AGENTS_SECTION}{AGENTS_END}\n");
    if let Some((start, end)) = section_bounds(agents)? {
        let mut tail = &agents[end + AGENTS_END.len()..];
        tail = tail.strip_prefix('\n').unwrap_or(tail);
        return Ok(format!("{}{block}{tail}", &agents[..start]));
    }
    if agents.is_empty() {
        return Ok(format!("# AGENTS.md\n\n{block}"));
    }
    let sep = if agents.ends_with("\n\n") {
        ""
    } else if agents.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    Ok(format!("{agents}{sep}{block}"))
}

fn read_optional(path: &Path) -> Result<Option<String>, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path: path.into(), source }),
    }
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path: path.into(), source }),
    }
}

fn skills_link_state(root: &Path) -> LinkState {
    let link = root.join(SKILLS_LINK);
    match fs::symlink_metadata(&link) {
        Err(_) => LinkState::Absent,
        Ok(meta) if meta.file_type().is_symlink() => {
            if fs::read_link(&link).is_ok_and(|target| target == Path::new(SKILLS_TARGET)) {
                LinkState::Ok
            } else {
                LinkState::Other
            }
        }
        Ok(meta) if meta.is_dir() => LinkState::Directory,
        Ok(_) => LinkState::Other,
    }
}

/// Replaces whatever is at `.agents/skills` with the link. [`init`] has
/// already refused a directory, so only a file or a link is removed here.
#[cfg(unix)]
fn write_skills_link(root: &Path) -> Result<(), Error> {
    let link = root.join(SKILLS_LINK);
    if let Ok(meta) = fs::symlink_metadata(&link) {
        if meta.is_dir() {
            return Err(Error::SkillsDir);
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
    fn install_refuses_an_installed_repository() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        assert!(matches!(install(dir.path(), &settings(), true), Err(Error::Installed)));
    }

    #[test]
    fn update_needs_an_install_and_keeps_its_settings() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(update(dir.path(), |_| {}, false), Err(Error::NotInstalled)));
        install(dir.path(), &settings(), false).unwrap();
        fs::write(dir.path().join(".github/storm/gate.sh"), "#!/bin/sh\n").unwrap();
        let report = update(dir.path(), |s| s.gates = "just check".into(), false).unwrap();
        assert_eq!(report.written.len(), 2, "{:?}", report.written);
        let toml = fs::read_to_string(dir.path().join(SETTINGS_PATH)).unwrap();
        assert!(toml.contains(r#"gates = "just check""#));
        assert!(toml.contains(r#"owner = "desertthunder""#));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn update_refuses_to_downgrade_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let newer = fs::read_to_string(&path).unwrap().replace(
            &format!(r#"version = "{}""#, crate::version()),
            r#"version = "99.0.0+abc1234""#,
        );
        fs::write(&path, newer).unwrap();
        let err = update(dir.path(), |_| {}, false).unwrap_err();
        assert!(matches!(&err, Error::Downgrade { installed, .. } if installed == "99.0.0+abc1234"));
        update(dir.path(), |_| {}, true).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
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
        let updated = with_agents_section(&text).unwrap();
        assert!(updated.starts_with(&format!("# P\n\n{AGENTS_BEGIN}\n## Branches")));
        assert!(updated.ends_with(&format!("{AGENTS_END}\n\n## After\n")));
        assert_eq!(agents_section(&updated).unwrap(), Some(AGENTS_SECTION));
    }

    #[test]
    fn a_begin_marker_without_an_end_fails_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("# P\n\n{AGENTS_BEGIN}\nmine\n");
        fs::write(dir.path().join("AGENTS.md"), &text).unwrap();
        let err = init(dir.path(), &settings(), false).unwrap_err();
        assert!(matches!(err, Error::Agents(_)));
        assert!(err.to_string().starts_with("AGENTS.md: "));
        assert_eq!(fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(), text);
        assert!(!dir.path().join(SETTINGS_PATH).exists());
    }

    #[test]
    fn an_end_marker_before_the_begin_marker_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("# P\n\n```\n{AGENTS_END}\n```\n\n{AGENTS_BEGIN}\nold\n{AGENTS_END}\n");
        fs::write(dir.path().join("AGENTS.md"), &text).unwrap();
        init(dir.path(), &settings(), false).unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let agents = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
        assert_eq!(agents.matches(AGENTS_BEGIN).count(), 1);
        assert!(agents.starts_with(&format!("# P\n\n```\n{AGENTS_END}\n```\n\n{AGENTS_BEGIN}\n## Branches")));
        assert_eq!(agents_section(&agents).unwrap(), Some(AGENTS_SECTION));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn a_crlf_agents_file_is_rewritten_with_lf() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join("AGENTS.md");
        let crlf = fs::read_to_string(&path).unwrap().replace('\n', "\r\n");
        fs::write(&path, &crlf).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        init(dir.path(), &settings(), false).unwrap();
        let agents = fs::read_to_string(&path).unwrap();
        assert!(!agents.contains('\r'));
        assert_eq!(agents, crlf.replace("\r\n", "\n"));
    }

    #[test]
    fn check_reports_a_settings_line_the_workflows_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace(r#"trailer = "Owais <desertthunder.dev@gmail.com>""#, r#"trailer="x""#);
        fs::write(&path, text).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![Drift::Differs(SETTINGS_PATH.into())]);
    }

    #[cfg(unix)]
    #[test]
    fn skills_link_conflicts_are_found_before_writing() {
        // A directory is refused on every run, before anything is written.
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(SKILLS_LINK)).unwrap();
        for force in [false, true] {
            let err = init(dir.path(), &settings(), force).unwrap_err();
            assert!(matches!(err, Error::SkillsDir));
            assert!(!err.to_string().contains("--force"));
            assert!(!dir.path().join(SETTINGS_PATH).exists());
        }
        assert!(dir.path().join(SKILLS_LINK).is_dir());

        // A wrong link is a conflict on a first install, and replaced with --force.
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".agents")).unwrap();
        std::os::unix::fs::symlink("elsewhere", dir.path().join(SKILLS_LINK)).unwrap();
        let err = init(dir.path(), &settings(), false).unwrap_err();
        assert!(matches!(err, Error::Conflict(paths) if paths == vec![PathBuf::from(SKILLS_LINK)]));
        assert!(!dir.path().join(SETTINGS_PATH).exists());
        init(dir.path(), &settings(), true).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);

        // On a later run a wrong link is replaced, and a directory is refused.
        let link = dir.path().join(SKILLS_LINK);
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("elsewhere", &link).unwrap();
        init(dir.path(), &settings(), false).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        fs::remove_file(&link).unwrap();
        fs::create_dir(&link).unwrap();
        assert!(matches!(
            init(dir.path(), &settings(), false).unwrap_err(),
            Error::SkillsDir
        ));
        assert!(link.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_at_a_managed_path_is_refused_and_not_followed() {
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("target");
        fs::write(&target, "theirs\n").unwrap();
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".github/storm")).unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join(".github/storm/gate.sh")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude")).unwrap();
        for force in [false, true] {
            let err = init(dir.path(), &settings(), force).unwrap_err();
            assert!(
                matches!(&err, Error::Unmanageable(paths)
                    if *paths == vec![PathBuf::from(".claude"), PathBuf::from(".github/storm/gate.sh")]),
                "{err}"
            );
            assert!(!dir.path().join(SETTINGS_PATH).exists());
        }
        assert_eq!(fs::read_to_string(&target).unwrap(), "theirs\n");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o111, 0);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);

        // Once installed, a link that appears later is refused too, and
        // `check` reports it without reading through it.
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let gate = dir.path().join(".github/storm/gate.sh");
        fs::remove_file(&gate).unwrap();
        std::os::unix::fs::symlink(&target, &gate).unwrap();
        assert!(matches!(
            init(dir.path(), &settings(), true),
            Err(Error::Unmanageable(_))
        ));
        assert_eq!(fs::read_to_string(&target).unwrap(), "theirs\n");
        assert_eq!(
            check(dir.path()).unwrap(),
            vec![Drift::Differs(".github/storm/gate.sh".into())]
        );
    }

    #[test]
    fn a_directory_at_a_managed_file_is_refused_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".github/CODEOWNERS")).unwrap();
        for force in [false, true] {
            let err = init(dir.path(), &settings(), force).unwrap_err();
            assert!(matches!(&err, Error::Unmanageable(paths) if *paths == vec![PathBuf::from(".github/CODEOWNERS")]));
        }
        let mut written: Vec<_> = walk(dir.path());
        written.sort();
        assert_eq!(
            written,
            vec![PathBuf::from(".github"), PathBuf::from(".github/CODEOWNERS")]
        );
    }

    #[test]
    fn a_non_utf8_managed_file_is_a_conflict_and_drift() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(".github/CODEOWNERS"), b"* @\xff\n").unwrap();
        let err = init(dir.path(), &settings(), false).unwrap_err();
        assert!(matches!(err, Error::Conflict(paths) if paths == vec![PathBuf::from(".github/CODEOWNERS")]));

        init(dir.path(), &settings(), true).unwrap();
        fs::write(dir.path().join(".github/CODEOWNERS"), b"* @\xff\n").unwrap();
        assert_eq!(
            check(dir.path()).unwrap(),
            vec![Drift::Differs(".github/CODEOWNERS".into())]
        );
    }

    #[test]
    fn a_directory_without_git_is_not_a_repository_root() {
        let dir = tempfile::tempdir().unwrap();
        let err = require_repo_root(dir.path()).unwrap_err();
        assert!(matches!(err, Error::NotRepoRoot(_)));
        assert!(err.to_string().contains("must be the repository root"));
        // A worktree's .git is a file.
        fs::write(dir.path().join(".git"), "gitdir: elsewhere\n").unwrap();
        require_repo_root(dir.path()).unwrap();
        fs::remove_file(dir.path().join(".git")).unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        require_repo_root(dir.path()).unwrap();
    }

    /// Every entry under `root`, relative to it.
    fn walk(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                out.push(path.strip_prefix(root).unwrap().to_path_buf());
                if fs::symlink_metadata(&path).unwrap().is_dir() {
                    stack.push(path);
                }
            }
        }
        out
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
