{
  description = "spike wrapper: nixpkgs#openssh at a799d3e3886da994fa307f817a6bc705ae538eeb";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb";
  outputs = { self, nixpkgs }:
    let systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
    in { packages = nixpkgs.lib.genAttrs systems (s: { default = nixpkgs.legacyPackages.${s}.openssh; }); };
}
