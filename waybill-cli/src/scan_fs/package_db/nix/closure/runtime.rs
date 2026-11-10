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
//! The evaluated derivation's **default output path**, never the installable.
//! Measured (`measurements/nix-behaviour/q9-*`): with `--store <binary cache>`
//! an installable makes evaluation write the `.drv` to the cache and fail;
//! curl's installable does not evaluate at all at the pinned revision; and an
//! installable selects `meta.outputsToInstall` rather than `outPath`, which is
//! what the committed runtime truth was taken from. A store path needs no
//! evaluation, so no repository code runs for this query either.
//!
//! The local store is asked first. Then each substituter the user's nix is
//! already configured to trust, one `--store <url>` call each, because nix
//! takes only the last `--store` it is given (q2). A remote answer is used only
//! after `nix store verify --no-contents -r` succeeds against the same store:
//! `nix path-info --store` does not check signatures, and a bogus trusted key
//! still exits 0 (q2 d). Every remote call carries `connect-timeout 2` and
//! `download-attempts 1`, which bound an unreachable cache at ~2.1 s instead of
//! nix's ~79 s default (q5).

use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

use serde_json::Value;
use waybill_common::resolution::{Relationship, ResolvedComponent};

use super::derivation::{store_basename, RawClosure};
use super::member_key::{key_of_component, key_of_derivation, MemberKey};
use super::static_link::{static_link_set, DeclaredEdges, StaticLinkSource};
use super::{guard, ClassifiedClosure};
use crate::scan_fs::package_db::nix::eval::invoke::run_bounded;
use crate::scan_fs::package_db::nix::eval::reason::DegradationReason;

/// The connect timeout passed on every remote query, in seconds (q5).
pub(crate) const REMOTE_CONNECT_TIMEOUT_SECS: &str = "2";
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
    /// Constructed by the walk fallback (milestone 1423 US2, tasks T024/T025).
    #[allow(dead_code)]
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

/// C216 — how the document's scope marks were decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeMethodRecord {
    pub(crate) method: ScopeMethod,
    pub(crate) store: Option<String>,
    pub(crate) verified: Option<bool>,
    pub(crate) unavailable_reason: Option<RuntimeUnavailable>,
    pub(crate) static_link_source: Option<StaticLinkSource>,
    pub(crate) unmatched_runtime_paths: Option<usize>,
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
        }
    }

    /// The record for marks left to the role rule.
    pub(crate) fn closure_role(reason: RuntimeUnavailable) -> Self {
        Self {
            method: ScopeMethod::ClosureRole,
            store: None,
            verified: None,
            unavailable_reason: Some(reason),
            static_link_source: None,
            unmatched_runtime_paths: None,
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
        Value::Object(o).to_string()
    }
}

/// `nix path-info -r <path>` against the default (local) store.
///
/// No bounds: the default store is the daemon, which refuses
/// `connect-timeout` and `download-attempts` from an untrusted user as
/// restricted settings (q9), and the local store involves no network.
pub(crate) fn local_argv(output: &str) -> Vec<&str> {
    vec!["path-info", "-r", output]
}

/// `nix path-info -r --store <url>` with the remote bounds.
pub(crate) fn remote_argv<'a>(url: &'a str, output: &'a str) -> Vec<&'a str> {
    vec![
        "path-info",
        "-r",
        "--store",
        url,
        "--option",
        "connect-timeout",
        REMOTE_CONNECT_TIMEOUT_SECS,
        "--option",
        "download-attempts",
        REMOTE_DOWNLOAD_ATTEMPTS,
        output,
    ]
}

