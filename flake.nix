{
  description = "youwhatknow — Claude Code hook server for file summaries";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config.allowUnfree = true;
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        beads-latest = (pkgs.beads.override {
          buildGoModule = pkgs.buildGoModule.override { go = pkgs.go_1_26; };
        }).overrideAttrs (old: rec {
          version = "0.60.0";
          src = pkgs.fetchFromGitHub {
            owner = "steveyegge";
            repo = "beads";
            rev = "v${version}";
            hash = "sha256-z3EDtaBHB3ltPRT7vuBFURD7UwgAJBXAPozRnkjejeU=";
          };
          vendorHash = "sha256-1BJsEPP5SYZFGCWHLn532IUKlzcGDg5nhrqGWylEHgY=";
          doCheck = false;
        });

        youwhatknow = pkgs.rustPlatform.buildRustPackage {
          pname = "youwhatknow";
          version = "0.0.1";
          src = ./.;
          cargoHash = "sha256-ugGZbHuU7ZQ15U+wS5ACzbs7j0ipyKOiTFPl1I1GKUI=";

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
          ];
        };

        # Shell hook script that starts youwhatknow in the background
        # if claude is on PATH and the server isn't already running.
        # The server auto-shuts down after idle_shutdown_minutes of inactivity,
        # handling the direnv unload case. PID file + EXIT trap handle shell exit.
        youwhatknowHook = ''
          if command -v claude &>/dev/null && command -v youwhatknow &>/dev/null; then
            _ywk_pidfile="$PWD/.claude/summaries/youwhatknow.pid"

            # Kill stale instance if PID file exists but process is dead
            if [ -f "$_ywk_pidfile" ]; then
              _ywk_old_pid=$(cat "$_ywk_pidfile" 2>/dev/null)
              if [ -n "$_ywk_old_pid" ] && kill -0 "$_ywk_old_pid" 2>/dev/null; then
                # Already running, nothing to do
                return 0 2>/dev/null || true
              else
                rm -f "$_ywk_pidfile"
              fi
            fi

            echo "Starting youwhatknow hook server..."
            youwhatknow &>/dev/null &
            _ywk_pid=$!
            disown

            # Clean up on shell exit (handles terminal close)
            trap "kill $_ywk_pid 2>/dev/null; rm -f '$_ywk_pidfile'" EXIT
          fi
        '';
      in
      {
        packages = {
          default = youwhatknow;
          inherit youwhatknow;
        };

        devShells.default =
          with pkgs;
          mkShell {
            packages = [
              nil
              just
              cargo-expand
              bacon
              claude-code
              dolt
              beads-latest
              cargo-dist
              youwhatknow
            ];

            buildInputs = [
              openssl
              pkg-config
              toolchain
            ];

            shellHook = youwhatknowHook;
          };
      }
    );
}
