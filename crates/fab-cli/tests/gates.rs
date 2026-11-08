//! Runs `fab gates` in scratch repositories whose `origin/main` is the first
//! commit, with gate commands that print their name.
#![cfg(unix)]
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const GATES: &str = r#"full = "echo ran-full"

[[scoped]]
name = "rust"
paths = ["crates/**"]
run = "echo ran-rust"

[[scoped]]
name = "pi"
paths = ["packages/**"]
run = "echo ran-pi; exit 7"

[[scoped]]
name = "late"
paths = ["packages/**"]
run = "echo ran-late"
"#;

/// A repository with `files` committed and `origin/main` at that commit.
fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    write(dir.path(), "README.md", "start\n");
    for (path, text) in files {
        write(dir.path(), path, text);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "start"]);
    git(dir.path(), &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    dir
}

fn write(dir: &Path, path: &str, text: &str) {
    let full = dir.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `fab gates` at the repository root and returns its output.
fn gates(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fab"))
        .arg("gates")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn text(output: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_scoped_change_runs_its_scope_and_says_why() {
    let dir = repo(&[(".github/fabrial-gates.toml", GATES)]);
    // A commit, and an untracked file, both under crates/.
    write(dir.path(), "crates/a/src/lib.rs", "x\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "code"]);
    write(dir.path(), "crates/a/src/new.rs", "y\n");
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert!(output.status.success(), "{err}");
    assert_eq!(out, "ran-rust\n");
    assert!(
        err.contains("fab gates: rust (crates/a/src/lib.rs and 1 more changed): echo ran-rust"),
        "{err}"
    );
}

#[test]
fn a_failing_scope_stops_the_run_with_its_exit_code() {
    let dir = repo(&[(".github/fabrial-gates.toml", GATES)]);
    // Staged and unstaged changes count.
    write(dir.path(), "packages/pi/x.mjs", "x\n");
    git(dir.path(), &["add", "-A"]);
    write(dir.path(), "crates/a.rs", "y\n");
    git(dir.path(), &["add", "-A"]);
    write(dir.path(), "crates/a.rs", "z\n");
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert_eq!(output.status.code(), Some(7), "{err}");
    assert_eq!(out, "ran-rust\nran-pi\n");
    assert!(err.contains("fab gates: pi failed with exit code 7"), "{err}");
}

#[test]
fn an_unmatched_change_runs_full() {
    let dir = repo(&[(".github/fabrial-gates.toml", GATES)]);
    write(dir.path(), "crates/a.rs", "x\n");
    write(dir.path(), "justfile", "check:\n");
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert!(output.status.success(), "{err}");
    assert_eq!(out, "ran-full\n");
    assert!(err.contains("full (justfile matches no scope)"), "{err}");

    let skip = GATES.replacen("\n[[scoped]]", "unmatched = \"skip\"\n\n[[scoped]]", 1);
    let dir = repo(&[(".github/fabrial-gates.toml", &skip)]);
    write(dir.path(), "crates/a.rs", "x\n");
    write(dir.path(), "justfile", "check:\n");
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert!(output.status.success(), "{err}");
    assert_eq!(out, "ran-rust\n");
    assert!(err.contains("skipping justfile, which no scope names"), "{err}");
}

#[test]
fn untracked_files_that_trigger_full_are_named_and_symlinks_do_not_count() {
    let dir = repo(&[(".github/fabrial-gates.toml", GATES)]);
    write(dir.path(), "crates/a.rs", "x\n");
    std::os::unix::fs::symlink("/nix/store/x", dir.path().join("result")).unwrap();
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert!(output.status.success(), "{err}");
    assert_eq!(out, "ran-rust\n", "{err}");

    write(dir.path(), "out/build.o", "x\n");
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert_eq!(out, "ran-full\n", "{err}");
    assert!(err.contains("untracked out/build.o matches no scope"), "{err}");
}

#[test]
fn a_change_to_the_gates_file_runs_the_bases_full() {
    let failing = GATES.replace("echo ran-full", "echo ran-base-full; exit 3");
    let dir = repo(&[(".github/fabrial-gates.toml", &failing)]);
    let loosened = failing.replace("echo ran-base-full; exit 3", "true");
    write(dir.path(), ".github/fabrial-gates.toml", &loosened);
    write(dir.path(), "crates/a.rs", "x\n");
    for args in [&[][..], &["--full"]] {
        let output = gates(dir.path(), args);
        let (out, err) = text(&output);
        assert_eq!(output.status.code(), Some(3), "{args:?}: {err}");
        assert_eq!(out, "ran-base-full\n", "{args:?}");
    }
    let (_, err) = text(&gates(dir.path(), &[]));
    assert!(
        err.contains(".github/fabrial-gates.toml changed, so `full` from the base"),
        "{err}"
    );
}

#[test]
fn a_docs_only_change_runs_nothing_and_full_runs_full() {
    let dir = repo(&[(".github/fabrial-gates.toml", GATES)]);
    write(dir.path(), "README.md", "changed\n");
    write(dir.path(), "docs/guide.md", "new\n");
    write(dir.path(), ".arcl/workspace/tasks/t.md", "task\n");
    let output = gates(dir.path(), &[]);
    let (out, err) = text(&output);
    assert!(output.status.success(), "{err}");
    assert_eq!(out, "");
    assert!(err.contains("only documentation and planning records changed"), "{err}");

    let output = gates(dir.path(), &["--full"]);
    let (out, err) = text(&output);
    assert!(output.status.success(), "{err}");
    assert_eq!(out, "ran-full\n");
    assert!(err.contains("full (--full): echo ran-full"), "{err}");
}

#[test]
fn without_the_file_the_settings_gates_command_runs() {
    let settings = "owner = \"o\"\ntrailer = \"A <a@b>\"\ngates = \"echo ran-settings\"\n";
    let dir = repo(&[(".github/fabrial.toml", settings)]);
    for args in [&[][..], &["--full"]] {
        let output = gates(dir.path(), args);
        let (out, err) = text(&output);
        assert!(output.status.success(), "{err}");
        assert_eq!(out, "ran-settings\n", "{args:?}");
        assert!(err.contains("has no .github/fabrial-gates.toml"), "{err}");
    }

    // A gates file the change adds waits for the base to hold it.
    write(dir.path(), ".github/fabrial-gates.toml", GATES);
    let (out, err) = text(&gates(dir.path(), &[]));
    assert_eq!(out, "ran-settings\n", "{err}");
    assert!(err.contains("has no .github/fabrial-gates.toml"), "{err}");

    // A base with neither file, as when installing fabrial, uses the
    // working tree's settings.
    let fresh = repo(&[]);
    write(fresh.path(), ".github/fabrial.toml", settings);
    let (out, err) = text(&gates(fresh.path(), &[]));
    assert_eq!(out, "ran-settings\n", "{err}");
    assert!(err.contains("has neither file"), "{err}");

    let bare = repo(&[]);
    let output = gates(bare.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).1.contains("are both missing"), "{:?}", text(&output));
}
