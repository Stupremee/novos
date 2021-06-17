{ lib, stdenv, fetchFromGitHub, dtc, nixosTests, fetchpatch }:

stdenv.mkDerivation rec {
  pname = "spike";
  version = "master";

  src = fetchFromGitHub {
    owner = "riscv";
    repo = "riscv-isa-sim";
    rev = "218777888c952c0846f8d186ac664dcd26c33c79";
    sha256 = "sha256-55UsAlyWu3+2/eP88wi+gwPi8PniVNt46nT2KbDb+Ag=";
  };

  nativeBuildInputs = [ dtc ];
  enableParallelBuilding = true;

  postPatch = ''
    patchShebangs scripts/*.sh
    patchShebangs tests/ebreak.py
  '';

  doCheck = true;

  passthru.tests = { can-run-hello-world = nixosTests.spike; };
}
