{
  description = "waybill fixture — one compiler series, so the boot-library
    union needs exactly one configuration-ghc-*.nix (#975).";
  outputs = { self, nixpkgs }: {
    packages.x86_64-linux.default =
      nixpkgs.legacyPackages.x86_64-linux.haskell.packages.ghc96.waybill-fixture-licproj;
  };
}
