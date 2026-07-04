# Research: Milestone 158 (graph-completeness annotations)

**Date**: 2026-07-03
**Feature**: [spec.md](./spec.md)
**Plan**: [plan.md](./plan.md)

Phase-0 outline of unknowns + design decisions. No NEEDS-CLARIFICATION markers survived the /speckit-clarify session; this research pins down the technical details.

## R1 — Where does `RootSelectionResult.losers` live at emission time?

**Decision**: The existing `RootSelectionResult` returned by `crate::generate::root_selector::select_root(...)` is called at `mikebom-cli/src/cli/scan_cmd.rs:2067` (CDX path) and `mikebom-cli/src/generate/spdx/v3_document.rs:870` (SPDX 3 path). The `losers: Vec<Purl>` field is populated in exactly the ladder branches 4–7 (per milestone 127 root_selector.rs:150–158). SPDX 2.3 also calls `select_root` — need to verify at plan-time by grep; if not called there, we pass the `RootSelectionResult` through into all three format emitters via `build_metadata`'s existing pass-through convention.

**Rationale**: No plumbing needed — the losers are already computed and available. Milestone 158 only needs to:

1. In each format's dependency emitter, when writing the root component's `dependsOn`, union in every `Purl` from `selection.losers`.
2. Pass the `RootSelectionResult` + assembled edges into the new `graph_completeness::compute(...)` function for the BFS pass.

**Alternatives considered**:

- **A. Recompute losers in each format emitter**: rejected — duplicates logic already in `select_root`, invites drift.
- **B. Emit an internal `WorkspacePeerLinkage` struct alongside `RootSelectionResult`**: rejected — adds a parallel data structure for zero benefit; `losers` IS the linkage list.

## R2 — What API do the three format emitters use for document-scope properties/annotations?

**Decision**: Follow the milestone-127 pattern exactly.

- **CDX 1.6** (`mikebom-cli/src/generate/cyclonedx/metadata.rs:436`): push a `serde_json::json!({...})` object onto the mutable `properties: &mut Vec<serde_json::Value>` in `build_metadata`. Property shape: `{"name": "mikebom:graph-completeness", "value": "<string>"}` (no envelope for the value — this is a plain 3-value string; envelope-wrapping would confuse consumers vs. the `mikebom:root-selection-heuristic` case which wraps because it carries structured object). For the reason string, follow the same plain-string convention.
- **SPDX 2.3** (`mikebom-cli/src/generate/spdx/annotations.rs:512`): push a document-scope `Annotation` object with `annotator = "Tool: mikebom-<version>"`, `annotationType = "OTHER"`, `comment = "<name>=<value>"`.
- **SPDX 3.0.1** (`mikebom-cli/src/generate/spdx/v3_annotations.rs:474`): push an `Annotation` element into the graph with `type = "Annotation"`, `subject = "SPDXRef-DOCUMENT"`, `statement = "<name>=<value>"`. The milestone-078 spdx3-validate CI gate is respected because both annotations pass through the JPEWdev `spdx3-validate==0.0.5` conformance check.

**Rationale**: The three format-emission points are already wired to produce document-scope annotations for the existing `mikebom:root-selection-heuristic` (milestone 127) and `mikebom:collisions-summary` (milestone 134). Milestone 158 adds two new entries following the identical pattern — no new infrastructure.

**Alternatives considered**:

