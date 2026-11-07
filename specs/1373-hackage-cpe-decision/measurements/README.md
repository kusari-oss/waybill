# Measurements — spec 1373 (issue #1375)

**Date**: 2026-11-07
**Host**: aarch64-darwin
**Closure**: `nix-closure-moat` at `d069055`, nixpkgs pinned `a799d3e3`
**waybill**: `157631c1` — main's tip after milestone 1372 merged

> **Issue #1375's figures predate milestone 1372.** It was filed against a
> document with 942 components; m1372 (#1366) took that to 977 and moved every
> number the issue quotes. Nothing below is comparable to the issue's text, and
> the issue has been corrected rather than left to be read as current.

---

## 1. The population, re-measured

| | issue #1375 | **now** |
| --- | --- | --- |
| `pkg:hackage` components | 138 | **163** |
| `eligible_unmapped` (aarch64-darwin) | 264 | **284** |
| `with_candidates` (aarch64-darwin) | 77 | **78** |

The pinned **x86_64-linux** corpus document reads `eligible_unmapped: 442,
with_candidates: 6` for the same closure. The two hosts resolve different
closures, and the corpus harness runs with `--no-deps-dev`, so milestone
1366's PyPI confirmation does not run and its 71 `pkg:pypi` CPE candidates are
absent. Quoting one host's figure against the other is the error milestone
1366 already recorded.

## 2. 24 of the 163 never reach the synthesizer

Their PURLs carry no version at all — `pkg:hackage/base`, not
`pkg:hackage/base@4.19.2.0` — and `is_release_shaped` returns `false` for an
empty version on its first line. They are therefore **not** in
`eligible_unmapped`, so hackage contributes **at most 139** to it, not 163.

The 24 are the GHC boot libraries: `base`, `array`, `binary`, `bytestring`,
`containers`, `deepseq`, `directory`, `exceptions`, `filepath`, `ghc-heap`,
`ghc-prim`, `mtl`, `os-string`, `parsec`, `pretty`, `process`, `stm`,
`template-haskell`, `Cabal`, `Cabal-syntax` and four more.

A versionless PURL also matches an advisory's affected range
*unconditionally* — three of the six OSV hits in §4 are versionless, so they
come back affected without their version ever being considered.

**I first recorded that as a defect, and it is not one.** Checking before
filing showed waybill already discloses every one. `base` carries
`waybill:haskell-version-unresolved-reason: compiler-supplied`,
`waybill:unresolved-reason: "supplied by the compiler; nixpkgs has no version
to give and a freeze file would not pin it either"`, `waybill:ghc-stdlib:
true`, `waybill:sbom-tier: design`, and
`waybill:nixpkgs-candidate-compilers: 9.10.x,9.4.x,9.6.x`.

The residue is a consumer-side consequence, not a waybill defect: the
disclosure lives in the annotations rather than in the PURL, so a tool that
queries PURLs and ignores annotations gets an over-broad answer. That belongs
in the consumer guide.

## 3. NVD's CPE dictionary — `scripts/probe-cpe-dictionary.sh`

Every `haskell:<name>` pair for all 163 names, plus the 20 capitalised names
re-probed lowercased, since CPE product values are conventionally lowercase
and `haskell:Cabal` / `haskell:cabal` both returning zero could not settle
case on their own.

Controls behaved: `openssl:openssl` = **385**,
`waybill-fixture-vendor:waybill-fixture-product` = **0**.

**All 171 pairs resolved. Zero unanswered.**

| | pairs |
| --- | --- |
| `haskell:<name>` probed | 171 (163 names + 8 lowercase variants) |
| **with at least one dictionary entry** | **1** |
| with none | 170 |

The one is **`haskell:aeson` = 95**. Every other name in the closure —
including `base`, `Cabal`, `cryptonite`, `text`, `bytestring`, `containers` —
returns zero. Case makes no difference: all 8 capitalised names return zero in
both forms.

So a `hackage` arm would produce an identifier NVD could match for **1 of 163
components**, and the metric it would move is `eligible_unmapped`, by one.

### The first sweep was unusable, and said so only on inspection

It was launched with `( … ) &` inside a shell that then exited, so it was
orphaned after 4 of 171 pairs. The wrapper still reported exit 0. Four rows,
all zero, read exactly like "every pair is absent" — the failure mode this
repository keeps rediscovering: **a check that did not run looks identical to
a check that passed.** It was caught by the row count, not by the status.

The second run interleaved its `2>&1` tracebacks with its data lines, fusing
some of them (`...Frahaskell:alex\t0`), so rows had to be recovered by
anchoring on the pair name rather than the line start. The third run separates
stderr and produces clean rows with no errors.

## 4. OSV — the counter-argument, measured rather than quoted

Issue #1375 argues these components do not need CPEs because OSV already
indexes Hackage. That is a claim about an external service, so it was probed.

### The first OSV probe was broken and its controls proved it

It returned `ERR` for every input **including its positive control** — a
shell-quoting defect, not an empty ecosystem. Uniform results across cases
that must differ are the signature of a broken harness. `osv-probe.py`
replaces it; its controls now read `urllib3`/PyPI = **44**, a nonexistent
package = **0**, and an invalid ecosystem = **HTTP 400**, three outcomes that
are distinguishable from one another.

### By name and ecosystem — 7 of 163

`Cabal` 1, `aeson` 2, `base` 2, `cmark-gfm` 1, `process` 1, `scientific` 2,
`text-iso8601` 1. All HSEC advisories.

### By waybill's own emitted PURL — 6 of 163

This is the query a consumer actually makes, and it is the one that matters:
it asks whether the identifier *in waybill's document* reaches the advisory.

```
pkg:hackage/Cabal                  1   HSEC-2026-0006
pkg:hackage/aeson@2.2.4.1          1   HSEC-2026-0007
pkg:hackage/base                   2   HSEC-2023-0007, HSEC-2024-0006
pkg:hackage/process                1   HSEC-2024-0003
pkg:hackage/scientific@0.3.8.1     1   HSEC-2026-0011
pkg:hackage/text-iso8601@0.1.1.1   1   HSEC-2026-0007
```

**Version filtering demonstrably works**: `aeson` returns 2 by name and 1 by
versioned PURL; `scientific` 2 and 1; `cmark-gfm` drops out entirely because
its version is outside every affected range. So the feed is applying ranges to
waybill's identifier, which is the end-to-end property the issue assumed.

### The decisive observation

`base`, `Cabal` and `process` carry live HSEC advisories and have **no NVD
dictionary entry at all**. A blanket `hackage` CPE arm would emit
`cpe:2.3:a:haskell:base:<v>:*:*:*:*:*:*:*` for exactly the packages in this
closure that have known vulnerabilities, and it would match nothing — a silent
zero, indistinguishable from a clean component. That is the failure
`scripts/probe-cpe-dictionary.sh` exists to prevent, and its header records
upstream shipping it twice.

## 5. One claim in the issue needs narrowing

> "OSV indexes the Hackage ecosystem from the Haskell Security Advisory DB —
> which this repository already tracks as the `haskell-security-advisories`
> corpus target."

The target exists, at `waybill-cli/tests/corpus_harness_195/`. But it is a
repository waybill **scans** as a Haskell project, not an advisory feed
waybill **consumes**. The two are unrelated, and the sentence reads as though
tracking the corpus target gives waybill advisory coverage. It does not; OSV
does, as §4 shows.

---

## 6. Which PURL types any feed can address — the basis for the new metric

OSV's declared ecosystem list, fetched 2026-11-07 from
`osv-vulnerabilities.storage.googleapis.com/ecosystems.txt` and committed as
`osv-ecosystems-2026-11-07.txt`: **47 entries**. `Hackage`, `PyPI`, `npm`,
`Go`, `crates.io`, `Maven`, `Debian`, `Alpine`, `RubyGems` and `NuGet` are all
present. **There is no `generic` and no `Generic`.**

Probed with known-vulnerable versions:

| PURL | advisories |
| --- | --- |
| `pkg:pypi/urllib3@1.26.4` | 22 |
| `pkg:npm/lodash@4.17.11` | 6 |
| `pkg:hackage/aeson@2.0.0.0` | 2 |
| `pkg:generic/openssl@3.0.0` | **0** |
| `pkg:generic/zlib@1.2.11` | **0** |
| `pkg:generic/curl@7.60.0` | **1** — `OSV-2017-79`, an OSS-Fuzz finding |

`openssl 3.0.0` and `zlib 1.2.11` both carry well-known CVEs and return
nothing, because `pkg:generic` names no registry to index against.

**The curl row is why the rule needs both halves.** A "non-zero probe means
addressable" rule would classify `generic` as addressable on the strength of
one OSS-Fuzz record keyed by project name. OSV's declared list excludes
`generic`, so requiring list membership *and* a successful probe rejects it.
FR-003 states both.

## 7. The partition, measured

| bucket | components | composition |
| --- | --- | --- |
| `with_candidates` | **78** | 72 `pypi`, 6 curated `generic` |
| `addressable_without_cpe` | **166** | 163 `hackage`, 3 `pypi` |
| `unaddressable` | **733** | 732 `generic`, 1 `github` |
| | **977** | equals the component count |

**`eligible_unmapped: 284` understated the real gap.** Its release-shape gate
excluded most `pkg:generic` components, so the number of components no feed
can address is **733**. The original issue asked whether 284 overstated a gap;
it overstated the Haskell share *and* understated the total, by a factor of
2.6 in the direction that matters.

---

# Implementation (milestone 1373)

**Pre-change binary**: `157631c1`, built in a worktree at the merge base.
**Host**: aarch64-darwin. **Closure**: `nix-closure-moat` at `d069055`.

## The delivered row

```json
{"schema":"waybill-cpe-coverage/v2","with_candidates":78,
 "addressable_without_cpe":166,"unaddressable":733}
```

**78 / 166 / 733 = 977**, equal to the component count — the partition holds,
and the figures are exactly those computed from the document before any code
was written. `partition.py` now prints **AGREES** where it printed
`DISAGREES`.

| | pre | post |
| --- | --- | --- |
| `with_candidates` | 78 | **78** — unchanged, as FR-006 requires |
| `eligible_unmapped` | 284 | **removed** |
| `addressable_without_cpe` | — | **166** |
| `unaddressable` | — | **733** |

## SC-003 / SC-004 — the change did not leak

| check | result |
| --- | --- |
| components whose `{purl, cpe, cpes}` changed | **0** of 977, compared as a set |
| differing JSON paths, whole document | **1** — the C209 value |

### The baseline was wrong the first time, and the control caught it

The first SC-003 run reported **42** differing paths. Almost all were
index-shift noise: `metadata.properties` differed by one entry because the
"before" document was a scan kept from milestone 1372's development and
predates C210 `waybill:closure-graph-admission`. A stale baseline, not a leak.

It would have been easy to read 42 differences as a problem with this change
and go looking in the wrong place. The tell was that the *first* differing
entry was a property this feature does not touch. Rebuilt at the merge base,
the count is **1**.

## T028 — the mutation check, and the invariant it found uncovered

| mutation | `--bins m1373` | `closure_component_cpes` |
| --- | --- | --- |
| every component classified `Unaddressable` | **2 failed** | ok |
| addressable components reported `WithCandidates` | **2 failed** | **1 failed** |
| **the partition assertion deleted** | **ok — survived** | **ok — survived** |
| a `hackage` arm added | **T023 failed** | — |

**The third mutation survived, and that was the point of running it.**
`fill_missing_cpes` carries a `debug_assert_eq!` comparing the sum to the
component count, and deleting it left the entire suite green: nothing else
compared the two. FR-002 and SC-002 both require the partition to be
*asserted*, and an assertion no test can distinguish from its own absence is a
comment with punctuation.

`m1373_t016_the_counts_partition_the_component_set` closes it, over a mixed
population including the two shapes the synthesis loop skips — an empty
version and a non-release-shaped one — because those are exactly the
components the v1 metric lost. Re-mutated to prove it has teeth:

| mutation | result |
| --- | --- |
| the counting loop drops a bucket (a component lands nowhere) | **failed** |
| two buckets collapsed into one (**the sum still holds**) | **failed** |

The second matters more than the first: a test that only checks the sum would
pass a classifier that put everything in one bucket.

## T027 — the parity extractors need no change

22 tests in `cargo test --lib parity::` pass untouched. The `cdx_anno!` /
`spdx23_anno!` / `spdx3_anno!` macros key on the row **label**, not the
payload, so C209's three rows carry the new value without an extractor edit.

**This task's text named a test that does not exist.** It said to run
`holistic_parity`; `cargo test --lib holistic_parity` reports
`0 passed; N filtered out`, which is indistinguishable from a pass. The real
gates are `every_catalog_row_has_an_extractor`,
`the_closure_rows_extract_equal_sets_from_all_three_formats`,
`extractors_table_is_sorted_by_row_id` and `extractors_have_unique_labels`.
Corrected in `tasks.md`.

## T029 / T030 — the local goldens, and a baseline that was double-counted

```
33 files changed, 33 insertions(+), 33 deletions(-)
every changed line: 66 of 66 are C209 values. OTHER: 0
```

One line replaced per file, in exactly the files that carry the row, and
nothing else moved.

**The expected count was 33, not the 66 this spec carried from T005 onward.**
`grep -rl 'waybill:cpe-coverage'` over the golden directory returns 66 files;
**half are `*.actual.json`** — diagnostic output a failing test writes beside
the golden it failed against, not tracked goldens. The tracked count is 33.

Counting them made a complete regeneration look half-applied. The check that
settled it was not the count but the complement: **which tracked golden
carrying C209 did *not* move?** The answer was none, which is the question
worth asking, because a count can be short for two very different reasons.
Corrected in `spec.md`, `plan.md`, `research.md`, `contracts/annotations.md`,
`quickstart.md` and `tasks.md`.

The corpus figure of 48 is unaffected: none of those is an `.actual` file.

## T021 — the row against the document, per corpus target

Run before the corpus refresh, so the expected answer is `DISAGREES`
everywhere — the committed goldens still carry `/v1`:

| | targets |
| --- | --- |
| `DISAGREES` (row is v1, document is the new partition) | **16** |
| no C209 row at all | **1** — `image-postgres16` |

Which is the 16-of-17 prediction, and the one exception is the target whose
golden predates milestone 1367. They become `AGREES` after T032's refresh;
this run establishes that the check can say `DISAGREES`, without which an
`AGREES` after the refresh would prove nothing.

## T022 / T023 were destroyed by the mutation check, and the gate passed anyway

The fourth mutation added a `hackage` arm to `generate/cpe.rs` to prove T023
would catch one. It did. The cleanup was `git checkout
waybill-cli/src/generate/cpe.rs` — and **that file was not committed yet**, so
the checkout reverted the mutation *and* T022's recorded decision and T023's
test along with it.

**The gate then passed: 371 targets, 7,459 passed, 0 failed.** It could not
have failed. A test that no longer exists is indistinguishable from a test
that was never required, and the only trace was `m1373_t023` missing from the
list of m1373 tests the run had executed — which is visible only if that list
is enumerated and compared against what was supposed to be there.

Every other mutation in this milestone used `cp` to a backup and `cp` back.
The one that used `git checkout` was the one that lost work, because the other
file had been committed and this one had not.

### And the restoration broke a neighbouring doc comment

Re-inserting T023 by anchoring on a nearby string spliced it into the **middle
of another test's doc comment**: T015's comment ended `…ahead of the` and then
continued into the milestone-1373 text, leaving `generic_symbol_fingerprint_
only_emits_no_cpe` with no header and a dangling fragment above it. This is
the displacement hazard `CLAUDE.md` records from PR #1321, where an insertion
above a function displaced its `#[cfg]` attribute and broke the build in the
one configuration that had not been tested.

It compiled and the tests passed in both the broken and the fixed state, so
nothing in the suite would have reported it. The fix was to move the test to
the end of the module rather than anchor it between existing items, and the
check was `git diff -U0 … | grep '^-'`: **one deleted line, the closing
brace.** A diff that deletes nothing but what it meant to move is the evidence
that an insertion did not displace anything.

## T032 / T033 — the corpus refresh, and FR-002 falsified by it

Dispatched against this branch; run `42347651466`. **The scanned commit it
reports — `7ab1fc9b` — is this branch's tip**, checked rather than assumed
(a `workflow_dispatch` run's `github.sha` is the workflow's commit, not the
scanned one).

Accounted for independently of the run summary: 51 committed JSON files, 51
in the artifact, none new, none missing, **all 51 differ**.

| target | changed lines | taken? |
| --- | --- | --- |
| 16 targets | **6 each, all C209, zero other** | **yes** — 48 files |
| `image-postgres16` | **896 other** + 4 C209 | **no** — #1372's drift |

**The prediction was 16 of 17 and that was right about this feature, but
wrong about the file count.** `image-postgres16` changes too; it simply
changes for its own reason. It stays excluded, so after this milestone every
corpus document carries `/v2` except that one, whose golden carries no C209
at all.

### FR-002 said something not satisfiable, and the refresh proved it

T021 re-run after the refresh: **9 of 16 targets still `DISAGREES`**. The row
and the document disagreed about the denominator, always in `unaddressable`,
always by components the document shows with **no PURL**.

They are **file-tier entries** — `type: "file"`, carrying
`waybill:component-tier`: `utils.sh`, `copy-examples`,
`verify_released_changelog.sh`. The file-tier emitter appends them *after* the
resolver, so the slice `fill_missing_cpes` counts never contains them.
`rust-ripgrep` has 7, `npm-express` and `go-opentelemetry` 1 each.

So FR-002's "partition every component in the document" **cannot hold at that
point in the pipeline**, and nothing in the suite could have told me: the
`debug_assert` compares against the slice, which is self-consistent. It took
comparing the emitted row against the emitted document on sixteen real
projects.

They also *should* be outside the partition. `utils.sh` is a file, not
software a vulnerability feed could address; counting one `unaddressable`
would make that word mean two different things in one row.

Scoped to "every **resolved** component" in FR-002, SC-002, `partition.py`,
the source comment and the C209 doc row. **All 16 now `AGREE`.**

This is the third claim of mine this milestone's measurement falsified, after
the non-zero-probe rule and the 66-golden baseline.
