"""Trusted host launcher for fresh Pi workers. Python 3.11 or newer."""
import argparse
import base64
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
import urllib.request
import uuid

ROLES = {"reviewer", "verifier", "adversarial-reviewer", "reviser"}
TOOLS = {"Read": ["read"], "Grep": ["grep"], "Glob": ["find", "ls"],
         "Bash": ["bash"], "Write": ["write"], "Edit": ["edit"]}
THINKING = {"off", "minimal", "low", "medium", "high", "xhigh", "max"}


class DispatchError(Exception):
    """An error whose message contains no subprocess output or credentials."""


def run(argv, *, env=None, data=None):
    result = subprocess.run(argv, input=data, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, env=env, timeout=30, check=False)
    if result.returncode:
        raise DispatchError("trusted subprocess failed: " + Path(argv[0]).name)
    return result.stdout


def trusted(root, relative):
    path = (root / relative).resolve(strict=True)
    if not path.is_relative_to(root):
        raise DispatchError("trusted input escapes instruction checkout")
    return path


def role_input(root, role):
    text = trusted(root, f".claude/agents/{role}.md").read_text()
    parts = text.split("---", 2)
    if len(parts) != 3 or parts[0].strip():
        raise DispatchError("role has no frontmatter")
    match = re.search(r"^tools: (.+)$", parts[1], re.M)
    if not match:
        raise DispatchError("role has no tools")
    mapped, unmapped = [], []
    for tool in match[1].split(","):
        tool = tool.strip()
        if tool in TOOLS:
            mapped.extend(TOOLS[tool])
        else:
            unmapped.append(tool)
    if "bash" not in mapped or "read" not in mapped:
        raise DispatchError("role needs bash and read")
    return sorted(set(mapped)), unmapped, parts[2].strip()


def role_config(root, role):
    config = tomllib.loads(trusted(root, ".github/fabrial-roles.toml").read_text())
    table = config.get(role, {})
    entry = table.get("pi", {}) if isinstance(table, dict) else {}
    if (not isinstance(entry, dict) or not isinstance(entry.get("model"), str)
            or not re.fullmatch(r"openai/[a-zA-Z0-9._-]+", entry["model"])
            or entry.get("thinking") not in THINKING):
        raise DispatchError(f"missing or invalid Pi model/thinking for {role}")
    return entry


def load_config(path):
    config = tomllib.loads(path.read_text())
    for key in ("sandbox", "pi_runtime", "auth_file", "state_dir", "keychain_service",
                "keychain_account", "app_id_secret", "installation_id_secret", "pem_secret",
                "repository"):
        if not isinstance(config.get(key), str) or not config[key]:
            raise DispatchError("missing local configuration: " + key)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", config["repository"]):
        raise DispatchError("invalid repository")
    for key in ("app_id_secret", "installation_id_secret", "pem_secret"):
        if str(uuid.UUID(config[key])) != config[key]:
            raise DispatchError("secret IDs must be explicit UUIDs")
    for key in ("sandbox", "pi_runtime", "auth_file", "state_dir"):
        path = Path(config[key])
        if not path.is_absolute():
            raise DispatchError("local paths must be absolute: " + key)
        config[key] = str(path.resolve(strict=True))
    if not config["sandbox"].startswith("/nix/store/"):
        raise DispatchError("sandbox must be the built Nix store executable")
    runtime = Path(config["pi_runtime"])
    if runtime == Path.home() or runtime in Path.home().parents or runtime == Path.home() / ".pi":
        raise DispatchError("Pi runtime must be a dedicated installation, not owner home or Pi state")
    package = json.loads((runtime / "node_modules/@earendil-works/pi-coding-agent/package.json").read_text())
    if package.get("name") != "@earendil-works/pi-coding-agent" or package.get("version") != "1.4.0":
        raise DispatchError("Pi runtime must be 1.4.0")
    return config


