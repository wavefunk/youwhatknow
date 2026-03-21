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
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let
                baseName = builtins.baseNameOf path;
                relPath = pkgs.lib.removePrefix (toString ./.) (toString path);
              in
              # Only include files that affect the Rust build
              (type == "directory" && (
                baseName == "src" ||
                pkgs.lib.hasPrefix "/src" relPath
              )) ||
              baseName == "Cargo.toml" ||
              baseName == "Cargo.lock" ||
              baseName == "build.rs" ||
              pkgs.lib.hasSuffix ".rs" baseName;
          };
          cargoHash = "sha256-BOovGtXjqJ2ZEI1IQeF6SE/ZB5QNs8/br46mrpM/RqI=";

          nativeBuildInputs = with pkgs; [
            pkg-config
            makeWrapper
          ];

          buildInputs = with pkgs; [
            openssl
          ];

          postInstall = ''
            wrapProgram $out/bin/youwhatknow \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.claude-code ]}
          '';
        };
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
            ];

            buildInputs = [
              openssl
              pkg-config
              toolchain
            ];
          };
      }
    );
}
