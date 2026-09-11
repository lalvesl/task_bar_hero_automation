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