/// `nix store verify --no-contents -r --store <url>` with the remote bounds.
///
/// `--no-contents` checks signatures only: it downloads no NARs.
pub(crate) fn verify_argv<'a>(url: &'a str, output: &'a str) -> Vec<&'a str> {
    vec![
        "store",
        "verify",
        "--no-contents",
        "-r",
        "--store",
        url,
        "--option",
        "connect-timeout",
        REMOTE_CONNECT_TIMEOUT_SECS,
        "--option",
        "download-attempts",
        REMOTE_DOWNLOAD_ATTEMPTS,
        output,
    ]
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

/// The full store path of the evaluated derivation's default output.
pub(crate) fn evaluated_output(raw: &RawClosure, evaluated: &str) -> Option<String> {
    let path = raw.derivations.get(evaluated)?.default_output_path()?;
    Some(if path.starts_with('/') {
        path.to_string()
    } else {
        // `outputs.*.path` is a bare basename in format version 4; nix itself
        // honours `NIX_STORE_DIR`, so the full path does too.
        super::contents::store_dir()
            .join(path)
            .display()
            .to_string()
    })
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

fn ask(argv: &[&str], budget: Duration) -> Answer {
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

/// Obtain the runtime closure of `output`: the local store first, then each
/// substituter in turn. The first verified answer wins.
///
/// Order affects latency only. Store paths are content-addressed, so any
/// trusted cache that holds the closure returns the same set (spec FR-004).
pub(crate) fn query(
    budget: Duration,
    output: &str,
    substituters: &[String],
) -> Result<RuntimeClosure, RuntimeUnavailable> {
    let mut worst = match ask(&local_argv(output), budget) {
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
    for url in substituters {
        let reason = match ask(&remote_argv(url, output), budget) {
            Answer::Hit(paths) => match run(&verify_argv(url, output), budget) {
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

/// Query the runtime closure of a classified closure's evaluated package.
pub(crate) fn query_closure(
    closure_raw: &RawClosure,
    evaluated: Option<&str>,
    budget: Duration,
    substituters: &[String],
) -> Result<RuntimeClosure, RuntimeUnavailable> {
    let Some(evaluated) = evaluated else {
        return Err(RuntimeUnavailable::NoEvaluatedPackage);
    };
    let Some(output) = evaluated_output(closure_raw, evaluated) else {
        tracing::info!("nix-closure: the evaluated derivation declares no output path");
        return Err(RuntimeUnavailable::QueryFailed);
    };
    let started = std::time::Instant::now();
    let result = query(budget, &output, substituters);
    match &result {
        Ok(rc) => tracing::info!(
            store = rc.store.wire(),
            paths = rc.paths.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "nix-closure: runtime closure obtained"
        ),
        Err(reason) => tracing::info!(
            reason = reason.wire(),
            substituters = substituters.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "nix-closure: runtime closure unavailable"
        ),
    }
    result
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
    /// Marks come from the runtime closure plus static linking.
    Closure {
        runtime: HashSet<MemberKey>,
        static_linked: HashSet<MemberKey>,
    },
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
    let rc = match &closure.runtime {
        None => return (RuntimeEvidence::None, None),
        Some(Err(reason)) => {
            // Until the walk fallback lands (US2), an unavailable runtime
            // closure leaves today's role rule in charge, and C216 says so.
            return (RuntimeEvidence::None, Some(ScopeMethodRecord::closure_role(*reason)));
        }
        Some(Ok(rc)) => rc,
    };
    let raw = &closure.raw;
    let (runtime_drvs, stray) = join(raw, &rc.paths);
    let declared = package_set_available
        .then(|| DeclaredEdges::from_document(raw, components, relationships));
    let linked = static_link_set(raw, &runtime_drvs, declared.as_ref());

    let key_of = |k: &String| raw.derivations.get(k).and_then(|d| key_of_derivation(d, k));
    let runtime: HashSet<MemberKey> = runtime_drvs.iter().filter_map(key_of).collect();
    let static_linked: HashSet<MemberKey> = linked.members.iter().filter_map(key_of).collect();

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
        statically_linked = static_linked.len(),
        static_link_source = linked.source.wire(),
        unmatched,
        "nix-closure: scope from the runtime closure"
    );
    let record = ScopeMethodRecord::runtime_closure(rc, linked.source, unmatched);
    (RuntimeEvidence::Closure { runtime, static_linked }, Some(record))
}

/// The runtime query's outcome as `ClassifiedClosure` carries it. `None`
/// means the query was never made, which only a closure built outside
/// [`super::resolve`] (a test) can be.
pub(crate) type RuntimeOutcome = Option<Result<RuntimeClosure, RuntimeUnavailable>>;

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
        let argv = remote_argv("https://cache.example", "/nix/store/x-out");
        let s = argv.join(" ");
        assert!(s.contains("--store https://cache.example"), "{s}");
        assert!(s.contains("--option connect-timeout 2"), "{s}");
        assert!(s.contains("--option download-attempts 1"), "{s}");
        assert_eq!(argv.iter().filter(|a| **a == "--store").count(), 1);
        assert_eq!(argv.last(), Some(&"/nix/store/x-out"));
        let v = verify_argv("https://cache.example", "/nix/store/x-out").join(" ");
        assert!(v.starts_with("store verify --no-contents -r --store https://cache.example"), "{v}");
        assert!(v.contains("--option connect-timeout 2") && v.contains("--option download-attempts 1"));
    }

    #[test]
    fn the_local_query_carries_no_store_and_no_restricted_options() {
        let argv = local_argv("/nix/store/x-out");
        assert_eq!(argv, vec!["path-info", "-r", "/nix/store/x-out"]);
    }

    #[test]
    fn every_argv_passes_the_guard() {
        for argv in [
            local_argv("/nix/store/x"),
            remote_argv("https://c", "/nix/store/x"),
            verify_argv("https://c", "/nix/store/x"),
        ] {
            assert!(argv_is_safe(&argv), "{argv:?}");
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
        assert_eq!(
            raw.derivations["d-app.drv"].default_output_path(),
            Some("h1-app-1-bin"),
            "`bin` heads `outputs`, though `out` sorts after it and exists"
        );
        assert_eq!(raw.derivations["d-lib.drv"].default_output_path(), Some("h3-lib-2"));
    }

    #[test]
    fn the_record_wire_is_sorted_and_complete() {
        let rc = RuntimeClosure {
            paths: BTreeSet::new(),
            store: RuntimeStore::Substituter("https://cache.example".into()),
            verified: true,
        };
        let w = ScopeMethodRecord::runtime_closure(&rc, StaticLinkSource::DeclaredEdges, 3).wire();
        assert_eq!(
            w,
            r#"{"method":"runtime-closure","schema":1,"static-c-linking-detected":false,"static-link-source":"declared-edges","store":"https://cache.example","unavailable-reason":null,"unmatched-runtime-paths":3,"verified":true}"#
        );
        let w = ScopeMethodRecord::closure_role(RuntimeUnavailable::NoEvaluatedPackage).wire();
        assert_eq!(
            w,
            r#"{"method":"closure-role","schema":1,"static-c-linking-detected":false,"static-link-source":null,"store":null,"unavailable-reason":"no-evaluated-package","unmatched-runtime-paths":null,"verified":null}"#
        );
    }
}
