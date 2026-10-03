# Deriva: Etta's library of what is dropped on her (a local SQLite, the
# files kept by their hash). Her page for it asks this program.
{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "deriva-worker";
  version = "0.1.0";
  src = ../deriva;
  cargoLock.lockFile = ../deriva/Cargo.lock;
  meta = {
    description = "Etta's local library: keeps what is dropped on her, and finds it again";
    homepage = "https://github.com/ely-id/etta";
    license = lib.licenses.mit;
    mainProgram = "deriva-worker";
    platforms = lib.platforms.linux;
  };
}
