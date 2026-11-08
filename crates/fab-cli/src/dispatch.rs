//! Trusted host orchestration for fresh, whole-process sandboxed Pi workers.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::Args;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod bridge;
pub(crate) mod operations;
mod recovery;
mod state;

use state::WorkerRecord;

const ROLES: &[&str] = &["implementer", "reviewer", "verifier", "adversarial-reviewer", "reviser"];
/// The roles whose commits the host pushes once their worker exits. Their
/// token, like every worker's, has Contents read and cannot push.
const WRITERS: &[&str] = &["implementer", "reviser"];
const GRACE: Duration = Duration::from_secs(120);
const RECORD_LIMIT: usize = 4 * 1024 * 1024;
/// What libsecret needs to find the session bus on Linux.
const SESSION_BUS: &[&str] = &["DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"];
/// Git settings for the host's push, on top of reading no configuration
/// the worker can write: no hooks or fsmonitor, HTTPS only, TLS verified, no
/// signing program, and only the push token as credential.
const PUSH_CONFIG: &[&str] = &[
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "core.askPass=",
    "protocol.allow=never",
    "protocol.https.allow=always",
    "http.sslVerify=true",
    "push.gpgSign=false",
    "credential.helper=",
];
/// The push's credential helper: it answers `get` for `github.com` alone,
/// with the token from `FAB_PUSH_TOKEN`, and answers nothing otherwise.
const CREDENTIAL_HELPER: &str = "f() { test \"$1\" = get || exit 0; ok=; while read -r l; do test \"$l\" = host=github.com && ok=1; done; test -n \"$ok\" && printf 'username=x-access-token\\npassword=%s\\n' \"$FAB_PUSH_TOKEN\"; }; f";
type Result<T> = std::result::Result<T, String>;

/// Explicit operator-selected dispatch inputs. No model or credential fallback.
#[derive(Args)]
pub struct Options {
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(ROLES))]
    role: String,
    #[arg(long)]
    task: PathBuf,
    #[arg(long)]
    worktree: PathBuf,
    #[arg(long)]
    trusted_checkout: PathBuf,
    #[arg(long)]
    config: PathBuf,
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..=3000))]
    timeout: u64,
    /// Return the worker identity once tmux accepts the launch.
    #[arg(long)]
    detach: bool,
}

#[derive(Serialize, Deserialize)]
struct Config {
    sandbox: PathBuf,
    pi_runtime: PathBuf,
    auth_file: PathBuf,
    state_dir: PathBuf,
    keychain_service: String,
    keychain_account: String,
    app_id_secret: String,
    installation_id_secret: String,
    pem_secret: String,
    repository: String,
}

#[derive(Serialize, Deserialize)]
struct Request {
    record: PathBuf,
    config: Config,
    role: String,
    worktree: PathBuf,
    timeout: u64,
    model: String,
    thinking: String,
    tools: Vec<String>,
    skills: Vec<String>,
    /// The caller's session bus variables. The recorder runs in a tmux pane,
    /// which takes the tmux server's environment rather than the caller's.
    #[serde(default)]
    session_bus: Vec<(String, String)>,
    /// Where the host pushes a writer's commits; `None` for other roles.
    #[serde(default)]
    push: Option<Push>,
}

/// A writer's branch, Git directory and object store, read on the host
/// before the worker starts, so the worker cannot redirect them.
#[derive(Debug, Serialize, Deserialize)]
struct Push {
    branch: String,
    /// Where the host reads the branch and head after the worker exits.
    git_dir: PathBuf,
    /// The object store the host's empty push repository borrows from.
    objects: PathBuf,
}

/// Owns a subprocess group; even a normally exited leader may leave children.
struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = killpg(Pid::from_raw(self.0.id() as i32), Signal::SIGKILL);
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Private state is retained unless the recorder has exited and revoked its token.
struct Private {
    path: PathBuf,
    started: bool,
}

impl Private {
    fn new(root: &Path) -> Result<Self> {
        let path = tempfile::Builder::new()
            .prefix("dispatch-")
            .tempdir_in(root)
            .map_err(|_| "cannot create private dispatch state")?
            .keep();
        Ok(Self { path, started: false })
    }
}

impl Drop for Private {
    fn drop(&mut self) {
        if !self.started {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Run dispatch and print only credential-free JSONL records.
pub fn run(options: Options) -> u8 {
    nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
    match dispatch(options) {
        Ok(code) => code,
        Err(error) => {
            emit(&json!({"type": "error", "message": error}));
            1
        }
    }
}

/// Hidden tmux recorder entry point, using this trusted executable, not PR code.
pub fn record(private: &Path) -> u8 {
    nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
    match read_json::<Request>(&private.join("request.json")) {
        Ok(request) => recorder(private, &request),
        Err(_) => 1,
    }
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|_| "cannot read trusted input".into())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_str(&read(path)?).map_err(|_| "invalid trusted JSON input".into())
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, bytes).map_err(|_| "cannot write private dispatch state".into())
}

fn touch(path: &Path) -> Result<()> {
    write(path, b"")
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|_| "missing local path".into())
}

fn trusted(root: &Path, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let path = canonical(&root.join(relative))?;
    if !path.starts_with(root) {
        return Err("trusted input escapes instruction checkout".into());
    }
    Ok(path)
}

