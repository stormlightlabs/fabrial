//! Exercise the actual recorder binary and host tools over subprocess pipes.
#![cfg(unix)]
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn script(path: &Path, text: &str) {
    fs::write(path, format!("#!/usr/bin/env bash\nset -eu\n{text}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

/// Run the real git in `dir` with a fixed identity, returning its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The modes whose role is a writer, so the host pushes after the worker.
const WRITER_MODES: &[&str] = &["reviser", "implementer", "push-diverged", "push-branch"];

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
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        let private = p.join("dispatch-fixture");
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
        let writer = WRITER_MODES.contains(&mode);
        let role = if mode == "reviser" {
            "reviser"
        } else if writer {
            "implementer"
        } else {
            "reviewer"
        };
        let record_dir = p.join("records/worker-one");
        fs::create_dir_all(&record_dir).unwrap();
        fs::write(record_dir.join("worker.json"), json!({
            "version": 1, "run_id": "run-one", "worker_id": "worker-one",
            "task_head": "1".repeat(40), "role": role,
            "repository": "org/repo", "worktree": p,
            "created_at": now, "updated_at": now, "state": "prepared",
            "credential_cleanup": "pending", "result_file": null, "error": null,
            "resources": {"private_state": private, "tmux_session": "fab-worker-one", "supervisor_pid": null, "worker_pid": null}
        }).to_string()).unwrap();
        let push = writer
            .then(|| json!({"branch": "storm/task", "git_dir": p.join(".git"), "objects": p.join(".git/objects")}));
        fs::write(private.join("request.json"), json!({"record": record_dir.join("worker.json"), "config": config, "role": role, "worktree": p, "timeout": if mode == "deadline" {1} else {15}, "model": "openai/gpt-6-sol", "thinking": "low", "tools": ["bash", "read"], "skills": [], "session_bus": [["DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/test-bus"], ["XDG_RUNTIME_DIR", "/run/test-runtime"]], "push": push}).to_string()).unwrap();
        if writer {
            // The worktree is the fixture directory on storm/task, one commit
            // ahead of origin, which is a bare repository the fake git puts
            // in place of the GitHub URL.
            let origin = p.join("origin.git");
            git(&p, &["init", "-q", "-b", "storm/task"]);
            git(&p, &["commit", "-q", "--allow-empty", "-m", "base"]);
            git(&p, &["init", "-q", "--bare", origin.to_str().unwrap()]);
            git(&p, &["push", "-q", origin.to_str().unwrap(), "storm/task"]);
            if mode == "push-diverged" {
                let other = git(&p, &["commit-tree", "HEAD^{tree}", "-p", "HEAD", "-m", "other"]);
                git(
                    &p,
                    &[
                        "push",
                        "-q",
                        origin.to_str().unwrap(),
                        &format!("{other}:refs/heads/storm/task"),
                    ],
                );
            }
            git(&p, &["commit", "-q", "--allow-empty", "-m", "work"]);
            if mode == "push-branch" {
                git(&p, &["switch", "-q", "-c", "feature"]);
            }
            // The worker can write its repository's config and hooks. If the
            // host's push read them, the pre-push hooks would refuse it and
            // the URL rewrites would send it to another repository.
            let elsewhere = p.join("elsewhere.git");
            git(&p, &["init", "-q", "--bare", elsewhere.to_str().unwrap()]);
            let hooks = p.join("planted-hooks");
            fs::create_dir(&hooks).unwrap();
            let hook = format!("touch {}\nexit 1", quote(&p.join("hook-ran")));
            script(&hooks.join("pre-push"), &hook);
            script(&p.join(".git/hooks/pre-push"), &hook);
            let (from, to) = (origin.to_str().unwrap(), elsewhere.to_str().unwrap());
            for (key, value) in [
                ("core.hooksPath", hooks.to_str().unwrap()),
                (&format!("url.{to}.insteadOf"), from),
                (&format!("url.{to}.pushInsteadOf"), from),
            ] {
                git(&p, &["config", key, value]);
            }
            let real = String::from_utf8(
                Command::new("sh")
                    .args(["-c", "command -v git"])
                    .output()
                    .unwrap()
                    .stdout,
            )
            .unwrap();
            // Secrets never reach git's argv; the push token comes only
            // through the environment. Hooks are off for every call.
            script(
                &bin.join("git"),
                &format!(
                    r#"
case "$*" in *machine-secret*|*pem-secret*|*t-secret*|*p-secret*) exit 90;; esac
[[ "$*" = *core.hooksPath=/dev/null* ]]
args=()
for a in "$@"; do
 if test "$a" = https://github.com/org/repo.git; then
  test "$FAB_PUSH_TOKEN" = p-secret
  a={origin}
 fi
 args+=("$a")
done
for a in "$@"; do test "$a" = push && printf '%s
' "$*" >> {calls}; done
exec {real} -c protocol.file.allow=always "${{args[@]}}"
"#,
                    origin = quote(&origin),
                    calls = quote(&p.join("pushes")),
                    real = real.trim()
                ),
            );
        }
        script(
            &bin.join("security"),
            "test \"$*\" = 'find-generic-password -s local-service -a local-account -w'\nprintf 'machine-secret\\n'",
        );
        // libsecret's lookup, which a host other than macOS uses; it reaches
        // the Secret Service over the session bus, so it must get the address.
        script(
            &bin.join("secret-tool"),
            "test \"$*\" = 'lookup service local-service account local-account'\ntest \"$DBUS_SESSION_BUS_ADDRESS\" = 'unix:path=/run/test-bus'\ntest \"$XDG_RUNTIME_DIR\" = '/run/test-runtime'\nprintf 'machine-secret'",
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
            "cancel" => format!("touch {}", quote(&record_dir.join("cancel"))),
            "deadline" => "sleep 1.2".into(),
            _ => String::new(),
        };
        let revoke = if mode == "revoke-fail" {
            "exit 9".to_string()
        } else if mode == "record-write-fail" {
            // A directory where the record goes makes the final write fail
            // for any user; a read-only mode does not stop root.
            let record = record_dir.join("worker.json");
            format!(
                "touch {}; mv {} {}; mkdir {}",
                quote(&p.join("revoked")),
                quote(&record),
                quote(&record_dir.join("worker.json.saved")),
                quote(&record)
            )
        } else {
            format!("touch {}", quote(&p.join("revoked")))
        };
        // The response grants what was asked, plus metadata, except where
        // the test widens it. A request without actions is the push token.
        let widen = if mode == "widened" { " | .permissions.contents = \"write\"" } else { "" };
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
 printf '%s\n' "$2" >> {}; {during}
 test "$3" = https://api.github.com/app/installations/456/access_tokens
 token=t-secret; [[ "$2" = *'"actions"'* ]] || token=p-secret
 printf '%s' "$2" | jq -c --arg t "$token" '{{token: $t, permissions: (.permissions + {{metadata: "read"}})}}{widen}';;
 *) exit 91;;
esac
"#,
                quote(&p.join("token-requests"))
            ),
        );
        script(
            &bin.join("gh"),
            &format!(
                r#"
# Secrets may be in the environment, never argv. No owner credential is used.
case "$*" in *machine-secret*|*pem-secret*|*t-secret*) exit 90;; esac
case "${{!#}}" in
 /installation/repositories) [[ "$GH_TOKEN" = [tp]-secret ]]; printf '{{"repositories":[{{"full_name":"org/repo"}}]}}';;
 /installation/token) [[ "$GH_TOKEN" = [tp]-secret ]]; printf '%s\n' "$GH_TOKEN" >> {revoked}; {revoke}; printf 'HTTP/2.0 204 No Content\r\n\r\n';;
 *) exit 91;;
