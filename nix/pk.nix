{ lib, stdenv, fetchFromGitHub, autoreconfHook, gcc10 }:

stdenv.mkDerivation rec {
  pname = "riscv-pk";
  version = "master";

  src = fetchFromGitHub {
    owner = "riscv";
    repo = "riscv-pk";
    rev = "ef7bebaf9bf24d3e90bcaae96387ce418e136b6d";
    sha256 = "sha256-wULIZurSgU/97g788WoS2QAFDUiZz1pwcmq/8TfBfcA=";
  };

  nativeBuildInputs = [ autoreconfHook ];

  preConfigure = ''
    mkdir build
    cd build
  '';

  configureScript = "../configure";

  hardeningDisable = [ "all" ];

  postInstall = ''
    mv $out/* $out/.cleanup
    mv $out/.cleanup/* $out
    rmdir $out/.cleanup
  '';
}
