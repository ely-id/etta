{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "marea-halley";
  version = "0.1.0";
  src = lib.cleanSource ../halley;
  cargoLock = {
    lockFile = ../halley/Cargo.lock;
    allowBuiltinFetchGit = true;
  };
  meta = {
    description = "Halley API adapter for Marea";
    license = lib.licenses.gpl3Only;
    mainProgram = "marea-halley";
    platforms = lib.platforms.linux;
  };
}
