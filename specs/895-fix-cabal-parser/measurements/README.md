# Milestone 895 — measurements

Every figure is an observation. The command that produced it is recorded so it
can be re-run when the inputs move.

## Binaries

| Name | Build | Path |
|---|---|---|
| baseline | `main` @ `48af0eab`, release | `target/release/waybill-baseline` |
| after | this branch, release | `target/release/waybill` |

Both report `waybill 0.7.0`; they differ only by build. Keep them side by side
rather than re-resolving `waybill` on `$PATH`.

## T002 / T003 — the before-side

### The #891 reproducer (two cabal layouts in one file)

```
pkg:hackage/waybill-fixture-cmt@--_hs-source-dirs:_default-language:____Haskell2010
pkg:hackage/waybill-fixture-core@>=4.11_&&_<4.22
pkg:hackage/waybill-fixture-tool:waybill-fixture-tool@build-depends:_waybill-fixture-core_>=4.11_&&_<4.22
pkg:hackage/waybill-fixture-vec@>=0.12_&&_<0.14_default-language:_Haskell2010
```

Four components, three malformed, one name
(`waybill-fixture-tool:waybill-fixture-tool`) that cannot exist as a package.
Reproduces exactly as #891 recorded.

### The fifth defect has two mechanisms, not one

Analysis described the fifth defect as "only the first dependency list in a
section is read". Measurement shows that is one of **two** outcomes, and which
one occurs depends on whether a blank line separates the lists.

On a fixture whose repeated `build-depends:` fields are separated by comment
lines rather than blank lines, the first match **spans** the later lists
instead of dropping them, and the entry splitter glues them into the preceding
entry's version:

```
waybill-fixture-alpha@>=1.0_&&_<2_--_Group_two_build-depends:
waybill-fixture-beta@^>=2.1_if_!impl(ghc_>=9.4)_build-depends:_waybill-fixture-cond_>=0.1_&&_<0.2_--_Group_three_build-depends:
waybill-fixture-gamma@==3.*_default-language:_Haskell2010
```

Three components for four declared dependencies — `waybill-fixture-cond`, the
one inside the conditional, is absorbed into `beta`'s version rather than
emitted.

Where a blank line *does* separate the lists, the lazy match terminates and
the later lists are dropped outright. Both outcomes are wrong; a fix that
addresses only "reads one block" would leave the spanning case.

### The intended corpus target

`haskell/aeson`, `aeson.cabal` at the revision fetched 2026-09-17. Scanned
with the baseline:

| | |
|---|---:|
| dependencies declared across all `build-depends` blocks | **48** |
| components emitted | 38 |
| **declared but never emitted** | **10** |
| emitted identifiers that are malformed | **13 of 38** |

Missing entirely: `aeson` (a self-reference from its own test-suite),
`character-ps`, `integer-conversion`, `integer-gmp`, `nothunks`, `primitive`,
`semialign`, `text-iso8601`, `th-abstraction`, `witherable`.

This is the denominator SC-002 and SC-002a need. It also settles the relative
size of the two problems: the completeness defect costs 10 dependencies on
this file, while the accuracy defect malforms 13 identifiers.

**Reproduce**:

```bash
mkdir -p /tmp/aeson-probe && cp <aeson.cabal> /tmp/aeson-probe/
target/release/waybill-baseline --offline sbom scan --path /tmp/aeson-probe \
  --format cyclonedx-json --output cyclonedx-json=/tmp/before.json
jq -r '.components[].purl' /tmp/before.json
```

## T015 — US1 teeth-check

Run before any implementation, against `target/release/waybill-baseline`:
**10 of 11 tests fail.** The one that passes is
`t005_the_measurement_helper_reads_both_layouts`, which checks the independent
declared-name reader and deliberately does not touch the parser under test.

### Three tests initially passed against the defect

Worth recording, because it is the failure mode SC-008 exists to catch and it
happened here on the first attempt.

