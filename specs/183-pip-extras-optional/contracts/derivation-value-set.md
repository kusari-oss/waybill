# Contract: C122 Derivation-Value Set (m183 update)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Research**: [../research.md](../research.md)

## Scope

Documents the expected value-set for the `mikebom:optional-derivation` annotation (C122 parity catalog row) after m183. This is a documentation contract — the C122 extractor is `Directionality::SymmetricEqual` and does NOT validate against a fixed value-set at runtime; the catalog docstring at `mikebom-cli/src/parity/extractors/cdx.rs:866` is the sole source of truth for expected values.

## Value Set (after m183)

| Value | Milestone | Emitted by | Semantic |
|---|---|---|---|
| `cargo-optional-true` | m179 | `cargo.rs::build_cargo_component` | Cargo `[dependencies].foo = { optional = true }` |
| `npm-optional-dependencies` | m180 + m181 | `npm/package_lock.rs`, `npm/pnpm_lock.rs`, `npm/yarn_lock.rs` (v1 + Berry) | npm-registry family `optionalDependencies` sub-block / `dependenciesMeta.<name>.optional` |
| **`pip-optional-dependencies`** ✱ | m183 | `pip/poetry.rs`, `pip/mod.rs` (main-module post-pass), `pip/uv_lock.rs` | pip-family: poetry.lock `optional = true`, pyproject `[project.optional-dependencies]`, uv.lock `optional-dependencies` |

✱ = added by m183. Docstring update required at `cdx.rs:866` from placeholder `pip-extras-require` to `pip-optional-dependencies`.

## Invariants

1. **Exactly one value per ecosystem family** — Cargo has one, npm-family has one, pip-family has one. No per-manager values (no `poetry-optional`, `uv-optional-dependencies`, etc.). The specific manifest is captured by `evidence.source_file_paths`, not the derivation value.
2. **No component carries multiple derivations** — Enforced by the "one derivation per component" invariant (Decision 2). If a component is classified by two different sources (e.g., both poetry.lock AND pyproject.toml), lockfile-precedence (Decision 3) picks one.
3. **Values are stable identifiers** — Once documented in this value-set, a value MUST NOT be renamed. Downstream SBOM consumers (pico, other analyzers) grep for these exact strings.
4. **Docstring lists all values** — The `cdx.rs:866` docstring is the sole source of truth; it MUST list every value the mikebom codebase can emit. Adding a fourth flavor in a future milestone requires updating the docstring + this document + the C122 extractor row's comment.

## Emitted Format Byte-Identity

The C122 extractor at `mikebom-cli/src/parity/extractors/mod.rs:545` is registered as `Directionality::SymmetricEqual`. This means:

- **CDX 1.6**: emitted as `properties[].value` on the component, key `mikebom:optional-derivation`.
- **SPDX 2.3**: emitted as an `annotationText` in a `PackageAnnotation` on the component, discoverable by string-search for `"mikebom:optional-derivation"`.
- **SPDX 3.0.1**: emitted as `elements[].extension[]` with the same annotation-key.

For any pip-family fixture, the three emitted formats MUST contain the exact string `"pip-optional-dependencies"` byte-identically. C122 SC-009 validates this automatically.

## Docstring Update (m183 implementation task)

**File**: `mikebom-cli/src/parity/extractors/cdx.rs`
**Line**: ~866 (the C122 docstring block)

**Diff shape**:
```rust
// C122 — `mikebom:optional-derivation` (milestone 179). Records
// which ecosystem construct produced the component's
// `LifecycleScope::Optional` classification (`cargo-optional-true`,
-// `npm-optional-dependencies`, `pip-extras-require`,
+// `npm-optional-dependencies`, `pip-optional-dependencies`,
// ...
```

Zero runtime behavior change. Purely a documentation-fix folded into the m183 commit.
