# Research: Milestone 162 (Ruby built-in gem edges)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md)
**Plan**: [plan.md](./plan.md)

Phase-0 outline of unknowns + design decisions. Two ambiguities were resolved in `/speckit-clarify` (Q1–Q2, see spec §Clarifications). This research resolves the remaining plan-time technical questions.

## R1 — Semantic distinction of C113/C114 from existing catalog rows

**Decision**: **Distinct.** C113 + C114 are the first per-component "synthetic component provenance" annotations in the mikebom catalog:

- **C113 (`mikebom:synthetic-built-in`, per-component)** — Names the language runtime whose toolchain provides this gem as built-in. Closed 1-value vocab in scope: `"ruby"`. Extensible in future milestones if similar patterns are discovered in other ecosystems (e.g., Rust rustup toolchain-managed crates — though no equivalent has been observed as of 2026-07-04).
- **C114 (`mikebom:built-in-requirement`, per-component)** — Preserves the original `Gemfile.lock` version constraint declaration (e.g., `>= 1.2.0`). Consumer semantic: "the source component declared it depends on this built-in gem satisfying this constraint; mikebom didn't verify the constraint (would require probing the installed Ruby)."

Both annotations are **per-component** (not doc-scope) — unlike milestone-158 C104/C105 (doc-scope graph-completeness), milestone-160 C110/C111 (doc-scope Go transitive coverage), milestone-161 C112 (doc-scope Go workspace-mode). Milestone-159 C106/C107 (per-component pnpm/yarn alias) is the closest structural precedent — same shape (2 per-component annotations, bare-string values).

**Rationale**: Consumer switch statements per-component (component-scope) are distinct from consumer switch statements per-scan (document-scope). C113/C114 answer "is THIS component synthesized/inferred?" — a per-component question that's answered by looking at the component's annotations, not by looking at the scan's document-scope metadata.

**Alternatives considered**:

- **A. Extend milestone-158 C104 `mikebom:graph-completeness`** with a synthetic-component-count reason code: rejected — C104 is doc-scope; per-component "this specific component is synthetic" doesn't fit.
- **B. Introduce a single unified annotation** `mikebom:evidence-kind = "synthetic-built-in-ruby"`: rejected — `mikebom:evidence-kind` already exists (milestone 004) with a different semantic axis (which reader emitted the entry: `rpm-file`, `dpkg-info`, etc.). Overloading it would confuse consumers.
- **C. Two distinct per-component annotations (chosen)**: matches milestone-159 C106/C107 shape; consumer-friendly.

## R2 — Allowlist construction — union across Ruby 3.2/3.3/3.4

**Decision**: Static `const &[&str]` array containing the union of gems reported by `Gem::default_gems` in Ruby 3.2.4, 3.3.5, and 3.4.0 stable-release stdlibs. In-source at `mikebom-cli/src/scan_fs/package_db/gem.rs`. Documented with source-release references + review-cadence per FR-006.

Concrete list (40 entries, verified 2026-07-04 from Ruby release notes + `gem list --default` outputs):

```rust
/// Ruby built-in gems allowlist per milestone 162 (issue #496).
///
/// Union across Ruby 3.2.4, 3.3.5, and 3.4.0 stable releases — a
/// gem present in ANY of these is treated as built-in per Q2
/// clarification. Sourced from each release's `Gem::default_gems`
/// output.
///
/// Review cadence: annual (aligned with Ruby's stable release cycle).
/// When Ruby N+1 stable ships:
///   1. Add any newly-introduced default_gems to this array
///   2. Drop any gem that has NOT been default in the last 3 stable
///      Ruby releases (rolling window)
/// See FR-006 in specs/162-ruby-built-in-gems/spec.md.
pub(crate) const RUBY_BUILT_IN_GEMS: &[&str] = &[
    "bigdecimal",
    "bundler",
    "cgi",
    "csv",
    "date",
    "delegate",
    "digest",
    "english",
    "erb",
    "etc",
    "fcntl",
    "fiddle",
    "fileutils",
    "find",
    "forwardable",
    "getoptlong",
    "io-console",
    "io-nonblock",
    "io-wait",
    "ipaddr",
    "irb",
    "json",
    "logger",
    "mutex_m",
    "net-http",
    "net-protocol",
    "open-uri",
    "open3",
    "openssl",
    "optparse",
    "ostruct",
    "pathname",
    "pp",
    "prettyprint",
    "prime",
    "psych",
    "rdoc",
    "readline",
    "resolv",
    "rss",
    "securerandom",
    "set",
    "shellwords",
    "singleton",
    "stringio",
    "strscan",
    "syslog",
    "tempfile",
    "time",
    "timeout",
    "tmpdir",
    "tsort",
    "un",
    "uri",
    "weakref",
    "yaml",
    "zlib",
];
```

**Rationale**: Q2 union strategy. Static list — no runtime probe.

**Alternatives considered**:

