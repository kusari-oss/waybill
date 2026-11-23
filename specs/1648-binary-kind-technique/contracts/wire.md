# Wire contract: dynamic linkage and embedded source paths are labelled binary analysis

## SBOM documents

A component of kind `dynamic-linkage` or `embedded-source-path` changes in exactly one place per format:

| Format | `main` | After |
|---|---|---|
| CycloneDX `evidence.identity[0].methods[0].technique` | `manifest-analysis` | `binary-analysis` |
| SPDX 2.3 D1 annotation (`evidence.identity`) `technique` | `package-database` | `binary-analysis` |
| SPDX 3 D1 annotation `technique` | `package-database` | `binary-analysis` |

The confidence and every other field are unchanged. A component that merges with one lists both methods, in spec 1646's form (CycloneDX `methods`: `manifest-analysis` then `binary-analysis`; SPDX D1 adds `methods`). No new field, annotation or catalog row.

## Fixture cases (`waybill-cli/tests/binary_kind_technique.rs`)

| Case | Input (synthetic, `waybill-fixture-*`) | Component | Expected technique, all formats |
|---|---|---|---|
| K1 | ELF with `DT_NEEDED libwaybill-fixture-dep.so.1` | `pkg:generic/libwaybill-fixture-dep.so.1` (`dynamic-linkage`) | `binary-analysis` |
| K2 | a bundled tree named by embedded source paths (spec 1534's shape) | its `embedded-source-path` component | `binary-analysis` |
| K3 | ELF with a `.dep-v0` section | each `pkg:cargo/…` (`cargo-auditable`) | `manifest-analysis`, byte-identical to `main` |
| K4 | ELF with a `.note.package` note | its `elf-note-package` component | `manifest-analysis`, byte-identical to `main` |
| K5 | a synthetic Go binary with BuildInfo | its `pkg:golang/…` components | `manifest-analysis`, byte-identical to `main` |
| K6 | K1 plus `lib/pkgconfig/libwaybill-fixture-dep.so.1.pc`, giving an equal PURL | the merged component | `manifest-analysis` then `binary-analysis` |
| K7 | a banner match (spec 1646 baseline) | `pkg:generic/zlib@…` | `binary-analysis`, as on `main` |

**K6 is conditional.** If no fixture can give a `dynamic-linkage` component and a record equal PURLs (a pkg-config name cannot carry a soname's version form), K6 uses `embedded-source-path` against a pkg-config record instead. If neither can merge, FR-004 is recorded as unreachable on real layouts, with the census count, and K6 is a unit test on constructed components.

Every case runs `waybill sbom parity-check` over its three documents.

## Census (DEBUG `waybill::version_census`)

The shapes are in data-model.md. Log-only examples:

```
technique_change={"evidence_kind":"dynamic-linkage","new":"binary-analysis","old":"package-database","purl":"pkg:generic/libwaybill-fixture-dep.so.1"}
kind_merge={"loser":{"evidence_kind":"dynamic-linkage","mechanism":null},"purl":"…","site":"S1","winner":{"evidence_kind":null,"mechanism":"pkg-config"}}
binary_derived_kind={"evidence_kind":"go-buildinfo","path":"…/waybill-fixture-gobin","purl":"pkg:golang/…","technique":"package-database"}
```
