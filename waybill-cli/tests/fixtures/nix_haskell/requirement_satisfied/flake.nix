{
  description = "waybill fixture — single compiler package set";
  outputs = { self, nixpkgs }: {
    packages.x86_64-linux.default =
      nixpkgs.legacyPackages.x86_64-linux.haskell.packages.ghc96.waybill-fixture-app;
  };
}
