// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// Orchestration: interleaved measurement, verdict assembly, self-check gate.

pub mod config;
pub mod identity;
pub mod report;
pub mod score;
pub mod selfcheck;
pub mod truth;

use std::error::Error;

use clap::Args;

/// `cargo run -p xtask --release -- compare`
#[derive(Args, Debug)]
pub struct CompareArgs {
    /// Operator-supplied tool set. Gitignored; see `tools.example.toml`.
    #[arg(long, default_value = "xtask/compare/tools.local.toml")]
    pub config: std::path::PathBuf,

    /// Restrict to named targets. Repeatable. Default: every target.
    #[arg(long)]
    pub target: Vec<String>,

    /// Timing repeats per tool per target. Below 3 the spread figure is
    /// not meaningful, so it is rejected rather than clamped.
    #[arg(long, default_value_t = 5)]
    pub repeats: usize,
}

pub fn run(args: CompareArgs) -> Result<(), Box<dyn Error>> {
    if args.repeats < 3 {
        return Err(format!(
            "--repeats must be at least 3 (got {}); a spread computed from \
             fewer than three observations does not distinguish noise from a \
             real difference, and reporting one would recreate the problem \
             this harness exists to solve",
            args.repeats
        )
        .into());
    }
    let workspace_root = workspace_root()?;

    // FR-012/FR-013, contract C-1. Before any tool is invoked against any
    // target, the harness confirms it recovers a known answer. A failure
    // aborts here having measured nothing.
    match selfcheck::run(&workspace_root)? {
        selfcheck::SelfCheckResult::Passed { expected } => {
            println!("self-check: {expected} expected, {expected} recovered  OK");
        }
        selfcheck::SelfCheckResult::Failed { detail } => {
            return Err(format!(
                "SELF-CHECK FAILED — measuring nothing.\n\n{detail}\n\n\
                 The harness could not recover a known answer from its own \
                 fixture, so any score it produced would be untrustworthy. \
                 Fix the reduction before comparing tools."
            )
            .into());
        }
    }

    let tools = config::load_tools(&args.config)?;
    let targets = config::load_targets(&config::default_targets_path(&workspace_root))?;
    let selected: Vec<_> = if args.target.is_empty() {
        targets
    } else {
        targets
            .into_iter()
            .filter(|t| args.target.contains(&t.name))
            .collect()
    };
    if selected.is_empty() {
        return Err("no targets selected".into());
    }

    println!(
        "measuring {} tool(s) across {} target(s), {} interleaved repeats",
        tools.len(),
        selected.len(),
        args.repeats
    );

    // FR-005 / T021a — provenance, captured once per session. A figure
    // without the version that produced it cannot be compared with a later
    // one, which is why this is a requirement and not a nicety.
    let mut tool_versions = std::collections::BTreeMap::new();
    for tool in &tools {
        tool_versions.insert(tool.id.clone(), capture_version(tool));
    }

    let host = HostInfo::detect();
    let mut reasons: Vec<WithheldReason> = Vec::new();
    if !host.is_reference_class() {
        reasons.push(WithheldReason::HostNotReferenceClass {
            class: format!("{:?}", host.class),
        });
    }

    println!("  host: {} ({:?})", host.uname, host.class);
    for (id, v) in &tool_versions {
        println!("  {id}: {v}");
    }

    // FR-010 / T035 — a mode mismatch is stated, not silently compared.
    let modes: std::collections::BTreeSet<_> =
        tools.iter().map(|t| format!("{:?}", t.network)).collect();
    if modes.len() > 1 {
        let mut it = modes.iter();
        reasons.push(WithheldReason::ModeMismatch {
            a: it.next().cloned().unwrap_or_default(),
            b: it.next().cloned().unwrap_or_default(),
        });
    }

    let scratch = tempfile::tempdir()?;
    let mut measurements: Vec<Measurement> = Vec::new();

    for target in &selected {
        let tree = fetch_target(&workspace_root, target)?;
        println!("\ntarget {} @ {}", target.name, &target.sha[..12.min(target.sha.len())]);

        let truth = match &target.truth {
            Some(spec) => Some(truth::derive(spec.method, &tree)?),
            None => None,
        };

        // Per-tool accumulators. Populated by the interleaved loop below.
        let mut acc: std::collections::BTreeMap<String, ToolAccumulator> = tools
            .iter()
            .map(|t| (t.id.clone(), ToolAccumulator::default()))
            .collect();

        // T020 / contract C-2 — INTERLEAVED, never per-tool blocks and
        // never concurrent. Block execution lets drift between blocks
        // masquerade as a difference between tools, which is how a 2.7x
        // contention error entered the measurements that motivated this.
        for round in 1..=args.repeats {
            for tool in &tools {
                let out = scratch.path().join(format!("{}-{}-{round}.json", tool.id, target.name));
                let one = measure_once(tool, &tree, &out);
                let entry = acc.get_mut(&tool.id).expect("pre-seeded");
                entry.push(one);
            }
            print!("  round {round}/{} done\r", args.repeats);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        println!();

        for tool in &tools {
            let a = acc.remove(&tool.id).expect("pre-seeded");
            let m = a.finish(tool, &target.name, truth.as_ref(), &target.ecosystem, &mut reasons);
            println!(
                "  {:<20} {:>8}  distinct={:<6} raw={:<6} identityless={:<5} {}",
                tool.id,
                m.median_secs.map(|v| format!("{v:.2}s")).unwrap_or_else(|| "-".into()),
                m.distinct_packages,
                m.raw_components,
                m.identityless,
                match &m.scored {
                    score::Scored::Yes(acc) => format!(
                        "found={} missed={} extra={}{}",
                        acc.found, acc.missed, acc.extra,
                        if acc.truth_is_superset { " [SUPERSET]" } else { "" }
                    ),
                    score::Scored::No(n) => n.reason(),
                }
            );
            measurements.push(m);
        }
    }

    let verdict = Verdict::from_reasons(reasons);
    println!();
    match &verdict {
        Verdict::Comparable => println!("VERDICT: comparable"),
        Verdict::Withheld { reasons } => {
            println!("VERDICT: withheld ({} reason(s))", reasons.len());
            for r in reasons {
                println!("  - {}", r.describe());
            }
            println!("Figures above are recorded for context and are NOT a comparison.");
        }
    }
    print!("\n{}", report::render_timing_ratios(&measurements));

    let path = report::write_run(&workspace_root, &measurements, &verdict)?;
    let ctx = format!(
        "Host: `{}` ({:?}). Tools: {}.",
        host.uname,
        host.class,
        tool_versions
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let summary = report::write_summary(&workspace_root, &measurements, &verdict, &ctx)?;
    println!("wrote {}", summary.display());
    println!("\nwrote {}", path.display());
    println!(
        "This file is under target/ and is gitignored. It records measured \n\
         quantities with their conditions; it is not a claim about any tool."
    );
    Ok(())
}

/// Resolve a target to an on-disk tree. Uses the m770 cache layout.
fn fetch_target(
    workspace_root: &std::path::Path,
    target: &config::Target,
) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let _ = workspace_root;
    let cache = dirs_cache()?.join("compare").join(&target.name).join(&target.sha);
    let repo_dir = cache.join("repo");
    if repo_dir.join(".git").is_dir() {
        return Ok(repo_dir);
    }
    std::fs::create_dir_all(&cache)?;
    let status = std::process::Command::new("git")
        .args(["clone", "--quiet", &target.repo])
        .arg(&repo_dir)
        .status()?;
    if !status.success() {
        return Err(format!("git clone failed for {}", target.repo).into());
    }
    let status = std::process::Command::new("git")
        .args(["-C"])
        .arg(&repo_dir)
        .args(["checkout", "--quiet", &target.sha])
        .status()?;
    if !status.success() {
        return Err(format!("git checkout {} failed", target.sha).into());
    }
    Ok(repo_dir)
}

fn dirs_cache() -> Result<std::path::PathBuf, Box<dyn Error>> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set")?;
    Ok(std::path::PathBuf::from(home).join(".cache/waybill"))
}

