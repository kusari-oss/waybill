# Research — milestone 084 CDX 1.6 main-module super-root collapse

Five implementation-level decisions to pin before Phase 1 design + the override-path investigation that informs FR-005's scope.

## §1 — `target_ref` derivation: where to compute the conditional

**Decision**: Add a small in-line conditional at `mikebom-cli/src/generate/cyclonedx/builder.rs:297-300` that picks the main-module PURL when one is present in `effective_components` and no override is active; otherwise falls back to the existing `format!("{}@{}", effective_target_name, effective_target_version)`. The conditional uses the same `mikebom:component-role: main-module` annotation marker that the milestone-077 override-filter at `builder.rs:272-293` already keys off — so detection logic is consistent between the two sites.

**Implementation shape** (target ~10 LOC):

```rust
// Find the main-module component (if any) — same predicate as the
// override-filter at line 272-293. Only used to align target_ref
// with metadata.component.bom-ref when milestone 053's main-module
// promotion is in effect.
let main_module_purl: Option<&str> = if !override_active {
    effective_components
        .iter()
        .find(|c| {
            c.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
        })
        .map(|c| c.purl.as_str())
} else {
    None
};

let target_ref: String = match main_module_purl {
    Some(purl) => purl.to_string(),
    None => format!("{}@{}", effective_target_name, effective_target_version),
};
```

**Rationale**: Inline conditional keeps the change auditable in a single file. Detection by `mikebom:component-role` annotation reuses the existing convention from milestone 053 (introduced by `metadata.rs:425-430` and consumed by `builder.rs:272-293`). Avoids replicating the `synthetic_component_purl` derivation from `metadata.rs` (which depends on milestone-077 override interaction); instead, reads the PURL directly from the `ResolvedComponent` whose annotation marks it as the main-module — single source of truth, no derivation drift risk.

**Alternatives considered**:

