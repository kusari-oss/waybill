# Contract: Graph-Completeness Classifier Input (m192)

**Date**: 2026-07-14
**Scope**: The behavioral contract for `build_ecosystem_root_set` and the downstream `MultiEcosystemPartialRoot` classifier after m192. Used by unit tests to lock in the fix semantics.

## Fixture reference

Three fixture shapes cover the interesting cases:

**Fixture O1 (operator-override, single ecosystem)**:
- `components[]` = `[pkg:golang/foo/bar@v1.0.0, pkg:golang/foo/baz@v2.0.0]`
- `target_ref` = `"pkg:generic/pico@abc123"` (operator supplied `--root-name pico --root-version abc123`)
- `RootSelectionResult.subject` = `OperatorOverride` (or `SyntheticPlaceholder`)

**Fixture O2 (operator-override, mixed ecosystems)**:
- `components[]` = `[pkg:golang/foo@v1, pkg:npm/bar@1.0, pkg:pypi/baz@1.0]`
- `target_ref` = `"pkg:generic/mixed@1.0"`
- `RootSelectionResult.subject` = `OperatorOverride`

**Fixture O3 (operator-override + --root-purl-type golang)**:
- `components[]` = `[pkg:golang/svc@v1, pkg:npm/dep@1.0]`
- `target_ref` = `"pkg:golang/github.com/example/svc@1.0"` (operator picked golang as root type)
- `RootSelectionResult.subject` = `OperatorOverride`

**Fixture N (native-root — must be byte-identical)**:
- `components[]` includes one entry with `mikebom:component-role: main-module` (e.g., a Go module root)
- `target_ref` = that main-module's PURL
- `RootSelectionResult.subject` = `MainModule(idx)` where `idx` points at the main-module component

## Contract for `build_ecosystem_root_set`

Given each fixture, `build_ecosystem_root_set(components, selection, target_ref)` MUST produce:

### Fixture O1 output

```
EcosystemRootSet {
    roots: {"pkg:generic/pico@abc123"},
    ecosystems_without_root: [],
}
```

- `roots` contains exactly one entry: the operator-supplied `target_ref`.
- `ecosystems_without_root` is EMPTY — the `golang` ecosystem is covered by the synthesized placeholder `(golang, "pkg:generic/pico@abc123")`.

### Fixture O2 output

```
EcosystemRootSet {
    roots: {"pkg:generic/mixed@1.0"},
    ecosystems_without_root: [],
}
```

- Synthesis fires for `golang`, `npm`, AND `pypi` — three placeholder entries all pointing at `target_ref`.
- `ecosystems_without_root = []`.
- INFO log fires with `synthesized_ecosystems_count = 3`.

### Fixture O3 output

```
EcosystemRootSet {
    roots: {"pkg:golang/github.com/example/svc@1.0"},
    ecosystems_without_root: [],
}
```

- `target_ref` parses as `pkg:golang/...` → `operator_root_ecosystem = Some("golang")`.
- Synthesis SKIPS `golang` (already covered by operator's PURL type per Q2 answer A).
- Synthesis fires for `npm` only — one placeholder entry `(npm, "pkg:golang/github.com/example/svc@1.0")`.
- INFO log fires with `synthesized_ecosystems_count = 1`.

### Fixture N output (native-root, byte-identity guard)

```
EcosystemRootSet {
    roots: {<main-module PURL>},
    ecosystems_without_root: [<any ecosystem present in components[] but without a main-module entry>],
}
```

- The synthesis block does NOT execute (`is_native_root == true`).
- Output is BYTE-IDENTICAL to pre-m192 for the same inputs.
- NO INFO log line emitted.

## Downstream classifier consequence

Given each fixture, `compute_graph_completeness` MUST produce:

### Fixture O1 → `GraphCompletenessValue::Complete`
Assuming BFS reaches every component via the primary-dep-fallback edges from `target_ref` to `graph_tops` (mod.rs:186-199), `orphan_count == 0` AND `reason_codes.is_empty()` → `Complete`.

### Fixture O2 → `GraphCompletenessValue::Complete`
Same rationale as O1. Multi-ecosystem doesn't matter post-synthesis.

### Fixture O3 → `GraphCompletenessValue::Complete`
Same rationale. The `--root-purl-type golang` case is handled without emitting a duplicate golang root.

### Fixture N → identical to pre-m192 classifier value
No behavior change on the native-root path.

## Contract for real-gap detection (FR-007)

**Given** Fixture O1 modified to include a synthetic orphan component `pkg:golang/isolated@1.0.0` with NO incoming edge AND NO outgoing edge in the assembled Relationships:

**Then** the classifier MUST produce `GraphCompletenessValue::Partial` with `reason_codes` containing `OrphanedComponentsDetected { orphan_count: 1 }` — the fix does NOT suppress real orphan detection. The `MultiEcosystemPartialRoot` classifier still doesn't fire (its precondition `ecosystems_without_root` is still empty), but the `OrphanedComponentsDetected` classifier does — that's the correct signal for a real gap.

## Cross-format consistency (FR-006)

The `GraphCompletenessResult` is computed ONCE at emit time and threaded into all three format emitters (CDX, SPDX 2.3, SPDX 3). Post-m192, all three formats reflect the corrected `mikebom:graph-completeness` value identically for the same input. No per-format shape change.
