{
  description = "waybill fixture: a minimal Nix build, for the paths that need no declaration";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = builtins.currentSystem or "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      # Deliberately plain. See README.md: a package defined here is not in
      # nixpkgs, so it can carry no nixpkgs declaration, and pretending
      # otherwise would test a path that cannot exist.
      packages.${system}.default = pkgs.stdenv.mkDerivation {
        pname = "waybill-fixture-root";
        version = "0.1.0";
        dontUnpack = true;
        installPhase = "mkdir -p $out";
      };
    };
}
