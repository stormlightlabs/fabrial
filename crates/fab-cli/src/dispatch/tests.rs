//! Parsing and real pipe/process tests without host credentials.
use super::*;
use std::os::unix::fs::symlink;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn shell(source: &str) -> Command {
    let mut c = Command::new("bash");
    c.args(["-c", source]);
    c
}

#[test]
fn roles_tools_and_no_fallback() {
    for role in ROLES {
        let (tools, unmapped, body) = role_input(&root(), role).unwrap();
        for tool in ["bash", "read", "find", "ls"] {
            assert!(tools.contains(&tool.to_string()));
        }
        assert_eq!(tools.contains(&"write".into()), *role == "reviser");
        assert!(unmapped.contains(&"Skill".into()));
        assert!(unmapped.contains(&"mcp__github__pull_request_read".into()));
        assert!(!body.is_empty());
        let (model, thinking) = role_config(&root(), role).unwrap();
        assert_eq!(
            thinking,
            if ["reviewer", "adversarial-reviewer"].contains(role) { "medium" } else { "low" }
        );
        assert_eq!(
            model,
            if *role == "reviewer" { "openai/gpt-6-sol" } else { "openai/gpt-6.1-sol" }
        );
    }
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join(".github")).unwrap();
    write(
        &temp.path().join(".github/fabrial-roles.toml"),
        "[reviewer.pi]\nmodel='openai/gpt-6-sol'\n",
    )
    .unwrap();
    for role in ROLES {
        assert!(role_config(temp.path(), role).is_err());
    }
    symlink(root().join("AGENTS.md"), temp.path().join("escape")).unwrap();
    assert!(trusted(temp.path(), "escape").is_err());
    let skills = temp.path().join("skills");
    fs::create_dir(&skills).unwrap();
    symlink(root().join("AGENTS.md"), skills.join("escape")).unwrap();
    assert!(copy_tree(temp.path(), &skills, &temp.path().join("copy")).is_err());
}

#[test]
fn assistant_validation_does_not_expose_provider_error() {
    for reason in ["error", "aborted", "length", "toolUse"] {
        let error = final_message(&json!({"stopReason": reason, "errorMessage": "secret"})).unwrap_err();
        assert!(!error.contains("secret"));
    }
    for message in [
        Value::Null,
        json!({"stopReason": "stop", "content": []}),
        json!({"stopReason": "stop", "errorMessage": "secret"}),
    ] {
        assert!(final_message(&message).is_err());
    }
    assert_eq!(
        final_message(&json!({"stopReason": "stop", "content": [{"type": "text", "text": "one\u{2028}two"}]})).unwrap(),
        "one\u{2028}two"
    );
}

#[test]
fn real_subprocess_frames_final_not_delta_or_user() {
    let records = [
        json!({"type": "message_update", "delta": "wrong"}),
        json!({"type": "message_end", "message": {"role": "assistant", "stopReason": "stop", "content": [{"type": "text", "text": "correct\u{2028}result"}]}}),
        json!({"type": "message_end", "message": {"role": "user", "content": "wrong"}}),
    ].map(|v| v.to_string()).join("\n");
    let command = shell(&format!("printf '%s\\n' {}", quote(&records)));
    assert_eq!(
        collect(command, Duration::from_secs(2), || false, |_| Ok(())).unwrap(),
        "correct\u{2028}result"
    );
    for source in [
        "exit 9",
        "printf 'not-json\\n'",
        "printf '{}'",
        "printf '%s\\n' '{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"stopReason\":\"error\",\"errorMessage\":\"secret\"}}'",
    ] {
        assert!(collect(shell(source), Duration::from_secs(2), || false, |_| Ok(())).is_err());
    }
}