- *Factor target_ref derivation into a shared helper called by both `metadata.rs:391-409` and `builder.rs`*. Rejected: the two sites already have different inputs (metadata.rs has `subject_name` + `synthetic_component_purl`; builder.rs has `effective_components` + `effective_target_name`). Aligning the inputs would require restructuring upstream signature, which is out of scope per spec ("renaming or removing the target_ref concept" — Out of Scope §3). The two-site approach is cheaper and avoids cross-cutting plumbing.
- *Pass `metadata.component.bom-ref` value back from `build_metadata` to builder.rs and reuse it as `target_ref`*. Rejected: tighter coupling between metadata builder and downstream consumers; the target_ref is conceptually independent (it's the *project root* in the dep graph, which happens to equal the metadata.component bom-ref by spec contract). Reading the main-module PURL from `effective_components` directly is semantically cleaner.

## §2 — `--root-component-name` override path: latent orphan or already coherent?

**Investigation**: Trace the override path's emitted `dependencies[]` to determine whether the same orphan-ref pattern exists when `override_active`.

**Today's flow in override case** (alpha.23, pre-fix):

1. `metadata.rs:391-409` → `bom-ref = format!("{}@{}", subject_name, subject_version)` (override identity).
2. `builder.rs:297-300` → `target_ref = format!("{}@{}", effective_target_name, effective_target_version)` (also override identity, via the override resolution at lines 246-264).
3. `builder.rs:272-293` filters main-module out of `effective_components`.
4. `relationships[]` from `scan_fs/mod.rs` is **NOT filtered** — still contains edges keyed off the manifest-derived main-module PURL (`from = main_module_PURL`, `to = direct_dep_PURL`).
5. `dependencies.rs:50-66` populates `dep_map` with: (a) every component in `effective_components` (main-module dropped), (b) every relationship's `from` (which still includes main-module-PURL).
6. Result: `dependencies[]` contains an entry `{ ref: <main-module-PURL>, dependsOn: [<direct deps>] }` — but main-module-PURL is NOT in `effective_components` (filtered out at step 3) and NOT == `metadata.component.bom-ref` (which is the override-form `<override-name>@<override-version>` from step 1).
7. **The main-module-PURL is therefore an orphan ref in the override path too** — same shape as the main-module path's orphan, just with the orphan and the legitimate root swapped.

**Confirmation pending**: this needs an empirical test against an existing milestone-077 fixture (or a new fixture) — the milestone-077 test at `mikebom-cli/tests/identifiers_root_component_override.rs` only asserts on `metadata.component.bom-ref` (line 138), not on `dependencies[].ref` closure, so the orphan would not have been caught.

**Decision**: scope FR-005's behavior to address this case. The fix in §1 already covers it for free — when `override_active`, `main_module_purl = None` (the conditional skips it), `target_ref = override-form`. But that doesn't fix the relationships-populated PURL entries; those need to also be filtered. **Two sub-options**:

| Option | Description | Pro | Con |
|---|---|---|---|
| **A** | Filter `relationships[]` in `builder.rs` when `override_active`: drop edges whose `from` matches the dropped main-module PURL. Replace those edges with edges from the override-form ref to the same `to` deps, so the root-to-direct-dep tree is preserved. | Self-contained in builder.rs; preserves dep-tree shape under override. | Requires knowing which PURL was the main-module (do during the override-filter pass, before `target_ref` derivation). |
| **B** | Document the override path as an empirical-test gap to file a follow-up issue against; this milestone scopes only the main-module path. | Smaller PR. Lower regression risk. | Leaves a known orphan in override emissions; FR-005 becomes aspirational rather than enforced. |
| **C** | Don't filter relationships, but also don't drop the main-module from `effective_components` when override is active — instead, replace the main-module's `metadata.component`-level identity with the override identity AND keep the main-module as a regular `components[]` entry. | Fully closes the closure invariant. | Changes milestone-077's clean-replacement semantics (override no longer "drops" the main-module from components[]); needs spec amendment. |

**Recommended**: **Option A**. Captures FR-005's invariant in the same PR; small additional scope (~15 LOC of relationship-filtering at the same site as the existing component-filter); regression-tested by the existing milestone-077 fixture's golden plus the new closure-invariant test from FR-011 (which will fail-fast on an orphan in any path).

**Rationale**: Option B silently ships a known orphan in the override path, contradicting FR-005. Option C changes milestone-077's semantics (a behavior change, not a fix). Option A is the smallest change that satisfies FR-005 + FR-006 across both `main_module.is_some()` and override paths.

**Risk**: The override path's existing golden at `mikebom-cli/tests/identifiers_root_component_override.rs` may show new diffs (one fewer `dependencies[]` entry under override, plus retargeted edges). The diff scope MUST stay within FR-005's invariant — same shape rule as for the main-module path goldens. If the diff is larger, the implementation has overcorrected.

## §3 — Goldens regen scope

**Decision**: enumerate the affected goldens up front so the PR diff is auditable.

**Affected goldens** (post-053 ecosystem fixtures):

| Fixture path | Ecosystem | Currently has orphan? |
|---|---|---|
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for Go fixtures (milestones 053, 102, etc.) | Go | YES (PURL form `pkg:golang/...@...` in metadata.component, short-name orphan in deps[]) |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for cargo (milestone 064) | cargo | YES |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for npm (milestone 066) | npm | YES |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for pip-poetry (milestone 068) | pip-poetry | YES |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for pip-pipfile (milestone 068) | pip-pipfile | YES |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for pip-plain (milestone 068) | pip-plain | LIKELY YES (degenerate but same code path) |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for gem (milestone 069) | gem | YES |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for maven (milestone 070) | maven | YES |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for milestone-077 override fixtures | (override path) | YES per §2 |
| `mikebom-cli/tests/fixtures/cdx_*.golden.json` for OS-package fixtures (dpkg/rpm/apk) | (no main-module) | NO — fallback path, byte-identical pre/post |
| SPDX 2.3 + SPDX 3 goldens (any ecosystem) | n/a | n/a — FR-007 mandates byte-identity pre/post |

**Pre-implementation discovery step** (research-task for the implementation phase):

```bash
# Enumerate every CDX golden that contains a non-PURL ref in dependencies[]:
find mikebom-cli/tests -name "cdx_*.golden.json" \
  | xargs -I{} sh -c '
      if jq -e ".dependencies[].ref | select(test(\"^pkg:\") | not)" "{}" >/dev/null 2>&1; then
        echo "{}"
      fi'
```

Result is the exhaustive regen list. Run via `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace` per CLAUDE.md golden-regen protocol.

**Diff invariant per golden** (audit guide for PR review):

For each affected golden, the post-fix diff MUST consist of EXACTLY:

1. One `dependencies[]` entry deleted (the orphan bridge entry). Identifiable by `ref` matching `<short-name>@0.0.0` shape and a `dependsOn` of length 1 pointing at the main-module PURL.
2. Two `compositions[]` entries with their `assemblies[0]` or `dependencies[0]` rewritten from `<short-name>@0.0.0` to the main-module PURL.

NO other changes. If the diff shows `metadata.component`, `components[]`, or any field besides those two, the implementation has overcorrected.

**Rationale**: explicit per-golden regen scope catches both under-coverage (a golden missed) and over-correction (a field unintentionally changed). The diff invariant is auditable in the PR review without re-running mikebom.

**Alternatives considered**: blanket "regen all CDX goldens" — rejected because it hides over-correction; the per-golden inspection is cheap and bounded (~6-8 files).

## §4 — FR-011 closure-invariant regression test design

**Decision**: single new test file `mikebom-cli/tests/cdx_ref_closure_invariant.rs`. Table-driven across post-053 ecosystem fixtures: each row is `(fixture_path, ecosystem_label)`. For each row, run mikebom with CDX 1.6 emission, parse the JSON, compute the closure set `S = components[].bom-ref ∪ {metadata.component.bom-ref}`, and assert `dependencies[].ref ⊆ S`, `dependencies[].dependsOn[] ⊆ S`, `compositions[].assemblies[] ⊆ S`, `compositions[].dependencies[] ⊆ S`.

**Test shape** (target ~80 LOC):

```rust
const FIXTURES: &[(&str, &str)] = &[
    ("tests/fixtures/.../<go-fixture>",    "go"),
    ("tests/fixtures/.../<cargo-fixture>", "cargo"),
    // ... per ecosystem
];

#[test]
fn cdx_ref_closure_invariant_holds() {
    for (path, eco) in FIXTURES {
        let cdx = run_mikebom_cdx(path);
        assert_closure(&cdx, eco);
    }
}

fn assert_closure(cdx: &serde_json::Value, eco: &str) {
    let mut declared: HashSet<String> = HashSet::new();
    if let Some(meta_ref) = cdx["metadata"]["component"]["bom-ref"].as_str() {
        declared.insert(meta_ref.to_string());
    }
    for c in cdx["components"].as_array().unwrap_or(&vec![]) {
        if let Some(r) = c["bom-ref"].as_str() {
            declared.insert(r.to_string());
        }
    }

    let mut violations: Vec<(String, String)> = vec![];
    // dependencies[].ref + dependencies[].dependsOn[]
    for dep in cdx["dependencies"].as_array().unwrap_or(&vec![]) {
        let r = dep["ref"].as_str().unwrap_or("");
        if !declared.contains(r) {
            violations.push((format!("dependencies[].ref"), r.to_string()));
        }
        for d in dep["dependsOn"].as_array().unwrap_or(&vec![]) {
            let s = d.as_str().unwrap_or("");
            if !declared.contains(s) {
                violations.push((format!("dependencies[].dependsOn[]"), s.to_string()));
            }
        }
    }
    // compositions[].assemblies[] + compositions[].dependencies[]
    for c in cdx["compositions"].as_array().unwrap_or(&vec![]) {
        for a in c["assemblies"].as_array().unwrap_or(&vec![]) {
            let s = a.as_str().unwrap_or("");
            if !declared.contains(s) {
                violations.push((format!("compositions[].assemblies[]"), s.to_string()));
            }
        }
        for d in c["dependencies"].as_array().unwrap_or(&vec![]) {
            let s = d.as_str().unwrap_or("");
            if !declared.contains(s) {
                violations.push((format!("compositions[].dependencies[]"), s.to_string()));
            }
        }
    }

    assert!(violations.is_empty(),
        "ecosystem {eco}: {} closure violations: {:?}",
        violations.len(), violations);
}
```

**Rationale**: table-driven keeps the test count low (one logical test, multiple data points) — fast and easy to extend. String-set closure check is the simplest faithful encoding of the spec's `refLinkType` contract. No external dependencies (`cyclonedx-cli` shell-out) — keeps the test hermetic and Linux/macOS-portable.

**Alternatives considered**:

- *Shell out to `cyclonedx-cli validate`*. Rejected: introduces an external tool dependency on CI runners (currently `cyclonedx-cli` is wired via milestone-013 format-parity gate but only on the parity lane; the closure invariant is cheaper to check in-Rust). Future work can layer cyclonedx-cli as a higher-level conformance check.
- *Per-ecosystem test files*. Rejected: 8+ near-identical files = noise; table-driven captures the same coverage in one file.
- *Property-based testing (proptest)*. Rejected: out of proportion for a closure-invariant check; the predicate is finite and a fixture-driven assertion is sufficient.

## §5 — Test fixture choice for FR-011

**Decision**: reuse existing per-ecosystem fixtures already in `mikebom-cli/tests/fixtures/`. If milestone 083's `transitive_parity` fixtures land first, those are richer (≥50 components, ≥100 edges per fixture per FR-002 of milestone 083) and exercise the closure invariant against larger graphs — preferred. Otherwise reuse the existing per-ecosystem fixtures from milestones 053/064/066/068/069/070.

**Per-ecosystem fixture map** (subject to verification at implementation time):

| Ecosystem | Fixture |
|---|---|
| Go | `mikebom-cli/tests/fixtures/golang_fixture/` (or 083's `transitive_parity/go/` if available) |
| cargo | the milestone-064 cargo fixture (likely `mikebom-cli/tests/fixtures/cargo_fixture/`) |
| npm | the milestone-066 npm fixture |
| pip-poetry | the milestone-068 poetry fixture |
| pip-pipfile | the milestone-068 pipfile fixture |
| pip-plain | the milestone-068 requirements.txt fixture |
| gem | the milestone-069 gem fixture |
| maven | the milestone-070 maven fixture |

**Rationale**: reuse maximizes coverage drift signal — if a future milestone changes a per-ecosystem reader and silently re-introduces a non-PURL ref, the closure-invariant test running against the same fixture catches it. New fixtures would not have this property.

**Alternatives considered**:

- *Synthetic minimal fixture per ecosystem*. Rejected: adds maintenance surface; doesn't drift-detect future regressions in the real code paths.
- *Just one fixture (Go) + assume other ecosystems are byte-identical*. Rejected: the bug is per-ecosystem-reader-derived (each main-module-promotion milestone is separate); per-ecosystem coverage is the load-bearing test surface.

## §6 — Pre-PR gate verification protocol

**Decision**: standard CLAUDE.md pre-PR sequence applies:

```bash
./scripts/pre-pr.sh
```

Which runs:

1. `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero errors AND zero warnings).
2. `cargo +stable test --workspace` (every suite `0 failed`).

Plus the milestone-specific verification:

3. `find mikebom-cli/tests -name "cdx_*.golden.json" | xargs -I{} jq -e '[.dependencies[].ref] | all(. as $r | $r | startswith("pkg:") or test("^[a-zA-Z][a-zA-Z0-9_-]*@\\d+\\.\\d+\\.\\d+$"))' "{}"` — sanity check that no obviously-malformed refs exist post-regen.

4. New closure-invariant test pre-run: `cargo +stable test -p mikebom --test cdx_ref_closure_invariant -- --nocapture`.

5. Diff-shape audit per §3: each affected golden's diff matches the "one fewer dep entry + two retargeted compositions" invariant.

**Rationale**: standard project workflow + milestone-specific golden audit. The closure-invariant test is the load-bearing assertion; the diff-shape audit catches over-correction.

## §7 — SPDX 2.3 / SPDX 3 byte-identity verification

**Decision**: explicit pre/post merge check that SPDX 2.3 + SPDX 3 goldens are byte-identical (FR-007 / SC-005). Run:

```bash
# Save pre-merge SPDX outputs
git stash  # if any uncommitted work
cargo +stable test -p mikebom --test spdx_regression -- --nocapture
cargo +stable test -p mikebom --test spdx3_regression -- --nocapture

# Apply the fix, re-run:
# (after implementation)
cargo +stable test -p mikebom --test spdx_regression -- --nocapture
cargo +stable test -p mikebom --test spdx3_regression -- --nocapture
```

Both must pass identically. If any SPDX golden regenerates, the implementation has touched SPDX emission — the fix is over-scoped and must be narrowed back to CDX-only.

**Rationale**: SPDX uses different identifier conventions (SPDXIDs, IRIs) and a different document-subject convention (`SPDXRef-DOCUMENT` describes-relationship). The fix targets `cyclonedx/builder.rs` only; SPDX emission lives in `mikebom-cli/src/generate/spdx*/` and is structurally separate. This check is cheap insurance against scope creep.

**Out of scope**: any SPDX-side closure-invariant audit. That is a candidate follow-up milestone.
