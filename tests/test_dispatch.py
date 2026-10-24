"""Focused host boundary tests; no owner credentials, Nix, tmux or model calls."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("dispatch", ROOT / "crates/fab-core/src/dispatch.py")
dispatch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(dispatch)


class DispatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / "target")
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)

    def test_roles_and_unmapped_tools(self):
        for role in dispatch.ROLES:
            tools, unmapped, body = dispatch.role_input(ROOT, role)
            self.assertIn("bash", tools)
            self.assertIn("read", tools)
            self.assertIn("find", tools)
            self.assertIn("ls", tools)
            self.assertIn("Skill", unmapped)
            self.assertIn("mcp__github__pull_request_read", unmapped)
            self.assertEqual("write" in tools, role == "reviser")
            self.assertTrue(body)
            entry = dispatch.role_config(ROOT, role)
            self.assertEqual(entry["thinking"], "low")
            self.assertEqual(entry["model"], "openai/gpt-6-sol" if role == "reviewer" else "openai/gpt-6.1-sol")

    def test_no_model_fallback_and_trusted_symlinks(self):
        (self.path / ".github").mkdir()
        (self.path / ".github/fabrial-roles.toml").write_text("[reviewer.pi]\nmodel='openai/gpt-6-sol'\n")
        for role in dispatch.ROLES:
            with self.assertRaises(dispatch.DispatchError):
                dispatch.role_config(self.path, role)
        (self.path / "escape").symlink_to(ROOT / "AGENTS.md")
        with self.assertRaises(dispatch.DispatchError):
            dispatch.trusted(self.path, "escape")

    def test_final_assistant_failure_even_with_zero_exit(self):
        for message in (None, {"stopReason": "error", "errorMessage": "secret"},
                        {"stopReason": "aborted"}, {"stopReason": "length"},
                        {"stopReason": "toolUse"}, {"stopReason": "stop", "content": []}):
            with self.assertRaises(dispatch.DispatchError) as error:
                dispatch.final_message(message)
            self.assertNotIn("secret", str(error.exception))
        self.assertEqual(dispatch.final_message({"stopReason": "stop", "content": [
            {"type": "text", "text": "one\u2028two"}]}), "one\u2028two")

    def collect(self, source, timeout=2, cancelled=lambda: False):
        # This worker's sandbox cannot signal host process groups. Exercise real
        # pipe framing with a fake group boundary; children here have no children.
        launched = []
        popen = subprocess.Popen
        def launch(*args, **kwargs):
            child = popen(*args, **kwargs)
            launched.append(child)
            return child
        def kill_group(pid, sig):
            self.assertEqual(pid, launched[0].pid)
            self.assertEqual(sig, dispatch.signal.SIGKILL)
            if launched[0].poll() is None:
                launched[0].kill()
        with patch.object(dispatch.subprocess, "Popen", side_effect=launch), patch.object(dispatch.os, "killpg", side_effect=kill_group) as kill:
            try:
                return dispatch.collect([sys.executable, "-c", source], dict(os.environ),
                                        self.path, timeout, lambda e: None, cancelled)
            finally:
                kill.assert_called_once()

    def test_subprocess_final_event_not_delta_or_user(self):
        records = [
            {"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "delta": "wrong"}},
            {"type": "message_end", "message": {"role": "assistant", "stopReason": "stop", "content": [{"type": "text", "text": "correct\u2028result"}]}},
            {"type": "message_end", "message": {"role": "user", "content": "wrong"}},
        ]
        text = "\n".join(json.dumps(e) for e in records) + "\n"
        self.assertEqual(self.collect(f"import sys; sys.stdout.write({text!r})"), "correct\u2028result")
        records[1]["message"]["stopReason"] = "error"
        with self.assertRaises(dispatch.DispatchError):
            self.collect(f"print({json.dumps(records[1])!r})")

    def test_subprocess_exit_timeout_cancel_and_invalid_json(self):
        for source in ("raise SystemExit(9)", "print('not json')", "print('{}', end='')"):
            with self.assertRaises(dispatch.DispatchError):
                self.collect(source)
        for timeout, cancelled, expected in ((0.05, lambda: False, "timed out"),
                                               (2, lambda: True, "cancelled")):
            started = time.monotonic()
            with self.assertRaisesRegex(dispatch.DispatchError, expected):
                self.collect("import time; time.sleep(30)", timeout, cancelled)
            self.assertLess(time.monotonic() - started, 2)

    def test_oauth_selects_only_subscription_credential(self):
        path = self.path / "auth.json"
        path.write_text(json.dumps({"openai": {"type": "oauth", "access": "access-secret",
            "refresh": "refresh-secret", "expires": (time.time() + 4000) * 1000,
            "accountId": "account", "unrelated": "omit"}, "other": {"key": "omit"}}))
        credential = dispatch.oauth(path, 1800)
        self.assertNotIn("other", credential)
        self.assertNotIn("unrelated", credential)
        path.write_text(json.dumps({"openai": {"type": "api_key", "key": "owner-secret"}}))
        with self.assertRaises(dispatch.DispatchError) as error:
            dispatch.oauth(path, 1800)
        self.assertNotIn("owner-secret", str(error.exception))

    def test_issuer_requests_only_explicit_secrets_and_restricts_token(self):
        config = {"keychain_service": "local-service", "keychain_account": "local-account",
                  "app_id_secret": "id-one", "installation_id_secret": "id-two",
                  "pem_secret": "id-three", "repository": "org/repo"}
        calls, requests = [], []
        def fake_run(argv, **kwargs):
            calls.append((argv, kwargs))
            if argv[0] == "security":
                return b"machine-secret\n"
            if argv[0] == "bws":
                return json.dumps({"value": {"id-one": "123", "id-two": "456", "id-three": "pem-secret"}[argv[3]]}).encode()
            return b"signature"
        def fake_api(method, path, token, body=None):
            requests.append((method, path, token, body))
            if path.endswith("/installation"):
                return {"id": 456}
            if path.endswith("/access_tokens"):
                return {"token": "worker-secret", "permissions": {**body["permissions"], "metadata": "read"}}
            return {"repositories": [{"full_name": "org/repo"}]}
        with patch.object(dispatch, "run", fake_run), patch.object(dispatch, "api", fake_api):
            self.assertEqual(dispatch.issue(config, self.path, "reviser"), "worker-secret")
        self.assertEqual([argv[3] for argv, _ in calls if argv[0] == "bws"], ["id-one", "id-two", "id-three"])
        self.assertFalse((self.path / "app.pem").exists())
        for argv, kwargs in calls:
            self.assertNotIn("machine-secret", str(argv))
            self.assertNotIn("pem-secret", str(argv))
            if argv[0] == "bws":
                self.assertEqual(kwargs["env"]["BWS_ACCESS_TOKEN"], "machine-secret")
        body = requests[1][3]
        self.assertEqual(body["repositories"], ["repo"])
        self.assertEqual(body["permissions"], {"actions": "write", "contents": "write", "pull_requests": "read"})

    def test_recorder_revokes_on_worker_failure_and_cleans_credentials(self):
        config = {"auth_file": "unused", "pi_runtime": "unused", "repository": "org/repo", "sandbox": "unused"}
        (self.path / "request.json").write_text(json.dumps({"config": config, "timeout": 1,
            "role": "reviewer", "worktree": str(self.path), "model": "openai/gpt-6-sol",
            "thinking": "low", "tools": ["read", "bash"], "skills": []}))
        (self.path / "input").mkdir()
        (self.path / "input/task.txt").write_text("task")
        calls = []
        credential = {"type": "oauth", "access": "a-secret", "refresh": "r-secret", "expires": 1e15}
        with patch.object(dispatch, "oauth", return_value=credential), patch.object(dispatch, "issue", return_value="t-secret"), patch.object(dispatch, "api", side_effect=lambda *a: calls.append(a)), patch.object(dispatch, "collect", side_effect=dispatch.DispatchError("worker failed")):
            self.assertEqual(dispatch.worker(self.path), 1)
        self.assertEqual(calls, [("DELETE", "/installation/token", "t-secret")])
        self.assertFalse((self.path / "state").exists())
        self.assertTrue((self.path / "cleanup-complete").exists())
        # Cancellation arriving during issuance must revoke without starting a worker.
        def delayed_issue(*args):
            (self.path / "cancel").touch()
            return "t-secret"
        with patch.object(dispatch, "oauth", return_value=credential), patch.object(dispatch, "issue", side_effect=delayed_issue), patch.object(dispatch, "api") as revoke, patch.object(dispatch, "collect") as collect:
            self.assertEqual(dispatch.worker(self.path), 1)
            collect.assert_not_called()
            revoke.assert_called_once_with("DELETE", "/installation/token", "t-secret")
        (self.path / "cleanup-complete").unlink()
        with patch.object(dispatch, "oauth", return_value=credential), patch.object(dispatch, "issue", return_value="t-secret"), patch.object(dispatch, "api", side_effect=dispatch.DispatchError("revocation failed")):
            self.assertEqual(dispatch.worker(self.path), 1)
        self.assertFalse((self.path / "cleanup-complete").exists())
        self.assertTrue((self.path / "revocation-failed").exists())
        logs = (self.path / "events.jsonl").read_text()
        for secret in ("a-secret", "r-secret", "t-secret"):
            self.assertNotIn(secret, logs)

    def test_local_config_validates_version_paths_and_secret_ids(self):
        package = self.path / "node_modules/@earendil-works/pi-coding-agent/package.json"
        package.parent.mkdir(parents=True)
        package.write_text(json.dumps({"name": "@earendil-works/pi-coding-agent", "version": "1.4.0"}))
        auth = self.path / "auth.json"
        auth.write_text("{}")
        values = {"sandbox": "/nix/store/test/bin/fab-pi-sandboxed", "pi_runtime": str(self.path),
                  "auth_file": str(auth), "state_dir": str(self.path), "repository": "org/repo",
                  "keychain_service": "local", "keychain_account": "local",
                  **{key: str(uuid.uuid4()) for key in ("app_id_secret", "installation_id_secret", "pem_secret")}}
        config = self.path / "config.toml"
        def save():
            config.write_text("\n".join(f"{k} = {json.dumps(v)}" for k, v in values.items()))
        save()
        # The trusted executable need not be available in this worker sandbox.
        resolve = Path.resolve
        def resolve_path(path, **kwargs):
            return path if str(path).startswith("/nix/store/test/") else resolve(path, **kwargs)
        with patch.object(Path, "resolve", resolve_path):
            self.assertEqual(dispatch.load_config(config)["repository"], "org/repo")
            for key, bad in (("pem_secret", "not-a-uuid"), ("pi_runtime", "relative"),
                             ("repository", "bad/path/extra")):
                old = values[key]
                values[key] = bad
                save()
                with self.assertRaises((dispatch.DispatchError, ValueError)):
                    dispatch.load_config(config)
                values[key] = old
            save()
            package.write_text(json.dumps({"name": "@earendil-works/pi-coding-agent", "version": "1.3.0"}))
            with self.assertRaisesRegex(dispatch.DispatchError, "1.4.0"):
                dispatch.load_config(config)

    def test_dispatch_host_tmux_boundary_and_unexpected_pane_exit(self):
        trusted = self.path / "trusted"
        worktree = self.path / "worktree"
        state = self.path / "runs"
        common = self.path / "common.git"
        for path in (trusted, worktree, state, common):
            path.mkdir(mode=0o700)
        (worktree / ".git").write_text("gitdir: fake")
        for name in ("review", "storm", "writing-docs"):
            path = trusted / ".claude/skills" / name
            path.mkdir(parents=True)
            (path / "SKILL.md").write_text("Trusted instruction")
        config_path = self.path / "issuer.toml"
        config_path.write_text("unused")
        task = self.path / "task.txt"
        task.write_text("Review a pinned head")
        config = {"state_dir": str(state), "pi_runtime": str(self.path / "runtime"),
                  "auth_file": str(self.path / "auth.json")}
        args = argparse.Namespace(trusted_checkout=trusted, worktree=worktree,
            config=config_path, role="reviewer", task=task, timeout=1)
        commands = []
        def fake_run(argv, **kwargs):
            commands.append(argv)
            if argv[0] == "git":
                return str(common).encode()
            self.assertEqual(argv[:3], ["tmux", "new-session", "-d"])
            private = next(state.iterdir())
            self.assertNotIn("GH_TOKEN", (private / "request.json").read_text())
            self.assertEqual((private / "input/task.txt").read_text(), task.read_text())
            self.assertIn("trusted", (private / "input/role.md").read_text())
            return b""
        with patch.object(dispatch, "load_config", return_value=config), patch.object(dispatch, "role_config", return_value={"model": "openai/gpt-6-sol", "thinking": "low"}), patch.object(dispatch, "role_input", return_value=(["read", "bash"], ["Skill"], "trusted body")), patch.object(dispatch, "run", side_effect=fake_run), patch.object(dispatch.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)), patch.object(dispatch, "emit"):
            with self.assertRaisesRegex(dispatch.DispatchError, "cleanup unconfirmed"):
                dispatch.dispatch(args)
        self.assertEqual(len(list(state.iterdir())), 1)
        self.assertTrue((next(state.iterdir()) / "cancel").exists())
        self.assertEqual(commands[1][0], "tmux")

    def test_cleanup_waits_for_recorder_exit_even_after_status(self):
        for status_ready in (False, True):
            with self.subTest(status_ready=status_ready), dispatch.private_directory(self.path) as private:
                (private / "recorder-started").touch()
                if status_ready:
                    (private / "status").write_text("1")
                    (private / "cleanup-complete").touch()
                polls = []
                def pane(*args, **kwargs):
                    self.assertEqual(args[0][1], "has-session")
                    self.assertTrue((private / "cancel").exists())
                    polls.append(1)
                    if len(polls) == 1:
                        return subprocess.CompletedProcess([], 0)
                    (private / "status").write_text("1")
                    (private / "cleanup-complete").touch()
                    return subprocess.CompletedProcess([], 1)
                with patch.object(dispatch.subprocess, "run", side_effect=pane), patch.object(dispatch.time, "sleep"):
                    dispatch.wait_for_cleanup(private, "session")
                self.assertEqual(len(polls), 2)
                (private / "host-cleanup-complete").touch()
            self.assertFalse(private.exists())

    def test_cleanup_retains_state_on_stall_or_failed_revocation(self):
        for pane_status in (0, 1):
            with self.subTest(pane_status=pane_status):
                with dispatch.private_directory(self.path) as private:
                    (private / "recorder-started").touch()
                    (private / "status").write_text("1")
                    with patch.object(dispatch.subprocess, "run", return_value=subprocess.CompletedProcess([], pane_status)):
                        with self.assertRaisesRegex(dispatch.DispatchError, "state retained"):
                            dispatch.wait_for_cleanup(private, "session", timeout=0)
                self.assertTrue(private.exists())
                self.assertTrue((private / "cancel").exists())

    def test_local_config_requires_every_credential_identifier(self):
        path = self.path / "config.toml"
        path.write_text('repository="org/repo"\n')
        with self.assertRaises(dispatch.DispatchError):
            dispatch.load_config(path)


if __name__ == "__main__":
    unittest.main()
