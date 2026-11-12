
  waybill-fixture-ShellCheck = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-ShellCheck";
      version = "0.11.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-aeson waybill-fixture-fgl ];
      testHaskellDepends = [ waybill-fixture-hspec ];
      license = lib.licenses.gpl3Only;
  }) { };
  waybill-fixture-aeson = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-aeson";
      version = "2.2.4.1";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-text ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-text = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-text";
      version = "2.0.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd2;
  }) { };
  waybill-fixture-fgl = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-fgl";
      version = "5.8.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-hspec = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-hspec";
      version = "2.11.9";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-hunit ];
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-hunit = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-hunit";
      version = "1.6.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-bootlib = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-bootlib";
      version = "1.1.1";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-zexe = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-zexe";
      version = "1.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      executableHaskellDepends = [ waybill-fixture-zlib waybill-fixture-fsnotify ];
      testHaskellDepends = [ waybill-fixture-hspec ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-zlib = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-zlib";
      version = "0.7.1.1";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-fsnotify = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-fsnotify";
      version = "0.4.4.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-hinotify ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-hfsevents = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-hfsevents";
      version = "0.1.8";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
