//! Task claims. A task's branch is its claim: a run claims a task by creating
//! `storm/<task>` in a remote with a push that succeeds only when the branch
//! does not exist yet. A rejected push means another run claimed first.
//!
//! The branch starts with a claim commit: an empty commit on `main` whose
//! trailers name the task, machine, run and expiry. Renewing adds a new claim
//! commit on the branch's tip, a fast-forward. Work goes on top of the claim.
//! An expired claim is taken by adding a claim commit on the tip, so the
//! commits already on the branch stay. Releasing deletes the branch, and only
//! when it holds nothing but claim commits. Every push carries a lease on the
//! tip it was built from, and no push rewrites history.
//!
//! Writes build their commits with git plumbing and never touch the
//! checkout's working tree, index or `HEAD`. A task is an opaque string:
//! fabrial reads no tracker.

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The prefix of every claimed task's branch.
pub const PREFIX: &str = "storm/";
/// How long a claim lasts unless the take or renew says otherwise, in seconds.
pub const DEFAULT_TTL: u64 = 60 * 60;
/// How many times a write fetches and pushes before giving up on a branch
/// other writers keep moving.
pub const ATTEMPTS: usize = 5;
/// The trailer that names the claimed task and marks a claim commit.
pub const TASK_KEY: &str = "Fabrial-Claim";
const MACHINE_KEY: &str = "Fabrial-Machine";
const RUN_KEY: &str = "Fabrial-Run";
const EXPIRES_KEY: &str = "Fabrial-Expires";
/// The longest escaped task ID, in bytes.
const MAX_NAME: usize = 200;

/// What a write does to one task's claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Claim the task for `ttl` seconds. Succeeds when nobody holds it, when
    /// the holder's claim expired, or when this run already holds it. A run
    /// whose own claim expired renews it.
    Take { ttl: u64 },
    /// Extend this run's claim to `ttl` seconds from now.
    Renew { ttl: u64 },
    /// Delete the branch of this run's claim.
    Release,
}

/// What a write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The branch did not exist, and now holds this run's claim.
    Claimed(Claim),
    /// This run already held a live claim on the task; nothing was pushed.
    AlreadyHeld(Claim),
    /// Another run's claim had expired, and this run's claim now sits on the
    /// branch's tip, above that run's commits.
    TookOver { claim: Claim, expired: Claim },
    /// This run's claim now runs to the new expiry.
    Renewed(Claim),
    /// The branch is gone.
    Released(Claim),
}

/// What can go wrong reading or writing a claim.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A task, machine or run is empty, starts or ends with whitespace, or
    /// holds a control character.
    #[error("{what} {value:?} is empty, starts or ends with whitespace, or holds a control character")]
    Invalid { what: &'static str, value: String },
    /// The task ID is too long to name a branch.
    #[error("task {0:?} is too long to claim")]
    TooLong(String),
    /// Another run holds a live claim on the task.
    #[error("run {} on {} holds {} until {}", .0.run, .0.machine, .0.task, utc(.0.expires_at))]
    Held(Claim),
    /// The run renewing or releasing a claim does not hold it.
    #[error("run {run} does not hold {task}")]
    NotHeld { task: String, run: String },
    /// The task's branch exists but holds no claim commit for it.
    #[error("{0} exists and holds no claim")]
    NoClaim(String),
    /// The branch holds work, so releasing would delete it.
    #[error("{0} holds commits besides its claim; it stays")]
    HoldsWork(String),
    /// Other writers moved the branch on every attempt.
    #[error("{0} kept moving; gave up after {ATTEMPTS} attempts")]
    Contended(String),
    /// A git command failed.
    #[error("{0}")]
    Git(String),
}

/// One claim, as a claim commit's trailers hold it. Times are UTC seconds
/// since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The task ID, as the tracker or person gave it.
    pub task: String,
    /// The machine the run is on.
    pub machine: String,
    /// The run that holds the task.
    pub run: String,
    /// When the claim stops holding the task.
    pub expires_at: u64,
}

impl Claim {
    /// Whether the claim still holds its task at `now`.
    pub fn is_live(&self, now: u64) -> bool {
        now < self.expires_at
    }

