//! Runs `fab manual` into a scratch directory and compares each page with
//! the committed reference pages of the docs site.
#![cfg(unix)]
use std::fs;
use std::path::Path;
use std::process::Command;

const REFERENCE: &str = "packages/website/src/content/docs/reference";

#[test]
fn the_committed_reference_pages_match_fab_manual() {
    let out = tempfile::tempdir().unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(["manual", "--out"])
        .arg(out.path())
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "fab manual failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(REFERENCE);
    let mut written: Vec<_> = fs::read_dir(out.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    written.sort();
    assert!(!written.is_empty(), "fab manual wrote no pages");
    let stale: Vec<String> = written
        .iter()
        .filter(|name| fs::read(out.path().join(name)).ok() != fs::read(committed.join(name)).ok())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    assert!(
        stale.is_empty(),
        "these reference pages differ from what fab manual writes: {}; run fab manual --out {REFERENCE}",
        stale.join(", ")
    );
}

#[test]
fn manual_is_hidden_from_help() {
    let help = Command::new(env!("CARGO_BIN_EXE_fab")).arg("--help").output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("init"), "{help}");
    assert!(!help.contains("manual"), "{help}");
}
