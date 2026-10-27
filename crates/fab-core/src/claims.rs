//! Task claims on `refs/fabrial/claims` in a remote. A claim names a task,
//! the machine and run that hold it, and when it expires.
//!
//! The ref is a chain of commits whose tree holds one file per claimed task,
//! `claims/<escaped task>.toml`. A write fetches the ref, decides from the
//! claims it holds, commits the new tree on the fetched tip and pushes it
//! without force. The push is the lock: when another writer moved the ref
//! first, the push is rejected and the write starts again from the new tip.
//! Writes build their objects with git plumbing and never touch the
//! checkout's working tree, index or `HEAD`.
//!
//! A task is an opaque string: fabrial reads no tracker.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// The ref that holds the claims, in the remote and in the local mirror of it.
pub const REF: &str = "refs/fabrial/claims";
/// The directory in the ref's tree that holds one file per claim.
pub const DIR: &str = "claims";
/// How long a claim lasts unless the take or renew says otherwise, in seconds.
pub const DEFAULT_TTL: u64 = 60 * 60;
/// How many times a write fetches and pushes before giving up on a ref other
/// writers keep moving.
pub const ATTEMPTS: usize = 5;
/// The longest escaped task ID, in bytes, which keeps the file name inside
/// every file system's limit.
const MAX_NAME: usize = 200;

/// What a write does to one task's claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Claim the task for `ttl` seconds. Succeeds when nobody holds it, when
    /// the holder's claim expired, or when this run already holds it.
    Take { ttl: u64 },
    /// Extend this run's claim to `ttl` seconds from now.
    Renew { ttl: u64 },
    /// Remove this run's claim.
    Release,
}

/// What a write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nobody held the task, and now this run does.
    Claimed(Claim),
    /// This run already held the task; the claim now runs to the new expiry.
    AlreadyHeld(Claim),
    /// Another run's claim had expired, and this run's replaced it.
    TookOver { claim: Claim, expired: Claim },
    /// This run's claim now runs to the new expiry.
    Renewed(Claim),
    /// This run's claim is gone.
    Released(Claim),
}

/// What can go wrong reading or writing claims.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A task, machine or run is empty or holds a control character.
    #[error("{what} {value:?} is empty or holds a control character")]
    Invalid { what: &'static str, value: String },
    /// The task ID is too long to name a file.
    #[error("task {0:?} is too long to claim")]
    TooLong(String),
    /// Another run holds a live claim on the task.
    #[error(
        "run {} on {} holds {} until {}",
        .0.run, .0.machine, .0.task, .0.expires_at
    )]
    Held(Claim),
    /// The run renewing or releasing a claim does not hold it.
    #[error("run {run} does not hold {task}")]
    NotHeld { task: String, run: String },
    /// Other writers moved the ref on every attempt.
    #[error("{REF} kept moving; gave up after {ATTEMPTS} attempts")]
    Contended,
    /// A claim file does not parse, or its name does not match its task.
    #[error("{REF}: {0}")]
    Corrupt(String),
    /// A git command failed.
    #[error("{0}")]
    Git(String),
}

/// One claim, as its file in the ref holds it. Times are UTC seconds since
/// the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    /// The task ID, as the tracker or person gave it.
    pub task: String,
    /// The machine the run is on.
    pub machine: String,
    /// The run that holds the task.
    pub run: String,
    /// When the run first claimed the task.
    pub claimed_at: u64,
    /// When the claim stops holding the task.
    pub expires_at: u64,
}

impl Claim {
    /// Whether the claim still holds its task at `now`.
    pub fn is_live(&self, now: u64) -> bool {
        now < self.expires_at
    }
}

/// One write to make: what to do to which task, for which run.
#[derive(Debug, Clone)]
pub struct Request {
    pub task: String,
    pub machine: String,
    pub run: String,
    pub action: Action,
    /// The time the write decides by, in UTC seconds.
    pub now: u64,
}

/// A write built on the tip it fetched, ready to push.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The commit to push.
    pub commit: String,
    /// What the write does once the push lands.
    pub outcome: Outcome,
}

/// The claims of one remote, read and written through a local repository.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    remote: String,
}

