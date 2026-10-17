# Phase 0 research — feature 1156

## R1. Principle V audit: is there a native carrier for "no version by construction"?

**Decision**: KEEP-NO-NATIVE. No construct in CycloneDX 1.6, SPDX 2.3 or
SPDX 3.0.1 expresses *this component has no version to have*. A
`waybill:*` annotation is permitted, and row C197 must carry this audit.

**Method**: read against the schemas vendored at
`waybill-cli/tests/fixtures/schemas/{cyclonedx-1.6,spdx-2.3,spdx-3.0.1}.json`
— the same files the conformance gates validate against — rather than from
recollection of the specs. Every enum below is quoted from those files.

### What each format actually offers

| Format | Version field | Sibling that could explain an absence |
|---|---|---|
| CDX 1.6 | `component.version`, optional, `"The component version. The version should ideally comply with semantic versioning but is not enforced."` It is the only `component` property whose name contains "version". | None. |
| SPDX 2.3 | `packages[].versionInfo`, optional plain `string`, no enum, no sentinel. The only version-bearing package property. Package `required` is `["SPDXID", "downloadLocation", "name"]`. | None. |
| SPDX 3.0.1 | `software_packageVersion`. A full walk of the schema for property names containing "version" returns only unrelated fields (`specVersion`, `simplelicensing_licenseListVersion`, `security_vexVersion`, licence-list bookkeeping). | None. |

### Rejected alternatives

1. **CDX `evidence.identity[]` with `field: "version"`.** This is the
   closest native construct in any of the three formats and it is a real
   candidate: `componentIdentityEvidence.field` is an enum that *includes*
   `version`, alongside a `confidence` number documented as `"The overall
   confidence of the evidence from 0 - 1, where 1 is 100% confidence."`
   Rejected because it describes how well evidence substantiates a
   *concluded value*. `confidence: 0` says "we are not sure what the
   version is" — which is the resolution-failure semantic that C151
   `waybill:unresolved-reason` already carries. Using it here would merge
   the two populations invariant I8 exists to separate, and the structure
   still presumes a value is being evidenced rather than that none exists.

2. **SPDX 2.3 `NOASSERTION`.** Not a permitted value for `versionInfo` in
   the schema (it is a plain string with no enum, and the SPDX sentinel
   convention applies to other fields). Semantically wrong regardless: it
   states the author's posture — no assertion is being made — not a fact
   about the component. The spec's own I6/I8 boundary classifies sentinel
   spellings as a different and worse wire shape: "a sentinel is a
   statement on the wire — a poor one, but a statement".

3. **SPDX 3.0.1 `software_SoftwarePurpose`.** Its enum does contain
   `manifest` and `bom`, which a lockfile arguably is — the nearest thing
   to a native fit anywhere in the three formats. Rejected twice over:
   it names what the component *is*, not that it has no version (the same
   objection that rules out `not-a-package` as the marker's value, and that
   C160 already recorded against this exact enum); and it exists only in
   SPDX 3, so adopting it would leave CDX and SPDX 2.3 with no carrier.
   That is precisely the parity gap Principle V's bridging clause covers.

4. **CDX `component.type`, CDX `compositions[].aggregate`, SPDX 2.3
   `primaryPackagePurpose`.** All audited and rejected by C160 for the
   adjacent "stands for a set, not acquirable" semantic; C197 cites that
   audit rather than repeating it. Verified still accurate against the
   schemas: `component.type` is `[application, framework, library,
   container, platform, operating-system, device, device-driver, firmware,
   file, machine-learning-model, data, cryptographic-asset]`;
   `aggregateType` is ten document-composition completeness values
   (`complete`, `incomplete`, …, `unknown`, `not_specified`), which is a
   statement about a composition rather than about a component in it;
   `primaryPackagePurpose` is twelve values with no manifest/lockfile
   member (`OTHER`, `INSTALL`, `ARCHIVE`, `FIRMWARE`, `APPLICATION`,
   `FRAMEWORK`, `LIBRARY`, `CONTAINER`, `SOURCE`, `DEVICE`,
   `OPERATING_SYSTEM`, `FILE`).

**Consequence for the spec**: Principle V requires the audit result to be
cited in the spec's Functional Requirements, not merely required of the
catalogue row. Spec FR-005a was added to carry it.

## R2. Where does the must-pass check attach?

**Decision**: a separate corpus-level pass in a new `must_pass.rs`, not a
third argument to `verdict()`.

**Rationale**: `verdict(outcome, declared)` is a pure two-input function
implementing an eight-row table that `data-model.md` for feature 1155
documents and that `mod.rs`'s own unit tests pin. Adding a third input
would re-shape that table, invalidate those tests, and couple a new
concern to the oldest contract in the harness.

