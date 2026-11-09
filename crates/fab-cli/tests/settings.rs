//! Runs `fab init` and `fab update` with their settings flags in a fresh
//! repository.
use std::fs;
use std::path::Path;
use std::process::Command;

/// Runs fab and returns its standard output.
fn fab(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).into_owned()
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
            "--app",
            "own-bot",
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
    let _ = fab(dir.path(), &["update", "--signoff", ""]);
    assert!(!settings().contains("signoff"), "{}", settings());
}

#[test]
fn assisted_by_is_set_by_init_and_changed_by_update() {
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
    fab(
        dir.path(),
        &[
            "init",
            "--owner",
            "o",
            "--app",
            "own-bot",
            "--trailer",
            "A <a@b>",
            "--gates",
            "true",
            "--assisted-by",
            "on",
        ],
    );
    assert!(
        settings().lines().any(|l| l == r#"assisted_by = "on""#),
        "{}",
        settings()
    );
    fab(dir.path(), &["update", "--assisted-by", "off"]);
    assert!(
        settings().lines().any(|l| l == r#"assisted_by = "off""#),
        "{}",
        settings()
    );
}

#[test]
fn update_says_which_secrets_to_add_when_the_workflows_rename_them() {
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
    let args = [
        "init",
        "--owner",
        "o",
        "--app",
        "own-bot",
        "--trailer",
        "A <a@b>",
        "--gates",
        "true",
    ];
    let _ = fab(dir.path(), &args);
    let review = dir.path().join(".github/workflows/review.yml");
    let text = fs::read_to_string(&review).unwrap();
    let old = text
        .replace("secrets.FABRIAL_APP_ID", "secrets.THUNDERBOT_APP_ID")
        .replace("secrets.FABRIAL_APP_PRIVATE_KEY", "secrets.THUNDERBOT_PRIVATE_KEY");
    fs::write(&review, old).unwrap();
    let out = fab(dir.path(), &["update"]);
    assert!(
        out.contains(
            "The workflows now read the secrets FABRIAL_APP_ID and FABRIAL_APP_PRIVATE_KEY \
             instead of THUNDERBOT_APP_ID and THUNDERBOT_PRIVATE_KEY."
        ),
        "{out}"
    );
    assert!(!fab(dir.path(), &["update"]).contains("secrets"));
}
