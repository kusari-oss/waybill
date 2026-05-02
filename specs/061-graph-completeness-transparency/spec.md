# Feature Specification: SBOM graph-completeness transparency signal (closes #119)

**Feature Branch**: `061-graph-completeness-transparency`
**Created**: 2026-05-02
**Status**: Draft
**Input**: User description: closes #119 (filed during #118 review). Per Constitution Principle X (Transparency), when mikebom can't fully resolve the dependency tree (e.g., offline + empty cache + indirect-only requires), it MUST signal that limitation in the SBOM output so consumers can distinguish "genuinely unused dep" from "mikebom couldn't compute the tree." Two granularities: document-level `mikebom:graph-completeness` annotation + per-component `mikebom:orphan-reason` on orphan components. **Blocks alpha.11** because shipping #118 (graph topology fix) without #119 leaves consumers with orphan components they can't interpret.

## Clarifications

### Session 2026-05-02

- Q: Per-component vs document-level signal? → A: **Both.** Document-level (`mikebom:graph-completeness`) tells consumers "the graph as a whole is partial" with a global reason. Per-component (`mikebom:orphan-reason`) tells consumers WHY each specific orphan exists. Two granularities serve different consumer use cases (filter the whole SBOM vs. triage individual components).
- Q: Native-field audit per Constitution Principle V? → A: **No native CDX/SPDX field for graph completeness.** CDX 1.6 has `metadata.properties[]` for SBOM-level annotations (used by `mikebom:generation-context` C21, `mikebom:os-release-missing-fields`, etc.). SPDX 2.3 + SPDX 3 have document-level `Annotation`. Per-component is the existing milestone 023 generic `extra_annotations` bag (auto-emits across all 3 formats). Two new catalog rows: C44 (document-level) + C45 (per-component).
- Q: How is "completeness" defined? → A: **`complete` = zero orphans; `partial` = one or more orphans; `unknown` = not applicable** (e.g., trace-mode SBOMs where the graph isn't constructed via the same pipeline). For now, milestone 061 implements only the Go-ecosystem signal (since #118's orphan condition is Go-specific). Other ecosystems aggregate into the doc-level signal as they gain main-module / resolver pathways (#104 follow-ups).
- Q: What goes in the `graph-completeness-reason` free-text? → A: **Per-ecosystem reason string in the form `<ecosystem>:<reason-class>`** — e.g., `go:offline-empty-cache`, `go:goproxy-off`, `go:proxy-fetch-failed`. Multiple reasons in one scan join as a comma-separated list. The reason classes mirror the milestone 055 ladder summary's `fetch_errors` keys + a default `unresolved-indirect-require` for the no-fetch-attempted case.
- Q: What goes in per-component `mikebom:orphan-reason`? → A: **Open-enum string** per the issue's proposal: `unresolved-indirect-require` (no resolver path supplied edges; component reached only via `// indirect` requires that resolution couldn't trace), `private-module` (matched `GOPRIVATE`; resolver intentionally skipped), `proxy-fetch-failed` (HTTP 4xx/5xx/timeout from `$GOPROXY`). Default if classifier can't pick: `unresolved-indirect-require`. Three-state semantics: absent on non-orphan components.
- Q: How does the resolver classify orphan reasons? → A: **Cross-reference the FR-009 ladder summary's `fetch_errors` map.** When the resolver returns a `LadderSummary` with non-zero fetch errors, the classifier knows specific modules failed to fetch (proxy-fetch-failed). When `\$GOPROXY=off` or `--offline` was set and the orphan happens to be a `// indirect`-only require, the reason is `unresolved-indirect-require`. The resolver currently doesn't track per-module fetch failures back to the orphan classification; this milestone adds a small map carrying `(failed_module → error_class)` from the resolver into `legacy::read()`.
- Q: Block alpha.11 on this? → A: **Yes.** Per #119 reviewer recommendation: shipping #118 (graph topology fix that creates orphans) without #119 (transparency about WHY orphans exist) violates the spirit of Constitution Principle X. Consumers reading post-#118 SBOMs would see orphans and draw wrong conclusions ("dead dep" vs "couldn't resolve").