/// T022 — how one invocation ended. A failure is never a coverage result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed { code: i32 },
    TimedOut,
    Unparseable,
    ToolAbsent,
}

impl Outcome {
    pub fn succeeded(&self) -> bool {
        matches!(self, Self::Ok)
    }
    pub fn label(&self) -> String {
        match self {
            Self::Ok => "ok".into(),
            Self::Failed { code } => format!("Failed(exit {code})"),
            Self::TimedOut => "TimedOut".into(),
            Self::Unparseable => "Unparseable".into(),
            Self::ToolAbsent => "ToolAbsent".into(),
        }
    }
}

struct SingleRun {
    secs: f64,
    outcome: Outcome,
    reduction: Option<identity::Reduction>,
}

/// T021 — one invocation, timed.
fn measure_once(
    tool: &config::ToolSpec,
    tree: &std::path::Path,
    out: &std::path::Path,
) -> SingleRun {
    let argv = tool.resolve_argv(tree, out);
    let Some((exe, rest)) = argv.split_first() else {
        return SingleRun { secs: 0.0, outcome: Outcome::ToolAbsent, reduction: None };
    };
    let started = std::time::Instant::now();
    let status = std::process::Command::new(exe)
        .args(rest)
        .envs(&tool.env)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let secs = started.elapsed().as_secs_f64();

    let outcome = match status {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Outcome::ToolAbsent,
        Err(_) => Outcome::Failed { code: -1 },
        Ok(s) if !s.success() => Outcome::Failed { code: s.code().unwrap_or(-1) },
        Ok(_) => Outcome::Ok,
    };
    if !outcome.succeeded() {
        return SingleRun { secs, outcome, reduction: None };
    }
    match std::fs::read_to_string(out).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        Some(doc) => SingleRun {
            secs,
            outcome: Outcome::Ok,
            reduction: Some(identity::reduce_cyclonedx(&doc)),
        },
        // Exited zero but produced nothing readable. Not a coverage result.
        None => SingleRun { secs, outcome: Outcome::Unparseable, reduction: None },
    }
}

