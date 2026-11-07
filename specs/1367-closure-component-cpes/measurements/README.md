# Measurements — milestone 1367 (#1371)

**Date**: 2026-11-06
**Host**: aarch64-darwin
**Closure**: `nix-closure-moat` at commit `d069055`, nixpkgs pinned `a799d3e3`
**Pre-change binary**: `main` at `5c9490d` (milestone 1366 merged)

> **These figures are aarch64-darwin.** The corpus goldens pin x86_64-linux,
> which is a **different closure**. Milestone 1366 measured 74 PyPI candidates
> on one and 75 on the other for exactly this reason. A count from one is
> wrong in membership when quoted against the other.

---

## T002 — the control

```sh
waybill sbom scan --path <moat clone at d069055> \
  --nix-closure --nix-closure-attr default \
  --format cyclonedx-json --output cyclonedx-json=before.cdx.json

jq '[.components[] | select(.cpe)] | length' before.cdx.json
```

| | |
| --- | --- |
| components | 942 |
| **carrying a `cpe` field** | **0** |

Zero is the whole defect, and it is also what "this closure has no mapped
software" would look like. That is why T013 writes a control before any CPE
assertion: without one, a test over an empty population passes while proving
nothing.

## T003 — the six targets, read from the document

| name | version | PURL today |
| --- | --- | --- |
| `curl` | 8.20.0 | `pkg:generic/curl@8.20.0` |
| `llvm` | 21.1.8 | `pkg:generic/llvm@21.1.8` |
| `openssl` | 3.6.2 | `pkg:generic/openssl@3.6.2` |
| `pcre2` | 10.46 | `pkg:generic/pcre2@10.46` |
| `sqlite` | 3.51.2 | `pkg:generic/sqlite@3.51.2` |
| `zlib` | 1.3.2 | `pkg:generic/zlib@1.3.2` |

Read out of the pre-change document, not quoted from issue #1371.

## T004 — the eligible population, by ecosystem

Components reaching the pass with **empty `cpes`** and a **release-shaped
version**:

| PURL type | eligible | has a synthesizer arm? | predicted outcome |
| --- | --- | --- | --- |
| `pkg:generic` | 132 | yes, curated-table-gated | **6** map; 126 are `eligible_unmapped` |
| `pkg:hackage` | **138** | **no** | **0** — all 138 are `eligible_unmapped` |
| `pkg:pypi` | 71 | yes | **71** map, 2 candidates each |
| **total** | **341** | | |

### Two things this changed

**1. Research R7 missed Hackage.** R7 predicted the delivered total would
exceed six "because the 74 `pkg:pypi` components take the pypi arm". It did
not consider that **138 `pkg:hackage` components are equally eligible** and
that `synthesize_cpes` has **no `hackage` arm** — they fall to the `_ =>
Vec::new()` default. They will be counted as `eligible_unmapped` and receive
nothing.

That is the correct behaviour (FR-007 silent-skip), and it makes
`eligible_unmapped` the larger of C209's two counts by a wide margin. Worth
knowing before the record's shape is read as a problem.

**2. FR-002 is confirmed belt-and-braces, by measurement.** **Zero** closure
members are eligible — not one of the 539 reaches the pass with both empty
CPEs and a release-shaped version, because only one carries a version at all
and its value is `boot`. So on this closure FR-003 alone would produce the
same result, and FR-002's guard is never the thing that declines a member.

This is exactly what `data-model.md` predicted, now observed rather than
reasoned. It also means **T010's test must construct a member that carries a
release version** — the corpus cannot produce one, so a test written against
the real closure would pass whether or not the guard exists.

### Prediction for T035

`with_candidates ≈ 77` (6 generic + 71 pypi), `eligible_unmapped ≈ 264`.

**This is arithmetic over a measured population, not a measurement.** T035
records what the implementation actually produces, and that is the figure
SC-001 requires.

---

## T018, T035, T036 — the delivered result

```json
{"schema":"waybill-cpe-coverage/v1","with_candidates":77,"eligible_unmapped":264}
```

| | before | after |
| --- | --- | --- |
| components | 942 | 942 |
| **carrying a CPE** | **0** | **77** |
| `pkg:generic` with a CPE | 0 | **6** |
| `pkg:pypi` with a CPE | 0 | **71** |
| members with a CPE | 0 | **0** |
| CPEs with a non-release version | — | **0** |

**T004's prediction was exact**: 6 + 71 = 77, and 341 − 77 = 264. The
arithmetic over the measured eligible population held, which is worth
recording — it means the pass reached every component the Phase-1 census said
it would, and nothing it said it would not.

### The six (T036, SC-006)

| name | version | CPE |
| --- | --- | --- |
| `curl` | 8.20.0 | `cpe:2.3:a:haxx:curl:8.20.0:*:*:*:*:*:*:*` |
| `llvm` | 21.1.8 | `cpe:2.3:a:llvm:llvm:21.1.8:*:*:*:*:*:*:*` |
| `openssl` | 3.6.2 | `cpe:2.3:a:openssl:openssl:3.6.2:*:*:*:*:*:*:*` |
| `pcre2` | 10.46 | `cpe:2.3:a:pcre:pcre2:10.46:*:*:*:*:*:*:*` |
| `sqlite` | 3.51.2 | `cpe:2.3:a:sqlite:sqlite:3.51.2:*:*:*:*:*:*:*` |
| `zlib` | 1.3.2 | `cpe:2.3:a:zlib:zlib:1.3.2:*:*:*:*:*:*:*` |