## Investigation findings

`legacy::read()` post-#118 already computes orphan count via the FR-004 graph-reachability summary:

```rust
let mut incoming_count: HashMap<&str, usize> = HashMap::new();
for entry in &out {
    if entry.purl.as_str().starts_with("pkg:golang/") {
        incoming_count.entry(&entry.name).or_insert(0);
    }
}
for entry in &out {
    for child_path in &entry.depends {
        if let Some(c) = incoming_count.get_mut(child_path.as_str()) {
            *c += 1;
        }
    }
}
let go_component_count = incoming_count.len();
let orphan_count = incoming_count.values().filter(|&&c| c == 0).count();
```

This data needs to flow OUT of `legacy::read()` so:
1. Orphan components get their `extra_annotations["mikebom:orphan-reason"]` populated **inside** `read()` (small change, mostly mechanical).
2. The aggregate (`go_graph_completeness` + reason) propagates UP via the existing `GoScanSignals` return value, then into `ScanDiagnostics`, then into the format emitters' `metadata.properties` / document-level annotation builders.

Existing infrastructure (verified):
- `ScanDiagnostics` (`scan_fs/package_db/mod.rs:267`) is the documented place for scan-time signals. Currently has `os_release_missing_fields: Vec<String>`. Open-ended per its own doc comment ("future scan-time diagnostics ... can be added without churning cross-module signatures").
- Doc-level emission is wired through `cyclonedx/metadata.rs::build_metadata` (CDX), `spdx/annotations.rs::build_document_annotations` (SPDX 2.3), `spdx/v3_annotations.rs::build_document_annotations` (SPDX 3). Each takes individual params (e.g., `os_release_missing_fields: &[String]`) and emits an entry into the document's `metadata.properties[]` / `annotations[]` array.
- Per-component emission via `extra_annotations` is automatic across all 3 formats per milestone 023.

The fan-out: ~30 LOC in `legacy::read()` for orphan classification + `extra_annotations` population, ~5 LOC per format for the doc-level emission, ~10 LOC of plumbing for the aggregate value, parity-catalog rows in CDX/SPDX2/SPDX3, sbom-format-mapping.md updates.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — SBOM consumer can interpret orphan components (Priority: P1)