impl Store {
    /// A store for `remote`, reached through the repository at `dir`.
    pub fn new(dir: impl Into<PathBuf>, remote: impl Into<String>) -> Self {
        Self { dir: dir.into(), remote: remote.into() }
    }

    /// Every claim in the remote, sorted by task, expired ones included.
    pub fn list(&self) -> Result<Vec<Claim>, Error> {
        let Some(tip) = self.fetch()? else {
            return Ok(Vec::new());
        };
        let mut claims: Vec<Claim> = self.read(&tip)?.into_values().map(|(_, c)| c).collect();
        claims.sort_by(|a, b| a.task.cmp(&b.task));
        Ok(claims)
    }

    /// Makes the write, fetching again and retrying while other writers move
    /// the ref first.
    pub fn write(&self, request: &Request) -> Result<Outcome, Error> {
        for _ in 0..ATTEMPTS {
            let prepared = self.prepare(request)?;
            if self.publish(&prepared)? {
                return Ok(prepared.outcome);
            }
        }
        Err(Error::Contended)
    }

    /// The first step of a write: fetches the ref, decides, and builds the
    /// commit on the tip it fetched. Expired claims on other tasks are left
    /// out of the new tree.
    pub fn prepare(&self, request: &Request) -> Result<Prepared, Error> {
        check("task", &request.task)?;
        check("machine", &request.machine)?;
        check("run", &request.run)?;
        let name = file_name(&request.task)?;
        let tip = self.fetch()?;
        let mut claims = match &tip {
            Some(tip) => self.read(tip)?,
            None => BTreeMap::new(),
        };
        let existing = claims.remove(&name).map(|(_, c)| c);
        claims.retain(|_, (_, c)| c.is_live(request.now));
        let outcome = decide(request, existing)?;
        let mut entries: Vec<(String, String)> = claims.into_iter().map(|(n, (blob, _))| (n, blob)).collect();
        if let Outcome::Claimed(c)
        | Outcome::AlreadyHeld(c)
        | Outcome::Renewed(c)
        | Outcome::TookOver { claim: c, .. } = &outcome
        {
            let text = toml::to_string(c).map_err(|e| Error::Corrupt(e.to_string()))?;
            entries.push((name, self.git(&["hash-object", "-w", "--stdin"], Some(&text))?));
        }
        let commit = self.commit(tip.as_deref(), &entries, &message(request))?;
        Ok(Prepared { commit, outcome })
    }

