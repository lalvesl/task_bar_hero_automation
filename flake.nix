{
  description = "TBH Automation — background farming automation for TBH: Task Bar Hero on Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import ./nix/pkgs.nix { inherit nixpkgs system rust-overlay; };

        inherit
          (import ./nix/rust.nix {
            inherit pkgs;
            toolchainFile = ./rust-toolchain.toml;
          })
          rustToolchain
          rustPlatform
          ;

        x11Tools = import ./nix/x11-tools.nix pkgs;
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "tbh-automation";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs =
            with pkgs;
            [
              rustToolchain

              # Code quality
              cargo-deny
              cargo-nextest
              taplo
              nixfmt
              prettier
            ]
            ++ x11Tools;

          # The bot reads the isolated display cookie from XAUTHORITY, and the
          # binary cannot set it for itself: doing that from Rust needs
          # `unsafe`, which the workspace forbids. Exporting it here is what
          # makes a plain `cargo run -- run` work from the shell.
          #
          # The file need not exist yet. `tbh run` starts the display, which
          # writes it, before anything connects.
          shellHook = ''
            export XAUTHORITY="''${XDG_RUNTIME_DIR:-/tmp}/tbh-automation/Xauthority"
          '';
        };

        apps.fmt = {
          type = "app";
          program = "${
            pkgs.writeShellApplication {
              name = "fmt";
              runtimeInputs = with pkgs; [
                rustToolchain
                nixfmt
                taplo
                prettier
              ];
              text = ''
                nixfmt .
                cargo fmt --all
                taplo fmt
                prettier --write "**/*.md"
              '';
            }
          }/bin/fmt";
        };
      }
    );
}
