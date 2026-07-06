# Research: milestone 166 — SPDX 3 annotation dedup fix

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Phase 0 research. Empirically-grounded from milestone-165 audit; documents the design decisions rather than exploring unknowns.

## R1 — Root-cause pinpoint

**Decision**: The bug lives at `mikebom-cli/src/generate/spdx/v3_document.rs:754-820` where 4 annotation builders are merged into `@graph[]`:

```rust
let mut annotations: Vec<Value> = Vec::new();
annotations.extend(super::v3_annotations::build_component_annotations(...));
annotations.extend(super::v3_annotations::build_document_annotations(...));
annotations.extend(super::v3_annotations::build_supplement_service_annotations(...));
// + optional user-supplied --metadata-comment + --annotator loops
annotations.sort_by(|a, b| {
    let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
    key(a).cmp(&key(b))
});
for anno in annotations {
    graph.push(anno);
}
```

**The bug**: no dedup step between `sort_by` and `graph.push`. When two builder call paths derive the same `hash(subject_iri | field, 16)` (per `v3_annotations.rs:174-176`), both land in `@graph[]`.

**Rationale**: The spdxId derivation scheme is byte-identity-safe (m017 T013b) and can't be changed without regression risk. Dedup at the merge point is the smallest surgical fix.

## R2 — Dedup posture: LAST-writer-wins vs FIRST-writer-wins

**Decision**: **LAST-writer-wins.**

Concrete rationale:

1. **Rust idiom**: `BTreeMap::insert(k, v)` returns `Option<V>` — `Some(old_value)` on replace, `None` on insert. Natural pattern: `if let Some(dropped) = map.insert(...) { counter += 1; }`. FIRST-wins would require `.entry(...).or_insert(...)` — same effect, slightly less natural for counting drops.

2. **Determinism guarantee**: builder invocation order at `v3_document.rs:754-820` is deterministic. Whichever builder runs LAST for a given `(subject_iri, field)` produces the winning entry. Both LAST and FIRST are byte-identity-safe; LAST just matches the `insert` idiom.

3. **Debuggability**: when duplicates arise (per FR-007 log fires), the WINNER is the last emitter — which is easier to trace in the fixed builder order (`build_component_annotations` → `build_document_annotations` → `build_supplement_service_annotations` → user annotations). If a component-emitter is later found to be redundantly firing document-scope annotations, the user annotations (or supplement annotations) still win — a plausible expected priority.

4. **Empirical evidence**: on the observed bug (`anno-GJJZ6XAC7UZOZO57` on Kubernetes, `anno-YNFF6NBSSKSMJZF2` on ArgoCD), the duplicates contain IDENTICAL content — LAST vs FIRST is irrelevant. Choosing LAST for the natural-Rust-idiom reason.

**Alternatives considered**:

- **A. FIRST-writer-wins**: rejected — no natural advantage; slightly awkward `.entry(...).or_insert(...)` pattern for counting drops.
- **B. LAST-writer-wins (chosen)**: minimal code, natural Rust idiom, byte-identity-safe.
- **C. Merge by content (union statements)**: rejected — SPDX 3 spec doesn't support multi-value statements on an Annotation; would just re-create the same validation failure with different symptoms.
- **D. Panic on duplicate**: rejected — would cause a scan failure on the observed real-world scenario (Kubernetes source scan). Violates Constitution Principle XI (every scan produces an SBOM).

## R3 — BTreeMap vs HashMap for the dedup

**Decision**: `BTreeMap<String, Value>`.

**Rationale**:

- **Deterministic iteration order**: `BTreeMap` iterates in key-sorted order. Since the existing `v3_document.rs:814-817` sort step sorts by `spdxId` post-merge, using `BTreeMap` means we can eliminate the explicit `sort_by` call — the BTreeMap's natural iteration is already sorted lexicographically by `spdxId` string. Small perf + code simplification win.

- **Byte-identity guarantee**: `BTreeMap` iteration is stable across Rust versions (unlike `HashMap` which uses randomized hashing by default). Matches the milestone-017 T013b cross-host byte-identity requirement.

- **Performance**: on 5000-annotation scans, `BTreeMap` insert is O(log N) vs `HashMap`'s O(1). For 5000 entries the difference is ~15 comparisons vs 1 hash — negligible in absolute terms.

**Alternatives considered**:

