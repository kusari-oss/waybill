# Phase 0 research: a CPE for the closure components a feed could match

**Feature**: `1367-closure-component-cpes` | **Issue**: #1371 | **Date**: 2026-11-06

Every finding is cited to a file and line read out of the tree at `019cb5bf`.
Where a claim is a prediction rather than an observation, it says so and names
the task that will measure it.

---

## R1 — The defect, located exactly

**`c.cpes = synthesize_cpes(c)` runs at `waybill-cli/src/scan_fs/mod.rs:1346`**,
inside `package_db::read_all`, over the post-dedup component set.

**The closure's components are added at `waybill-cli/src/cli/scan_cmd.rs:4748`**,
by `merge_closure_components`, in `scan_cmd::execute`.

The first happens strictly before the second. The pass is not gated, not
flagged and not ecosystem-restricted — it simply runs over a set that does not
yet contain a single closure component.

The `"generic"` arm (`cpe.rs:254`) would match all six measured components:
each name is a row in `GENERIC_LIBRARY_CPES` and each carries the non-empty
version the function requires at `cpe.rs:161`.

**This is not dead code.** An earlier reading of this concluded
`synthesize_cpes` had no production caller at all; that was a `grep | head`
truncation artifact, corrected before it reached the issue. The function is
called, once, in the wrong place for this tier.

---

## R2 — Where the second pass goes, and why not a move

**Decision**: add a second, **fill-only-if-empty** synthesis pass in
`scan_cmd::execute`, after the closure merge. Do **not** move the existing one.

**Why not move it**: `package_db::read_all` has two production callers —
`scan_fs/mod.rs:457` and `report/mod.rs:77`. Moving the pass into `scan_cmd`
would silently strip CPEs from the report path, which has nothing to do with
this defect. The existing call site also carries its own measured reasoning
(R3) that a move would discard.

**Why fill-only-if-empty rather than unconditional**: an ordering fact, read
rather than assumed.

| line | step |
| --- | --- |
| `scan_cmd.rs:4334` | `supplement::merge` — **operator-supplied CPEs land here** |
| `scan_cmd.rs:4748` | the closure merge |
| ~4760–4833 | the milestone-1366 identity pass, then the closure edge build |

`supplement/merge.rs:240` sets `cpes` from the operator's `--supplement-cdx`
file. That happens **before** any post-closure pass could run, so an
unconditional `c.cpes = synthesize_cpes(c)` would **overwrite an operator's
explicit CPEs with synthesized guesses**. Filling only where `cpes` is empty
makes FR-004 and FR-005 true by construction rather than by care.

**Placement within the window**: after the milestone-1366 identity pass, not
before. That pass rewrites 74 components from `pkg:generic/<n>@<v>` to
`pkg:pypi/<n>@<v>`, and `synthesize_cpes` switches on
`component.purl.ecosystem()` — so the same component takes a different arm
depending on which side of that pass the synthesis runs. Running after means a
closure component is treated exactly as the same package would be from any
other reader, which is the consistency this feature is about.

Unlike milestone 1366's pass, **there is no edge-ordering constraint**: CPEs
are per-component metadata and no relationship endpoint is derived from them.

---

## R3 — The existing placement's reasoning, preserved

`scan_fs/mod.rs:1341-1345` records why the pass sits where it does:

> *"Post-dedup CPE synthesis — runs on the merged set so a component that
> exists in both the filename pass and the dpkg pass gets one set of CPEs
> (attached to the single winning entry) instead of two."*

That reasoning is about running **before** dedup, which would attach CPEs to
entries that later merge away. It says nothing against running **again** over
a set that has since grown.

`synthesize_cpes` is a **pure function** of `(purl.ecosystem(), name,
version)` (`cpe.rs:158-161`) with no interior mutability and no inputs beyond
the component. Re-invoking it on an unchanged component yields an identical
result, so a second pass cannot produce a different answer for a component the
first already saw — and the fill-if-empty guard means it does not even try.

---

## R4 — Emission needs no work

Every format already reads `component.cpes`:

| format | carrier | site |
| --- | --- | --- |
| CycloneDX | `component.cpe` (first candidate) | `cyclonedx/metadata.rs:1267` |
| CycloneDX | `waybill:cpe-candidates` property (the rest) | `cyclonedx/metadata.rs:1589` |
| SPDX 2.3 | `externalRefs[]` `cpe23Type` (first) | `spdx/packages.rs:533` |
| SPDX 2.3 | `waybill:cpe-candidates` annotation (the rest) | `spdx/annotations.rs:337` |
| SPDX 3 | `externalIdentifier` per candidate | `spdx/v3_external_ids.rs:74` |

**C19 `waybill:cpe-candidates` already exists** in the parity catalog. This
feature adds no per-component row: it populates a field three emitters already
carry. FR-006 is satisfied by the existing wiring.

---

## R5 — FR-003 is NOT satisfied today, and needs a new guard

**Finding**: `synthesize_cpes` gates on `version.is_empty()` (`cpe.rs:161`)
and **nothing else**. There is no release-shape test.

`format_cpe` (`cpe.rs:319`) escapes per CPE 2.3 §6.2, so a strange version
produces a **well-formed but unmatchable** CPE rather than a corrupt one —
`cpe:2.3:a:x:x:26.05pre-git:*:*:*:*:*:*:*` is valid syntax and matches nothing
in NVD.

