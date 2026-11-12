{
  description = "#966 fixture: a library a later GHC series ships is not compiler-supplied on an earlier one";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }: { };
}
