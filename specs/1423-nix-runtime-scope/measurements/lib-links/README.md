# #1455, second rule: references that exist only through darwin's `lib/links`

Collected 2026-11-12 on aarch64-darwin, `nix (Determinate Nix 3.20.0) 2.34.6`,
against moat at `d0690555` (the `nix-closure-moat` corpus pin), whose `out`
was already in the local store. Nothing was built or substituted. The
runtime-closure listing and derivations are the ones
`../doc-outputs/collect.sh` writes.

**The decision (maintainer, option 1 on #1455):** on darwin, a reference that
exists only because of a member's `lib/links` symlink folder (the Haskell
generic builder's link farm) does not count as shipping its target. The hard
constraint: a dylib that a Mach-O file in a shipped output actually loads
still ships.

## Scripts

| script | output | what it does |
|---|---|---|
| `probe.sh <outdir> <store path>…` | `out/moat-out.{macho,otool,lib-links}` | each output's files, its Mach-O files and their `otool -L`, and its `lib/links` entries |
| `analyze.py <pathinfo.json> <drvs.json> <root>…` | `out/analysis.txt` | classifies every reference from an output with a `lib/links` folder, applies the candidate rule with the documentation rule, and checks no loaded library is dropped. `WHY=<pkg>,…` explains what keeps a package |
| `dyld.py <store path>…` | `out/dyld-moat.txt` | the store paths dyld loads starting from every Mach-O file in the outputs, resolving `@rpath/` through each image's and its loaders' `LC_RPATH` |

`out/why-moat-{hspec,vector}.txt` are `nix why-depends --precise --all` from
moat's `out`; `out/otool-extra.txt` holds the `otool` evidence quoted below.

## What `lib/links` is, on moat

- moat's `out` is a library: no `bin/`, one Mach-O file,
  `libHSmoat-0.1-…-ghc9.10.3.dylib` (`out/moat-out.macho`). Its first
  `LC_RPATH` is `$out/lib/links`.
- Its `lib/links` holds 75 symlinks (`out/moat-out.lib-links`), among them 33
  Haskell dylibs: hspec, hspec-core, hspec-discover, hspec-expectations,
  hspec-golden, HUnit, QuickCheck, quickcheck-io, tasty, and so on.
- The dylib loads (`out/moat-out.otool`) 3 of those 33: th-abstraction,
  cmark-gfm and haddock-library, plus GHC's boot libraries from GHC's own
  directory and `libSystem`.
- dyld, starting from that dylib and following every image's `@rpath/`
  through its own and its loaders' rpaths, loads images from 7 store paths
  (`out/dyld-moat.txt`): moat, th-abstraction, haddock-library, cmark-gfm,
  ghc, gmp-with-cxx, libiconv.
- `nix why-depends --precise --all` reaches hspec from moat only through
  `lib/links` symlinks: moat's own, haddock-library's, th-compat's,
  hspec-golden's, vector's, unordered-containers' (`out/why-moat-hspec.txt`).
- moat's declared library dependencies that its dylib does not load (vector,
  primitive, unordered-containers, …) are named in
  `nix-support/propagated-build-inputs`, which is not `lib/links`
  (`out/otool-extra.txt`, the vector line).

**How GHC links the executables.** moat has none. The two Haskell
executables in its runtime closure, `hspec-discover` and `doctest`, load no
Haskell dylib at all: `otool -L` lists only `libSystem`, `libiconv`, `libgmp`
and `libffi`, by absolute path (`out/otool-extra.txt`). On this revision
nixpkgs links Haskell executables on darwin statically against the Haskell
libraries, so `lib/links` is not how an executable finds them. Libraries
statically linked into a runtime executable are what the existing
static-link set (#1423) keeps.

## Telling a loaded farm entry from an unloaded one

For an output `O` with `lib/links` and each reference `O → T`
(`out/analysis.txt`):

- 2,172 references: `T`'s hash appears in `O` outside `lib/links` (a
  Mach-O's absolute install name, an rpath, `package.conf.d`,
  `propagated-build-inputs`, …). They stand.