- **A. HashMap**: rejected — requires an explicit sort step afterward. No byte-identity guarantee across `std` versions.
- **B. BTreeMap (chosen)**: sorted iteration is a natural byproduct; byte-identity-safe.
- **C. `Vec::dedup_by_key`**: rejected — would require pre-sorting; error-prone if the sort key isn't exactly `spdxId`. BTreeMap is cleaner.

## R4 — FR-007 tracing log field naming

**Decision**: Extend the existing per-scan info log with a new field `spdx3_annotation_duplicates_dropped=<N>`. Fire the log line UNCONDITIONALLY (even when N == 0) so consumers doing regex parsing for the field can always find it — matches the milestone-158-onwards `packages_count`, `phantom_prevented_count`, etc. convention.

**Rationale**:

- Matches the milestone-157/158/159/160/161/162/163/164/165 observability pattern: single summary log line per scan/emission with all counters.
- Zero-baseline case (N == 0) is expected on healthy scans — surfacing the zero explicitly makes CI-log analysis easier (grep the field, see it's present at 0).
- Non-zero N signals a redundant emitter — surfaces the code path for future investigation (milestone 167+).

**Alternatives considered**:

- **A. Only log when N > 0**: rejected — makes it harder to prove "no duplicates on this scan" in CI logs.
- **B. Log at DEBUG level when N == 0, INFO when N > 0**: rejected — inconsistent log level based on state is confusing.
- **C. Log every dropped duplicate at WARN**: rejected — could be noisy on future scenarios (if a redundant emitter fires 100 times, we'd emit 100 WARN lines).

**Log placement**: extend the existing SPDX 3 emission info log (if there is one) OR add a new one at the end of `build_v3_document` in `v3_document.rs`. Verified at implementation time.

## R5 — Interaction with milestone-078 SPDX 3 conformance test

**Decision**: `mikebom-cli/tests/spdx3_conformance.rs` (milestone 078) MUST continue to pass. Milestone 166 does NOT change the conformance gate — it just fixes an emission bug that was causing the gate to fail on real upstream targets (K8s, ArgoCD) but not on milestone-090 fixtures.

**Empirical verification**: run `cargo test --test spdx3_conformance` pre-166 and post-166. Both MUST return the same PASS. If pre-166 already passes on milestone-090 fixtures, that means the fixtures don't currently produce duplicates — the bug is exercised only by real upstream targets (K8s, ArgoCD) OR the SC-009 integration test's synthesized fixture.

**Alternative rationale**: if pre-166 conformance test FAILS on some milestone-090 fixture (contradicting the audit's finding that fixtures pass), that's a critical anomaly — need to investigate why the m165 audit found duplicates on K8s+ArgoCD but not on fixtures. Expected outcome: pre-166 conformance test passes on fixtures; SC-009 synthesized fixture exercises the bug.

## R6 — SC-009 integration test approach: synthesized fixture

**Decision**: Synthesize a scan that INTENTIONALLY produces duplicate Annotation spdxIds by exercising milestone 158's graph-completeness annotation on a subject that ALSO receives another annotation via a different builder path.

**Concrete approach**: Test invokes `parse_pnpm_lock` or an analogous m158-triggering path that emits a `mikebom:graph-completeness` annotation at document scope. Then verify: (a) the emitted `@graph[]` has no duplicate spdxIds; (b) the FR-007 log fires; (c) the retained Annotation matches LAST-writer per FR-004.

**Alternative rationale**: The exact code path that produces the observed duplicates on K8s+ArgoCD but NOT on milestone-090 fixtures is unknown pre-Phase-3. The integration test may need to invoke the release binary against a synthesized fixture that mimics K8s' shape (Go monorepo with staging repos) — or, if the bug is easier to reproduce at unit level, exercise the merge point directly with two builders emitting same-hash annotations.

## R7 — Root-cause emitter investigation: DEFERRED

**Decision**: Milestone 166 fixes the SYMPTOM (dedup at merge). Root-cause investigation of WHICH builder is emitting the redundant annotation is DEFERRED to a follow-on milestone (167+) IF FR-007's log surfaces significant volume post-166.

**Rationale**: A defense-in-depth architecture — dedup at merge is a general safeguard against any current or future redundant emitter. Investigating the specific emitter that causes the observed K8s+ArgoCD duplicates would require reproducing on those specific SBOMs at fixed commit SHAs — orthogonal work.

**Milestone 167+ trigger**: if post-166 audit round shows `spdx3_annotation_duplicates_dropped > 10` on any real target, promote root-cause investigation to a follow-on milestone.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data-model.md + contracts + quickstart).
