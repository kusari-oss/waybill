# Contract: `mikebom:produces-binaries` source-tier property

**Feature**: 116-produces-binaries
**Date**: 2026-06-13
**Consumed by**: cross-tier binder (`mikebom-cli/src/binding/verify.rs`), any downstream SBOM tooling that wants to know binary names a source-tier component is expected to produce
**Spec mapping**: FR-001, FR-005 (Cargo), FR-006 (npm), FR-007 (pip), FR-008 (gem), FR-009 (maven), FR-010 (Go), FR-011 (Principle V audit), FR-012 (union-merge), FR-015 (consumer self-sufficiency)

## Property name

```text
mikebom:produces-binaries
```

The kebab-case is intentional and matches the existing `mikebom:component-role` (milestone 047/049 C40) and `mikebom:source-document-binding` (milestone 072) conventions. Other prefixes (`mikebom:produced-binaries`, `mikebom:executables`, `mikebom:binary-names`) were considered but `produces-binaries` won for clarity — the verb makes the capability-claim semantics explicit.

## Property value

A JSON-encoded string whose decoded form is a non-empty JSON array of strings. Each string is one canonical binary name.

```json
"[\"baz\", \"baz-cli\", \"baz-debug\"]"
```

(In CDX 1.6 the outer string-encoding is forced by the `properties[].value: string` field-type constraint. SPDX 2.3/3 wraps the same array in the existing `MikebomAnnotationCommentV1` envelope.)

### Value invariants

1. **Non-empty**: an empty array is NEVER serialized. The property is OMITTED entirely when no binary names can be extracted (per FR-001).
2. **Lowercase ASCII**: every element matches `^[a-z0-9][a-z0-9_-]*$` after normalization. Non-ASCII characters are stripped at extraction time (Cargo / npm / pip / gem / maven ecosystem manifests don't carry non-ASCII binary names in practice).
3. **Extensionless**: no element ends in `.exe`, `.jar`, `.dll`, `.so`, `.dylib`, `.bin`. Suffix translation is the binder's job (contracts/binder.md § "Image-side normalization").
4. **Sorted lex**: `entries == LC_ALL=C sort -u(entries)`. Cross-host byte-stable.
5. **Deduped**: `len(entries) == len(set(entries))`.

### Value examples

| Ecosystem | Manifest input | Property value (decoded) |
|---|---|---|
| Cargo (default-binary) | `Cargo.toml` `name = "baz"` + `src/main.rs` | `["baz"]` |
| Cargo (multiple `[[bin]]`) | `Cargo.toml` `[[bin]] name = "baz"` + `[[bin]] name = "baz-helper"` | `["baz", "baz-helper"]` |
| Cargo (`src/bin/*.rs`) | `src/bin/foo.rs` + `src/bin/bar.rs` | `["bar", "foo"]` |
| Cargo (all three sources) | `[[bin]] name = "baz"` + `src/main.rs` + `src/bin/foo.rs` | `["baz", "foo"]` (note: `src/main.rs` uses package name `baz`) |
| npm string-form | `"name": "baz"`, `"bin": "./bin/baz.js"` | `["baz"]` |
| npm object-form | `"bin": {"baz": "./cli.js", "baz-init": "./init.js"}` | `["baz", "baz-init"]` |
| pip | `[project.scripts] baz = "baz.cli:main"` | `["baz"]` |
| gem | `executables = ["baz", "baz-server"]` | `["baz", "baz-server"]` |
| maven shade-plugin | `<finalName>baz</finalName>` (produces `baz.jar`) | `["baz"]` |
| Go (US3) | `cmd/baz/main.go` + `cmd/baz-helper/main.go` | `["baz", "baz-helper"]` |
| Cargo (library-only) | No `[[bin]]`, no `src/main.rs`, no `src/bin/*.rs` | (property OMITTED) |

## Carrying component

The property MUST be stamped on a source-tier SBOM component whose `mikebom:component-role` (C40 from milestone 047/049) is `main-module`. The property MUST NOT be stamped on:

- Transitive-dependency components (those representing libraries the main module depends on)
- Components whose `mikebom:component-role` is `build-tool`, `language-runtime`, `workspace-root`, or any other non-main-module role
- Components emitted from non-main-module-aware ecosystems (rpm, dpkg, alpine, vcpkg, conan, nuget, swift, west, idf_component, opkg, yocto-bb)

This main-module-only scoping is per spec clarification Q3. It keeps the property's semantics crisp (it's a CAPABILITY claim about the operator's project) and confines the per-ecosystem extractor scope to a single component per ecosystem per scan.

## Union-merge rule (FR-012)

When a per-ecosystem extractor runs against a manifest AND the resulting main-module component's `extra_annotations` ALREADY contains a `mikebom:produces-binaries` entry (e.g., from a hand-edited input SBOM consumed via the milestone-072 `--from-sbom` round-trip, OR from a future feature that pre-stuffs the property), the final value is `LC_ALL=C sort -u(existing ∪ discovered)`. Specifically:

- Pre-existing operator entries are NEVER dropped.
- Pre-existing operator entries that mikebom did not discover stay in the output.
- Newly-discovered entries are added.
- Duplicates collapse to one entry.
- The resulting list is re-sorted.

This guarantees operator-supplied declarations survive a `mikebom sbom scan --from-sbom <prev>` round-trip even when the underlying manifest doesn't carry the operator's hand-edited names.

## Constitution Principle V citation (FR-011)

Per research.md § Decision 1, the Principle V bullet 5 audit concluded that no native CycloneDX 1.6 or SPDX 2.3/3.x field expresses "list of executable names this package produces." Closest neighbor (CDX `externalReferences[type=executable]`) carries URL endpoints, not output identifiers. SPDX `Package.annotation[]` is the documented extensibility path for novel semantics. Therefore `mikebom:produces-binaries` is justified as a parity-bridging annotation. The audit result + this justification is replicated in `docs/reference/sbom-format-mapping.md` per the principle's documentation requirement.

## Forward-compatibility

- **New ecosystems**: a future PR adding (e.g.) an Elixir reader extends the table above without changing the property name, shape, or invariants. The contract is ecosystem-agnostic on the consumer side.
- **Removal of an ecosystem from main-module emission**: would be a breaking change for downstream consumers depending on the property. None planned.
- **Changing the property value shape** (e.g., to an object with name → path mapping): would require a new property name (e.g., `mikebom:produces-binaries-v2`) and a deprecation cycle for the old one. The current shape is committed indefinitely.
