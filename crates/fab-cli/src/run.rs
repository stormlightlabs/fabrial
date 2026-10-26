//! Running `gh` and `git` for the commands workflows call: the Review gate,
//! Branch PR and the release checks.

use std::path::Path;
use std::process::Command;

/// A failure, as the message `fab` prints.
pub type Result<T> = std::result::Result<T, String>;

/// Runs `gh` and returns its standard output without the final newline.
pub fn gh(args: &[&str]) -> Result<String> {
    output("gh", Command::new("gh").args(args), args)
}

/// Runs `git` in `dir` and returns its standard output without the final
/// newline.
pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    output("git", Command::new("git").arg("-C").arg(dir).args(args), args)
}

/// Prints a write to GitHub instead of sending it, with the text of any
/// `--body-file` it names, for `DRY_RUN`.
pub fn print_write(args: &[&str]) {
    eprintln!(
        "DRY_RUN gh {}",
        args.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(" ")
    );
    for pair in args.windows(2) {
        if pair[0] == "--body-file" {
            eprintln!(
                "DRY_RUN body: {:?}",
                std::fs::read_to_string(pair[1]).unwrap_or_default()
            );
        }
    }
}

/// Runs a command and returns its standard output. A failure names the
/// program and its first four arguments, with its standard error.
fn output(program: &str, command: &mut Command, args: &[&str]) -> Result<String> {
    let output = command.output().map_err(|e| format!("cannot run {program}: {e}"))?;
    if !output.status.success() {
        let what = args.iter().take(4).copied().collect::<Vec<_>>().join(" ");
        return Err(format!(
            "{program} {what} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_string())
}
