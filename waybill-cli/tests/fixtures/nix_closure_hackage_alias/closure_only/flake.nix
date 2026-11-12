{
  description = "#1282 fixture: closure members the pinned package set holds only under a versioned alias";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }: { };
}
