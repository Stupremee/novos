# Nix Derivation for building OpenSBI.
# Output directory has following structure:
#
# platform/fw_{dynamic,jump,payload}.elf

{ platform ? "generic", pkgs ? import <nixpkgs> { crossSystem.config = "riscv64-none-elf"; } }:
let
  inherit (pkgs) stdenv fetchFromGitHub;

  version = "master";
in stdenv.mkDerivation rec {
  name = "opensbi";
  inherit version;

  src = fetchFromGitHub {
    owner = "riscv";
    repo = name;
    rev = "50d4fde1c5a4ceb063d7f9a402769fb5be6d59ad";
    sha256 = "sha256-RjxtcbxpK3ow1Xp2lA5ygA5EsxyOFt0LixEmYcPMWMs=";
  };

  PLATFORM = platform;
  installPhase = ''
    mkdir -p $out/platform/
    mv ./build/platform/${platform}/firmware/{*.elf,*.bin} $out/platform
  '';
}
