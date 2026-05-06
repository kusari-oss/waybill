# T001 audit — milestone 076 pre-flight

Captured for T010 / T013 / T014 / T015 wiring.

## (a) Existing build-tier fixtures

No integration tests invoke `mikebom trace run` end-to-end against an
in-toto subject set. `mikebom trace run` requires Linux + eBPF +
privileges, so unit/integration tests that exercise the trace pipeline
are gated on those preconditions. Tests that touch
`auto_detect_build_tier_identifiers` (milestone 074) build synthetic
git-fixture tempdirs and never go near the trace pipeline.

Implication for T010: synthetic-subject-set fixtures + direct calls to
`subject_identifiers_from_attestation_subjects` are the only way to
exercise the build-tier subject auto-detect path on macOS/CI.

## (b) Trace subject-set field

The trace's subject set is **not held in long-lived in-process state**
inside `cli/run.rs::execute`. The flow is:

1. `super::scan::execute(scan_args).await?` writes the in-toto
   attestation (DSSE envelope) to disk at `args.attestation_output`.
2. `super::generate::execute(generate_args, false).await?` reads it
   back via `crate::attestation::serializer::read_attestation`, yielding
   an `InTotoStatement { subject: Vec<ResourceDescriptor>, … }`.

`ResourceDescriptor` lives at `mikebom_common::attestation::statement`:

```rust
pub struct ResourceDescriptor {
    pub name: String,
    pub digest: BTreeMap<String, String>,
}
```

The post-trace, in-process subject collection is therefore
`statement.subject: &Vec<ResourceDescriptor>`. The internal
`mikebom_cli::attestation::subject::Subject` enum exists but is the
pre-serialization in-process resolver type; by the time `run.rs` is
ready to merge subjects into identifiers, the canonical form is
`ResourceDescriptor`.

**T010 wiring decision**: read the attestation file once in
`cli/run.rs::execute` between `scan::execute` and `generate::execute`,
extract `statement.subject`, call
`subject_identifiers_from_attestation_subjects(&statement.subject)`,
and merge into the existing `assembled_ids` flow. This adds one
additional disk read but keeps all subject parsing in the
already-canonical wire form.

`subject_identifiers_from_attestation_subjects`'s parameter type is
`&[ResourceDescriptor]` accordingly.

## (c) Per-format component-emission sites

| Format | File | Function | Per-component carrier |
|---|---|---|---|
| CDX 1.6 | `mikebom-cli/src/generate/cyclonedx/builder.rs` | `CycloneDxBuilder::build_components` (~line 206) | `properties: Vec<serde_json::Value>` (~line 420), set on `entry` at line 649 |
| SPDX 2.3 | `mikebom-cli/src/generate/spdx/packages.rs` | `component_to_package` (~line 341) | `external_refs: Vec<SpdxExternalRef>` (~line 369) |
| SPDX 3 | `mikebom-cli/src/generate/spdx/v3_packages.rs` | `build_packages` (~line 52) | `externalIdentifier` array assembled via `super::v3_external_ids::build_external_identifiers_for(c)` then set at line 151 |

Both SPDX 2.3 (`packages.rs::build_packages`) and SPDX 3
(`v3_packages.rs::build_packages`) currently take `&[ResolvedComponent]`
or a `&ScanArtifacts<'_>` directly. To wire `--component-id`,
`v3_packages::build_packages` needs the `&[ComponentIdentifierFlag]`
slice threaded through. Easiest path: extend its signature to take
`scan: &ScanArtifacts<'_>` (or just the `component_identifiers` slice)
and update the single call site in `v3_document::build_document`.

For CDX, `CycloneDxBuilder` already stores most cross-cutting state
as member fields (`identifiers`, `source_document_binding`, etc.) —
add `component_identifiers: Vec<ComponentIdentifierFlag>` plus a
`with_component_identifiers` setter, populated in `cyclonedx/mod.rs`
serializer alongside `with_identifiers`.

## (d) Pre-existing per-component entries (preserve at original positions)

### CDX `components[].properties[]`
Built by `builder.rs::build_components` in supply order:
- `mikebom:cpe-candidates` (when len > 1)
- `mikebom:source-files` (when include_source_files set)
- `mikebom:lifecycle-scope` (non-runtime + include_dev)
- `mikebom:requirement-range`
- `mikebom:source-type`
- `mikebom:co-owned-by`
- evidence-derived properties (variable list)
- `mikebom:sbom-tier`
- `mikebom:npm-role`
- `mikebom:raw-version`
- `mikebom:buildinfo-status`
- `mikebom:evidence-kind`
- `mikebom:confidence`
- `mikebom:binary-class`
- `mikebom:binary-stripped`
- `mikebom:linkage-kind`
- `mikebom:detected-go`
- `mikebom:shade-relocation`
- `mikebom:binary-packed`
- All `extra_annotations` entries (incl. `mikebom:not-linked`,
  `mikebom:component-role`, `mikebom:shade-relocation`, etc.)

T013 must append `--component-id` entries AFTER all of the above, in
lexical order by `(scheme, value)`.

### SPDX 2.3 `Package.externalRefs[]`
Built by `packages.rs::component_to_package` in supply order:
- `purl` (PACKAGE-MANAGER, always first)
- `cpe23Type` (SECURITY, when CPE present)
- `homepage`/`vcs`/`distribution` etc. from
  `external_references` (OTHER)
- Milestone-073 built-in identifiers (PERSISTENT-ID; only on
  main-module)

T014 must append `--component-id` entries AFTER all of the above, as
PERSISTENT-ID rows, in lexical order by `(scheme, value)`.

### SPDX 3 `Element.externalIdentifier[]`
Built by `v3_external_ids::build_external_identifiers_for(c)` in supply
order — typically the PURL row first then CPEs. T015 must append
`--component-id` entries AFTER, in lexical order by `(scheme, value)`.

## Implementation summary

- T010: read `statement.subject: &Vec<ResourceDescriptor>` in
  `run.rs::execute` after `scan::execute` returns; wire into
  `assembled_ids`.
- T013/T014/T015: thread `&[ComponentIdentifierFlag]` through
  builder/packages/v3_packages; iterate flags, match by
  `purl == component.purl.as_str()` byte-equality, append per-format
  entries lex-sorted by `(scheme, value)` after pre-existing entries;
  warn on zero match after the per-component loop completes.
