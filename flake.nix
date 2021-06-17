{
  description = "NovOS shell with all required tools";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        pkgsRiscv = import nixpkgs {
          inherit system;
          crossSystem.config = "riscv64-none-elf";
          currentSystem = system;
        };
      in
      {
        devShell =
          let
            rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
            opensbi = pkgsRiscv.callPackage ./nix/opensbi.nix { };
            spike = pkgs.callPackage ./nix/spike.nix { };
          in
          pkgs.mkShell {
            buildInputs = with pkgs; [
              rust

              llvm_11
              qemu
              python3
              dtc
              jq

              cargo-expand
              cargo-watch

              spike
            ];

            OPENSBI = "${opensbi}";
          };
      }
    );
}
