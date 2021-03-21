{ pkgs ? import <nixpkgs> { } }:
let rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
in pkgs.mkShell {
  name = "rust-shell";
  nativeBuildInputs = with pkgs; [
    rust
    llvm_11
    unstable.qemu
    spike
    python3
    dtc
    cargo-expand
    cargo-watch
    jq
  ];
}
