# Deriva: Marea's library of what is dropped on her (a local SQLite, the
# files kept by their hash). Her page for it asks this program.
{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "deriva-worker";
  version = "0.1.0";
  src = ../deriva;
  cargoLock.lockFile = ../deriva/Cargo.lock;
  meta = {
    description = "Marea's local library: keeps what is dropped on her, and finds it again";
    homepage = "https://github.com/k4ditano/marea-plm";
    license = lib.licenses.mit;
    mainProgram = "deriva-worker";
    platforms = lib.platforms.linux;
  };
}
