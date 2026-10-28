//! Checks the session setup against PLAN.md and the hosts it runs on.
use std::fs;
use std::path::Path;
use std::process::Command;

/// The arcl version the session-start hook pins and the one PLAN.md's
/// install line names agree, so a cloud session and a person following the
/// plan run the same arcl. PLAN.md holds one install line, so an older one
/// left beside it fails too.
#[test]
fn the_session_hook_and_the_plan_pin_the_same_arcl() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let hook = fs::read_to_string(root.join(".claude/hooks/session-start.sh")).unwrap();
    let plan = fs::read_to_string(root.join("PLAN.md")).unwrap();
    let pinned = hook
        .lines()
        .find_map(|l| l.strip_prefix("version="))
        .expect("the hook sets version=");
    let installs: Vec<&str> = plan.lines().filter(|l| l.contains("/arcl-cli-installer.sh")).collect();
    let install = format!("arclightning/releases/download/v{pinned}/arcl-cli-installer.sh");
    assert_eq!(installs.len(), 1, "PLAN.md has one arcl install line: {installs:?}");
    assert!(installs[0].contains(&install), "PLAN.md does not install arcl {pinned}");
}

/// No tracked file names `/bin/bash` in a shebang, since NixOS has none.
/// CI runs where it exists, so only this check catches one coming back.
#[test]
fn no_tracked_file_runs_bin_bash() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let shebang = concat!("#!", "/bin/bash");
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success(), "git ls-files failed");
    let files = String::from_utf8(out.stdout).unwrap();
    let found: Vec<&str> = files
        .split_terminator('\0')
        .filter(|f| fs::read_to_string(root.join(f)).is_ok_and(|t| t.contains(shebang)))
        .collect();
    assert!(found.is_empty(), "use #!/usr/bin/env bash in {found:?}");
}
