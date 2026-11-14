# Feature Specification: Operator-supplied CPEs, and the curated CPE table as data

**Feature Branch**: `1491-operator-cpe-table`
**Created**: 2026-11-13
**Status**: Draft
**Issues**: #1413, #1491

**Input**: User description: "#1413 and #1491 — Let operators supply CPEs for the document subject and for components, and move the curated CPE table out of Rust source into one committed data file embedded with include_str! (as #966's GHC boot table is). Today the table is GENERIC_LIBRARY_CPES (82 rows) plus NVD_DICTIONARY_OBSERVED (95 vendor/product/dictionary-entry observations) in waybill-cli/src/generate/cpe.rs, with evidence spread across specs/*/measurements/. Each data-file row carries vendor, product, secondary vendors, the dictionary entry count, the observation date and the evidence file path — no CVE counts or other vulnerability data (waybill emits identifiers only; NVD CVE data stays dev-time evidence under specs/). The hermetic enforcement tests (sorted, valid CPE 2.3, no duplicate slug, every pair seen in the dictionary, no stale observation) must read the same data file. The operator's mapping file uses the same schema, so an operator file extends or overrides the built-in table. Decide the operator surface: --root-cpe beside --root-name/--root-purl for the subject; a mapping file keyed by PURL or name+version for components; and whether --supplement-cdx, which already carries component CPEs and is never overwritten (m1377 FR-013), already covers components. Decide precedence between operator, nixpkgs-declared (m1377) and curated CPEs, starting from FR-013's rule that the operator wins, and record it in waybill:cpe-provenance including the existing overrode value. Validate with the Cpe type (waybill_common::types::cpe::Cpe, #1403): CPE 2.3, named vendor and product, no wildcards; a malformed operator CPE is an error, not a silent drop. Record provenance natively as a third source, operator-supplied: CycloneDX evidence.identity[], the comment field in SPDX 2.3 externalRefs and in SPDX 3 ExternalIdentifier. A scan-time downloadable table is out of scope (#1491 records why). Moving the table must leave every corpus golden byte-identical."

## Context

A component's CPE comes from one of three places today:

1. **Synthesis.** Every component the filesystem scan produces has its CPEs recomputed after dedup (`scan_fs/mod.rs:1348`). For a `pkg:generic` component this is the curated table, keyed by component name. For other ecosystems it is a per-ecosystem heuristic: for example `debian` plus the name for a `deb`, the name for a `cargo` crate, and `python-<name>` for `pypi`.
2. **nixpkgs-declared** (m1377). A closure member whose nixpkgs attribute declares `meta.identifiers.cpe` carries that CPE. It replaces a curated one, and C212 `waybill:cpe-provenance` records what it `overrode`.
3. **`--supplement-cdx`** (m119). An operator's CycloneDX supplement may carry component CPEs.

**The description's premise about `--supplement-cdx` is only half true.** Read from the code on 2026-11-13:

