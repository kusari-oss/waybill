//! Milestone 1065 (#853) — the Go proxy-fetch bounds, end to end.
//!
//! Every scan here reaches only local servers or closed ports. `go` is kept
//! off PATH so ladder step 1 cannot do its own network work, and the module
//! cache is empty, so step 3 (proxy fetch) is the only tier with work to do.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const COVERAGE: &str = "waybill:go-transitive-coverage";
const REASON: &str = "waybill:go-transitive-coverage-reason";

/// A Go module whose go.sum lists `n` modules nothing can serve.
fn repo(n: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut go_mod = String::from("module example.com/probe1065\n\ngo 1.22\n\nrequire (\n");
    let mut go_sum = String::new();
    for i in 0..n {
        let m = format!("example.com/bounds1065/m{i}");
        go_mod.push_str(&format!("\t{m} v1.0.0\n"));
        go_sum.push_str(&format!(
            "{m} v1.0.0 h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n{m} v1.0.0/go.mod h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n"
        ));
    }
    go_mod.push_str(")\n");
    std::fs::write(dir.path().join("go.mod"), go_mod).unwrap();
    std::fs::write(dir.path().join("go.sum"), go_sum).unwrap();
    std::fs::write(dir.path().join("main.go"), "package main\nfunc main() {}\n").unwrap();
    dir
}

fn closed_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap().to_string();
    drop(l);
    a
}

/// Answers 404 to everything after `delay`.
fn http_404_server(delay: Duration) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                std::thread::sleep(delay);
                let _ = s.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            });
        }
    });
    a
}

struct Scan {
    cdx: serde_json::Value,
    spdx23: serde_json::Value,
    spdx3: serde_json::Value,
    stderr: String,
    raw: String,
}

fn scan(root: &Path, goproxy: &str, extra_env: &[(&str, &str)], extra_args: &[&str]) -> Scan {
    let out = tempfile::tempdir().unwrap();
    let empty_path = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let modcache = home.path().join("modcache");
    std::fs::create_dir_all(&modcache).unwrap();
    let p = |f: &str| out.path().join(f);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    cmd.env_clear()
        .env("PATH", empty_path.path())
        .env("HOME", home.path())
        .env("GOMODCACHE", &modcache)
        .env("GOPROXY", goproxy)
        .env("WAYBILL_NO_GO_MOD_WHY", "1");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let o = cmd
        .args(["sbom", "scan", "--no-deep-hash", "--no-deps-dev", "--path"])
        .arg(root)
        .args(["--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", p("o.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", p("o.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", p("o.spdx3.json").display()))
        .args(extra_args)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "scan failed: {stderr}");
    let read = |f: &str| -> (serde_json::Value, String) {
        let raw = std::fs::read_to_string(p(f)).unwrap();
        (serde_json::from_str(&raw).unwrap(), raw)
    };
    let (cdx, a) = read("o.cdx.json");
    let (spdx23, b) = read("o.spdx.json");
    let (spdx3, c) = read("o.spdx3.json");
    Scan { cdx, spdx23, spdx3, stderr, raw: format!("{a}\n{b}\n{c}") }
}

fn envelope_value(raw: &str, field: &str) -> Option<String> {
    let env: serde_json::Value = serde_json::from_str(raw).ok()?;
    if env["field"].as_str() == Some(field) {
        return env["value"].as_str().map(str::to_string);
    }
    None
}

/// The decoded document-scope value of `field` in each of the three formats.
fn doc_values(s: &Scan, field: &str) -> [Option<String>; 3] {
    let cdx = s.cdx["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some(field))
        .and_then(|p| p["value"].as_str().map(str::to_string));
    let spdx23 = s.spdx23["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|a| envelope_value(a["comment"].as_str()?, field));
    let spdx3 = s.spdx3["@graph"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|e| envelope_value(e["statement"].as_str()?, field));
    [cdx, spdx23, spdx3]
}

fn all_three(s: &Scan, field: &str) -> Option<String> {
    let [a, b, c] = doc_values(s, field);
    assert_eq!(a, b, "{field}: CycloneDX vs SPDX 2.3");
    assert_eq!(a, c, "{field}: CycloneDX vs SPDX 3");
    a
}

fn go_components(s: &Scan) -> Vec<&serde_json::Value> {
    s.cdx["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:golang/example.com/bounds1065/"))
        })
        .collect()
}