- **A. Envelope-wrap the value in an `mikebom-annotation/v1` JSON object** (like `mikebom:root-selection-heuristic` at `metadata.rs:438`): rejected — the value is a plain 3-value string; envelope-wrapping would need a schema for a trivial payload. The reason string is a single semi-colon-joined line — also fine as plain string.
- **B. Attach to `metadata.component.properties[]` instead of `metadata.properties[]`**: rejected — this is a DOCUMENT-scope property (about the emitted SBOM's shape), not a COMPONENT-scope property. All three emitters have distinct "document-scope" and "component-scope" wiring; use the document-scope one.

## R3 — How does milestone 127 handle the same-annotation-across-3-formats parity?

**Decision**: Milestone 127 defined the C69 parity-catalog row at `mikebom-cli/src/parity/extractors/mod.rs:375`, with per-format extractors at `cdx.rs:738`, `spdx2.rs:513`, `spdx3.rs:581`. The extractor macros (`cdx_anno!`, `spdx23_anno!`, `spdx3_anno!`) take the annotation name + scope (`document`) and register the correct format-native lookup path.

Milestone 158 adds two new catalog rows following the identical macro invocations:

```rust
// mikebom-cli/src/parity/extractors/mod.rs (new entries following the C69 pattern at :375)
ParityExtractor {
    row_id: "C70",
    label: "mikebom:graph-completeness",
    cdx: c70_cdx, spdx23: c70_spdx23, spdx3: c70_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
ParityExtractor {
    row_id: "C71",
    label: "mikebom:graph-completeness-reason",
    cdx: c71_cdx, spdx23: c71_spdx23, spdx3: c71_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
```

Plus 6 per-format extractor macro invocations (2 annotations × 3 formats).

**Rationale**: The parity catalog is the milestone-071 invariant that enforces "if you emit an annotation in CDX, it MUST also be present in SPDX 2.3 and SPDX 3 with the same value." Not registering the new annotations there would silently fail this milestone-071 gate. The mechanical shape is well-established.

**Alternatives considered**:

- **A. Skip parity catalog registration on the grounds that the annotation is optional**: rejected — Directionality::SymmetricEqual is the correct semantic (annotation MUST be present in all three formats OR absent from all three; per FR-003 it's always present).
- **B. Set Directionality::OneWay because SPDX 2.3 has weaker annotation semantics**: rejected — milestone 127 already proved SymmetricEqual works for a document-scope string annotation; no reason to weaken here.

## R4 — What's the wire format for the reason-code string?

**Decision**: Plain string with grammar `<code>: <detail>[; <code>: <detail>]*` where:

- `<code>` is a lowercase kebab-case token from the fixed 8-code vocabulary (SC-005).
- `<detail>` is a human-readable string with optional numeric counts (`orphaned-components-detected: 3`).
- Multiple codes are joined by `; ` (semicolon + space) per FR-012.

**Grammar (BNF)**:

```text
reason_string    ::= reason_entry ("; " reason_entry)*
reason_entry     ::= code ": " detail
code             ::= "workspace-peer-detection-degraded"
                   | "root-selection-ambiguous"
                   | "root-selection-failed"
                   | "edge-resolution-degraded"
                   | "go-transitive-coverage-degraded"
                   | "go-workspace-mode-anomaly"
                   | "orphaned-components-detected"
                   | "multi-ecosystem-partial-root"
detail           ::= <human-readable string, may contain digits, may contain colons but NOT `;` or `; `>
```

**Rationale**: Plain string is grep-friendly + jq-friendly + trivial to parse in downstream consumer tools. A structured JSON object would add complexity for a value most consumers will just read as a status message. The `; ` join is unambiguous because `;` is illegal inside `<detail>`.

**Alternatives considered**:

- **A. JSON array of {code, detail} objects**: rejected — added structural complexity. Consumers who want programmatic dispatch can `split('; ')` + `split_once(':')`.
- **B. Repeat the annotation N times (once per code)** in `metadata.properties[]`: rejected — CDX allows duplicate property names but consumers wouldn't know to expect them; single string with grammar is cleaner.
- **C. Colon-separated top-level then comma-separated within, e.g. `codes: a; b; c` + `details: x; y; z`**: rejected — decouples codes from details, invites index-mismatch bugs.

## R5 — How does BFS represent "multi-root" reachability?

**Decision**: The BFS pass starts by identifying a **seed set** of ecosystem roots:

1. Query all `ResolvedComponent`s where `extra_annotations["mikebom:component-role"] == "main-module"`.
2. Group by `purl.ecosystem()` (e.g., `npm`, `gem`, `pypi`, `cargo`, `go`, `maven`).
3. For each ecosystem, pick the "top" main-module using the existing `select_root`-style ladder:
   a. Prefer main-modules where `mikebom:is-workspace-root == true`.
   b. Fall back to longest-common-prefix of manifest paths (matches milestone 127 heuristic).
4. The primary root (from `RootSelectionResult.subject`) is guaranteed to be in the seed set; other ecosystems' roots ADD to it.

BFS then walks `dependsOn` from ALL seeds simultaneously (single visited-set, multiple starting nodes). Reachability = the visited set at completion.

**Pseudocode**:

```rust
fn compute_reachability(
    components: &[ResolvedComponent],
    dependency_edges: &HashMap<PurlKey, Vec<PurlKey>>,
    selection: &RootSelectionResult,
) -> HashSet<PurlKey> {
    let mut seeds: HashSet<PurlKey> = HashSet::new();

    // Primary root (already picked by select_root)
    if let Some(primary) = selection.subject.as_purl_key(components) {
        seeds.insert(primary);
    }

    // Additional per-ecosystem roots
    let main_modules: Vec<_> = components.iter()
        .filter(|c| c.extra_annotations.get("mikebom:component-role")
            .and_then(|v| v.as_str()) == Some("main-module"))
        .collect();
    let mut by_ecosystem: HashMap<&str, Vec<&ResolvedComponent>> = HashMap::new();
    for c in &main_modules {
        by_ecosystem.entry(c.purl.ecosystem()).or_default().push(c);
    }
    for (_eco, mods) in by_ecosystem {
        // Pick "top" per-ecosystem main-module (workspace-root first, else LCP)
        if let Some(top) = pick_ecosystem_top(&mods) {
            seeds.insert(purl_key(top));
        }
    }

    // Multi-source BFS
    let mut visited: HashSet<PurlKey> = seeds.clone();
    let mut queue: VecDeque<PurlKey> = seeds.into_iter().collect();
    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = dependency_edges.get(&node) {
            for neighbor in neighbors {
                if visited.insert(*neighbor) {
                    queue.push_back(*neighbor);
                }
            }
        }
    }
    visited
}
```

**Rationale**: Multi-source BFS is a well-known algorithm (Dijkstra + Prim + etc. all use it). The seed-set derivation reuses the existing `mikebom:component-role = "main-module"` marker + purl-ecosystem grouping. `pick_ecosystem_top` reuses the milestone-127 heuristic in a per-ecosystem scope. Reachability is the visited set — components NOT in the set are the orphans.

**Alternatives considered**:

- **A. BFS from `metadata.component` only**: rejected — this is exactly the bug the milestone fixes (single-root undercount). Q3 clarified we want union across ecosystem roots.
- **B. Synthesize a virtual `pkg:generic/scan-root` component with `dependsOn = [each ecosystem's root]` and BFS from that**: rejected — introduces a synthetic component in the emitted SBOM, breaks byte-identity for single-ecosystem cases, and doesn't match how consumers actually render the graph.
- **C. Union-find / connected-components**: rejected — pnpm/npm-style ecosystems have implicit "you don't leave your ecosystem" boundaries; union-find would incorrectly merge unrelated ecosystems into one component.

## R6 — Should milestone 158 emit `mikebom:graph-completeness = complete` on repos with `heuristic == None` + no losers (single-component repos)?

**Decision**: Yes, unconditionally. Per FR-003 the annotation is emitted on EVERY SBOM regardless of shape. The value defaults to `complete` when the BFS pass runs to completion with 100% reachability and no orphan/multi-eco reason codes. Milestone-090's single-package fixtures (alpine, apk, cargo, cyclonedx-source, deb, gem, maven, npm, pip, rpm, spdx-source) all hit this default — SC-002 explicitly requires ONE property addition + zero other bytes changed.

**Rationale**: Universal presence is the whole design (SC-003). A CI gate that says "assert `.mikebom:graph-completeness == complete`" needs the annotation to be present-or-absent in a predictable way; making it optional invites schema-drift bugs.

**Alternatives considered**:

- **A. Skip emission when `complete` (implicit-default)**: rejected — consumers can't distinguish "mikebom said complete" from "mikebom is old and doesn't emit this annotation." Universal presence solves this.
- **B. Emit only when non-complete**: same rejection as A.

## R7 — Performance profile of BFS on 2835 components (test-podman-desktop)

**Decision**: The BFS pass is O(V+E). For test-podman-desktop: V=2835, E≈6184 → ~9000 operations, each a HashMap lookup + HashSet insert. On modern hardware this is <5ms (measured via `criterion` on a comparable graph would confirm at plan-time via T003 in tasks).

The plan target: `<20ms on the 2835-component test-podman-desktop testbed`; the FR-008 constraint: `MUST NOT add >100ms to scan time for repos with ≤10,000 components`. Both are comfortable — a 10,000-component/25,000-edge graph is ~35,000 operations, still well under 100ms.

**Rationale**: The perf budget is not a real constraint. Milestone 090's `test-fixture` split showed scan wall-time is dominated by lockfile parsing + PURL construction, not by post-processing passes.

**Alternatives considered**:

- **A. Skip BFS on repos with >10,000 components + emit `unknown`**: rejected — the perf math shows no such threshold is needed. Caution-first FR-006 already handles genuine "can't compute" scenarios.
- **B. Persist BFS results across scans in a cache**: rejected — matches the "no state persistence" project rule (spec assumption 6).

## R8 — Observability signal (deferred from clarify)

**Decision** (plan-time resolution of a deferred clarify item): Emit an info-level tracing log line at scan-emission time, following the milestone-157 FR-007 pattern:

```rust
tracing::info!(
    value = %result.value,           // "complete" | "partial" | "unknown"
    reachable_count = %result.reachable_count,
    total_count = %result.total_count,
    orphan_count = %result.orphan_count,
    reason_codes = ?result.reason_codes,
    "graph completeness computed"
);
```

This log line is grep-friendly for CI-log analysis and doesn't affect the SBOM wire format. It becomes **FR-013** in the spec (added during plan/task).

**Rationale**: Milestone 157 established this pattern (FR-007 `pnpm-lock parsed` log). Consumers reading CI logs can grep for `"graph completeness computed"` and see per-scan diagnostics without parsing the SBOM. Constitution Principle X extends beyond in-SBOM metadata.

**Alternatives considered**:

- **A. Skip logging (annotation-only)**: rejected — misses the milestone-157 precedent for CI-log observability.
- **B. Emit at debug level instead of info**: rejected — debug is too quiet for CI-log grep; info matches the milestone-157 convention.

## R9 — CHANGELOG entry shape

**Decision**: Follow the milestone-157 CHANGELOG-entry template exactly (unreleased section, headline, background paragraph, Q&A summaries, empirical numbers, consumer jq recipe, wire-format cleanliness note). The consumer jq recipe:

```bash
# Gate a CI pipeline on graph completeness
completeness=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value' sbom.cdx.json)
case "$completeness" in
    complete) echo "Graph is fully connected — safe to consume" ;;
    partial)  reason=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' sbom.cdx.json)
              echo "Partial graph: $reason" ;;
    unknown)  reason=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' sbom.cdx.json)
              echo "Unknown completeness: $reason (recommend re-scan or manual review)"; exit 1 ;;
esac
```

**Rationale**: Milestone 157's CHANGELOG entry was well-received in the PR; reuse the shape.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data model + contracts).
