# Contract — `mikebom:source-files` dedup / single-source-of-truth

Phase 1 output. Defines the invariant that `mikebom:source-files` MUST be emitted from EXACTLY ONE source per component, restoring C18 parity across all three formats.

## Pre-145 (broken)

`mikebom:source-files` is set from TWO independent sources on every component (per research §C):

1. **Field-derived** — `c.evidence.source_file_paths` (`Vec<String>`), set by the orchestrator at scan time. All three emitters read this field directly:
   - CDX: `cyclonedx/builder.rs:830-839`
   - SPDX 2.3: `spdx/annotations.rs:302-308`
   - SPDX 3: `spdx/v3_annotations.rs:267-273`
2. **Reader-stamped** — `c.extra_annotations["mikebom:source-files"]`, set by per-reader code. Maven's nested-JAR reader is the documented offender (`maven.rs:2244`); workspace + yocto readers also touch this key but on non-Maven components, so their drift exposure is different.

   All three emitters ALSO iterate `c.extra_annotations` and emit each entry as a property/annotation:
   - CDX: `cyclonedx/builder.rs:1086-1098`
   - SPDX 2.3: `spdx/annotations.rs:371`
   - SPDX 3: `spdx/v3_annotations.rs:332-337`

Net effect: 2 emissions per component, with different surviving entries per format → C18 `Y | ... | Y` value-drift findings.

## Post-145 (single-source invariant)

The implement-phase reproduction on `polyglot-builder-image` will choose between two fix paths (per research §C "Decision" part). The CONTRACT in both cases:

**Invariant**: On any single component, the set of `mikebom:source-files` emissions across the three formats MUST be byte-equivalent. In array form, that means same elements in the same order.

### Option 1 — emitter-side dedup guard

At each `extra_annotations` iteration site, skip emitting keys that are already emitted from a known field-derived source. Add a constant `FIELD_OWNED_ANNOTATION_KEYS: &[&str] = &["mikebom:source-files"]` and a helper `is_field_owned(key) -> bool`. The iteration:

```rust
for (key, value) in &c.extra_annotations {
    if is_field_owned(key) || is_internal_emission_key(key) {
        continue;
    }
    push(out, key, value.clone());
}
```

Existing milestone-127 internal-key filter (`is_internal_emission_key`) sits at the same site; the field-owned guard sits next to it.

### Option 2 — Maven reader stops stamping into the colliding key

In `maven.rs:2244`, replace `mikebom:source-files` with `mikebom:source-files-nested-url` (or simply drop the stamping if no downstream consumer depends on the value). The field-derived emission at `c.evidence.source_file_paths` continues to win on the resulting single-source emission.

### Comparison

| | Option 1 | Option 2 |
|---|---|---|
| Surface area | 3 emission sites edited | 1 reader site edited (or removed) |
| Blast radius | Per-emitter behavior change; no reader-side change | Maven reader behavior change; consumers of the existing key lose it |
| Defensive against future drift | YES — any new field-owned key added later is automatically guarded | NO — requires per-reader discipline |
| Wire-byte changes | Maven components lose the duplicate `mikebom:source-files-from-extra-annotations` entry across all three formats | Same as Option 1 PLUS the Maven reader emits a different key |

Implement-phase decision after fixture-reproduction. Defense-in-depth ideal: apply BOTH (Option 1 catches future drift; Option 2 cleans up the trap at the source).

## Test contract (in-file test for either option)

```rust
#[test]
fn mikebom_source_files_byte_equivalent_across_emitters_for_maven_nested_jar() {
    // Synthetic ResolvedComponent matching a Maven nested-JAR component:
    // both source paths (field-derived rootfs-relative + extra_annotations
    // JAR-URL) are present pre-145.
    let mut c = synthetic_maven_component_with_source_files(
        "root/.m2/repository/.../surefire-booter-3.2.2.jar",
    );
    c.extra_annotations.insert(
        "mikebom:source-files".to_string(),
        json!("root/.m2/repository/.../surefire-booter-3.2.2.jar!META-INF/MANIFEST.MF"),
    );

    let cdx_value = extract_source_files_from_cdx(&c);
    let spdx2_value = extract_source_files_from_spdx2(&c);
    let spdx3_value = extract_source_files_from_spdx3(&c);

    assert_eq!(cdx_value, spdx2_value, "CDX vs SPDX 2.3 source-files drift");
    assert_eq!(cdx_value, spdx3_value, "CDX vs SPDX 3 source-files drift");
}
```

(`extract_source_files_from_*` helpers reuse the milestone-071 parity-extractor infrastructure.)

## Invariants

| | Before | After |
|---|---|---|
| Number of `mikebom:source-files` entries per component per emitted format | up to 2 (field-derived + reader-stamped) | EXACTLY 1 |
| Value source | per-emitter dedup-order-dependent | field-derived `c.evidence.source_file_paths` (Option 1 + Option 2 both), normalized via milestone-133 pipeline |
| Cross-format equality (C18 `Directionality::SymmetricEqual`) | violated for Maven nested-JAR components on image scans | restored |

## Doc-comment additions (recurrence prevention)

Both the field-setter site (`scan_fs/mod.rs:198` and `:636`) and the Maven reader stamping site (`maven.rs:2244`) MUST gain a one-line note like:

```rust
// NOTE (milestone 145): `mikebom:source-files` has TWO emission sources —
// this field (canonical) AND `extra_annotations["mikebom:source-files"]`
// (legacy, dedup'd at emit time). DO NOT stamp the latter from a new
// reader; if you need to carry per-reader source provenance, use a
// distinct key like `mikebom:<reader>-source-url`.
```

This is the audit-trail signal for the next time someone is tempted to add a second source.
