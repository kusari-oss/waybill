# Research — milestone 087 Cargo workspace-member version-disambiguation

Five implementation-level decisions to pin before Phase 1 design.

## §1 — Cargo.lock dep-string format reference

**Decision**: rely on Cargo's own deterministic encoding of `[[package]] dependencies = [...]` entries.

Per [the Cargo book § The lockfile](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html) + the `cargo` source code, Cargo emits dep-strings in one of three forms:

| Form | When it's used | Example |
|---|---|---|
| `"name"` | Single version of `name` exists in the lockfile | `"serde"` |
| `"name version"` | Multiple versions of `name` exist in the lockfile | `"clap_builder 4.5.21"` |
| `"name version (source)"` | Multiple versions + multiple sources for that name+version | `"clap_builder 4.5.21 (registry+https://github.com/rust-lang/crates.io-index)"` |

The version field uses a strict `MAJOR.MINOR.PATCH[-pre][+build]` semver string. The source field uses `registry+<URL>`, `git+<URL>`, or `path+<URL>` URI schemes.

**Rationale**: Cargo's encoding is deterministic and stable across `cargo` releases (the format is part of the lockfile compatibility contract). We don't need to invent disambiguation; just preserve what Cargo writes.

## §2 — Where in the cargo reader to fix

**Decision**: fix at the source — `mikebom-cli/src/scan_fs/package_db/cargo.rs:132-141` (`package_to_entry`'s dep-list construction).

Current code:

```rust
// Dependencies are encoded as `<name>` or `<name> <version>` or
// `<name> <version> (registry+...)`. Take just the name.
let depends: Vec<String> = pkg
    .dependencies
    .iter()
    .map(|d| {
        d.split_whitespace()
            .next()
            .unwrap_or(d)
            .to_string()
    })
    .collect();
```

The comment correctly identifies the three forms — but discards the version information that disambiguates same-name multi-version cases. Replace the `split_whitespace().next()` truncation with a strip-source-suffix-only parser:

```rust
// Per Cargo.lock §1, dep-strings are "<name>", "<name> <version>",
// or "<name> <version> (<source>)". Strip only the (source) suffix;
// preserve the version when present so that downstream
// `name_to_purl` lookups can disambiguate same-name multi-version
// crates (issue #172). Cargo writes the version-suffix form ONLY
// when ambiguity exists in the lockfile, so this preserves the
// "name only" form for unambiguous deps.
let depends: Vec<String> = pkg
    .dependencies
    .iter()
    .map(|d| match d.find(" (") {
        Some(idx) => d[..idx].to_string(),  // strip " (source)" suffix
        None => d.clone(),                   // already "name" or "name version"
    })
    .collect();
```

**Rationale**: minimal change, locally readable, preserves the existing `Vec<String>` shape so no ripple to other ecosystem readers. The cargo dep-name strings now carry version when Cargo deemed it necessary.

**Alternatives considered**:

- *Promote `PackageDbEntry.depends` to `Vec<(String, Option<String>)>` (typed)*: the cleanest type-driven solution per Constitution Principle IV, but cross-ecosystem ripple (every reader's `package_to_entry` must update). Out of scope for this hot-fix-class milestone; tracked as a future cleanup follow-up.
- *Per-edge resolution at the scan_fs/mod.rs:548 edge-emission loop*: leave `depends` as name-only, add a parallel `depends_versioned` field. Doubles the data shape; requires keeping the two in sync. Rejected.

## §3 — Where in `scan_fs/mod.rs` to disambiguate

**Decision**: extend the `name_to_purl` insert loop at `scan_fs/mod.rs:373-379` to add a per-cargo-entry dual-key insert. Mirrors milestone 085's maven `groupId:artifactId` pattern at the same site.

Current code (post-milestone-085):

```rust
for e in &db_entries {
    let ecosystem = e.purl.ecosystem().to_string();
    name_to_purl.insert(
        (ecosystem.clone(), normalize_dep_name(e.purl.ecosystem(), &e.name)),
        e.purl.as_str().to_string(),
    );
    // Milestone 085 maven block (preserved)
    if ecosystem == "maven" {
        if let Some(group_id) = e.purl.namespace() {
            let gav_key = format!("{}:{}", group_id, e.name);
            name_to_purl.insert(
                (ecosystem, normalize_dep_name("maven", &gav_key)),
                e.purl.as_str().to_string(),
            );
        }
    }
}
```

Post-fix add the cargo block alongside the maven one:

```rust
// Milestone 087 — cargo entries get a "name version" disambiguation
// key so that lookups against Cargo.lock's `<name> <version>` form
// (used when multiple [[package]] blocks share a name) resolve to
// the correct same-name same-version PURL. Without this, the
// name-only key would last-write-win between e.g. clap_builder@4.5.9
// and clap_builder@4.5.21 — issue #172.
if ecosystem == "cargo" {
    let nv_key = format!("{} {}", e.name, e.version);
    name_to_purl.insert(
        (ecosystem.clone(), normalize_dep_name("cargo", &nv_key)),
        e.purl.as_str().to_string(),
    );
}
```

**Rationale**: minimal change; same shape as milestone 085's maven block; doesn't touch the existing single-key insert (so single-version cases still resolve via the name-only key).

**Alternatives considered**:

- *Replace the name-only key entirely with name-version keys*: would break the single-version lookup form (Cargo writes `"name"` for unambiguous cases). Rejected.
- *Use a different separator for cargo (e.g., `name@version`)*: the space separator matches Cargo's own format, so the dep-string from `cargo.rs:132` and the lookup-key from `mod.rs:373` use the same shape. No translation needed.

## §4 — `normalize_dep_name` interaction

**Decision**: `normalize_dep_name` for cargo currently does `name.to_lowercase()`. Applied to `"clap_builder 4.5.21"` it produces `"clap_builder 4.5.21"` (no uppercase chars, idempotent). Apply uniformly to both the dep-string from the `package_to_entry`-side AND the lookup-key from the `mod.rs`-side; both end up at the same lowercased form. Lookup hits.

**Rationale**: nothing to change. The version part contains digits + dots + optional `-pre+build` tokens, none of which `to_lowercase()` modifies. The cargo crate-name part is conventionally already lowercase + underscores. Tests exist in `mod tests` of `cargo.rs` for the existing reader; they continue to pass.

**Alternatives considered**:

- *Cargo-specific normalize that strips the version before lowercasing*: rejected — adds complexity, doesn't materially change behavior.

## §5 — Regression test baseline bump

**Decision**: bump `mikebom-cli/tests/transitive_parity_cargo.rs`'s `EXPECTED_MIKEBOM_EDGE_COUNT` (currently 319) + the workspace-internal `EXPECTED_REPRESENTATIVE_EDGES` entries to reflect the post-087 correct edge resolution. Also update `specs/083-transitive-correctness/research.md §8 — Ecosystem: cargo`'s "Specific gaps surfaced (mikebom-side)" list to mark gap #1 closed; gap #2 (clap_derive zero outgoing edges, issue #173) remains.

The post-087 edge count delta is the number of mikebom-only edges that resolved to wrong-version targets and now resolve to the correct ones. Pre-fix: 56 mikebom-only edges (per audit row). Post-fix: most should disappear (becoming agreement edges with trivy + syft). Exact post-087 numbers are recorded during T-task execution.

**Rationale**: the regression test is designed exactly for this maintainer workflow — quickstart Recipe 3. The bump is a deliberate output-shape change, audited via the diff invariant in spec FR-009.

**Alternatives considered**:

- *Suppress the test failure with an `#[ignore]`*: rejected — defeats the purpose of the regression test.
- *Add a separate post-087 baseline file*: rejected — milestone-083's pattern is single-baseline-pinned-per-ecosystem; bumping it is the maintained workflow.

## §6 — Pre-PR gate verification protocol

**Decision**: standard CLAUDE.md pre-PR sequence applies:

```bash
./scripts/pre-pr.sh
```

Plus milestone-specific verification:

1. New `transitive_parity_cargo` baseline passes against post-087 mikebom: `cargo +stable test -p mikebom --test transitive_parity_cargo`.
2. Cargo CDX/SPDX goldens regenerated cleanly: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression cdx_regression_cargo` etc.
3. Other ecosystems' goldens stay byte-identical: standard `cdx_regression`, `spdx_regression`, `spdx3_regression` runs.
4. Closure-invariant test (milestone 084) passes for cargo: `cargo +stable test -p mikebom --test cdx_ref_closure_invariant`.
5. Diff scope audit on cargo goldens — only dep-edge version strings change; no other fields drift.

**Rationale**: standard project workflow + milestone-specific golden audit. The diff-shape audit catches over-correction (e.g., if the fix accidentally also affected component identity or scope classification).