fn role_input(root: &Path, role: &str) -> Result<(Vec<String>, Vec<String>, String)> {
    let text = read(&trusted(root, format!(".claude/agents/{role}.md"))?)?;
    let mut parts = text.splitn(3, "---");
    if parts.next().is_none_or(|s| !s.trim().is_empty()) {
        return Err("role has no frontmatter".into());
    }
    let front = parts.next().ok_or("role has no frontmatter")?;
    let body = parts.next().ok_or("role has no frontmatter")?.trim().to_string();
    let tools = front
        .lines()
        .find_map(|s| s.strip_prefix("tools: "))
        .ok_or("role has no tools")?;
    let mut mapped = BTreeSet::new();
    let mut unmapped = Vec::new();
    for tool in tools.split(',').map(str::trim) {
        let names: &[&str] = match tool {
            "Read" => &["read"],
            "Grep" => &["grep"],
            "Glob" => &["find", "ls"],
            "Bash" => &["bash"],
            "Write" => &["write"],
            "Edit" => &["edit"],
            _ => {
                unmapped.push(tool.to_string());
                &[]
            }
        };
        mapped.extend(names.iter().map(|s| s.to_string()));
    }
    if !mapped.contains("bash") || !mapped.contains("read") {
        return Err("role needs bash and read".into());
    }
    Ok((mapped.into_iter().collect(), unmapped, body))
}

/// The skills a role's worker gets as read-only input. The implementer gets
/// `implement` in place of `review`, and `arclightning` where the repository
/// commits an arcl workspace and carries that skill; the reviser adds
/// `revise` and `resync`.
fn role_skills(root: &Path, role: &str) -> Vec<String> {
    let mut skills: Vec<String> = match role {
        "implementer" => vec!["implement".into(), "storm".into(), "writing-docs".into()],
        _ => vec!["review".into(), "storm".into(), "writing-docs".into()],
    };
    if role == "implementer"
        && root.join(".arcl/workspace").is_dir()
        && root.join(".claude/skills/arclightning").is_dir()
    {
        skills.push("arclightning".into());
    }
    if role == "reviser" {
        skills.extend(["revise".into(), "resync".into()]);
    }
    skills
}

fn role_config(root: &Path, role: &str) -> Result<(String, String)> {
    let text = read(&trusted(root, ".github/fabrial-roles.toml")?)?;
    let config: toml::Value = toml::from_str(&text).map_err(|_| "invalid role configuration")?;
    let entry = config.get(role).and_then(|v| v.get("pi"));
    let model = entry
        .and_then(|v| v.get("model"))
        .and_then(toml::Value::as_str)
        .unwrap_or("");
    let thinking = entry
        .and_then(|v| v.get("thinking"))
        .and_then(toml::Value::as_str)
        .unwrap_or("");
    if !model
        .strip_prefix("openai/")
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)))
        || !["off", "minimal", "low", "medium", "high", "xhigh", "max"].contains(&thinking)
    {
        return Err(format!("missing or invalid Pi model/thinking for {role}"));
    }
    Ok((model.into(), thinking.into()))
}

fn load_config(path: &Path) -> Result<Config> {
    let mut c: Config = toml::from_str(&read(path)?).map_err(|_| "missing or invalid local configuration")?;
    if c.keychain_service.is_empty() || c.keychain_account.is_empty() {
        return Err("missing secret store configuration".into());
    }
    let repo: Vec<_> = c.repository.split('/').collect();
    if repo.len() != 2
        || repo
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)))
    {
        return Err("invalid repository".into());
    }
    for id in [&c.app_id_secret, &c.installation_id_secret, &c.pem_secret] {
        if uuid::Uuid::parse_str(id).is_err() || uuid::Uuid::parse_str(id).unwrap().to_string() != *id {
            return Err("secret IDs must be explicit UUIDs".into());
        }
    }
    for p in [&mut c.sandbox, &mut c.pi_runtime, &mut c.auth_file, &mut c.state_dir] {
        if !p.is_absolute() {
            return Err("local paths must be absolute".into());
        }
        *p = canonical(p)?;
    }
    if !c.sandbox.starts_with("/nix/store") {
        return Err("sandbox must be the built Nix store executable".into());
    }
    let home = home()?;
    if home.starts_with(&c.pi_runtime) || c.pi_runtime == home.join(".pi") {
        return Err("Pi runtime must be a dedicated installation".into());
    }
    validate_runtime(&c.pi_runtime)?;
    Ok(c)
}

fn validate_runtime(runtime: &Path) -> Result<()> {
    let package: Value = read_json(&runtime.join("node_modules/@earendil-works/pi-coding-agent/package.json"))?;
    if package["name"] != "@earendil-works/pi-coding-agent" || package["version"] != "1.4.0" {
        return Err("Pi runtime must be 1.4.0".into());
    }
    Ok(())
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("host HOME missing".into())
}

fn host_command(name: impl AsRef<std::ffi::OsStr>) -> Result<Command> {
    let mut c = Command::new(name);
    c.env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or("host PATH missing")?)
        .env("HOME", home()?);
    Ok(c)
}

