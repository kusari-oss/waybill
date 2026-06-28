# Contract — `mikebom:lifecycle-scope` SPDX 3 emission

Phase 1 output. Defines the SPDX 3 emission contract for the `mikebom:lifecycle-scope` annotation, bringing SPDX 3 into parity with CDX 1.6 + SPDX 2.3.

## Pre-145 (broken)

| Format | Behavior |
|---|---|
| CDX 1.6 | Emits for non-Runtime scopes (`cyclonedx/builder.rs:851` via `s.is_non_runtime()` filter) |
| SPDX 2.3 | Emits for non-Runtime scopes (`spdx/annotations.rs:227-236`, `LifecycleScope::Runtime => None` match arm) |
| SPDX 3 | **DOES NOT EMIT** (the emitter at `spdx/v3_annotations.rs` has zero references to `lifecycle_scope` / `LifecycleScope`) |

Audit cluster pattern: `Y | Y | -` on 261 components in the `node-dev-vs-prod` fixture.

## Post-145 (parity restored)

All three formats emit `mikebom:lifecycle-scope` with the SAME value mapping:

| `ResolvedComponent.lifecycle_scope` | Emit annotation? | Value string |
|---|---|---|
| `Some(LifecycleScope::Development)` | YES | `"development"` |
| `Some(LifecycleScope::Build)` | YES | `"build"` |
| `Some(LifecycleScope::Test)` | YES | `"test"` |
| `Some(LifecycleScope::Runtime)` | **NO** (Runtime is the default; no annotation needed; preserves CDX + SPDX 2.3 existing behavior) | n/a |
| `None` | NO | n/a |

## Insertion location in SPDX 3 emitter

Per research §B, insert after the existing `mikebom:raw-version` push at approximately line 264 of `v3_annotations.rs`:

```rust
// C42 mikebom:lifecycle-scope — parity-bridging annotation mirroring
// the SPDX 2.3 sibling at annotations.rs:227-236 (milestone 145 US2).
// CDX and SPDX 2.3 both emit this annotation for non-Runtime scopes;
// SPDX 3 was the pre-145 outlier.
if let Some(ref scope) = c.lifecycle_scope {
    use mikebom_common::resolution::LifecycleScope;
    let s = match scope {
        LifecycleScope::Development => Some("development"),
        LifecycleScope::Build => Some("build"),
        LifecycleScope::Test => Some("test"),
        LifecycleScope::Runtime => None,
    };
    if let Some(s) = s {
        push(out, "mikebom:lifecycle-scope", json!(s));
    }
}
```

## Test contract (in-file unit tests, post-fix)

```rust
#[test]
fn spdx3_lifecycle_scope_development_emits() {
    let c = ResolvedComponent {
        lifecycle_scope: Some(LifecycleScope::Development),
        ..synthetic_resolved_component()
    };
    let annos = build_component_annotations_v3(&c, /* include_dev = */ true);
    assert!(
        annos.iter().any(|a| envelope_field(a) == "mikebom:lifecycle-scope"
            && envelope_value(a) == &json!("development")),
        "expected mikebom:lifecycle-scope=development for Development scope; got {annos:?}"
    );
}

#[test]
fn spdx3_lifecycle_scope_runtime_omitted() {
    let c = ResolvedComponent {
        lifecycle_scope: Some(LifecycleScope::Runtime),
        ..synthetic_resolved_component()
    };
    let annos = build_component_annotations_v3(&c, /* include_dev = */ true);
    assert!(
        !annos.iter().any(|a| envelope_field(a) == "mikebom:lifecycle-scope"),
        "expected no mikebom:lifecycle-scope annotation for Runtime scope; got {annos:?}"
    );
}
```

(`envelope_field` and `envelope_value` are test helpers parsing the `MikebomAnnotationCommentV1` envelope; depending on the test-module already-existing infrastructure in `v3_annotations.rs`, these may need to be added.)

## Wire byte-deltas vs pre-145

| Format | Goldens affected? |
|---|---|
| CDX | NO |
| SPDX 2.3 | NO |
| SPDX 3 | YES — every golden containing a fixture with a non-Runtime `lifecycle_scope` component will gain a new annotation entry. `node-dev-vs-prod` and similar npm fixtures dominate. |

## Cross-emitter parity invariant

For every `ResolvedComponent c` with `c.lifecycle_scope.filter(|s| s.is_non_runtime()).is_some()`:
- the SBOM emitted as CDX MUST contain `mikebom:lifecycle-scope` for `c`'s component
- the SBOM emitted as SPDX 2.3 MUST contain `mikebom:lifecycle-scope` for `c`'s package
- the SBOM emitted as SPDX 3 MUST contain `mikebom:lifecycle-scope` for `c`'s software package

All three MUST agree on the value string.

For every `c` with `c.lifecycle_scope` `None` or `Some(Runtime)`:
- no format emits `mikebom:lifecycle-scope`.
