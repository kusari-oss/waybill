# The invariant cells the IFD targets reported

The public-corpus run that first scanned slack-web, slacklinker and openai
with `--nix-closure --nix-allow-ifd` (x86_64-linux, each document C216
`method: runtime-closure`, `static-link-source: derivation-fields`) reported
undeclared cells on I2, I3, I9 and I10. Counts below are read off those
emitted documents with the harness's own checkers, not re-derived.

| cell | slack-web | slacklinker | openai | cause | action |
|---|---|---|---|---|---|
| I10 `closure-statically-linked-but-marked` | 15 | 9 | 2 | the static-link fallback followed `buildInputs`, which with `doCheck` holds a package's test suite | fixed: `closure/static_link.rs` |
| I3 `unmarked-non-runtime-reachable` | 31 | 15 | 2 | the checker read only C42; every offender carries the native not-in-artifact mark | fixed: the I3 checker |
| I9 `compiler-supplied-non-runtime` | 1 | 0 | 0 | the Haskell reader's own `development` scope on `template-haskell`, which only the test suite declares | fixed: `resolve/scope_propagation.rs` |
| I2 `duplicate-library-identity` | 20 | 29 | 21 | milestone 1282's nameless members | declared, as for moat and shellcheck |

## Field placement: `generic-builder-fields.sh`

Run at nixpkgs `a799d3e3` for x86_64-linux, evaluation only. For six
libraries built with or without `doCheck`, every library and executable
dependency cabal2nix records (`getCabalDeps`) is in `propagatedBuildInputs`
and none is only in `buildInputs`; what `buildInputs` adds is the test suite
(`tasty`, `tasty-hunit`, `hspec`, `hspec-wai`, …). For the
`justStaticExecutables` ShellCheck build nothing is propagated and every link
dependency is only in `buildInputs`. Output:
`generic-builder-fields.x86_64-linux.out`.

So at a library the fallback's `buildInputs` step adds nothing linked, and at
an executable-only build it mixes link and test dependencies, which only the
project's manifest can separate.

Propagation alone does not say which is which: `hashable`, whose dependencies
are all boot packages, propagates nothing and has its test suite
(`tasty-hunit`, `tasty`, `QuickCheck`, …) in `buildInputs`, the shape of an
executable-only build. The install phase does: all six libraries register a
package (`Setup register --gen-pkg-config`, the builder's `isLibrary`
branch) and the static ShellCheck build does not. A first version of the fix
that read propagation only left `hashable`'s test suite in the set, and
`aeson` propagates `hashable` into both libraries' runtime closures.

## The I10 population

All 26 carried C203 `statically-linked`, the project's own lifecycle scope
`development` (direct for the declared test-suite dependencies, `transitive`
for their closures) and therefore CycloneDX `scope: excluded`, with no SPDX
2.3 `BUILD_TOOL_OF`. None is in its document's runtime closure. slack-web and
openai are libraries: 138 and 149 Hackage members are in their runtime
closures, because a library's output references what it links. slacklinker
is executable-only: none of its Hackage dependencies is. Every runtime Hackage
member of the two libraries lacks C201 `waybill:closure-identity`, so none is
confirmed and the fallback ran at all of them, adding each one's own test
suite too (76 and 62 unmarked `statically-linked` members, `tasty-*`,
`test-framework*`, `hedgehog`, …). slacklinker's `tmp-postgres` is declared at
1.34.1.0 and built at 1.35.0.0, so the manifest's test edge names a different
component from the built one, and is matched by package name.

## The I3 population

All 48 offenders carry CycloneDX `scope: "excluded"`, SPDX 2.3
`BUILD_TOOL_OF` and SPDX 3 `usesTool` scoped `build`: 44 source tarballs,
`.cabal` revisions and patches of test libraries, and four test libraries
reached only through build-graph edges. The committed `nix-closure-moat`
golden carries 16 offenders under a declaration written for one
(`hspec-meta`, unmarked in every format); the other 15 are the same shape as
the 48.