/// Refuse a writer role unless the worktree is on an agent branch, and
/// return where the host pushes once the worker exits: that branch, the
/// worktree's Git directory and its object store. Other roles push nothing.
fn writer_branch(worktree: &Path, role: &str) -> Result<Option<Push>> {
    if !WRITERS.contains(&role) {
        return Ok(None);
    }
    let refused = || format!("{role} needs a worktree on a claude/, codex/ or storm/ branch");
    let git = |args: &[&str]| -> Result<String> {
        let mut git = host_command("git")?;
        git.arg("-C").arg(worktree).args(args);
        let bytes = execute(git, None, Duration::from_secs(30)).map_err(|_| refused())?;
        Ok(String::from_utf8(bytes).map_err(|_| refused())?.trim().to_string())
    };
    let branch = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if !crate::cleanup::is_agent(&branch) {
        return Err(refused());
    }
    let git_dir = canonical(Path::new(&git(&["rev-parse", "--absolute-git-dir"])?))?;
    let objects = git(&["rev-parse", "--path-format=absolute", "--git-path", "objects"])?;
    let objects = canonical(Path::new(&objects))?;
    Ok(Some(Push { branch, git_dir, objects }))
}

/// Push a writer's branch after its worker has exited, and return the head
/// pushed. It pushes only while the Git directory recorded at dispatch is
/// still on the writer's branch. The push itself runs from an empty bare
/// repository the host creates in its private state, whose only link to the
/// worktree is its object store: objects are data, so nothing the worker
/// wrote is read as config or run as a hook. Git refuses a push that does
/// not fast-forward the branch on origin. The token is minted for this push
/// alone, reaches Git through a credential helper scoped to `github.com`, and
/// is revoked after.
fn push(request: &Request, private: &Path) -> Result<String> {
    let target = request.push.as_ref().ok_or("writer has no recorded branch")?;
    let branch = &target.branch;
    let home = private_dir(&private.join("push-home"))?;
    let run = |mut git: Command, args: &[&str], token: Option<&str>| -> Result<(bool, String)> {
        git.args(args);
        if let Some(token) = token {
            git.env("FAB_PUSH_TOKEN", token);
        }
        let (status, bytes) = execute_outcome(git, None, Duration::from_secs(120))?;
        let text = String::from_utf8(bytes).map_err(|_| "invalid Git output")?;
        Ok((status.success(), text.trim().to_string()))
    };
    // These two local reads load the worktree's repository config, which
    // can run no hook, helper or transport for them. The sha read here is
    // the one pushed, whatever the worktree holds afterwards.
    let worktree = || push_git(&home, &target.git_dir);
    let current = run(worktree()?, &["symbolic-ref", "--quiet", "--short", "HEAD"], None)?;
    if !current.0 || current.1 != *branch {
        return Err(format!("not pushed: the worktree is no longer on {branch}"));
    }
    let (found, head) = run(worktree()?, &["rev-parse", "--verify", "HEAD^{commit}"], None)?;
    if !found {
        return Err(format!("not pushed: {branch} has no commit"));
    }
    let bare = private_dir(&private.join("push.git"))?;
    if !run(
        push_git(&home, &bare)?,
        &["init", "--quiet", "--bare", "--template="],
        None,
    )?
    .0
    {
        return Err("cannot create the push repository".into());
    }
    let info = bare.join("objects/info");
    fs::create_dir_all(&info).map_err(|_| "cannot create the push repository")?;
    write(&info.join("alternates"), format!("{}\n", target.objects.display()))?;
    let token = issue(
        &request.config,
        &request.session_bus,
        private,
        &json!({"contents": "write"}),
    )?;
    let url = format!("https://github.com/{}.git", request.config.repository);
    let refspec = format!("{head}:refs/heads/{branch}");
    let pushed = run(
        push_git(&home, &bare)?,
        &["push", "--porcelain", &url, &refspec],
        Some(&token),
    )
    .and_then(|(ok, report)| {
        if ok {
            return Ok(());
        }
        // `--porcelain` prints a refused ref as `!<TAB>src:dst<TAB>reason`.
        let reason = report
            .lines()
            .find(|l| l.starts_with('!'))
            .and_then(|l| l.rsplit('\t').next())
            .unwrap_or("the push failed");
        Err(format!("not pushed: origin refused {branch}: {reason}"))
    });
    let revoked = revoke(private, &token);
    pushed?;
    revoked?;
    Ok(head)
}

/// A Git command for `git_dir` that reads no system or global config: HOME
/// and XDG_CONFIG_HOME are `home`, an empty private directory, and the
/// host's push settings come last on the command line.
fn push_git(home: &Path, git_dir: &Path) -> Result<Command> {
    let mut git = host_command("git")?;
    git.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("--git-dir")
        .arg(git_dir)
        .args(PUSH_CONFIG.iter().flat_map(|c| ["-c", *c]))
        .arg("-c")
        .arg(format!("credential.helper=!{CREDENTIAL_HELPER}"));
    Ok(git)
}

/// Create a directory only the host user can enter.
fn private_dir(path: &Path) -> Result<PathBuf> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| "cannot create private push state")?;
    Ok(path.to_path_buf())
}

/// Capture both pipes concurrently, discard diagnostics, and impose a host-tool deadline.
fn execute(command: Command, data: Option<Vec<u8>>, timeout: Duration) -> Result<Vec<u8>> {
    let (status, bytes) = execute_outcome(command, data, timeout)?;
    if !status.success() {
        return Err("trusted subprocess failed".into());
    }
    Ok(bytes)
}

