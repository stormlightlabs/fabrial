//! `fab`, the fabrial command-line interface.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
                s.owner = owner.unwrap_or(std::mem::take(&mut s.owner));
                s.trailer = trailer.unwrap_or(std::mem::take(&mut s.trailer));
                s.gates = gates.unwrap_or(std::mem::take(&mut s.gates));
            };
            install::update(dir, change, force).map(|r| written(&r, "updated"))
        }),
        Command::Check { repo } => in_repo(&repo, check),
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

fn check(dir: &Path) -> Result<ExitCode, fab_core::Error> {
    let drift = install::check(dir)?;
    if drift.is_empty() {
        println!("fabrial {} is installed with no drift.", fab_core::version());
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
        fab_core::version(),
        report.written.len(),
        report.unchanged.len()
    );
    ExitCode::SUCCESS
}