- 1,179 references: only `lib/links` symlinks point into `T`. They stand only
  when the symlink's name is the last component of an install name some
  Mach-O file in a shipping output loads. dyld searches the rpaths of every
  image in the load chain, so the loaded names are pooled over all shipping
  outputs and the computation repeats until nothing more ships.
- No reference was found in neither place.

Result, combined with the documentation rule:

- shipping paths: 169 → 68; 101 set aside;
- of the 23 test-framework packages, 22 are set aside. **tasty is kept,
  correctly**: vector's `out` installs an internal library,
  `libHSvector-0.13.2.0-…-benchmarks-O2-ghc9.10.3.dylib`, whose
  `LC_LOAD_DYLIB` names `@rpath/libHStasty-1.5.4-…dylib`
  (`out/otool-extra.txt`). That is a Mach-O file in a shipped output loading
  tasty, so the hard constraint keeps it;
- every library dyld loads from moat's dylib is kept
  (`lib/links targets loaded by a shipping output but dropped: none`, and
  none of the 7 paths in `out/dyld-moat.txt` is set aside).

## The waybill binary on moat, before and after

Both scans ran as the corpus harness runs them
(`../doc-outputs/moat-scan.sh`), each followed by
`sbom edit --drop 'scope=excluded'`. Before: the documentation rule only
(#1475). After: this branch. Outputs are in `out/scan-{before,after}.txt` and
`out/marked-{before,after}.txt`.

| | before | after |
|---|---|---|
| C216 `lib-links-excluded` | (absent) | `{"members":101,"not-assessed":null,"paths":101,"rule":"unloaded-lib-links"}` |
| SPDX 2.3 `BUILD_TOOL_OF` members | 669 | 771 |
| test-framework packages marked (of 23) | 0 | 22, all but tasty |
| runtime view, SPDX 2.3 / SPDX 3: test-framework packages | 23 / 23 | 1 / 1 (tasty) |
| runtime view, CycloneDX: test-framework packages | 23 | 9 |
| runtime view, SPDX 2.3 packages | 183 | 82 |
| rule cost (`nix-closure: lib/links references`, `elapsed_ms`) | — | 1,798 |

- The 102 new marks are the 101 packages `analyze.py` sets aside, the same
  names, plus `os-string 2.0.10`. os-string is not in the runtime closure. It
  was kept only as statically linked into a Haskell member the rule now sets
  aside, so it is marked as well. It is not counted in `members`, which counts
  members whose own runtime paths the rule set aside.
- None of the 7 store paths dyld loads from moat's dylib is marked. The one
  marked `libiconv` is GNU libiconv 1.19, not the `libiconv-113` dyld loads.
- CycloneDX keeps 8 marked test frameworks in its runtime view: hspec,
  hspec-core, hspec-discover, hspec-expectations, hspec-golden, HUnit,
  QuickCheck, quickcheck-io. They are `scope: excluded` and carry
  `waybill:lifecycle-scope=development`, and `sbom edit`'s CycloneDX selector
  leaves such a component alone, because one field carries both marks
  (`docs/user-guide/sbom-edit.md`, the CycloneDX limit). SPDX keeps the two
  marks apart and drops them.
- `static-link-roots` goes from `{"declared-edges":91,"derivation-fields":33}`
  to `{"declared-edges":0,"derivation-fields":23}`. The roots are the
  Haskell members that ship, and those 23 are not identity-confirmed by the
  package set.

The first run of the binary found 8 universal (`0xcafebabe`) files from
apple-sdk in the closure, six of them static archives whose slices are `ar`
archives, and reported `not-assessed: unparsed-mach-o`. Archive slices load
nothing at run time, so they are now skipped. A slice that is a Mach-O image
still has to parse.

## Where the rule is not assessed

- **A substituter answered.** Its files are not in the local store, so which
  file holds a reference cannot be told. Every reference counts, and C216
  says `not-assessed: remote-store`.
- **A shipping output, or one with a farm, cannot be read**, or **a Mach-O
  file's load commands cannot be parsed**: what it loads is unknown, so
  nothing is set aside (`unreadable-output`, `unparsed-mach-o`).
- **Linux** has no `lib/links`. When no runtime output has the folder, the
  rule reads no file and changes nothing.