- **A component only the supplement declares** carries the supplement's CPE.
- **A closure component** keeps the supplement's CPE, because the supplement merges before the closure tier fills empty CPE slots. This is the case m1377 FR-013 describes.
- **Every other component the scanner also found:** the scanner wins on `cpe` (`supplement/conflict.rs:355`, m119's "bytes-derived" partition). The CPE the scanner holds at merge time is the synthesized one from step 1. A curated row or a heuristic guess therefore beats the operator's explicit value. The operator's value survives only as a `waybill:declared-cpe` annotation and a `waybill:assertion-conflict` record.

So for most components the operator does **not** win today. The CPE that wins is a guess, not bytes-derived.

**The subject.** Since #1402, a document subject carries a CPE only when the selected main module has one of its own. A subject named with `--root-name` or `--root-purl` carries none, and nothing lets the operator supply one.

**The curated table is Rust source.** It has two parts in `waybill-cli/src/generate/cpe.rs`:

- `GENERIC_LIBRARY_CPES`: 82 rows, each a slug with an ordered list of vendor/product pairs.
- `NVD_DICTIONARY_OBSERVED`: 95 `(vendor, product, entries)` observations, test-only.

Their rationale lives in comments, and their title evidence in six `nvd-titles` files under four spec directories.

Some behaviour that the table's rows depend on is code, not data:
- `openjdk` keeps only the leading numeric version;
- `go` has its own arm;
- `boringssl` is deliberately omitted;
- the rejected names are listed after the table.

**What no corpus golden exercises.** No corpus target passes `--supplement-cdx`, and no golden carries `waybill:declared-cpe`. Every operator-path change below is therefore invisible to the goldens. Only the table move touches code that every golden runs through.

## Clarifications

### Session 2026-11-13

- Q: When a `--supplement-cdx` CPE names a component the scanner also found, which CPE is emitted? → A: The operator's. Its CPEs replace the synthesized list, the conflict record is kept with the winner flipped, and the displaced primary is recorded as C212 `overrode`. Only m119's `cpe` rule is reversed (FR-003).
- Q: What key does a `--cpe-map` row match components by? → A: A versionless PURL: type, namespace and name. A version-exact statement is the supplement's job (FR-004).
- Q: Does a `--cpe-map` row beat a nixpkgs-declared CPE? → A: Yes. Every operator surface outranks every upstream source, and C212 records the nixpkgs value as `overrode` (FR-005).
- Q: How is a malformed operator CPE handled? → A: A bad `--root-cpe` or `--cpe-map` row is a usage error, and the scan exits before scanning. A bad `--supplement-cdx` CPE keeps #1403's WARN-and-drop (FR-006).
- Q: How does this ship? → A: As two stacked PRs (FR-017). PR A is the table move (US4) and the shared schema and parser. PR B merges A's exact head and adds US1–US3.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An operator names the CPE of their own product (Priority: P1)

An operator scans in-house software and names the subject with `--root-name`/`--root-purl`. They know their product's NVD CPE and want the document subject to carry it. A scanner that matches only on CPE can then find the product, and the document meets the CISA 2026 "all known identifiers" element.

**Why this priority**: The subject is the one component no table or upstream can know. Today it has no path to a CPE at all.

**Independent Test**: Scan any fixture with `--root-name` and one `--root-cpe`. The subject carries that CPE in all three formats, with operator-supplied provenance. Without the flag, the output is byte-identical to today.

**Acceptance Scenarios**:

1. **Given** a scan with `--root-name acme-app --root-version 2.1.0 --root-cpe cpe:2.3:a:acme:acme_app:2.1.0:*:*:*:*:*:*:*`, **When** the document is emitted, **Then**:
   - the subject's primary CPE is that value in CycloneDX `metadata.component.cpe`, the SPDX 2.3 subject package's `externalRefs` and the SPDX 3 subject's `externalIdentifier`;
   - each carries operator-supplied provenance.
2. **Given** a main module that already has a synthesized CPE, **When** `--root-cpe` is passed, **Then**:
   - the operator's CPE is primary, and no synthesized candidate is kept beside it;
   - the displaced primary is recorded as `overrode`.
3. **Given** `--root-cpe cpe:2.3:a:*:acme_app:...` (wildcard vendor), or a value that is not CPE 2.3, **When** the scan starts, **Then** it exits non-zero before scanning, naming the flag and the reason. No document is written.
4. **Given** `--root-cpe` passed twice, **When** emitted, **Then**:
   - the first value is the primary CPE;
   - the second is a secondary CPE on the subject, in the same carriers secondary CPEs use today;
   - both carry operator provenance.

---

### User Story 2 - An operator's CPE beats a synthesized guess (Priority: P1)

An operator supplies component CPEs, because the curated table has no row for a product or the heuristic guesses the wrong vendor. They expect their value to be the one a scanner sees. Today, for a component the scanner also found, the synthesized guess wins.

**Why this priority**: This is a correctness defect in an existing surface. Operator-supplied identifiers are silently demoted to an annotation, and nothing in the primary CPE field shows it.

**Independent Test**: Supplement a scanned `deb` component, which gets a heuristic `debian:` CPE, with the correct NVD CPE. The emitted primary CPE is the operator's, and the displaced value is recorded.

**Acceptance Scenarios**:

1. **Given** a component the scanner found with a synthesized CPE, and a supplement for the same PURL carrying a valid CPE, **When** emitted, **Then**:
   - the supplement's CPEs replace the synthesized list;
   - the first displaced CPE is recorded as `overrode`;
   - the component's CPE carries operator provenance in all three formats.
2. **Given** a closure component with a nixpkgs-declared CPE and a supplement CPE for its PURL, **When** emitted, **Then** the operator's CPE is primary, and the nixpkgs-declared one is recorded as `overrode`.
3. **Given** a supplement CPE identical to what the scanner would have emitted, **When** emitted, **Then**:
   - the CPE is unchanged;
   - its provenance is operator-supplied;
   - no `overrode` value is recorded.
4. **Given** a supplement CPE that is malformed, **When** merged, **Then** it is dropped with a WARN, as #1403 settled. The component keeps its synthesized CPE, and the supplement's other fields still merge.

---

### User Story 3 - An operator extends the curated table for their estate (Priority: P2)

An operator scans many images that contain the same in-house or uncatalogued library. They want one file that says "this package is NVD's `vendor:product`", applied wherever the package appears, at whatever version it is. They do not want a supplement entry per PURL per version.

**Why this priority**: It covers every scan of a fleet with one file. It depends on US4's schema existing.

**Independent Test**: Write a mapping file with one row keyed `pkg:deb/debian/libfoo` → `foo_project:libfoo`. Scan a fixture containing `libfoo` 1.2-3. The component's primary CPE is `cpe:2.3:a:foo_project:libfoo:1.2-3:…`, with operator provenance.

**Acceptance Scenarios**:

1. **Given** a mapping-file row keyed by a versionless PURL, **When** a component matches that PURL's type, namespace and name, **Then**:
   - its CPEs are built from the row's pairs and the component's own version, as curated rows are today;
   - they replace any synthesized or curated list.
2. **Given** a mapping-file row with the same key as a built-in curated row, **When** a matching component is scanned, **Then**:
   - the operator's row is used;
   - the curated primary it displaced is recorded as `overrode`.
3. **Given** a component matched by both a mapping-file row and a supplement CPE for its exact PURL, **When** emitted, **Then** the supplement's CPE wins: it is the more specific statement.
4. **Given** a mapping file with a malformed row, an unknown field, a duplicate key, or a pair that does not form a valid CPE 2.3, **When** the scan starts, **Then** it exits non-zero, naming the file, the row and the reason. No document is written.
5. **Given** a mapping-file row that matches nothing in the scan, **When** the scan ends, **Then** the output is as if the row were absent. The count of unused rows is logged at INFO.

---

### User Story 4 - A maintainer edits the curated table as data (Priority: P2)

A maintainer adds or corrects a curated row. Today that is a Rust edit in three places: the row, the dictionary observation and a comment pointing at the evidence. They want one row in one data file that carries the pairs, the observation and the evidence path. The existing enforcement tests should catch a row that lacks any of them.

**Why this priority**: It makes the table maintainable and gives US3 its schema. It changes no output.

**Independent Test**: After the move, every corpus golden is byte-identical, and every enforcement test reads the data file. Mutation: delete a row's dictionary observation, then point a row's evidence path at a missing file. Each fails a test.

**Acceptance Scenarios**:

1. **Given** the data file replaces both Rust tables, **When** the full corpus is regenerated, **Then** every golden in every format is byte-identical to the goldens at the branch base.
2. **Given** a row whose pair has no dictionary observation, whose evidence file does not exist, or whose evidence file lacks a title query for its product, **When** the tests run, **Then** a test fails naming the row.
3. **Given** a row out of sort order, a duplicate slug, or an observation for a pair no row emits, **When** the tests run, **Then** the existing sorted, no-duplicate and no-stale-observation tests fail, reading the data file.
4. **Given** the data file, **When** searched for vulnerability counts or advisory identifiers, **Then** none are present.

---

### Edge Cases

- **A subject named with `--root-purl` that has a matching mapping-file row:** the row applies to the subject, as to any component. `--root-cpe` beats it.
- **`--root-cpe` together with a supplement `metadata.component` CPE:** the subject is not a supplement component (m119 FR-014), so the flag is the only subject path. A CPE on the supplement's `metadata.component` is ignored, with a WARN.
- **`--root-cpe` together with `--split`:** rejected as a usage error. A split run emits one document per subproject, each with its own subject, and one CPE cannot name them all. Per-subproject CPEs come from `--supplement-cdx` or `--cpe-map`.
- **An operator CPE whose version differs from the component's version:** emitted as given. The operator owns the statement, and waybill does not rewrite it. A WARN names the component.
- **A mapping row for a `pkg:generic` name that the built-in table deliberately omits** (`boringssl`) **or rejects:** the operator's row is used. The built-in decision is a default, not a block.
- **A component whose version is empty:** a mapping-file row emits nothing for it, exactly as a curated row does today. A wildcard-version CPE is never built. `--root-cpe` and supplement CPEs are full CPEs, so they are unaffected.
- **Per-row version handling that is code today** (`openjdk` truncation, the `go` arm): preserved by the move. An operator row gets the plain component version.
- **A closure scan** in which the operator wins over a nixpkgs-declared CPE: C212 is emitted with the operator basis and the declared value as `overrode` (FR-009). The closure tier's C209 coverage counts are unchanged in meaning.
- **A component matched by two operator surfaces** (map and supplement): the supplement's list wins whole (FR-005). The lists are never merged.

## Requirements *(mandatory)*

### Functional Requirements

**Subject (US1)**

- **FR-001**: The scan command MUST accept `--root-cpe <cpe>`, repeatable. The first value is the subject's primary CPE, and the rest are secondary CPEs. It applies to the selected subject when that is a scanned main module or an `--root-name`/`--root-purl` override.

  **Amended at implementation (2026-11-13):** a synthetic *placeholder* subject is rejected with a usage error, and no document is written. A placeholder is named after the scan directory, so a CPE on it names nothing the operator chose. On such a scan SPDX 2.3 also roots the document at the single top-level component, not at the placeholder that CycloneDX and SPDX 3 use, so the CPE would land on a dependency or nowhere (observed on a one-package dpkg fixture). The remedy is to name the subject. A Maven-coordinate subject is rejected for the same reason: each emitter builds it separately.
- **FR-002**: When `--root-cpe` is given, the subject's CPE list MUST be exactly the operator's values. Any synthesized, curated or declared CPE the subject would otherwise carry is not kept.

**Components (US2, US3)**

- **FR-003**: A valid CPE from `--supplement-cdx` for a PURL the scanner also found MUST replace that component's CPE list. This changes m119's partition: `cpe` moves from scanner-wins to operator-wins. The conflict record is kept, with the winner changed, so the displaced value stays visible. Other fields' partitions are unchanged.
- **FR-004**: The scan command MUST accept a CPE mapping file (`--cpe-map <path>`, one file). Its rows MUST use the same schema as the built-in data file (FR-011). A row is keyed by a versionless PURL: type, optional namespace and name. Its pairs build CPEs with the matching component's own version. A built-in row's key is the versionless PURL `pkg:generic/<slug>`.
- **FR-005**: Precedence MUST be, highest first, for each component:
  1. `--root-cpe` (subject only);
  2. `--supplement-cdx` CPE for the exact PURL;
  3. `--cpe-map` row;
  4. nixpkgs-declared (m1377);
  5. curated built-in row;
  6. ecosystem heuristic.

  The highest source present supplies the whole list. Lower sources contribute nothing beside it.

**Validation**

- **FR-006**: Every operator CPE MUST be validated as a `Cpe` (#1403): CPE 2.3 formatted string, named non-wildcard vendor and product.
  - A malformed `--root-cpe`, or any malformed `--cpe-map` row, is a usage error. The scan exits non-zero before scanning, and no document is written.
  - A malformed `--supplement-cdx` CPE keeps #1403's behaviour: it is dropped with a WARN, and the rest of the supplement merges.

**Provenance**

- **FR-007**: A component or subject whose emitted CPEs came from an operator surface MUST record "operator-supplied" as their source natively, as a third source beside nixpkgs-declared and curated:
  - CycloneDX `evidence.identity[]` with `field: "cpe"`;
  - SPDX 2.3 `externalRefs[].comment`;
  - SPDX 3 `ExternalIdentifier.comment`.

  The fixed strings name the surface (`--root-cpe`, `--supplement-cdx`, `--cpe-map`). This applies to every component, not only closure members.
- **FR-008**: The three formats MUST agree on which components carry operator provenance, and on its value. The existing C212 parity row stays `SymmetricEqual`.
- **FR-009**: C212 `waybill:cpe-provenance` MUST be emitted on every component whose CPE an operator surface supplied **in place of a different primary**. That includes a subject that is a scanned component. *Amended 2026-11-13:* an operator CPE that displaced nothing (no prior CPE, the same primary, or a supplement-only component) carries no C212, and any C212 a lower source attached is removed. Its `basis` alone would restate the surface the native comment and CycloneDX method already name (Principle V); `overrode` is the part no format can carry. A synthetic or `--root-name`/`--root-purl` subject has no component scope and nothing to override, because it has carried no CPE since #1402. Its source is recorded natively only (FR-007).
  - `basis` names how the operator value was joined: `root-cpe`, `purl` for a supplement exact-PURL match, or `cpe-map` for a mapping row.
  - `overrode` is the primary CPE the next-highest source (FR-005) would have emitted. On an operator CPE it is always present, since its absence means no C212.
  - C212's existing `drv`/`name-version` values and their meaning are unchanged.
  - The catalog row in `docs/reference/sbom-format-mapping.md` is updated to say C212 is no longer closure-only.
- **FR-010**: A curated or heuristic CPE on a non-closure component MUST stay without recorded provenance, as today. Extending curated provenance beyond closure members would change every golden, and is out of scope.

**The data file (US4)**

- **FR-011**: The curated table MUST live in one committed data file, embedded in the binary at build time. The file replaces both `GENERIC_LIBRARY_CPES` and `NVD_DICTIONARY_OBSERVED`. Each row carries:
  - the key;
  - the ordered vendor/product pairs, the first being primary and the rest secondary;
  - for each pair, the NVD dictionary entry count, the observation date, and two evidence file paths relative to the repository root: one for the dictionary count and one for the title query (research R2).

  It carries no CVE counts, advisory identifiers or other vulnerability data.
- **FR-012**: Behaviour that rows depend on today MUST be preserved exactly: the `openjdk` version truncation, the `go` arm, the `boringssl` omission and the rejected names. The data file has no field for these special cases. Whether they become data fields or stay code is a plan decision.
- **FR-013**: The enforcement tests MUST read the data file, and stay hermetic, with no network. That covers sorted rows, valid CPE 2.3, no duplicate key, every pair observed in the dictionary, no stale observation, and every product having a committed title query. Three checks are added:
  - every evidence path exists;
  - every pair has a non-zero entry count, equal to the count its dictionary evidence file records;
  - a pair shared by two rows carries identical evidence in both.
- **FR-014**: Moving the table MUST NOT change any emitted byte. Every corpus golden and every in-repo golden is byte-identical at the end of the move.

**Scope**

- **FR-015**: Waybill MUST NOT fetch a CPE table or mapping file at scan time. `--cpe-map` reads a local file only. A table downloaded at scan time is out of scope, for the reasons recorded in #1491.
- **FR-016**: With none of `--root-cpe`, `--cpe-map` or `--supplement-cdx` given, output MUST be byte-identical to the branch base.

**Delivery**

- **FR-017**: The work MUST ship as two stacked PRs:
  - **PR A** delivers FR-011–FR-014: the data file, and the schema and parser that `--cpe-map` will reuse. Its non-regenerating corpus run is the evidence for FR-014 on its own.
  - **PR B** merges PR A's exact head and delivers FR-001–FR-010 and FR-015–FR-016. It changes no golden.
  - The two PRs' CHANGELOG entries are not adjacent.

### Key Entities

- **CPE mapping row:** a key (versionless PURL) and an ordered list of vendor/product pairs. Each pair has its dictionary evidence: entry count, observation date and evidence path. Built-in rows and operator rows share the schema. An operator row may omit the evidence fields.
- **CPE source:** where a component's emitted CPEs came from. One of operator (root flag, supplement, map), nixpkgs-declared, curated or heuristic. Only the first three are recorded on the wire for every component, and curated only on closure members (FR-010).
- **Override record (C212):** the join basis of a non-curated CPE and the primary CPE it displaced.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With `--root-cpe` on a fixture whose subject is named by `--root-name`, the subject carries the operator CPE in 3 of 3 formats, with operator provenance in 3 of 3. Before this change it carries one in 0 of 3.
- **SC-002**: On a fixture with a supplemented `deb`, `pypi` and `pkg:generic` (curated) component, the emitted primary CPE equals the operator's in 3 of 3 components, in all three formats. On the branch base it equals the operator's in 0 of 3.
- **SC-003**: On a fixture where a `--cpe-map` row matches 2 components at different versions, both carry the mapped vendor and product at their own versions.
- **SC-004**: Every malformed-input case in US1 scenario 3 and US3 scenario 4 exits non-zero and writes no document.
- **SC-005**: After the table move, 100% of corpus goldens and in-repo goldens are byte-identical to the branch base, measured by a non-regenerating corpus run.
- **SC-006**: The curated table's rows, observations and evidence paths are in one file. Adding a row touches that file and its evidence, and no Rust source.
- **SC-007**: Each new and changed test fails under a mutation that disables the rule it covers: precedence order, operator provenance, C212 `overrode`, malformed-input rejection, and each data-file check in FR-013.

## Assumptions

- **Rule changes.** FR-003 deliberately reverses m119's `cpe` rule. That rule called a CPE "bytes-derived", but at merge time every scanner CPE is synthesized: no production reader sets one from artifact bytes (checked 2026-11-13; the only non-synthesized source is nixpkgs-declared, which FR-005 also ranks below the operator). If a future reader derives a CPE from bytes, its rank is a decision for that reader's spec.
- **Mapping-file key.** The key is a versionless PURL, not name+version. A bare name is ambiguous across ecosystems. A version belongs to the component, and the operator who wants a version-exact statement already has `--supplement-cdx`.
- **File format.** The built-in file's text format is a plan decision, subject to FR-011. It must be readable and diffable one row per product, and the parser shared with `--cpe-map`. #966's TSV is the precedent for embedding.
- **Root flag scope.** `--root-cpe` has no effect without a subject, and every scan has one, so it is never a no-op. It does not require `--root-name`.
- **Unused rows.** The INFO log of unused map rows is a diagnostic, not a wire record. C209 `waybill:cpe-coverage` gains no field, because that would change every golden.
- **Retiring rows.** Upstreaming curated rows to nixpkgs (#1491's last section) is separate work. It needs no change here, because nixpkgs-declared already outranks curated.
