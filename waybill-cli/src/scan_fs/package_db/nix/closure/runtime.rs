//! Milestone 1423 (#1423) — the evaluated output's runtime closure.
//!
//! A `--nix-closure` document lists the whole build closure. What ships is a
//! subset of it, and nix already knows which: `nix path-info -r` returns the
//! store paths a built output references, without building anything. Scoring
//! the milestone-1277 role marks against that set gave them 31% precision as a
//! findings filter (`specs/1423-nix-runtime-scope/measurements/README.md`).
//!
//! # What is queried, and where
//!
//! The store paths of the outputs the evaluated attribute **installs**, never
//! the installable. Which outputs those are is asked of one small pure
//! evaluation of the same attribute ([`SELECTION_EXPR`]), following nix's own
//! rule: the selected output when the attribute is one (`outputSpecified`),
//! else `meta.outputsToInstall`. Their paths come from the closure already
//! read. When the selection cannot be had, the derivation's default output
//! (the head of its `outputs`) is queried and C216 says so (#1458).
//!
//! Why not the head of `outputs` alone: `pkgs.postgrest` is postgrest's `bin`
//! output, while its `outputs` start with `out`, the library, whose runtime
//! closure is 754 paths against `bin`'s 129 (`measurements/nix-behaviour/
//! q11-*`). Why not the installable: with `--store <binary cache>` it makes
//! evaluation write the `.drv` to the cache and fail, and curl's installable
//! fails at the pinned revision with `meta.outputsToInstall … is not a list of
//! strings`, which q11 traced to nix's evaluation cache: `nix eval` reads the
//! same list, and the installable succeeds with `eval-cache false` (q9, q11).
//! A store path needs no evaluation, so the path queries run no repository
//! code.
//!
//! The local store is asked first. Then each substituter the user's nix is
//! already configured to trust, one `--store <url>` call each, because nix
//! takes only the last `--store` it is given (q2). A remote answer is used only
//! after `nix store verify --no-contents -r` succeeds against the same store:
//! `nix path-info --store` does not check signatures, and a bogus trusted key
//! still exits 0 (q2 d). Every remote call carries `connect-timeout 2` and
//! `download-attempts 1`, which bound an unreachable cache at ~2.1 s instead of
//! nix's ~79 s default (q5). The timeout shrinks to what is left of a 5 s
//! budget across substituters ([`REMOTE_BUDGET`]).

use std::collections::{BTreeSet, HashSet};
use std::time::{Duration, Instant};

use serde_json::Value;
use waybill_common::resolution::{Relationship, ResolvedComponent};

use super::derivation::{store_basename, RawClosure};
use super::member_key::{key_of_component, key_of_derivation, MemberKey};
use super::static_link::{static_link_set, DeclaredEdges, ManifestTestDeps, StaticLinkSource};
use super::walk;
use super::{guard, ClassifiedClosure};
use crate::scan_fs::package_db::nix::eval::invoke::run_bounded;
use crate::scan_fs::package_db::nix::eval::preflight::IFD_SETTING;
use crate::scan_fs::package_db::nix::eval::reason::DegradationReason;
use crate::scan_fs::package_db::nix::eval::result::IfdPolicyVerified;

/// The connect timeout passed on a remote query, in whole seconds (q5),
/// unless less of the remote budget is left ([`connect_timeout_for`]).
pub(crate) const REMOTE_CONNECT_TIMEOUT_SECS: u64 = 2;
/// The download attempts passed on every remote query (q5).
pub(crate) const REMOTE_DOWNLOAD_ATTEMPTS: &str = "1";

/// The store that answered the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeStore {
    Local,
    Substituter(String),
}

impl RuntimeStore {
    /// The C216 `store` value.
    pub(crate) fn wire(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::Substituter(url) => url.as_str(),
        }
    }
}

/// The runtime closure of the evaluated output, as a store answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeClosure {
    /// Store basenames.
    pub(crate) paths: BTreeSet<String>,
    pub(crate) store: RuntimeStore,
    /// Signature-verified. Always true: an unverified answer is never built.
    pub(crate) verified: bool,
}

/// How the queried outputs were chosen (C216 `outputs-source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputsSource {
    /// The attribute is one output of its derivation (`outputSpecified`), as
    /// `pkgs.postgrest` is its `bin`: that output (`outputName`).
    OutputSpecified,
    /// `meta.outputsToInstall`, as for shellcheck (`bin`, `man`, `doc`).
    OutputsToInstall,
    /// The fallback: the selection could not be evaluated, declared no
    /// outputs, or named one the evaluated derivation does not have, so the
    /// derivation's default output was queried. It may not be what the
    /// attribute installs.
    DefaultOutput,
}

impl OutputsSource {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::OutputSpecified => "output-specified",
            Self::OutputsToInstall => "outputs-to-install",
            Self::DefaultOutput => "default-output",
        }
    }
}

/// The outputs whose runtime closure was asked for (C216 `outputs`), in the
/// order the attribute names them, and how they were chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputSelection {
    pub(crate) names: Vec<String>,
    pub(crate) source: OutputsSource,
}

/// The expression applied to the evaluated attribute to learn which outputs
/// it installs: nix's own order of precedence, an attribute that is one
/// output first, then `meta.outputsToInstall`. Measured on nixpkgs
/// `a799d3e3` (q11): postgrest and cachix → `output-specified` `["bin"]`;
/// shellcheck → `outputs-to-install` `["bin","man","doc"]`; `curl.out` →
/// `output-specified` `["out"]`, though its `meta.outputsToInstall` says
/// `["bin","man"]`. ~0.45 s on the postgrest wrapper flake, warm.
pub(crate) const SELECTION_EXPR: &str = "d: if d.outputSpecified or false \
     then { source = \"output-specified\"; outputs = [ d.outputName ]; } \
     else { source = \"outputs-to-install\"; outputs = d.meta.outputsToInstall or null; }";

/// `nix eval --json` of [`SELECTION_EXPR`] over the scanned attribute, as
/// pure as the closure query itself: no `--impure`, and the same verified
/// import-from-derivation policy the closure was read under.
///
/// Milestone 1283 (research R1): under `--nix-allow-ifd` this call must
/// allow IFD too. Refusing here on an IFD flake would read the closure and
/// then fall back silently to the default output, leaving only C216
/// `outputs-source: default-output` as a trace.
pub(crate) fn selection_argv<'a>(flakeref: &'a str, ifd: &IfdPolicyVerified) -> Vec<&'a str> {
    vec![
        "eval",
        "--json",
        "--option",
        IFD_SETTING,
        ifd.policy().setting(),
        "--apply",
        SELECTION_EXPR,
        flakeref,
    ]
}

/// The selection [`SELECTION_EXPR`] printed, or `None` when it declares no
/// outputs or is not the shape the expression produces.
pub(crate) fn parse_selection(stdout: &str) -> Option<OutputSelection> {
    let v: Value = serde_json::from_str(stdout.trim()).ok()?;
    let source = match v.get("source")?.as_str()? {
        "output-specified" => OutputsSource::OutputSpecified,
        "outputs-to-install" => OutputsSource::OutputsToInstall,
        _ => return None,
    };
    let mut names: Vec<String> = Vec::new();
    for n in v.get("outputs")?.as_array()? {
        let n = n.as_str()?.to_string();
        if !names.contains(&n) {
            names.push(n);
        }
    }
    (!names.is_empty()).then_some(OutputSelection { names, source })
}

