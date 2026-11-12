{
  description = "#966 fixture: no manifest, so every boot-library version comes from the closure tier";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }: { };
}
