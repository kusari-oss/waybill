#[allow(unused_imports)]
use crate::corpus_harness_195::harness::{AssertionFailure, EmittedSboms, FailureFormat};
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::EmittedPaths;
use crate::corpus_harness_195::expectations::*;
use serde_json::json;

/// One CPE hit (openssl, through a candidate CPE), one purl hit (aeson,
/// nested and carrying qualifiers), one miss (zlib, present only with
/// `scope: excluded`).
const TRUTH: &str = r#"{
  "schema": 1,
  "produced": "2026-11-11",
  "tools": {"grype": "0.120.0"},
  "target-pin": "7b46273d3723ba37da26e4c4f7a3c5575ae9e837",
  "findings": [
    {"id": "CVE-2026-0001", "name": "openssl", "version": "3.6.0", "matcher": {"cpe": "openssl:openssl"}},
    {"id": "HSEC-2026-0007", "name": "aeson", "version": "2.2.3.0", "matcher": {"purl": "pkg:hackage/aeson@2.2.3.0"}},
    {"id": "CVE-2026-0002", "name": "zlib", "version": "1.3.1", "matcher": {"cpe": "zlib:zlib"}}
  ],
  "excluded": []
}"#;

fn cdx() -> serde_json::Value {
    json!({
        "bomFormat": "CycloneDX",
        "components": [
            {"name": "openssl", "version": "3.6.0", "purl": "pkg:nix/openssl@3.6.0",
             "properties": [{"name": "waybill:cpe-candidates",
                             "value": "[\"cpe:2.3:a:openssl_project:openssl:3.6.0:*:*:*:*:*:*:*\",\"cpe:2.3:a:openssl:openssl:3.6.0:*:*:*:*:*:*:*\"]"}]},
            {"name": "slack-web", "version": "2.3.0.0", "purl": "pkg:hackage/slack-web@2.3.0.0",
             "components": [
                 {"name": "aeson", "version": "2.2.3.0",
                  "purl": "pkg:hackage/aeson@2.2.3.0?repository_url=https://hackage.haskell.org"}
             ]},
            {"name": "zlib", "version": "1.3.1", "scope": "excluded",
             "cpe": "cpe:2.3:a:zlib:zlib:1.3.1:*:*:*:*:*:*:*"}
        ]
    })
}

fn truth_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("recall-truth.json");
    std::fs::write(&p, TRUTH).unwrap();
    (d, p)
}

#[test]
fn three_finding_truth_recalls_two_of_three() {
    let (_d, p) = truth_file();
    let truth: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let o = recall_proxy(&cdx(), &truth).unwrap();
    assert_eq!((o.recalled, o.total), (2, 3), "{o:?}");
    assert!((o.ratio() - 0.667).abs() < 0.001, "{}", o.ratio());
    assert_eq!(o.misses.len(), 1, "{o:?}");
    assert!(o.misses[0].starts_with("CVE-2026-0002 zlib@1.3.1"), "{}", o.misses[0]);
    assert!(o.misses[0].contains("scope=excluded"), "{}", o.misses[0]);
}

#[test]
fn below_the_floor_fails_and_lists_the_miss() {
    let (_d, p) = truth_file();
    let err = check_recall(&cdx(), "nix-closure-slack-web", "7b46273d3723ba37da26e4c4f7a3c5575ae9e837", &p)
        .unwrap_err();
    assert_eq!(err.check_name, "1283-runtime-recall-floor");
    assert!(err.observed.contains("recalled 2 of 3"), "{}", err.observed);
    assert!(err.observed.contains("CVE-2026-0002 zlib@1.3.1"), "{}", err.observed);
}

#[test]
fn at_the_floor_passes() {
    // Ten findings, nine of them the openssl one: 9/10 is exactly 0.90.
    let mut t: serde_json::Value = serde_json::from_str(TRUTH).unwrap();
    let hit = t["findings"][0].clone();
    let miss = t["findings"][2].clone();
    let mut fs = vec![miss];
    fs.extend(std::iter::repeat_n(hit, 9));
    t["findings"] = json!(fs);
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("recall-truth.json");
    std::fs::write(&p, t.to_string()).unwrap();
    check_recall(&cdx(), "nix-closure-slack-web", "7b46273d3723ba37da26e4c4f7a3c5575ae9e837", &p).unwrap();
}