/// Evaluate [`SELECTION_EXPR`] over `flakeref`. `None` on any failure: the
/// caller falls back to the default output, and records that it did.
pub(crate) fn evaluate_selection(
    flakeref: &str,
    ifd: &IfdPolicyVerified,
    budget: Duration,
) -> Option<OutputSelection> {
    let argv = selection_argv(flakeref, ifd);
    guard(&argv).ok()?;
    match run_bounded(&argv, budget) {
        Ok(o) if o.status_success => {
            let parsed = parse_selection(&o.stdout);
            if parsed.is_none() {
                tracing::info!("nix-closure: the attribute declares no outputs to install");
            }
            parsed
        }
        Ok(o) => {
            tracing::info!(
                stderr = %crate::scan_fs::package_db::nix::eval::stderr::salient_stderr(o.stderr.trim()),
                "nix-closure: which outputs the attribute installs could not be evaluated"
            );
            None
        }
        Err(reason) => {
            tracing::info!(
                reason = reason.wire(),
                "nix-closure: which outputs the attribute installs could not be evaluated"
            );
            None
        }
    }
}

/// The full store paths of the outputs to query, and the selection they
/// came from: `declared` when the evaluated derivation has every output it
/// names, else its default output, recorded as [`OutputsSource::DefaultOutput`].
/// `None` when the derivation declares no usable output at all.
pub(crate) fn selected_outputs(
    raw: &RawClosure,
    evaluated: &str,
    declared: Option<OutputSelection>,
) -> Option<(OutputSelection, Vec<String>)> {
    let drv = raw.derivations.get(evaluated)?;
    let full = |path: &str| -> String {
        if path.starts_with('/') {
            path.to_string()
        } else {
            // `outputs.*.path` is a bare basename in format version 4; nix
            // itself honours `NIX_STORE_DIR`, so the full path does too.
            super::contents::store_dir().join(path).display().to_string()
        }
    };
    if let Some(sel) = declared {
        let paths: Option<Vec<String>> =
            sel.names.iter().map(|n| drv.output_path(n).map(full)).collect();
        match paths {
            Some(paths) => return Some((sel, paths)),
            None => tracing::info!(
                outputs = %sel.names.join(" "),
                "nix-closure: the evaluated derivation lacks an output the attribute installs; \
                 querying its default output"
            ),
        }
    }
    let name = drv.default_output_name()?;
    let path = full(drv.output_path(name)?);
    Some((
        OutputSelection { names: vec![name.to_string()], source: OutputsSource::DefaultOutput },
        vec![path],
    ))
}

/// Why no runtime closure was obtained (C216 `unavailable-reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuntimeUnavailable {
    /// No store holds the output.
    NotInAnyStore,
    /// A store answered and the answer failed signature verification.
    VerificationFailed,
    /// A query ran out of time: the budget, or nix's own connect timeout.
    QueryTimedOut,
    /// The closure names no single evaluated package, so there is nothing to
    /// query.
    NoEvaluatedPackage,
    /// A query failed for any other reason.
    QueryFailed,
}

impl RuntimeUnavailable {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::NotInAnyStore => "not-in-any-store",
            Self::VerificationFailed => "verification-failed",
            Self::QueryTimedOut => "query-timed-out",
            Self::NoEvaluatedPackage => "no-evaluated-package",
            Self::QueryFailed => "query-failed",
        }
    }

    /// Which of two failures to report when every store failed.
    ///
    /// A store that *had* the output but could not be trusted, or could not
    /// be reached, says more than a store that simply lacked it: the output
    /// may well be in the one that timed out.
    fn severity(self) -> u8 {
        match self {
            Self::VerificationFailed => 4,
            Self::QueryTimedOut => 3,
            Self::QueryFailed => 2,
            Self::NoEvaluatedPackage => 1,
            Self::NotInAnyStore => 0,
        }
    }
}

/// Which method produced the document's scope marks (C216 `method`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeMethod {
    RuntimeClosure,
    /// The walk fallback (US2): no store answered.
    ReachabilityWalk,
    ClosureRole,
}

impl ScopeMethod {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::RuntimeClosure => "runtime-closure",
            Self::ReachabilityWalk => "reachability-walk",
            Self::ClosureRole => "closure-role",
        }
    }
}

/// What became of the reachability walk (C216 `walk-outcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkOutcome {
    /// The walk decided the marks.
    Used,
    /// The walk reached no closure member but the evaluated root, so it was
    /// not used and the role rule decided. A `justStaticExecutables` wrapper
    /// names its static build only in `inputs.drvs` and `installPhase`, so
    /// the walk from it reaches nothing; used, it would mark every library
    /// compiled into the binary, and the fallback errs toward runtime, never
    /// toward dropping linked code (spec FR-002). For the same reason the
    /// consumed static build and its libraries are kept runtime
    /// ([`RuntimeEvidence::RoleWithLinked`]).
    ReachedOnlyRoot,
}

impl WalkOutcome {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::ReachedOnlyRoot => "reached-only-root",
        }
    }
}

/// C216 — how the document's scope marks were decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeMethodRecord {
    pub(crate) method: ScopeMethod,
    pub(crate) store: Option<String>,
    pub(crate) verified: Option<bool>,
    pub(crate) unavailable_reason: Option<RuntimeUnavailable>,
    pub(crate) static_link_source: Option<StaticLinkSource>,
    pub(crate) unmatched_runtime_paths: Option<usize>,
    /// `None` when no walk ran.
    pub(crate) walk_outcome: Option<WalkOutcome>,
    /// #1458 — the outputs whose runtime closure was asked for. `None` when
    /// no query was made (`no-evaluated-package`, or no output declared).
    pub(crate) outputs: Option<OutputSelection>,
}

impl ScopeMethodRecord {
    /// The record for marks taken from the runtime closure.
    pub(crate) fn runtime_closure(
        runtime: &RuntimeClosure,
        static_link_source: StaticLinkSource,
        unmatched_runtime_paths: usize,
    ) -> Self {
        Self {
            method: ScopeMethod::RuntimeClosure,
            store: Some(runtime.store.wire().to_string()),
            verified: Some(runtime.verified),
            unavailable_reason: None,
            static_link_source: Some(static_link_source),
            unmatched_runtime_paths: Some(unmatched_runtime_paths),
            walk_outcome: None,
            outputs: None,
        }
    }

    /// The record for marks taken from the reachability walk, because the
    /// runtime closure could not be had for `reason`.
    pub(crate) fn reachability_walk(
        reason: RuntimeUnavailable,
        static_link_source: StaticLinkSource,
    ) -> Self {
        Self {
            method: ScopeMethod::ReachabilityWalk,
            store: None,
            verified: None,
            unavailable_reason: Some(reason),
            static_link_source: Some(static_link_source),
            unmatched_runtime_paths: None,
            walk_outcome: Some(WalkOutcome::Used),
            outputs: None,
        }
    }