/// Recovery must distinguish an invalid token (HTTP 401) from transport failure.
fn execute_outcome(
    mut command: Command, data: Option<Vec<u8>>, timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = Process(command.spawn().map_err(|_| "trusted subprocess could not start")?);
    let mut stdin = child.0.stdin.take().unwrap();
    let input = thread::spawn(move || {
        if let Some(data) = data {
            let _ = stdin.write_all(&data);
        }
    });
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes);
        let _ = tx.send((result, bytes));
    });
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            return Err("trusted subprocess timed out".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|_| "trusted subprocess wait failed")? {
            let (result, bytes) = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "trusted subprocess pipe timed out")?;
            result.map_err(|_| "trusted subprocess read failed")?;
            if bytes.len() > 16 * 1024 * 1024 {
                return Err("trusted subprocess output too large".into());
            }
            let _ = input.join();
            return Ok((status, bytes));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn tool(name: &str, args: &[&str], data: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let mut c = host_command(name)?;
    c.args(args);
    execute(c, data, Duration::from_secs(30))
}

/// Send App JWT headers over stdin, never through arguments or files.
fn app_api(method: &str, path: &str, jwt: &str, body: Option<Value>) -> Result<Value> {
    let mut c = host_command("curl")?;
    c.args([
        "--disable",
        "--silent",
        "--show-error",
        "--fail",
        "--max-time",
        "25",
        "--request",
        method,
        "--header",
        "@-",
    ]);
    if let Some(body) = body {
        c.args(["--data-binary", &body.to_string()]);
    }
    c.arg(format!("https://api.github.com{path}"));
    let headers = format!(
        "Authorization: Bearer {jwt}\nAccept: application/vnd.github+json\nX-GitHub-Api-Version: 2022-11-28\nContent-Type: application/json\n"
    );
    let bytes =
        execute(c, Some(headers.into_bytes()), Duration::from_secs(25)).map_err(|_| "GitHub App request failed")?;
    serde_json::from_slice(&bytes).map_err(|_| "invalid GitHub App response".into())
}

/// gh receives installation tokens only through its environment.
fn api(method: &str, path: &str, token: &str, body: Option<Value>) -> Result<Value> {
    let mut c = host_command("gh")?;
    c.env("GH_TOKEN", token).args([
        "api",
        "--hostname",
        "github.com",
        "--method",
        method,
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "X-GitHub-Api-Version: 2022-11-28",
        path,
    ]);
    if body.is_some() {
        c.args(["--input", "-"]);
    }
    let bytes = execute(c, body.map(|v| v.to_string().into_bytes()), Duration::from_secs(25))
        .map_err(|_| "GitHub App request failed")?;
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_slice(&bytes).map_err(|_| "invalid GitHub App response".into())
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Reads the issuer's machine credential from the host's secret store: the
/// macOS Keychain through `security`, elsewhere the Secret Service through
/// libsecret's `secret-tool`, which finds the session bus through
/// `DBUS_SESSION_BUS_ADDRESS` or `XDG_RUNTIME_DIR`, as the caller of
/// `fab dispatch` had them. Workers cannot reach either: the sandbox denies
/// the Keychain services on macOS and every AF_UNIX socket, the session bus
/// included, on Linux.
fn machine_credential(c: &Config, session_bus: &[(String, String)]) -> Result<String> {
    let macos = cfg!(target_os = "macos");
    let (name, args) = credential_lookup(macos, &c.keychain_service, &c.keychain_account);
    let mut cmd = host_command(name)?;
    cmd.args(&args);
    if !macos {
        for (name, value) in session_bus.iter().filter(|(n, _)| SESSION_BUS.contains(&n.as_str())) {
            cmd.env(name, value);
        }
    }
    let bytes = execute(cmd, None, Duration::from_secs(30)).map_err(|_| "cannot read the issuer credential")?;
    let machine = String::from_utf8(bytes).map_err(|_| "invalid issuer credential")?;
    if machine.trim().is_empty() {
        return Err("empty issuer credential".into());
    }
    Ok(machine)
}

/// The session bus variables `get` holds, for the recorder's `secret-tool`.
fn session_bus(get: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    SESSION_BUS
        .iter()
        .filter_map(|name| get(name).map(|value| (name.to_string(), value)))
        .collect()
}

/// The command and arguments that print a secret store entry named by
/// service and account.
fn credential_lookup<'a>(macos: bool, service: &'a str, account: &'a str) -> (&'static str, Vec<&'a str>) {
    if macos {
        (
            "security",
            vec!["find-generic-password", "-s", service, "-a", account, "-w"],
        )
    } else {
        ("secret-tool", vec!["lookup", "service", service, "account", account])
    }
}

