{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "etta-halley";
  version = "0.1.0";
  src = lib.cleanSource ../halley;
  cargoLock = {
    lockFile = ../halley/Cargo.lock;
    allowBuiltinFetchGit = true;
  };
  meta = {
    description = "Halley API adapter for Etta";
    license = lib.licenses.gpl3Only;
    mainProgram = "etta-halley";
    platforms = lib.platforms.linux;
  };
}