fn property<'a>(c: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))?["value"]
        .as_str()
}

fn property_names(s: &Scan) -> std::collections::BTreeSet<String> {
    go_components(s)
        .iter()
        .flat_map(|c| c["properties"].as_array().cloned().unwrap_or_default())
        .filter_map(|p| p["name"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn t010_unreachable_proxy_is_reported_and_loses_nothing() {
    let r = repo(40);
    let port = closed_port();
    let s = scan(r.path(), &format!("http://{port}"), &[], &[]);

    assert_eq!(all_three(&s, COVERAGE).as_deref(), Some("unknown"));
    assert_eq!(
        all_three(&s, REASON),
        Some(format!(
            "proxy-unreachable: http://{port} failed at the network level (connection); 40 modules resolved from go.sum only"
        ))
    );
    let go = go_components(&s);
    assert_eq!(go.len(), 40, "no Go component may be lost (FR-005)");
    for c in &go {
        assert_eq!(property(c, "waybill:go-transitive-source"), Some("go-sum-fallback"));
    }
    assert!(s.stderr.contains("Go module proxy unreachable"), "FR-008 warning missing");

    // FR-005: no new per-component annotation. Compare with a scan that
    // never fetches at all.
    let off = scan(r.path(), "off", &[], &[]);
    assert_eq!(property_names(&s), property_names(&off));
}

#[test]
fn t010e_credentials_never_appear() {
    let r = repo(20);
    let port = closed_port();
    let s = scan(r.path(), &format!("http://user1065:secret1065@{port}"), &[], &[]);
    // This feature's outputs only: the documents and its own warning.
    // Pre-existing log lines that print the raw GOPROXY URL are #1110.
    let own_warnings: Vec<&str> = s
        .stderr
        .lines()
        .filter(|l| l.contains("Go module proxy unreachable"))
        .collect();
    assert!(!own_warnings.is_empty(), "FR-008 warning missing");
    for needle in ["secret1065", "user1065"] {
        assert!(!s.raw.contains(needle), "{needle} in an emitted document");
        for l in &own_warnings {
            assert!(!l.contains(needle), "{needle} in the FR-008 warning: {l}");
        }
    }
    assert!(all_three(&s, REASON).unwrap().starts_with(&format!(
        "proxy-unreachable: http://{port} failed"
    )));
}

#[test]
fn t016_budget_exhaustion_is_partial_in_every_format() {
    let r = repo(160);
    let addr = http_404_server(Duration::from_millis(200));
    let s = scan(
        r.path(),
        &format!("http://{addr}"),
        &[("WAYBILL_GO_PROXY_FETCH_BUDGET_MS", "500")],
        &[],
    );
    assert_eq!(all_three(&s, COVERAGE).as_deref(), Some("partial"));
    let reason = all_three(&s, REASON).unwrap();
    assert!(reason.starts_with("proxy-fetch-budget-exhausted: 500ms spent; "), "{reason}");
    assert_eq!(go_components(&s).len(), 160);
}

#[test]
fn t021_ordinary_404s_are_reported_as_before() {
    let r = repo(40);
    let addr = http_404_server(Duration::ZERO);
    let s = scan(r.path(), &format!("http://{addr}"), &[], &[]);
    assert_eq!(all_three(&s, COVERAGE).as_deref(), Some("complete"));
    assert_eq!(all_three(&s, REASON), None);
    assert_eq!(
        all_three(&s, "waybill:go-transitive-fallback-count").as_deref(),
        Some("40")
    );
    assert!(!s.stderr.contains("Go module proxy unreachable"));
}

#[test]
fn t022_bounds_stay_out_of_disabled_fetching() {
    let r = repo(20);
    let s = scan(
        r.path(),
        &format!("http://{}", closed_port()),
        &[],
        &["--no-go-proxy-fetch"],
    );
    assert_eq!(all_three(&s, REASON), None);
    assert!(!s.stderr.contains("Go module proxy unreachable"));
}

