// Milestone 1065 (#853) — bounds on step 3 of the resolution ladder (proxy
// `.mod` fetch). Two, because the measured failure shapes need different
// caps (specs/1065-go-proxy-fetch-bounds/research.md R1):
//
// - a per-entry circuit breaker for a proxy that never answers, where every
//   module otherwise waits out its own 10 s / 30 s timeout;
// - a per-scan time budget for many genuinely missing modules, which the
//   proxy answers in ~1.4 s per batch of 16.
//
// State lives for one scan and is shared by every workspace and every fetch
// worker, hence `Mutex` behind the `Arc` the resolver holds.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::scan_fs::package_db::golang::graph_resolver::{ErrorClass, GoTransitiveCoverage};

/// Research R3: 60 s covers ~685 cold missing modules at the measured
/// ~1.4 s per batch of 16, and is over 100x the healthy cost of a
/// 197-module real tree. Same value as m771's `go mod why` budget.
pub const DEFAULT_PROXY_FETCH_BUDGET: Duration = Duration::from_secs(60);

/// Default budget, or the test-only `WAYBILL_GO_PROXY_FETCH_BUDGET_MS`
/// integer-milliseconds override (m771 precedent; not an operator setting).
pub fn budget_from_env() -> Duration {
    std::env::var("WAYBILL_GO_PROXY_FETCH_BUDGET_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_PROXY_FETCH_BUDGET)
}

/// A proxy entry as it may appear in output: `scheme://host[:port]`.
///
/// `GOPROXY` URLs may carry credentials as userinfo; they, the path and the
/// query never leave this function (research R6, FR-006, FR-008).
pub fn entry_label(url: &reqwest::Url) -> String {
    let host = url.host_str().unwrap_or("");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

/// A raw `GOPROXY` token that may not parse as a URL, with any userinfo
/// (`user:token@`) between `://` and the host removed. Used where the input
/// itself must be shown, e.g. a parse error (#1110).
pub fn redact_userinfo(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return raw.to_string();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    match rest[..authority_end].rfind('@') {
        Some(at) => format!("{scheme}://{}", &rest[at + 1..]),
        None => raw.to_string(),
    }
}

/// Failures that say the proxy itself is unusable, as opposed to an answer
/// about one module.
pub fn is_network_level(class: ErrorClass) -> bool {
    matches!(
        class,
        ErrorClass::Connection | ErrorClass::Timeout | ErrorClass::Dns | ErrorClass::Tls
    )
}

#[derive(Debug, Default, Clone)]
struct EntryHealth {
    label: String,
    responded: bool,
    consecutive_network_failures: usize,
    tripped: Option<ErrorClass>,
    /// Requests sent to this entry that have not finished yet.
    in_flight: usize,
    /// Modules whose fetch ended without a body after this entry failed at
    /// the network level or was skipped as tripped.
    affected: usize,
}

#[derive(Debug, Default)]
struct Inner {
    entries: Vec<EntryHealth>,
    started: Option<Instant>,
    exhausted: bool,
    not_attempted: usize,
    attempts: usize,
}

/// Per-scan breaker and budget state.
#[derive(Debug)]
pub struct ProxyFetchBounds {
    inner: Mutex<Inner>,
    /// Signalled whenever a request finishes, so workers holding back from
    /// a suspect entry re-check it.
    finished: Condvar,
    trip_threshold: usize,
    budget: Duration,
}

/// What a fetch should do with chain entry `idx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryGate {
    Send,
    /// The entry tripped; treat as a network failure of this class without
    /// sending.
    Skip(ErrorClass),
}

impl ProxyFetchBounds {
    pub fn new(trip_threshold: usize, budget: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            finished: Condvar::new(),
            trip_threshold: trip_threshold.max(1),
            budget,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn entry<'a>(inner: &'a mut Inner, idx: usize, label: &str) -> &'a mut EntryHealth {
        if inner.entries.len() <= idx {
            inner.entries.resize_with(idx + 1, EntryHealth::default);
        }
        let e = &mut inner.entries[idx];
        if e.label.is_empty() {
            e.label = label.to_string();
        }
        e
    }

    /// Whether a new fetch may start. Starts the clock on the first call;
    /// once the budget is spent, counts the module as not attempted.
    pub fn admit(&self) -> bool {
        let mut inner = self.lock();
        let started = *inner.started.get_or_insert_with(Instant::now);
        if inner.exhausted || started.elapsed() >= self.budget {
            if !inner.exhausted {
                inner.exhausted = true;
                tracing::debug!(
                    budget_ms = self.budget.as_millis() as u64,
                    "Go proxy-fetch budget exhausted"
                );
            }
            inner.not_attempted += 1;
            return false;
        }
        true
    }

    /// Whether to send to entry `idx`. Every `Send` MUST be followed by
    /// exactly one `record_response` / `record_network_failure` /
    /// `record_other_failure` for the same entry.
    ///
    /// Half-open hold: while an entry has never answered and already has a
    /// network-level failure, no new request starts until those in flight
    /// finish. Without it the first worker to fail re-dispatches before its
    /// batch-mates fail, and a dead proxy costs a second full timeout
    /// (measured: 20.3 s instead of 10 s for 64 modules). A proxy that has
    /// answered is never held.
    pub fn gate(&self, idx: usize, label: &str) -> EntryGate {
        let mut inner = self.lock();
        loop {
            let e = Self::entry(&mut inner, idx, label);
            if let Some(class) = e.tripped {
                return EntryGate::Skip(class);
            }
            let suspect =
                !e.responded && e.consecutive_network_failures > 0 && e.in_flight > 0;
            if !suspect {
                e.in_flight += 1;
                inner.attempts += 1;
                return EntryGate::Send;
            }
            // Bounded by the in-flight requests' own timeouts; the timed
            // wait only guards against a missed signal.
            inner = match self.finished.wait_timeout(inner, Duration::from_millis(250)) {
                Ok((g, _)) => g,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    fn finish(&self, idx: usize, label: &str, f: impl FnOnce(&mut EntryHealth)) {
        {
            let mut inner = self.lock();
            let e = Self::entry(&mut inner, idx, label);
            e.in_flight = e.in_flight.saturating_sub(1);
            f(e);
        }
        self.finished.notify_all();
    }

    /// Any HTTP response, whatever its status: the entry is reachable and
    /// can never trip (FR-001).
    pub fn record_response(&self, idx: usize, label: &str) {
        self.finish(idx, label, |e| e.responded = true);
    }

    pub fn record_network_failure(&self, idx: usize, label: &str, class: ErrorClass) {
        let threshold = self.trip_threshold;
        self.finish(idx, label, |e| {
            if e.responded || e.tripped.is_some() {
                return;
            }
            e.consecutive_network_failures += 1;
            if e.consecutive_network_failures >= threshold {
                e.tripped = Some(class);
                tracing::debug!(proxy = %e.label, class = class.as_str(), "Go module proxy tripped");
            }
        });
    }

    /// A sent request that ended in neither a response nor a network-level
    /// failure (e.g. a body that would not decode).
    pub fn record_other_failure(&self, idx: usize, label: &str) {
        self.finish(idx, label, |_| {});
    }

    /// A module's fetch ended without a body. Charge it to every entry that
    /// failed it at the network level or was skipped as tripped.
    pub fn record_unresolved(&self, network_failed_entries: &[usize]) {
        let mut inner = self.lock();
        for &idx in network_failed_entries {
            if let Some(e) = inner.entries.get_mut(idx) {
                e.affected += 1;
            }
        }
    }

    #[cfg(test)]
    pub fn attempts(&self) -> usize {
        self.lock().attempts
    }

    pub fn outcome(&self) -> BoundOutcome {
        let inner = self.lock();
        BoundOutcome {
            breaker: inner
                .entries
                .iter()
                .filter_map(|e| e.tripped.map(|c| (e.label.clone(), c, e.affected)))
                .collect(),
            budget: inner
                .exhausted
                .then_some((self.budget, inner.not_attempted)),
        }
    }
}

/// The per-scan result of both bounds (data-model.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundOutcome {
    /// `(label, trip class, modules resolved from go.sum only)` per tripped
    /// entry, in chain order.
    pub breaker: Vec<(String, ErrorClass, usize)>,
    /// `(budget, modules not attempted)` iff the budget was exhausted.
    pub budget: Option<(Duration, usize)>,
}

fn budget_text(d: Duration) -> String {
    if d.subsec_millis() == 0 {
        format!("{}s", d.as_secs())
    } else {
        format!("{}ms", d.as_millis())
    }
}

impl BoundOutcome {
    /// The C110/C111 contribution (contracts/coverage-reason.md). `None`
    /// when no bound left a module to the go.sum fallback, so a scan with
    /// no effect is byte-identical (FR-009). A tripped entry whose modules
    /// all resolved through a later `|` entry lost nothing and adds nothing.
    pub fn coverage(&self) -> Option<GoTransitiveCoverage> {
        let breaker: Vec<String> = self
            .breaker
            .iter()
            .filter(|(_, _, n)| *n > 0)
            .map(|(label, class, n)| {
                format!(
                    "proxy-unreachable: {label} failed at the network level ({}); {n} modules resolved from go.sum only",
                    class.as_str()
                )
            })
            .collect();
        let budget = self
            .budget
            .filter(|(_, n)| *n > 0)
            .map(|(d, n)| {
                format!(
                    "proxy-fetch-budget-exhausted: {} spent; {n} modules not attempted, resolved from go.sum only",
                    budget_text(d)
                )
            });
        let reasons: Vec<String> = breaker.iter().cloned().chain(budget.clone()).collect();
        if reasons.is_empty() {
            return None;
        }
        let reason = reasons.join("; ");
        Some(if breaker.is_empty() {
            GoTransitiveCoverage::Partial(reason)
        } else {
            GoTransitiveCoverage::Unknown(reason)
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    #[test]
    fn label_is_scheme_host_and_port_only() {
        assert_eq!(entry_label(&url("https://proxy.golang.org")), "https://proxy.golang.org");
        assert_eq!(
            entry_label(&url("https://user:secret@corp.example/goproxy/?x=1#f")),
            "https://corp.example"
        );
        assert_eq!(entry_label(&url("http://127.0.0.1:18853/")), "http://127.0.0.1:18853");
        let l = entry_label(&url("https://user:secret@corp.example:8443/p"));
        assert_eq!(l, "https://corp.example:8443");
        assert!(!l.contains("secret") && !l.contains("user"));
    }

    #[test]
    fn redact_userinfo_drops_credentials_only() {
        assert_eq!(redact_userinfo("https://u:s3cret@corp.example/p"), "https://corp.example/p");
        assert_eq!(redact_userinfo("https://corp.example/p@v1"), "https://corp.example/p@v1");
        assert_eq!(redact_userinfo("https://u:p@w@h:8080/"), "https://h:8080/");
        assert_eq!(redact_userinfo("not a url"), "not a url");
        assert_eq!(redact_userinfo("https://u:s3cret@[bad"), "https://[bad");
    }

    #[test]
    fn trips_after_threshold_network_failures_without_a_response() {
        let b = ProxyFetchBounds::new(3, DEFAULT_PROXY_FETCH_BUDGET);
        for _ in 0..2 {
            assert_eq!(b.gate(0, "http://x"), EntryGate::Send);
            b.record_network_failure(0, "http://x", ErrorClass::Connection);
        }
        assert_eq!(b.gate(0, "http://x"), EntryGate::Send);
        b.record_network_failure(0, "http://x", ErrorClass::Connection);
        assert_eq!(b.gate(0, "http://x"), EntryGate::Skip(ErrorClass::Connection));
    }

    #[test]
    fn any_response_means_never_trip() {
        let b = ProxyFetchBounds::new(2, DEFAULT_PROXY_FETCH_BUDGET);
        b.record_response(0, "http://x");
        for _ in 0..10 {
            b.record_network_failure(0, "http://x", ErrorClass::Timeout);
        }
        assert_eq!(b.gate(0, "http://x"), EntryGate::Send);
        assert!(b.outcome().breaker.is_empty());
    }

    #[test]
    fn budget_counts_modules_not_attempted() {
        let b = ProxyFetchBounds::new(16, Duration::ZERO);
        assert!(!b.admit());
        assert!(!b.admit());
        assert_eq!(b.outcome().budget, Some((Duration::ZERO, 2)));
    }

    #[test]
    fn no_effect_means_no_coverage_contribution() {
        let b = ProxyFetchBounds::new(16, DEFAULT_PROXY_FETCH_BUDGET);
        assert!(b.admit());
        assert_eq!(b.outcome().coverage(), None);
    }

    #[test]
    fn coverage_strings_match_the_contract() {
        let breaker_only = BoundOutcome {
            breaker: vec![("http://10.255.255.1".into(), ErrorClass::Timeout, 64)],
            budget: None,
        };
        assert_eq!(
            breaker_only.coverage(),
            Some(GoTransitiveCoverage::Unknown(
                "proxy-unreachable: http://10.255.255.1 failed at the network level (timeout); 64 modules resolved from go.sum only".into()
            ))
        );
        let both = BoundOutcome {
            breaker: vec![("https://corp-proxy.example".into(), ErrorClass::Connection, 900)],
            budget: Some((Duration::from_secs(60), 212)),
        };
        assert_eq!(
            both.coverage(),
            Some(GoTransitiveCoverage::Unknown(
                "proxy-unreachable: https://corp-proxy.example failed at the network level (connection); 900 modules resolved from go.sum only; proxy-fetch-budget-exhausted: 60s spent; 212 modules not attempted, resolved from go.sum only".into()
            ))
        );
        let budget_only = BoundOutcome {
            breaker: vec![],
            budget: Some((Duration::from_millis(500), 7)),
        };
        assert_eq!(
            budget_only.coverage(),
            Some(GoTransitiveCoverage::Partial(
                "proxy-fetch-budget-exhausted: 500ms spent; 7 modules not attempted, resolved from go.sum only".into()
            ))
        );
        let tripped_but_nothing_lost = BoundOutcome {
            breaker: vec![("http://a".into(), ErrorClass::Connection, 0)],
            budget: None,
        };
        assert_eq!(tripped_but_nothing_lost.coverage(), None);
    }
}
