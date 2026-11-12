
  waybill-fixture-lib = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-lib";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-clock waybill-fixture-gone ];
      license = lib.licenses.bsd3;
  }) { };
