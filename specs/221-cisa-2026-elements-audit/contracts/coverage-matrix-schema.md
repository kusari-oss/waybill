# Contract: `docs/cisa-2026-coverage.md` schema

**Feature**: 221-cisa-2026-elements-audit
**Deliverable owner**: US1 (P1)
**Machine-checked**: `waybill-cli/tests/cisa_2026_coverage_matrix.rs`

The coverage document is both human-readable and machine-parseable.
This contract fixes the structure so the integration test can load
the doc, walk every ✅ row, and verify against a live scan.

---

## File location

`docs/cisa-2026-coverage.md` (committed at repo root, in the
existing `docs/` tree per project convention).

## Front-matter (mandatory)

```yaml
---
cisa-publication: "2026 Minimum Elements for a Software Bill of Materials (SBOM)"
cisa-publication-date: 2026-07-29
cisa-publication-tlp: TLP:CLEAR
waybill-milestone: 221
last-verified: 2026-07-29     # updated when the integration test runs green
---
```

**Rationale**: FR-015 requires citation of the CISA publication
(title, date, TLP). Front-matter keeps this machine-parseable and
lets consumers correlate the coverage snapshot with a specific
waybill milestone/version.

## Body structure

Two H2 sections in fixed order:

1. `## Data Fields (17)` — matrix table with rows for the 17 data
   elements from CISA § SBOM Metadata (9) + § Component Data (8).
2. `## Practices & Processes (6)` — narrative rows for the 6
   organizational practices (not payload gaps).

### Data Fields matrix table

Exact Markdown table shape:

```markdown
| # | Element (CISA 2026) | Category | Change (vs 2021) | CDX 1.6 | SPDX 2.3 | SPDX 3.0.1 | Notes |
|---|---------------------|----------|-------------------|---------|----------|-----------|-------|
| 1 | SBOM Author | Metadata | Major Update | ✅ `metadata.authors[]` at `waybill-cli/src/generate/cyclonedx/metadata.rs:798` | ✅ `creationInfo.creators[]` at `spdx/document.rs:XXX` | ✅ `CreationInfo.createdBy` at `spdx/v3_document.rs:XXX` | m080 flow-through for `--creator` operator supplied |
```

**Per-cell format for the three emitter columns**:

- **`✅ <slot>` at `<file>:<line>`** — native field. Integration test
  extracts the slot path and runs the corresponding jq recipe (see
  jq-recipe registry below) to assert non-empty on a live scan.
- **`⚠️ <slot>` at `<file>:<line>` (annotation-only)** — waybill:*
  property. Test asserts the property key is present. Follow-up
  reference (`See US3`) required.
- **`❌ (see US2)`** — absent. Test asserts *no* attempt is made to
  emit. Follow-up user-story link required.

**Change column** enum: `New`, `Major Update`, `Minor Update`,
`Removed`, `Unchanged`.

### Practices & Processes narrative rows

Exact Markdown shape:

```markdown
### Frequency (Minor Update)

**CISA text**: > "Each software version or update should have an
associated SBOM..." (page 14)

**Classification**: **Organizational practice** — not a payload
element. Consumer indexing this element looks for evidence in
operator workflows, not in the SBOM document itself.

**How waybill enables the operator to satisfy this**:
- Every `waybill scan` invocation regenerates the SBOM from
  scratch with a fresh `serialNumber` (CDX) and content-addressed
  `documentNamespace` / `@id` (SPDX per m010).
- No caching of prior SBOMs means every emission reflects the
  current state of the target.
- Operators pipe `waybill scan` into their CI on every commit,
  every release tag, or every published container image, per
  their policy.
```

**Required subsections per practice**: "CISA text" (verbatim
quote + page number), "Classification" (must state
"Organizational practice"), "How waybill enables the operator
to satisfy this" (bulleted).

### jq-recipe registry

At the bottom of the doc, an appendix titled `## Appendix A —
Reproducible verification recipes`:

```markdown
### Appendix A — Reproducible verification recipes

Every ✅ cell above cites a slot; this appendix gives the exact
`jq` / `yq` recipe to extract the value from a fresh scan.

**Setup** (run once):

```bash
waybill scan ./target-dir \
  --format cyclonedx-1.6 --output /tmp/scan.cdx.json
waybill scan ./target-dir \
  --format spdx-2.3 --output /tmp/scan.spdx.json
waybill scan ./target-dir \
  --format spdx-3.0.1 --output /tmp/scan.spdx3.json
```

**Element: SBOM Author** (row #1)
- CDX: `jq -r '.metadata.authors[].name' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.creationInfo.creators[]' /tmp/scan.spdx.json`
- SPDX 3: `jq -r '.["@graph"][] | select(.["@type"]=="CreationInfo") | .createdBy' /tmp/scan.spdx3.json`

...

**Element: Component Hash Value** (row #12)
- CDX: `jq -r '.components[]?.hashes[]?.content' /tmp/scan.cdx.json`
- ...
```

The integration test parses this appendix by anchor
(`**Element: <name>**` for each row) and executes each recipe
against the corresponding fresh-scan output, asserting the recipe
returns a non-empty JSON value.

---

## Regeneration process

When a subsequent CISA publication or a subsequent waybill
milestone changes any cell:

1. Update the affected row's cell (verdict / slot / file:line).
2. Update `last-verified` in the front-matter to the current date.
3. Run `cargo test --workspace --test cisa_2026_coverage_matrix`
   locally to confirm every recipe still resolves to a non-empty
   value.
4. If a CISA element itself was added/removed, bump the section
   header count (`(17)` → `(18)`) and add/remove the matrix row.
5. If a waybill emitter surface moved (line-number churn), the test
   `--nocapture` output will name the failing cell — update the
   citation.

---

## Integration-test contract summary

The `waybill-cli/tests/cisa_2026_coverage_matrix.rs` test:

- Parses `docs/cisa-2026-coverage.md` front-matter → asserts
  `waybill-milestone: 221` (or greater) and `cisa-publication-date`
  present.
- Parses the Data Fields matrix table → collects `(element_id,
  emitter, verdict)` tuples.
- Parses Appendix A → collects `(element_id, emitter, jq_recipe)`
  tuples.
- Runs `waybill scan` against the milestone-090 fixture repo
  target `transitive_parity/cargo` → gets three emitter outputs.
- For each `Verdict::Native` and `Verdict::Annotation` row: runs the
  jq recipe, asserts result is non-empty. On failure: prints the
  failing element + emitter + expected slot + actual jq output.
- For each `Verdict::Absent` row with a follow-up: asserts the
  follow-up user-story ID matches an existing user story in
  `spec.md`.
- For each Practice row: asserts the three required subsections
  ("CISA text", "Classification", "How waybill enables...") exist.

Failure of the integration test blocks the PR (Principle VII
places it in the `--workspace` set that CI runs).
