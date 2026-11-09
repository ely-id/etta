# Etta: her scene, her logic and her things, with the pleamar that runs her
# and the programs she asks for (finding files, screenshots, the clipboard…).
{
  lib,
  stdenvNoCC,
  makeWrapper,
  glibc,
  pleamar,
  fd,
  grim,
  slurp,
  wl-clipboard,
  wf-recorder,
  imagemagick,
  libnotify,
  networkmanager,
  bluez,
  pulseaudio,
  brightnessctl,
  xdg-utils,
  xdg-user-dirs,
  swaybg,
  procps,
  nodejs,
  callPackage,
}:
let
  deriva = callPackage ./deriva.nix { };
  halley = callPackage ./halley.nix { };
in
stdenvNoCC.mkDerivation {
  pname = "etta";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ../.;
    # Not the README's pictures nor the measurements' evidence.
    filter = path: _type: !(lib.hasInfix "/assets" path) && !(lib.hasInfix "/evidencia" path);
  };

  nativeBuildInputs = [ makeWrapper ];

  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share/etta $out/bin
    cp -r . $out/share/etta
    # Her launcher finds her folder by where it really is, so it is called
    # there, with the pleamar that runs her and what she asks for on its PATH.
    makeWrapper $out/share/etta/etta $out/bin/etta \
      --set-default PLEAMAR ${pleamar}/bin/pleamar \
      --prefix PATH : ${
        lib.makeBinPath [
          pleamar
          glibc.bin
          fd
          grim
          slurp
          wl-clipboard
          wf-recorder
          imagemagick
          libnotify
          networkmanager
          bluez
          pulseaudio
          brightnessctl
          xdg-utils
          xdg-user-dirs
          swaybg
          procps
          nodejs
          deriva
          halley
        ]
      }
    runHook postInstall
  '';

  meta = {
    description = "A desktop companion that lives at the top of your screen, written in pleamar";
    homepage = "https://github.com/ely-id/etta";
    license = lib.licenses.bsd3;
    mainProgram = "etta";
    platforms = lib.platforms.linux;
  };
}
