# Nix Derivation for building OpenSBI.
# Output directory has following structure:
#
# platform/fw_{dynamic,jump,payload}.elf

{ platform ? "generic", pkgs }:
let
  inherit (pkgs) stdenv fetchFromGitHub;

  version = "master";
in
stdenv.mkDerivation rec {
  name = "opensbi";
  inherit version;

  src = fetchFromGitHub {
    owner = "riscv";
    repo = name;
    rev = "79f9b4220ffa7f74356054be25d450d7958bf16c";
    sha256 = "sha256-otbS2IXsVitjfWXf6XcPxLS2R3o2ZplcGrjo3HFXI1A=";
  };

  # If this is yes, then there's a gcc error about `-fPIC` not being valid
  FW_PIC = "n";

  PLATFORM = platform;
  installPhase = ''
    mkdir -p $out/platform/
    mv ./build/platform/${platform}/firmware/{*.elf,*.bin} $out/platform
  '';
}
