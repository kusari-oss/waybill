# Contract: `mikebom:optional-derivation` Annotation

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Purpose (Principle V KEEP-BOTH carve-out)

The `mikebom:optional-derivation` annotation records WHICH mechanism populated the `LifecycleScope::Optional` classification on a target component. This is finer-grained information than the standard's native `OPTIONAL_DEPENDENCY_OF` relationship type carries — the native relationship type says "this edge is optional"; the annotation additionally says "we know it's optional because Cargo's manifest had `optional = true`" (vs. npm's `optionalDependencies` field, vs. Maven's `<optional>true</optional>` element, etc.).

Principle V KEEP-BOTH polarity per m178 precedent: native primary + annotation supplement.

## Annotation name and value vocabulary

**Name**: `mikebom:optional-derivation` (colon prefix matches all other mikebom annotations; kebab-case within the suffix).

**Value type**: JSON string. Enum-constrained values as of m179:

- `"cargo-optional-true"` — Cargo `[dependencies]` entry with `optional = true`.
- `"npm-optional-dependencies"` — npm `optionalDependencies` top-level field (or yarn/pnpm variants that read the same field).
- `"pip-extras-require"` — Python `[project.optional-dependencies.<extra>]` (pyproject.toml) OR `extras_require` (setup.py) OR `[options.extras_require]` (setup.cfg) OR uv `optional` markers OR Poetry extras.
- `"maven-optional-element"` — Maven `<dependency>` with `<optional>true</optional>` child element.
- `"gradle-compile-only"` — Gradle `compileOnly` dep configuration.
- `"erlang-optional-applications"` — Erlang `.app.src` `optional_applications` list.

Additional values MAY be added by future milestones without changing the annotation name. The value vocabulary is intentionally open-ended per FR-019.

## Emitter carrier per format

### CycloneDX 1.6

Emitted as `component.properties[]` entry:

```json
{
  "name": "mikebom:optional-derivation",
  "value": "cargo-optional-true"
}
```

Emission site: `mikebom-cli/src/generate/cyclonedx/builder.rs`, after the m112 `mikebom:build-inclusion-derivation` property emission block.

### SPDX 2.3

Emitted as a `Package.annotations[]` entry wrapping the `MikebomAnnotationCommentV1` envelope (matches m147 peer-edge-targets and m112 build-inclusion-derivation shape):

```json
{
  "annotationDate": "<scan-emission-time>",
  "annotationType": "OTHER",
  "annotator": "Tool: mikebom",
  "annotationComment": "{\"schema\":\"mikebom.annotation.v1\",\"name\":\"mikebom:optional-derivation\",\"value\":\"cargo-optional-true\"}"
}
```

Note the `annotationComment` field is a JSON-string-encoded envelope, matching every other `mikebom:*` annotation carried through SPDX 2.3 today.

Emission site: `mikebom-cli/src/generate/spdx/annotations.rs`, colocated with m112 emission.

### SPDX 3.0.1

Emitted as a `spdx:Annotation` node with the same envelope in `spdx:statement`:

```json
{
  "type": "Annotation",
  "spdxId": "spdx:<derived-node-id>",
  "subject": "spdx:<package-spdx-id>",
  "annotationType": "other",
  "statement": "{\"schema\":\"mikebom.annotation.v1\",\"name\":\"mikebom:optional-derivation\",\"value\":\"cargo-optional-true\"}",
  "creationInfo": "_:mikebomCreationInfo",
  "creationBy": ["_:mikebomToolAgent"]
}
```

Emission site: `mikebom-cli/src/generate/spdx/v3_annotations.rs`, colocated with m112 emission.

## Parity extractor registration

A new catalog row MUST be added to `mikebom-cli/src/parity/extractors/` with:

- Directionality: `SymmetricEqual` — value MUST match byte-identically across CDX, SPDX 2.3, and SPDX 3 for the same source component.
- Canonical name: `mikebom:optional-derivation`
- Column in parity catalog: matches the m112 `mikebom:build-inclusion-derivation` catalog row shape.

## Byte-identity requirement (SC-008)

For every fixture that exercises the new signal:

1. Regenerate CDX + SPDX 2.3 + SPDX 3 goldens via `MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1 cargo test --workspace`.
2. For each component with `mikebom:optional-derivation` in the CDX properties, the SAME value MUST appear in the SPDX 2.3 annotation for the SAME PURL AND in the SPDX 3 annotation for the SAME PURL.
3. Parity CI (existing `parity_symmetric_equal.rs` harness) MUST include the new annotation in its regression sweep.

## Non-goals

- The annotation is NOT emitted for components with `build_inclusion = NotNeeded` (the pico-flagship Go case). Those use the existing `mikebom:build-inclusion-derivation` annotation with value `"go-mod-why"`. The two annotation names are disjoint — a component either exercises `optional` (from a manifest) OR `NotNeeded` (from build-graph analysis), never both.
- The annotation is NOT emitted for components with any m052 `lifecycle_scope` (Development, Build, Test). Those already have the `mikebom:lifecycle-scope` annotation; keeping the annotation name orthogonal preserves audit clarity.

## Verification tests

- Unit test: emitter code writes the correct property/annotation shape for each of the 6 derivation values.
- Integration test: end-to-end scan of a Cargo fixture emits the annotation in all three formats with byte-identical `value` field.
- Parity CI regression: annotation appears in the parity catalog with `SymmetricEqual` polarity + all three format extractors round-trip it correctly.