    /// The record for marks left to the role rule, because the runtime
    /// closure could not be had for `reason`. `walk_outcome` says whether a
    /// walk ran and was set aside, or none could run. `static_link_source`
    /// is set only when the walk was set aside and the consumed static
    /// build's libraries were kept runtime (T042).
    pub(crate) fn closure_role(
        reason: RuntimeUnavailable,
        walk_outcome: Option<WalkOutcome>,
        static_link_source: Option<StaticLinkSource>,
    ) -> Self {
        Self {
            method: ScopeMethod::ClosureRole,
            store: None,
            verified: None,
            unavailable_reason: Some(reason),
            static_link_source,
            unmatched_runtime_paths: None,
            walk_outcome,
            outputs: None,
        }
    }

    /// The C216 wire value: a JSON object with sorted keys, identical in all
    /// three formats.
    pub(crate) fn wire(&self) -> String {
        let opt_str = |v: Option<&str>| v.map_or(Value::Null, |s| Value::String(s.to_string()));
        // Inserted in sorted order so the bytes do not depend on whether
        // `serde_json` preserves insertion order.
        let mut o = serde_json::Map::new();
        o.insert("method".into(), self.method.wire().into());
        o.insert(
            "outputs".into(),
            self.outputs
                .as_ref()
                .map_or(Value::Null, |s| s.names.iter().map(|n| Value::from(n.as_str())).collect()),
        );
        o.insert(
            "outputs-source".into(),
            opt_str(self.outputs.as_ref().map(|s| s.source.wire())),
        );
        o.insert("schema".into(), 1.into());
        // Always false: nothing here detects static C linking, and the
        // document says so rather than implying otherwise.
        o.insert("static-c-linking-detected".into(), false.into());
        o.insert(
            "static-link-source".into(),
            opt_str(self.static_link_source.map(StaticLinkSource::wire)),
        );
        o.insert("store".into(), opt_str(self.store.as_deref()));
        o.insert(
            "unavailable-reason".into(),
            opt_str(self.unavailable_reason.map(RuntimeUnavailable::wire)),
        );
        o.insert(
            "unmatched-runtime-paths".into(),
            self.unmatched_runtime_paths.map_or(Value::Null, Value::from),
        );
        o.insert("verified".into(), self.verified.map_or(Value::Null, Value::from));
        o.insert(
            "walk-outcome".into(),
            opt_str(self.walk_outcome.map(WalkOutcome::wire)),
        );
        Value::Object(o).to_string()
    }
}

/// `nix path-info -r <path>…` against the default (local) store. Given
/// several outputs it lists the union of their closures (q11).
///
/// No bounds: the default store is the daemon, which refuses
/// `connect-timeout` and `download-attempts` from an untrusted user as
/// restricted settings (q9), and the local store involves no network.
pub(crate) fn local_argv<'a>(outputs: &[&'a str]) -> Vec<&'a str> {
    let mut argv = vec!["path-info", "-r"];
    argv.extend_from_slice(outputs);
    argv
}

/// The connect timeout for a substituter started with `remaining` of the
/// remote budget left, or `None` when too little is left to start one.
///
/// nix takes this setting in whole seconds: a fractional value is rejected
/// with a warning and the 15 s default applies, and 0 means no limit (q5c).
/// So the value is the whole seconds left, capped at
/// [`REMOTE_CONNECT_TIMEOUT_SECS`], and under 1 s left nothing is started.
pub(crate) fn connect_timeout_for(remaining: Duration) -> Option<u64> {
    let secs = remaining.as_secs();
    (secs >= 1).then(|| secs.min(REMOTE_CONNECT_TIMEOUT_SECS))
}

/// `nix path-info -r --store <url>` with the remote bounds. `connect_timeout`
/// is whole seconds ([`connect_timeout_for`]).
pub(crate) fn remote_argv<'a>(url: &'a str, connect_timeout: &'a str, outputs: &[&'a str]) -> Vec<&'a str> {
    let mut argv = vec![
        "path-info",
        "-r",
        "--store",
        url,
        "--option",
        "connect-timeout",
        connect_timeout,
        "--option",
        "download-attempts",
        REMOTE_DOWNLOAD_ATTEMPTS,
    ];
    argv.extend_from_slice(outputs);
    argv
}

/// `nix store verify --no-contents -r --store <url>` with the remote bounds.
///
/// `--no-contents` checks signatures only: it downloads no NARs.
pub(crate) fn verify_argv<'a>(url: &'a str, connect_timeout: &'a str, outputs: &[&'a str]) -> Vec<&'a str> {
    let mut argv = vec![
        "store",
        "verify",
        "--no-contents",
        "-r",
        "--store",
        url,
        "--option",
        "connect-timeout",
        connect_timeout,
        "--option",
        "download-attempts",
        REMOTE_DOWNLOAD_ATTEMPTS,
    ];
    argv.extend_from_slice(outputs);
    argv
}

/// Store basenames from `nix path-info -r`'s plain output: one full store
/// path per line (q2, re-confirmed at T001).
pub(crate) fn parse_paths(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| store_basename(l).to_string())
        .collect()
}

