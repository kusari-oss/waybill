
  waybill-fixture-lib = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-lib";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ base ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-dep = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-dep";
      version = "1.2.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ base stm ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-boottool = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-boottool";
      version = "2.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ base stm ];
      license = lib.licenses.bsd3;
  }) { };
  # #966 US2: a deliberately unreal version, so no committed fixture names a
  # real coordinate of a library some GHC ships.
  os-string = callPackage ({ mkDerivation }: mkDerivation {
      pname = "os-string";
      version = "0.0.0.966";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
