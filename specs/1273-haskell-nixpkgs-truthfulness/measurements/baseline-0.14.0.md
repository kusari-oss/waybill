# Milestone 1273 — pre-change baseline

Captured at `2ff788be` on 2026-11-03T01:01:12Z, BEFORE any
implementation task touched a reader.

FR-016 requires every corrected fixture be justified "against a measurement
rather than against this spec". A baseline captured *after* the change has
already lost that comparison, so this file is T003 and runs first.

Binary: `waybill 0.14.0`

```text

════ Probe B — license shapes ════
revision: a799d3e3886da994fa307f817a6bc705ae538eeb
file:     /Users/mlieberman/.cache/waybill/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb/hackage-packages.nix (16634427 bytes)

=== denominators ===
  '= callPackage' attribute blocks           19253
  'license = ' declarations                  19437
  of those, single-line (ending in ';')      19437
  multi-line forms (no ';' on the line)      0
  'with lib.licenses; [ ... ]' list forms    0
  bare list forms 'license = [ ... ]'        0

=== the three families (must sum to the declaration count) ===
  lib.licenses.<attr>                        14481
  lib.meta.getLicenseFromSpdxId "<id>"       3962
  a bare string                              994

=== family 1: the whole lib.licenses attribute vocabulary ===
    (the size of the mapping table FR-010 needs)
  9069 bsd3
  3314 mit
   661 gpl3Only
   389 asl20
   313 publicDomain
   152 gpl2Only
   136 bsd2
   127 lgpl21Only
   105 lgpl3Only
    79 mpl20
    79 agpl3Only
    51 isc
     3 bsdOriginal
     2 lgpl2Only
     1 lgpl21Plus

=== family 2: distinct SPDX ids (pass-through, no mapping) ===
  0BSD AGPL-3.0-only AGPL-3.0-or-later Apache-2.0 BSD-2-Clause-Patent 
  BSD-2-Clause BSD-3-Clause CC0-1.0 EUPL-1.2 GPL-2.0-only 
  GPL-2.0-or-later GPL-3.0-only GPL-3.0-or-later ISC LGPL-2.0-or-later 
  LGPL-2.1-only LGPL-2.1-or-later LGPL-3.0-only LGPL-3.0-or-later MIT 
  MPL-2.0 Unlicense Zlib 
=== family 3: every bare-string value (FR-011's population) ===
   438 "GPL"
   360 "unknown"
   143 "LGPL"
    11 "(BSD-2-Clause OR Apache-2.0)"
     8 "(Apache-2.0 OR MPL-2.0)"
     6 "(AGPL-3.0-only WITH Universal-FOSS-exception-1.0 OR LicenseRef-commercial)"
     5 "AGPL"
     4 "(BSD-3-Clause OR Apache-2.0)"
     3 "GPL-2.0-or-later AND BSD-3-Clause"
     3 "(AGPL-3.0-only WITH Universal-FOSS-exception-1.0 OR AGPL-3.0-only OR LicenseRef-commercial)"
     2 "BSD-3-Clause AND GPL-3.0-or-later"
     2 "BSD-3-Clause AND GPL-2.0-or-later"
     2 "(Apache-2.0 OR BSD-3-Clause)"
     1 "MPL-2.0 AND BSD-3-Clause"
     1 "GPL-3.0-or-later AND BSD-3-Clause"
     1 "(SSPL-1.0 OR AGPL-3.0-only)"
     1 "(MIT OR Apache-2.0)"
     1 "(CC0-1.0 OR BSD-2-Clause)"
     1 "(Apache-2.0 OR MIT)"
     1 "(AGPL-3.0-only WITH Universal-FOSS-exception-1.0 OR AGPL-3.0-only OR LicenseRef-commercial) AND BSD-3-Clause"
──── Probe B — license shapes: PASS

════ Probe C — the four issues ════
=== #1278 — hpack-only project ===
  hackage components: 0   (package.yaml declares 3)
  all components:     []
  graph-completeness: complete
  per_reader_dispatch_counts "haskell": 1
  diagnostic naming package.yaml: ABSENT  <-- the defect

=== #1281 — case A: .cabal only (the mark should hold) ===
  aeson @ - | scope=excluded | lifecycle=optional | derivation=cabal-flag-disabled-stanza
  async @ - | scope=excluded | lifecycle=optional | derivation=cabal-flag-disabled-stanza

=== #1281 — case B: + flake.lock pinning nixpkgs (the mark is withdrawn) ===
  aeson @ 2.2.4.1 | scope=None | lifecycle=None | derivation=None
  async @ 2.2.6 | scope=None | lifecycle=None | derivation=None
  the seed that justifies the withdrawal:
    runtime closure resolved declared=5 transitive=62
    restored=2
    (declared=5 CONTAINS aeson and async, so the closure reaches them
     by construction -- nothing in the library reaches either)

=== #1280 (licenses) and #1279 (unresolved-reason) ===
  hackage components:                      66
  #1280  carrying a license:               0/66
  #1279  carrying waybill:unresolved-reason: 4/66
  #1279  RESOLVED *and* still unresolved:  2   <-- the contradiction
           aeson @ 2.2.4.1 :: declared in .cabal / package.yaml; no cabal.project.freeze fallback
           async @ 2.2.6 :: declared in .cabal / package.yaml; no cabal.project.freeze fallback
──── Probe C — the four issues: PASS

════ Probe D — SC-003 public library ════
subject:  the slack-web Haskell library (a corpus subject) @ 7b46273
waybill:  waybill 0.14.0
nixpkgs:  5ae3b07d8d6527c42f17c876e404993199144b6a

=== case A: no flake.lock ===
=== case B: flake.lock pinning nixpkgs ===
  runtime closure resolved declared=45 transitive=146
  restored=2

=== the disagreement ===
  hackage components:  A=44  B=190
  classified optional: A=2  B=0
  optional in A, NOT optional in B: 2
    butcher                      A=optional/cabal-flag-disabled-stanza -> B=runtime/- @ 1.3.3.2
    monad-loops                  A=optional/cabal-flag-disabled-stanza -> B=runtime/- @ 0.4.3
  DIRECTLY marked in A (cabal-flag-disabled-stanza): 2 ['butcher', 'monad-loops']
  of those, still marked in B: []

=== the fan-out below the marked pair ===
    (re-measures the figure #1281 quoted as '8 components in total')
  nixpkgs revision read for boot names:      5ae3b07d8d6527c42f17c876e404993199144b6a
  hackage components:                        190
  reachable from the root:                   190
  reachable without traversing the marked:   179
  reachable ONLY through the marked:         11
    the marked components themselves:  2 ['butcher', 'monad-loops']
    non-boot fan-out below them:       8 ['clock', 'deque', 'extra', 'microlens', 'microlens-th', 'multistate', 'strict-list', 'unsafe']
    compiler-supplied (boot):          1 ['pretty']
      ^ present in every build of the project; not attributable
  ==> the defect's population: 10 components
──── Probe D — SC-003 public library: PASS

════ Probe A — hpack semantics ════
hpack: hpack version 0.38.3

──── c1-toplevel-inheritance ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c1
    hs-source-dirs:
        src
    build-depends:
        base
      , text
  
  executable cli
    main-is: Main.hs
    other-modules:
        Paths_waybill_fixture_c1
    hs-source-dirs:
        app
    build-depends:
        base
      , text
  
  test-suite spec
    type: exitcode-stdio-1.0
    main-is: Main.hs
    other-modules:
        Paths_waybill_fixture_c1
    hs-source-dirs:
        app
    build-depends:
        base
      , text
  
  benchmark bench
    type: exitcode-stdio-1.0
    main-is: Main.hs
    other-modules:
        Paths_waybill_fixture_c1
    hs-source-dirs:
        app
    build-depends:
        base
      , text

──── c2-percomponent-merge ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c2
    hs-source-dirs:
        src
    build-depends:
        base
      , containers
  
  executable cli
    main-is: Main.hs
    other-modules:
        Paths_waybill_fixture_c2
    hs-source-dirs:
        app
    build-depends:
        aeson
      , base

──── c3-constrained-then-bare ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c3
    hs-source-dirs:
        src
    build-depends:
        base

──── c4-bare-then-constrained ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c4
    hs-source-dirs:
        src
    build-depends:
        base ==4.*

──── c5-two-constraints ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c5
    hs-source-dirs:
        src
    build-depends:
        text <3

──── c6-duplicate-in-one-list ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c6
    hs-source-dirs:
        src
    build-depends:
        text <3

──── c7-when-literal-true ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c7
    hs-source-dirs:
        src
    build-depends:
        base
    if true
      build-depends:
          unix

──── c8-when-literal-false ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c8
    hs-source-dirs:
        src
    build-depends:
        base

──── c9-when-undecidable ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c9
    hs-source-dirs:
        src
    build-depends:
        base
    if os(darwin)
      build-depends:
          unix
    if flag(fast)
      build-depends:
          vector

──── c10-source-dirs ────────────────────────────────────────────
  library
    exposed-modules:
        Lib
        Other
    other-modules:
        Paths_waybill_fixture_c10
    hs-source-dirs:
        src
        other
    build-depends:
        base

──── c11-defaults ────────────────────────────────────────────
  hpack REFUSED the manifest:
    Invalid value for "defaults"! File https://raw.githubusercontent.com/waybill-fixture/does-not-exist/main/.hpack/defaults.yaml does not exist!

──── c12-include-sibling ────────────────────────────────
  library
    exposed-modules:
        Lib
    other-modules:
        Paths_waybill_fixture_c12
    hs-source-dirs:
        src
    build-depends:
        base
      , text
──── Probe A — hpack semantics: PASS

════ summary ════
all probes that ran reproduced their finding
```