/// The configured substituters, from `nix config show`'s full output.
///
/// The tier already runs `nix config show` for the import-from-derivation
/// pre-flight, and that output carries `substituters = <url> <url>` with the
/// `extra-substituters` already merged in (q1), so no second invocation is
/// needed. A value that would read as a flag is dropped rather than passed
/// to nix.
pub(crate) fn substituters_from_config(config: &str) -> Vec<String> {
    config
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "substituters").then_some(v)
        })
        .map(|v| {
            v.split_whitespace()
                .filter(|s| !s.starts_with('-'))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// One store's answer.
enum Answer {
    Hit(BTreeSet<String>),
    Miss(RuntimeUnavailable),
}

/// Classify a non-zero `path-info` or `store verify`.
fn failure_of(stderr: &str) -> RuntimeUnavailable {
    if stderr.contains("is not valid") || stderr.contains("don't know how to build") {
        RuntimeUnavailable::NotInAnyStore
    } else if stderr.contains("Timeout was reached") || stderr.contains("timed out") {
        RuntimeUnavailable::QueryTimedOut
    } else {
        RuntimeUnavailable::QueryFailed
    }
}

fn run(argv: &[&str], budget: Duration) -> Result<(bool, String, String), RuntimeUnavailable> {
    guard(argv).map_err(|_| RuntimeUnavailable::QueryFailed)?;
    match run_bounded(argv, budget) {
        Ok(o) => Ok((o.status_success, o.stdout, o.stderr)),
        Err(DegradationReason::BudgetExceeded { .. }) => Err(RuntimeUnavailable::QueryTimedOut),
        Err(_) => Err(RuntimeUnavailable::QueryFailed),
    }
}

/// How a query runs one `nix` invocation: [`run`] in production, a fake in
/// the tests, so the cumulative budget can be tested without a clock.
type Runner<'a> =
    dyn FnMut(&[&str], Duration) -> Result<(bool, String, String), RuntimeUnavailable> + 'a;

fn ask(run: &mut Runner<'_>, argv: &[&str], budget: Duration) -> Answer {
    match run(argv, budget) {
        Ok((true, stdout, _)) => {
            let paths = parse_paths(&stdout);
            if paths.is_empty() {
                // A zero-path closure is not an answer: every output
                // references at least itself.
                Answer::Miss(RuntimeUnavailable::QueryFailed)
            } else {
                Answer::Hit(paths)
            }
        }
        Ok((false, _, stderr)) => Answer::Miss(failure_of(&stderr)),
        Err(reason) => Answer::Miss(reason),
    }
}

/// Obtain the runtime closure of `outputs` (their union): the local store
/// first, then each substituter in turn. The first verified answer wins. A
/// store that lacks any one of the outputs is a miss.
///
/// Order affects latency only. Store paths are content-addressed, so any
/// trusted cache that holds the closure returns the same set (spec FR-004).
pub(crate) fn query(
    budget: Duration,
    outputs: &[&str],
    substituters: &[String],
) -> Result<RuntimeClosure, RuntimeUnavailable> {
    let started = Instant::now();
    query_with(&mut run, &|| started.elapsed(), budget, REMOTE_BUDGET, outputs, substituters)
}

/// The total time the substituter queries may spend on unreachable caches
/// before the walk takes over (T041 and its follow-up, SC-005).
///
/// An unreachable cache costs its connect timeout plus ~0.1 s (q5: 2.1 s at
/// 2 s). Tried one after another at a fixed 2 s, three would cost ~6.3 s.
/// So before each substituter the connect timeout is shrunk to the whole
/// seconds of this budget left, and with under 1 s left no further one is
/// started ([`connect_timeout_for`]). Three unreachable caches then cost
/// 4.2 s, the third never started.
///
/// The timeout bounds the connect phase only. A reachable cache answering a
/// cold query is slow by nature (hlint: 11 s for 322 paths, q3), and that
/// answer is not cut short: cutting it would trade a correct answer for the
/// walk.
pub(crate) const REMOTE_BUDGET: Duration = Duration::from_secs(5);

/// [`query`], with the runner, the clock and the remote budget supplied.
///
/// `elapsed` is time since the query started; only the time spent on
/// substituters counts against `remote_budget`, the local store's answer
/// costing no network.
fn query_with(
    run: &mut Runner<'_>,
    elapsed: &dyn Fn() -> Duration,
    budget: Duration,
    remote_budget: Duration,
    outputs: &[&str],
    substituters: &[String],
) -> Result<RuntimeClosure, RuntimeUnavailable> {
    let mut worst = match ask(run, &local_argv(outputs), budget) {
        Answer::Hit(paths) => {
            return Ok(RuntimeClosure {
                paths,
                store: RuntimeStore::Local,
                // The local store holds only paths it has verified or built.
                verified: true,
            })
        }
        Answer::Miss(reason) => reason,
    };
    let remote_started = elapsed();
    for (i, url) in substituters.iter().enumerate() {
        let spent = elapsed().saturating_sub(remote_started);
        let Some(connect) = connect_timeout_for(remote_budget.saturating_sub(spent)) else {
            tracing::info!(
                spent_ms = spent.as_millis(),
                untried = substituters.len() - i,
                "nix-closure: substituter budget spent; not trying the rest"
            );
            if RuntimeUnavailable::QueryTimedOut.severity() > worst.severity() {
                worst = RuntimeUnavailable::QueryTimedOut;
            }
            break;
        };
        // One timeout per substituter, for its query and its verification
        // alike: a verification follows a query that just reached the cache,
        // and is never skipped, since skipping it would discard the answer.
        let connect = connect.to_string();
        let reason = match ask(run, &remote_argv(url, &connect, outputs), budget) {
            Answer::Hit(paths) => match run(&verify_argv(url, &connect, outputs), budget) {
                Ok((true, _, _)) => {
                    return Ok(RuntimeClosure {
                        paths,
                        store: RuntimeStore::Substituter(url.clone()),
                        verified: true,
                    })
                }
                // A failed or unrunnable verification is a miss: an answer we
                // cannot trust is not used (FR-004).
                Ok((false, _, _)) | Err(_) => RuntimeUnavailable::VerificationFailed,
            },
            Answer::Miss(reason) => reason,
        };
        tracing::debug!(store = %url, reason = reason.wire(), "nix-closure: runtime query missed");
        if reason.severity() > worst.severity() {
            worst = reason;
        }
    }
    Err(worst)
}

/// Query the runtime closure of the outputs the evaluated attribute
/// installs. `flakeref` is the attribute the closure was taken of; it is
/// evaluated once more, purely, to learn which outputs those are
/// ([`evaluate_selection`]).
pub(crate) fn query_closure(
    closure_raw: &RawClosure,
    evaluated: Option<&str>,
    flakeref: &str,
    ifd: &IfdPolicyVerified,
    budget: Duration,
    substituters: &[String],
) -> RuntimeQuery {
    let Some(evaluated) = evaluated else {
        return RuntimeQuery { outputs: None, result: Err(RuntimeUnavailable::NoEvaluatedPackage) };
    };
    let declared = evaluate_selection(flakeref, ifd, budget);
    let Some((selection, paths)) = selected_outputs(closure_raw, evaluated, declared) else {
        tracing::info!("nix-closure: the evaluated derivation declares no output path");
        return RuntimeQuery { outputs: None, result: Err(RuntimeUnavailable::QueryFailed) };
    };
    let started = std::time::Instant::now();
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let result = query(budget, &path_refs, substituters);
    let outputs = selection.names.join(" ");
    let outputs_source = selection.source.wire();
    match &result {
        Ok(rc) => tracing::info!(
            store = rc.store.wire(),
            paths = rc.paths.len(),
            outputs = %outputs,
            outputs_source,
            elapsed_ms = started.elapsed().as_millis(),
            "nix-closure: runtime closure obtained"
        ),
        Err(reason) => tracing::info!(
            reason = reason.wire(),
            substituters = substituters.len(),
            outputs = %outputs,
            outputs_source,
            elapsed_ms = started.elapsed().as_millis(),
            "nix-closure: runtime closure unavailable"
        ),
    }
    RuntimeQuery { outputs: Some(selection), result }
}

/// Runtime paths → the `.drv` keys that produce them, by output path.
///
/// **Never by `nix path-info`'s `deriver`**, which is not the evaluated
/// `.drv`: for curl it names `pdhn4vf0…` where evaluation built `6jvivpv1…`,
/// both for the same output (q2-deriver). The join goes through the closure's
/// own `outputs.*.path`, the same index every other closure join uses.
///
/// Returns the matched keys and the paths no closure output produced.
pub(crate) fn join(raw: &RawClosure, paths: &BTreeSet<String>) -> (BTreeSet<String>, Vec<String>) {
    let index = raw.output_index();
    let mut drvs = BTreeSet::new();
    let mut unmatched = Vec::new();
    for p in paths {
        match index.get(p.as_str()) {
            Some(key) => {
                drvs.insert((*key).to_string());
            }
            None => unmatched.push(p.clone()),
        }
    }
    (drvs, unmatched)
}

/// What the scope pass decides from (data-model.md), joined to component
/// identities.
///
/// Keyed by [`MemberKey`] rather than `.drv` path because a component is an
/// identity and build variants share it: if any variant is in the runtime
/// closure, the identity ships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeEvidence {
    /// Marks come from the runtime closure plus static linking. `static_linked`
    /// holds the Haskell libraries linked into a runtime member, and the
    /// Haskell builds a wrapper root consumed through `inputs.drvs` together
    /// with what is linked into them, when the runtime closure does not
    /// already hold those builds (x86_64-linux, q10).
    Closure {
        runtime: HashSet<MemberKey>,
        static_linked: HashSet<MemberKey>,
    },
    /// Marks come from the reachability walk (US2): `reached` is what the walk
    /// from the evaluated package reaches, plus the Haskell libraries
    /// statically linked into the reached members. One set, because every
    /// member in it is decided by the walk's approximation, and C203 says so
    /// for each rather than presenting a link-edge inference from an
    /// approximate root as proven (spec US2 scenario 2).
    Walk { reached: HashSet<MemberKey> },
    /// The walk was set aside (`reached-only-root`), so the role rule
    /// decides, except for `linked`: the Haskell builds the wrapper root
    /// consumed through `inputs.drvs` and the libraries linked into them,
    /// which ship inside the wrapper's binary and are kept runtime as
    /// statically linked (T042). The fallback errs toward runtime, never
    /// toward dropping linked code (FR-002).
    RoleWithLinked { linked: HashSet<MemberKey> },
    /// No evidence: the milestone-1277 role rule decides, as before 1423.
    None,
}

/// Build the scope pass's evidence and the C216 record from a classified
/// closure and the document as it stands just before the pass (T017).
///
/// `package_set_available` is whether the milestone-1275 identity pass had
/// the pinned package set, which is what decides whether its declared edges
/// can be the static-link source. The record is `None` only for a closure
/// whose runtime query was never made, which no production path produces.
pub(crate) fn evidence(
    closure: &ClassifiedClosure,
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    package_set_available: bool,
) -> (RuntimeEvidence, Option<ScopeMethodRecord>) {
    let Some(query) = &closure.runtime else {
        return (RuntimeEvidence::None, None);
    };
    let (evidence, mut record) =
        decide(closure, &query.result, components, relationships, package_set_available);
    // #1458 — which outputs were asked about is a fact of the query, whatever
    // the scope pass then used.
    if let Some(r) = record.as_mut() {
        r.outputs = query.outputs.clone();
    }
    (evidence, record)
}

fn decide(
    closure: &ClassifiedClosure,
    result: &Result<RuntimeClosure, RuntimeUnavailable>,
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    package_set_available: bool,
) -> (RuntimeEvidence, Option<ScopeMethodRecord>) {
    let raw = &closure.raw;
    let declared = || {
        package_set_available.then(|| DeclaredEdges::from_document(raw, components, relationships))
    };
    // #1283 — what the scanned project's manifest declares for its tests,
    // which the static-link fallback must not read as linked code.
    let test_deps = ManifestTestDeps::from_document(raw, components, relationships);
    let key_of = |k: &String| raw.derivations.get(k).and_then(|d| key_of_derivation(d, k));
    let rc = match result {
        Err(reason) => {
            // Data-model rule 5: no single evaluated package means no root to
            // walk from, so the role rule decides and C216 says why. The
            // query reports `no-evaluated-package` for exactly this case.
            let Some(root) = closure.graph.evaluated.as_deref() else {
                let record = ScopeMethodRecord::closure_role(
                    RuntimeUnavailable::NoEvaluatedPackage,
                    None,
                    None,
                );
                return (RuntimeEvidence::None, Some(record));
            };
            // Rule 4 (FR-005): the walk, never the role rule...
            let walked = walk::reachable(raw, root);
            let linked = static_link_set(raw, &walked, declared().as_ref(), &test_deps);
            let reached: HashSet<MemberKey> =
                walked.iter().chain(&linked.members).filter_map(key_of).collect();
            // ...unless it reached no member but the root, from a root that
            // consumed a Haskell build. A wrapper that names its static build
            // only in `inputs.drvs` (shellcheck's `justStaticExecutables`
            // shape) gives such a walk, and using it would mark every library
            // linked into the binary. The fallback errs toward runtime, never
            // toward dropping linked code (FR-002), so the role rule decides,
            // as with no evidence at all. A root that reaches only itself and
            // consumed no Haskell build keeps the walk: gh's does, and is
            // exact on its findings (`walk::consumed_haskell_builds`).
            //
            // T042: the consumed Haskell builds themselves, and what is
            // linked into them, are not left to the role rule, which marks
            // the build `build-tooling`: their code is in the wrapper's
            // binary.
            let root_key = key_of(&root.to_string());
            let consumed = walk::consumed_haskell_builds(raw, root);
            if reached.iter().all(|k| Some(k) == root_key.as_ref()) && !consumed.is_empty() {
                let linked_libs = static_link_set(raw, &consumed, declared().as_ref(), &test_deps);
                let linked: HashSet<MemberKey> =
                    consumed.iter().chain(&linked_libs.members).filter_map(key_of).collect();
                tracing::info!(
                    reason = reason.wire(),
                    derivations = raw.derivations.len(),
                    reached_derivations = walked.len(),
                    consumed_haskell_builds = consumed.len(),
                    statically_linked = linked.len(),
                    static_link_source = linked_libs.source.wire(),
                    "nix-closure: the reachability walk reached only the evaluated package; \
                     scope from the closure role, keeping the consumed static build runtime"
                );
                let record = ScopeMethodRecord::closure_role(
                    *reason,
                    Some(WalkOutcome::ReachedOnlyRoot),
                    Some(linked_libs.source),
                );
                return (RuntimeEvidence::RoleWithLinked { linked }, Some(record));
            }
            tracing::info!(
                reason = reason.wire(),
                derivations = raw.derivations.len(),
                reached_derivations = walked.len(),
                statically_linked = linked.members.len(),
                reached_members = reached.len(),
                static_link_source = linked.source.wire(),
                "nix-closure: scope from the reachability walk (an approximation)"
            );
            let record = ScopeMethodRecord::reachability_walk(*reason, linked.source);
            return (RuntimeEvidence::Walk { reached }, Some(record));
        }
        Ok(rc) => rc,
    };
    let (runtime_drvs, stray) = join(raw, &rc.paths);
    // Static linking is seeded from the runtime members and, when the
    // evaluated root is a wrapper rather than a Haskell build itself, from the
    // Haskell builds it consumed through `inputs.drvs`. The root is runtime by
    // definition, and its consumed static build is what its binary was copied
    // from. The runtime closure cannot be relied on to name that build: it
    // does on aarch64-darwin, and on x86_64-linux it does not (q10), which
    // left every library linked into shellcheck marked.
    //
    // A Haskell root is already a seed through the runtime closure, and its
    // own `inputs.drvs` carries its test dependencies, which its declared
    // edges exist to leave out.
    let seeds: BTreeSet<String> = closure
        .graph
        .evaluated
        .as_deref()
        .filter(|r| !raw.derivations.get(*r).is_some_and(|d| d.is_haskell_builder()))
        .map(|r| walk::consumed_haskell_builds(raw, r))
        .unwrap_or_default();
    let roots: BTreeSet<String> = runtime_drvs.union(&seeds).cloned().collect();
    let linked = static_link_set(raw, &roots, declared().as_ref(), &test_deps);

    let runtime: HashSet<MemberKey> = runtime_drvs.iter().filter_map(key_of).collect();
    let static_linked: HashSet<MemberKey> = linked
        .members
        .iter()
        .chain(seeds.difference(&runtime_drvs))
        .filter_map(key_of)
        .collect();

    // FR-003 / C216 `unmatched-runtime-paths`: a runtime path no closure
    // output produced, or whose producer became no component. Counted, never
    // guessed at by name.
    let present: HashSet<MemberKey> = components.iter().filter_map(key_of_component).collect();
    let index = raw.output_index();
    let unmatched = stray.len()
        + rc
            .paths
            .iter()
            .filter_map(|p| index.get(p.as_str()))
            .filter(|k| {
                raw.derivations
                    .get(**k)
                    .and_then(|d| key_of_derivation(d, k))
                    .is_none_or(|mk| !present.contains(&mk))
            })
            .count();
    tracing::info!(
        runtime_paths = rc.paths.len(),
        runtime_members = runtime.len(),
        seeded_from_root = seeds.difference(&runtime_drvs).count(),
        statically_linked = static_linked.len(),
        static_link_source = linked.source.wire(),
        unmatched,
        "nix-closure: scope from the runtime closure"
    );
    let record = ScopeMethodRecord::runtime_closure(rc, linked.source, unmatched);
    (RuntimeEvidence::Closure { runtime, static_linked }, Some(record))
}

/// The runtime query: which outputs it asked about, and what came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeQuery {
    /// `None` when no output was queried (`no-evaluated-package`, or the
    /// evaluated derivation declares no output path).
    pub(crate) outputs: Option<OutputSelection>,
    pub(crate) result: Result<RuntimeClosure, RuntimeUnavailable>,
}

