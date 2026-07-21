# Contract: mikebom:* → waybill:* annotation prefix migration mapping

**Feature**: 214-rename-to-waybill
**Kind**: Wire-shape contract (downstream SBOM consumer migration reference)
**Consumers**: security scanners, compliance tools, VEX processors, or any downstream code that parses `metadata.properties[].name`, SPDX 2.3 `Annotation.comment`, or SPDX 3 `Annotation` values from Waybill-generated SBOMs.

## Migration rule

**Mechanical prefix swap**. For any annotation key whose name matches `^mikebom:(.+)$`, the post-rename key is `waybill:\1`. Every suffix is preserved verbatim — the 192 distinct annotation-name suffixes remain 192 distinct suffixes. **All annotation VALUES are unchanged**; only the string prefix `mikebom:` → `waybill:` changes.

Cross-layer contract: post-rename `waybill:*` annotation names MUST match the corresponding `FilterCategoryTag::name()` values and any other in-code identifier that mirrors an annotation name. In other words, the rename is atomic across the wire + the code that produces + consumes it.

## Enumeration (192 distinct annotations surveyed 2026-07-21)

The full list of 192 pre-rename annotation names is grep-derivable from the source at any pre-rename SHA:

```bash
grep -rho '"mikebom:[a-z-]*"' waybill-cli/src/ waybill-common/src/ | sort -u
```

**Structural observations** (rather than a 192-entry table):

- All annotations use lowercase kebab-case suffixes.
- Suffix vocabulary spans: `also-detected-via`, `arch-source`, `assembly-*`, `bazel-*`, `binary-*`, `build-*`, `cargo-*`, `cmake-*`, `compiler-pipeline-*`, `component-*`, `composer-*`, `dep-*`, `elf-*`, `erlang-*`, `file-inventory-*`, `fingerprint-*`, `helm-*`, `layer-*`, `lifecycle-*`, `maven-*`, `npm-*`, `oci-*`, `optional-*`, `pip-*`, `pnpm-*`, `podman-*`, `pyproject-*`, `rpm-*`, `rust-*`, `scala-*`, `scanner-*`, `source-*`, `spdx-*`, `stackage-*`, `supplement-*`, `symbol-*`, `sync-*`, `trace-integrity-*`, `vcs-*`, `vex-*`, `workspace-*`, `yarn-*`, `yocto-*` — plus one-off keys like `confidence`, `copyright`, `description`.

**Sample subset** (first 60 by lex order — full enumeration is in the pre-rename source grep):

```
mikebom:also-detected-via                 → waybill:also-detected-via
mikebom:arch-source                       → waybill:arch-source
mikebom:assembly-cultures                 → waybill:assembly-cultures
mikebom:assembly-version-file             → waybill:assembly-version-file
mikebom:assembly-version-informational-stripped → waybill:assembly-version-informational-stripped
mikebom:assembly-version-informational    → waybill:assembly-version-informational
mikebom:assembly-version-runtime          → waybill:assembly-version-runtime
mikebom:assertion-conflict                → waybill:assertion-conflict
mikebom:bazel-archive-name                → waybill:bazel-archive-name
mikebom:bbappend-applied                  → waybill:bbappend-applied
mikebom:binary-class                      → waybill:binary-class
mikebom:binary-packed                     → waybill:binary-packed
mikebom:binary-stripped                   → waybill:binary-stripped
mikebom:build-inclusion-derivation        → waybill:build-inclusion-derivation
mikebom:build-inclusion                   → waybill:build-inclusion
mikebom:build-reference                   → waybill:build-reference
mikebom:buildinfo-status                  → waybill:buildinfo-status
mikebom:built-in-requirement              → waybill:built-in-requirement
mikebom:cargo-auditable-kind              → waybill:cargo-auditable-kind
mikebom:cargo-auditable-source            → waybill:cargo-auditable-source
mikebom:cargo-vcs-source-url              → waybill:cargo-vcs-source-url
mikebom:cmake-find-package-name           → waybill:cmake-find-package-name
mikebom:co-owned-by                       → waybill:co-owned-by
mikebom:compiler-pipeline-completeness    → waybill:compiler-pipeline-completeness
mikebom:component-role                    → waybill:component-role
mikebom:component-tier                    → waybill:component-tier
mikebom:composer-type                     → waybill:composer-type
mikebom:confidence                        → waybill:confidence
mikebom:copyright                         → waybill:copyright
mikebom:cpe-candidates                    → waybill:cpe-candidates
… (remainder follows same pattern; full list at git blob `specs/214-rename-to-waybill/contracts/annotation-migration-full.txt` if generated post-plan)
```

