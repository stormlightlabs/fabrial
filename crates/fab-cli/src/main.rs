//! `fab`, the fabrial command-line interface.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod branch_pr;
#[cfg(unix)]
mod dispatch;
mod gate;
mod release;
mod run;
mod squash;

use clap::{Args, Parser, Subcommand};
use fab_core::install::{self, Report};
use fab_core::settings::Settings;

/// fabrial: a software factory that takes tasks to reviewed pull requests.
#[derive(Parser)]
#[command(name = "fab", version = fab_core::VERSION, long_version = fab_core::FULL_VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a fresh isolated Pi review worker (host tmux backend).
    #[cfg(unix)]
    Dispatch(dispatch::Options),
    /// Inspect, cancel, or explicitly wait for an independently supervised worker.
    #[cfg(unix)]
    Worker {
        #[command(subcommand)]
        operation: dispatch::operations::Operation,
    },
    /// Internal trusted tmux recorder.
    #[cfg(unix)]
    #[command(hide = true)]
    DispatchRecord { private: PathBuf },
    /// Install fabrial's workflows, gate, skills and agents into a repository
    /// that does not have them yet.
    Init {
        #[command(flatten)]
        repo: Repo,
        /// GitHub login of the person who approves pull requests.
        #[arg(long)]
        owner: String,
        /// The squash commit's Co-authored-by value, as "Name <email>".
        #[arg(long)]
        trailer: String,
        /// The command agents run before pushing.
        #[arg(long)]
        gates: String,
        /// Replace files fabrial did not write.
        #[arg(long)]
        force: bool,
    },
    /// Replace an installed copy's files with this build's, keeping its
    /// settings unless a flag changes one.
    Update {
        #[command(flatten)]
        repo: Repo,
        /// A new GitHub login for the person who approves pull requests.
        #[arg(long)]
        owner: Option<String>,
        /// A new Co-authored-by value for the squash commit.
        #[arg(long)]
        trailer: Option<String>,
        /// A new command for agents to run before pushing.
        #[arg(long)]
        gates: Option<String>,
        /// Install this build's files over a newer release's.
        #[arg(long)]
        force: bool,
    },
    /// Report where an installed copy differs from this build; exits 1 when
    /// there is any drift.
    Check {
        #[command(flatten)]
        repo: Repo,
    },
    /// Set the storm/review check for a pull request event; the Review gate
    /// workflow runs it from a checkout of main.
    Gate(gate::Options),
    /// Open a draft pull request for an agent's work branch; the Branch PR
    /// workflow runs it.
    BranchPr(branch_pr::Options),
    /// fabrial's own release steps, which its release workflows run.
    Release {
        #[command(subcommand)]
        step: release::Command,
    },
}

#[derive(Args)]
struct Repo {
    /// The repository root.
    #[arg(long, default_value = ".")]
    dir: PathBuf,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Init { repo, owner, trailer, gates, force } => in_repo(&repo, |dir| {
            let settings = Settings { owner, trailer, gates, version: None };
            install::install(dir, &settings, force).map(|r| written(&r, "installed"))
        }),
        Command::Update { repo, owner, trailer, gates, force } => in_repo(&repo, |dir| {
            let change = |s: &mut Settings| {
                if let Some(owner) = owner {
                    s.owner = owner;
                }
                if let Some(trailer) = trailer {
                    s.trailer = trailer;
                }
                if let Some(gates) = gates {
                    s.gates = gates;
                }
            };
            install::update(dir, change, force).map(|r| written(&r, "updated"))
        }),
        Command::Check { repo } => in_repo(&repo, check),
        Command::Gate(options) => return in_workflow(gate::run(options)),
        Command::BranchPr(options) => return in_workflow(branch_pr::run(options)),
        Command::Release { step } => return in_workflow(step.run()),
        #[cfg(unix)]
        Command::Dispatch(options) => return ExitCode::from(dispatch::run(options)),
        #[cfg(unix)]
        Command::Worker { operation } => return ExitCode::from(operation.run()),
        #[cfg(unix)]
        Command::DispatchRecord { private } => return ExitCode::from(dispatch::record(&private)),
    };
    result.unwrap_or_else(|e| {
        eprintln!("fab: {e}");
        ExitCode::from(2)
    })
}

/// Runs `run` on the repository root `repo` names, once it is known to be one.
fn in_repo(
    repo: &Repo, run: impl FnOnce(&Path) -> Result<ExitCode, fab_core::Error>,
) -> Result<ExitCode, fab_core::Error> {
    install::require_repo_root(&repo.dir)?;
    run(&repo.dir)
}

/// The exit code for a command a workflow runs. A failure is printed to
/// standard error, as an error annotation when it runs in GitHub Actions, so
/// a step that reads standard output does not swallow it.
fn in_workflow(result: run::Result<()>) -> ExitCode {
    let Err(e) = result else {
        return ExitCode::SUCCESS;
    };
    if std::env::var_os("GITHUB_ACTIONS").is_some_and(|v| v == "true") {
        // An annotation is one line; GitHub reads `%0A` as a line break.
        let e = e.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A");
        eprintln!("::error title=fab::{e}");
    } else {
        eprintln!("fab: {e}");
    }
    ExitCode::from(2)
}

fn check(dir: &Path) -> Result<ExitCode, fab_core::Error> {
    let drift = install::check(dir)?;
    if drift.is_empty() {
        println!("fabrial {} is installed with no drift.", fab_core::VERSION);
        return Ok(ExitCode::SUCCESS);
    }
    for d in &drift {
        println!("{d}");
    }
    Ok(ExitCode::from(1))
}

fn written(report: &Report, done: &str) -> ExitCode {
    for path in &report.written {
        println!("wrote {}", path.display());
    }
    println!(
        "fabrial {} {done}: {} written, {} unchanged.",
        fab_core::VERSION,
        report.written.len(),
        report.unchanged.len()
    );
    ExitCode::SUCCESS
}
