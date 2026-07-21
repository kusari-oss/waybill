// Milestone 055 — Step 3 of the resolution ladder: HTTP fetch of a
// module's `go.mod` from the Go module proxy (`$GOPROXY`).
//
// Endpoint format (per <https://proxy.golang.org/>):
//   <proxy>/<escaped-mod-path>/@v/<escaped-version>.mod
//
// where escape rules are Go's:
//   - lowercase ASCII / digits / `.-_~/+` pass through
//   - uppercase ASCII X → `!x`
//   - other bytes are an error
//
// **Sync, not async** — implementation discovery in T020 confirmed
// `golang::legacy::read()` is sync and called from a sync chain
// (`scan_fs::scan_path` → `package_db::read_all` → `golang::read`),
// so an async resolver would require runtime gymnastics
// (`block_in_place`, fresh-runtime fallback) that brittle the test
// harness. Path A from the implementation checkpoint: pure-sync via
// `reqwest::blocking::Client` (workspace `blocking` feature) and
// `std::thread` worker pool. Spec FR-008 (10 s connect / 30 s total
// timeouts) and FR-008a (16-way concurrency) are satisfied identically.
//
// See specs/055-go-transitive-edges/research.md R6 for the escape
// algorithm derivation.

use reqwest::Url;

use crate::scan_fs::package_db::golang::goprivate::{ProxyChain, ProxyEntry};
use crate::scan_fs::package_db::golang::graph_resolver::{
    ErrorClass, GraphResolverConfig, StepError, StepResult,
};
use crate::scan_fs::package_db::golang::module_id::ModuleId;

// --------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum EscapeError {
    #[error("invalid byte 0x{byte:02x} at position {position} in module path/version")]
    InvalidByte { byte: u8, position: usize },
}

// --------------------------------------------------------------------
// Module-path escape
// --------------------------------------------------------------------

/// Escape a Go module path or version per the proxy protocol.
///
/// Per `cmd/go/internal/module/module.go::EscapePath`:
/// - lowercase ASCII letters, digits, and `.-_~/+` pass through
/// - uppercase ASCII letters become `!<lowercase>` (e.g., `Azure` → `!azure`)
/// - anything else is an error
pub fn escape_module_path(input: &str) -> Result<String, EscapeError> {
    let mut out = String::with_capacity(input.len());
    for (position, byte) in input.bytes().enumerate() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b'~' | b'/' | b'+' => {
                out.push(byte as char);
            }
            b'A'..=b'Z' => {
                out.push('!');
                out.push((byte + 32) as char); // ASCII shift to lowercase
            }
            _ => {
                return Err(EscapeError::InvalidByte { byte, position });
            }
        }
    }
    Ok(out)
}

/// Build the URL to fetch a module's `go.mod` from a proxy base URL.
///
/// Pure function — no I/O. Returns `Err` if either the path or the
/// version contains a byte not allowed by `escape_module_path`.
pub fn build_proxy_url(proxy_base: &Url, target: &ModuleId) -> Result<Url, EscapeError> {
    let path = escape_module_path(target.path())?;
    let version = escape_module_path(target.version())?;
    let suffix = format!("{path}/@v/{version}.mod");
    // Url::join treats a trailing `/` on the base as significant; we
    // want the suffix to extend the base path verbatim, so we manually
    // concatenate to avoid Url::join's path-replacement semantics for
    // bases without a trailing slash.
    let base_str = proxy_base.as_str().trim_end_matches('/');
    let combined = format!("{base_str}/{suffix}");
    Url::parse(&combined).map_err(|_| EscapeError::InvalidByte {
        byte: 0,
        position: 0,
    })
}

// --------------------------------------------------------------------
// Fetcher (T020 — sync via reqwest::blocking)
// --------------------------------------------------------------------

