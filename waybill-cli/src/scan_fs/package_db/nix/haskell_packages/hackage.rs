//! #1046 — asking Hackage whether `<name>` `<version>` is published.
//!
//! One `GET /package/<name>-<version>/<name>.cabal` per question. Measured
//! (`specs/1046-haskell-overlay-identity/measurements/hackage-probe-*.txt`):
//! 200 for a published version, 404 for an unpublished one or an unknown
//! name, names case-sensitive, 213–420 ms per request, and 20 sequential
//! requests unthrottled.
//!
//! Only a GHC-built closure member whose name the pinned nixpkgs set carries,
//! at a version it does not, is ever asked about (spec FR-006a): a name the
//! set does not carry may be a private package, and is never sent.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

/// Overrides the Hackage base URL. A test seam: integration tests set it on
/// the spawned `Command` to a local stub or a closed port, never with
/// `std::env::set_var` (the race #1493 fixed).
pub(crate) const BASE_URL_ENV: &str = "WAYBILL_HACKAGE_URL";
const DEFAULT_BASE_URL: &str = "https://hackage.haskell.org";

/// What Hackage answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Published {
    Yes,
    No,
    /// No answer: transport failure, timeout, or a status other than 200 or
    /// 404. Carries the detail for the log.
    Unreachable(String),
}

/// A source of "is this published" answers. A trait so tests inject one with
/// no network (Principle VII).
pub(crate) trait HackageSource {
    fn published(&self, name: &str, version: &str) -> Published;
}

/// Hackage package names are letters, digits and hyphens; versions are
/// dot-separated digits. Anything else cannot be on Hackage, and is never
/// interpolated into a URL.
fn hackage_shaped(name: &str, version: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !version.is_empty()
        && version.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Production source: one bounded HTTPS GET per question, memoized for the
/// scan.
pub(crate) struct HttpHackage {
    base: String,
    timeout: Duration,
    memo: RefCell<HashMap<(String, String), Published>>,
}

impl HttpHackage {
    /// The base URL comes from [`BASE_URL_ENV`] when set, else Hackage.
    pub(crate) fn new(timeout_secs: u64) -> Self {
        let base = std::env::var(BASE_URL_ENV)
            .ok()
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self::with_base(base, timeout_secs)
    }

    pub(crate) fn with_base(base: impl Into<String>, timeout_secs: u64) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            timeout: Duration::from_secs(timeout_secs),
            memo: RefCell::new(HashMap::new()),
        }
    }

    fn ask(&self, name: &str, version: &str) -> Published {
        if !hackage_shaped(name, version) {
            return Published::Unreachable(format!("not a Hackage-shaped name or version: {name} {version}"));
        }
        let url = format!("{}/package/{name}-{version}/{name}.cabal", self.base);
        let timeout = self.timeout;
        // A dedicated thread, as `fetch::HttpSource` does: `reqwest::blocking`
        // builds its own runtime, and dropping it inside the scan's async
        // context panics.
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let client = match reqwest::blocking::Client::builder().timeout(timeout).build() {
                        Ok(c) => c,
                        Err(e) => return Published::Unreachable(e.to_string()),
                    };
                    match client.get(&url).send() {
                        Ok(resp) => match resp.status().as_u16() {
                            200 => Published::Yes,
                            404 => Published::No,
                            other => Published::Unreachable(format!("HTTP {other}")),
                        },
                        Err(e) if e.is_timeout() => Published::Unreachable("timed out".to_string()),
                        Err(e) => Published::Unreachable(e.to_string()),
                    }
                })
                .join()
                .unwrap_or_else(|_| Published::Unreachable("lookup thread panicked".to_string()))
        })
    }
}

impl HackageSource for HttpHackage {
    fn published(&self, name: &str, version: &str) -> Published {
        let key = (name.to_string(), version.to_string());
        if let Some(hit) = self.memo.borrow().get(&key) {
            return hit.clone();
        }
        let answer = self.ask(name, version);
        self.memo.borrow_mut().insert(key, answer.clone());
        answer
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server(status: u16, at: &str) -> MockServer {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(status))
            .mount(&s)
            .await;
        s
    }

    /// Run a blocking lookup off the async test's thread.
    async fn lookup(base: String, name: &'static str, version: &'static str) -> Published {
        tokio::task::spawn_blocking(move || HttpHackage::with_base(base, 5).published(name, version))
            .await
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_200_is_published_and_a_404_is_not() {
        let s = server(200, "/package/waybill-fixture-a-2.3.0.0/waybill-fixture-a.cabal").await;
        assert_eq!(lookup(s.uri(), "waybill-fixture-a", "2.3.0.0").await, Published::Yes);
        let s = server(404, "/package/waybill-fixture-a-2.3.0.1/waybill-fixture-a.cabal").await;
        assert_eq!(lookup(s.uri(), "waybill-fixture-a", "2.3.0.1").await, Published::No);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn any_other_status_is_unreachable() {
        let s = server(503, "/package/waybill-fixture-a-1.0/waybill-fixture-a.cabal").await;
        assert!(matches!(lookup(s.uri(), "waybill-fixture-a", "1.0").await, Published::Unreachable(_)));
    }

    /// Names are case-sensitive on Hackage: the request uses the name as given.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_name_keeps_its_case() {
        let s = server(200, "/package/Waybill-Fixture-A-1.0/Waybill-Fixture-A.cabal").await;
        assert_eq!(lookup(s.uri(), "Waybill-Fixture-A", "1.0").await, Published::Yes);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_memo_asks_once() {
        let s = server(200, "/package/waybill-fixture-a-1.0/waybill-fixture-a.cabal").await;
        let base = s.uri();
        tokio::task::spawn_blocking(move || {
            let h = HttpHackage::with_base(base, 5);
            assert_eq!(h.published("waybill-fixture-a", "1.0"), Published::Yes);
            assert_eq!(h.published("waybill-fixture-a", "1.0"), Published::Yes);
        })
        .await
        .unwrap();
        assert_eq!(s.received_requests().await.unwrap().len(), 1);
    }

    /// A closed port answers nothing, quickly.
    #[test]
    fn an_unreachable_host_is_unreachable() {
        let h = HttpHackage::with_base("http://127.0.0.1:9", 5);
        assert!(matches!(h.published("waybill-fixture-a", "1.0"), Published::Unreachable(_)));
    }

    /// A name or version that cannot be on Hackage is never sent.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_non_hackage_shape_is_never_requested() {
        let s = MockServer::start().await;
        for (n, v) in [("../etc", "1.0"), ("waybill-fixture-a", "1.0/x"), ("waybill-fixture-a", "")] {
            assert!(matches!(lookup(s.uri(), n, v).await, Published::Unreachable(_)), "{n} {v}");
        }
        assert_eq!(s.received_requests().await.unwrap().len(), 0);
    }
}
