//! Exercise the actual recorder binary and host tools over subprocess pipes.
#![cfg(unix)]
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn script(path: &Path, text: &str) {
    fs::write(path, format!("#!/bin/bash\nset -eu\n{text}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

struct Fixture {
    temp: tempfile::TempDir,
    private: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new(mode: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let temp = tempfile::tempdir_in(root).unwrap();
        let p = temp.path().canonicalize().unwrap();
        let private = p.join("private");
        let bin = p.join("bin");
        fs::create_dir(&private).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::create_dir(private.join("input")).unwrap();
        fs::write(private.join("input/task.txt"), "Review pinned head").unwrap();
        fs::write(private.join("input/role.md"), "Trusted instructions").unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        fs::write(p.join("auth.json"), json!({"openai": {"type": "oauth", "access": "a-secret", "refresh": "r-secret", "expires": (now+4000)*1000}, "other": {"key": "owner-secret"}}).to_string()).unwrap();
        let config = json!({"sandbox": bin.join("sandbox"), "pi_runtime": p.join("runtime"), "auth_file": p.join("auth.json"), "state_dir": p,
            "repository": "org/repo", "keychain_service": "local-service", "keychain_account": "local-account", "app_id_secret": "id-one", "installation_id_secret": "id-two", "pem_secret": "id-three"});
        fs::write(private.join("request.json"), json!({"config": config, "role": if mode == "reviser" { "reviser" } else { "reviewer" }, "worktree": p, "timeout": if mode == "deadline" {1} else if mode == "worker-timeout" {5} else {15}, "model": "openai/gpt-6-sol", "thinking": "low", "tools": ["bash", "read"], "skills": []}).to_string()).unwrap();
        script(
            &bin.join("security"),
            "test \"$*\" = 'find-generic-password -s local-service -a local-account -w'\nprintf 'machine-secret\\n'",
        );
        script(
            &bin.join("bws"),
            &format!(
                "test \"$BWS_ACCESS_TOKEN\" = 'machine-secret'\nprintf '%s\\n' \"$3\" >> {}\ncase \"$3\" in id-one) printf '{{\"value\":\"123\"}}';; id-two) printf '{{\"value\":\"456\"}}';; id-three) printf '{{\"value\":\"pem-secret\"}}';; *) exit 9;; esac",
                quote(&p.join("secret-ids"))
            ),
        );
        script(
            &bin.join("openssl"),
            "test \"$*\" != *pem-secret*\ncat >/dev/null\nprintf signature",
        );
        let during = match mode {
            "cancel" => format!("touch {}", quote(&private.join("cancel"))),
            "deadline" => "sleep 1.2".into(),
            _ => String::new(),
        };
        let revoke = if mode == "revoke-fail" {
            "exit 9".to_string()
        } else {
            format!("touch {}", quote(&p.join("revoked")))
        };
        let perms = if ["widened", "reviser"].contains(&mode) { "write" } else { "read" };
        script(
            &bin.join("curl"),
            &format!(
                r#"
test "$1" = --disable
shift
test "$1 $2 $3 $4 $5" = '--silent --show-error --fail --max-time 25'
shift 5
test "$1" = --request
method="$2"
shift 2
test "$1 $2" = '--header @-'
shift 2
IFS= read -r authorization
[[ "$authorization" = 'Authorization: Bearer '* ]]
jwt="${{authorization#Authorization: Bearer }}"
[[ "$jwt" = ey*.ey*.c2lnbmF0dXJl ]]
[[ "$*" != *"$jwt"* ]]
test "$(cat)" = $'Accept: application/vnd.github+json\nX-GitHub-Api-Version: 2022-11-28\nContent-Type: application/json'
# Even transport diagnostics must never escape the recorder.
printf '%s' "$jwt" >&2
case "$method" in
 GET) test "$#" = 1; test "$1" = https://api.github.com/repos/org/repo/installation
 printf '{{"id":456}}';;
 POST) test "$#" = 3; test "$1" = --data-binary
 printf '%s' "$2" > {}; {during}
 test "$3" = https://api.github.com/app/installations/456/access_tokens
 printf '{{"token":"t-secret","permissions":{{"actions":"write","contents":"{perms}","pull_requests":"read","metadata":"read"}}}}';;
 *) exit 91;;
esac
"#,
                quote(&p.join("token-request"))
            ),
        );
        script(
            &bin.join("gh"),
            &format!(
                r#"
# Secrets may be in the environment, never argv. No owner credential is used.
case "$*" in *machine-secret*|*pem-secret*|*t-secret*) exit 90;; esac
case "${{!#}}" in
 /installation/repositories) test "$GH_TOKEN" = t-secret; printf '{{"repositories":[{{"full_name":"org/repo"}}]}}';;
 /installation/token) test "$GH_TOKEN" = t-secret; {revoke};;
 *) exit 91;;