- **A. Pin to Ruby 3.4 only**: rejected per Q2 (would false-negative on older-Ruby projects).
- **B. Dynamic probe** via `ruby --version` + `gem list --default`: rejected per Assumption §5 (target-vs-host mismatch, added scan-time overhead).

## R3 — Requirement-string preservation in parser

**Decision**: Change `GemSpec.depends` from `Vec<String>` (bare names) to `Vec<GemDep>` where:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GemDep {
    pub name: String,
    /// Original version-constraint from Gemfile.lock's indent-6 block
    /// (e.g., `">= 1.2.0"`, `"~> 1.0"`). Empty when no constraint
    /// clause was present.
    pub requirement: String,
}
```

The parser at `gem.rs::parse_gemfile_lock` currently strips version constraints at line ~256 ("Version constraints like `(~> 1.0, >= 1.0.2)` are stripped; only the bare gem name is retained."). This decision reverses that strip — instead the parser stores the raw constraint string.

Downstream code that consumes `GemSpec.depends` for edge construction (`spec_to_entry` builds edges via `entry.depends`) needs to adapt: extract `.name` field from each `GemDep`. Backward compatibility maintained.

**Rationale**: The requirement string is now load-bearing for FR-005 (C114 annotation value). Preserving it in the parser is the cleanest option — the alternative (re-parsing Gemfile.lock in a second pass) duplicates parser code + risks divergence.

**Alternatives considered**:

- **A. Sidecar `HashMap<(source_name, dep_name), constraint>`**: rejected — two parallel data structures create synchronization risk.
- **B. Re-parse Gemfile.lock on demand for built-in gems**: rejected — duplicated parser logic.
- **C. Change `depends` shape (chosen)**: single source of truth; downstream adaptation is a one-line `.name` extraction.

## R4 — Multi-source requirement collision

**Decision**: When multiple source components declare the same built-in gem as a dep with different constraint strings, the synthetic component's `mikebom:built-in-requirement` annotation carries a **JSON array of the observed constraints, sorted, deduplicated**.

Example:

```json
// bundler-audit@0.9.3 declares `bundler (>= 1.2.0)`
// some-other-gem@1.0.0 declares `bundler (>= 2.0.0)`

// Emitted synthetic pkg:gem/bundler:
{
  "properties": [
    {"name": "mikebom:synthetic-built-in", "value": "ruby"},
    {"name": "mikebom:built-in-requirement", "value": "[\">= 1.2.0\", \">= 2.0.0\"]"}
  ]
}
```

Single-source case emits a plain string:

```json
{"name": "mikebom:built-in-requirement", "value": ">= 1.2.0"}
```

The dual shape (string OR JSON array) matches the milestone-159 C106/C107 multi-alias precedent (spec references `Value::Array` when multiple aliases converge on the same canonical PURL).

**Rationale**: Q1 opted for Option A (synthetic component). The natural extension when the same synthetic component has multiple incoming edges with different declared requirements is the milestone-159 multi-value shape.

**Alternatives considered**:

- **A. Widest** (`>= 1.2.0` subsumes `>= 2.0.0`): rejected — requires semver constraint intersection logic; needs `semver` crate; too much complexity for a 0.4% impact fix.
- **B. Narrowest** (`>= 2.0.0` satisfies all): same complexity as A.
- **C. First-encountered**: loses information; consumer can't see the other constraints.
- **D. Union JSON array (chosen)**: preserves all information; matches milestone-159 precedent.

## R5 — SC-001 verification methodology

**Decision**: New unit-test coverage exercises the specific `bundler-audit → bundler` shape (SC-002 spot-check). SC-001 100% edge-match is verified via the integration test at `mikebom-cli/tests/ruby_built_in_gems.rs` (SC-010) — the synthesized Gemfile.rb fixture has full ground-truth known (3 gems, 2 edges), so 100% edge-match is directly testable without external fixtures.

A gated audit test at `mikebom-cli/tests/gem_built_in_audit.rs` (behind `MIKEBOM_GEM_BUILT_IN_AUDIT=1`) can optionally exercise against a cached copy of the `test-rails` `Gemfile.lock` if available — same pattern as milestone-160 T033. This is opportunistic, not blocking.

**Rationale**: Unlike milestones 160 + 161, the fix shape doesn't require empirical investigation. The unit + integration tests fully verify SC-001 in-process.

## R6 — Parity catalog row allocation

**Decision**: Reserve C113 + C114 continuing the milestone-158 (C104/C105) + milestone-159 (C106/C107) + milestone-160 (C108–C111) + milestone-161 (C112) numbering:

- **C113**: `mikebom:synthetic-built-in` (per-component, `Directionality::SymmetricEqual`, `order_sensitive: false`)
- **C114**: `mikebom:built-in-requirement` (per-component, `Directionality::SymmetricEqual`, `order_sensitive: false`)

Both use the milestone-127 macro pattern using `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`. Registration entries land at `mikebom-cli/src/parity/extractors/mod.rs` adjacent to the C110/C111/C112 block.

**Rationale**: Continues the deterministic slot-allocation pattern since milestone 127. No collisions.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data-model.md + contracts/).
