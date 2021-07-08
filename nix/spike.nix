{ lib, stdenv, fetchFromGitHub, dtc, nixosTests, fetchpatch }:

stdenv.mkDerivation rec {
  pname = "spike";
  version = "master";

  src = fetchFromGitHub {
    owner = "riscv";
    repo = "riscv-isa-sim";
    rev = "cc38be9991f3abd0831d141ebff8b4fd7a4990ea";
    sha256 = "sha256-REtvCRFmv1XZPCMO5yxGUJcLWfnlr8zh76QnYa9CjD4=";
  };

  nativeBuildInputs = [ dtc ];
  enableParallelBuilding = true;

  RISCV_ENABLE_DIRTY = 1;

  postPatch = ''
    patchShebangs scripts/*.sh
    patchShebangs tests/ebreak.py
  '';

  doCheck = true;

  passthru.tests = { can-run-hello-world = nixosTests.spike; };
}
