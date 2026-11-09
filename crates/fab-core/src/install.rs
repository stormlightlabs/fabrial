//! Installs fabrial into a repository by copying files, and reports where an
//! installed copy has drifted from this build.
//!
//! Most files are built into the binary from this repository. `CODEOWNERS`,
//! the settings file and a section of `AGENTS.md` are written from the
//! repository's [`Settings`]. The gates file is the repository's own: `init`
//! writes a starter when there is none, and nothing else touches it.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::gates::{GATES_PATH, Gates};
use crate::settings::{LEGACY_SECRET_PREFIX, SETTINGS_PATH, Settings};

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
/// The secret prefix fabrial's own workflows, which [`EMBEDDED`] copies,
/// read. [`expected_files`] replaces it with the repository's.
const SOURCE_SECRET_PREFIX: &str = "THUNDERBOT";
/// The secrets the workflows read, without their prefix.
const SECRET_SUFFIXES: &[&str] = &["_APP_ID", "_PRIVATE_KEY"];
/// The step that marks a job as one that mints the App's token, which
/// [`with_environment`] puts in the repository's environment.
const MINT_STEP: &str = "uses: actions/create-github-app-token@";

/// The files copied as they are, with whether each is executable.
const EMBEDDED: &[(&str, &str, bool)] = &[
    (
        ".github/fabrial-roles.toml",
        include_str!("../../../.github/fabrial-roles.toml"),
        false,
    ),
    (
        ".github/fabrial-sequence.toml",
        include_str!("../../../.github/fabrial-sequence.toml"),
        false,
    ),
    (
        ".github/workflows/branch-pr.yml",
        include_str!("../../../.github/workflows/branch-pr.yml"),
        false,
    ),
    (
        ".github/workflows/branch-pr-open.yml",
        include_str!("../../../.github/workflows/branch-pr-open.yml"),
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
        ".github/actions/fab/action.yml",
        include_str!("../../../.github/actions/fab/action.yml"),
        false,
    ),
    (
        ".github/storm/shot.sh",
        include_str!("../../../.github/storm/shot.sh"),
        true,
    ),
    (
        ".github/storm/sgr.pl",
        include_str!("../../../.github/storm/sgr.pl"),
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
        ".claude/skills/fab-adv-rev/SKILL.md",
        include_str!("../../../.claude/skills/fab-adv-rev/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-decompose/SKILL.md",
        include_str!("../../../.claude/skills/fab-decompose/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-handoff/SKILL.md",
        include_str!("../../../.claude/skills/fab-handoff/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-implement/SKILL.md",
        include_str!("../../../.claude/skills/fab-implement/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-resync/SKILL.md",
        include_str!("../../../.claude/skills/fab-resync/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-review/SKILL.md",
        include_str!("../../../.claude/skills/fab-review/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-review/references/reading-the-diff.md",
        include_str!("../../../.claude/skills/fab-review/references/reading-the-diff.md"),
        false,
    ),
    (
        ".claude/skills/fab-revise/SKILL.md",
        include_str!("../../../.claude/skills/fab-revise/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-rubber-duck/SKILL.md",
        include_str!("../../../.claude/skills/fab-rubber-duck/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-storm/SKILL.md",
        include_str!("../../../.claude/skills/fab-storm/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-writing-docs/SKILL.md",
        include_str!("../../../.claude/skills/fab-writing-docs/SKILL.md"),
        false,
    ),
    (
        ".claude/skills/fab-writing-docs/references/tells.md",
        include_str!("../../../.claude/skills/fab-writing-docs/references/tells.md"),
        false,
    ),
];

/// Files an earlier release installed and this one does not, each with the
/// SHA-256 of every version a release wrote there. [`update`] deletes one
/// that matches a version and keeps one edited since. The skills were renamed
/// with a `fab-` prefix, so their old paths are here too, with the version
/// `main` held before the rename for an install made from an unreleased
/// build. An old skill path is a common name such as `review`, so a skill
/// there whose `SKILL.md` matches no version is treated as the repository's
/// own, as [`old_skill_state`] says.
const RETIRED: &[(&str, &[&str])] = &[
    (
        // v0.2.0 to v0.4.0.
        ".github/storm/gate.sh",
        &[
            "a8944659524249837e4aa9575052f8b8e166eab9264127aa51c8ab0fc2c0ba3f",
            "d7a33b56985918ef94c989a24ca3b6c9adbfe2136b9d0755c9a16fdf8c5165fb",
            "1f94766941b2386f1f47061553e3943310638ea49bf4e30794224da0f39c0d5c",
            "f3335b91195d494aad0b47b9836e3d61f147d05608eab056594a1d7d7439d12d",
        ],
    ),
    (
        // v0.2.0 to v0.5.0.
        ".github/storm/squash-body.sh",
        &[
            "ee2768dd437672877b423386500b4f099ff56cad73305e89c7ec1031a0946a91",
            "dd4e5f5185cae70d21e15ef3a5b45a86da1350e7f2bab3059112a3b18b687a31",
        ],
    ),
    (
        // Renamed to fab-adv-rev.
        ".claude/skills/adv-rev/SKILL.md",
        &[
            "4264de2770c44c5a66dc8ec985381a996921fc782a5256179b1d75b19671bc20",
            "fdd398ea1e0cd1268a66b0706ebf9873dcd88c7d36b02cbf94114bb4b84ba21c",
        ],
    ),
    (
        // Renamed to fab-decompose.
        ".claude/skills/decompose/SKILL.md",
        &["5a48bd8ea0592de5fe70dc90f6646cbd9e796fc5db33ccada2324c26a0491d89"],
    ),
    (
        // Renamed to fab-handoff.
        ".claude/skills/handoff/SKILL.md",
        &[
            "26cd756586075384d83d0b96c2a7716ee90333d2bc6f77603b8d44fe74143acd",
            "22be7c6d2a89d388439d31e0008c4c8a1b3123850ac2c9d64dda32fe4c943e91",
            "e889e54ebf65ae090316eafe1169d9d5da15b7edf2f1b90d4da47ec0edcc8d52",
            "3ff6dfe6fddac338e76446b7ee32746c33113a3a4076ef4fcee7b1dccf624947",
        ],
    ),
    (
        // Renamed to fab-implement.
        ".claude/skills/implement/SKILL.md",
        &[
            "872df25dc29240bd00ca948118c22d4837e9cbf62b23ede3f4f109429b0ebe5c",
            "a49c0d325dc8539f071d6ce0e64969903c6005491ea6672cdcce96d0f26ce899",
            "86ac5c758f318985792fab6c4b5e2b36d5dcc8611527d5612ae78e0df6487d6a",
            "1ff274dab6c68d46cfe56cfe072edde80121f538de77e3d44ab49f9d0caaf1f1",
            "f4f65b06b68830ea90c91f19800cba58c75c871a5eddf8fcb98adc057e4617d0",
            "9570437444c0f1666954c8b2c84ea2a77bf5037fd3fb3adfb2fa50a86d072500",
        ],
    ),
    (
        // Renamed to fab-resync.
        ".claude/skills/resync/SKILL.md",
        &[
            "854e775b86c50fc01534faebb7c7200d209c0ee6cc6517a2373f452c5ef305cb",
            "a93c346cf32d965a691a1fdc20bc504a8a9db9cd9feada710b5fd9153ea20737",
            "f3e0d499755591ed6ff9f3fcb4d0f7467072f38800291ea94f4d8cbff9df84eb",
        ],
    ),
    (
        // Renamed to fab-review.
        ".claude/skills/review/SKILL.md",
        &[
            "5b44fac25f3a4b9e473683adb099f0324ac3952b20ba60b81705d79a7d5bbf92",
            "c98aa3572bbd95965f834a07bd6164ab8dea697f27283f63eda23b54b932b5fd",
            "bfbd50e47b2959d3f42ca27acd244f57f4be1a8e4dc4698168c8f7b58a408f89",
            "79a42fd006a87fc0fb7b699e68fc6f87e562d6369b69e9d2af253c571660ab5e",
            "9b2cf4f45d2afa7aafa44e23010f284e8b3aea4e53908a0f410459f204014758",
            "4d989432157cf691535bc774100d4f2219dc4a750677973775150e91a3099aa4",
            "20a7cb18da86a33ec62ad8e9958d7283040a7a7c667a9c167b0c796c09da63f7",
        ],
    ),
    (
        // Renamed to fab-review.
        ".claude/skills/review/references/reading-the-diff.md",
        &["249930a2b0bd6ec5eaa3f78e3942cd010cda6b9eb53d4a1576f1cdde2801fb77"],
    ),
    (
        // Renamed to fab-revise.
        ".claude/skills/revise/SKILL.md",
        &[
            "afc4178bd6b7756e4a6ca9ded09e89835300979cb62fab84a54d1b0974f8cda6",
            "45e95d438b6a90c0ba4dc69765b52932932bacb16fa02a05fc15c23e8a5bbca6",
            "6693cd983a9f119d1042d793111c309c8333b480158252ef1ac900bd7dea8d4f",
            "35700736df7eda4b535a52a2aefaa94e85dd24d87531414b47b657ad672a40bd",
            "6932a2c48c98074979136de8635c8befb0a149a0b7c0e1a1e2eeee6784514f92",
        ],
    ),
    (
        // Renamed to fab-rubber-duck.
        ".claude/skills/rubber-duck/SKILL.md",
        &["8cc692eee9a552addcab0cb46c7f62973e6c853a805f9da199143784ccfd6867"],
    ),
    (
        // Renamed to fab-storm.
        ".claude/skills/storm/SKILL.md",
        &[
            "2d1bb2c4caacc1486166489b02adc172624f6bdd316e081553c6f0898ad23d3d",
            "b0ca94b4e2e4852238ec745a0e75108e8feaa558c8d46dc25fe2d05bf943c6d6",
            "ae495aea01bdbf7de7b7cebeae9a938b5b0c31d24de2acca40264d679a1244d7",
            "a20cf52b2ea6945bf35a7de38b6fd96be9ec27ebffe0f4695b2a4e70d07aa54d",
            "cee4b46f72699fc25c78117e3d92cf32667ed209b2fd555e70d4fab9773fdeb8",
            "e2dad404dba4fee485bf19667e180e8df540917d4d86dd733b733cf247f0ca45",
            "d57be777ee6d45781f73018a5c682a1574d26aa038b1835c580c6481e815ebdc",
            "05a61b0c68098315cb15baa5af8fa1ae573c5e1341817fd0036719f62436e294",
        ],
    ),
    (
        // Renamed to fab-writing-docs.
        ".claude/skills/writing-docs/SKILL.md",
        &[
            "ea5ac12dbc6d2f0a639c0cf9b7c237a3e87fc9655a261441edaa6b2ef3af1c9d",
            "92bfbf6703a7070dcb6282c1cf2b670fc26f759ec3ea1203f0d1ecb7a9aca206",
        ],
    ),
    (
        // Renamed to fab-writing-docs.
        ".claude/skills/writing-docs/references/tells.md",
        &["13f4cc2f21eebcbcdd1a593e3b1214d3a00e6f617213ae18d22f6671c082368b"],
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
    /// The settings name no `app`, which every workflow's `fab` command
    /// needs.
    NoApp,
    /// A file an earlier release installed and this one does not is still
    /// there. Under a skill's old name, each file when all of them are as a
    /// release wrote them, or the skill's directory when anything else is
    /// there beside a `SKILL.md` a release wrote.
    Retired(PathBuf),
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
            Drift::Retired(path) => write!(f, "retired: {}", path.display()),
            Drift::NoApp => write!(
                f,
                "missing: `app` in {SETTINGS_PATH}; run fab update --app <slug> with the repository's GitHub App"
            ),
        }
    }
}

/// Why [`update`] kept a retired file instead of deleting it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeptReason {
    /// A regular file that matches no version a release wrote.
    Edited,
    /// A symlink, which is never followed or deleted.
    Symlink,
    /// A directory or another kind of entry that is not a regular file.
    NotAFile,
    /// A skill under its old name, kept whole because its directory holds a
    /// file no release wrote there, or one edited since, beside files a
    /// release wrote.
    SkillChanged,
    /// Reading or deleting it failed, with the error.
    Failed(String),
}

/// What sits at a skill's old directory, as [`old_skill_state`] decides.
#[derive(Debug, PartialEq, Eq)]
enum OldSkill {
    /// Nothing, a symlink, or the repository's own skill: left alone and not
    /// reported.
    Leave,
    /// Only retired files of this skill, each as a release wrote it.
    Released(Vec<PathBuf>),
    /// Files a release wrote beside anything else, kept whole and reported.
    Changed,
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

/// What [`init`] or [`update`] wrote.
#[derive(Debug, Default)]
pub struct Report {
    /// Files created or replaced, relative to the repository root.
    pub written: Vec<PathBuf>,
    /// Files already as this build would write them.
    pub unchanged: Vec<PathBuf>,
    /// Retired files [`update`] deleted, each as a release wrote it.
    pub removed: Vec<PathBuf>,
    /// Retired files [`update`] kept, each with the reason.
    pub kept: Vec<Kept>,
}

/// A retired file [`update`] kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// The file, relative to the repository root.
    pub path: PathBuf,
    /// Why it is still there.
    pub reason: KeptReason,
}

/// The text this build installs at `path`, relative to the repository root,
/// or `None` when it installs no file there or writes it from settings.
pub fn embedded(path: &str) -> Option<&'static str> {
    EMBEDDED.iter().find(|(p, _, _)| *p == path).map(|(_, text, _)| *text)
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
/// `force` replaces managed files someone else wrote, as in [`init`]. A
/// starter gates file that runs `settings.gates` for every change is written
/// when nothing is at its path; an existing one is kept, even with `force`.
pub fn install(root: &Path, settings: &Settings, force: bool) -> Result<Report, Error> {
    if read_settings(root)?.is_some() {
        return Err(Error::Installed);
    }
    let mut report = init(root, settings, force)?;
    // `init` has refused a symlinked `.github`, so this looks at the path
    // itself only, and anything there, of any kind, is the repository's.
    let gates = root.join(GATES_PATH);
    match fs::symlink_metadata(&gates) {
        Ok(_) => report.unchanged.push(GATES_PATH.into()),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            fs::write(&gates, Gates::starter(&settings.gates))
                .map_err(|source| Error::Io { path: gates.clone(), source })?;
            report.written.push(GATES_PATH.into());
        }
        Err(source) => return Err(Error::Io { path: gates, source }),
    }
    Ok(report)
}

/// Replaces an install's files with this build's: `fab update`. `change`
/// edits the installed settings first, for a new owner, trailer or gate
/// command. An install that records a newer release than this build, or a
/// version that is not a release, is refused unless `downgrade` is set; one
/// with no recorded version is not. A file only an earlier release installed
/// is deleted when it is as a release wrote it, and kept otherwise.
pub fn update(root: &Path, change: impl FnOnce(&mut Settings), downgrade: bool) -> Result<Report, Error> {
    let mut settings = read_settings(root)?.ok_or(Error::NotInstalled)?;
    if let Some(installed) = settings.version.take()
        && !downgrade
    {
        match crate::release(&installed) {
            None => return Err(Error::UnknownVersion(installed)),
            Some(release) if Some(release) > crate::release(crate::VERSION) => {
                return Err(Error::Downgrade { installed, this: crate::VERSION.into() });
            }
            Some(_) => {}
        }
    }
    // An install from before the setting reads the secrets 0.9.0 and
    // earlier named; writing that prefix keeps them working.
    settings
        .secret_prefix
        .get_or_insert_with(|| LEGACY_SECRET_PREFIX.into());
    change(&mut settings);
    let mut report = init(root, &settings, false)?;
    remove_retired(root, &mut report);
    Ok(report)
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
    if settings.app.is_none() {
        return Err(Error::Settings(
            "`app` is not set; pass --app with the slug of the repository's GitHub App, as docs/bot-setup.md says"
                .into(),
        ));
    }
    // Everything that can refuse is decided before the first write.
    let bad = unmanageable(root, managed_paths())?;
    if !bad.is_empty() {
        return Err(Error::Unmanageable(bad));
    }
    let first_install = !root.join(SETTINGS_PATH).exists();
    // The release alone, so a new commit leaves the settings file as it is
    // and pull requests do not all change the same line.
    let files = expected_files(settings, crate::VERSION);

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
    let running = crate::VERSION.to_owned();
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

    // Only the release counts: a new commit with the same files is not drift.
    if settings.version.as_deref().map(crate::release) != Some(crate::release(&running)) {
        drift.push(Drift::Version { installed: settings.version.clone(), running: running.clone() });
    }
    // The workflows refuse to run without it, so a file that lost it is
    // drift even though it matches the render of its own values.
    if settings.app.is_none() {
        drift.push(Drift::NoApp);
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
    let retired_bad = unmanageable(root, retired_paths())?;
    for (path, _) in RETIRED {
        let path = Path::new(path);
        if is_old_skill(path) || blocked(path) || under_refused(path, &retired_bad) {
            continue;
        }
        if retired_bad.iter().any(|b| b == path) || fs::symlink_metadata(root.join(path)).is_ok() {
            drift.push(Drift::Retired(path.into()));
        }
    }
    for skill in old_skills() {
        if blocked(&skill) {
            continue;
        }
        match old_skill_state(root, &skill)? {
            OldSkill::Leave => {}
            OldSkill::Released(files) => drift.extend(files.into_iter().map(Drift::Retired)),
            OldSkill::Changed => drift.push(Drift::Retired(skill)),
        }
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

/// Every path in [`RETIRED`], as a file.
fn retired_paths() -> Vec<(PathBuf, Kind)> {
    RETIRED.iter().map(|(path, _)| ((*path).into(), Kind::File)).collect()
}

/// Whether a directory above `path` is among the `refused` entries, so
/// nothing at `path` can be looked at without following a symlink.
fn under_refused(path: &Path, refused: &[PathBuf]) -> bool {
    refused.iter().any(|b| b != path && path.starts_with(b))
}

/// Deletes each retired file that matches a version a release wrote, and
/// records the rest in `report.kept`. A symlink or another kind of entry at
/// a retired path is kept, and nothing is followed through one. A file that
/// cannot be read or deleted is kept with the error, so the files `init`
/// already wrote stay in the report. A skill under its old name is deleted
/// or kept whole, as [`old_skill_state`] says.
fn remove_retired(root: &Path, report: &mut Report) {
    for (path, hashes) in RETIRED {
        let rel = Path::new(path);
        if is_old_skill(rel) {
            continue;
        }
        let reason = match retire(root, rel, &root.join(rel), hashes, &mut report.removed) {
            Ok(None) => continue,
            Ok(Some(reason)) => reason,
            Err(e) => KeptReason::Failed(e.to_string()),
        };
        report.kept.push(Kept { path: rel.into(), reason });
    }
    for skill in old_skills() {
        let files = match old_skill_state(root, &skill) {
            Ok(OldSkill::Leave) => continue,
            Ok(OldSkill::Released(files)) => files,
            Ok(OldSkill::Changed) => {
                report.kept.push(Kept { path: skill, reason: KeptReason::SkillChanged });
                continue;
            }
            Err(e) => {
                report
                    .kept
                    .push(Kept { path: skill, reason: KeptReason::Failed(e.to_string()) });
                continue;
            }
        };
        for rel in files {
            if let Err(e) = delete_retired(root, &rel, &root.join(&rel), &mut report.removed) {
                report
                    .kept
                    .push(Kept { path: rel, reason: KeptReason::Failed(e.to_string()) });
            }
        }
    }
}

/// Deletes the retired file at `rel` if it matches one of `hashes`, and
/// records it in `removed`. Returns why it was kept, or `None` when it was
/// deleted or nothing is there.
fn retire(
    root: &Path, rel: &Path, full: &Path, hashes: &[&str], removed: &mut Vec<PathBuf>,
) -> Result<Option<KeptReason>, Error> {
    let bad = unmanageable(root, [(rel.to_path_buf(), Kind::File)])?;
    if under_refused(rel, &bad) {
        return Ok(None);
    }
    if bad.iter().any(|b| b == rel) {
        let meta = fs::symlink_metadata(full).map_err(|source| Error::Io { path: full.into(), source })?;
        return Ok(Some(if meta.file_type().is_symlink() {
            KeptReason::Symlink
        } else {
            KeptReason::NotAFile
        }));
    }
    let Some(bytes) = read_bytes(full)? else {
        return Ok(None);
    };
    if !hashes.contains(&sha256_hex(&bytes).as_str()) {
        return Ok(Some(KeptReason::Edited));
    }
    delete_retired(root, rel, full, removed)
}

/// Deletes the retired file at `rel`, records it in `removed`, and removes
/// the directories its removal leaves empty.
fn delete_retired(
    root: &Path, rel: &Path, full: &Path, removed: &mut Vec<PathBuf>,
) -> Result<Option<KeptReason>, Error> {
    fs::remove_file(full).map_err(|source| Error::Io { path: full.into(), source })?;
    removed.push(rel.into());
    remove_empty_parents(root, rel);
    Ok(None)
}

/// Whether the retired path `rel` is under a skill's old name.
fn is_old_skill(rel: &Path) -> bool {
    rel.starts_with(".claude/skills")
}

/// The hashes [`RETIRED`] lists for `rel`, or none.
fn retired_hashes(rel: &Path) -> &'static [&'static str] {
    RETIRED
        .iter()
        .find(|(p, _)| Path::new(p) == rel)
        .map_or(&[], |(_, hashes)| *hashes)
}

/// Whether the entry at `rel` is a regular file that matches one of
/// `hashes`: `None` when nothing is there, and `Some(false)` for a symlink,
/// another kind of entry, or anything under a symlinked directory, none of
/// which a release wrote.
fn written_by_release(root: &Path, rel: &Path, hashes: &[&str]) -> Result<Option<bool>, Error> {
    let bad = unmanageable(root, [(rel.to_path_buf(), Kind::File)])?;
    if !bad.is_empty() {
        return Ok(Some(false));
    }
    Ok(read_bytes(&root.join(rel))?.map(|bytes| hashes.contains(&sha256_hex(&bytes).as_str())))
}

/// Each skill directory [`RETIRED`] lists files under, such as
/// `.claude/skills/review`, in order.
fn old_skills() -> BTreeSet<PathBuf> {
    RETIRED
        .iter()
        .map(|(path, _)| Path::new(path))
        .filter(|path| is_old_skill(path))
        .map(|path| path.components().take(3).collect())
        .collect()
}

/// What sits at `skill`, a skill's old directory. It is the repository's
/// own, and left alone, when its `SKILL.md` is there and is not as a release
/// wrote it. That rule also covers a fabrial copy someone edited, which looks
/// the same, so `fab check` never flags a repository's own skill and `fab
/// update` never deletes one, at the cost of leaving an edited copy
/// unreported. Otherwise every entry under it is looked at, without following
/// a symlink: when each is a retired file of this skill as a release wrote
/// it, the skill is [`OldSkill::Released`], and when anything else is there,
/// [`OldSkill::Changed`], so its files are kept together rather than half
/// removed.
fn old_skill_state(root: &Path, skill: &Path) -> Result<OldSkill, Error> {
    if !unmanageable(root, [(skill.to_path_buf(), Kind::Dir)])?.is_empty() {
        return Ok(OldSkill::Leave);
    }
    let skill_md = skill.join("SKILL.md");
    if written_by_release(root, &skill_md, retired_hashes(&skill_md))? == Some(false) {
        return Ok(OldSkill::Leave);
    }
    let mut files = Vec::new();
    let mut dirs = vec![skill.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let full = root.join(&dir);
        let entries = match fs::read_dir(&full) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(source) => return Err(Error::Io { path: full, source }),
        };
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io { path: full.clone(), source })?;
            let rel = dir.join(entry.file_name());
            let kind = entry
                .file_type()
                .map_err(|source| Error::Io { path: root.join(&rel), source })?;
            if kind.is_dir() {
                dirs.push(rel);
                continue;
            }
            let hashes = retired_hashes(&rel);
            let released = kind.is_file()
                && read_bytes(&root.join(&rel))?.is_some_and(|bytes| hashes.contains(&sha256_hex(&bytes).as_str()));
            if !released {
                return Ok(OldSkill::Changed);
            }
            files.push(rel);
        }
    }
    files.sort();
    Ok(if files.is_empty() { OldSkill::Leave } else { OldSkill::Released(files) })
}

/// Removes each directory above `rel` that is left empty, from the deepest
/// up, and stops at the first that is not, such as a renamed skill's old
/// directory once its files are gone.
fn remove_empty_parents(root: &Path, rel: &Path) {
    for dir in rel.ancestors().skip(1) {
        if dir.as_os_str().is_empty() || fs::remove_dir(root.join(dir)).is_err() {
            break;
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Every file `init` writes, with its contents and whether it is executable.
/// The workflows read the secrets under the repository's prefix, in the
/// repository's environment when the settings name one.
fn expected_files(settings: &Settings, version: &str) -> Vec<(PathBuf, String, bool)> {
    let mut files: Vec<(PathBuf, String, bool)> = EMBEDDED
        .iter()
        .map(|(path, contents, executable)| {
            let contents = with_environment(&with_secrets(contents, settings), settings);
            ((*path).into(), contents, *executable)
        })
        .collect();
    files.push((CODEOWNERS_PATH.into(), codeowners(&settings.owner), false));
    files.push((SETTINGS_PATH.into(), settings.render(version), false));
    files
}

/// `contents` with each secret fabrial's own workflows read renamed to the
/// repository's prefix. Text without one comes back as it is.
///
/// Only a whole `${{ secrets.<name> }}` expression is renamed, in one scan,
/// so a prefix that itself ends in a suffix is never renamed twice.
fn with_secrets(contents: &str, settings: &Settings) -> String {
    let opener = format!("${{{{ secrets.{SOURCE_SECRET_PREFIX}");
    let mut out = String::with_capacity(contents.len());
    let mut rest = contents;
    while let Some(at) = rest.find(&opener) {
        out.push_str(&rest[..at]);
        let after = &rest[at + opener.len()..];
        match SECRET_SUFFIXES
            .iter()
            .find(|suffix| after.starts_with(&format!("{suffix} }}}}")))
        {
            Some(suffix) => {
                out.push_str(&format!("${{{{ secrets.{}{suffix} }}}}", settings.secret_prefix()));
                rest = &after[suffix.len() + " }}".len()..];
            }
            None => {
                out.push_str(&opener);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `contents` with each job that mints the App's token ([`MINT_STEP`])
/// naming the settings' environment, in a block put directly under the
/// job's `runs-on:` line. fabrial's own workflows name none, so with no
/// environment set the text comes back as it is. A job is the text from one
/// line indented two spaces to the next.
fn with_environment(contents: &str, settings: &Settings) -> String {
    let Some(name) = &settings.environment else {
        return contents.to_string();
    };
    let block = format!("    environment:\n      name: {name}\n      deployment: false\n");
    let mut jobs: Vec<Vec<&str>> = vec![Vec::new()];
    for line in contents.split_inclusive('\n') {
        let opens = line.starts_with("  ") && !line[2..].starts_with([' ', '\n', '#']);
        if opens {
            jobs.push(Vec::new());
        }
        jobs.last_mut().unwrap().push(line);
    }
    let mut out = String::with_capacity(contents.len() + block.len());
    for job in jobs {
        let mints = job.iter().any(|l| l.contains(MINT_STEP));
        let mut placed = !mints;
        for line in job {
            out.push_str(line);
            if !placed && line.starts_with("    runs-on:") {
                out.push_str(&block);
                placed = true;
            }
        }
    }
    out
}

fn codeowners(owner: &str) -> String {
    format!(
        "# Every pull request requests a review from the owner. Pull requests are\n\
         # opened by the repository's GitHub App, so the owner is never the author\n\
         # and can approve them. `fab init` writes this file from\n\
         # `.github/fabrial.toml`.\n\
         * @{owner}\n"
    )
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

    #[test]
    fn installed_files_name_no_task_tracker() {
        // fabrial tracks its own work in arcl, but what it installs reads the
        // tracker from the repository's AGENTS.md.
        let files = EMBEDDED.iter().map(|(p, c, _)| (*p, *c));
        for (path, contents) in files.chain([("agents-section.md", AGENTS_SECTION)]) {
            let lower = contents.to_lowercase();
            for name in ["arcl", "arc lightning"] {
                assert!(!lower.contains(name), "{path} names {name}");
            }
        }
    }

    #[test]
    fn installed_files_name_no_app() {
        // fabrial's own App is stormlight-thunderbot, but each repository
        // that installs fabrial names its own in `.github/fabrial.toml`, and
        // the default install reads its secrets under FABRIAL.
        let files = expected_files(&settings(), crate::VERSION);
        let section = (PathBuf::from("agents-section.md"), AGENTS_SECTION.to_owned(), false);
        for (path, contents, _) in files.into_iter().chain([section]) {
            if path == Path::new(SETTINGS_PATH) {
                continue;
            }
            assert!(
                !contents.to_lowercase().contains("thunderbot"),
                "{} names an App",
                path.display()
            );
        }
    }

    #[test]
    fn the_workflows_read_the_secrets_under_the_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let own = Settings { secret_prefix: Some("OWN_BOT".into()), ..settings() };
        install(dir.path(), &own, false).unwrap();
        for workflow in ["branch-pr-open.yml", "review.yml", "comment.yml"] {
            let text = fs::read_to_string(dir.path().join(".github/workflows").join(workflow)).unwrap();
            assert!(text.contains("${{ secrets.OWN_BOT_APP_ID }}"), "{workflow}");
            assert!(text.contains("${{ secrets.OWN_BOT_PRIVATE_KEY }}"), "{workflow}");
            assert!(!text.contains(SOURCE_SECRET_PREFIX), "{workflow}");
        }
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    /// The jobs that mint the App's token, as `(workflow, job)`, each of
    /// which must name the environment.
    const MINTING_JOBS: &[(&str, &str)] = &[
        ("branch-pr-open.yml", "pr"),
        ("review.yml", "gate"),
        ("comment.yml", "comment"),
    ];

    #[test]
    fn every_job_that_mints_the_token_names_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let own = Settings { environment: Some("own-env".into()), ..settings() };
        install(dir.path(), &own, false).unwrap();
        let workflows = dir.path().join(".github/workflows");
        for (workflow, job) in MINTING_JOBS {
            let text = fs::read_to_string(workflows.join(workflow)).unwrap();
            let start = text
                .find(&format!("\n  {job}:\n"))
                .unwrap_or_else(|| panic!("{workflow}: no {job}"));
            let steps = start + text[start..].find("\n    steps:\n").unwrap() + 1;
            assert!(
                text[start..steps].contains(
                    "\n    runs-on: ubuntu-latest\n    environment:\n      name: own-env\n      deployment: false\n"
                ),
                "{workflow}: {job} names no environment"
            );
            assert_eq!(text.matches("create-github-app-token").count(), 1, "{workflow}");
            assert_eq!(text.matches("environment:").count(), 1, "{workflow}");
        }
        // The half a push runs, from the pushed branch, reads no secrets.
        let push = fs::read_to_string(workflows.join("branch-pr.yml")).unwrap();
        assert!(!push.contains("secrets.") && !push.contains("create-github-app-token"));
        assert!(!push.contains("environment:"));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn without_an_environment_no_workflow_names_one() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &Settings { environment: None, ..settings() }, false).unwrap();
        for (workflow, _) in MINTING_JOBS {
            let text = fs::read_to_string(dir.path().join(".github/workflows").join(workflow)).unwrap();
            assert!(!text.contains("environment:"), "{workflow}");
        }
        assert!(
            !fs::read_to_string(dir.path().join(SETTINGS_PATH))
                .unwrap()
                .contains("environment =")
        );
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn only_a_job_that_mints_the_token_gets_the_environment() {
        let own = Settings { environment: Some("own".into()), ..settings() };
        let text = "jobs:\n  a:\n    runs-on: x\n    steps:\n      - run: true\n  \
                    b:\n    runs-on: x\n    steps:\n      - uses: actions/create-github-app-token@v2\n";
        assert_eq!(
            with_environment(text, &own),
            "jobs:\n  a:\n    runs-on: x\n    steps:\n      - run: true\n  \
             b:\n    runs-on: x\n    environment:\n      name: own\n      deployment: false\n    steps:\n      \
             - uses: actions/create-github-app-token@v2\n"
        );
        assert_eq!(with_environment(text, &settings()), text);
    }

    #[test]
    fn update_adds_no_environment_and_keeps_one_set() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        update(dir.path(), |_| {}, false).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("environment ="));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        update(dir.path(), |s| s.environment = Some("own".into()), false).unwrap();
        update(dir.path(), |_| {}, false).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("environment = \"own\"\n"));
        let review = fs::read_to_string(dir.path().join(".github/workflows/review.yml")).unwrap();
        assert!(review.contains("    environment:\n      name: own\n"), "{review}");
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn a_prefix_ending_in_a_suffix_is_renamed_once() {
        let own = Settings { secret_prefix: Some("THUNDERBOT_PRIVATE_KEY".into()), ..settings() };
        let text = "id: ${{ secrets.THUNDERBOT_APP_ID }}\nkey: ${{ secrets.THUNDERBOT_PRIVATE_KEY }}\nother: ${{ secrets.THUNDERBOT_OTHER }}\n";
        assert_eq!(
            with_secrets(text, &own),
            "id: ${{ secrets.THUNDERBOT_PRIVATE_KEY_APP_ID }}\n\
             key: ${{ secrets.THUNDERBOT_PRIVATE_KEY_PRIVATE_KEY }}\n\
             other: ${{ secrets.THUNDERBOT_OTHER }}\n"
        );
    }

    #[test]
    fn update_keeps_the_secrets_an_install_without_a_prefix_reads() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let old: String = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with("secret_prefix = "))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&path, old).unwrap();
        update(dir.path(), |_| {}, false).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("secret_prefix = \"THUNDERBOT\"\n")
        );
        let review = fs::read_to_string(dir.path().join(".github/workflows/review.yml")).unwrap();
        assert!(review.contains("secrets.THUNDERBOT_APP_ID") && review.contains("secrets.THUNDERBOT_PRIVATE_KEY"));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        // A prefix the install sets is kept.
        update(dir.path(), |s| s.secret_prefix = Some("OWN".into()), false).unwrap();
        update(dir.path(), |_| {}, false).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("secret_prefix = \"OWN\"\n"));
    }

    #[test]
    fn init_needs_an_app() {
        let dir = tempfile::tempdir().unwrap();
        let err = init(dir.path(), &Settings { app: None, ..settings() }, false).unwrap_err();
        assert!(err.to_string().contains("--app"), "{err}");
        assert!(!dir.path().join(SETTINGS_PATH).exists());
    }

    #[test]
    fn update_adds_an_app_to_an_install_without_one() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let old: String = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with("app = "))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&path, old).unwrap();
        // Check catches a pull request that drops the line. The comment
        // left above it is drift of its own.
        assert!(check(dir.path()).unwrap().contains(&Drift::NoApp));
        assert!(Drift::NoApp.to_string().contains("fab update --app"));
        let err = update(dir.path(), |_| {}, false).unwrap_err();
        assert!(err.to_string().contains("--app"), "{err}");
        update(dir.path(), |s| s.app = Some("own-bot".into()), false).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("app = \"own-bot\"\n"));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    fn settings() -> Settings {
        Settings {
            owner: "desertthunder".into(),
            app: Some("fabrial-bot".into()),
            secret_prefix: Some(crate::settings::DEFAULT_SECRET_PREFIX.into()),
            environment: None,
            trailer: "Owais <desertthunder.dev@gmail.com>".into(),
            signoff: None,
            assisted_by: None,
            disposable: None,
            planning: None,
            tasks: None,
            gates: "npm test".into(),
            version: None,
        }
    }

    #[test]
    fn init_then_check_reports_no_drift() {
        let dir = tempfile::tempdir().unwrap();
        let report = init(dir.path(), &settings(), false).unwrap();
        assert!(report.written.iter().any(|p| p == Path::new(".github/storm/shot.sh")));
        for skill in ["fab-rubber-duck", "fab-decompose"] {
            let path = PathBuf::from(format!(".claude/skills/{skill}/SKILL.md"));
            assert!(report.written.contains(&path), "{skill} is not installed");
        }
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        let toml = fs::read_to_string(dir.path().join(SETTINGS_PATH)).unwrap();
        assert!(toml.contains(r#"gates = "npm test""#));
        let owners = fs::read_to_string(dir.path().join(".github/CODEOWNERS")).unwrap();
        assert!(owners.ends_with("* @desertthunder\n"));
    }

    /// Every version of each [`RETIRED`] file a release wrote, from
    /// `git show <tag>:<path>` at the first release that wrote it, with its
    /// file name under `src/testdata/retired`. A skill version no release
    /// wrote is `main`'s at 69d748d, the commit before the `fab-` rename.
    const RETIRED_VERSIONS: &[(&str, &str, &str)] = &[
        (
            ".github/storm/gate.sh",
            "gate-0.2.0.sh",
            include_str!("testdata/retired/gate-0.2.0.sh"),
        ),
        (
            ".github/storm/gate.sh",
            "gate-0.3.0.sh",
            include_str!("testdata/retired/gate-0.3.0.sh"),
        ),
        (
            ".github/storm/gate.sh",
            "gate-0.3.1.sh",
            include_str!("testdata/retired/gate-0.3.1.sh"),
        ),
        (
            ".github/storm/gate.sh",
            "gate-0.3.2.sh",
            include_str!("testdata/retired/gate-0.3.2.sh"),
        ),
        (
            ".github/storm/squash-body.sh",
            "squash-body-0.2.0.sh",
            include_str!("testdata/retired/squash-body-0.2.0.sh"),
        ),
        (
            ".github/storm/squash-body.sh",
            "squash-body-0.3.2.sh",
            include_str!("testdata/retired/squash-body-0.3.2.sh"),
        ),
        (
            ".claude/skills/adv-rev/SKILL.md",
            "skill-adv-rev-0.2.0.md",
            include_str!("testdata/retired/skill-adv-rev-0.2.0.md"),
        ),
        (
            ".claude/skills/adv-rev/SKILL.md",
            "skill-adv-rev-0.3.0.md",
            include_str!("testdata/retired/skill-adv-rev-0.3.0.md"),
        ),
        (
            ".claude/skills/decompose/SKILL.md",
            "skill-decompose-69d748d.md",
            include_str!("testdata/retired/skill-decompose-69d748d.md"),
        ),
        (
            ".claude/skills/handoff/SKILL.md",
            "skill-handoff-0.2.0.md",
            include_str!("testdata/retired/skill-handoff-0.2.0.md"),
        ),
        (
            ".claude/skills/handoff/SKILL.md",
            "skill-handoff-0.3.0.md",
            include_str!("testdata/retired/skill-handoff-0.3.0.md"),
        ),
        (
            ".claude/skills/handoff/SKILL.md",
            "skill-handoff-0.4.0.md",
            include_str!("testdata/retired/skill-handoff-0.4.0.md"),
        ),
        (
            ".claude/skills/handoff/SKILL.md",
            "skill-handoff-69d748d.md",
            include_str!("testdata/retired/skill-handoff-69d748d.md"),
        ),
        (
            ".claude/skills/implement/SKILL.md",
            "skill-implement-0.2.0.md",
            include_str!("testdata/retired/skill-implement-0.2.0.md"),
        ),
        (
            ".claude/skills/implement/SKILL.md",
            "skill-implement-0.3.0.md",
            include_str!("testdata/retired/skill-implement-0.3.0.md"),
        ),
        (
            ".claude/skills/implement/SKILL.md",
            "skill-implement-0.3.1.md",
            include_str!("testdata/retired/skill-implement-0.3.1.md"),
        ),
        (
            ".claude/skills/implement/SKILL.md",
            "skill-implement-0.3.3.md",
            include_str!("testdata/retired/skill-implement-0.3.3.md"),
        ),
        (
            ".claude/skills/implement/SKILL.md",
            "skill-implement-0.4.0.md",
            include_str!("testdata/retired/skill-implement-0.4.0.md"),
        ),
        (
            ".claude/skills/implement/SKILL.md",
            "skill-implement-69d748d.md",
            include_str!("testdata/retired/skill-implement-69d748d.md"),
        ),
        (
            ".claude/skills/resync/SKILL.md",
            "skill-resync-0.2.0.md",
            include_str!("testdata/retired/skill-resync-0.2.0.md"),
        ),
        (
            ".claude/skills/resync/SKILL.md",
            "skill-resync-0.3.0.md",
            include_str!("testdata/retired/skill-resync-0.3.0.md"),
        ),
        (
            ".claude/skills/resync/SKILL.md",
            "skill-resync-69d748d.md",
            include_str!("testdata/retired/skill-resync-69d748d.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-0.2.0.md",
            include_str!("testdata/retired/skill-review-0.2.0.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-0.3.0.md",
            include_str!("testdata/retired/skill-review-0.3.0.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-0.4.0.md",
            include_str!("testdata/retired/skill-review-0.4.0.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-0.6.0.md",
            include_str!("testdata/retired/skill-review-0.6.0.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-0.7.0.md",
            include_str!("testdata/retired/skill-review-0.7.0.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-0.8.0.md",
            include_str!("testdata/retired/skill-review-0.8.0.md"),
        ),
        (
            ".claude/skills/review/SKILL.md",
            "skill-review-69d748d.md",
            include_str!("testdata/retired/skill-review-69d748d.md"),
        ),
        (
            ".claude/skills/review/references/reading-the-diff.md",
            "skill-review-reading-the-diff-0.2.0.md",
            include_str!("testdata/retired/skill-review-reading-the-diff-0.2.0.md"),
        ),
        (
            ".claude/skills/revise/SKILL.md",
            "skill-revise-0.2.0.md",
            include_str!("testdata/retired/skill-revise-0.2.0.md"),
        ),
        (
            ".claude/skills/revise/SKILL.md",
            "skill-revise-0.3.3.md",
            include_str!("testdata/retired/skill-revise-0.3.3.md"),
        ),
        (
            ".claude/skills/revise/SKILL.md",
            "skill-revise-0.7.0.md",
            include_str!("testdata/retired/skill-revise-0.7.0.md"),
        ),
        (
            ".claude/skills/revise/SKILL.md",
            "skill-revise-0.8.0.md",
            include_str!("testdata/retired/skill-revise-0.8.0.md"),
        ),
        (
            ".claude/skills/revise/SKILL.md",
            "skill-revise-69d748d.md",
            include_str!("testdata/retired/skill-revise-69d748d.md"),
        ),
        (
            ".claude/skills/rubber-duck/SKILL.md",
            "skill-rubber-duck-69d748d.md",
            include_str!("testdata/retired/skill-rubber-duck-69d748d.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.2.0.md",
            include_str!("testdata/retired/skill-storm-0.2.0.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.3.0.md",
            include_str!("testdata/retired/skill-storm-0.3.0.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.3.3.md",
            include_str!("testdata/retired/skill-storm-0.3.3.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.4.0.md",
            include_str!("testdata/retired/skill-storm-0.4.0.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.6.0.md",
            include_str!("testdata/retired/skill-storm-0.6.0.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.7.0.md",
            include_str!("testdata/retired/skill-storm-0.7.0.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-0.8.0.md",
            include_str!("testdata/retired/skill-storm-0.8.0.md"),
        ),
        (
            ".claude/skills/storm/SKILL.md",
            "skill-storm-69d748d.md",
            include_str!("testdata/retired/skill-storm-69d748d.md"),
        ),
        (
            ".claude/skills/writing-docs/SKILL.md",
            "skill-writing-docs-0.2.0.md",
            include_str!("testdata/retired/skill-writing-docs-0.2.0.md"),
        ),
        (
            ".claude/skills/writing-docs/SKILL.md",
            "skill-writing-docs-0.3.1.md",
            include_str!("testdata/retired/skill-writing-docs-0.3.1.md"),
        ),
        (
            ".claude/skills/writing-docs/references/tells.md",
            "skill-writing-docs-tells-0.2.0.md",
            include_str!("testdata/retired/skill-writing-docs-tells-0.2.0.md"),
        ),
    ];

    /// `.github/storm/squash-body.sh` as v0.3.2 to v0.5.0 installed it.
    const SQUASH_BODY_0_5_0: &str = include_str!("testdata/retired/squash-body-0.3.2.sh");

    #[test]
    fn retired_hashes_match_the_versions_in_testdata() {
        for (path, hashes) in RETIRED {
            let listed: BTreeSet<&str> = hashes.iter().copied().collect();
            let found: BTreeSet<String> = RETIRED_VERSIONS
                .iter()
                .filter(|(p, _, _)| p == path)
                .map(|(_, _, contents)| sha256_hex(contents.as_bytes()))
                .collect();
            let found: BTreeSet<&str> = found.iter().map(String::as_str).collect();
            assert_eq!(listed, found, "{path}");
        }
        for (path, _, _) in RETIRED_VERSIONS {
            assert!(RETIRED.iter().any(|(p, _)| p == path), "{path} is not retired");
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/testdata/retired");
        let on_disk: BTreeSet<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        let named: BTreeSet<String> = RETIRED_VERSIONS.iter().map(|(_, name, _)| name.to_string()).collect();
        assert_eq!(on_disk, named);
    }

    #[test]
    fn update_deletes_every_version_of_a_retired_file() {
        for (path, name, contents) in RETIRED_VERSIONS {
            let dir = tempfile::tempdir().unwrap();
            install(dir.path(), &settings(), false).unwrap();
            let target = dir.path().join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, contents).unwrap();
            let report = update(dir.path(), |_| {}, false).unwrap();
            assert_eq!(report.removed, vec![PathBuf::from(path)], "{name}");
            assert!(!target.exists(), "{name}");
        }
    }

    #[test]
    fn retired_files_are_not_installed_and_list_sha256_hashes() {
        for (path, hashes) in RETIRED {
            assert!(embedded(path).is_none(), "{path} is still installed");
            assert!(!hashes.is_empty(), "{path} lists no version");
            for hash in *hashes {
                assert!(
                    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                    "{path}: {hash}"
                );
            }
        }
    }

    #[test]
    fn update_deletes_a_retired_file_as_a_release_wrote_it() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(".github/storm/squash-body.sh");
        fs::write(&path, SQUASH_BODY_0_5_0).unwrap();
        assert_eq!(
            check(dir.path()).unwrap(),
            vec![Drift::Retired(".github/storm/squash-body.sh".into())]
        );
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert_eq!(report.removed, vec![PathBuf::from(".github/storm/squash-body.sh")]);
        assert!(report.kept.is_empty());
        assert!(!path.exists());
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn update_keeps_and_reports_an_edited_retired_file() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let edited = format!("{SQUASH_BODY_0_5_0}# mine\n");
        let path = dir.path().join(".github/storm/squash-body.sh");
        fs::write(&path, &edited).unwrap();
        fs::write(dir.path().join(".github/storm/gate.sh"), "#!/bin/sh\n").unwrap();
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty());
        assert_eq!(
            report.kept,
            vec![
                Kept { path: ".github/storm/gate.sh".into(), reason: KeptReason::Edited },
                Kept { path: ".github/storm/squash-body.sh".into(), reason: KeptReason::Edited },
            ]
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        assert_eq!(
            check(dir.path()).unwrap(),
            vec![
                Drift::Retired(".github/storm/gate.sh".into()),
                Drift::Retired(".github/storm/squash-body.sh".into()),
            ]
        );
    }

    #[test]
    fn installed_skills_carry_the_fab_prefix() {
        let skills: BTreeSet<&str> = EMBEDDED
            .iter()
            .filter_map(|(path, _, _)| path.strip_prefix(".claude/skills/"))
            .map(|rest| rest.split('/').next().unwrap())
            .collect();
        assert!(skills.contains("fab-review"), "{skills:?}");
        for skill in skills {
            assert!(skill.starts_with("fab-"), "{skill}");
            let text = embedded(&format!(".claude/skills/{skill}/SKILL.md")).unwrap();
            assert!(text.lines().any(|l| l == format!("name: {skill}")), "{skill}");
        }
    }

    /// Each old skill path with the version `main` held before the rename,
    /// the last one listed for it.
    fn old_skills() -> Vec<(&'static str, &'static str)> {
        let mut last = std::collections::BTreeMap::new();
        for (path, _, contents) in RETIRED_VERSIONS {
            if path.starts_with(".claude/skills/") {
                last.insert(*path, *contents);
            }
        }
        last.into_iter().collect()
    }

    #[test]
    fn update_moves_skills_from_their_old_names() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let old = old_skills();
        assert_eq!(old.len(), 12);
        for (path, contents) in &old {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, contents).unwrap();
        }
        let drift = check(dir.path()).unwrap();
        assert_eq!(drift.len(), old.len(), "{drift:?}");
        let report = update(dir.path(), |_| {}, false).unwrap();
        let removed: Vec<&str> = report.removed.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(removed, old.iter().map(|(p, _)| *p).collect::<Vec<_>>());
        assert!(report.kept.is_empty(), "{:?}", report.kept);
        let left: Vec<String> = fs::read_dir(dir.path().join(".claude/skills"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|name| !name.starts_with("fab-"))
            .collect();
        assert_eq!(left, Vec::<String>::new());
        assert!(
            dir.path()
                .join(".claude/skills/fab-review/references/reading-the-diff.md")
                .is_file()
        );
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    /// An edited copy under an old name keeps every file of its skill, the
    /// unedited reference included, and none is reported.
    #[test]
    fn update_keeps_an_edited_skill_under_its_old_name_whole() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let (path, contents) = old_skills()
            .into_iter()
            .find(|(p, _)| *p == ".claude/skills/review/SKILL.md")
            .unwrap();
        let reference = ".claude/skills/review/references/reading-the-diff.md";
        let edited = format!("{contents}\nOur own rule.\n");
        let full = dir.path().join(path);
        fs::create_dir_all(dir.path().join(reference).parent().unwrap()).unwrap();
        fs::write(&full, &edited).unwrap();
        let reference_text = embedded(".claude/skills/fab-review/references/reading-the-diff.md").unwrap();
        fs::write(dir.path().join(reference), reference_text).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty(), "{:?}", report.removed);
        assert!(report.kept.is_empty(), "{:?}", report.kept);
        assert_eq!(fs::read_to_string(&full).unwrap(), edited);
        assert_eq!(fs::read_to_string(dir.path().join(reference)).unwrap(), reference_text);
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    /// Writes the last pre-rename `review` skill under its old name, with
    /// its reference as `reference` holds it.
    fn old_review(root: &Path, reference: &str) {
        let (_, contents) = old_skills()
            .into_iter()
            .find(|(p, _)| *p == ".claude/skills/review/SKILL.md")
            .unwrap();
        let skill = root.join(".claude/skills/review");
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(skill.join("SKILL.md"), contents).unwrap();
        fs::write(skill.join("references/reading-the-diff.md"), reference).unwrap();
    }

    /// An unedited old skill beside a file the repository added is kept
    /// whole and reported, never half removed.
    #[test]
    fn update_keeps_an_old_skill_with_an_added_file_whole() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let reference = embedded(".claude/skills/fab-review/references/reading-the-diff.md").unwrap();
        old_review(dir.path(), reference);
        let added = dir.path().join(".claude/skills/review/references/checklist.md");
        fs::write(&added, "Our checklist.\n").unwrap();
        let skill = PathBuf::from(".claude/skills/review");
        assert_eq!(check(dir.path()).unwrap(), vec![Drift::Retired(skill.clone())]);
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty(), "{:?}", report.removed);
        assert_eq!(
            report.kept,
            vec![Kept { path: skill.clone(), reason: KeptReason::SkillChanged }]
        );
        assert!(dir.path().join(&skill).join("SKILL.md").is_file());
        assert!(dir.path().join(&skill).join("references/reading-the-diff.md").is_file());
        assert!(added.is_file());
        assert_eq!(check(dir.path()).unwrap(), vec![Drift::Retired(skill)]);
    }

    /// An unedited old `SKILL.md` beside an edited reference is kept whole
    /// and reported.
    #[test]
    fn update_keeps_an_old_skill_with_an_edited_reference_whole() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        old_review(dir.path(), "Our reading notes.\n");
        let skill = PathBuf::from(".claude/skills/review");
        assert_eq!(check(dir.path()).unwrap(), vec![Drift::Retired(skill.clone())]);
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty(), "{:?}", report.removed);
        assert_eq!(
            report.kept,
            vec![Kept { path: skill.clone(), reason: KeptReason::SkillChanged }]
        );
        assert!(dir.path().join(&skill).join("SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(dir.path().join(&skill).join("references/reading-the-diff.md")).unwrap(),
            "Our reading notes.\n"
        );
    }

    /// A repository's own skill at an old name is neither reported nor
    /// touched.
    #[test]
    fn update_and_check_leave_a_repositorys_own_skill_alone() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let own = "---\nname: review\ndescription: Our review.\n---\n\nReview it our way.\n";
        let notes = "Our notes.\n";
        let skill = dir.path().join(".claude/skills/review");
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(skill.join("SKILL.md"), own).unwrap();
        fs::write(skill.join("references/reading-the-diff.md"), notes).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty() && report.kept.is_empty(), "{report:?}");
        assert_eq!(fs::read_to_string(skill.join("SKILL.md")).unwrap(), own);
        assert_eq!(
            fs::read_to_string(skill.join("references/reading-the-diff.md")).unwrap(),
            notes
        );
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    /// A symlink at an old skill path is not a file a release wrote, so it
    /// is left alone and not reported.
    #[cfg(unix)]
    #[test]
    fn update_and_check_leave_a_symlinked_old_skill_alone() {
        let outside = tempfile::tempdir().unwrap();
        let (_, contents) = old_skills()
            .into_iter()
            .find(|(p, _)| *p == ".claude/skills/review/SKILL.md")
            .unwrap();
        fs::write(outside.path().join("SKILL.md"), contents).unwrap();
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let link = dir.path().join(".claude/skills/review");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty() && report.kept.is_empty(), "{report:?}");
        assert!(outside.path().join("SKILL.md").is_file());
    }

    #[test]
    fn update_without_retired_files_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert!(report.removed.is_empty() && report.kept.is_empty());
        assert_eq!(check(dir.path()).unwrap(), vec![]);
    }

    #[cfg(unix)]
    #[test]
    fn update_keeps_a_symlink_at_a_retired_path() {
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("target");
        fs::write(&target, SQUASH_BODY_0_5_0).unwrap();
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let link = dir.path().join(".github/storm/squash-body.sh");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert_eq!(
            report.kept,
            vec![Kept { path: ".github/storm/squash-body.sh".into(), reason: KeptReason::Symlink }]
        );
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), SQUASH_BODY_0_5_0);
        assert_eq!(
            check(dir.path()).unwrap(),
            vec![Drift::Retired(".github/storm/squash-body.sh".into())]
        );
    }

    #[test]
    fn update_keeps_a_directory_at_a_retired_path() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        fs::create_dir(dir.path().join(".github/storm/gate.sh")).unwrap();
        let report = update(dir.path(), |_| {}, false).unwrap();
        assert_eq!(
            report.kept,
            vec![Kept { path: ".github/storm/gate.sh".into(), reason: KeptReason::NotAFile }]
        );
        assert!(dir.path().join(".github/storm/gate.sh").is_dir());
    }

    /// A delete that fails is kept with its error, and the files `init`
    /// wrote before it stay in the report. Skipped where the user can write
    /// to a read-only directory, as root can.
    #[cfg(unix)]
    #[test]
    fn update_reports_its_writes_when_a_retired_file_cannot_be_deleted() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let storm = dir.path().join(".github/storm");
        fs::write(storm.join("squash-body.sh"), SQUASH_BODY_0_5_0).unwrap();
        fs::set_permissions(&storm, fs::Permissions::from_mode(0o555)).unwrap();
        if fs::write(storm.join("probe"), "").is_ok() {
            fs::set_permissions(&storm, fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!("skipped: this user can write to a read-only directory");
            return;
        }
        let report = update(dir.path(), |s| s.owner = "someone-else".into(), false);
        fs::set_permissions(&storm, fs::Permissions::from_mode(0o755)).unwrap();
        let report = report.unwrap();
        assert_eq!(
            report.written,
            vec![PathBuf::from(CODEOWNERS_PATH), PathBuf::from(SETTINGS_PATH)]
        );
        assert!(report.removed.is_empty());
        assert_eq!(report.kept.len(), 1);
        assert_eq!(report.kept[0].path, PathBuf::from(".github/storm/squash-body.sh"));
        assert!(
            matches!(report.kept[0].reason, KeptReason::Failed(_)),
            "{:?}",
            report.kept[0]
        );
        assert!(storm.join("squash-body.sh").exists());
    }

    /// Every file a tagged release installed is installed by this build or
    /// listed in [`RETIRED`] with that release's version of it. A tag's
    /// installed files are the `include_str!` paths in its `install.rs`.
    /// Skipped without git or tags, as in a shallow checkout.
    #[test]
    fn every_file_a_release_installed_is_installed_or_retired() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| out.stdout)
        };
        let tags = git(&["tag", "--list", "v*"])
            .map(|out| String::from_utf8(out).unwrap())
            .unwrap_or_default();
        if tags.trim().is_empty() {
            eprintln!("skipped: no release tags in this checkout");
            return;
        }
        for tag in tags.lines() {
            let Some(source) = git(&["show", &format!("{tag}:crates/fab-core/src/install.rs")]) else {
                continue;
            };
            let source = String::from_utf8(source).unwrap();
            for path in source
                .split("include_str!(\"../../../")
                .skip(1)
                .filter_map(|rest| rest.split('"').next())
            {
                if embedded(path).is_some() {
                    continue;
                }
                let hashes = RETIRED
                    .iter()
                    .find(|(p, _)| *p == path)
                    .unwrap_or_else(|| panic!("{tag} installed {path}, which is neither installed nor retired"))
                    .1;
                let contents = git(&["show", &format!("{tag}:{path}")]).unwrap();
                assert!(
                    hashes.contains(&sha256_hex(&contents).as_str()),
                    "{tag}'s {path} is not in RETIRED"
                );
            }
        }
    }

    #[test]
    fn install_refuses_an_installed_repository() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        assert!(matches!(install(dir.path(), &settings(), true), Err(Error::Installed)));
    }

    #[test]
    fn init_writes_a_starter_gates_file_that_update_and_check_leave_alone() {
        let dir = tempfile::tempdir().unwrap();
        let report = install(dir.path(), &settings(), false).unwrap();
        assert!(report.written.contains(&PathBuf::from(GATES_PATH)));
        let path = dir.path().join(GATES_PATH);
        let starter = Gates::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(starter.full, settings().gates);

        let own = "full = \"make test\"\n";
        fs::write(&path, own).unwrap();
        let report = update(dir.path(), |s| s.gates = "just check".into(), false).unwrap();
        assert!(!report.written.contains(&PathBuf::from(GATES_PATH)));
        assert_eq!(fs::read_to_string(&path).unwrap(), own);
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        fs::remove_file(&path).unwrap();
        update(dir.path(), |_| {}, false).unwrap();
        assert!(!path.exists());
        assert_eq!(check(dir.path()).unwrap(), vec![]);

        // A first install keeps the repository's own file, even with force.
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(GATES_PATH), own).unwrap();
        let report = install(dir.path(), &settings(), true).unwrap();
        assert!(report.unchanged.contains(&PathBuf::from(GATES_PATH)));
        assert_eq!(fs::read_to_string(dir.path().join(GATES_PATH)).unwrap(), own);
    }

    #[test]
    fn update_needs_an_install_and_keeps_its_settings() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(update(dir.path(), |_| {}, false), Err(Error::NotInstalled)));
        install(dir.path(), &settings(), false).unwrap();
        fs::write(dir.path().join(".github/storm/shot.sh"), "#!/bin/sh\n").unwrap();
        let report = update(dir.path(), |s| s.gates = "just check".into(), false).unwrap();
        assert_eq!(report.written.len(), 2, "{:?}", report.written);
        let toml = fs::read_to_string(dir.path().join(SETTINGS_PATH)).unwrap();
        assert!(toml.contains(r#"gates = "just check""#));
        assert!(toml.contains(r#"owner = "desertthunder""#));
        assert_eq!(check(dir.path()).unwrap(), vec![]);
        update(dir.path(), |s| s.signoff = Some("Owais <o@x>".into()), false).unwrap();
        update(dir.path(), |s| s.planning = Some(vec!["plans/".into()]), false).unwrap();
        update(dir.path(), |_| {}, false).unwrap();
        let toml = fs::read_to_string(dir.path().join(SETTINGS_PATH)).unwrap();
        assert!(toml.contains(r#"signoff = "Owais <o@x>""#), "{toml}");
        assert!(toml.contains(r#"planning = ["plans/"]"#), "{toml}");
    }

    #[test]
    fn update_refuses_to_downgrade_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let newer = fs::read_to_string(&path).unwrap().replace(
            &format!(r#"version = "{}""#, crate::VERSION),
            r#"version = "99.0.0+abc1234""#,
        );
        fs::write(&path, newer).unwrap();
        let err = update(dir.path(), |_| {}, false).unwrap_err();
        assert!(matches!(&err, Error::Downgrade { installed, .. } if installed == "99.0.0+abc1234"));
        update(dir.path(), |_| {}, true).unwrap();
        assert_eq!(check(dir.path()).unwrap(), vec![]);

        let text = fs::read_to_string(&path).unwrap().replace(
            &format!(r#"version = "{}""#, crate::VERSION),
            r#"version = "0.3.0-rc.1""#,
        );
        fs::write(&path, text).unwrap();
        let err = update(dir.path(), |_| {}, false).unwrap_err();
        assert!(matches!(&err, Error::UnknownVersion(v) if v == "0.3.0-rc.1"));
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
        fs::write(dir.path().join(".github/storm/shot.sh"), "#!/bin/sh\n").unwrap();
        fs::remove_file(dir.path().join(".claude/agents/verifier.md")).unwrap();
        let drift = check(dir.path()).unwrap();
        assert!(drift.contains(&Drift::Differs(".github/storm/shot.sh".into())));
        assert!(drift.contains(&Drift::Missing(".claude/agents/verifier.md".into())));
    }

    #[test]
    fn check_reports_an_older_installed_version() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let text = fs::read_to_string(&path).unwrap().replace(crate::VERSION, "0.0.1");
        fs::write(&path, text).unwrap();
        let drift = check(dir.path()).unwrap();
        assert!(matches!(&drift[..], [Drift::Version { installed: Some(v), .. }] if v == "0.0.1"));
    }

    #[test]
    fn init_records_the_release_alone_and_check_accepts_an_older_stamp() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        let path = dir.path().join(SETTINGS_PATH);
        let stamp = format!(r#"version = "{}""#, crate::VERSION);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.lines().any(|l| l == stamp), "{text}");
        // 0.2.0 and earlier recorded the commit too.
        let text = text.replace(&stamp, &format!(r#"version = "{}+abc1234""#, crate::VERSION));
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
        std::os::unix::fs::symlink(&target, dir.path().join(".github/storm/shot.sh")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude")).unwrap();
        for force in [false, true] {
            let err = init(dir.path(), &settings(), force).unwrap_err();
            assert!(
                matches!(&err, Error::Unmanageable(paths)
                    if *paths == vec![PathBuf::from(".claude"), PathBuf::from(".github/storm/shot.sh")]),
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
        let script = dir.path().join(".github/storm/shot.sh");
        fs::remove_file(&script).unwrap();
        std::os::unix::fs::symlink(&target, &script).unwrap();
        assert!(matches!(
            init(dir.path(), &settings(), true),
            Err(Error::Unmanageable(_))
        ));
        assert_eq!(fs::read_to_string(&target).unwrap(), "theirs\n");
        assert_eq!(
            check(dir.path()).unwrap(),
            vec![Drift::Differs(".github/storm/shot.sh".into())]
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
    fn scripts_are_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &settings(), false).unwrap();
        for script in [".github/storm/shot.sh", ".github/storm/sgr.pl"] {
            let mode = fs::metadata(dir.path().join(script)).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "{script}");
        }
    }
}