    /// The second step of a write: pushes the commit without force. `false`
    /// when the remote rejected it because the ref moved since the fetch.
    pub fn publish(&self, prepared: &Prepared) -> Result<bool, Error> {
        let refspec = format!("{}:{REF}", prepared.commit);
        let output = self
            .command(&["push", "--porcelain", "--no-verify", &self.remote, &refspec])
            .output()
            .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
        if output.status.success() {
            return Ok(true);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.lines().any(|l| l.starts_with('!') && l.contains("\t[rejected]")) {
            return Ok(false);
        }
        Err(Error::Git(format!(
            "git push {} {refspec} failed: {}",
            self.remote,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }

    /// Fetches the remote's ref into the local mirror and returns its tip,
    /// or `None` when the remote has no claims yet.
    fn fetch(&self) -> Result<Option<String>, Error> {
        let refspec = format!("+{REF}:{REF}");
        let output = self
            .command(&[
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
                &self.remote,
                &refspec,
            ])
            .output()
            .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("couldn't find remote ref") {
                return Ok(None);
            }
            return Err(Error::Git(format!(
                "git fetch {} {REF} failed: {}",
                self.remote,
                stderr.trim()
            )));
        }
        self.git(&["rev-parse", "--verify", &format!("{REF}^{{commit}}")], None)
            .map(Some)
    }

    /// The claims in `tip`'s tree, by file name, with each one's blob.
    fn read(&self, tip: &str) -> Result<BTreeMap<String, (String, Claim)>, Error> {
        let listing = self.git(&["ls-tree", "-z", tip, "--", &format!("{DIR}/")], None)?;
        let mut blobs = Vec::new();
        for entry in listing.split('\0').filter(|e| !e.is_empty()) {
            let (meta, path) = entry
                .split_once('\t')
                .ok_or_else(|| Error::Corrupt(format!("unreadable tree entry {entry:?}")))?;
            let blob = meta.split(' ').nth(2).unwrap_or_default().to_string();
            let name = path.strip_prefix(&format!("{DIR}/")).unwrap_or(path).to_string();
            blobs.push((name, blob));
        }
        let mut claims = BTreeMap::new();
        for (name, blob) in blobs {
            let text = self.git(&["cat-file", "blob", &blob], None)?;
            let claim: Claim = toml::from_str(&text).map_err(|e| Error::Corrupt(format!("{DIR}/{name}: {e}")))?;
            if file_name(&claim.task).ok().as_deref() != Some(name.as_str()) {
                return Err(Error::Corrupt(format!("{DIR}/{name} holds task {:?}", claim.task)));
            }
            claims.insert(name, (blob, claim));
        }
        Ok(claims)
    }

    /// Writes the tree of `entries`, file name and blob, and a commit of it
    /// on `parent`.
    fn commit(&self, parent: Option<&str>, entries: &[(String, String)], message: &str) -> Result<String, Error> {
        let listing: String = entries
            .iter()
            .map(|(name, blob)| format!("100644 blob {blob}\t{name}\0"))
            .collect();
        let dir = self.git(&["mktree", "-z"], Some(&listing))?;
        let root = if entries.is_empty() { String::new() } else { format!("040000 tree {dir}\t{DIR}\0") };
        let tree = self.git(&["mktree", "-z"], Some(&root))?;
        let mut args = vec!["commit-tree", "--no-gpg-sign", tree.as_str(), "-m", message];
        if let Some(parent) = parent {
            args.extend(["-p", parent]);
        }
        self.git(&args, None)
    }

    /// Runs git in the repository and returns its standard output without
    /// the final newline.
    fn git(&self, args: &[&str], stdin: Option<&str>) -> Result<String, Error> {
        let mut command = self.command(args);
        command
            .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let fail = |e: std::io::Error| Error::Git(format!("cannot run git: {e}"));
        let mut child = command.spawn().map_err(fail)?;
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .expect("piped")
                .write_all(input.as_bytes())
                .map_err(fail)?;
        }
        let output = child.wait_with_output().map_err(fail)?;
        if !output.status.success() {
            return Err(Error::Git(format!(
                "git {} failed: {}",
                args.iter().take(3).copied().collect::<Vec<_>>().join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string())
    }

    /// A git command in the repository. Claim commits carry fabrial's name,
    /// so they need no identity configured on the machine.
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "fabrial")
            .env("GIT_AUTHOR_EMAIL", "fabrial@invalid")
            .env("GIT_COMMITTER_NAME", "fabrial")
            .env("GIT_COMMITTER_EMAIL", "fabrial@invalid")
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }
}

/// The file name a task's claim lives in: the task ID with every byte
/// outside `A-Z a-z 0-9 - _ .`, and a leading `.`, written as `%XX`, then
/// `.toml`. Distinct IDs get distinct names.
pub fn file_name(task: &str) -> Result<String, Error> {
    check("task", task)?;
    let mut name = String::new();
    for (i, byte) in task.bytes().enumerate() {
        let plain = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') || (byte == b'.' && i > 0);
        if plain {
            name.push(byte as char);
        } else {
            name.push_str(&format!("%{byte:02X}"));
        }
    }
    if name.len() > MAX_NAME {
        return Err(Error::TooLong(task.to_string()));
    }
    Ok(name + ".toml")
}

/// The current time in UTC seconds.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Refuses an empty value or one holding a control character.
fn check(what: &'static str, value: &str) -> Result<(), Error> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::Invalid { what, value: value.to_string() });
    }
    Ok(())
}

/// What `request` does to the task's `existing` claim.
fn decide(request: &Request, existing: Option<Claim>) -> Result<Outcome, Error> {
    let fresh = |claimed_at: u64, ttl: u64| Claim {
        task: request.task.clone(),
        machine: request.machine.clone(),
        run: request.run.clone(),
        claimed_at,
        expires_at: request.now.saturating_add(ttl),
    };
    let not_held = || Error::NotHeld { task: request.task.clone(), run: request.run.clone() };
    match (request.action, existing) {
        (Action::Take { ttl }, None) => Ok(Outcome::Claimed(fresh(request.now, ttl))),
        (Action::Take { ttl }, Some(c)) if c.run == request.run => Ok(Outcome::AlreadyHeld(fresh(c.claimed_at, ttl))),
        (Action::Take { ttl }, Some(c)) if !c.is_live(request.now) => {
            Ok(Outcome::TookOver { claim: fresh(request.now, ttl), expired: c })
        }
        (Action::Take { .. }, Some(c)) => Err(Error::Held(c)),
        (Action::Renew { ttl }, Some(c)) if c.run == request.run => Ok(Outcome::Renewed(fresh(c.claimed_at, ttl))),
        (Action::Release, Some(c)) if c.run == request.run => Ok(Outcome::Released(c)),
        (Action::Renew { .. } | Action::Release, _) => Err(not_held()),
    }
}

/// The claim commit's message.
fn message(request: &Request) -> String {
    let verb = match request.action {
        Action::Take { .. } => "take",
        Action::Renew { .. } => "renew",
        Action::Release => "release",
    };
    format!("{verb} {} for run {} on {}", request.task, request.run, request.machine)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(run: &str, action: Action) -> Request {
        Request { task: "t".into(), machine: "m".into(), run: run.into(), action, now: 100 }
    }

    fn claim(run: &str, expires_at: u64) -> Claim {
        Claim { task: "t".into(), machine: "m".into(), run: run.into(), claimed_at: 10, expires_at }
    }

    #[test]
    fn file_names_keep_plain_ids_and_escape_the_rest() {
        assert_eq!(file_name("issue-12").unwrap(), "issue-12.toml");
        assert_eq!(file_name("a/b c").unwrap(), "a%2Fb%20c.toml");
        assert_eq!(file_name("..").unwrap(), "%2E..toml");
        assert_eq!(file_name("a%2F").unwrap(), "a%252F.toml");
        assert_ne!(file_name("a/").unwrap(), file_name("a%2F").unwrap());
        for bad in ["", "a\nb", "\t"] {
            assert!(matches!(file_name(bad), Err(Error::Invalid { .. })), "{bad:?}");
        }
        assert!(matches!(file_name(&"/".repeat(100)), Err(Error::TooLong(_))));
    }

    #[test]
    fn take_refuses_a_live_claim_and_replaces_an_expired_one() {
        let take = Action::Take { ttl: 50 };
        assert!(matches!(
            decide(&request("b", take), Some(claim("a", 101))),
            Err(Error::Held(_))
        ));
        let Ok(Outcome::TookOver { claim: c, expired }) = decide(&request("b", take), Some(claim("a", 100))) else {
            panic!("an expired claim is taken over");
        };
        assert_eq!(
            (c.run.as_str(), c.claimed_at, c.expires_at, expired.run.as_str()),
            ("b", 100, 150, "a")
        );
    }

    #[test]
    fn the_holding_run_retakes_renews_and_releases_its_claim() {
        let Ok(Outcome::AlreadyHeld(c)) = decide(&request("a", Action::Take { ttl: 50 }), Some(claim("a", 101))) else {
            panic!("the holder retakes its claim");
        };
        assert_eq!((c.claimed_at, c.expires_at), (10, 150));
        assert!(matches!(
            decide(&request("a", Action::Renew { ttl: 5 }), Some(claim("a", 101))),
            Ok(Outcome::Renewed(Claim { expires_at: 105, .. }))
        ));
        assert!(matches!(
            decide(&request("a", Action::Release), Some(claim("a", 101))),
            Ok(Outcome::Released(_))
        ));
        for action in [Action::Renew { ttl: 5 }, Action::Release] {
            assert!(matches!(
                decide(&request("b", action), Some(claim("a", 101))),
                Err(Error::NotHeld { .. })
            ));
            assert!(matches!(
                decide(&request("b", action), None),
                Err(Error::NotHeld { .. })
            ));
        }
    }
}