/// Fetch a module's `go.mod` body from the proxy chain (synchronous).
///
/// Walks the `proxy_chain` per spec FR-004 separator semantics:
/// - `,` between entries: fall through on HTTP 404/410 only.
/// - `|` between entries: fall through on any error.
/// - `Direct` or `Off` short-circuit the walk.
///
/// Returns `StepResult::Unavailable` if the chain is empty / Off /
/// Direct-only. Returns `StepResult::Ok(body)` on the first successful
/// fetch. Returns `StepResult::Failed` only when the WHOLE chain has
/// been walked without a successful fetch.
pub fn fetch_module_mod(
    client: &reqwest::blocking::Client,
    proxy_chain: &ProxyChain,
    target: &ModuleId,
) -> StepResult<String> {
    if proxy_chain.is_empty() || proxy_chain.is_off() {
        return StepResult::Unavailable;
    }

    let mut last_error: Option<StepError> = None;

    for entry in proxy_chain.iter() {
        match entry {
            ProxyEntry::Off => return StepResult::Unavailable,
            ProxyEntry::Direct => {
                // Direct (source-VCS) is out of scope for 055; it
                // terminates the chain. If we got here without a
                // successful fetch, surface the last_error or
                // Unavailable.
                return last_error
                    .map(StepResult::Failed)
                    .unwrap_or(StepResult::Unavailable);
            }
            ProxyEntry::Url {
                url,
                fall_through_on_404_only,
            } => {
                let target_url = match build_proxy_url(url, target) {
                    Ok(u) => u,
                    Err(e) => {
                        return StepResult::Failed(StepError {
                            class: ErrorClass::Other,
                            detail: format!("escape error for {target}: {e}"),
                        });
                    }
                };

                match client.get(target_url.clone()).send() {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            match resp.text() {
                                Ok(body) => return StepResult::Ok(body),
                                Err(e) => {
                                    last_error = Some(StepError {
                                        class: ErrorClass::Parse,
                                        detail: format!(
                                            "body read failed for {target}: {e}"
                                        ),
                                    });
                                    if !*fall_through_on_404_only {
                                        // Pipe-separator: fall through on any error.
                                        continue;
                                    }
                                    return StepResult::Failed(
                                        last_error.expect("just set"),
                                    );
                                }
                            }
                        } else {
                            let class = if status.as_u16() == 404 || status.as_u16() == 410 {
                                ErrorClass::Http404
                            } else if status.is_client_error() {
                                ErrorClass::Http4xx
                            } else if status.is_server_error() {
                                ErrorClass::Http5xx
                            } else {
                                ErrorClass::Other
                            };
                            last_error = Some(StepError {
                                class,
                                detail: format!(
                                    "{target} from {url}: HTTP {status}"
                                ),
                            });
                            // Fall-through rules: comma → only on 404/410;
                            // pipe → on any error.
                            let is_404 = matches!(class, ErrorClass::Http404);
                            if *fall_through_on_404_only && !is_404 {
                                return StepResult::Failed(
                                    last_error.expect("just set"),
                                );
                            }
                            // Otherwise loop to next entry.
                        }
                    }
                    Err(e) => {
                        let class = classify_reqwest_error(&e);
                        last_error = Some(StepError {
                            class,
                            detail: format!("{target} from {url}: {e}"),
                        });
                        // Comma-separator: do NOT fall through on
                        // network errors (only 404). Pipe-separator: do.
                        if *fall_through_on_404_only {
                            return StepResult::Failed(last_error.expect("just set"));
                        }
                    }
                }
            }
        }
    }

    last_error
        .map(StepResult::Failed)
        .unwrap_or(StepResult::Unavailable)
}

fn classify_reqwest_error(err: &reqwest::Error) -> ErrorClass {
    if err.is_timeout() {
        ErrorClass::Timeout
    } else if err.is_connect() {
        ErrorClass::Connection
    } else if err.is_decode() || err.is_body() {
        ErrorClass::Parse
    } else {
        // Best-effort string sniffing for DNS / TLS — reqwest doesn't
        // expose typed predicates for these in 0.12.
        let s = err.to_string().to_lowercase();
        if s.contains("dns") || s.contains("name resolution") {
            ErrorClass::Dns
        } else if s.contains("tls") || s.contains("certificate") || s.contains("handshake") {
            ErrorClass::Tls
        } else {
            ErrorClass::Other
        }
    }
}

