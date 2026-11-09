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
fn secret_prefix_defaults_on_init_is_set_by_its_flag_and_kept_by_update() {
    let line = |dir: &Path| {
        let text = fs::read_to_string(dir.join(".github/fabrial.toml")).unwrap();
        text.lines()
            .find(|l| l.starts_with("secret_prefix = "))
            .map(str::to_owned)
    };
    let review = |dir: &Path| fs::read_to_string(dir.join(".github/workflows/review.yml")).unwrap();
    let init = [
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
    for (flag, prefix) in [(None, "FABRIAL"), (Some("OWN_BOT"), "OWN_BOT")] {
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
        let mut args = init.to_vec();
        args.extend(flag.iter().flat_map(|f| ["--secret-prefix", *f]));
        let _ = fab(dir.path(), &args);
        assert_eq!(line(dir.path()), Some(format!("secret_prefix = \"{prefix}\"")));
        let text = review(dir.path());
        assert!(text.contains(&format!("${{{{ secrets.{prefix}_APP_ID }}}}")), "{text}");
        assert!(
            text.contains(&format!("${{{{ secrets.{prefix}_PRIVATE_KEY }}}}")),
            "{text}"
        );
        let _ = fab(dir.path(), &["update"]);
        assert_eq!(line(dir.path()), Some(format!("secret_prefix = \"{prefix}\"")));
    }
}

#[test]
fn update_gives_an_install_without_a_prefix_the_secrets_it_read() {
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
    let path = dir.path().join(".github/fabrial.toml");
    let old: String = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("secret_prefix = "))
        .map(|l| format!("{l}\n"))
        .collect();
    fs::write(&path, old).unwrap();
    let _ = fab(dir.path(), &["update"]);
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("secret_prefix = \"THUNDERBOT\"\n")
    );
    let review = fs::read_to_string(dir.path().join(".github/workflows/review.yml")).unwrap();
    assert!(review.contains("${{ secrets.THUNDERBOT_APP_ID }}"), "{review}");
    assert!(review.contains("${{ secrets.THUNDERBOT_PRIVATE_KEY }}"), "{review}");
    let _ = fab(dir.path(), &["check"]);
}