def api(method, path, token, body=None):
    request = urllib.request.Request("https://api.github.com" + path,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"Authorization": "Bearer " + token, "Accept": "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "fabrial"}, method=method)
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            raw = response.read()
            return json.loads(raw) if raw else None
    except Exception:
        raise DispatchError("GitHub App request failed") from None


def encode(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def issue(config, private, role):
    machine = run(["security", "find-generic-password", "-s", config["keychain_service"],
                   "-a", config["keychain_account"], "-w"]).decode().strip()
    env = {"PATH": os.environ["PATH"], "HOME": os.environ["HOME"], "BWS_ACCESS_TOKEN": machine}
    values = []
    for key in ("app_id_secret", "installation_id_secret", "pem_secret"):
        value = json.loads(run(["bws", "secret", "get", config[key], "--output", "json"], env=env))
        values.append(value["value"])
    app, installation, pem = values
    if not str(app).isdigit() or not str(installation).isdigit():
        raise DispatchError("App and installation IDs must be numeric")
    keyfile = private / "app.pem"
    keyfile.write_text(pem)
    keyfile.chmod(0o600)
    now = int(time.time())
    signing = encode(b'{"alg":"RS256","typ":"JWT"}') + "." + encode(json.dumps(
        {"iat": now - 60, "exp": now + 540, "iss": app}).encode())
    try:
        signature = run(["openssl", "dgst", "-sha256", "-sign", str(keyfile)], data=signing.encode())
    finally:
        keyfile.unlink(missing_ok=True)
    jwt = signing + "." + encode(signature)
    owner, repo = config["repository"].split("/")
    installed = api("GET", f"/repos/{owner}/{repo}/installation", jwt)
    if str(installed["id"]) != str(installation):
        raise DispatchError("installation does not match repository")
    permissions = {"actions": "write", "contents": "write" if role == "reviser" else "read",
                   "pull_requests": "read"}
    response = api("POST", f"/app/installations/{installation}/access_tokens", jwt,
                   {"repositories": [repo], "permissions": permissions})
    token = response["token"]
    try:
        # Reject server-side widening rather than assuming the requested scope was used.
        if response.get("permissions") != {**permissions, "metadata": "read"}:
            raise DispatchError("unexpected installation token permissions")
        repos = api("GET", "/installation/repositories", token)["repositories"]
        if [r["full_name"].lower() for r in repos] != [config["repository"].lower()]:
            raise DispatchError("installation token is not repository-restricted")
    except BaseException:
        try:
            api("DELETE", "/installation/token", token)
        except BaseException:
            (private / "revocation-failed").touch()
            raise
        raise
    return token


def oauth(path, timeout):
    value = json.loads(Path(path).read_text()).get("openai", {})
    if (value.get("type") != "oauth" or not value.get("access") or not value.get("refresh")
            or value.get("expires", 0) < (time.time() + timeout + 60) * 1000):
        raise DispatchError("openai OAuth credential missing or expires before worker deadline; refresh on host")
    return {key: value[key] for key in ("type", "access", "refresh", "expires", "accountId") if key in value}


def final_message(message):
    if not message or message.get("stopReason") != "stop" or message.get("errorMessage"):
        raise DispatchError("worker has no successful final assistant message")
    content = message.get("content", [])
    text = "\n".join(block["text"] for block in content if block.get("type") == "text")
    if not text.strip():
        raise DispatchError("worker returned no final text")
    return text


def emit(event):
    print(json.dumps(event, ensure_ascii=False), flush=True)


def collect(argv, env, directory, timeout, output, cancelled):
    """Pipe both worker streams to the host; never bind host logs into sandbox."""
    process = subprocess.Popen(argv, cwd=directory, env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    selector = selectors.DefaultSelector()
    buffers = {}
    message = None
    deadline = time.monotonic() + timeout
    try:
        for stream in (process.stdout, process.stderr):
            selector.register(stream, selectors.EVENT_READ)
            buffers[stream] = b""
        while selector.get_map() or process.poll() is None:
            if cancelled():
                raise DispatchError("worker cancelled")
            if time.monotonic() >= deadline:
                raise DispatchError("worker timed out")
            for key, _ in selector.select(0.1):
                stream = key.fileobj
                chunk = os.read(stream.fileno(), 65536)
                if not chunk:
                    selector.unregister(stream)
                    if buffers[stream] and stream is process.stdout:
                        raise DispatchError("incomplete worker JSONL record")
                    continue
                buffers[stream] += chunk
                if len(buffers[stream]) > 4 * 1024 * 1024:
                    raise DispatchError("worker record exceeds 4 MiB")
                while b"\n" in buffers[stream]:
                    line, buffers[stream] = buffers[stream].split(b"\n", 1)
                    if stream is process.stderr:
                        continue  # May contain credential-bearing provider diagnostics.
                    try:
                        event = json.loads(line)
                    except (ValueError, UnicodeError):
                        raise DispatchError("invalid worker JSONL") from None
                    if event.get("type") == "message_end" and event.get("message", {}).get("role") == "assistant":
                        message = event["message"]
                    if event.get("type") in {"turn_start", "tool_execution_start", "tool_execution_end"}:
                        output({"type": "progress", "event": event["type"]})
        if process.wait() != 0:
            raise DispatchError("worker subprocess failed")
        return final_message(message)
    finally:
        selector.close()
        # Kill the whole process group even if the leader exited with descendants alive.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        process.stdout.close()
        process.stderr.close()


def worker(private):
    request = json.loads((private / "request.json").read_text())
    config = request["config"]
    token = None
    interrupted = False
    def stop(_sig, _frame):
        nonlocal interrupted
        interrupted = True
    for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        signal.signal(sig, stop)
    def cancelled():
        return interrupted or (private / "cancel").exists()
    def output(event):
        with (private / "events.jsonl").open("a") as file:
            file.write(json.dumps(event) + "\n")
        # The pane shows progress without worker output or secrets.
        print(event["type"], flush=True)
    status = 1
    try:
        credential = oauth(config["auth_file"], request["timeout"])
        token = issue(config, private, request["role"])
        if cancelled():
            raise DispatchError("worker cancelled")
        state = private / "state"
        state.mkdir(mode=0o700)
        auth = state / "auth.json"
        auth.write_text(json.dumps({"openai": credential}))
        auth.chmod(0o600)
        env = {"PATH": os.environ["PATH"], "HOME": os.environ["HOME"],
               "FAB_WORKTREE": request["worktree"], "FAB_INPUT": str(private / "input"),
               "FAB_STATE": str(state), "FAB_PI_RUNTIME": config["pi_runtime"],
               "GH_TOKEN": token, "GH_REPO": config["repository"],
               "XDG_STATE_HOME": str(private / "sandbox-logs")}
        args = [config["sandbox"], "--mode", "json", "--print", "--no-session", "--no-approve",
                "--no-extensions", "--no-mcp", "--no-skills", "--no-context-files",
                "--no-prompt-templates", "--no-themes", "--model", request["model"],
                "--thinking", request["thinking"], "--tools", ",".join(request["tools"]),
                "--append-system-prompt", str(private / "input" / "role.md")]
        for skill in request["skills"]:
            args += ["--skill", str(private / "input" / "skills" / skill)]
        args += ["--", (private / "input" / "task.txt").read_text()]
        result = collect(args, env, request["worktree"], request["timeout"], output, cancelled)
        # Do not persist credentials even if a model repeats one in its final answer.
        for secret in (token, credential["access"], credential["refresh"]):
            result = result.replace(secret, "[redacted]")
        output({"type": "result", "text": result})
        status = 0
    except BaseException as error:
        output({"type": "error", "message": str(error) if isinstance(error, DispatchError)
                else "trusted worker launcher failed"})
    finally:
        if token:
            try:
                api("DELETE", "/installation/token", token)
            except BaseException:
                output({"type": "error", "message": "token revocation failed; token expires within one hour"})
                (private / "revocation-failed").touch()
                status = 1
        shutil.rmtree(private / "state", ignore_errors=True)
        if not (private / "revocation-failed").exists():
            (private / "cleanup-complete").touch()
        (private / "status").write_text(str(status))
    return status


def wait_for_cleanup(private, session, timeout=120):
    """Keep cancellation and issuer state until recorder exit and revocation."""
    (private / "cancel").touch()
    deadline = time.monotonic() + timeout
    while True:
        try:
            pane = subprocess.run(["tmux", "has-session", "-t", session],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                  timeout=5)
        except (OSError, subprocess.SubprocessError):
            break
        if pane.returncode == 1:
            if (private / "status").exists() and (private / "cleanup-complete").exists():
                return
            break
        if pane.returncode or time.monotonic() >= deadline:
            break
        time.sleep(0.1)
    raise DispatchError(f"recorder cleanup unconfirmed; private state retained at {private}")


@contextmanager
def private_directory(state_root):
    """Delete only after cleanup is confirmed, including on host deadline failure."""
    private = Path(tempfile.mkdtemp(prefix="dispatch-", dir=state_root))
    try:
        yield private
    finally:
        if not (private / "recorder-started").exists() or (private / "host-cleanup-complete").exists():
            shutil.rmtree(private)


def dispatch(args):
    root = args.trusted_checkout.resolve(strict=True)
    worktree = args.worktree.resolve(strict=True)
    config_path = args.config.resolve(strict=True)
    if config_path.is_relative_to(worktree):
        raise DispatchError("local issuer configuration must be outside worktree")
    config = load_config(config_path)
    if root == worktree or root.is_relative_to(worktree) or worktree.is_relative_to(root):
        raise DispatchError("trusted checkout and worktree must be separate")
    if not (worktree / ".git").is_file():
        raise DispatchError("worker requires a linked Git worktree")
    common = Path(run(["git", "-C", str(worktree), "rev-parse", "--path-format=absolute",
                       "--git-common-dir"]).decode().strip()).resolve(strict=True)
    # Shared Git metadata is writable too; never place issuer state there.
    for path in [config_path] + [Path(config[key]) for key in ("pi_runtime", "auth_file", "state_dir")]:
        if path.is_relative_to(worktree) or path.is_relative_to(common):
            raise DispatchError("host configuration overlaps worktree or shared Git metadata")
    if worktree == Path.home() or worktree in Path.home().parents:
        raise DispatchError("worktree exposes owner home")
    entry = role_config(root, args.role)
    tools, unmapped, body = role_input(root, args.role)
    emit({"type": "progress", "event": "unmapped tools: " + ", ".join(unmapped)})
    skills = ["review", "storm", "writing-docs"]
    if args.role == "reviser":
        skills += ["revise", "resync"]
    # Validate before copying, including all symlink targets in skill trees.
    for name in skills:
        source = trusted(root, f".claude/skills/{name}")
        for path in source.rglob("*"):
            if not path.resolve(strict=True).is_relative_to(root):
                raise DispatchError("skill escapes trusted checkout")
    state_root = Path(config["state_dir"])
    if state_root.stat().st_mode & 0o077:
        raise DispatchError("state_dir must be private (mode 0700)")
    with private_directory(state_root) as private:
        inputs = private / "input"
        inputs.mkdir(mode=0o700)
        (inputs / "task.txt").write_text(args.task.read_text())
        (inputs / "role.md").write_text(body + "\n\nNo GitHub MCP tools exist. Use gh. Skills are files to read.\n"
            "Never approve or merge. Never bypass repository rulesets. Do not load PR code as trusted configuration.\n"
            "Use the trusted Bot comment workflow. Queue acceptance is not posting: confirm the bot author and first line.\n"
            "Push over HTTPS with transient gh auth git-credential; never change shared Git configuration.\n"
            "Unmapped role tools (use gh or read the skill): " + ", ".join(unmapped) + "\n")
        (inputs / "skills").mkdir()
        for name in skills:
            shutil.copytree(trusted(root, f".claude/skills/{name}"), inputs / "skills" / name)
        request = {"config": config, "role": args.role, "worktree": str(worktree),
                   "timeout": args.timeout, "tools": tools, "skills": skills,
                   "model": entry["model"], "thinking": entry["thinking"]}
        (private / "request.json").write_text(json.dumps(request))
        # The recorder is trusted binary content, never a file from the PR worktree.
        source = Path(__file__).read_text() if "__file__" in globals() else sys.orig_argv[2]
        (private / "recorder.py").write_text(source)
        session = "fab-" + uuid.uuid4().hex
        command = [sys.executable, str(private / "recorder.py"), "--record", str(private)]
        (private / "recorder-started").touch()
        run(["tmux", "new-session", "-d", "-s", session, "-c", str(worktree), shlex.join(command)])
        interrupted = False
        def stop(_sig, _frame):
            nonlocal interrupted
            interrupted = True
            (private / "cancel").touch()
        previous = {sig: signal.signal(sig, stop) for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
        offset = 0
        deadline = time.monotonic() + args.timeout + 120
        try:
            while True:
                events = private / "events.jsonl"
                if events.exists():
                    with events.open() as file:
                        file.seek(offset)
                        for line in file:
                            emit(json.loads(line))
                        offset = file.tell()
                if (private / "status").exists():
                    # Drain the final record on the next iteration before exiting.
                    if events.exists() and events.stat().st_size != offset:
                        continue
                    return 1 if interrupted else int((private / "status").read_text())
                if time.monotonic() >= deadline:
                    raise DispatchError("host recorder deadline exceeded; inspect local sandbox logs")
                pane = subprocess.run(["tmux", "has-session", "-t", session],
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if pane.returncode:
                    # The status is committed before the pane exits.
                    if (private / "status").exists():
                        continue
                    raise DispatchError("worker pane exited without status")
                time.sleep(0.1)
        finally:
            try:
                wait_for_cleanup(private, session)
                (private / "host-cleanup-complete").touch()
            finally:
                for sig, handler in previous.items():
                    signal.signal(sig, handler)


def main():
    os.umask(0o077)
    if len(sys.argv) == 3 and sys.argv[1] == "--record":
        return worker(Path(sys.argv[2]))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("role", choices=sorted(ROLES))
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument("--worktree", required=True, type=Path)
    parser.add_argument("--trusted-checkout", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=1800)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 3000:
        parser.error("timeout must be 1..3000 seconds (installation tokens expire after one hour)")
    try:
        return dispatch(args)
    except DispatchError as error:
        emit({"type": "error", "message": str(error)})
        return 1
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        emit({"type": "error", "message": "dispatch failed; check trusted configuration and host tools"})
        return 1


if __name__ == "__main__":
    sys.exit(main())
