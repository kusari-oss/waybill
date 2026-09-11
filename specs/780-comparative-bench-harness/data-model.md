# Phase 1 Data Model: Comparative benchmark harness

**Feature**: 780-comparative-bench-harness

All types live in `xtask/src/compare/`. Nothing here is exported to the
waybill binary; this is measurement scaffolding.

## `ToolSpec` — operator-supplied, never committed

```rust
/// One tool to measure. Read from `tools.local.toml`, which is gitignored.
/// The committed example uses placeholder names so the format is documented
/// without this repository naming any specific tool (FR-015).
struct ToolSpec {
    /// Operator's label. Appears in output. Free text.
    id: String,
    /// Executable plus arguments. `{target}` and `{out}` are substituted.
    argv: Vec<String>,
    /// Environment overrides for this invocation.
    env: BTreeMap<String, String>,
    /// Whether this invocation contacts a third-party service. Drives the
    /// authoritative/indicative split (FR-002a).
    network: NetworkMode,
    /// How to obtain the tool's version string, for FR-005.
    version_argv: Vec<String>,
}

enum NetworkMode { Offline, Enriched }
```

## `Target` and `TruthSet`

```rust
struct Target {
    name: String,
    /// Fetched by the m770 pinned-SHA fetcher.
    repo: String,
    sha: String,
    ecosystem: String,
    truth: Option<TruthSpec>,
}

/// Declared per target (FR-008a). Never inferred.
struct TruthSpec {
    method: TruthMethod,
}

enum TruthMethod {
    /// Union of every go.sum in the tree. Always available offline, but a
    /// SUPERSET of what is built: go.sum carries hashes for modules merely
    /// considered during resolution. Scoring against it penalises a tool for
    /// correctly omitting unused modules (research.md R5).
    GoSumUnion,
    /// Union of every go.mod require. Closer, still not the built set.
    GoModRequires,
    /// Fixture with an exact hand-authored expected set. Used by the
    /// self-check and by any target where truth is known by construction.
    DeclaredExact,
}

impl TruthMethod {
    /// FR-008b — a superset must be labelled, because it changes what a
    /// precision score means.
    fn is_superset(&self) -> bool {
        matches!(self, Self::GoSumUnion | Self::GoModRequires)
    }
}
```

## `PackageIdentity` — the reduction rule

```rust
/// FR-006. Full package URL INCLUDING version, normalised so cosmetic
/// differences between tools do not register as different packages.
///
/// Normalisation is hand-rolled here rather than reusing the library's PURL
/// type on purpose (research.md R4): an instrument that shares code with its
/// subject would apply the same bug to every tool and to its own self-check.
#[derive(PartialEq, Eq, Hash, Ord, PartialOrd)]
struct PackageIdentity {
    ptype: String,     // lowercased
    namespace: String, // normalised, may be empty
    name: String,
    version: String,   // RETAINED — foo@1.0 and foo@2.0 are two identities
}
```

Qualifiers and subpath are discarded. Version is kept: the duplication that
motivated the feature repeated the same name *and* version, so keeping
version removes it without merging genuinely distinct findings.

## `Measurement`

```rust
struct Measurement {
    tool_id: String,
    target: String,
    mode: NetworkMode,
    outcome: Outcome,
    /// Every repeat, in execution order. Interleaved with other tools
    /// (FR-001a), so ordering is meaningful for drift analysis.
    wall_secs: Vec<f64>,
    peak_rss_mb: u64,
    /// Reduced per FR-006.
    distinct_packages: usize,
    /// FR-006a — the count reduced FROM. A large gap means the tool emits
    /// duplicates, which is a fact worth surfacing, not noise to hide.
    raw_components: usize,
    /// FR-007 — never folded into distinct_packages.
    identityless_components: usize,
    accuracy: Option<Accuracy>,
}

enum Outcome { Ok, Failed { code: i32 }, TimedOut, Unparseable, ToolAbsent }

/// Only present when the target declared a truth set.
struct Accuracy {
    method: TruthMethod,
    truth_size: usize,
    found: usize,          // in truth AND reported
    missed: usize,         // in truth, not reported
    extra: usize,          // reported, not in truth
    truth_is_superset: bool,
}
```

## `Verdict`

```rust
/// FR-002/FR-004/FR-017. The harness states whether its preconditions held.
/// It never states that a tool is better.
enum Verdict {
    /// Preconditions met; figures may be compared.
    Comparable,
    /// Figures recorded, comparison withheld. Reasons are cumulative.
    Withheld { reasons: Vec<WithheldReason> },
}

enum WithheldReason {
    HostNotReferenceClass { class: String },
    /// FR-005. Comparing figures produced by different tool versions or
    /// against different target revisions measures the difference between
    /// the inputs, not between the tools.
    ToolVersionMismatch { tool: String, then: String, now: String },
    TargetRevisionMismatch { target: String, then: String, now: String },
    TimingSpreadExceeded { tool: String, ratio: f64, limit: f64 },
    CoverageNotReproducible { tool: String, first: usize, second: usize },
    TruthMethodMismatch { a: TruthMethod, b: TruthMethod },
    ModeMismatch { a: NetworkMode, b: NetworkMode },
    ToolFailed { tool: String },
    SelfCheckFailed { detail: String },
}
```

`Withheld` carries every reason rather than the first, so one run tells the
operator everything that needs fixing.

## `ComparisonRun`

```rust
struct ComparisonRun {
    schema_version: u32,
    started_at: DateTime<Utc>,
    /// Reused from `xtask::bench`. "Reference class" throughout the spec
    /// means `NoiseClass::Reference`.
    host_class: NoiseClass,
    runner_uname: String,
    tool_versions: BTreeMap<String, String>,  // FR-005
    self_check: SelfCheckResult,
    measurements: Vec<Measurement>,
    verdict: Verdict,
}
```

Written only to `target/compare/` (FR-014). No documentation output, no
committed report.