esac
"#,
            ),
        );
        let worker = match mode {
            "failure" => "printf '%s\\n' '{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"stopReason\":\"error\",\"errorMessage\":\"a-secret\"}}'".to_string(),
            "worker-timeout" | "worker-cancel" => "exec sleep 30".into(),
            _ => "printf '%s\\n' '{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"stopReason\":\"stop\",\"content\":[{\"type\":\"text\",\"text\":\"a-secret r-secret t-secret done\"}]}}'".into(),
        };
        script(
            &bin.join("sandbox"),
            &format!(
                r#"
touch {}
test -z "${{BWS_ACCESS_TOKEN+x}}"
test -z "${{OWNER_SENTINEL+x}}"
test "$GH_TOKEN" = t-secret
test "$GH_REPO" = org/repo
test ! -e {}
[[ "$*" = *--no-approve* ]]
[[ "$*" = *--no-context-files* ]]
[[ "$*" = *--no-extensions* ]]
! grep -q owner-secret "$FAB_STATE/auth.json"
{worker}
"#,
                quote(&p.join("launched")),
                quote(&private.join("app.pem"))
            ),
        );
        Self { temp, private, bin }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fab"));
        command
            .arg("dispatch-record")
            .arg(&self.private)
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("HOME", self.temp.path())
            .env("GH_TOKEN", "owner-secret")
            .env("BWS_ACCESS_TOKEN", "owner-secret")
            .env("OWNER_SENTINEL", "must-not-inherit");
        command
    }

    fn run(&self) -> std::process::Output {
        self.command().output().unwrap()
    }

    fn logs(&self) -> String {
        fs::read_to_string(self.private.join("events.jsonl")).unwrap()
    }
}

#[test]
fn recorder_issues_restricted_token_launches_and_revokes() {
    let f = Fixture::new("success");
    let out = f.run();
    assert!(out.status.success(), "{}", f.logs());
    assert!(f.temp.path().join("revoked").exists());
    assert!(f.temp.path().join("launched").exists());
    assert!(!f.private.join("state").exists());
    assert!(!f.private.join("installation-token").exists());
    assert!(f.private.join("cleanup-complete").exists());
    let ids = fs::read_to_string(f.temp.path().join("secret-ids")).unwrap();
    assert_eq!(ids, "id-one\nid-two\nid-three\n");
    let body: Value = serde_json::from_str(&fs::read_to_string(f.temp.path().join("token-request")).unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"repositories": ["repo"], "permissions": {"actions": "write", "contents": "read", "pull_requests": "read"}})
    );
    let logs = f.logs();
    assert!(logs.contains("[redacted] [redacted] [redacted] done"));
    for secret in [
        "c2lnbmF0dXJl",
        "machine-secret",
        "pem-secret",
        "owner-secret",
        "a-secret",
        "r-secret",
        "t-secret",
    ] {
        assert!(!logs.contains(secret));
        assert!(!String::from_utf8_lossy(&out.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&out.stderr).contains(secret));
    }
}

#[test]
fn reviser_requests_contents_write_only_for_its_repository() {
    let f = Fixture::new("reviser");
    assert!(f.run().status.success(), "{}", f.logs());
    let body: Value = serde_json::from_str(&fs::read_to_string(f.temp.path().join("token-request")).unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"repositories": ["repo"], "permissions": {"actions": "write", "contents": "write", "pull_requests": "read"}})
    );
    assert!(f.temp.path().join("revoked").exists());
}

#[test]
fn recorder_failure_timeout_and_cancellation_revoke_before_cleanup() {
    for mode in ["failure", "worker-timeout", "cancel", "deadline", "widened"] {
        let f = Fixture::new(mode);
        assert!(!f.run().status.success(), "{mode}");
        assert!(f.temp.path().join("revoked").exists(), "{mode}: {}", f.logs());
        assert!(f.private.join("cleanup-complete").exists(), "{mode}");
        assert!(!f.private.join("state").exists());
        if ["cancel", "deadline", "widened"].contains(&mode) {
            assert!(!f.temp.path().join("launched").exists(), "{mode}");
        }
        let logs = f.logs();
        let expected = match mode {
            "failure" => "no successful final assistant message",
            "worker-timeout" => "worker timed out",
            "cancel" | "deadline" => "during issuance",
            "widened" => "unexpected installation token permissions",
            _ => unreachable!(),
        };
        assert!(logs.contains(expected), "{mode}: {logs}");
        assert!(!logs.contains("a-secret"));
    }
}

#[test]
fn cancellation_file_stops_running_worker_and_revokes() {
    let f = Fixture::new("worker-cancel");
    let child = f.command().stdout(std::process::Stdio::piped()).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !f.temp.path().join("launched").exists() {
        assert!(std::time::Instant::now() < deadline, "worker did not launch");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    fs::write(f.private.join("cancel"), "").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(f.logs().contains("worker cancelled"));
    assert!(f.temp.path().join("revoked").exists());
    assert!(f.private.join("cleanup-complete").exists());
    assert!(!f.private.join("state").exists());
}

#[test]
fn failed_revocation_retains_recovery_state_and_cannot_signal_cleanup() {
    let f = Fixture::new("revoke-fail");
    assert!(!f.run().status.success());
    assert!(!f.private.join("cleanup-complete").exists());
    assert!(f.private.join("installation-token").exists());
    assert!(f.private.join("state/auth.json").exists());
    assert!(f.logs().contains("cleanup unconfirmed"));
}
