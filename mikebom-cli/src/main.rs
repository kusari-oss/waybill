// Milestone 003 T012: production code MUST NOT use `.unwrap()` — see
// `.specify/memory/constitution.md` Principle IV and
// `specs/003-multi-ecosystem-expansion/research.md` R10. Test modules
// opt back in via `#[cfg_attr(test, allow(clippy::unwrap_used))]` on
// their `#[cfg(test)] mod tests` block; see existing examples in
// `scan_fs/package_db/npm.rs` and friends.
#![deny(clippy::unwrap_used)]

use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

/// Lifecycle-scope variants the `--exclude-scope` flag accepts.
/// Maps to `mikebom_common::resolution::LifecycleScope` non-Runtime
/// variants — `Runtime` is intentionally not exposed (excluding it
/// would produce an empty SBOM). Milestone 052/part-3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum ExcludeScopeArg {
    Dev,
    Build,
    Test,
}

impl ExcludeScopeArg {
    pub fn as_lifecycle_scope(self) -> mikebom_common::resolution::LifecycleScope {
        use mikebom_common::resolution::LifecycleScope;
        match self {
            ExcludeScopeArg::Dev => LifecycleScope::Development,
            ExcludeScopeArg::Build => LifecycleScope::Build,
            ExcludeScopeArg::Test => LifecycleScope::Test,
        }
    }
}

mod attestation;
mod cli;
mod config;
mod enrich;
mod error;
mod generate;
mod policy;
mod resolve;
mod sbom;
mod scan_fs;
mod trace;

