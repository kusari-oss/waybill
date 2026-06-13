# Quickstart — Automatic binary-name binding via produces-binaries

**Feature**: 116-produces-binaries
**Audience**: an operator running `mikebom sbom scan --image <ref> --bind-to-source <source.cdx.json>` in CI; a contributor adding a new ecosystem reader; a reviewer evaluating a binding-result audit trail.

## The TL;DR for operators

After this feature ships, the textbook source-to-image binding workflow gets one fewer flag. Specifically:

**Before (milestone 111, the workaround):**

```bash
mikebom sbom scan \
    --image myregistry/myimage:tag \
    --bind-to-source baz-source.cdx.json \
    --pkg-alias "pkg:generic/baz=pkg:cargo/baz@1.0.0" \
    --output baz-image.cdx.json
```

**After (this feature, the polished default):**

```bash
mikebom sbom scan \
    --image myregistry/myimage:tag \
    --bind-to-source baz-source.cdx.json \
    --output baz-image.cdx.json
```

The `--pkg-alias` flag is gone. The binding for `baz` is `verified` (or `weak`), never `Unknown`. The image-tier SBOM records that the alias was reached automatically via the source-side declaration.

## Five-minute walkthrough — Cargo (US1)

You manage a Rust project at `github.com/foo/bar` with `Cargo.toml`:

```toml
[package]
name = "baz"
version = "1.0.0"

# (no [[bin]] table — uses the default-binary rule)
```

with a `src/main.rs` file. CI runs:

```bash
# Step 1: scan the source tree.
mikebom sbom scan --path . --output baz-source.cdx.json

# Step 2: publish baz-source.cdx.json with the release artifact.
gh release upload v1.0.0 baz-source.cdx.json

# Step 3: build + push the image.
docker build -t myregistry/baz:1.0.0 .
docker push myregistry/baz:1.0.0

# Step 4: scan the image with --bind-to-source.
mikebom sbom scan \
    --image myregistry/baz:1.0.0 \
    --bind-to-source baz-source.cdx.json \
    --output baz-image.cdx.json
```

### What the source-tier SBOM contains

The main-module component (PURL `pkg:cargo/baz@1.0.0`) carries:

```json
{
  "purl": "pkg:cargo/baz@1.0.0",
  "properties": [
    { "name": "mikebom:component-role", "value": "\"main-module\"" },
    { "name": "mikebom:produces-binaries", "value": "[\"baz\"]" }
  ]
}
```

(CDX 1.6 forces the value to be a string; the array is JSON-encoded.)

### What the image-tier SBOM contains

The binary component (PURL `pkg:generic/baz` — milestone-096 binary discovery) carries:

```json
{
  "purl": "pkg:generic/baz",
  "properties": [
    {
      "name": "mikebom:source-document-binding",
      "value": "{\"source_doc_id\":\"...\",\"hash\":\"sha256:...\",\"strength\":\"verified\",\"algo\":\"binding-v1\",\"alias_from\":\"pkg:generic/baz\",\"alias_to\":\"pkg:cargo/baz@1.0.0\",\"alias_source\":\"automatic-from-produces-binaries\"}"
    }
  ]
}
```

The auditor reading this SBOM later can see:
- The binding is `verified` (hash evidence aligned).
- An alias was applied: `pkg:generic/baz` → `pkg:cargo/baz@1.0.0`.
- The alias source was `automatic-from-produces-binaries` (NOT operator-supplied via `--pkg-alias`).

## Five-minute walkthrough — Cargo workspace with multiple binaries

A Cargo workspace with a member crate `baz` declaring three binaries:

```toml
# Cargo.toml of the baz member crate

[package]
name = "baz"
version = "1.0.0"

[[bin]]
name = "baz"        # explicit [[bin]]

[[bin]]
name = "baz-cli"    # explicit [[bin]]

# also: src/bin/internal-debug.rs (implicit)
```

The source-tier scan emits on the `baz` main-module component:

```json
{ "name": "mikebom:produces-binaries", "value": "[\"baz\", \"baz-cli\", \"internal-debug\"]" }
```

(Lex-sorted; the three sources — Cargo `[[bin]]` table, `src/main.rs` default, `src/bin/*.rs` implicit — all contribute.)

An image containing all three binaries gets three binding results, one per `pkg:generic/<name>` component, all bound back to `pkg:cargo/baz@1.0.0`.

## Five-minute walkthrough — when the auto-alias doesn't fire

Library-only crate (no binaries):

```toml
[package]
name = "baz"
version = "1.0.0"

# (no [[bin]] table, no src/main.rs, no src/bin/*.rs)
[lib]
name = "baz"
```

The source-tier SBOM's main-module component carries `mikebom:component-role = "main-module"` but NO `mikebom:produces-binaries` property. The image-tier scan can't auto-alias because there's no declaration to consult — which is correct (a library crate doesn't produce binaries; trying to alias a `pkg:generic/baz` image-tier component to a library source-tier component would be a false match).

If the library happens to be embedded in an image (because some downstream binary links it statically), the image scan reports `pkg:generic/baz` as `Unknown { source-not-found-in-bind-target }` — unchanged from milestone-072 behavior. Operators who want to bind a library can fall back to milestone-111's `--pkg-alias` flag.

## Five-minute walkthrough — Operator override precedence

The operator decides they want to override the automatic alias for whatever reason (e.g., they want to point a `pkg:generic/baz` image binary at a DIFFERENT source-tier crate `pkg:cargo/baz-experimental@2.0.0` for an A/B comparison). They run:

