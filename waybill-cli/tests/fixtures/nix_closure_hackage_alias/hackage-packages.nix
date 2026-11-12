
  waybill-fixture-cabal_3_12_1_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-cabal";
      version = "3.12.1.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-cabal_3_14_2_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-cabal";
      version = "3.14.2.0";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
      libraryHaskellDepends = [ waybill-fixture-syntax ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-syntax_3_14_2_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-syntax";
      version = "3.14.2.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-hashable = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-hashable";
      version = "1.5.1.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-text ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-hashable_1_4_7_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-hashable";
      version = "1.4.7.0";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
      libraryHaskellDepends = [ waybill-fixture-old ];
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-text = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-text";
      version = "2.0.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-old = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-old";
      version = "1.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-ext = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-ext";
      version = "0.1.0.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-ext_0_1_1_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-ext";
      version = "0.1.1.0";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
      libraryHaskellDepends = [ waybill-fixture-text ];
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-stale_1_0_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-stale";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-cnative_2_0_0 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-cnative";
      version = "2.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
