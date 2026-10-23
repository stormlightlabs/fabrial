//! `fab`, the fabrial command-line interface.

use std::process::ExitCode;

const USAGE: &str = "usage: fab [--version | --help]";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("fab {}", fab_core::VERSION);
            ExitCode::SUCCESS
        }
        None | Some("--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("fab: unknown argument {other:?}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
