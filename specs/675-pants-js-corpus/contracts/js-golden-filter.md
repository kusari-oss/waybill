# Contract — JS-only golden filter (`js_filter` module)

**Module**: `waybill-cli/tests/corpus_harness_195/js_filter.rs` (NEW file)

## Purpose

Filter emitted SBOMs down to the `pkg:npm/*` surface only before layer 2 byte-identity comparison, per Session 2026-09-02 Q1 clarification and FR-008.

## Public API

```rust
pub fn filter_cdx_to_js(v: &mut serde_json::Value);
pub fn filter_spdx23_to_js(v: &mut serde_json::Value);
pub fn filter_spdx3_to_js(v: &mut serde_json::Value);
```

All three MUST:

- **Mutate in place** — the caller in `layer2_golden::compare_golden` operates on a `masked: serde_json::Value` local; filter functions accept `&mut` and modify it.
- **Be idempotent** — applying the filter to already-filtered input yields the same output (bytewise).
- **Preserve JSON ordering of retained elements** — required for the existing byte-identity compare.
- **Not panic on unexpected shapes** — an SBOM missing an expected field (e.g., `.components` absent) MUST silently no-op that field's filter step.

## `filter_cdx_to_js`

### Retention rules

**Keep**:
- Top-level envelope: `bomFormat`, `specVersion`, `serialNumber`, `version` (already masked)
- `.metadata` in its entirety (doc-scope; masked upstream)
- `.components[]` entries where `.purl` (as `&str`) starts with `"pkg:npm/"`
- the `.dependencies[]` entry whose `.ref` equals `.metadata.component["bom-ref"]` — the primary component, whose ref is the operator-supplied root identity and never an npm PURL. Its `.dependsOn[]` is still pruned to the npm surface.
- `.dependencies[]` entries where `.ref` (as `&str`) starts with `"pkg:npm/"` — AND filter each such entry's `.dependsOn` array to only entries starting with `"pkg:npm/"`

**Remove**:
- `.components[]` entries whose `.purl` does not start with `"pkg:npm/"` (or is missing)
- `.dependencies[]` entries whose `.ref` is not a kept component's PURL
- Within retained `.dependencies[]` entries: `.dependsOn` array items pointing at non-npm PURLs

### Edge cases

- **`.components` field missing** → no-op that step.
- **`.dependencies` field missing** → no-op that step.
- **Component missing `.purl`** → treat as non-npm (remove).
- **Root component in `.metadata.component`** → out of scope for filtering (retained as-is per doc-scope rule).

## `filter_spdx23_to_js`

### Retention rules