The precedent for a separate pass is in the same file:
`unfalsifiable_invariants(cells)` (feature 1155 FR-006a) is already a
corpus-level pass that produces lane failures outside `verdict()`. The
must-pass registry is the same shape of thing, so it follows the same
pattern — and the lane already aggregates failures from more than one
source.

**Alternatives considered**: extending `verdict()` (rejected above);
adding an expected-outcome field to `invariant-declarations.toml`
(rejected at clarify time — it changes the meaning of a file every
existing row depends on); making `NotApplicable` fail everywhere with an
opt-out list (rejected at clarify time — many of the 136 cells are
legitimately not applicable and would all need declaring at once).

## R3. What the registry is actually catching that existing gates do not

**Finding**: `unfalsifiable_invariants` already fails the lane when an
invariant is `NotApplicable` on *every* target. That is the corpus-wide
case. It cannot see the per-cell case, which is the one this feature
needs: if the seven Pants anchors disappeared, I8 would still be
applicable on `image-postgres16` and any other target with a versionless
component, so the invariant stays falsifiable while four specific cells
quietly stop meaning anything.

The existing I8 mutation tests do assert a `Pass` baseline — `mutate()`
asserts `before == CellOutcome::Pass` before perturbing — but they select
their subject with `first_passing("I8")`, which returns whichever target
passes first. That anchor floats: drop the anchors and the search silently
picks a different target, and every test stays green. This is the specific
hole FR-012 closes, and the reason FR-012 requires named targets.

## R4. Cost and shape of the golden refresh — measured

**Measured**: the last three `public-corpus.yml` runs took **5m04s, 5m02s
and 4m36s** wall clock (`gh run list --workflow=public-corpus.yml`,
2026-10-16: 06:42:05→06:47:09Z, 04:48:42→04:53:44Z, 03:48:56→03:53:32Z).
The workflow's own header comment estimates ~30 min cold-cache and sets
`timeout-minutes: 45`.

**Consequence**: the refresh is roughly a 10–15 minute round trip for two
dispatches plus the review between them, not an afternoon. The tasks list
should budget it as two waits, and should not try to parallelise them —
the second dispatch validates what the first produced.

**Also measured**: today's 06:42 nightly was green on `main`. The goldens
are therefore current, which is what makes FR-015 and SC-006 checkable —
any difference beyond the marker is a real finding rather than accumulated
drift.

**Dispatch form** (`docs/development/refreshing-corpus-goldens.md`):

```sh
gh workflow run "Public corpus regression" -f branch=<branch> -f regen_goldens=true
```

`-f branch=`, never `--ref`. `--ref` silently runs against `main` and
reports success — the footgun documented in #1223.

## R5. Emission and extractor cost

**Finding**: a per-component string annotation is a fixed, small cost in
this codebase, verified by reading C160's implementation:

- 1 `ParityExtractor` row in `parity/extractors/mod.rs`;
- 3 one-line macro invocations — `cdx_anno!(c197_cdx, "…", component)`,
  `spdx23_anno!(…)`, `spdx3_anno!(…)` — matching `c160_cdx` at
  `cdx.rs:969`, `c160_spdx23` at `spdx2.rs:668`, `c160_spdx3` at
  `spdx3.rs:752`;
- 3 import-list additions (the three long `use` lists at `mod.rs:63`,
  `:74`, `:85`);
- 1 catalogue row in `docs/reference/sbom-format-mapping.md`.

`Directionality::SymmetricEqual`, `order_sensitive: false`, matching C151
and C160 — the value is a single scalar, so there is no ordering to get
wrong and no asymmetry between formats.

**Constants home**: `pants_resolve.rs` already holds
`ANNOTATION_KEY = "waybill:pants-resolve"` (`:36`) and
`NAMESPACE_KEY = "waybill:pants-resolve-namespace"` (`:310`), and both
readers import from it. The marker's key and value constants go beside
them so neither reader holds a literal the other must match by hand.

**Emission sites**: `pants/lockfile.rs:382` and `pants_jvm/lockfile.rs:495`,
both `resolve_component_entry`, both already building an
`extra_annotations` map that includes `waybill:component-kind`.

## R6. Does the catalogue gate constrain the prose note?

**Finding**: no. `invariants/catalog.rs` parses
`docs/reference/sbom-invariants.md` into `CatalogRow { id, statement,
precondition, kinds, retired }` and gates those fields against `REGISTRY`
in both directions. I8's `statement`, `precondition` and `kinds` are
unchanged by this feature — only which annotations the checker accepts
changes, which is internal to the checker.

The note at `sbom-invariants.md:181` ("`waybill:component-kind` =
`lockfile-resolve` is **not** accepted …") is prose outside the parsed
fields, so rewriting it is not gated by a test. FR-008 requires it anyway,
because the note currently argues the opposite of what the code will do
and an ungated wrong sentence is still a wrong sentence — and this one
pre-specifies the fix, so it is the first thing a reader will check.