#[derive(Default)]
struct ToolAccumulator {
    runs: Vec<SingleRun>,
}

impl ToolAccumulator {
    fn push(&mut self, run: SingleRun) {
        self.runs.push(run);
    }

    /// Collapse repeats into one measurement, adding any gate violations.
    fn finish(
        self,
        tool: &config::ToolSpec,
        target: &str,
        truth: Option<&truth::TruthSet>,
        ecosystem: &str,
        reasons: &mut Vec<WithheldReason>,
    ) -> Measurement {
        let succeeded = self.runs.iter().all(|r| r.outcome.succeeded());
        let outcome = self
            .runs
            .iter()
            .find(|r| !r.outcome.succeeded())
            .map(|r| r.outcome.clone())
            .unwrap_or(Outcome::Ok);

        if !succeeded {
            reasons.push(WithheldReason::ToolFailed { tool: tool.id.clone() });
        }

        // T023 — timing spread gate. Enriched modes get a wider tolerance
        // because their timing includes a third party's latency (FR-002a).
        let secs: Vec<f64> = self.runs.iter().map(|r| r.secs).collect();
        let limit = spread_limit_for(tool.network);
        let observed = spread_ratio(&secs);
        if succeeded && observed > limit {
            reasons.push(WithheldReason::TimingSpreadExceeded {
                tool: tool.id.clone(),
                observed,
                limit,
            });
        }

        // T024 — coverage MUST be identical across repeats (FR-001c). A
        // difference is a defect, not noise, so it is never averaged.
        let reductions: Vec<&identity::Reduction> =
            self.runs.iter().filter_map(|r| r.reduction.as_ref()).collect();
        let mut distinct = 0usize;
        let mut raw = 0usize;
        let mut identityless = 0usize;
        if let Some(first) = reductions.first() {
            distinct = first.distinct();
            raw = first.raw_components;
            identityless = first.identityless;
            for other in &reductions[1..] {
                if other.distinct() != distinct {
                    reasons.push(WithheldReason::CoverageNotReproducible {
                        tool: tool.id.clone(),
                        first: distinct,
                        second: other.distinct(),
                    });
                    break;
                }
            }
        }

        let reported = reductions
            .first()
            .map(|r| r.identities.clone())
            .unwrap_or_default();
        let scored = score::score(&reported, truth, ecosystem, succeeded, &outcome.label());

        let mut sorted = secs.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_secs = (!sorted.is_empty()).then(|| sorted[sorted.len() / 2]);

        Measurement {
            tool_id: tool.id.clone(),
            target: target.to_string(),
            enriched: tool.network.is_enriched(),
            outcome,
            wall_secs: secs,
            median_secs,
            spread: observed,
            distinct_packages: distinct,
            raw_components: raw,
            identityless,
            scored,
        }
    }
}

/// Tolerance for a mode. Enriched runs include a third party's latency, so
/// they are held to a wider bound and never support a speed claim (FR-002a).
pub fn spread_limit_for(mode: config::NetworkMode) -> f64 {
    if mode.is_enriched() {
        ENRICHED_SPREAD_LIMIT
    } else {
        OFFLINE_SPREAD_LIMIT
    }
}