#[test]
fn real_subprocess_cancel_timeout_and_tool_deadline() {
    for (timeout, cancel, expected) in [
        (Duration::from_millis(50), false, "timed out"),
        (Duration::from_secs(2), true, "cancelled"),
    ] {
        let started = Instant::now();
        let error = collect(shell("exec sleep 30"), timeout, || cancel, |_| Ok(())).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    assert!(
        execute(shell("exec sleep 30"), None, Duration::from_millis(50))
            .unwrap_err()
            .contains("timed out")
    );
}

#[test]
fn oauth_selects_only_subscription_entry() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    write(&path, json!({"openai": {"type": "oauth", "access": "a-secret", "refresh": "r-secret", "expires": (now()+4000)*1000, "accountId": "account", "unrelated": "omit"}, "other": {"key": "omit"}}).to_string()).unwrap();
    let selected = oauth(&path, 1800).unwrap();
    assert!(selected.get("unrelated").is_none());
    assert!(selected.get("other").is_none());
    for entry in [
        json!({"type": "api_key", "key": "owner-secret"}),
        json!({"type": "oauth", "access": "a", "refresh": "r", "expires": 1}),
    ] {
        write(&path, json!({"openai": entry}).to_string()).unwrap();
        assert!(!oauth(&path, 1800).unwrap_err().contains("owner-secret"));
    }
}

#[test]
fn local_config_requires_explicit_paths_version_and_ids() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path();
    let package = p.join("node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&package).unwrap();
    write(
        &package.join("package.json"),
        json!({"name": "@earendil-works/pi-coding-agent", "version": "1.4.0"}).to_string(),
    )
    .unwrap();
    validate_runtime(p).unwrap();
    write(
        &package.join("package.json"),
        json!({"name": "@earendil-works/pi-coding-agent", "version": "1.3.0"}).to_string(),
    )
    .unwrap();
    assert!(validate_runtime(p).unwrap_err().contains("1.4.0"));
    write(&p.join("auth.json"), "{}").unwrap();
    let mut config = json!({"sandbox": "/missing", "pi_runtime": p, "auth_file": p.join("auth.json"), "state_dir": p,
        "repository": "org/repo", "keychain_service": "local", "keychain_account": "local",
        "app_id_secret": uuid::Uuid::new_v4().to_string(), "installation_id_secret": uuid::Uuid::new_v4().to_string(), "pem_secret": uuid::Uuid::new_v4().to_string()});
    let path = p.join("config.toml");
    for (key, bad) in [
        ("pem_secret", "not-a-uuid"),
        ("pi_runtime", "relative"),
        ("repository", "bad/path/extra"),
    ] {
        let old = config[key].clone();
        config[key] = json!(bad);
        write(&path, toml::to_string(&config).unwrap()).unwrap();
        assert!(load_config(&path).is_err());
        config[key] = old;
    }
    write(&path, "repository='org/repo'").unwrap();
    assert!(load_config(&path).is_err());
}

#[test]
fn cleanup_waits_for_exit_even_after_status_and_retains_failures() {
    for status_ready in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let mut private = Private::new(temp.path()).unwrap();
        private.started = true;
        let path = private.path.clone();
        if status_ready {
            touch(&path.join("status")).unwrap();
            touch(&path.join("cleanup-complete")).unwrap();
        }
        let mut polls = 0;
        cleanup_with(&path, Duration::from_secs(1), || {
            assert!(path.join("cancel").exists());
            polls += 1;
            if polls == 1 {
                return Ok(true);
            }
            touch(&path.join("status")).unwrap();
            touch(&path.join("cleanup-complete")).unwrap();
            Ok(false)
        })
        .unwrap();
        assert_eq!(polls, 2);
        private.complete = true;
        drop(private);
        assert!(!path.exists());
    }
    for alive in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let mut private = Private::new(temp.path()).unwrap();
        private.started = true;
        let path = private.path.clone();
        touch(&path.join("status")).unwrap();
        assert!(
            cleanup_with(&path, Duration::ZERO, || Ok(alive))
                .unwrap_err()
                .contains("state retained")
        );
        drop(private);
        assert!(path.join("cancel").exists());
    }
}