    /// The claim a commit message records, or `None` when it is not a claim
    /// commit.
    pub fn parse(message: &str) -> Option<Claim> {
        let value = |key: &str| {
            message.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k == key).then(|| v.trim().to_string())
            })
        };
        Some(Claim {
            task: value(TASK_KEY)?,
            machine: value(MACHINE_KEY)?,
            run: value(RUN_KEY)?,
            expires_at: value(EXPIRES_KEY)?.parse().ok()?,
        })
    }

    /// The claim commit's message.
    pub fn message(&self) -> String {
        format!(
            "Claim {} for run {} until {}\n\n{TASK_KEY}: {}\n{MACHINE_KEY}: {}\n{RUN_KEY}: {}\n{EXPIRES_KEY}: {}\n",
            self.task,
            self.run,
            utc(self.expires_at),
            self.task,
            self.machine,
            self.run,
            self.expires_at
        )
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

/// A write decided on the branch it fetched, ready to push.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The task's branch.
    pub branch: String,
    /// The tip the push expects the branch to have, or `None` when it must
    /// not exist.
    pub lease: Option<String>,
    /// The commit to push, `None` to delete the branch. Unused when
    /// `push` is false.
    pub commit: Option<String>,
    /// Whether the write pushes anything.
    pub push: bool,
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

    /// Makes the write, fetching again and retrying while other writers move
    /// the branch first.
    pub fn write(&self, request: &Request) -> Result<Outcome, Error> {
        for _ in 0..ATTEMPTS {
            let prepared = self.prepare(request)?;
            if self.publish(&prepared)? {
                return Ok(prepared.outcome);
            }
        }
        Err(Error::Contended(branch(&request.task)?))
    }

    /// The first step of a write: fetches `main` and the task's branch,
    /// decides, and builds the claim commit on what it fetched.
    pub fn prepare(&self, request: &Request) -> Result<Prepared, Error> {
        check("machine", &request.machine)?;
        check("run", &request.run)?;
        let name = branch(&request.task)?;
        let main = self
            .fetch("main")?
            .ok_or_else(|| Error::Git(format!("{} has no main", self.remote)))?;
        let fetched = match self.fetch(&name)? {
            Some(tip) => Some(self.read(&request.task, &main, tip)?),
            None => None,
        };
        let outcome = decide(request, fetched.as_ref(), &name)?;
        let lease = fetched.map(|b| b.tip);
        let base = lease.clone().unwrap_or(main);
        let (commit, push) = match &outcome {
            Outcome::AlreadyHeld(_) => (None, false),
            Outcome::Released(_) => (None, true),
            Outcome::Claimed(c) | Outcome::Renewed(c) | Outcome::TookOver { claim: c, .. } => {
                let tree = format!("{base}^{{tree}}");
                let commit = self.git(&["commit-tree", "--no-gpg-sign", &tree, "-p", &base, "-m", &c.message()])?;
                (Some(commit), true)
            }
        };
        Ok(Prepared { branch: name, lease, commit, push, outcome })
    }

    /// The second step of a write: pushes the commit, or deletes the branch,
    /// with a lease on the tip it was built from. `false` when the remote
    /// rejected it because the branch moved since the fetch.
    pub fn publish(&self, prepared: &Prepared) -> Result<bool, Error> {
        if !prepared.push {
            return Ok(true);
        }
        let target = format!("refs/heads/{}", prepared.branch);
        let lease = format!(
            "--force-with-lease={target}:{}",
            prepared.lease.as_deref().unwrap_or_default()
        );
        let refspec = format!("{}:{target}", prepared.commit.as_deref().unwrap_or_default());
        let output = self
            .command(&["push", "--porcelain", "--no-verify", &lease, &self.remote, &refspec])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            if stdout.lines().any(|l| l.starts_with('!') && l.contains("\t[rejected]")) {
                return Ok(false);
            }
            if let (None, Outcome::Released(claim)) = (&prepared.commit, &prepared.outcome) {
                return Err(Error::Git(format!(
                    "{} refused to delete {}, so the claim stays until it lapses at {}: {}",
                    self.remote,
                    prepared.branch,
                    utc(claim.expires_at),
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            return Err(Error::Git(format!(
                "git push {} {refspec} failed: {}",
                self.remote,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let tracking = format!("refs/remotes/{}/{}", self.remote, prepared.branch);
        match &prepared.commit {
            Some(commit) => self.git(&["update-ref", &tracking, commit])?,
            None => self.git(&["update-ref", "-d", &tracking])?,
        };
        Ok(true)
    }

    /// Fetches the remote's `branch` into its remote-tracking ref and returns
    /// the tip, or `None` when the remote has no such branch.
    fn fetch(&self, branch: &str) -> Result<Option<String>, Error> {
        let tracking = format!("refs/remotes/{}/{branch}", self.remote);
        let refspec = format!("+refs/heads/{branch}:{tracking}");
        let output = self
            .command(&[
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
                &self.remote,
                &refspec,
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("couldn't find remote ref") {
                return Ok(None);
            }
            return Err(Error::Git(format!(
                "git fetch {} {branch} failed: {}",
                self.remote,
                stderr.trim()
            )));
        }
        self.git(&["rev-parse", "--verify", &format!("{tracking}^{{commit}}")])
            .map(Some)
    }

    /// Reads the branch at `tip`: the newest claim on `task` among the
    /// commits `main` lacks, and whether it holds any other commit or change.
    fn read(&self, task: &str, main: &str, tip: String) -> Result<Branch, Error> {
        let log = self.git(&["log", "--topo-order", "--format=%B%x00", &tip, "--not", main])?;
        let mut holder = None;
        let mut work = false;
        for message in log.split('\0').map(str::trim).filter(|m| !m.is_empty()) {
            match Claim::parse(message) {
                Some(claim) if claim.task == task => {
                    holder.get_or_insert(claim);
                }
                Some(_) => {}
                None => work = true,
            }
        }
        if !work {
            let base = self.git(&["merge-base", main, &tip])?;
            work = !self.git(&["diff", "--name-only", &base, &tip])?.is_empty();
        }
        Ok(Branch { tip, holder, work })
    }

    /// Runs git in the repository and returns its standard output without
    /// the final newline.
    fn git(&self, args: &[&str]) -> Result<String, Error> {
        let output = self
            .command(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
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
    /// so they need no identity configured on the machine. `LC_ALL=C` keeps
    /// git's messages in English, since [`Store::fetch`] reads one.
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
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        command
    }
}

/// The task's branch as fetched: its tip, the newest claim on it, and
/// whether it holds anything besides claim commits.
#[derive(Debug, Clone)]
struct Branch {
    tip: String,
    holder: Option<Claim>,
    work: bool,
}

/// The branch that claims a task: `storm/` and the task ID with every byte
/// outside `A-Z a-z 0-9 - _` written as `%XX`. Distinct IDs get distinct
/// branches, and an arclightning ID or `issue-12` stays as it is.
pub fn branch(task: &str) -> Result<String, Error> {
    check("task", task)?;
    let mut name = String::from(PREFIX);
    for byte in task.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            name.push(byte as char);
        } else {
            name.push_str(&format!("%{byte:02X}"));
        }
    }
    if name.len() > PREFIX.len() + MAX_NAME {
        return Err(Error::TooLong(task.to_string()));
    }
    Ok(name)
}

/// The current time in UTC seconds.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// UTC seconds as `2026-10-27T12:00:00Z`.
pub fn utc(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Howard Hinnant's days-to-civil algorithm, for days since 1970-01-01.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Refuses an empty value, one holding a control character, which would break
/// the trailer it goes in, and one with whitespace at either end, which the
/// trailer would lose when it is read back.
fn check(what: &'static str, value: &str) -> Result<(), Error> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(Error::Invalid { what, value: value.to_string() });
    }
    Ok(())
}

/// What `request` does to the task's branch, `None` when it does not exist.
fn decide(request: &Request, fetched: Option<&Branch>, name: &str) -> Result<Outcome, Error> {
    let fresh = |ttl: u64| Claim {
        task: request.task.clone(),
        machine: request.machine.clone(),
        run: request.run.clone(),
        expires_at: request.now.saturating_add(ttl),
    };
    let not_held = || Error::NotHeld { task: request.task.clone(), run: request.run.clone() };
    let Some(fetched) = fetched else {
        return match request.action {
            Action::Take { ttl } => Ok(Outcome::Claimed(fresh(ttl))),
            Action::Renew { .. } | Action::Release => Err(not_held()),
        };
    };
    let Some(holder) = fetched.holder.clone() else {
        return Err(Error::NoClaim(name.to_string()));
    };
    let ours = holder.run == request.run;
    match request.action {
        Action::Take { .. } if ours && holder.is_live(request.now) => Ok(Outcome::AlreadyHeld(holder)),
        Action::Take { ttl } if ours => Ok(Outcome::Renewed(fresh(ttl))),
        Action::Take { .. } if holder.is_live(request.now) => Err(Error::Held(holder)),
        Action::Take { ttl } => Ok(Outcome::TookOver { claim: fresh(ttl), expired: holder }),
        Action::Renew { ttl } if ours => Ok(Outcome::Renewed(fresh(ttl))),
        Action::Release if ours && fetched.work => Err(Error::HoldsWork(name.to_string())),
        Action::Release if ours => Ok(Outcome::Released(holder)),
        Action::Renew { .. } | Action::Release => Err(not_held()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(run: &str, action: Action) -> Request {
        Request { task: "t".into(), machine: "m".into(), run: run.into(), action, now: 100 }
    }

    fn held(run: &str, expires_at: u64, work: bool) -> Branch {
        let holder = Claim { task: "t".into(), machine: "m".into(), run: run.into(), expires_at };
        Branch { tip: "abc".into(), holder: Some(holder), work }
    }

    #[test]
    fn branches_keep_plain_ids_and_escape_the_rest() {
        assert_eq!(branch("issue-12").unwrap(), "storm/issue-12");
        assert_eq!(branch("a/b c.d").unwrap(), "storm/a%2Fb%20c%2Ed");
        assert_ne!(branch("a/").unwrap(), branch("a%2F").unwrap());
        for bad in ["", " ", "a\nb", " t2", "t2 ", "t\u{a0}"] {
            assert!(matches!(branch(bad), Err(Error::Invalid { .. })), "{bad:?}");
        }
        assert!(matches!(branch(&"/".repeat(100)), Err(Error::TooLong(_))));
    }

    #[test]
    fn a_claim_round_trips_through_its_message() {
        let claim = Claim { task: "issue-12".into(), machine: "m".into(), run: "r 1".into(), expires_at: 3600 };
        let message = claim.message();
        assert!(message.starts_with("Claim issue-12 for run r 1 until 1970-01-01T01:00:00Z\n\n"));
        assert_eq!(Claim::parse(&message), Some(claim));
        assert_eq!(Claim::parse("Fix a bug\n\nFabrial-Claim: t\n"), None);
    }

    #[test]
    fn a_run_or_machine_with_surrounding_whitespace_is_refused() {
        for (machine, run) in [("m", " r"), ("m", "r "), (" m", "r")] {
            let request = Request { machine: machine.into(), run: run.into(), ..request("r", Action::Release) };
            let store = Store::new("/nonexistent", "origin");
            assert!(
                matches!(store.prepare(&request), Err(Error::Invalid { .. })),
                "{machine:?} {run:?}"
            );
        }
    }

    #[test]
    fn take_refuses_a_live_claim_and_builds_on_an_expired_one() {
        let take = Action::Take { ttl: 50 };
        assert!(matches!(
            decide(&request("b", take), Some(&held("a", 101, false)), "storm/t"),
            Err(Error::Held(_))
        ));
        let Ok(Outcome::TookOver { claim, expired }) = decide(&request("b", take), Some(&held("a", 100, true)), "")
        else {
            panic!("an expired claim is taken over");
        };
        assert_eq!(
            (claim.run.as_str(), claim.expires_at, expired.run.as_str()),
            ("b", 150, "a")
        );
        let unclaimed = Branch { tip: "abc".into(), holder: None, work: true };
        assert!(matches!(
            decide(&request("b", take), Some(&unclaimed), "storm/t"),
            Err(Error::NoClaim(_))
        ));
    }

    #[test]
    fn the_holding_run_renews_its_expired_claim_on_take() {
        let Ok(Outcome::Renewed(claim)) =
            decide(&request("a", Action::Take { ttl: 5 }), Some(&held("a", 100, true)), "")
        else {
            panic!("an expired claim of this run is renewed");
        };
        assert_eq!((claim.run.as_str(), claim.expires_at), ("a", 105));
    }

    #[test]
    fn the_holding_run_retakes_renews_and_releases_only_an_empty_branch() {
        let live = held("a", 101, false);
        assert!(matches!(
            decide(&request("a", Action::Take { ttl: 5 }), Some(&live), ""),
            Ok(Outcome::AlreadyHeld(Claim { expires_at: 101, .. }))
        ));
        assert!(matches!(
            decide(&request("a", Action::Renew { ttl: 5 }), Some(&live), ""),
            Ok(Outcome::Renewed(Claim { expires_at: 105, .. }))
        ));
        assert!(matches!(
            decide(&request("a", Action::Release), Some(&live), ""),
            Ok(Outcome::Released(_))
        ));
        assert!(matches!(
            decide(&request("a", Action::Release), Some(&held("a", 101, true)), ""),
            Err(Error::HoldsWork(_))
        ));
        for action in [Action::Renew { ttl: 5 }, Action::Release] {
            assert!(matches!(
                decide(&request("b", action), Some(&live), ""),
                Err(Error::NotHeld { .. })
            ));
            assert!(matches!(
                decide(&request("b", action), None, ""),
                Err(Error::NotHeld { .. })
            ));
        }
    }
}