/// Offline timings are gated at this ratio. Provisional — T043 calibrates
/// on a reference-class host, because m770 observed a 2.6x spread on
/// identical hardware and a tolerance chosen on a laptop will not transfer.
pub const OFFLINE_SPREAD_LIMIT: f64 = 1.25;
/// Enriched timings include a third party's latency, so they carry a wider
/// tolerance and are never the basis of a speed claim (FR-002a).
pub const ENRICHED_SPREAD_LIMIT: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct Measurement {
    pub tool_id: String,
    pub target: String,
    pub enriched: bool,
    pub outcome: Outcome,
    pub wall_secs: Vec<f64>,
    pub median_secs: Option<f64>,
    pub spread: f64,
    pub distinct_packages: usize,
    pub raw_components: usize,
    pub identityless: usize,
    pub scored: score::Scored,
}

/// Repository root, derived from this crate's manifest directory.
fn workspace_root() -> Result<std::path::PathBuf, Box<dyn Error>> {
    Ok(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest dir has no parent")?
        .to_path_buf())
}

/// FR-005. Best-effort: a tool that will not report its version is still
/// measurable, but the run records that its provenance is unknown rather
/// than pretending otherwise.
fn capture_version(tool: &config::ToolSpec) -> String {
    if tool.version_argv.is_empty() {
        return "<no version_argv configured>".to_string();
    }
    let (exe, rest) = tool.version_argv.split_first().expect("non-empty");
    match std::process::Command::new(exe).args(rest).output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let text = if text.trim().is_empty() {
                String::from_utf8_lossy(&out.stderr).into_owned()
            } else {
                text.into_owned()
            };
            // Some tools colourise their version output. Strip escapes so
            // the recorded provenance is the text a human reads, not the
            // bytes a terminal renders.
            strip_ansi(text.lines().next().unwrap_or("<empty>")).trim().to_string()
        }
        Err(e) => format!("<unavailable: {e}>"),
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Debug, Clone)]
struct HostInfo {
    uname: String,
    class: crate::bench::schema::NoiseClass,
}

impl HostInfo {
    /// Reuses m669's classification rather than duplicating it. A second
    /// notion of host class would drift from the first and the two would
    /// eventually disagree about the same machine — m669 distinguishes
    /// Reference / Noisy / Other, and collapsing Other into Noisy would
    /// misreport a Linux ARM host.
    fn detect() -> Self {
        let uname = crate::bench::run::read_uname_srmn();
        let class = crate::bench::run::classify_noise(&uname);
        Self { uname, class }
    }

    fn is_reference_class(&self) -> bool {
        matches!(self.class, crate::bench::schema::NoiseClass::Reference)
    }
}

/// Why a comparative verdict was withheld. Accumulated, never
/// short-circuited: one run should tell the operator everything to fix.
///
/// `PartialEq` only — `TimingSpreadExceeded` carries `f64`, which has no
/// total equality.
#[derive(Debug, Clone, PartialEq)]
pub enum WithheldReason {
    HostNotReferenceClass { class: String },
    TimingSpreadExceeded { tool: String, observed: f64, limit: f64 },
    CoverageNotReproducible { tool: String, first: usize, second: usize },
    TruthMethodMismatch { a: String, b: String },
    ModeMismatch { a: String, b: String },
    ToolVersionMismatch { tool: String, then: String, now: String },
    ToolFailed { tool: String },
    SelfCheckFailed { detail: String },
}

impl WithheldReason {
    pub fn describe(&self) -> String {
        match self {
            Self::HostNotReferenceClass { class } => format!(
                "host is {class}, not reference class — coverage and accuracy \
                 figures are still exact; only timing is affected"
            ),
            Self::TimingSpreadExceeded { tool, observed, limit } => format!(
                "timing spread for {tool} exceeded {limit:.2} (observed {observed:.2})"
            ),
            Self::CoverageNotReproducible { tool, first, second } => format!(
                "{tool} reported {first} then {second} packages for the same \
                 input — a difference here is a defect, not noise"
            ),
            Self::TruthMethodMismatch { a, b } => format!(
                "truth methods differ ({a} vs {b}); the scores measure \
                 against different universes"
            ),
            Self::ModeMismatch { a, b } => {
                format!("comparing {a} against {b} sets one tool's cheapest mode against another's richest")
            }
            Self::ToolVersionMismatch { tool, then, now } => format!(
                "{tool} was {then}, now {now} — the difference measured is \
                 between the inputs, not between the tools"
            ),
            Self::ToolFailed { tool } => format!("{tool} did not produce usable output"),
            Self::SelfCheckFailed { detail } => format!("self-check failed: {detail}"),
        }
    }
}

