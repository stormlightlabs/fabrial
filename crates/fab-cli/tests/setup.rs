//! Checks the cloud session setup against PLAN.md.
use std::fs;
use std::path::Path;

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
