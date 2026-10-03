{
  description = "Marea: a desktop companion that lives at the top of your screen, written in pleamar";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    pleamar = {
      url = "github:k4ditano/pleamar";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      pleamar,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (
        system: pkgs: rec {
          marea = pkgs.callPackage ./nix/package.nix { pleamar = pleamar.packages.${system}.pleamar; };
          deriva-worker = pkgs.callPackage ./nix/deriva.nix { };
          marea-halley = pkgs.callPackage ./nix/halley.nix { };
          default = marea;
        }
      );

      # `nix run github:k4ditano/marea-plm -- start`
      apps = forAll (
        system: _pkgs: {
          default = {
            type = "app";
            program = "${self.packages.${system}.marea}/bin/marea";
          };
        }
      );

      overlays.default = final: _prev: {
        marea = final.callPackage ./nix/package.nix { pleamar = pleamar.packages.${final.stdenv.hostPlatform.system}.pleamar; };
      };
    };
}