/// Mint an installation token with `permissions` for the configured
/// repository alone, persisting it as recovery state until `revoke`.
fn issue(c: &Config, session_bus: &[(String, String)], private: &Path, permissions: &Value) -> Result<String> {
    let machine = machine_credential(c, session_bus)?;
    let mut values = Vec::new();
    for id in [&c.app_id_secret, &c.installation_id_secret, &c.pem_secret] {
        let mut cmd = host_command("bws")?;
        cmd.env("BWS_ACCESS_TOKEN", machine.trim())
            .args(["secret", "get", id, "--output", "json"]);
        let value: Value = serde_json::from_slice(&execute(cmd, None, Duration::from_secs(30))?)
            .map_err(|_| "invalid BWS response")?;
        values.push(value["value"].as_str().ok_or("missing BWS value")?.to_string());
    }
    let (app, installation, pem) = (&values[0], &values[1], &values[2]);
    if [app, installation]
        .iter()
        .any(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err("App and installation IDs must be numeric".into());
    }
    let key = private.join("app.pem");
    write(&key, pem)?;
    let signing = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(b"{\"alg\":\"RS256\",\"typ\":\"JWT\"}"),
        URL_SAFE_NO_PAD.encode(json!({"iat": now().saturating_sub(60), "exp": now() + 540, "iss": app}).to_string())
    );
    let signature = tool(
        "openssl",
        &["dgst", "-sha256", "-sign", key.to_str().ok_or("invalid key path")?],
        Some(signing.as_bytes().to_vec()),
    );
    fs::remove_file(key).map_err(|_| "cannot remove issuer key")?;
    let jwt = format!("{signing}.{}", URL_SAFE_NO_PAD.encode(signature?));
    let installed = app_api("GET", &format!("/repos/{}/installation", c.repository), &jwt, None)?;
    if installed["id"].as_u64().map(|v| v.to_string()).as_ref() != Some(installation) {
        return Err("installation does not match repository".into());
    }
    let repo = c.repository.split('/').nth(1).unwrap();
    let response = app_api(
        "POST",
        &format!("/app/installations/{installation}/access_tokens"),
        &jwt,
        Some(json!({"repositories": [repo], "permissions": permissions})),
    )?;
    let token = response["token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("missing installation token")?
        .to_string();
    // Persist immediately so failed validation or later revocation retains recovery state.
    if write(&private.join("installation-token"), &token).is_err() {
        if api("DELETE", "/installation/token", &token, None).is_err() {
            let _ = touch(&private.join("revocation-failed"));
        }
        return Err("cannot persist installation token recovery state".into());
    }
    let checked = (|| {
        let mut expected = permissions.clone();
        expected["metadata"] = json!("read");
        if response["permissions"] != expected {
            return Err("unexpected installation token permissions".into());
        }
        let repos = api("GET", "/installation/repositories", &token, None)?;
        let list = repos["repositories"].as_array().ok_or("missing token repositories")?;
        if list.len() != 1
            || !list[0]["full_name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&c.repository))
        {
            return Err("installation token is not repository-restricted".into());
        }
        Ok(())
    })();
    if let Err(error) = checked {
        revoke(private, &token)?;
        return Err(error);
    }
    Ok(token)
}

fn revoke(private: &Path, token: &str) -> Result<()> {
    let mut command = host_command("gh")?;
    command.env("GH_TOKEN", token).args([
        "api",
        "--hostname",
        "github.com",
        "--include",
        "--method",
        "DELETE",
        "/installation/token",
    ]);
    let (status, bytes) = execute_outcome(command, None, Duration::from_secs(30))?;
    let response = std::str::from_utf8(&bytes).map_err(|_| "invalid revocation response")?;
    let mut header = response.lines().next().unwrap_or("").split_whitespace();
    if !header.next().is_some_and(|h| h.starts_with("HTTP/")) {
        return Err("revocation HTTP status missing".into());
    }
    match header.next() {
        Some("204") if status.success() => {}
        Some("401") => {} // Already revoked or expired: safe to finish interrupted cleanup.
        _ => return Err("credential revocation unconfirmed".into()),
    }
    fs::remove_file(private.join("installation-token")).map_err(|_| "cannot remove revoked token".into())
}

fn oauth(path: &Path, timeout: u64) -> Result<Value> {
    let auth: Value = read_json(path)?;
    let entry = &auth["openai"];
    if entry["type"] != "oauth"
        || !["access", "refresh"]
            .iter()
            .all(|k| entry[k].as_str().is_some_and(|s| !s.is_empty()))
        || entry["expires"].as_f64().unwrap_or(0.0) < ((now() + timeout + 60) * 1000) as f64
    {
        return Err("openai OAuth credential missing or expires before worker deadline; refresh on host".into());
    }
    let mut selected = serde_json::Map::new();
    for key in ["type", "access", "refresh", "expires", "accountId"] {
        if let Some(v) = entry.get(key) {
            selected.insert(key.into(), v.clone());
        }
    }
    Ok(Value::Object(selected))
}

fn redact(text: &str, token: &str, credential: &Value) -> String {
    let mut text = text.to_string();
    for secret in [
        Some(token),
        credential["access"].as_str(),
        credential["refresh"].as_str(),
    ]
    .into_iter()
    .flatten()
    {
        text = text.replace(secret, "[redacted]");
    }
    text
}

fn final_message(message: &Value) -> Result<String> {
    if message["stopReason"] != "stop" || message.get("errorMessage").is_some_and(|v| !v.is_null() && v != "") {
        return Err("worker has no successful final assistant message".into());
    }
    let text = message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err("worker returned no final text".into());
    }
    Ok(text)
}

fn emit(event: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{event}");
    let _ = out.flush();
}

fn output(directory: &Path, event: &Value) -> Result<()> {
    let path = directory.join("events.jsonl");
    let existed = path.exists();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|_| "cannot open recorder events")?;
    writeln!(file, "{event}").map_err(|_| "cannot write recorder event")?;
    file.sync_data().map_err(|_| "cannot sync recorder event")?;
    if !existed {
        state::sync_directory(directory)?;
    }
    println!("{}", event["type"].as_str().unwrap_or("progress"));
    Ok(())
}

