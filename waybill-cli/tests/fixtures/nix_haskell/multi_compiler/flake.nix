{
  description = "waybill fixture — three candidate compiler package sets";
  outputs = { self, nixpkgs }:
    let p = nixpkgs.legacyPackages.x86_64-linux; in {
      packages.x86_64-linux = {
        ghc94 = p.haskell.packages.ghc94.waybill-fixture-app;
        ghc96 = p.haskell.packages.ghc96.waybill-fixture-app;
        ghc910 = p.haskell.packages.ghc910.waybill-fixture-app;
      };
    };
}
