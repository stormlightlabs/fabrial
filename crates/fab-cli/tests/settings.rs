//! Runs `fab init` and `fab update` with `--signoff` in a fresh repository.
use std::fs;
use std::path::Path;
use std::process::Command;

fn fab(dir: &Path, args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(args)
        .arg("--dir")
        .arg(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fab {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn signoff_is_set_by_init_kept_by_update_and_removed_by_an_empty_flag() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let settings = || fs::read_to_string(dir.path().join(".github/fabrial.toml")).unwrap();
    let line = r#"signoff = "A <a@b>""#;
    fab(
        dir.path(),
        &[
            "init",
            "--owner",
            "o",
            "--trailer",
            "A <a@b>",
            "--gates",
            "true",
            "--signoff",
            "A <a@b>",
        ],
    );
    assert!(settings().lines().any(|l| l == line), "{}", settings());
    fab(dir.path(), &["update"]);
    assert!(settings().lines().any(|l| l == line), "{}", settings());
    fab(dir.path(), &["update", "--signoff", ""]);
    assert!(!settings().contains("signoff"), "{}", settings());
}
