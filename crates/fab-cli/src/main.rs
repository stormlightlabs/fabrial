//! `fab`, the fabrial command-line interface.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fab_core::install;
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
    /// Install fabrial's workflows, gate, skills and agents into a repository,
    /// or report where an installed copy has drifted.
    Init {
        /// The repository root.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Report drift instead of writing; exits 1 when there is any.
        #[arg(long)]
        check: bool,
        /// GitHub login of the person who approves pull requests.
        #[arg(long)]
        owner: Option<String>,
        /// The squash commit's Co-authored-by value, as "Name <email>".
        #[arg(long)]
        trailer: Option<String>,
        /// The command agents run before pushing.
        #[arg(long)]
        gates: Option<String>,
        /// On a first install, replace files fabrial did not write.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Init { dir, check, owner, trailer, gates, force } => {
            let result = install::require_repo_root(&dir).and_then(|()| {
                if check { run_check(&dir) } else { run_init(&dir, owner, trailer, gates, force) }
            });
            result.unwrap_or_else(|e| {
                eprintln!("fab: {e}");
                ExitCode::from(2)
            })
        }
    }
}

fn run_check(dir: &std::path::Path) -> Result<ExitCode, fab_core::Error> {
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

fn run_init(
    dir: &std::path::Path, owner: Option<String>, trailer: Option<String>, gates: Option<String>, force: bool,
) -> Result<ExitCode, fab_core::Error> {
    let existing = install::read_settings(dir)?;
    let pick = |flag: Option<String>, old: Option<&String>, name: &str| {
        flag.or_else(|| old.cloned())
            .ok_or_else(|| fab_core::Error::Settings(format!("no `{name}` yet; pass --{name} on the first install")))
    };
    let settings = Settings {
        owner: pick(owner, existing.as_ref().map(|s| &s.owner), "owner")?,
        trailer: pick(trailer, existing.as_ref().map(|s| &s.trailer), "trailer")?,
        gates: pick(gates, existing.as_ref().map(|s| &s.gates), "gates")?,
        version: None,
    };
    let report = install::init(dir, &settings, force)?;
    for path in &report.written {
        println!("wrote {}", path.display());
    }
    println!(
        "fabrial {} installed: {} written, {} unchanged.",
        fab_core::version(),
        report.written.len(),
        report.unchanged.len()
    );
    Ok(ExitCode::SUCCESS)
}