esac
"#,
                revoked = quote(&p.join("revoked-tokens"))
            ),
        );
        let worker = match mode {
            "failure" => "printf '%s\\n' '{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"stopReason\":\"error\",\"errorMessage\":\"a-secret\"}}'".to_string(),
            "worker-timeout" | "worker-cancel" => "exec sleep 30".into(),
            "controlled" => {
                let id = "11111111-1111-4111-8111-111111111111";
                let events = [
                    json!({"type":"response", "id":id, "success":true, "data":{"disposition":"queued"}}),
                    json!({"type":"message_end", "message":{"role":"user", "content":format!("[fabrial-control:{id}] instruction")}}),
                    json!({"type":"message_end", "message":{"role":"assistant", "stopReason":"stop", "content":[{"type":"text", "text":"done"}]}}),
                ].map(|e| e.to_string()).join("' '");
                format!("IFS= read -r steer; printf '%s\\n' '{events}'; control=\"$FAB_WORKTREE/records/worker-one/controls/{id}.sent\"; for i in {{1..1000}}; do if test ! -e \"$control\"; then break; fi; sleep 0.01; done; test ! -e \"$control\"; touch \"$FAB_WORKTREE/retired-before-settlement\"")
            },
            "blocked" => "printf '%s\\n' '{\"type\":\"entry_appended\",\"entry\":{\"customType\":\"fabrial.worker-report\",\"data\":{\"kind\":\"blocked\",\"text\":\"a-secret t-secret need driver\"}}}'; exec sleep 30".into(),
            _ => "printf '%s\\n' '{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"stopReason\":\"stop\",\"content\":[{\"type\":\"text\",\"text\":\"a-secret r-secret t-secret done\"}]}}'".into(),
        };
        script(
            &bin.join("sandbox"),
            &format!(
                r#"
touch {}
test -z "${{BWS_ACCESS_TOKEN+x}}"
test -z "${{OWNER_SENTINEL+x}}"
test -z "${{DBUS_SESSION_BUS_ADDRESS+x}}"
test "$GH_TOKEN" = t-secret
test "$GH_REPO" = org/repo
test "$XDG_STATE_HOME" = "${{FAB_STATE%/state}}/sandbox-logs"
mkdir -p "$XDG_STATE_HOME/agent-sandbox/fixture"
touch "$XDG_STATE_HOME/agent-sandbox/fixture/temporary"
test ! -e {}
[[ "$*" = *--no-approve* ]]
[[ "$*" = *--no-context-files* ]]
[[ "$*" = *--no-extensions* ]]
! grep -q owner-secret "$FAB_STATE/auth.json"
IFS= read -r ready
[[ "$ready" = *'"type":"get_commands"'* ]]
printf '%s\n' '{{"type":"response","id":"ready","success":true,"data":{{"commands":[{{"name":"fabrial-worker-ready"}}]}}}}'
IFS= read -r prompt
[[ "$prompt" = *'"type":"prompt"'* ]]
printf '%s\n' '{{"type":"response","id":"initial","success":true,"data":{{"disposition":"started"}}}}'
{worker}
printf '%s\n' '{{"type":"agent_settled","aborted":false}}'
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
            .env("OWNER_SENTINEL", "must-not-inherit")
            // A tmux pane holds the server's bus, not the caller's: the
            // recorder must take the request's.
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/stale-bus")
            .env("XDG_RUNTIME_DIR", "/run/stale-runtime");
        command
    }

    fn run(&self) -> std::process::Output {
        self.command().output().unwrap()
    }

    fn logs(&self) -> String {
        fs::read_to_string(self.temp.path().join("records/worker-one/events.jsonl")).unwrap()
    }

    /// Each token request body, in order.
    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.temp.path().join("token-requests"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Each revoked token, in order.
    fn revoked(&self) -> Vec<String> {
        fs::read_to_string(self.temp.path().join("revoked-tokens"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn record(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.temp.path().join("records/worker-one/worker.json")).unwrap())
            .unwrap()
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
    assert!(!f.private.exists());
    let ids = fs::read_to_string(f.temp.path().join("secret-ids")).unwrap();
    assert_eq!(ids, "id-one\nid-two\nid-three\n");
    assert_eq!(
        f.requests(),
        [
            json!({"repositories": ["repo"], "permissions": {"actions": "write", "contents": "read", "pull_requests": "read"}})
        ]
    );
    assert_eq!(f.revoked(), ["t-secret"]);
    assert!(!f.temp.path().join("pushes").exists());
    let record = f.record();
    assert_eq!(record["state"], "succeeded");
    assert_eq!(record["credential_cleanup"], "complete");
    assert_eq!(record["task_head"], "1".repeat(40));
    assert!(record["resources"]["supervisor_pid"].as_u64().unwrap() > 0);
    assert!(record["resources"]["worker_pid"].as_u64().unwrap() > 0);
    let result_path = PathBuf::from(record["result_file"].as_str().unwrap());
    let saved_result = fs::read_to_string(&result_path).unwrap();
    assert_eq!(saved_result, "[redacted] [redacted] [redacted] done");
    assert_eq!(fs::metadata(&result_path).unwrap().permissions().mode() & 0o777, 0o600);
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
        "p-secret",
    ] {
        assert!(!logs.contains(secret));
        assert!(!String::from_utf8_lossy(&out.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&out.stderr).contains(secret));
        assert!(!record.to_string().contains(secret));
        assert!(!saved_result.contains(secret));
    }
    assert!(!f.private.exists());
    assert_eq!(f.record(), record);
    assert_eq!(fs::read_to_string(result_path).unwrap(), saved_result);
}

#[test]
fn writers_get_read_tokens_and_the_host_pushes_with_its_own() {
    for role in ["reviser", "implementer"] {
        let f = Fixture::new(role);
        let out = f.run();
        assert!(out.status.success(), "{role}: {}", f.logs());
        assert_eq!(
            f.requests(),
            [
                json!({"repositories": ["repo"], "permissions": {"actions": "write", "contents": "read", "pull_requests": "read"}}),
                json!({"repositories": ["repo"], "permissions": {"contents": "write"}}),
            ],
            "{role}"
        );
        // The worker's token goes before the push token is minted.
        assert_eq!(f.revoked(), ["t-secret", "p-secret"], "{role}");
        let head = git(f.temp.path(), &["rev-parse", "HEAD"]);
        let origin = f.temp.path().join("origin.git");
        assert_eq!(git(&origin, &["rev-parse", "refs/heads/storm/task"]), head, "{role}");
        let elsewhere = f.temp.path().join("elsewhere.git");
        assert_eq!(git(&elsewhere, &["for-each-ref"]), "", "{role}");
        assert!(!f.temp.path().join("hook-ran").exists(), "{role}");
        let pushes = fs::read_to_string(f.temp.path().join("pushes")).unwrap();
        assert!(pushes.contains(&format!("{head}:refs/heads/storm/task")), "{pushes}");
        let logs = f.logs();
        assert!(logs.contains(&format!(r#""head":"{head}""#)), "{logs}");
        assert_eq!(f.record()["state"], "succeeded");
        assert!(!f.private.exists());
        for secret in ["t-secret", "p-secret"] {
            assert!(!logs.contains(secret));
            assert!(!String::from_utf8_lossy(&out.stdout).contains(secret));
        }
    }
}

#[test]
fn host_refuses_a_push_off_the_branch_or_not_a_fast_forward() {
    for mode in ["push-diverged", "push-branch"] {
        let f = Fixture::new(mode);
        let origin = f.temp.path().join("origin.git");
        let before = git(&origin, &["rev-parse", "refs/heads/storm/task"]);
        assert!(!f.run().status.success(), "{mode}");
        assert_eq!(git(&origin, &["rev-parse", "refs/heads/storm/task"]), before, "{mode}");
        // Git refuses the non-fast-forward; a wrong branch never reaches it.
        assert_eq!(f.temp.path().join("pushes").exists(), mode == "push-diverged", "{mode}");
        let logs = f.logs();
        let expected = if mode == "push-diverged" {
            "origin refused storm/task: [rejected] (non-fast-forward)"
        } else {
            "no longer on storm/task"
        };
        assert!(logs.contains(expected), "{mode}: {logs}");
        let record = f.record();
        assert_eq!(record["state"], "failed", "{mode}");
        assert_eq!(record["credential_cleanup"], "complete", "{mode}");
        assert!(!f.private.exists(), "{mode}");
        // A wrong branch is refused before a push token exists.
        let revoked: &[&str] = if mode == "push-diverged" { &["t-secret", "p-secret"] } else { &["t-secret"] };
        assert_eq!(f.revoked(), revoked, "{mode}");
    }
}

#[test]
fn recorder_failure_timeout_and_cancellation_revoke_before_cleanup() {
    for mode in ["failure", "worker-timeout", "cancel", "deadline", "widened"] {
        let f = Fixture::new(mode);
        assert!(!f.run().status.success(), "{mode}");
        assert!(f.temp.path().join("revoked").exists(), "{mode}: {}", f.logs());
        assert!(!f.private.exists(), "{mode}");
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
        let record = f.record();
        assert_eq!(record["credential_cleanup"], "complete", "{mode}");
        let state = match mode {
            "cancel" => "cancelled",
            "deadline" | "worker-timeout" => "timed_out",
            _ => "failed",
        };
        assert_eq!(record["state"], state, "{mode}");
        assert!(record["error"].as_str().unwrap().contains(expected));
    }
}

#[test]
fn cancellation_file_stops_running_worker_and_revokes() {
    let f = Fixture::new("worker-cancel");
    let child = f.command().stdout(std::process::Stdio::piped()).spawn().unwrap();
    // Match the request's startup budget; concurrent fsyncs can exceed five seconds.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !f.temp.path().join("launched").exists() {
        if std::time::Instant::now() >= deadline {
            fs::write(f.temp.path().join("records/worker-one/cancel"), "").unwrap();
            let out = child.wait_with_output().unwrap();
            panic!("worker did not launch: {}", String::from_utf8_lossy(&out.stdout));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    fs::write(f.temp.path().join("records/worker-one/cancel"), "").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(f.logs().contains("worker cancelled"));
    assert!(f.temp.path().join("revoked").exists());
    assert_eq!(f.record()["credential_cleanup"], "complete");
    assert!(!f.private.exists());
}

#[test]
fn failed_revocation_retains_recovery_state_and_cannot_signal_cleanup() {
    let f = Fixture::new("revoke-fail");
    assert!(!f.run().status.success());
    assert!(!f.private.join("cleanup-complete").exists());
    assert!(f.private.join("installation-token").exists());
    assert!(f.private.join("state/auth.json").exists());
    assert!(f.logs().contains("cleanup unconfirmed"));
    let record = f.record();
    assert_eq!(record["state"], "failed");
    assert_eq!(record["credential_cleanup"], "unconfirmed");
    assert_eq!(record["resources"]["private_state"], f.private.to_str().unwrap());
}

#[test]
fn final_record_write_failure_retains_durable_evidence() {
    let f = Fixture::new("record-write-fail");
    let out = f.run();
    let directory = f.temp.path().join("records/worker-one");
    fs::remove_dir(directory.join("worker.json")).unwrap();
    fs::rename(directory.join("worker.json.saved"), directory.join("worker.json")).unwrap();
    assert!(!out.status.success());
    assert!(f.temp.path().join("revoked").exists());
    assert!(!f.private.join("installation-token").exists());
    assert!(!f.private.join("state").exists());
    assert!(!f.private.join("cleanup-complete").exists());
    assert!(f.logs().contains("worker record unconfirmed"));
    assert!(directory.join("result.txt").exists());
    assert_eq!(f.record()["state"], "running");
}

#[test]
fn processed_controls_release_their_outstanding_queue_slot() {
    let f = Fixture::new("controlled");
    let controls = f.temp.path().join("records/worker-one/controls");
    fs::create_dir(&controls).unwrap();
    let id = "11111111-1111-4111-8111-111111111111";
    fs::write(
        controls.join(format!("{id}.json")),
        json!({"id":id,"sequence":1,"worker_id":"worker-one","run_id":"run-one","task_head":"1".repeat(40),"message":"instruction"})
            .to_string(),
    )
    .unwrap();
    let output = f.run();
    assert!(output.status.success(), "{}", f.logs());
    assert!(f.logs().contains("control_processed"));
    assert!(f.temp.path().join("retired-before-settlement").exists());
    assert!(!controls.join(format!("{id}.sent")).exists());
}

#[test]
fn blocker_report_stops_worker_and_persists_only_redacted_data() {
    let f = Fixture::new("blocked");
    assert!(!f.run().status.success());
    assert_eq!(f.record()["state"], "blocked");
    assert_eq!(f.record()["report"]["text"], "[redacted] [redacted] need driver");
    assert_eq!(f.record()["credential_cleanup"], "complete");
    assert!(f.temp.path().join("revoked").exists());
    assert!(!f.private.exists());
    assert!(!f.logs().contains("a-secret"));
    assert!(!f.logs().contains("t-secret"));
}

#[test]
fn recovery_is_idempotent_and_retains_uncertain_credentials_or_live_owners() {
    for mode in ["invalid-token", "transport-failure", "live-owner", "unpersisted-token"] {
        let f = Fixture::new("revoke-fail");
        assert!(!f.run().status.success());
        let id = uuid::Uuid::new_v4().to_string();
        let directory = f.temp.path().canonicalize().unwrap().join("records").join(&id);
        let mut record = f.record();
        fs::rename(f.temp.path().join("records/worker-one"), &directory).unwrap();
        record["worker_id"] = json!(id);
        record["result_file"] = json!(directory.join("result.txt"));
        if mode == "live-owner" {
            record["resources"]["launcher_pid"] = json!(std::process::id());
        } else {
            record["boot_id"] = json!("previous-boot");
            // These live PIDs must be ignored after a reboot, never signalled.
            record["resources"]["supervisor_pid"] = json!(std::process::id());
            record["resources"]["worker_pid"] = json!(std::process::id());
        }
        fs::write(directory.join("worker.json"), record.to_string()).unwrap();
        let mut request: Value =
            serde_json::from_str(&fs::read_to_string(f.private.join("request.json")).unwrap()).unwrap();
        request["record"] = json!(directory.join("worker.json"));
        fs::write(f.private.join("request.json"), request.to_string()).unwrap();
        if mode == "unpersisted-token" {
            fs::remove_file(f.private.join("installation-token")).unwrap();
            fs::write(
                f.private.join("revocation-failed"),
                b"token was issued but not persisted",
            )
            .unwrap();
        }
        script(&f.bin.join("tmux"), "exit 1");
        let calls = f.temp.path().join("recovery-calls");
        let response = if mode == "invalid-token" {
            format!(
                "test \"$GH_TOKEN\" = t-secret; echo called >> '{}'; printf 'HTTP/2.0 401 Unauthorized\\r\\n\\r\\n'; exit 1",
                calls.display()
            )
        } else {
            "exit 9".into()
        };
        script(&f.bin.join("gh"), &response);
        let config = f.temp.path().join("recover.toml");
        fs::write(&config, format!("state_dir = {:?}\n", f.temp.path().to_str().unwrap())).unwrap();
        let run = || {
            Command::new(env!("CARGO_BIN_EXE_fab"))
                .args(["worker", "recover", &id, "--config"])
                .arg(&config)
                .env(
                    "PATH",
                    format!("{}:{}", f.bin.display(), std::env::var("PATH").unwrap()),
                )
                .env("HOME", f.temp.path())
                .env("GH_TOKEN", "owner-secret")
                .output()
                .unwrap()
        };
        let output = run();
        assert_eq!(
            output.status.success(),
            mode == "invalid-token",
            "{mode}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(f.private.exists(), mode != "invalid-token");
        if mode == "invalid-token" {
            assert!(directory.join("result.txt").exists());
            assert!(run().status.success());
            assert_eq!(fs::read_to_string(calls).unwrap().lines().count(), 1);
            let record: Value = serde_json::from_slice(&fs::read(directory.join("worker.json")).unwrap()).unwrap();
            assert_eq!(record["credential_cleanup"], "complete");
        }
        assert!(!String::from_utf8_lossy(&output.stdout).contains("t-secret"));
    }
}

#[test]
fn missing_worker_record_prevents_credential_issuance() {
    let f = Fixture::new("success");
    fs::remove_file(f.temp.path().join("records/worker-one/worker.json")).unwrap();
    assert!(!f.run().status.success());
    assert!(!f.temp.path().join("secret-ids").exists());
    assert!(!f.temp.path().join("launched").exists());
}
