# Data Model: Milestone 162 (Ruby built-in gem edges)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase-1 entity + type inventory. All entities are Rust types in `mikebom-cli/src/scan_fs/package_db/gem.rs` unless otherwise noted; wire-shape entities are per-format JSON constructs described in `contracts/annotations.md`.

## Rust types

### E1 — `RUBY_BUILT_IN_GEMS` (NEW const array)

**Location**: NEW module-level `const &[&str]` in `mikebom-cli/src/scan_fs/package_db/gem.rs`

```rust
pub(crate) const RUBY_BUILT_IN_GEMS: &[&str] = &[
    "bigdecimal", "bundler", "cgi", "csv", "date", "delegate", "digest",
    "english", "erb", "etc", "fcntl", "fiddle", "fileutils", "find",
    "forwardable", "getoptlong", "io-console", "io-nonblock", "io-wait",
    "ipaddr", "irb", "json", "logger", "mutex_m", "net-http",
    "net-protocol", "open-uri", "open3", "openssl", "optparse",
    "ostruct", "pathname", "pp", "prettyprint", "prime", "psych",
    "rdoc", "readline", "resolv", "rss", "securerandom", "set",
    "shellwords", "singleton", "stringio", "strscan", "syslog",
    "tempfile", "time", "timeout", "tmpdir", "tsort", "un", "uri",
    "weakref", "yaml", "zlib",
];
```

Per Q2: **union of Ruby 3.2.4, 3.3.5, 3.4.0** stable-release `Gem::default_gems` outputs. Alphabetical order (stable for git-diff review; not semantic).

**Companion helper**:

```rust
/// True iff `name` is a Ruby toolchain-provided built-in gem per the
/// FR-001 allowlist. O(N) linear scan — N=57 as of 2026-07-04.
/// Callers may reasonably assume sub-microsecond cost per lookup.
pub(crate) fn is_ruby_built_in_gem(name: &str) -> bool {
    RUBY_BUILT_IN_GEMS.iter().any(|&g| g == name)
}
```

Not a `HashSet` because the array is small (57 entries) and the lookup happens ~O(gems × avg-deps) per scan — for typical Ruby projects (~150 gems × ~5 deps × 57-way scan) the total cost is ~40k comparisons per scan, sub-millisecond. Preserves the `const` shape (usable at compile time if ever needed).

### E2 — `GemDep` (NEW struct; EXTENDS `GemSpec.depends` type)

**Location**: NEW type in `mikebom-cli/src/scan_fs/package_db/gem.rs`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GemDep {
    pub name: String,
    /// Original version-constraint clause from Gemfile.lock's indent-6
    /// block (e.g., `">= 1.2.0"`, `"~> 1.0"`). Empty when no clause
    /// was present. Load-bearing for FR-005 (C114 annotation value).
    pub requirement: String,
}
```

**Change to existing `GemSpec`**: replaces `pub depends: Vec<String>` with `pub depends: Vec<GemDep>`. Downstream code that consumed the old `Vec<String>` (edge construction in `spec_to_entry`) adapts to `.iter().map(|d| &d.name)`.

**Validation rules**: `name` is a valid Ruby gem name (matches the parser's existing regex). `requirement` is a raw copy of the parenthesized clause from Gemfile.lock — no normalization.

### E3 — `SyntheticGemKind` (NEW enum)

**Location**: NEW type in `mikebom-cli/src/scan_fs/package_db/gem.rs`

```rust
/// Language runtime whose toolchain provides this gem as built-in.
/// Serialized to the C113 annotation value.
///
/// Closed 1-variant vocab in scope for milestone 162; extensible in
/// future milestones if similar patterns are discovered in other
/// ecosystems (e.g., Rust toolchain-managed crates).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyntheticGemKind {
    RubyBuiltIn,
}

