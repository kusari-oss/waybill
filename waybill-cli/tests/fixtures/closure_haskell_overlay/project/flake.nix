{
  description = "#1046 fixture: a Haskell project whose overlays pin versions the nixpkgs package set does not carry";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }: { };
}
