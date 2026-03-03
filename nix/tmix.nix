{
  lib,
  stdenv,
  makeRustPlatform,

  nix-filter,
  rust-toolchain,
}:
let
  rust-platform = makeRustPlatform {
    cargo = rust-toolchain;
    rustc = rust-toolchain;
    inherit stdenv;
  };

  find-package =
    {
      cargo-lock,
      name,
    }:
    let
      string = builtins.readFile cargo-lock;
      lock = builtins.fromTOML string;
      package = lib.lists.findFirst (p: p.name == name) null lock.package;
    in
    assert (package != null) || throw "Package `${name}` not found in Cargo.lock";
    package;

  package = find-package {
    cargo-lock = ../Cargo.lock;
    name = "tmix";
  };
in
rust-platform.buildRustPackage {
  inherit (package) version;
  name = package.name;

  src = nix-filter {
    root = ../.;
    include = [
      "src"
      "Cargo.lock"
      "Cargo.toml"
    ];
  };
  cargoBuildFlags = [
    "--package"
    package.name
  ];
  doCheck = false;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };
}