`curl` resolves to the `haxx` vendor slug — the one carrying 228 NVD
dictionary entries, against 2 for `curl:curl`. The curated table emits both;
the primary shown here is the first candidate.

## T031, T032 — non-regression

| criterion | population | result |
| --- | --- | --- |
| **SC-002** / FR-002 | closure members | 539 before, 539 after, **0 with a CPE** — PASS |
| **SC-004** / FR-004 | components from other readers | 25 before, 25 after — PASS |

Component count unchanged at 942: the pass added metadata, not components.

## T038 — the mutation check

Four mutants, each run against both suites:

| mutant | unit suite | integration suite |
| --- | --- | --- |
| 1 — the pass is a no-op | **1 of 8 red** | **3 of 9 red** |
| 2 — fill unconditionally, not only-if-empty | **1 of 8 red** | 9 green |
| 3 — remove the release-shape guard | **1 of 8 red** | 9 green |
| 4 — remove the closure-member guard | **1 of 8 red** | 9 green |

**Three of the four guards are invisible to the integration suite**, and that
is the plan's reasoning confirmed rather than a gap. The pinned closure cannot
produce the input: no component on it both receives a `--supplement-cdx` value
and matches the curated table; no mapped component carries a non-release
version; and **zero members reach the pass eligible**.

The last one is the sharpest. The integration fixture contains a member named
`zlib-9.9.9.tar.gz` — built deliberately to exercise FR-002 — and it *still*
cannot, because the member carries no version and the release-shape guard
declines it first. A fixture constructed specifically for that requirement is
defeated by the same structural fact that defeats the real closure.

Only the unit tests, with constructed input, catch mutants 2–4.

## T033, T034 — a scan with no closure

```sh
waybill sbom scan --path <a project with no flake> --no-deep-hash \
  --format cyclonedx-json --output cyclonedx-json=out.cdx.json
```

| | |
| --- | --- |
| components | 5 |
| carrying a CPE | **0** |
| C209 | `{"with_candidates":0,"eligible_unmapped":1}` — **present** |
| C208 (closure-only) | absent — correct |

The document gains exactly one property against the pre-change binary, which
is SC-005's shape. **C209 is present even though no closure ran**, which is
the design: the pass it describes runs for every component from every reader,
and gating the record on `--nix-closure` would leave non-closure scans with
the blind spot this feature exists to close.

T034 additionally asserts the release-shape property **on the emitted
document** rather than on the predicate. The unit test proves the guard is
correct; this proves it was called. A guard that is correct and never invoked
satisfies the first and not the second — which is a fair description of the
defect this whole milestone fixes.

## T042 — the gate, and the cost the plan did not predict

The first run **failed**: 35 local golden regressions across 24 tests.

That is the direct consequence of a design decision made deliberately — C209
is emitted on **every** scan, not only `--nix-closure` ones — and it was not
in the plan's risk table. Milestone 1366's C208 is closure-gated and touched
one corpus target; C209 touches every committed golden. The decision stands
(gating the record would reintroduce the blind spot for non-closure scans),
but the consequence should have been predicted rather than discovered.

Six test files write goldens, not the three `*_regression` ones:
`cdx_regression`, `spdx_regression`, `spdx3_regression`,
`pkg_alias_binding_us1`, `oci_pull_backward_compat`,
`optional_dep_classification`.

```sh
WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
  cargo test --test cdx_regression --test spdx_regression \
             --test spdx3_regression --test pkg_alias_binding_us1 \
             --test oci_pull_backward_compat --test optional_dep_classification
```

The churn was **verified, not trusted**:

| | |
| --- | --- |
| golden files changed | 34 |
| insertions | **202** |
| deletions | **0** |
| added lines that are not C209 or its structural braces | **0** |

Second run:

| | |
| --- | --- |
| `GATE_RESULT` | **pass** |
| test targets | 371, all `ok.` |
| totals | **7,445 passed, 0 failed** |
| failure blocks | 0 |

+18 against main's 7,427 — the 8 unit and 10 integration tests this milestone
adds.

## T039, T040, T041 — the corpus goldens

Refreshed via `public-corpus.yml` dispatch (run 42312727773, 357s, success).

**All 51 golden files differ**, which is the expected consequence of C209
being unconditional — and the reason T040 requires each one be accounted for
rather than assumed. Every added and removed line was classified:

| target | + | − | content |
| --- | --- | --- | --- |
| 15 targets (go-cobra, python-flask, rust-ripgrep, the five pants ones, …) | 18 each | **0** | C209 and its structure, nothing else |
| `nix-closure-moat` | 107 | **0** | C209 + CPE fields + C19 candidate lists |
| `image-postgres16` | 459 | **441** | C209 **+ 441 removals that are not this feature's** |

**Committed: 16 targets, 48 files, 377 insertions, 0 deletions.**

**`image-postgres16` is deliberately excluded.** Its 441 removals are the
layer-digest drift of issue #1372 — a digest-pinned image whose layers moved,
unrelated to CPEs and already failing on `main`. Committing its regenerated
goldens here would bury an unrelated upstream change inside a feature diff and
answer "why did the digests move" with "they moved, so I wrote down the new
ones".

**A `regen_goldens=true` run proves nothing by passing**: it regenerates the
goldens it then compares against. The evidence is the artifact, classified
file by file.

### Another per-system difference

The x86_64-linux corpus closure gains a CPE for `gnutls 3.8.13`, which the
aarch64-darwin closure does not contain at all. Same project, same pinned
nixpkgs, different system — the hazard this file opens with, observed for the
third time this milestone.

---

## Nothing open

Every task is measured. The remaining work is review.