#[derive(Parser)]
#[command(
    name = "mikebom",
    version,
    about = "SBOM generator with optional eBPF build-tracing",
    long_about = "SBOM generator.\n\n\
                  - `mikebom sbom scan` (stable): filesystem / image scanning with \
                  lockfile-aware dep-graph extraction. Cross-platform, no privileges.\n\
                  - `mikebom sbom verify` / `policy init` / `sbom enrich` (stable): \
                  signed attestation verification + in-toto layouts + RFC 6902 \
                  SBOM enrichment.\n\
                  - `mikebom trace` (experimental, Linux only): eBPF-based build-time \
                  capture. Produces attestations bound to the build event. Requires \
                  CAP_BPF + CAP_PERFMON; 2-3× slowdown on syscall-heavy builds. See \
                  docs/user-guide/quickstart.md for stability notes."
)]
struct Cli {
    /// Disable all outbound network calls (deps.dev license/CPE lookups,
    /// deps.dev hash queries). When set, enrichment falls back to what
    /// can be derived from the local filesystem and package databases
    /// alone. Useful for air-gapped scanners, reproducible-build
    /// environments, and CI runs that can't reach the internet.
    ///
    /// Accepted forms: `--offline` (alone, equivalent to `--offline=true`),
    /// `--offline=true`, `--offline=false`. The `=` is required when a
    /// value is supplied (e.g. `--offline true` is rejected); this keeps
    /// the next positional argument from being silently consumed.
    #[arg(
        long,
        global = true,
        require_equals = true,
        num_args = 0..=1,
        default_value_t = false,
        default_missing_value = "true",
    )]
    offline: bool,

    /// Disable Go package-level build-graph classification. By default,
    /// when a `go` toolchain is found on PATH during a Go source scan,
    /// mikebom runs `go mod why -m -vendor` (60-second total budget,
    /// modules batched in chunks of 20) to classify modules outside the
    /// build graph as `not-needed` (emitted with `scope: "excluded"`)
    /// or test-only (emitted with test scope). With this flag set, that
    /// subprocess never runs: affected modules keep the conservative
    /// `mikebom:build-inclusion: unknown` marker instead.
    ///
    /// Also settable via `MIKEBOM_NO_GO_MOD_WHY` (any non-empty value
    /// other than `0` disables classification). Flag or env var — either
    /// one disables. Milestone 112.
    #[arg(long = "no-go-mod-why", global = true)]
    no_go_mod_why: bool,

    /// Drop components whose lifecycle scope matches any of the
    /// listed values. Comma-separated. Valid values: `dev`,
    /// `build`, `test`. Runtime-scope is always retained
    /// (excluding all runtime would produce an empty SBOM).
    ///
    /// Example: `--exclude-scope dev,build,test` produces the
    /// strict "what shipped to production" view (alpha.9 default
    /// behavior). `--exclude-scope test` drops only test deps;
    /// `--exclude-scope dev,build` keeps test for security-audit
    /// workflows.
    ///
    /// When omitted, mikebom emits all scopes (Runtime +
    /// Development + Build + Test) — the milestone-052 default.
    #[arg(long, global = true, value_delimiter = ',')]
    exclude_scope: Vec<ExcludeScopeArg>,

    /// Skip directory subtrees matching the given path or glob pattern
    /// during scan. Repeatable for multiple entries. Entries containing
    /// glob metacharacters (`*`, `?`, `[`) are treated as patterns
    /// matched at any depth in the tree; entries containing none are
    /// literal paths anchored at the scan root (e.g.
    /// `--exclude-path tests/fixtures` matches `<root>/tests/fixtures`
    /// only, while `--exclude-path '**/testdata'` matches every
    /// `testdata` directory at any depth).
    ///
    /// Honored across every ecosystem walker (cargo, maven, gem, pip,
    /// npm, gradle, nuget, yocto, Go source, Go binary). Additive on
    /// top of the scanner's built-in skip set (`vendor/`,
    /// `node_modules/`, `target/`, `dist/`, `build/`, `__pycache__/`,
    /// `.`-prefixed dirs, the Go-tool unconditional skips of
    /// `testdata/` and `_`-prefixed dirs).
    ///
    /// Also via `MIKEBOM_EXCLUDE_PATH` using the platform's path-list
    /// separator (`:` on Unix, `;` on Windows). CLI flags and env-var
    /// entries combine by union.
    ///
    /// When any exclusion is in effect the emitted SBOM carries a
    /// `mikebom:exclude-path` envelope annotation listing the active
    /// entries (Constitution Principle X). When omitted the emitted
    /// SBOM is byte-identical to a pre-feature build.
    ///
    /// See docs/user-guide/cli-reference.md#--exclude-path.
    #[arg(long, global = true, action = clap::ArgAction::Append, value_name = "PATH_OR_PATTERN")]
    exclude_path: Vec<String>,

    /// Include declared-but-not-on-disk dependencies (manifest SBOM).
    /// By default, mikebom emits only components physically present in
    /// the scanned tree or image ("artifact SBOM" — if it's in the
    /// image, it's in the SBOM). When set, also emits: (1) deps.dev-
    /// reported transitives with no on-disk trace
    /// (`source_type = declared-not-cached`); (2) Maven pom.xml-
    /// declared direct deps with no matching JAR or `.m2` cache entry
    /// (`source_type = workspace`); (3) Maven BFS cache-miss
    /// transitives (`source_type = transitive`, no `.pom` on disk).
    /// Auto-enabled for `sbom scan --path` so source-tree scans keep
    /// the "what would be pulled in on build" view; explicit for
    /// `--image` when you want the same permissive output from a
    /// container scan. See docs/design-notes.md "Scope: artifact vs
    /// manifest SBOM" for the full rationale. Common causes of
    /// declared-but-not-shipped: Maven `<scope>provided</scope>` deps
    /// (servlet-api, etc.), JDK-bundled classes, optional deps,
    /// aggressive shade-plugin metadata stripping, and closure-union
    /// inflation across many observed roots.
    #[arg(long, global = true)]
    include_declared_deps: bool,

    /// Enable reading of legacy Berkeley-DB rpmdb (`/var/lib/rpm/Packages`)
    /// on pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images. Off by default;
    /// preserves milestone-003 behaviour (diagnostic log, zero components)
    /// so existing scans don't silently change output. Canonical
    /// invocation: `mikebom sbom scan --include-legacy-rpmdb …`. Also
    /// enabled via `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`. Milestone 004 US4.
    #[arg(long, global = true, env = "MIKEBOM_INCLUDE_LEGACY_RPMDB")]
    include_legacy_rpmdb: bool,

    /// Wall-clock time limit for the entire mikebom invocation, in
    /// seconds. If exceeded, mikebom emits a tracing::error and exits
    /// with status 124 (POSIX `timeout(1)` convention).
    ///
    /// Use cases: bound a runaway scan in CI; protect a Kubernetes
    /// CronJob's pod-disruption budget; cap exploratory image scans
    /// against unknown content.
    ///
    /// Disabled when omitted or set to 0. Mutually exclusive with no
    /// other flag — it complements `--offline`, registry timeouts,
    /// and the existing `trace run --timeout` (which caps the
    /// SUBPROCESS being traced, not mikebom itself; whichever fires
    /// first wins).
    ///
    /// Partial output may not be written when the watchdog fires —
    /// no atomic-flush guarantees apply. Operators who need
    /// "produce-the-best-SBOM-you-can-in-N-seconds" semantics should
    /// pair `--timeout` with `--output` to a specific path and check
    /// for that file's presence after the run.
    #[arg(long, global = true, value_name = "SECONDS")]
    timeout: Option<u64>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// [EXPERIMENTAL, Linux-only] eBPF build-process tracing.
    /// Produces attestations bound to the build event. Requires CAP_BPF +
    /// CAP_PERFMON. Adds ~2-3× wall-clock overhead on syscall-heavy builds.
    /// For most SBOM use cases, prefer `mikebom sbom scan`.
    Trace(cli::trace_cmd::TraceCommand),
    /// SBOM generation, enrichment, and validation
    Sbom(cli::sbom_cmd::SbomCommand),
    /// Attestation management
    Attestation(cli::attestation_cmd::AttestationCommand),
    /// In-toto policy layout management (feature 006 US4)
    Policy(cli::policy::PolicyCommand),
    /// Manage the external symbol-fingerprint corpus cache.
    /// Air-gapped operators use `fingerprints fetch` to pre-populate
    /// the cache on an internet-connected machine, then ship the
    /// cache directory to the air-gapped destination. See
    /// docs/reference/identifiers.md §11 for the consumer-side
    /// verification recipe. (Milestone 108 US4.)
    Fingerprints(cli::fingerprints_cmd::FingerprintsCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<std::process::ExitCode> {
    // Default: INFO + WARN visible at stderr; users override via RUST_LOG.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Milestone 055 (T010): expose --offline as MIKEBOM_OFFLINE env var
    // so the Go transitive-edge resolver in `scan_fs::package_db::
    // golang::graph_resolver::WorkspaceContext::from_parts` can honor
    // it without requiring the multi-test-fixture refactor of plumbing
    // a `bool` parameter through `scan_path` → `read_all` → `golang::
    // read`. Future cleanup: replace this env-var bridge with a proper
    // signature parameter when threading through the broader scan API.
    if cli.offline {
        // SAFETY: single-threaded prelude before any async runtime
        // workers spawn — env mutation here is race-free.
        std::env::set_var("MIKEBOM_OFFLINE", "1");
    }

    // Milestone 112 (T012): expose --no-go-mod-why as the
    // MIKEBOM_NO_GO_MOD_WHY env var so the go-mod-why classification
    // pass in `scan_fs::package_db` can honor it without plumbing a
    // bool through `scan_path` → `read_all` → `golang::read` (same
    // env-var-bridge rationale as MIKEBOM_OFFLINE above). The read
    // side treats any non-empty value other than `0` as "disabled",
    // so flag OR pre-set env var disables classification.
    if cli.no_go_mod_why {
        // SAFETY: single-threaded prelude before any async runtime
        // workers spawn — env mutation here is race-free.
        std::env::set_var("MIKEBOM_NO_GO_MOD_WHY", "1");
    }

    // Global wall-clock watchdog. When `--timeout <SECONDS>` is set
    // to a non-zero value, spawn a detached tokio task that sleeps
    // for the configured duration; if it fires before the main work
    // completes, emit a tracing::error and exit with status 124
    // (matching POSIX `timeout(1)` from coreutils). The watchdog
    // task is detached because we want the process to exit
    // naturally on success — there's no `.await` on the handle.
    //
    // Whichever finishes first wins: if the main work completes
    // before the timeout, the process exits via the normal return
    // path and the still-sleeping watchdog gets cancelled by tokio
    // runtime shutdown.
    if let Some(secs) = cli.timeout {
        if secs > 0 {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                tracing::error!(
                    timeout_secs = secs,
                    "mikebom exceeded the configured --timeout wall-clock limit; exiting with status 124"
                );
                std::process::exit(124);
            });
        }
    }

    let exclude_scope: Vec<mikebom_common::resolution::LifecycleScope> =
        cli.exclude_scope.iter().map(|a| a.as_lifecycle_scope()).collect();

    // Milestone 113 — user-supplied directory exclusion. CLI flag
    // entries plus MIKEBOM_EXCLUDE_PATH env-var entries (split on the
    // platform's path-list separator: `:` on Unix, `;` on Windows)
    // combine by union, in flag-then-env order. Reject malformed
    // entries at parse time (FR-007 / SC-005) so the scan never
    // begins on a bad config.
    let mut exclude_path_entries: Vec<String> = cli.exclude_path.clone();
    if let Ok(env_value) = std::env::var("MIKEBOM_EXCLUDE_PATH") {
        for piece in std::env::split_paths(&env_value) {
            let s = piece.to_string_lossy().into_owned();
            if !s.is_empty() {
                exclude_path_entries.push(s);
            }
        }
    }
    let exclude_set = scan_fs::package_db::exclude_path::ExclusionSet::from_iter(
        exclude_path_entries.iter().map(|s| s.as_str()),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    match cli.command {
        Commands::Trace(cmd) => {
            cli::trace_cmd::execute(cmd).await?;
            Ok(std::process::ExitCode::from(0))
        }
        Commands::Sbom(cmd) => {
            cli::sbom_cmd::execute(
                cmd,
                cli.offline,
                exclude_scope,
                cli.include_legacy_rpmdb,
                cli.include_declared_deps,
                exclude_set,
            )
            .await
        }
        Commands::Attestation(cmd) => {
            cli::attestation_cmd::execute(cmd).await?;
            Ok(std::process::ExitCode::from(0))
        }
        Commands::Policy(cmd) => {
            cli::policy::execute(cmd).await?;
            Ok(std::process::ExitCode::from(0))
        }
        Commands::Fingerprints(cmd) => cli::fingerprints_cmd::execute(cmd).await,
    }
}