impl SyntheticGemKind {
    pub(crate) fn as_wire_str(&self) -> &'static str {
        match self {
            Self::RubyBuiltIn => "ruby",
        }
    }
}
```

**Fields**: single variant. **Relationships**: consumed by the synthetic-emission builder (below); serialized as the C113 annotation value.

**Validation rules**: closed vocab. Extension requires a spec-milestone bump per FR-006 governance precedent.

### E4 — Synthetic emission logic (NEW helper function)

**Location**: NEW function in `mikebom-cli/src/scan_fs/package_db/gem.rs`, called from `read()` after the per-spec emission loop.

```rust
/// Milestone 162: emit synthetic components for Ruby built-in gems
/// referenced as dep targets but not present in GEM/specs.
///
/// - `out` — the already-populated `Vec<PackageDbEntry>` from the
///   per-spec emission loop. Mutated in-place to append synthetic
///   entries. Also read to enforce FR-004 real-gem-precedence.
/// - `emitted_names` — set of gem names already in `out` (indexed
///   for O(1) FR-004 collision check).
/// - `source_path` — the Gemfile.lock path (for evidence provenance).
/// - `specs` — the parsed `GemfileLockDocument.specs` (for iterating
///   deps).
fn append_synthetic_built_in_gems(
    out: &mut Vec<PackageDbEntry>,
    emitted_names: &HashSet<String>,
    source_path: &str,
    specs: &[GemSpec],
) {
    // Collect (name, [requirements]) tuples for each built-in gem
    // referenced as a dep-target across all specs. Multi-source case
    // → union of requirement strings (deduplicated + sorted).
    let mut built_in_refs: BTreeMap<String, BTreeSet<String>> =
        BTreeMap::new();

    for spec in specs {
        for dep in &spec.depends {
            if !is_ruby_built_in_gem(&dep.name) {
                continue;
            }
            if emitted_names.contains(&dep.name) {
                // FR-004: real gem takes precedence over synthetic.
                continue;
            }
            let requirements =
                built_in_refs.entry(dep.name.clone()).or_default();
            if !dep.requirement.is_empty() {
                requirements.insert(dep.requirement.clone());
            }
        }
    }

    for (name, requirements) in built_in_refs {
        // FR-003 versionless PURL: pkg:gem/<name> — no @version.
        let Some(purl) = Purl::new(&format!(
            "pkg:gem/{}",
            encode_purl_segment(&name),
        ))
        .ok()
        else {
            continue;
        };

        let mut entry = PackageDbEntry {
            // ... boilerplate matching existing spec_to_entry shape ...
            purl,
            name: name.clone(),
            version: String::new(), // versionless
            source_path: source_path.to_string(),
            ..default_package_db_entry()
        };

        entry.extra_annotations.insert(
            "mikebom:synthetic-built-in".to_string(),
            serde_json::Value::String(
                SyntheticGemKind::RubyBuiltIn.as_wire_str().to_string(),
            ),
        );
        // FR-005 + R4: single requirement → plain string; multiple
        // requirements → JSON array of sorted, deduplicated strings.
        if !requirements.is_empty() {
            let value = match requirements.len() {
                1 => serde_json::Value::String(
                    requirements.into_iter().next().unwrap(),
                ),
                _ => serde_json::Value::Array(
                    requirements
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            };
            entry.extra_annotations.insert(
                "mikebom:built-in-requirement".to_string(),
                value,
            );
        }

        out.push(entry);
    }
}
```

**Fields**: pure function; no state. **Relationships**: read from `specs` + `emitted_names`; write into `out`.

**Validation rules**:
- Enforces FR-002 (only allowlist gems get synthetic entries).
- Enforces FR-004 (real GEM/specs entries take precedence).
- Enforces FR-003 (versionless PURL — empty `version` string + no `@version` PURL segment).
- Enforces FR-005 (requirement annotation emission with R4 multi-source union).
- Preserves FR-007 (edges from source specs to built-in names remain in `entry.depends`; the emitted synthetic entry means the graph resolver will find the target at dedup time and preserve the edge).

## Wire types

### W1 — `mikebom:synthetic-built-in` (C113, per-component)

**Wire format**: raw string `"ruby"` (closed 1-value vocab in scope).

**Universality**: Emitted iff the component is a synthetic Ruby built-in gem (versionless `pkg:gem/*` PURL emitted per E4). NEVER emitted on non-synthetic gem components.

**Per-format shape**: see `contracts/annotations.md` §C113.

### W2 — `mikebom:built-in-requirement` (C114, per-component)

**Wire format**:
- **Single source**: raw string of the requirement clause (e.g., `">= 1.2.0"`).
- **Multiple sources with different constraints**: JSON array of sorted+deduplicated strings (e.g., `["\">= 1.2.0\"", "\">= 2.0.0\""]`).
- **No requirement clause** at any source: annotation NOT emitted (only C113 is emitted).

**Universality**: Emitted iff (a) the component is a synthetic Ruby built-in gem AND (b) at least one source spec's dep-declaration had a non-empty requirement clause.

**Per-format shape**: see `contracts/annotations.md` §C114.

## Relationships

```text
gem::read()
     │
     ├── existing per-spec emission loop → populates `out: Vec<PackageDbEntry>`
     │                                     with real GEM/specs entries
     │
     └── NEW: append_synthetic_built_in_gems(&mut out, ...)
              │
              ├── walks `specs[*].depends[*].name`
              │
              ├── filters via is_ruby_built_in_gem() (E1 lookup)
              │
              ├── skips names already in `out` (FR-004)
              │
              ├── unions requirement strings across sources (R4)
              │
              └── appends synthetic PackageDbEntry with:
                      - purl = versionless `pkg:gem/<name>` (E4)
                      - extra_annotations["mikebom:synthetic-built-in"] = "ruby"
                      - extra_annotations["mikebom:built-in-requirement"] = <string|array>
```

## State transitions

**Synthetic emission** is idempotent: same inputs (same allowlist + same parsed Gemfile.lock) produce same output.

## Data volume assumptions

- **Allowlist size**: 57 gem names as of 2026-07-04 (Ruby 3.2/3.3/3.4 union).
- **Synthetic components per scan**: bounded by allowlist size. For `test-rails`: 1 synthetic (`bundler`). For a Ruby monorepo referencing many built-ins: bounded by ~40-50.
- **Requirement-string length**: bounded (`>= 1.2.0` is typical; even complex clauses like `[">= 1.2.0", "< 3.0"]` are ~30 chars). No unbounded growth.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| Only allowlist gems become synthetic | Guarded by `is_ruby_built_in_gem()` check in E4. |
| Real gems take precedence over synthetic (FR-004) | Guarded by `emitted_names.contains(&dep.name)` check in E4. |
| Synthetic PURL is versionless (FR-003) | Enforced by construction: `format!("pkg:gem/{}", encode_purl_segment(name))` — no `@version` segment. Verified by unit test T029d. |
| C113 emitted iff synthetic + non-synthetic gems never carry it | Enforced by construction: only `append_synthetic_built_in_gems` adds the annotation. Verified by unit test T029e. |
| C114 emitted iff synthetic + at least one requirement clause | Enforced by construction: `if !requirements.is_empty()`. Verified by unit test T029f. |
| C113 value closed to `"ruby"` for milestone 162 | Enforced by `SyntheticGemKind::as_wire_str()` returning `"ruby"` for the single variant. |
