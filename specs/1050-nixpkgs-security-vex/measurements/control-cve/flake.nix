{
  description = "waybill validation control: a build that accepted an insecure package";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs {
        inherit system;
        config.permittedInsecurePackages = [ "dcraw-9.28.0" ];
        config.allowUnfree = true;
      };
    in {
      packages = forAll (system:
        let pkgs = pkgsFor system; in {
          default = pkgs.stdenv.mkDerivation {
            pname = "waybill-cve-control";
            version = "0.1.0";
            dontUnpack = true;
            buildInputs = [ pkgs.dcraw ];
            installPhase = "mkdir -p $out";
          };
        });
    };
}