```bash
mikebom sbom scan \
    --image myregistry/baz:1.0.0 \
    --bind-to-source baz-experimental-source.cdx.json \
    --pkg-alias "pkg:generic/baz=pkg:cargo/baz-experimental@2.0.0" \
    --output baz-image.cdx.json
```

The image-tier binding result records:

```json
{
  "alias_from": "pkg:generic/baz",
  "alias_to": "pkg:cargo/baz-experimental@2.0.0",
  "alias_source": "operator-supplied"
}
```

The automatic alias path is SUPPRESSED — even if `baz-experimental-source.cdx.json` ALSO declared `baz` as a produced binary name, the operator's explicit flag wins. Auditors reading the resulting SBOM can tell from the `alias_source = "operator-supplied"` field that the operator made an explicit decision, not that mikebom auto-resolved it.

## Contributor walkthrough — adding a new ecosystem (e.g., Elixir)

You want to add support for Elixir's `mix.exs` declaration of `escript: [name: :baz]` (Elixir's escript executable convention). Your steps:

### Step 1 — Extend the existing main-module extractor

Find the Elixir reader at `mikebom-cli/src/scan_fs/package_db/elixir/` (or create one if it doesn't exist; that's a milestone-of-its-own scope). Locate the function that builds the main-module `PackageDbEntry` (analogous to `build_cargo_main_module_entry` at `mikebom-cli/src/scan_fs/package_db/cargo.rs:352`). Add a parsing step:

```rust
// Parse the escript declaration from mix.exs.
let binary_names = parse_mix_escript_name(&manifest_content);
let binary_names = normalize_produces_binaries(binary_names);  // shared helper
if !binary_names.is_empty() {
    extra_annotations.insert(
        "mikebom:produces-binaries".to_string(),
        serde_json::Value::Array(
            binary_names.iter().map(|s| serde_json::Value::String(s.clone())).collect()
        ),
    );
}
```

### Step 2 — Add a fixture

Create `mikebom-cli/tests/fixtures/produces_binaries/elixir/` with a minimal Mix project + a `mix.exs` declaring the escript name. Optional: a pre-baked source-tier SBOM the test asserts byte-equivalence against.

### Step 3 — Add an integration test

Create `mikebom-cli/tests/produces_binaries_elixir.rs` following the same shape as `produces_binaries_cargo.rs`:

```rust
#[test]
fn elixir_main_module_emits_produces_binaries() {
    // Run mikebom against the fixture; assert the main-module component
    // carries mikebom:produces-binaries with the expected value.
}

#[test]
fn elixir_main_module_binds_image_via_auto_alias() {
    // Produce source SBOM + image SBOM; assert binding strength is
    // verified/weak; assert alias_source = automatic-from-produces-binaries.
}
```

### Step 4 — Update docs

Add an entry to the per-ecosystem extraction table in `contracts/property.md` § "Per-ecosystem extraction sources" naming the Elixir manifest field + reference. Update `docs/reference/sbom-format-mapping.md` if needed (the `mikebom:produces-binaries` row already exists; the per-ecosystem sub-rows live in the spec contracts).

### Step 5 — No binder change needed

The cross-tier binder (`mikebom-cli/src/binding/verify.rs`) is ecosystem-agnostic — it consumes `mikebom:produces-binaries` regardless of which extractor produced it. Your new Elixir support inherits the auto-alias machinery for free.

## When NOT to use produces-binaries

- **Library-only projects**: the main-module component has no binaries to declare. The extractor correctly omits the property.
- **Component is not a main module**: per spec clarification Q3, only main-module components carry the property. Transitive deps and non-main-module components NEVER do.
- **Ecosystems without binary-name declarations**: rpm / dpkg / alpine packages have their binaries identified at install time, not at source-manifest time. These ecosystems don't get the property.

## When the operator workflow goes weird

| Symptom | Diagnosis | Fix |
|---|---|---|
| Binding still `Unknown` after this feature ships | Source SBOM was produced pre-feature OR by a non-mikebom tool that doesn't carry the declaration | Re-scan source with post-feature mikebom, OR fall back to milestone-111 `--pkg-alias` |
| Binding `Weak` with `multiple-source-candidates-for-binary-name` | Two source-tier components both declared the same binary name | Disambiguate via operator `--pkg-alias` flag (operator precedence wins) |
| Image binary name has a non-standard extension (e.g., `.bin`) | FR-002 suffix-tolerance only covers `.exe` + `.jar` | Operator `--pkg-alias` flag |
| Auto-alias fires unexpectedly for a binary that should be `Unknown` | The image-side binary name accidentally collides with a source-tier declaration | Operator `--pkg-alias` with explicit `pkg:generic/X=pkg:cargo/Y@version` to override |

In all weird cases, milestone-111's `--pkg-alias` remains the escape hatch. The automatic path is the polished default; the operator path is the override.

## Related docs

- [`spec.md`](./spec.md) — the user-visible contract
- [`research.md`](./research.md) — the 8 implementation decisions
- [`data-model.md`](./data-model.md) — entity definitions + invariants + lifecycle
- [`contracts/property.md`](./contracts/property.md) — the `mikebom:produces-binaries` property shape contract
- [`contracts/binder.md`](./contracts/binder.md) — the binder's auto-alias derivation + provenance contract
- Milestone 111 (`specs/111-pkg-alias-binding/`) — Option A of issue #225 (operator-supplied `--pkg-alias`); this feature is Option B
- Milestone 072 (`specs/072-cross-tier-sbom-binding/`) — the underlying `--bind-to-source` flow
- Issue #225 — the motivating issue
- `docs/reference/sbom-format-mapping.md` — the canonical home for the Principle V audit citation
