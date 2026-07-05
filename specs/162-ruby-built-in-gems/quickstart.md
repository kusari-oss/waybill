# Quickstart: Milestone 162 (Ruby built-in gem edges)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Contributor onboarding for milestone 162. Assumes a working mikebom dev environment (per top-level `CLAUDE.md`).

## 1. Prerequisites

- Rust stable toolchain (workspace-managed).
- No `ruby` binary required (mikebom uses static allowlist per Q2 — no runtime probe).

Verify:

```bash
cargo +stable --version                       # expect: cargo 1.75+
```

## 2. Implementation overview

Unlike milestones 160 + 161, milestone 162 needs NO empirical investigation. The fix is fully specified:

1. Add `RUBY_BUILT_IN_GEMS: &[&str]` const to `gem.rs` per data-model.md E1.
2. Change `GemSpec.depends: Vec<String>` → `Vec<GemDep>` where `GemDep` carries name + requirement string per data-model.md E2. Update the parser at `gem.rs::parse_gemfile_lock` to populate the requirement clause.
3. Add `SyntheticGemKind::RubyBuiltIn` enum with `as_wire_str()` per data-model.md E3.
4. Add `append_synthetic_built_in_gems()` helper per data-model.md E4, called from `read()` after the per-spec loop.
5. Register C113/C114 in the parity catalog per contracts/annotations.md.
6. Add C113/C114 rows to `docs/reference/sbom-format-mapping.md`.

## 3. Parser change (T005-ish — most delicate step)

Current parser at `gem.rs` line ~256:

```rust
// specs block (indent 6): dep-name plus optional version clause
if indent == 6 {
    let dep_name = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    // version constraints stripped
    if let Some(spec) = doc.specs.last_mut() {
        spec.depends.push(dep_name);
    }
}
```

Change to preserve the constraint clause:

```rust
if indent == 6 {
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let dep_name = parts.next().unwrap_or("").to_string();
    // The rest is the parenthesized requirement clause, e.g., "(>= 1.2.0)"
    let raw_req = parts.next().unwrap_or("").trim();
    let requirement = raw_req
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(raw_req)
        .to_string();
    if let Some(spec) = doc.specs.last_mut() {
        spec.depends.push(GemDep { name: dep_name, requirement });
    }
}
```

Downstream code that reads `spec.depends` for edge construction adapts:

```rust
// Before
for dep_name in &spec.depends { ... }

// After
for dep in &spec.depends {
    let dep_name = &dep.name;
    ...
}
```

The `spec_to_entry` function's `entry.depends` population needs the `.name` extraction.

## 4. Synthetic emission

Insertion point in `gem::read()` — after the per-spec emission loop, before the gemspec walk:

```rust
for lock_path in find_gemfile_locks(rootfs, exclude_set) {
    // ... existing per-spec emission loop populating `out` ...

    // Milestone 162: append synthetic built-in components
    let emitted_names: HashSet<String> = out
        .iter()
        .filter(|e| e.purl.as_str().starts_with("pkg:gem/"))
        .map(|e| e.name.clone())
        .collect();
    append_synthetic_built_in_gems(&mut out, &emitted_names, &source_path, &doc.specs);
}
```

Note that `emitted_names` should be scoped to this Gemfile.lock's iteration OR track all previously-emitted gem names across Gemfile.lock files in the scan (safer choice — mirrors the existing `seen_purls` dedup).

## 5. Parity catalog registration

Update 4 files in one atomic commit:

- `mikebom-cli/src/parity/extractors/cdx.rs` — add C113/C114 `cdx_anno!` invocations.
- `mikebom-cli/src/parity/extractors/spdx2.rs` — add C113/C114 `spdx23_anno!` invocations.
- `mikebom-cli/src/parity/extractors/spdx3.rs` — add C113/C114 `spdx3_anno!` invocations.
- `mikebom-cli/src/parity/extractors/mod.rs` — add 2 `ParityExtractor` registration entries + import lines.

Also update `docs/reference/sbom-format-mapping.md` with C113/C114 rows.

## 6. Testing

```bash
# Full pre-PR gate
./scripts/pre-pr.sh

# Unit tests only
cargo +stable test --bin mikebom scan_fs::package_db::gem::tests

# Integration test
cargo +stable test --test ruby_built_in_gems
```

## 7. Debugging: tracing recipes

```bash
# Detect synthetic built-in emissions in a scan
RUST_LOG=mikebom_cli::scan_fs::package_db::gem=info \
    mikebom sbom scan --path <fixture> 2>&1 \
    | grep 'built-in'
```

## 8. Common pitfalls

- **Emitting synthetic when a real GEM/specs entry exists** (FR-004 violation): guard via `emitted_names.contains(&dep.name)` check. Test T029g covers this.
- **Emitting a version segment on synthetic PURL**: FR-003 mandates versionless PURL. Test T029d covers this.
- **Multi-source requirement collision** (R4): union into JSON array. Test T029h covers this.
- **Requirement clause parenthesis handling** (`(>= 1.2.0)` vs `>= 1.2.0`): the parser strips the parens; the annotation value is the raw string without parens.

## 9. Verify SC-002 spot-check

Post-fix, `bundler-audit@0.9.3 → bundler` MUST appear:

```bash
# Synthesize the fixture (or use test-rails cache)
cargo test --test ruby_built_in_gems

# Or manually against test-rails:
mikebom sbom scan --path test-rails --format cyclonedx-json \
    --output cyclonedx-json=/tmp/out.cdx.json
jq '.dependencies[]
    | select(.ref | contains("bundler-audit"))
    | .dependsOn[]' /tmp/out.cdx.json \
    | grep bundler
# Expected: at least one `pkg:gem/bundler` line (versionless)
```