**Keep**:
- Top-level envelope: `spdxVersion`, `dataLicense`, `SPDXID`, `documentNamespace`, `name` (mostly masked upstream)
- `.creationInfo` in its entirety (masked upstream)
- `.documentDescribes` array as-is (points at the root package's SPDXID, which we retain)
- `.packages[]` entries where any `.externalRefs[]` entry has `.referenceLocator` (as `&str`) starting with `"pkg:npm/"` — collect the SPDXIDs of kept packages into a `HashSet<&str>`
- `.relationships[]` entries where BOTH `.spdxElementId` AND `.relatedSpdxElement` are in the kept-SPDXID set

**Remove**:
- `.packages[]` entries not matching the retention rule (no `pkg:npm/` externalRef)
- `.relationships[]` entries where either endpoint SPDXID is not in the kept set

### Edge cases

- **Root/document package** (typically referenced by `documentDescribes`) — MUST be kept regardless of whether it has a `pkg:npm/*` external ref, since dropping it invalidates `documentDescribes`. Detection: `SPDXID == "SPDXRef-DOCUMENT"` OR the SPDXID appears in `documentDescribes`.
- **Package with multiple externalRefs, only some pkg:npm/*** → keep the package; DO NOT filter its externalRefs array (retain all of them for the retained package).
- **Package with no externalRefs at all** → remove unless it's the root/document package.

## `filter_spdx3_to_js`

### Retention rules

SPDX 3.0.1 uses a JSON-LD `@graph[]` array of typed nodes. Types are keyed by `type` (or `@type` — check both).

**Keep**:
- Top-level envelope: `@context`
- `.@graph[]` nodes where `type` is one of the doc-scope types: `"SpdxDocument"`, `"CreationInfo"`, `"Person"`, `"Organization"`, `"Tool"` (the actor/document node vocabulary)
- The `SpdxDocument`'s `.rootElement[]` (and `.software_rootElement[]`) targets, unconditionally — the graph head, mirroring the SPDX 2.3 `documentDescribes` retention below. Its PURL is `pkg:generic/*`, so the npm test would drop it along with the only edge that says what the project itself depends on.
- `.@graph[]` nodes of type `"software_Package"` OR `"software_File"` where the associated PURL starts with `"pkg:npm/"`. PURL is located at `.software_packageUrl`, or at `.externalIdentifier[].identifier` when `.externalIdentifier[].externalIdentifierType` is **`"packageUrl"`** (the value waybill emits, and the SPDX 3.0.1 vocabulary term) or `"purl"`. Collect kept spdxIds into a `HashSet<&str>`.
- `.@graph[]` nodes of type `"Relationship"` **or `"LifecycleScopedRelationship"`** (waybill spells a scoped dependency edge with the latter, and it is the overwhelming majority of them) where BOTH `.from` and `.to` reference kept spdxIds. `.to` may be an array — filter its members to only kept spdxIds; drop the relationship if `.to` becomes empty.

**Remove**:
- Component nodes (packages, files) not matching the PURL rule.
- Relationship nodes where either endpoint references a removed node (after `.to` array filtering).

### Edge cases

- **Root document node** — always kept (doc-scope).
- **Component with no `externalIdentifier`** — treat as non-npm; remove unless it's a doc-scope type.
- **Relationship with `.to` as string vs array** — handle both. If string and points at removed node, drop the relationship.

## Testing the filter functions

Unit-test the filter functions with hand-authored `serde_json::Value` inputs covering:

1. Happy path: mixed CDX with 3 npm + 3 pypi components → 3 npm components + npm-only edges remain.
2. Missing field: CDX with no `.dependencies` array → filter runs without panic; `.components` still filtered correctly.
3. Idempotency: apply filter twice → output byte-identical to single application.
4. SPDX 2.3 root retention: root document package with no PURL survives filtering.
5. SPDX 3 relationship with mixed `.to` array: filter drops non-npm targets, retains relationship if any npm targets remain, drops relationship if all removed.

Tests live in `#[cfg(test)] mod tests { ... }` inside `js_filter.rs`. Use `serde_json::json!(...)` for input construction. Assertions on structural presence of expected fields, not string-diff of the whole document.

## Non-goals

- **PURL parsing** — filter uses string-prefix matching (`str::starts_with("pkg:npm/")`), NOT purl-spec-compliant parsing. This is deliberate: filter runs on already-emitted-by-waybill PURLs which are known well-formed per Constitution Principle V.
- **Cross-format consistency verification** — filter functions operate on each format independently. Verifying the three filtered outputs are structurally consistent is out of scope (that's parity-catalog territory).
- **Configurable filter** — filter is hard-coded to `pkg:npm/`; not parameterized. If future features need `pkg:pnpm/` or per-target filtering, extend the module then.

## Amendment — 2026-09-27 (issue #881)

Three defects in the SPDX 3 and CycloneDX arms of this contract, each of
which silently removed content the filter was specified to keep:

1. `externalIdentifierType` was compared against `"purl"`; waybill emits
   `"packageUrl"`. Every `software_Package` was dropped.
2. Only `type == "Relationship"` counted as an edge; waybill emits
   `LifecycleScopedRelationship` for 640 of this target's 641 edges. Every
   scoped edge was dropped.
3. Neither the CycloneDX primary component's `dependencies[]` entry nor the
   SPDX 3 `rootElement` was retained, while the SPDX 2.3 arm retained its
   equivalent. Root out-edge counts therefore differed across the three
   goldens for reasons internal to the filter.

Consequences. The committed SPDX 3 golden was 14 nodes — 12 Organizations, a
Tool, the SpdxDocument — for a target whose stated purpose is to
regression-lock 304 npm components and their edges. It asserted nothing, and
it satisfied SC-004's byte ceiling **by** asserting nothing. Defect 3 was read
off the goldens as a CycloneDX emitter bug and filed as issue #881; all three
formats in fact emit one root out-edge on this target.

Testing note, added because the unit tests did not catch any of this: the
pre-existing `spdx3_relationship_mixed_to_array` test passed throughout,
because its fixture was hand-written with the same `"purl"` spelling the
filter looked for. A fixture that encodes the implementation's assumption
cannot test that assumption. Fixtures for this filter must use shapes copied
from a real waybill scan, and the regression tests added under #881 do, with
the counters cross-checked against committed goldens from eleven targets.