/// The runtime query's outcome as `ClassifiedClosure` carries it. `None`
/// means the query was never made, which only a closure built outside
/// [`super::resolve`] (a test) can be.
pub(crate) type RuntimeOutcome = Option<RuntimeQuery>;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::nix::eval::invoke::argv_is_safe;

    #[test]
    fn the_listing_parses_into_store_basenames() {
        let out = "/nix/store/aaa-libffi-40\n/nix/store/bbb-shellcheck-0.11.0-bin\n\n  \n";
        let got = parse_paths(out);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            vec!["aaa-libffi-40".to_string(), "bbb-shellcheck-0.11.0-bin".to_string()]
        );
        assert!(parse_paths("").is_empty());
    }

    #[test]
    fn remote_queries_are_bounded_and_target_one_store() {
        let argv = remote_argv("https://cache.example", "2", &["/nix/store/x-out"]);
        let s = argv.join(" ");
        assert!(s.contains("--store https://cache.example"), "{s}");
        assert!(s.contains("--option connect-timeout 2"), "{s}");
        assert!(s.contains("--option download-attempts 1"), "{s}");
        assert_eq!(argv.iter().filter(|a| **a == "--store").count(), 1);
        assert_eq!(argv.last(), Some(&"/nix/store/x-out"));
        let v = verify_argv("https://cache.example", "1", &["/nix/store/x-out"]).join(" ");
        assert!(v.starts_with("store verify --no-contents -r --store https://cache.example"), "{v}");
        assert!(v.contains("--option connect-timeout 1") && v.contains("--option download-attempts 1"));
    }

    /// #1458: several outputs go into one query, after every option, so the
    /// union of their closures comes back from one store (q11).
    #[test]
    fn several_outputs_are_queried_together_after_the_options() {
        let outs = ["/nix/store/a-bin", "/nix/store/a-man"];
        assert_eq!(local_argv(&outs), vec!["path-info", "-r", "/nix/store/a-bin", "/nix/store/a-man"]);
        for argv in [remote_argv("https://c", "2", &outs), verify_argv("https://c", "2", &outs)] {
            assert_eq!(&argv[argv.len() - 2..], &outs[..], "{argv:?}");
        }
    }

    /// q5c: nix takes whole seconds (a fraction silently becomes the 15 s
    /// default) and 0 means no limit, so neither is ever passed.
    #[test]
    fn the_connect_timeout_is_whole_seconds_capped_and_never_zero() {
        let ms = Duration::from_millis;
        assert_eq!(connect_timeout_for(REMOTE_BUDGET), Some(2));
        assert_eq!(connect_timeout_for(ms(2900)), Some(2));
        assert_eq!(connect_timeout_for(ms(2000)), Some(2));
        assert_eq!(connect_timeout_for(ms(1999)), Some(1));
        assert_eq!(connect_timeout_for(ms(1000)), Some(1));
        assert_eq!(connect_timeout_for(ms(999)), None);
        assert_eq!(connect_timeout_for(Duration::ZERO), None);
    }

    #[test]
    fn the_local_query_carries_no_store_and_no_restricted_options() {
        let argv = local_argv(&["/nix/store/x-out"]);
        assert_eq!(argv, vec!["path-info", "-r", "/nix/store/x-out"]);
    }

    #[test]
    fn every_argv_passes_the_guard() {
        for argv in [
            local_argv(&["/nix/store/x"]),
            remote_argv("https://c", "2", &["/nix/store/x"]),
            verify_argv("https://c", "2", &["/nix/store/x"]),
            selection_argv(
                "/proj#packages.x86_64-linux.default",
                &IfdPolicyVerified::assumed_for_test(
                    crate::scan_fs::package_db::nix::eval::preflight::IfdPolicy::Allow,
                ),
            ),
        ] {
            assert!(argv_is_safe(&argv), "{argv:?}");
        }
    }

    /// m1283 (T013a, research R1) — the selection evaluation carries the
    /// verified policy's value, both ways. Refusing here under the opt-in
    /// would read an IFD flake's closure and then quietly fall back to the
    /// default output.
    #[test]
    fn the_selection_argv_carries_the_verified_policy() {
        use crate::scan_fs::package_db::nix::eval::preflight::IfdPolicy;
        for (p, want) in [(IfdPolicy::Refuse, "false"), (IfdPolicy::Allow, "true")] {
            let ifd = IfdPolicyVerified::assumed_for_test(p);
            let argv = selection_argv("/proj#packages.x86_64-linux.default", &ifd);
            assert_eq!(
                argv.iter().position(|a| *a == IFD_SETTING).map(|i| argv[i + 1]),
                Some(want),
                "{argv:?}"
            );
            assert!(!argv.contains(&"--impure") && !argv.contains(&"--accept-flake-config"));
        }
    }

    #[test]
    fn substituters_come_from_the_full_config_listing() {
        let cfg = "allow-dirty = true\nsubstituters = https://cache.nixos.org/ https://install.determinate.systems\ntrusted-substituters = https://x\n";
        assert_eq!(
            substituters_from_config(cfg),
            vec!["https://cache.nixos.org/".to_string(), "https://install.determinate.systems".to_string()]
        );
        assert!(substituters_from_config("allow-dirty = true\n").is_empty());
        assert!(substituters_from_config("substituters =\n").is_empty());
        // `trusted-substituters` must not be mistaken for `substituters`.
        assert!(substituters_from_config("trusted-substituters = https://x\n").is_empty());
        assert_eq!(
            substituters_from_config("substituters = --impure https://ok\n"),
            vec!["https://ok".to_string()],
            "a value that reads as a flag never reaches nix"
        );
    }

    #[test]
    fn failures_classify_by_what_nix_said() {
        assert_eq!(
            failure_of("don't know how to build these paths:\nerror: path '/nix/store/x' is not valid"),
            RuntimeUnavailable::NotInAnyStore
        );
        assert_eq!(
            failure_of("error: unable to download '…': Timeout was reached (28)"),
            RuntimeUnavailable::QueryTimedOut
        );
        assert_eq!(failure_of("error: something else"), RuntimeUnavailable::QueryFailed);
    }

    /// How one substituter behaves in the stub `nix` for [`query_with`].
    #[derive(Clone, Copy)]
    enum Sub {
        /// Unreachable: fails when nix's connect timeout fires, costing the
        /// timeout the argv carried plus 0.1 s, as measured (q5: 2.1 s at 2).
        Unreachable,
        /// Reachable, but does not hold the output; costs the given time.
        Miss(Duration),
        /// Holds the output; the query costs the given time, and the answer
        /// verifies.
        Hit(Duration),
    }

    /// A stub `nix`: the local store misses at once, and each substituter
    /// behaves as `subs` says, advancing a fake clock. Returns the outcome,
    /// every argv the stub saw, and the time the substituters cost.
    fn stub_query(
        subs: &[(&str, Sub)],
        remote_budget: Duration,
    ) -> (Result<RuntimeClosure, RuntimeUnavailable>, Vec<String>, Duration) {
        use std::cell::{Cell, RefCell};
        let clock = Cell::new(Duration::ZERO);
        let log = RefCell::new(Vec::new());
        let mut stub = |argv: &[&str], _budget: Duration| {
            log.borrow_mut().push(argv.join(" "));
            let Some(url) = argv.iter().position(|a| *a == "--store").map(|i| argv[i + 1]) else {
                return Ok((false, String::new(), "error: path '/nix/store/x' is not valid".to_string()));
            };
            let connect = argv
                .iter()
                .position(|a| *a == "connect-timeout")
                .and_then(|i| argv[i + 1].parse::<u64>().ok())
                .unwrap();
            let verify = argv.first() == Some(&"store");
            match subs.iter().find(|(u, _)| *u == url).unwrap().1 {
                Sub::Unreachable => {
                    clock.set(clock.get() + Duration::from_secs(connect) + Duration::from_millis(100));
                    Ok((false, String::new(), "error: unable to download: Timeout was reached".to_string()))
                }
                Sub::Miss(cost) => {
                    clock.set(clock.get() + cost);
                    Ok((false, String::new(), "error: path '/nix/store/x' is not valid".to_string()))
                }
                Sub::Hit(_) if verify => Ok((true, String::new(), String::new())),
                Sub::Hit(cost) => {
                    clock.set(clock.get() + cost);
                    Ok((true, "/nix/store/a-x-out\n".to_string(), String::new()))
                }
            }
        };
        let urls: Vec<String> = subs.iter().map(|(u, _)| u.to_string()).collect();
        let got = query_with(
            &mut stub,
            &|| clock.get(),
            Duration::from_secs(300),
            remote_budget,
            &["/nix/store/x-out"],
            &urls,
        );
        (got, log.into_inner(), clock.get())
    }

    /// The `connect-timeout` value of each remote `path-info` call, in order.
    fn connect_timeouts(log: &[String]) -> Vec<u64> {
        log.iter()
            .filter(|l| l.starts_with("path-info") && l.contains("--store"))
            .map(|l| {
                let w: Vec<&str> = l.split(' ').collect();
                let i = w.iter().position(|a| *a == "connect-timeout").unwrap();
                w[i + 1].parse().unwrap()
            })
            .collect()
    }

    /// T041 follow-up / SC-005: three unreachable caches at the measured
    /// 2.1 s each stop within the 5 s budget. The third is not started with
    /// 0.8 s left: nix takes whole seconds, and 0 means no limit (q5c).
    #[test]
    fn three_unreachable_substituters_stop_within_the_budget() {
        let subs = [
            ("https://a.test", Sub::Unreachable),
            ("https://b.test", Sub::Unreachable),
            ("https://c.test", Sub::Unreachable),
        ];
        let (got, log, spent) = stub_query(&subs, REMOTE_BUDGET);
        assert_eq!(got, Err(RuntimeUnavailable::QueryTimedOut));
        assert_eq!(connect_timeouts(&log), vec![2, 2], "{log:?}");
        assert!(!log.iter().any(|l| l.contains("https://c.test")), "{log:?}");
        assert!(spent <= REMOTE_BUDGET, "spent {spent:?}");
    }

    /// The timeout shrinks to the whole seconds left: after a 1.4 s miss and
    /// one unreachable cache, 1.5 s remain, so the next gets 1 s, and the one
    /// after that, with 0.4 s left, is skipped.
    #[test]
    fn the_connect_timeout_shrinks_to_the_remaining_budget() {
        let subs = [
            ("https://a.test", Sub::Miss(Duration::from_millis(1400))),
            ("https://b.test", Sub::Unreachable),
            ("https://c.test", Sub::Unreachable),
            ("https://d.test", Sub::Unreachable),
        ];
        let (got, log, spent) = stub_query(&subs, REMOTE_BUDGET);
        assert_eq!(got, Err(RuntimeUnavailable::QueryTimedOut));
        assert_eq!(connect_timeouts(&log), vec![2, 2, 1], "{log:?}");
        assert!(!log.iter().any(|l| l.contains("https://d.test")), "{log:?}");
        assert!(spent <= REMOTE_BUDGET, "spent {spent:?}");
    }

    /// CONTROL: a reachable cache's slow but legitimate answer is used. The
    /// connect timeout bounds only the connect phase, so an 11 s cold query
    /// (hlint, q3) is not cut short, and its verification runs with the same
    /// timeout as its query.
    #[test]
    fn a_reachable_slow_answer_is_not_cut_short() {
        let subs = [("https://a.test", Sub::Hit(Duration::from_secs(11)))];
        let (got, log, _) = stub_query(&subs, REMOTE_BUDGET);
        let rc = got.unwrap();
        assert_eq!(rc.store, RuntimeStore::Substituter("https://a.test".into()));
        assert!(
            log.iter().any(|l| l.starts_with("store verify") && l.contains("connect-timeout 2")),
            "{log:?}"
        );
    }

    /// The local store costs nothing against the remote budget, and a zero
    /// budget tries no substituter at all.
    #[test]
    fn a_spent_budget_tries_no_substituter() {
        let (got, log, _) = stub_query(&[("https://a.test", Sub::Unreachable)], Duration::ZERO);
        assert_eq!(got, Err(RuntimeUnavailable::QueryTimedOut));
        assert_eq!(log, vec!["path-info -r /nix/store/x-out".to_string()]);
        // Without substituters there is nothing to stop, so the reason stays
        // the local miss.
        let (got, _, _) = stub_query(&[], Duration::ZERO);
        assert_eq!(got, Err(RuntimeUnavailable::NotInAnyStore));
    }

    #[test]
    fn the_worst_reason_is_reported() {
        let mut v = [
            RuntimeUnavailable::NotInAnyStore,
            RuntimeUnavailable::QueryFailed,
            RuntimeUnavailable::QueryTimedOut,
            RuntimeUnavailable::VerificationFailed,
        ];
        v.sort_by_key(|r| r.severity());
        assert_eq!(v.last(), Some(&RuntimeUnavailable::VerificationFailed));
        assert_eq!(v.first(), Some(&RuntimeUnavailable::NotInAnyStore));
    }

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-app.drv": {"env": {"pname":"app","version":"1","outputs":"bin out"},
                      "outputs": {"bin": {"path":"h1-app-1-bin"}, "out": {"path":"h2-app-1"}}},
        "d-lib.drv": {"env": {"pname":"lib","version":"2"},
                      "outputs": {"out": {"path":"h3-lib-2"}}}
      }
    }"#;

    #[test]
    fn the_join_is_by_output_path_and_counts_strays() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let paths: BTreeSet<String> =
            ["h1-app-1-bin", "h3-lib-2", "zz-libSystem"].iter().map(|s| s.to_string()).collect();
        let (drvs, unmatched) = join(&raw, &paths);
        assert_eq!(drvs.into_iter().collect::<Vec<_>>(), vec!["d-app.drv", "d-lib.drv"]);
        assert_eq!(unmatched, vec!["zz-libSystem".to_string()]);
    }

    #[test]
    fn the_default_output_is_the_head_of_the_outputs_attribute() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let app = &raw.derivations["d-app.drv"];
        assert_eq!(
            app.default_output_name(),
            Some("bin"),
            "`bin` heads `outputs`, though `out` sorts after it and exists"
        );
        assert_eq!(app.output_path("bin"), Some("h1-app-1-bin"));
        assert_eq!(raw.derivations["d-lib.drv"].default_output_name(), Some("out"));
    }

    /// q11: what [`SELECTION_EXPR`] printed for each measured attribute.
    #[test]
    fn the_selection_parses_as_nix_printed_it() {
        let sel = |names: &[&str], source| OutputSelection {
            names: names.iter().map(|s| s.to_string()).collect(),
            source,
        };
        assert_eq!(
            parse_selection(r#"{"outputs":["bin"],"source":"output-specified"}"#),
            Some(sel(&["bin"], OutputsSource::OutputSpecified))
        );
        assert_eq!(
            parse_selection("{\"outputs\":[\"bin\",\"man\",\"doc\"],\"source\":\"outputs-to-install\"}\n"),
            Some(sel(&["bin", "man", "doc"], OutputsSource::OutputsToInstall))
        );
        assert_eq!(
            parse_selection(r#"{"outputs":["bin","bin"],"source":"outputs-to-install"}"#),
            Some(sel(&["bin"], OutputsSource::OutputsToInstall)),
            "a repeated name is one output"
        );
        for bad in [
            r#"{"outputs":null,"source":"outputs-to-install"}"#,
            r#"{"outputs":[],"source":"outputs-to-install"}"#,
            r#"{"outputs":["bin",3],"source":"outputs-to-install"}"#,
            r#"{"outputs":["bin"],"source":"something-else"}"#,
            r#"["bin"]"#,
            "",
        ] {
            assert_eq!(parse_selection(bad), None, "{bad}");
        }
    }

    /// postgrest's shape: `outputs` is headed by `out`, the attribute is
    /// `bin`. The selection decides; without one, or with a name the
    /// derivation lacks, the head of `outputs` is used and says so.
    #[test]
    fn selected_outputs_follow_the_selection_and_fall_back_to_the_default() {
        let raw = RawClosure::parse(
            r#"{"derivations": {"d-pg.drv": {"env": {"pname":"pg","version":"1","outputs":"out doc bin"},
               "outputs": {"bin": {"path":"/s/h-pg-bin"}, "doc": {"path":"/s/h-pg-doc"}, "out": {"path":"/s/h-pg"}}}}}"#,
        )
        .unwrap();
        let declared = |names: &[&str], source| {
            Some(OutputSelection { names: names.iter().map(|s| s.to_string()).collect(), source })
        };
        let (sel, paths) =
            selected_outputs(&raw, "d-pg.drv", declared(&["bin"], OutputsSource::OutputSpecified)).unwrap();
        assert_eq!((sel.names, sel.source), (vec!["bin".to_string()], OutputsSource::OutputSpecified));
        assert_eq!(paths, vec!["/s/h-pg-bin"]);

        let (sel, paths) = selected_outputs(
            &raw,
            "d-pg.drv",
            declared(&["bin", "doc"], OutputsSource::OutputsToInstall),
        )
        .unwrap();
        assert_eq!(sel.source, OutputsSource::OutputsToInstall);
        assert_eq!(paths, vec!["/s/h-pg-bin", "/s/h-pg-doc"]);

        for fallback in [None, declared(&["bin", "man"], OutputsSource::OutputsToInstall)] {
            let (sel, paths) = selected_outputs(&raw, "d-pg.drv", fallback).unwrap();
            assert_eq!((sel.names, sel.source), (vec!["out".to_string()], OutputsSource::DefaultOutput));
            assert_eq!(paths, vec!["/s/h-pg"]);
        }
        assert_eq!(selected_outputs(&raw, "d-absent.drv", None), None);
    }

    #[test]
    fn the_record_wire_is_sorted_and_complete() {
        let rc = RuntimeClosure {
            paths: BTreeSet::new(),
            store: RuntimeStore::Substituter("https://cache.example".into()),
            verified: true,
        };
        let mut record = ScopeMethodRecord::runtime_closure(&rc, StaticLinkSource::DeclaredEdges, 3);
        record.outputs = Some(OutputSelection {
            names: vec!["bin".into(), "man".into()],
            source: OutputsSource::OutputsToInstall,
        });
        assert_eq!(
            record.wire(),
            r#"{"method":"runtime-closure","outputs":["bin","man"],"outputs-source":"outputs-to-install","schema":1,"static-c-linking-detected":false,"static-link-source":"declared-edges","store":"https://cache.example","unavailable-reason":null,"unmatched-runtime-paths":3,"verified":true,"walk-outcome":null}"#
        );
        let w = ScopeMethodRecord::closure_role(RuntimeUnavailable::NoEvaluatedPackage, None, None)
            .wire();
        assert_eq!(
            w,
            r#"{"method":"closure-role","outputs":null,"outputs-source":null,"schema":1,"static-c-linking-detected":false,"static-link-source":null,"store":null,"unavailable-reason":"no-evaluated-package","unmatched-runtime-paths":null,"verified":null,"walk-outcome":null}"#
        );
        let w = ScopeMethodRecord::closure_role(
            RuntimeUnavailable::NotInAnyStore,
            Some(WalkOutcome::ReachedOnlyRoot),
            Some(StaticLinkSource::DerivationFields),
        )
        .wire();
        assert!(w.ends_with(r#""verified":null,"walk-outcome":"reached-only-root"}"#), "{w}");
        assert!(w.contains(r#""static-link-source":"derivation-fields""#), "{w}");
        let w = ScopeMethodRecord::reachability_walk(
            RuntimeUnavailable::QueryTimedOut,
            StaticLinkSource::None,
        )
        .wire();
        assert!(w.ends_with(r#""verified":null,"walk-outcome":"used"}"#), "{w}");
    }
}