**Sample tail** (last 20 by lex order):

```
mikebom:umbrella-root                     → waybill:umbrella-root
mikebom:unresolved-declared-dep           → waybill:unresolved-declared-dep
mikebom:vcs-declared-ref                  → waybill:vcs-declared-ref
mikebom:vcs-ref                           → waybill:vcs-ref
mikebom:vendored                          → waybill:vendored
mikebom:version-status                    → waybill:version-status
mikebom:vex-binding-status                → waybill:vex-binding-status
mikebom:vex-propagation-refusals          → waybill:vex-propagation-refusals
mikebom:workspace-member                  → waybill:workspace-member
mikebom:workspaces-detected               → waybill:workspaces-detected
mikebom:yarn-alias                        → waybill:yarn-alias
mikebom:yocto-class-extend                → waybill:yocto-class-extend
mikebom:yocto-description                 → waybill:yocto-description
mikebom:yocto-layer-series                → waybill:yocto-layer-series
mikebom:yocto-layer-version-missing       → waybill:yocto-layer-version-missing
mikebom:yocto-layer-version               → waybill:yocto-layer-version
mikebom:yocto-layer                       → waybill:yocto-layer
mikebom:yocto-license-closed              → waybill:yocto-license-closed
mikebom:yocto-overrides-merged            → waybill:yocto-overrides-merged
mikebom:yocto-recipe-name                 → waybill:yocto-recipe-name
mikebom:yocto-recipe-version              → waybill:yocto-recipe-version
mikebom:yocto-unexpanded-vars             → waybill:yocto-unexpanded-vars
```

## Consumer migration recipe

**Any tool that parses Waybill SBOMs** needs one code change:

```rust
// Pre-rename consumer parsing CycloneDX metadata.properties:
if property.name == "mikebom:build-inclusion" { ... }

// Post-rename:
if property.name == "waybill:build-inclusion" { ... }

// Or handle both during migration (only if the consumer needs to parse
// BOTH pre-rename AND post-rename SBOMs — the tool itself only emits
// waybill:* post-rename per Clarification Q1):
match property.name.strip_prefix("mikebom:").or_else(|| property.name.strip_prefix("waybill:")) {
    Some(suffix) => { ... suffix-driven logic ... }
    None => { /* not an annotation from this tool */ }
}
```

**Shell / jq**:

```jq
# Pre-rename query
.metadata.properties[] | select(.name | startswith("mikebom:"))

# Post-rename query
.metadata.properties[] | select(.name | startswith("waybill:"))
```

**Batch update to consumer code**:

```bash
# Any consumer's parser tree, one-shot substitution:
sed -i.bak 's|"mikebom:|"waybill:|g' <consumer-source-files>
```

## Wire-shape stability guarantees (post-rename)

- **Suffix preservation**: The suffix after `:` is byte-identical to the pre-rename form for all 192 keys.
- **Value preservation**: The value stored under each annotation is byte-identical to the pre-rename form. No JSON restructuring, no enum-value renames, no type changes.
- **Position preservation**: Emit order of annotations within a component or metadata bag preserves the pre-rename ordering. The `serde` derive on each emit struct outputs fields in struct-definition order; struct definitions are unchanged except for identifier renames.
- **Cross-format consistency**: The same annotation appearing in CycloneDX 1.6 + SPDX 2.3 + SPDX 3.0.1 emissions uses the same waybill-prefixed name in all three. No format-specific prefix drift.

## Contract stability going forward

- Discriminants (annotation keys) are stable within the post-rename world.
- Newly-added annotations in future specs use the `waybill:<X>` form.
- If a future consumer needs to parse SBOMs from BOTH pre-rename (v0.1.0-alpha.65 and earlier) and post-rename (v0.1.0-alpha.66+), the recommended pattern is the `strip_prefix` alternation shown above.
