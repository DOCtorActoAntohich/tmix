{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-filter.url = "github:numtide/nix-filter";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      nix-filter,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [
          (import rust-overlay)
        ];
        pkgs = import nixpkgs { inherit system overlays; };

        musl-pkgs = if pkgs.stdenv.isLinux then pkgs.pkgsMusl else pkgs;
        rust-stable = pkgs.rust-bin.stable."1.93.0".minimal;
        rust = {
          dev = rust-stable.override {
            extensions = [
              "rustfmt"
              "clippy"
              "rust-analyzer"
              "rust-src"
            ];
          };
          ci = rust-stable.override {
            extensions = [
              "rustfmt"
              "clippy"
            ];
          };
          build = rust-stable.override {
            targets = pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.pkgsMusl.stdenv.hostPlatform.rust.rustcTarget
            ];
          };
        };

        tmix = pkgs.callPackage ./nix/tmix.nix {
          inherit nix-filter;
          inherit (musl-pkgs) stdenv;
          rust-toolchain = rust.build;
        };
      in
      {
        formatter = pkgs.nixfmt-tree;

        packages = {
          inherit tmix;
          default = tmix;
        };

        devShells = {
          default = pkgs.mkShell {
            packages = [ rust.dev ];
          };
        };

        ci = {
          cargo-fmt = pkgs.writeShellApplication {
            name = "cargo-fmt";
            text = "cargo fmt --all --check";
            runtimeInputs = [
              rust.ci
            ];
          };
          cargo-clippy = pkgs.writeShellApplication {
            name = "cargo-clippy";
            text = "cargo clippy --workspace --tests --all-features -- -Dwarnings";
            runtimeInputs = [
              rust.ci
            ];
          };
          cargo-test = pkgs.writeShellApplication {
            name = "cargo-test";
            text = "cargo test --workspace --all-features";
            runtimeInputs = [
              rust.ci
            ];
          };
        };
      }
    );
}
