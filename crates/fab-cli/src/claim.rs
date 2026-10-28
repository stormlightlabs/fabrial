//! `fab claim`: take, renew and release a task's claim, its `storm/<task>`
//! branch, over [`fab_core::claims`].

use std::process::{Command, ExitCode};

use clap::{Args, Subcommand};
use fab_core::claims::{self, Action, Claim, Error, Outcome, Request, Store, utc};

/// The environment variable that sets the clock, in UTC seconds, so tests can
/// expire a claim without waiting.
pub const NOW_VAR: &str = "FAB_CLAIM_NOW";

/// What `fab claim` does.
#[derive(Subcommand)]
pub enum Operation {
    /// Claim a task for a run by creating its branch. Exits 0 when the run
    /// holds the claim, a new one or one it already held, and 1 when another
    /// run holds it.
    Take {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        ttl: Ttl,
    },
    /// Extend a run's claim. Exits 1 when the run does not hold it.
    Renew {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        ttl: Ttl,
    },
    /// Delete the branch of a run's claim when it holds nothing else. Exits 1
    /// when the run does not hold it or the branch holds work.
    Release {
        #[command(flatten)]
        target: Target,
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
    /// The remote that holds the task's branch.
    #[arg(long, default_value = "origin")]
    remote: String,
}

/// How long a claim lasts.
#[derive(Args)]
pub struct Ttl {
    /// Minutes until the claim expires, at least 1.
    #[arg(long, default_value_t = claims::DEFAULT_TTL / 60, value_parser = clap::value_parser!(u64).range(1..))]
    ttl: u64,
}

impl Operation {
    /// Runs the operation in the current repository. Exits 1 when the claim
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
            Operation::Take { target, ttl } => (target, Action::Take { ttl: ttl.ttl.saturating_mul(60) }),
            Operation::Renew { target, ttl } => (target, Action::Renew { ttl: ttl.ttl.saturating_mul(60) }),
            Operation::Release { target } => (target, Action::Release),
        };
        let machine = match target.machine.map_or_else(hostname, Ok) {
            Ok(m) => m,
            Err(e) => return fail(&e),
        };
        let request = Request { task: target.task, machine, run: target.run, action, now };
        let branch = match claims::branch(&request.task) {
            Ok(b) => b,
            Err(e) => return fail(&e.to_string()),
        };
        match Store::new(".", target.remote).write(&request) {
            Ok(outcome) => {
                println!("{}", done(&outcome, &branch));
                ExitCode::SUCCESS
            }
            Err(Error::Held(c)) => {
                eprintln!("fab: {branch}: {}", Error::Held(c));
                ExitCode::from(1)
            }
            Err(e @ (Error::NotHeld { .. } | Error::NoClaim(_) | Error::HoldsWork(_))) => {
                eprintln!("fab: {e}");
                ExitCode::from(1)
            }
            Err(e) => fail(&e.to_string()),
        }
    }
}

/// The line a successful write prints.
fn done(outcome: &Outcome, branch: &str) -> String {
    let until = |c: &Claim| {
        format!(
            "{branch} for run {} on {} until {}",
            c.run,
            c.machine,
            utc(c.expires_at)
        )
    };
    match outcome {
        Outcome::Claimed(c) => format!("claimed {}", until(c)),
        Outcome::AlreadyHeld(c) => format!("already held: {}", until(c)),
        Outcome::TookOver { claim, expired } => {
            format!("claimed {}; run {}'s claim had expired", until(claim), expired.run)
        }
        Outcome::Renewed(c) => format!("renewed {}", until(c)),
        Outcome::Released(c) => format!("released {branch} for run {}; the branch is deleted", c.run),
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

fn fail(message: &str) -> ExitCode {
    eprintln!("fab: {message}");
    ExitCode::from(2)
}