`t009`, `t010` and `t012` passed against the unfixed parser. All three
asserted on component **names**, and the helper that extracts them strips
everything after `@` — so a component whose version had swallowed the next
three lines still had the correct name:

```
pkg:hackage/waybill-fixture-core@>=4.11_&&_<4.22_default-language:_Haskell2010
                                 ^ the defect lives here, invisible to a name assertion
```

`t010` was worse than blind: it asserted no identifier contained the string
`build-depends`, and passed because the build-tool component's PURL was
malformed enough to fail validation and be **dropped entirely**. The
assertion held vacuously over a component set the defect had emptied.

Fixed by adding `assert_identifiers_wellformed`, which rejects any identifier
carrying a cabal field name, a comment marker, or the underscore the
sanitiser substitutes for swallowed whitespace — and by asserting in `t010`
that the declared build tool is *present*, not merely un-leaked.

After strengthening, all three fail against the baseline as they should.

## A sixth defect, found during implementation

A `library` stanza whose **first line is its dependency field** emitted
nothing at all. Measured on the baseline as well as on the work-in-progress
build, so it is pre-existing rather than introduced here.

Cause: the stanza-opener regex ended `\s*$`, and `\s` matches newlines. The
greedy trailing `\s*` consumed the line break *and* the following line's
indentation, so the first field inside the stanza appeared to start at column
zero and was skipped. Any preceding line — a comment, another field — masked
it, which is why it survived this long.

```
library
  build-depends:          <- emitted 0 components
      waybill-fixture-core >=4.11 && <4.22

library
  -- anything at all
  build-depends:          <- emitted normally
      waybill-fixture-core >=4.11 && <4.22
```

Fixed by anchoring the opener with `[ \t\r]*$`.

## T020 — after, on the corpus target

`aeson.cabal`, same file, both binaries:

| | before | after |
|---|---:|---:|
| declared across all blocks | 48 | 48 |
| components emitted | 38 | **47** |
| declared but never emitted | **10** | **1** |
| malformed identifiers | **38 of 38** | **0** |

The single remaining absence is `aeson` itself — a self-reference from its
own test-suite, which is correctly not emitted as an external dependency
rather than a defect.

## T021 — correct input unchanged

All four existing Haskell integration targets pass unmodified —
`haskell_cabal_baseline` (3), `haskell_edge_cases` (8),
`haskell_tier_fallbacks` (5), `haskell_stack_discrimination` (5) — plus the
23 in-src unit tests. Contract A-7 holds, and the `ghc` / stackage-resolver
placeholder keeps its `@unspecified` shape (R2, T031).

## Phase 6 — partial-failure reporting

### A seventh manifestation, found writing the test

There was no package-name validation at all. Anything surviving the comma
split became a component:

```
build-depends:
    waybill-fixture-good >=1 && <2
  , !!!not-a-package          ->  pkg:hackage/!!!not-a-package
  , waybill-fixture-also-good
```

`Purl::new` accepts it, so it shipped. A consumer cannot distinguish it from
a real dependency, which is the same accuracy failure as #891's four in a
different guise.

Now rejected against cabal's package-name grammar (alphanumerics and hyphens,
at least one letter), counted, and reported document-scope as C162
`waybill:cabal-entries-skipped`. The readable siblings are unaffected —
skipping is per entry, not per list.

### T044 teeth-check

| Test | Fails on baseline? | Why |
|---|---|---|
| `t042_one_unreadable_entry_costs_one_component_not_the_list` | yes | `!!!not-a-package` was emitted, and no count existed |
| `t043_the_skip_count_is_reported_even_when_zero` | yes | the annotation did not exist |
| `t043b_the_count_is_absent_when_no_cabal_file_was_read` | **no** | byte-identity guard (SEC-2); asserts an absence that already held |

`t043` asserts on `Option`, so an absent field fails it rather than only a
wrong value — which is what FR-012b asks for and what T043's task text
required be checked.
