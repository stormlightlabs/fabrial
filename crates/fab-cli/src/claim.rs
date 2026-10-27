//! `fab claim`: take, renew, release and list task claims on
//! `refs/fabrial/claims`, over [`fab_core::claims`].

use std::process::{Command, ExitCode};

use clap::{Args, Subcommand};
use fab_core::claims::{self, Action, Claim, Error, Outcome, Request, Store};
use serde_json::json;

/// The environment variable that sets the clock, in UTC seconds, so tests can
/// expire a claim without waiting.
pub const NOW_VAR: &str = "FAB_CLAIM_NOW";

/// What `fab claim` does.
#[derive(Subcommand)]
pub enum Operation {
    /// Claim a task for a run. Exits 0 when the run holds the claim, a new one
    /// or one it already held, and 1 when another run holds it.
    Take {
        #[command(flatten)]
        claim: Target,
        #[command(flatten)]
        ttl: Ttl,
    },
    /// Extend a run's claim. Exits 1 when the run does not hold it.
    Renew {
        #[command(flatten)]
        claim: Target,
        #[command(flatten)]
        ttl: Ttl,
    },
    /// Remove a run's claim. Exits 1 when the run does not hold it.
    Release {
        #[command(flatten)]
        claim: Target,
    },
    /// Print every claim, marking expired ones.
    List {
        #[command(flatten)]
        remote: Remote,
        /// Print a JSON array instead of one line per claim.
        #[arg(long)]
        json: bool,
    },
}

/// The task and run a write is for.
#[derive(Args)]
pub struct Target {
    /// The task ID, as the tracker or person gives it.
    task: String,
    /// The run that holds or takes the claim.
    #[arg(long)]
    run: String,
    /// The machine the run is on; the hostname by default.
    #[arg(long)]
    machine: Option<String>,
    #[command(flatten)]
    remote: Remote,
}

/// How long a claim lasts.
#[derive(Args)]
pub struct Ttl {
    /// Minutes until the claim expires.
    #[arg(long, default_value_t = claims::DEFAULT_TTL / 60)]
    ttl: u64,
}

/// The remote whose claims to read or write.
#[derive(Args)]
pub struct Remote {
    /// The remote that holds refs/fabrial/claims.
    #[arg(long, default_value = "origin")]
    remote: String,
}

impl Operation {
    /// Runs the operation in the current repository. Exits 1 when a claim
    /// refuses the write and 2 on any other failure.
    pub fn run(self) -> ExitCode {
        let now = match std::env::var(NOW_VAR) {
            Ok(v) => match v.parse() {
                Ok(n) => n,
                Err(_) => return fail(&format!("{NOW_VAR}={v} is not a number of seconds")),
            },
            Err(_) => claims::now(),
        };
        let (target, action) = match self {
            Operation::List { remote, json } => return list(&remote.remote, json, now),
            Operation::Take { claim, ttl } => (claim, Action::Take { ttl: ttl.ttl.saturating_mul(60) }),
            Operation::Renew { claim, ttl } => (claim, Action::Renew { ttl: ttl.ttl.saturating_mul(60) }),
            Operation::Release { claim } => (claim, Action::Release),
        };
        let machine = match target.machine.map_or_else(hostname, Ok) {
            Ok(m) => m,
            Err(e) => return fail(&e),
        };
        let request = Request { task: target.task, machine, run: target.run, action, now };
        match Store::new(".", target.remote.remote).write(&request) {
            Ok(outcome) => {
                println!("{}", done(&outcome));
                ExitCode::SUCCESS
            }
            Err(Error::Held(c)) => {
                eprintln!(
                    "fab: run {} on {} holds {} until {}",
                    c.run,
                    c.machine,
                    c.task,
                    time(c.expires_at)
                );
                ExitCode::from(1)
            }
            Err(e @ Error::NotHeld { .. }) => {
                eprintln!("fab: {e}");
                ExitCode::from(1)
            }
            Err(e) => fail(&e.to_string()),
        }
    }
}

/// Prints the claims, one line each or as JSON.
fn list(remote: &str, as_json: bool, now: u64) -> ExitCode {
    let claims = match Store::new(".", remote).list() {
        Ok(c) => c,
        Err(e) => return fail(&e.to_string()),
    };
    if as_json {
        let rows: Vec<_> = claims
            .iter()
            .map(|c| {
                json!({"task": c.task, "machine": c.machine, "run": c.run, "claimed_at": c.claimed_at,
                       "expires_at": c.expires_at, "expired": !c.is_live(now)})
            })
            .collect();
        println!("{}", serde_json::Value::Array(rows));
        return ExitCode::SUCCESS;
    }
    for c in &claims {
        let state = if c.is_live(now) { "until" } else { "expired" };
        println!(
            "{}\trun {} on {}\t{state} {}",
            c.task,
            c.run,
            c.machine,
            time(c.expires_at)
        );
    }
    ExitCode::SUCCESS
}

/// The line a successful write prints.
fn done(outcome: &Outcome) -> String {
    let until = |c: &Claim| {
        format!(
            "{} for run {} on {} until {}",
            c.task,
            c.run,
            c.machine,
            time(c.expires_at)
        )
    };
    match outcome {
        Outcome::Claimed(c) => format!("claimed {}", until(c)),
        Outcome::AlreadyHeld(c) => format!("already held; renewed {}", until(c)),
        Outcome::TookOver { claim, expired } => {
            format!("claimed {}; run {}'s claim had expired", until(claim), expired.run)
        }
        Outcome::Renewed(c) => format!("renewed {}", until(c)),
        Outcome::Released(c) => format!("released {} for run {}", c.task, c.run),
    }
}

/// This machine's name, from `hostname`.
fn hostname() -> Result<String, String> {
    let output = Command::new("hostname")
        .output()
        .map_err(|e| format!("cannot run hostname: {e}; pass --machine"))?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || name.is_empty() {
        return Err("hostname gave no name; pass --machine".into());
    }
    Ok(name)
}

/// UTC seconds as `2026-10-27T12:00:00Z`.
fn time(seconds: u64) -> String {
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

fn fail(message: &str) -> ExitCode {
    eprintln!("fab: {message}");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::time;

    #[test]
    fn times_print_as_utc() {
        assert_eq!(time(0), "1970-01-01T00:00:00Z");
        assert_eq!(time(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(time(1_793_102_445), "2026-10-27T12:00:45Z");
    }
}