So FR-003's purpose is not "avoid corrupt output"; it is **avoid claiming
coverage that cannot match**. A document reporting 40 CPEs of which 34 are
unmatchable overstates what a scanner can do with it.

**No live defect today**: all six measured targets carry release-shaped
versions. The guard is about what happens when the table grows — and the
measured closure already holds the shapes that would trip it:
`libcxx 21.1.6+apple-sdk-26.4`, `libedit 20251016-3.1`,
`kyua 0.13-unstable-2024-01-22`, `CoreSymbolication 0-unstable-2018-06-17`.

**Decision**: implement the release-shape guard in this feature, applied where
the new pass calls the synthesizer rather than inside it. Changing
`synthesize_cpes` itself would alter CPEs for deb/apk/rpm components across
every existing golden, which is a separate decision with its own blast radius.

---

## R6 — The document-scope record (FR-008)

**Decision**: one new catalog row, **C209**, document scope, recording how many
components received CPE candidates and how many were eligible but unmapped.

Next free id confirmed by `grep -oE "^\| C[0-9]+"` over
`docs/reference/sbom-format-mapping.md` — highest existing is **C208**
(milestone 1366).

**Why a record at all**: without it, "this closure has no mapped software" and
"the synthesis pass did not reach these components" are the same document —
zero CPEs either way. That is precisely the defect this feature exists to fix,
and shipping the fix without a way to tell those two apart would leave the next
regression as invisible as this one was.

**Shape**: a JSON object on the C192 / C204 / C208 template — `schema`,
`with_candidates`, `eligible_unmapped`. Standards-native audit: CDX
`metadata.properties[]` and the SPDX document-scope annotation are the same
carriers C208 uses; no format has a native "how many identifiers did this pass
synthesize" construct. Parity bridge, `SymmetricEqual`.

---

## R7 — Predicted outcome, labelled as a prediction

The six measured components will receive CPEs. **A fill-if-empty pass will
also reach components beyond them**, and the size of that is a prediction, not
a measurement:

- The 74 components milestone 1366 re-identified to `pkg:pypi` currently have
  empty `cpes` (they were generic when the first pass ran, and the generic arm
  declined them as unmapped). After this feature they take the `"pypi"` arm
  (`cpe.rs:208`), which pushes `<name>` and `python-<name>` as vendors.
- Any other closure component whose `(ecosystem, name)` hits an arm.

This is **consistency, not scope creep**: a pypi component contributed by the
pip reader already receives those candidates today. A closure-contributed one
not receiving them is the same defect as the six, in a different arm.

SC-001 is written as "at least the 6" and accommodates this. **Task T035
records the actual total**; no number here may be quoted as data.

---

## R8 — The target population, measured

On the pinned closure (aarch64-darwin, nixpkgs `a799d3e3`), of 942 components:

| | count |
| --- | --- |
| carrying a CPE today | **0** |
| `pkg:generic`, closure members | 539 — 1 carries a version, whose value is `boot` |
| `pkg:generic`, non-member, in the curated table | **6** |
| `pkg:generic`, non-member, release version, unmapped | 117 names |
| `pkg:generic`, non-member, non-release version | 6 |
| `pkg:pypi` (milestone 1366) | 74 |

The six: `openssl 3.6.2`, `curl 8.20.0`, `zlib 1.3.2`, `sqlite 3.51.2`,
`pcre2 10.46`, `llvm 21.1.8`.

Recorded in `specs/1366-closure-ecosystem-purl/measurements/` and in issue
#1371.

---

## R9 — An adjacent defect, not fixed here

Verifying "members never want a CPE" turned up **issue #1374**: all 12
members classified `package` are source trees, a patch file and an
environment, because `member::classify` (`member.rs:190`) assigns that role on
name shape alone.

It does not affect this feature — none of the twelve carries a version, so
FR-003 excludes them regardless — and it is **deliberately out of scope**:
changing a frozen four-value enum needs its own measurement on more than one
closure. It is recorded because `role == package` is exactly the signal a
later CPE milestone would reach for, and it does not mean what it says.

---

## Constitution alignment

| Principle | Finding |
| --- | --- |
| **I. Pure Rust, statically linked** | Zero new Cargo dependencies. |
| **IV. Type-driven correctness** | `cpes` is `Vec<String>` today and this feature does not change that; a CPE newtype would be a wider refactor touching five emitters. No `.unwrap()` in production. |
| **V. Specification compliance** | CPE 2.3 formatted-string binding already implemented and escaped (`cpe.rs:319-344`). The new C209 row carries a native-field audit; no format has a construct for a synthesis-coverage count. The per-component carrier is the existing C19, so no new per-component row. |
| **VIII. Completeness** | The feature closes a false negative: software present in the build and invisible to an NVD-backed scanner. |
| **IX. Accuracy** | The governing principle here. *"Ambiguous or low-confidence matches MUST be flagged rather than silently included as definitive."* A CPE is emitted as a **candidate**, and FR-002/FR-003 keep candidates off things that are not software or cannot match. |
| **X. Transparency** | FR-008's C209 makes "no mapped software" distinguishable from "the pass did not run" — the exact confusion that let this defect live. |
| **XII. External data source enrichment** | No external source. The mapping table is in-source and the version comes from the closure. |
| **Operating Modes** | Scan mode; `waybill:generation-context` unchanged. |

**No violations. Complexity Tracking empty.**
