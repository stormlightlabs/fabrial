# Build on the trusted host with: nix-build nix/worker.nix
# No credential values or personal paths are evaluated into the store.
let
  nixpkgs = builtins.fetchTree {
    type = "github";
    owner = "NixOS";
    repo = "nixpkgs";
    rev = "7a0f122f5090cf4c2ade2a13a0e229d4e19ba71f";
    narHash = "sha256-ZoxIApko70jCdbH3l20HWXOBaT2HZd87orzd2yJ9dVE=";
  };
  pkgs = import nixpkgs.outPath { };
  upstream = builtins.fetchGit {
    url = "https://github.com/archie-judd/agent-sandbox.nix.git";
    rev = "bda0de5679017097f3a46f32b9d6b35dfd9b6327";
    allRefs = true;
  };
  patched = pkgs.runCommand "fabrial-agent-sandbox-source" { nativeBuildInputs = [ pkgs.patch ]; } ''
    cp -R ${upstream} "$out"
    chmod -R u+w "$out"
    cd "$out"
    patch -p1 < ${./deny-owner-keychain.patch}
  '';
  sandbox = import patched { inherit pkgs; };
  worker = pkgs.writeShellScriptBin "fab-pi" ''
    exec ${pkgs.nodejs}/bin/node "$FAB_PI_RUNTIME/node_modules/@earendil-works/pi-coding-agent/dist/cli.js" "$@"
  '';
in sandbox.mkSandbox {
  pkg = worker;
  binName = "fab-pi";
  outName = "fab-pi-sandboxed";
  allowedPackages = sandbox.commonTools ++ [
    pkgs.nodejs pkgs.gh pkgs.cargo pkgs.rustc pkgs.clippy
    pkgs.rustfmt pkgs.stdenv.cc pkgs.just
  ];
  workspaceDir = "$FAB_WORKTREE";
  roDirs = [ "$FAB_INPUT" "$FAB_PI_RUNTIME" ];
  rwDirs = [ "$FAB_STATE" ];
  allowNix = false;
  allowUnixSockets = false;
  allowedHostPorts = [ ];
  publishedPorts = [ ];
  allowedDomains = {
    "api.github.com" = [ "GET" "POST" ];
    "github.com" = [ "GET" "POST" ];
    "crates.io" = [ "GET" ];
    "registry.npmjs.org" = [ "GET" ];
    "api.openai.com" = "*";
    "auth.openai.com" = [ "GET" "POST" ];
  };
  env = {
    FAB_PI_RUNTIME = "$FAB_PI_RUNTIME";
    PI_CODING_AGENT_DIR = "$FAB_STATE";
    PI_SKIP_VERSION_CHECK = "1";
    PI_TELEMETRY = "0";
    CARGO_TARGET_DIR = "$FAB_WORKTREE/target";
    GH_TOKEN = "$GH_TOKEN";
    GH_REPO = "$GH_REPO";
    GIT_CONFIG_NOSYSTEM = "1";
    GIT_CONFIG_GLOBAL = "/dev/null";
    GIT_AUTHOR_NAME = "stormlight-thunderbot[bot]";
    GIT_AUTHOR_EMAIL = "stormlight-thunderbot[bot]@users.noreply.github.com";
    GIT_COMMITTER_NAME = "stormlight-thunderbot[bot]";
    GIT_COMMITTER_EMAIL = "stormlight-thunderbot[bot]@users.noreply.github.com";
  };
}
