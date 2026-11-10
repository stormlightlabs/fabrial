//! Runs `fab manual` into a scratch directory and compares its pages with
//! the committed reference pages of the docs site, which hold nothing else.
#![cfg(unix)]
use std::ffi::OsString;
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
    let extra = extra_pages(&committed, &written);
    assert!(
        extra.is_empty(),
        "fab manual writes no page named {}; delete it or rename it, then run fab manual --out {REFERENCE}",
        extra.join(", ")
    );
}

/// The pages under `dir`, at any depth, that are not among the `written`
/// pages, as paths relative to `dir`, sorted. A page is any `.md` or `.svx`
/// file, the files the docs site loads.
fn extra_pages(dir: &Path, written: &[OsString]) -> Vec<String> {
    let mut extra = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(next) = dirs.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let page = path.extension().is_some_and(|e| e == "md" || e == "svx");
            let relative = path.strip_prefix(dir).unwrap();
            if page && !written.iter().any(|w| Path::new(w) == relative) {
                extra.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    extra.sort();
    extra
}

#[test]
fn a_committed_page_fab_manual_does_not_write_is_named() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("old/deeper")).unwrap();
    for name in [
        "settings.md",
        "old-name.md",
        "old.svx",
        "notes.txt",
        "old/x.md",
        "old/deeper/y.svx",
        "old/settings.md",
    ] {
        fs::write(dir.path().join(name), "").unwrap();
    }
    assert_eq!(
        extra_pages(dir.path(), &["settings.md".into()]),
        [
            "old-name.md",
            "old.svx",
            "old/deeper/y.svx",
            "old/settings.md",
            "old/x.md"
        ]
    );
}

#[test]
fn manual_is_hidden_from_help() {
    let help = Command::new(env!("CARGO_BIN_EXE_fab")).arg("--help").output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("init"), "{help}");
    assert!(!help.contains("manual"), "{help}");
}
