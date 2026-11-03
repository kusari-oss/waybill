{
  description = "waybill fixture — one compiler series, so the boot-library
    union needs exactly one configuration-ghc-*.nix. Without this file
    `candidate_series` returns every KNOWN_GHC_SERIES, and an offline cache
    miss on ANY of them degrades the whole pass (#975) -- which is how the
    first version of this fixture made the US2 tests pass vacuously.";
  outputs = { self, nixpkgs }: {
    packages.x86_64-linux.default =
      nixpkgs.legacyPackages.x86_64-linux.haskell.packages.ghc96.waybill-fixture-flagproj;
  };
}