/// FR-002/FR-004/FR-017.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Comparable,
    Withheld { reasons: Vec<WithheldReason> },
}

impl Verdict {
    pub fn from_reasons(reasons: Vec<WithheldReason>) -> Self {
        if reasons.is_empty() {
            Self::Comparable
        } else {
            Self::Withheld { reasons }
        }
    }

    pub fn is_comparable(&self) -> bool {
        matches!(self, Self::Comparable)
    }
}

/// Max/min across repeats. The spread gate's input.
pub fn spread_ratio(samples: &[f64]) -> f64 {
    let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if min <= 0.0 || !min.is_finite() || !max.is_finite() {
        return f64::INFINITY;
    }
    max / min
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reasons_is_comparable() {
        assert!(Verdict::from_reasons(vec![]).is_comparable());
    }

    /// One run must tell the operator everything to fix, not the first thing.
    #[test]
    fn all_reasons_accumulate() {
        let v = Verdict::from_reasons(vec![
            WithheldReason::HostNotReferenceClass { class: "Noisy".into() },
            WithheldReason::TimingSpreadExceeded { tool: "a".into(), observed: 1.61, limit: 1.25 },
            WithheldReason::ToolFailed { tool: "b".into() },
        ]);
        match v {
            Verdict::Withheld { reasons } => assert_eq!(reasons.len(), 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reasons_describe_themselves_actionably() {
        let r = WithheldReason::CoverageNotReproducible { tool: "a".into(), first: 100, second: 101 };
        assert!(r.describe().contains("defect, not noise"), "{}", r.describe());
        let r = WithheldReason::ToolVersionMismatch { tool: "a".into(), then: "1.0".into(), now: "2.0".into() };
        assert!(r.describe().contains("between the inputs"), "{}", r.describe());
    }

    #[test]
    fn spread_ratio_computes_max_over_min() {
        assert!((spread_ratio(&[1.0, 1.2, 1.1]) - 1.2).abs() < 1e-9);
        assert!((spread_ratio(&[2.0, 2.0]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_spread_is_infinite_not_silently_ok() {
        assert!(spread_ratio(&[0.0, 1.0]).is_infinite());
        assert!(spread_ratio(&[]).is_infinite());
    }

    #[test]
    fn host_detection_reuses_the_bench_classification() {
        let h = HostInfo::detect();
        assert!(!h.uname.is_empty());
        let expected = crate::bench::run::classify_noise(&h.uname);
        assert_eq!(format!("{:?}", h.class), format!("{expected:?}"));
    }

    #[test]
    fn ansi_escapes_are_stripped_from_recorded_versions() {
        assert_eq!(strip_ansi("\u{1b}[1mTool 1.2.3\u{1b}[0m"), "Tool 1.2.3");
        assert_eq!(strip_ansi("plain 4.5.6"), "plain 4.5.6");
    }

    #[test]
    fn only_ok_counts_as_success() {
        assert!(Outcome::Ok.succeeded());
        for o in [
            Outcome::Failed { code: 1 },
            Outcome::TimedOut,
            Outcome::Unparseable,
            Outcome::ToolAbsent,
        ] {
            assert!(!o.succeeded(), "{o:?} must not read as success");
            assert!(!o.label().is_empty());
        }
    }

    /// A tool exiting zero while writing nothing readable is NOT a tool that
    /// found zero packages. Conflating the two turns a broken invocation
    /// into a coverage result.
    #[test]
    fn unparseable_output_is_distinct_from_finding_nothing() {
        assert_ne!(Outcome::Unparseable, Outcome::Ok);
        assert!(!Outcome::Unparseable.succeeded());
    }

    /// FR-002a — enriched timings tolerate more spread because they carry a
    /// third party's latency, and are never a speed claim regardless.
    #[test]
    fn enriched_mode_selects_the_wider_tolerance() {
        assert!(
            spread_limit_for(config::NetworkMode::Enriched)
                > spread_limit_for(config::NetworkMode::Offline)
        );
    }
}