/// Build a `reqwest::blocking::Client` configured with the spec FR-008 timeouts.
pub fn build_http_client(
    config: &GraphResolverConfig,
) -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(config.fetch_connect_timeout)
        .timeout(config.fetch_total_timeout)
        // Default no proxy — we manage `$GOPROXY` chain ourselves.
        .no_proxy()
        .user_agent(concat!(
            "mikebom/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/Kusari-OSS/mikebom; +go-transitive-resolver)"
        ))
        .build()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // --- escape_module_path (T011 / T015) ---

    #[test]
    fn escape_lowercase_passes_through() {
        assert_eq!(
            escape_module_path("github.com/foo/bar").unwrap(),
            "github.com/foo/bar"
        );
        assert_eq!(escape_module_path("gopkg.in/yaml.v2").unwrap(), "gopkg.in/yaml.v2");
    }

    #[test]
    fn escape_uppercase_letters_lowercase_with_bang() {
        // Documented Go example: github.com/Azure/...
        assert_eq!(
            escape_module_path("github.com/Azure/azure-sdk-for-go").unwrap(),
            "github.com/!azure/azure-sdk-for-go"
        );
        // Multiple uppercase chars in one segment.
        assert_eq!(
            escape_module_path("github.com/SAP/go-hdb").unwrap(),
            "github.com/!s!a!p/go-hdb"
        );
    }

    #[test]
    fn escape_digits_and_punctuation_pass_through() {
        assert_eq!(
            escape_module_path("github.com/user-name_x/repo+v2.0.0").unwrap(),
            "github.com/user-name_x/repo+v2.0.0"
        );
        assert_eq!(
            escape_module_path("v0.0.0-20211123-abcd1234abcd").unwrap(),
            "v0.0.0-20211123-abcd1234abcd"
        );
    }

    #[test]
    fn escape_returns_err_on_disallowed_byte() {
        assert_eq!(
            escape_module_path("github.com/has space/repo"),
            Err(EscapeError::InvalidByte {
                byte: b' ',
                position: 14,
            })
        );
        assert_eq!(
            escape_module_path("?"),
            Err(EscapeError::InvalidByte {
                byte: b'?',
                position: 0,
            })
        );
        let m = "github.com/föo/bar";
        let err = escape_module_path(m).unwrap_err();
        assert!(matches!(err, EscapeError::InvalidByte { .. }));
    }

    #[test]
    fn build_proxy_url_documented_example() {
        // R6: github.com/Azure/azure-sdk-for-go @ v1.2.3 against
        // https://proxy.golang.org →
        // https://proxy.golang.org/github.com/!azure/azure-sdk-for-go/@v/v1.2.3.mod
        let proxy = Url::parse("https://proxy.golang.org").unwrap();
        let target = ModuleId::new("github.com/Azure/azure-sdk-for-go", "v1.2.3");
        let u = build_proxy_url(&proxy, &target).unwrap();
        assert_eq!(
            u.as_str(),
            "https://proxy.golang.org/github.com/!azure/azure-sdk-for-go/@v/v1.2.3.mod"
        );
    }

    #[test]
    fn build_proxy_url_with_trailing_slash_base() {
        let proxy = Url::parse("https://proxy.golang.org/").unwrap();
        let target = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let u = build_proxy_url(&proxy, &target).unwrap();
        assert_eq!(
            u.as_str(),
            "https://proxy.golang.org/github.com/foo/bar/@v/v1.0.0.mod"
        );
    }

    #[test]
    fn http_client_builds_with_default_config() {
        // Verifies the FR-008 timeouts are accepted by reqwest's
        // builder; functional behavior covered by T014's wiremock test.
        let cfg = GraphResolverConfig::default();
        let client = build_http_client(&cfg).expect("client builds");
        drop(client);
    }

    // --- fetch_module_mod fallthrough (T014 — proxy chain semantics
    // without a real HTTP server). Wiremock-backed end-to-end fetch
    // coverage lives in mikebom-cli/tests/go_transitive_edges.rs (T027).

    #[test]
    fn fetch_returns_unavailable_for_off_chain() {
        let chain = ProxyChain {
            entries: vec![ProxyEntry::Off],
        };
        let client = reqwest::blocking::Client::new();
        let target = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let r = fetch_module_mod(&client, &chain, &target);
        assert!(matches!(r, StepResult::Unavailable));
    }

    #[test]
    fn fetch_returns_unavailable_for_direct_only_chain() {
        let chain = ProxyChain {
            entries: vec![ProxyEntry::Direct],
        };
        let client = reqwest::blocking::Client::new();
        let target = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let r = fetch_module_mod(&client, &chain, &target);
        assert!(matches!(r, StepResult::Unavailable));
    }

    #[test]
    fn fetch_returns_unavailable_for_empty_chain() {
        let chain = ProxyChain { entries: vec![] };
        let client = reqwest::blocking::Client::new();
        let target = ModuleId::new("github.com/foo/bar", "v1.0.0");
        let r = fetch_module_mod(&client, &chain, &target);
        assert!(matches!(r, StepResult::Unavailable));
    }
}