/// Read raw LF-delimited bytes; Unicode line separators are text, not framing.
fn records(reader: impl Read, tx: mpsc::SyncSender<Result<Value>>) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = Vec::new();
        let result = reader
            .by_ref()
            .take((RECORD_LIMIT + 1) as u64)
            .read_until(b'\n', &mut line);
        match result {
            Ok(0) => break,
            Ok(_) if line.len() > RECORD_LIMIT => {
                let _ = tx.send(Err("worker record exceeds 4 MiB".into()));
                break;
            }
            Ok(_) if !line.ends_with(b"\n") => {
                let _ = tx.send(Err("incomplete worker JSONL record".into()));
                break;
            }
            Ok(_) => {
                let event = serde_json::from_slice(&line).map_err(|_| "invalid worker JSONL".into());
                let failed = event.is_err();
                if tx.send(event).is_err() || failed {
                    break;
                }
            }
            Err(_) => {
                let _ = tx.send(Err("worker pipe read failed".into()));
                break;
            }
        }
    }
}

fn signals() -> Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
    ] {
        signal_hook::flag::register(sig, flag.clone()).map_err(|_| "cannot install cancellation handler")?;
    }
    Ok(flag)
}

fn recorder(private: &Path, request: &Request) -> u8 {
    let mut worker = match WorkerRecord::open(&request.record).and_then(|mut worker| {
        worker.start(private)?;
        Ok(worker)
    }) {
        Ok(worker) => worker,
        Err(error) => {
            emit(&json!({"type": "error", "message": error}));
            return 1;
        }
    };
    let interrupted = match signals() {
        Ok(flag) => flag,
        Err(_) => return 1,
    };
    let directory = worker.directory().to_path_buf();
    let identity = match worker.snapshot() {
        Ok(identity) => identity,
        Err(_) => return 1,
    };
    let deadline = Instant::now() + Duration::from_secs(request.timeout);
    let cancelled = || interrupted.load(Ordering::Relaxed) || directory.join("cancel").exists();
    let mut result = (|| {
        if !ROLES.contains(&request.role.as_str()) {
            return Err("invalid recorder role".into());
        }
        let credential = oauth(&request.config.auth_file, request.timeout)?;
        // Every worker reads the repository, runs the Bot comment workflow
        // and reads pull requests. None can push; the host pushes for writers.
        let permissions = json!({"actions": "write", "contents": "read", "pull_requests": "read"});
        let token = issue(&request.config, &request.session_bus, private, &permissions)?;
        // Issuance can outlive cancellation or the deadline. Never launch in that case.
        if cancelled() || Instant::now() >= deadline {
            return Err("worker cancelled or deadline exceeded during issuance".into());
        }
        let state = private.join("state");
        fs::create_dir(&state).map_err(|_| "cannot create worker state")?;
        write(&state.join("auth.json"), json!({"openai": credential}).to_string())?;
        let mut command = host_command(&request.config.sandbox)?;
        command
            .current_dir(&request.worktree)
            .env("FAB_WORKTREE", &request.worktree)
            .env("FAB_INPUT", private.join("input"))
            .env("FAB_STATE", &state)
            .env("FAB_PI_RUNTIME", &request.config.pi_runtime)
            .env("GH_TOKEN", &token)
            .env("GH_REPO", &request.config.repository)
            .env("XDG_STATE_HOME", private.join("sandbox-logs"))
            .args([
                "--mode",
                "rpc",
                "--no-session",
                "--no-approve",
                "--no-extensions",
                "--no-mcp",
                "--no-skills",
                "--no-context-files",
                "--no-prompt-templates",
                "--no-themes",
                "--extension",
            ])
            .arg(private.join("input/worker.ts"))
            .args([
                "--model",
                &request.model,
                "--thinking",
                &request.thinking,
                "--tools",
                &format!("{},worker_report", request.tools.join(",")),
                "--append-system-prompt",
            ])
            .arg(private.join("input/role.md"));
        for skill in &request.skills {
            command.arg("--skill").arg(private.join("input/skills").join(skill));
        }
        let task = read(&private.join("input/task.txt"))?;
        let text = bridge::collect(
            command,
            deadline.saturating_duration_since(Instant::now()),
            cancelled,
            |e| {
                if e["type"] == "worker_started" {
                    worker.worker_started(e["pid"].as_u64().ok_or("worker PID missing")? as u32)
                } else {
                    let mut event = e;
                    if event["type"] == "worker_report" {
                        event["text"] = json!(redact(event["text"].as_str().unwrap_or(""), &token, &credential));
                        worker.report(&event)?;
                    } else if event["type"] == "progress" {
                        worker.progress(&event)?;
                    }
                    output(&directory, &event)?;
                    if event["type"] == "control_processed" {
                        let id = event["id"].as_str().ok_or("control receipt ID missing")?;
                        let id = uuid::Uuid::parse_str(id).map_err(|_| "invalid control receipt UUID")?;
                        let controls = directory.join("controls");
                        fs::remove_file(controls.join(format!("{id}.sent")))
                            .map_err(|_| "cannot retire processed control")?;
                        state::sync_directory(&controls)?;
                    }
                    Ok(())
                }
            },
            || bridge::controls(&directory, &identity),
            &task,
        )?;
        let text = redact(&text, &token, &credential);
        worker.result(&text)?;
        if WRITERS.contains(&request.role.as_str()) {
            // The worker has exited: retire its token before minting the push token.
            revoke(private, &token)?;
            let head = push(request, private)?;
            let branch = request.push.as_ref().map_or("", |p| p.branch.as_str());
            output(&directory, &json!({"type": "pushed", "branch": branch, "head": head}))?;
        }
        output(&directory, &json!({"type": "result", "text": text}))
    })();
    let cancellation_observed = cancelled();
    let deadline_exceeded = Instant::now() >= deadline;
    let mut status = u8::from(result.is_err());
    if let Err(error) = &result {
        let _ = output(&directory, &json!({"type": "error", "message": error}));
    }
    let cleanup = (|| {
        if private.join("revocation-failed").exists() {
            return Err("token revocation unconfirmed".to_string());
        }
        if private.join("installation-token").exists() {
            let token = read(&private.join("installation-token"))?;
            revoke(private, &token)?;
        }
        if private.join("state").exists() {
            fs::remove_dir_all(private.join("state")).map_err(|_| "cannot remove worker credentials")?;
        }
        state::discard_controls(&directory)?;
        Ok(())
    })();
    if cleanup.is_err() {
        status = 1;
        let _ = output(
            &directory,
            &json!({"type": "error", "message": "credential cleanup unconfirmed; private state retained"}),
        );
    } else if (|| {
        state::atomic_write(&directory.join("cleanup-owned"), private.as_os_str().as_encoded_bytes())?;
        fs::remove_dir_all(private).map_err(|_| "cannot remove temporary state")?;
        state::sync_directory(private.parent().ok_or("private state has no parent")?)
    })()
    .is_err()
    {
        result = Err("temporary state cleanup unconfirmed; evidence retained".into());
        status = 1;
        let _ = output(
            &directory,
            &json!({"type": "error", "message": result.as_ref().unwrap_err()}),
        );
    }
    if worker
        .finish(&result, cleanup.is_ok(), cancellation_observed, deadline_exceeded)
        .is_err()
    {
        status = 1;
        let _ = output(
            &directory,
            &json!({"type": "error", "message": "worker record unconfirmed; durable evidence retained"}),
        );
    }
    status
}

