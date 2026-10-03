{
  description = "Etta: a desktop companion for Halley, written in pleamar";

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
          etta = pkgs.callPackage ./nix/package.nix { pleamar = pleamar.packages.${system}.pleamar; };
          deriva-worker = pkgs.callPackage ./nix/deriva.nix { };
          etta-halley = pkgs.callPackage ./nix/halley.nix { };
          default = etta;
        }
      );

      # `nix run github:ely-id/etta -- start`
      apps = forAll (
        system: _pkgs: {
          default = {
            type = "app";
            program = "${self.packages.${system}.etta}/bin/etta";
          };
        }
      );

      overlays.default = final: _prev: {
        etta = final.callPackage ./nix/package.nix { pleamar = pleamar.packages.${final.stdenv.hostPlatform.system}.pleamar; };
      };
    };
}