A security analyst opens a mikebom-generated SBOM for a Go project scanned with `--offline` and an empty `$GOMODCACHE`. The graph (post-#118) has several orphan components — `pkg:golang/...` entries with no incoming `dependsOn` edges. Pre-061, the analyst can't tell whether the orphan means "dead dep, remove it" or "mikebom couldn't compute the tree." Post-061, the document-level `mikebom:graph-completeness: partial` annotation + each orphan's `mikebom:orphan-reason: unresolved-indirect-require` annotation tell the analyst exactly what's happening: this is a scan-environment limitation, not a dead dep.

**Why this priority**: This is the constitutional fix per Principle X. Without it, #118's graph-topology improvement creates a NEW class of consumer confusion (orphans without context). Shipping alpha.11 without #119 leaves consumers worse off than alpha.10 in the offline+empty-cache case.

**Independent Test**: Scan the existing `tests/fixtures/go/simple-module/` fixture with `--offline` (simulating the worst case). Assert:
1. The CDX output's `metadata.properties[]` contains `{"name": "mikebom:graph-completeness", "value": "partial"}` and `{"name": "mikebom:graph-completeness-reason", "value": "go:unresolved-indirect-require"}` (or similar).
2. SPDX 2.3 output's document-level `annotations[]` contains the equivalent envelope.
3. SPDX 3 output's document-level annotations contain the equivalent.
4. Each orphan component's `properties[]` (CDX) / `annotations[]` (SPDX 2.3 + 3) contains `{"name": "mikebom:orphan-reason", "value": "unresolved-indirect-require"}`.

**Acceptance Scenarios**:

1. **Given** a Go scan that produced N go.sum components with M orphans where M > 0, **When** mikebom emits the SBOM, **Then** the document carries `mikebom:graph-completeness: partial` AND `mikebom:graph-completeness-reason: go:<reason>`.
2. **Given** a Go scan that produced N go.sum components with 0 orphans (cache populated, all transitive edges resolved), **When** mikebom emits the SBOM, **Then** the document carries `mikebom:graph-completeness: complete` AND no `*-reason` annotation.
3. **Given** a non-Go scan (only npm components, no Go ecosystem), **When** mikebom emits the SBOM, **Then** the document carries NO `mikebom:graph-completeness` annotation (the signal is currently Go-only; absent = "not applicable").
4. **Given** an orphan component (no incoming `dependsOn`), **When** mikebom emits it, **Then** the component carries `mikebom:orphan-reason: <classification>`.
5. **Given** a non-orphan component (≥ 1 incoming `dependsOn`), **When** mikebom emits it, **Then** the component MUST NOT carry `mikebom:orphan-reason` (three-state semantics).

### User Story 2 — Reason field aggregates per-orphan classifications (Priority: P2)

A maintainer triaging a partial SBOM wants the doc-level reason field to summarize what went wrong, not just say "partial." If the orphans broke down as 3 from `proxy-fetch-failed` + 5 from `unresolved-indirect-require`, the doc-level reason should communicate both classes, e.g., `go:proxy-fetch-failed,go:unresolved-indirect-require`.

**Why this priority**: Operationally useful but P2 because the per-component annotation already carries the per-orphan reason. The doc-level reason is a quick-glance summary.

**Independent Test**: Construct a scan scenario producing both `proxy-fetch-failed` orphans AND `unresolved-indirect-require` orphans. Assert the doc-level `mikebom:graph-completeness-reason` contains both class names.

### Edge Cases

- **Trace-mode SBOM** (no scan_fs path): doesn't go through the orphan classifier. The doc-level annotation is absent (= "not applicable / not classified"). Future trace-mode work could add its own completeness signal; out of scope for 061.
- **Multi-ecosystem scan** (Go + npm): the doc-level signal aggregates via "any partial → partial." Reason field carries Go-specific classes. Other ecosystems gain their own reason classes as their resolvers land (#104 follow-ups).
- **Orphan that is the workspace's main module itself**: shouldn't happen because the main-module entry is constructed separately from go.sum entries. If somehow an SBOM has main-module as an orphan, the orphan-reason annotation is absent (the per-component classifier only runs on `pkg:golang/` non-main components).
- **Component with multiple eligible reason classes** (e.g., proxy-fetched-failed AND `// indirect`-only): take the more specific reason — `proxy-fetch-failed` wins over the default `unresolved-indirect-require`.
- **Orphan whose path is in `$GOPRIVATE` AND `$GOPROXY=off`**: reason = `private-module` (the GOPRIVATE match is more specific than the global GOPROXY=off; consumer can act on the GOPRIVATE config).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Mikebom MUST classify orphan Go components at end of `legacy::read()` and populate `extra_annotations["mikebom:orphan-reason"]` on each. Open-enum string values: `unresolved-indirect-require` (default), `private-module` (when path matches `\$GOPRIVATE`), `proxy-fetch-failed` (when the milestone 055 resolver's `LadderSummary.fetch_errors` map recorded a failure for this specific module).

- **FR-002**: Non-orphan components (≥ 1 incoming `dependsOn`) MUST NOT carry the `mikebom:orphan-reason` annotation. Three-state semantics: absent = not orphan.

- **FR-003**: `ScanDiagnostics` MUST gain two new fields: `go_graph_completeness: Option<GraphCompleteness>` (enum: `Complete` / `Partial`) and `go_graph_completeness_reason: Option<String>` (free-text, comma-separated reason-class list). Populated from `legacy::read()` based on the orphan classification.

- **FR-004**: `GraphCompleteness::Complete` ⇔ zero orphans across all `pkg:golang/...` components emitted from `go.sum`. `Partial` ⇔ ≥ 1 orphan. `None` ⇔ no Go scan happened (signal not applicable).

- **FR-005**: `cyclonedx/metadata.rs::build_metadata` MUST accept a new param carrying the completeness signal and emit `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` (when reason is non-empty) into `metadata.properties[]`.

- **FR-006**: `spdx/annotations.rs::build_document_annotations` (SPDX 2.3) MUST emit equivalent document-level annotations.

- **FR-007**: `spdx/v3_annotations.rs::build_document_annotations` (SPDX 3) MUST emit equivalent document-level annotations.

- **FR-008**: Catalog rows C44 (document-level `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`) and C45 (per-component `mikebom:orphan-reason`) MUST be added to `docs/reference/sbom-format-mapping.md` with the Principle V native-field audit recorded.

- **FR-009**: Parity-extractor framework MUST gain rows C44 + C45 with `SymmetricEqual` directionality. Each format-side extractor pulls from the appropriate native channel (CDX `metadata.properties[]` for C44; component `properties[]` for C45; SPDX equivalents).

- **FR-010**: Existing 27 byte-identity goldens MUST be regenerated to absorb the new annotations on Go-touching fixtures. Non-Go fixtures stay byte-identical.

- **FR-011**: Pre-PR gate MUST pass.

### Key Entities

- **`GraphCompleteness`** enum (in `mikebom-cli/src/scan_fs/package_db/mod.rs` or new types module): two variants `Complete` and `Partial`. Serializes as kebab-case via the existing serde convention (`complete` / `partial`).
- **`mikebom:graph-completeness`** annotation: document-level open-enum string. Catalog row C44.
- **`mikebom:graph-completeness-reason`** annotation: document-level free-text comma-separated list of `<ecosystem>:<reason-class>` tokens. Catalog row C44 (same row as the completeness signal).
- **`mikebom:orphan-reason`** annotation: per-component open-enum string. Catalog row C45.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of `tests/fixtures/go/simple-module/` with `--offline` produces a CDX document whose `metadata.properties[]` contains both `mikebom:graph-completeness: partial` AND `mikebom:graph-completeness-reason: go:unresolved-indirect-require` (or the appropriate reason given the cache state). Asserted via golden regen.
- **SC-002**: Each orphan component in the same fixture's regenerated golden has a `mikebom:orphan-reason` property; non-orphan components do NOT.
- **SC-003**: Cross-format parity: the document-level annotation appears identically in CDX `metadata.properties[]`, SPDX 2.3 document `annotations[]`, and SPDX 3 document `annotations[]` per the C44 SymmetricEqual extractor's assertion.
- **SC-004**: Cross-format parity: the per-component annotation appears identically across formats per C45.
- **SC-005**: Pre-PR gate passes.
- **SC-006**: Non-Go fixture goldens (cargo, npm, maven, pip, gem, dpkg, apk, rpm) stay byte-identical.

## Assumptions

- **The milestone 055 `LadderSummary.fetch_errors` map carries enough info to classify orphan reasons**: verified per `golang/graph_resolver.rs` — the resolver populates `fetch_errors[error_class]` per failed proxy fetch. The classifier needs to map these back to specific orphan modules; this requires a small extension of the resolver to track `(failed_module → error_class)` (not just aggregate counts). ~10 LOC addition.
- **Orphan reasons are mutually exclusive per orphan**: when a module both matches `GOPRIVATE` AND failed proxy-fetch, take the more specific reason. Documented in spec Edge Cases.
- **No new crate.** All work is in mikebom-cli + the existing format emitters.
- **Out of scope**: per-ecosystem extension (npm / cargo / maven / pip / gem) — those don't have main-module / resolver pathways yet (#104 follow-ups). Doc-level signal aggregates them as `complete` (no orphans) until they gain their own.
- **Out of scope**: trace-mode SBOM completeness (separate concern; trace mode doesn't go through `legacy::read()`).
