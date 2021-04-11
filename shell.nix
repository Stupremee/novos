{ pkgs ? import <nixpkgs> { } }:
let
  pkgsRiscv = import <nixpkgs> { crossSystem.config = "riscv64-none-elf"; };

  rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
  rustfmt = pkgs.rust-bin.stable.latest.rustfmt;

  spike = pkgs.callPackage ./nix/spike.nix { };
  pk = pkgsRiscv.callPackage ./nix/pk.nix { };
in pkgs.mkShell {
  name = "rust-shell";
  nativeBuildInputs = with pkgs; [
    rust
    rustfmt

    llvm_11
    unstable.qemu
    python3
    dtc
    cargo-expand
    cargo-watch
    jq

    spike
    pk
  ];
}