fn copy_tree(root: &Path, source: &Path, destination: &Path) -> Result<()> {
    let source = canonical(source)?;
    if !source.starts_with(root) {
        return Err("skill escapes trusted checkout".into());
    }
    fs::create_dir(destination).map_err(|_| "cannot copy trusted skill")?;
    for entry in fs::read_dir(source).map_err(|_| "cannot read trusted skill")? {
        let entry = entry.map_err(|_| "cannot read trusted skill")?;
        let path = canonical(&entry.path())?;
        if !path.starts_with(root) {
            return Err("skill escapes trusted checkout".into());
        }
        let dest = destination.join(entry.file_name());
        if path.is_dir() {
            copy_tree(root, &path, &dest)?;
        } else {
            fs::copy(path, dest).map_err(|_| "cannot copy trusted skill")?;
        }
    }
    Ok(())
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn pane(session: &str) -> Result<bool> {
    let mut command = host_command("tmux")?;
    command
        .args(["has-session", "-t", session])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = Process(command.spawn().map_err(|_| "cannot inspect recorder pane")?);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().map_err(|_| "cannot inspect recorder pane")? {
            return match status.code() {
                Some(0) => Ok(true),
                Some(1) => Ok(false),
                _ => Err("cannot inspect recorder pane".into()),
            };
        }
        if Instant::now() >= deadline {
            return Err("recorder pane inspection timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn dispatch(options: Options) -> Result<u8> {
    let root = canonical(&options.trusted_checkout)?;
    let worktree = canonical(&options.worktree)?;
    let config_path = canonical(&options.config)?;
    if config_path.starts_with(&worktree) {
        return Err("local issuer configuration must be outside worktree".into());
    }
    let config = load_config(&config_path)?;
    if root.starts_with(&worktree) || worktree.starts_with(&root) {
        return Err("trusted checkout and worktree must be separate".into());
    }
    if !worktree.join(".git").is_file() {
        return Err("worker requires a linked Git worktree".into());
    }
    let mut git = host_command("git")?;
    git.arg("-C")
        .arg(&worktree)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let common =
        String::from_utf8(execute(git, None, Duration::from_secs(30))?).map_err(|_| "invalid Git metadata path")?;
    let common = canonical(Path::new(common.trim()))?;
    for path in [&config_path, &config.pi_runtime, &config.auth_file, &config.state_dir] {
        if path.starts_with(&worktree) || path.starts_with(&common) {
            return Err("host configuration overlaps worktree or shared Git metadata".into());
        }
    }
    if home()?.starts_with(&worktree) {
        return Err("worktree exposes owner home".into());
    }
    let push = writer_branch(&worktree, &options.role)?;
    let (model, thinking) = role_config(&root, &options.role)?;
    let (tools, unmapped, body) = role_input(&root, &options.role)?;
    emit(&json!({"type": "progress", "event": format!("unmapped tools: {}", unmapped.join(", "))}));
    let skills = role_skills(&root, &options.role);
    if fs::metadata(&config.state_dir)
        .map_err(|_| "cannot inspect state directory")?
        .permissions()
        .mode()
        & 0o077
        != 0
    {
        return Err("state_dir must be private (mode 0700)".into());
    }
    let interrupted = signals()?;
    let mut private = Private::new(&config.state_dir)?;
    let p = &private.path;
    fs::create_dir(p.join("input")).map_err(|_| "cannot create trusted input directory")?;
    fs::create_dir(p.join("input/skills")).map_err(|_| "cannot create trusted skill directory")?;
    write(
        &p.join("input/worker.ts"),
        include_str!("../../../packages/pi/worker.ts"),
    )?;
    write(&p.join("input/task.txt"), read(&options.task)?)?;
    write(
        &p.join("input/role.md"),
        format!(
            "{body}\n\nYou start in your worktree, the directory FAB_WORKTREE names; it is the directory the run named. Make no worktree of your own.\nNo GitHub MCP tools exist. Use gh. Skills are files to read.\nNever approve or merge. Never bypass repository rulesets. Do not load PR code as trusted configuration.\nUse worker_report immediately for blockers or questions; those reports end this worker and return control to the driver.\nUse the trusted Bot comment workflow. Queue acceptance is not posting: confirm the bot author and first line.\n{}Never change shared Git configuration.\nUnmapped role tools (use gh or read the skill): {}\n",
            if push.is_some() {
                "Commit your work on your branch and do not push: your token cannot push. Once you exit, the host pushes the branch if it fast-forwards origin. A commit's SHA is the one that lands.\n"
            } else {
                ""
            },
            unmapped.join(", ")
        ),
    )?;
    for name in &skills {
        copy_tree(
            &root,
            &trusted(&root, format!(".claude/skills/{name}"))?,
            &p.join("input/skills").join(name),
        )?;
    }
    let mut git = host_command("git")?;
    git.arg("-C").arg(&worktree).args(["rev-parse", "--verify", "HEAD"]);
    let task_head = String::from_utf8(execute(git, None, Duration::from_secs(30))?)
        .map_err(|_| "invalid task head")?
        .trim()
        .to_string();
    if ![40, 64].contains(&task_head.len()) || !task_head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid task head".into());
    }
    let worker = WorkerRecord::create(
        &config.state_dir,
        &options.role,
        &config.repository,
        &worktree,
        task_head,
        p,
    )?;
    let request = Request {
        record: worker.path().into(),
        config,
        role: options.role,
        worktree: worktree.clone(),
        timeout: options.timeout,
        model,
        thinking,
        tools,
        skills,
        session_bus: session_bus(|name| std::env::var(name).ok()),
        push,
    };
    write(
        &p.join("request.json"),
        serde_json::to_vec(&request).map_err(|_| "cannot serialize recorder request")?,
    )?;
    let binary = std::env::current_exe().map_err(|_| "cannot locate trusted fab executable")?;
    if binary.starts_with(&worktree) || binary.starts_with(&common) {
        return Err("fab executable must be outside worker worktree and Git metadata".into());
    }
    let session = worker.session();
    let command = format!(
        "{} dispatch-record {}",
        quote(binary.to_str().ok_or("invalid binary path")?),
        quote(p.to_str().ok_or("invalid state path")?)
    );
    let mut tmux = host_command("tmux")?;
    tmux.args(["new-session", "-d", "-s", session, "-c"])
        .arg(&worktree)
        .arg(command);
    if interrupted.load(Ordering::Relaxed) {
        return Err("dispatch cancelled before launch".into());
    }
    private.started = true; // From here the recorder owns cleanup, even if this caller exits.
    if let Err(error) = execute(tmux, None, Duration::from_secs(30)) {
        worker.cancel()?;
        return Err(error);
    }
    if options.detach {
        if interrupted.load(Ordering::Relaxed) {
            worker.cancel()?;
            return Err("dispatch cancelled during launch".into());
        }
        emit(&json!({"type": "started", "worker": worker.snapshot()?}));
        Ok(0)
    } else {
        follow(&worker, options.timeout, &interrupted)
    }
}

fn follow(worker: &WorkerRecord, timeout: u64, interrupted: &AtomicBool) -> Result<u8> {
    let error = match observe(worker, timeout, interrupted) {
        Ok(code) => return Ok(code),
        Err(error) => error,
    };
    // Synchronous callers own cancellation on observation failure. Detached
    // dispatch and explicit `worker wait` deliberately do not use this path.
    let requested = worker.cancel().is_ok();
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        if WorkerRecord::open(worker.path()).is_ok_and(|current| current.terminal()) {
            return Err(error);
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "{error}; cancellation recorded: {requested}; cleanup unconfirmed: {}",
        worker.path().display()
    ))
}

fn observe(worker: &WorkerRecord, timeout: u64, interrupted: &AtomicBool) -> Result<u8> {
    let deadline = Instant::now() + Duration::from_secs(timeout) + GRACE;
    let mut offset = 0;
    let events = worker.directory().join("events.jsonl");
    loop {
        let current = WorkerRecord::open(worker.path())?;
        if interrupted.load(Ordering::Relaxed) && !current.terminal() {
            current.cancel()?;
        }
        if events.exists() {
            let bytes = fs::read(&events).map_err(|_| "cannot read recorder events")?;
            while let Some(end) = bytes[offset..].iter().position(|b| *b == b'\n') {
                let event: Value =
                    serde_json::from_slice(&bytes[offset..offset + end]).map_err(|_| "invalid recorder event")?;
                emit(&event);
                offset += end + 1;
            }
        }
        if current.terminal() && !pane(worker.session())? {
            if fs::metadata(&events).is_ok_and(|m| m.len() as usize != offset) {
                continue;
            }
            return Ok(if interrupted.load(Ordering::Relaxed) { 1 } else { current.exit_code() });
        }
        if Instant::now() >= deadline {
            return Err("host recorder deadline exceeded".into());
        }
        if !pane(worker.session())? && !WorkerRecord::open(worker.path())?.terminal() {
            return Err("worker pane exited without terminal record; recovery state retained".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests;
