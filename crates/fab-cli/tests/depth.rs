//! Runs `fab review-depth` in a fresh repository, with the planning
//! directories its base's settings name.
use std::fs;
use std::path::Path;
use std::process::Command;

const SETTINGS: &str = "owner = \"o\"\ntrailer = \"O <o@example.com>\"\ngates = \"true\"\n";

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
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

fn commit(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", path]);
}

/// The depth `fab review-depth` prints for a branch that adds a planning
/// record on a base whose settings are `settings`.
fn depth(settings: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, ".github/fabrial.toml", settings);
    git(dir, &["switch", "-q", "-c", "work"]);
    commit(dir, "plans/tasks/t-1.md", "# A task\n");
    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(["review-depth", "--base", "main", "--head", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn planning_records_take_the_standard_pass_only_where_the_base_names_them() {
    assert_eq!(depth(&format!("{SETTINGS}planning = [\"plans/\"]\n")), "standard");
    assert_eq!(depth(SETTINGS), "full");
    assert_eq!(depth(&format!("{SETTINGS}planning = [\"other/\"]\n")), "full");
    // Settings that do not parse name no planning directory.
    assert_eq!(depth(&format!("{SETTINGS}planning = [\"plans\"]\n")), "full");
}
