{
  description = "waybill fixture: a minimal Nix build, for the paths that need no declaration";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      # Every system the test suite might run on. `builtins.currentSystem`
      # is unavailable under pure evaluation -- which is how waybill
      # evaluates -- so a fixture that leans on it silently defines its
      # packages for the wrong platform and the closure query degrades with
      # `no-evaluable-attribute`. That reads as a broken feature rather than
      # a broken fixture.
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = nixpkgs.lib.genAttrs systems;
    in {
      # Deliberately plain. See README.md: a package defined here is not in
      # nixpkgs, so it can carry no nixpkgs declaration.
      packages = forAll (system: {
        default = nixpkgs.legacyPackages.${system}.stdenv.mkDerivation {
          pname = "waybill-fixture-root";
          version = "0.1.0";
          dontUnpack = true;
          installPhase = "mkdir -p $out";
        };
      });
    };
}