#[test]
fn an_absent_truth_file_fails_naming_the_dispatch() {
    let d = tempfile::tempdir().unwrap();
    let err = check_recall(&cdx(), "nix-closure-openai", "x", &d.path().join("recall-truth.json"))
        .unwrap_err();
    assert_eq!(err.check_name, "1283-recall-truth-present");
    assert!(
        err.expected.contains("gh workflow run recall-truth.yml")
            && err.expected.contains("-f targets=nix-closure-openai"),
        "{}",
        err.expected
    );
}

#[test]
fn truth_for_another_commit_fails() {
    let (_d, p) = truth_file();
    let err = check_recall(&cdx(), "nix-closure-slack-web", "0000000000000000000000000000000000000000", &p)
        .unwrap_err();
    assert_eq!(err.check_name, "1283-recall-truth-matches-pin");
}

#[test]
fn an_empty_truth_is_an_error_not_a_pass() {
    let t = json!({"schema": 1, "target-pin": "x", "findings": []});
    assert!(recall_proxy(&cdx(), &t).unwrap_err().contains("empty"));
}

#[test]
fn the_cpe_matcher_needs_name_and_version_but_the_purl_matcher_does_not() {
    let t = |m: serde_json::Value, name: &str, version: &str| {
        json!({"schema": 1, "findings": [{"id": "X", "name": name, "version": version, "matcher": m}]})
    };
    let c = cdx();
    // Right vendor:product, wrong version: no.
    let o = recall_proxy(&c, &t(json!({"cpe": "openssl:openssl"}), "openssl", "3.5.0")).unwrap();
    assert_eq!(o.recalled, 0);
    // Right name and version, wrong vendor:product: no.
    let o = recall_proxy(&c, &t(json!({"cpe": "zlib:zlib"}), "openssl", "3.6.0")).unwrap();
    assert_eq!(o.recalled, 0);
    // Purl decides, whatever the finding's name says.
    let o = recall_proxy(&c, &t(json!({"purl": "pkg:hackage/aeson@2.2.3.0"}), "Aeson-renamed", "0")).unwrap();
    assert_eq!(o.recalled, 1);
    // A list matcher: any member matches.
    let o = recall_proxy(&c, &t(json!({"cpe": ["nope:nope", "openssl:openssl"]}), "openssl", "3.6.0")).unwrap();
    assert_eq!(o.recalled, 1);
}

#[test]
fn names_normalize_like_classify_py() {
    assert_eq!(recall_norm_name("Python3.12-Requests"), "requests");
    assert_eq!(recall_norm_name("python3-six"), "six");
    assert_eq!(recall_norm_name("python3x-six"), "python3x-six");
    assert_eq!(recall_norm_name("python3.-six"), "python3.-six");
    assert_eq!(recall_norm_name("OpenSSL"), "openssl");
}

#[test]
fn document_checks_need_c217_allowed_verified_and_no_c190() {
    let doc = |props: serde_json::Value| json!({"metadata": {"properties": props}, "components": []});
    let c216 = json!({"name": "waybill:closure-scope-method", "value": "{\"method\":\"runtime-closure\"}"});
    let c217 = |ifd: &str, verified: bool| {
        json!({"name": "waybill:nix-evaluation-policy",
               "value": json!({"import-from-derivation": ifd, "sandbox": "true", "schema": 1, "verified": verified}).to_string()})
    };
    ifd_document_checks(&doc(json!([c216, c217("allowed", true)]))).unwrap();
    assert_eq!(
        ifd_document_checks(&doc(json!([c216, c217("refused", true)]))).unwrap_err().check_name,
        "1283-ifd-allowed-and-verified"
    );
    assert_eq!(
        ifd_document_checks(&doc(json!([c216, c217("allowed", false)]))).unwrap_err().check_name,
        "1283-ifd-allowed-and-verified"
    );
    assert_eq!(
        ifd_document_checks(&doc(json!([c217("allowed", true)]))).unwrap_err().check_name,
        "1283-scope-method-present"
    );
    let c190 = json!({"name": "waybill:nix-closure-degraded",
                      "value": "{\"reason\":\"import-from-derivation-required\"}"});
    assert_eq!(
        ifd_document_checks(&doc(json!([c216, c217("allowed", true), c190]))).unwrap_err().check_name,
        "1283-no-nix-degradation"
    );
}
