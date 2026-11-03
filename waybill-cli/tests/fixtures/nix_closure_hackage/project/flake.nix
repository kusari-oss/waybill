{
  description = "milestone 1275 fixture: a flake whose closure holds Hackage packages";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }: { };
}
