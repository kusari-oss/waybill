//! Spec 1640 (#1647): a URL that a scanned project declares as a place to
//! fetch from reaches the SBOM without its userinfo.
//!
//! Contract `specs/1640-strip-url-credentials/contracts/wire.md`. Every
//! fixture is a template whose `{CRED}` is either the credential
//! `wbfxuser:wbfxtoken7c41@` or nothing. A D-case scans the credentialled
//! variant and checks:
//! - neither string occurs in the three documents or in the log, captured at
//!   `trace`;
//! - the field still carries the clean URL (the control on the branch);
//! - the redacted warning names the fixture's manifest;
//! - the formats agree, CycloneDX validates, `bom-ref`s are unique, and 10
//!   scans are byte-identical.

#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::cmake_fetch::{cdx_components, prop, ROOT};
use common::git_fixture::GitFixture;
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const USER: &str = "wbfxuser";
const TOKEN: &str = "wbfxtoken7c41";
const CRED: &str = "wbfxuser:wbfxtoken7c41@";
const WARNING: &str = "stripped credentials from URL";
const REPEATS: usize = 10;
/// Rows whose formats must agree on every fixture (FR-007): PURL, the
/// VCS and distribution references, and the URL-bearing annotations.
const ROWS: &[&str] = &["A1", "A10", "A11", "C53", "C71", "C144", "C154"];

// ---------------------------------------------------------------------
// Harness (T002)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    /// The three documents as written, for byte searches.
    raw: [String; 3],
    /// Everything the scan logged, at `trace`.
    log: String,
}

/// A fixture tree: `files` written under `<fx>/tree`, `{CRED}` replaced by
/// `cred`, and the tree made a git repository.
fn tree(files: &[(&str, &str)], cred: &str) -> (GitFixture, PathBuf) {
    let fx = GitFixture::new();
    fx.repo("tree");
    for (rel, text) in files {
        fx.write(&format!("tree/{rel}"), &text.replace("{CRED}", cred));
    }
    let path = fx.path("tree");
    (fx, path)
}

/// The binary under test, or `WAYBILL_DECLARED_URL_BIN` when generating
/// C-ctl's golden from `main`'s binary.
fn binary() -> String {
    std::env::var("WAYBILL_DECLARED_URL_BIN").unwrap_or_else(|_| common::bin().to_string())
}

/// One scan in all three formats, with the log captured at `trace`.
fn scan3(fx: &GitFixture, path: &Path, extra: &[&str]) -> Scan {
    let out_dir = tempfile::tempdir().unwrap();
    let files = [
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    ];
    let out = Command::new(binary())
        .env("HOME", fx.home())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", fx.home().join(".gitconfig"))
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "trace")
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--no-deep-hash", "--root-name", "waybill-fixture-root", "--root-version", "1"])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", files[0].display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", files[1].display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", files[2].display()))
        .args(extra)
        .output()
        .unwrap();
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "scan failed: {log}");
    let raw = files.clone().map(|p| std::fs::read_to_string(p).unwrap());
    let parse = |s: &str| serde_json::from_str::<Value>(s).unwrap();
    Scan { cdx: parse(&raw[0]), spdx23: parse(&raw[1]), spdx3: parse(&raw[2]), raw, log }
}

/// SC-002: neither credential string occurs in any document or the log.
fn assert_no_cred(s: &Scan) {
    let named = [("CycloneDX", &s.raw[0]), ("SPDX 2.3", &s.raw[1]), ("SPDX 3", &s.raw[2]), ("log", &s.log)];
    for (name, text) in named {
        for needle in [USER, TOKEN] {
            assert!(!text.contains(needle), "{needle} occurs in the {name}");
        }
    }
}

/// FR-005: at least one warning, and every warning is redacted and names
/// the manifest (a reader may read one declaration twice, analysis U2).
fn assert_warned(s: &Scan, manifest: &str) {
    let lines: Vec<&str> = s.log.lines().filter(|l| l.contains(WARNING)).collect();
    assert!(!lines.is_empty(), "no `{WARNING}` warning in the log");
    for l in lines {
        assert!(l.contains("<userinfo redacted>") && l.contains(manifest), "warning without the redaction or {manifest}: {l}");
    }
}

fn assert_not_warned(s: &Scan) {
    let lines: Vec<&str> = s.log.lines().filter(|l| l.contains(WARNING)).collect();
    assert!(lines.is_empty(), "unexpected warnings: {lines:?}");
}

/// FR-007: the formats agree, CycloneDX validates, no `bom-ref` repeats.
fn parity(s: &Scan) {
    for row in ROWS {
        let e = waybill::parity::extractors::EXTRACTORS
            .iter()
            .find(|e| e.row_id == *row)
            .unwrap_or_else(|| panic!("no extractor for {row}"));
        let (a, b, c) = ((e.cdx)(&s.cdx), (e.spdx23)(&s.spdx23), (e.spdx3)(&s.spdx3));
        assert_eq!(a, b, "{row}: CycloneDX and SPDX 2.3 disagree");
        // #1653: the SPDX 3 walker skips `dataset_DatasetPackage`, so a
        // dataset's PURL is compared with SPDX 2.3 only.
        let datasets: std::collections::BTreeSet<String> = cdx_components(&s.cdx)
            .into_iter()
            .filter(|c| c["type"] == "data")
            .filter_map(|c| c["purl"].as_str().map(str::to_string))
            .collect();
        let a3: std::collections::BTreeSet<String> = if *row == "A1" { a.difference(&datasets).cloned().collect() } else { a.clone() };
        assert_eq!(a3, c, "{row}: CycloneDX and SPDX 3 disagree");
    }
    let errors = common::cdx_schema::cdx_validation_errors(&s.cdx);
    assert!(errors.is_empty(), "CycloneDX schema errors: {errors:?}");
    let mut refs: Vec<&str> = cdx_components(&s.cdx).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
    refs.extend(s.cdx["metadata"]["component"]["bom-ref"].as_str());
    let mut seen = std::collections::BTreeSet::new();
    let dups: Vec<&str> = refs.iter().copied().filter(|r| !seen.insert(*r)).collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
}

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r#"[^"]*?[/\\]tree(?:[/\\"])"#).unwrap();
    serde_json::from_str(&re.replace_all(&s, |c: &regex::Captures| {
        let m = c.get(0).unwrap().as_str();
        format!("/TMP/tree{}", &m[m.len() - 1..])
    }))
    .unwrap()
}

/// FR-008: repeat scans give the same masked output.
fn assert_repeatable(fx: &GitFixture, path: &Path, extra: &[&str], first: &Scan) {
    for _ in 1..REPEATS {
        let again = scan3(fx, path, extra);
        for (name, a, b) in [("cdx", &first.cdx, &again.cdx), ("spdx23", &first.spdx23, &again.spdx23), ("spdx3", &first.spdx3, &again.spdx3)] {
            let (a, b) = (masked(a), masked(b));
            assert!(a == b, "{name}: two scans differ; {}", masked_difference(&a, &b).unwrap_or_default());
        }
    }
}

/// A D-case: scan the credentialled tree and run every check. Returns the
/// scan for the field assertion.
fn d_case(files: &[(&str, &str)], manifest: &str) -> Scan {
    let (fx, path) = tree(files, CRED);
    let s = scan3(&fx, &path, &[]);
    assert_no_cred(&s);
    assert_warned(&s, manifest);
    parity(&s);
    assert_repeatable(&fx, &path, &[], &s);
    s
}

/// The components whose PURL starts with `prefix`.
fn with_purl<'a>(s: &'a Scan, prefix: &str) -> Vec<&'a Value> {
    cdx_components(&s.cdx).into_iter().filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(prefix))).collect()
}

fn one<'a>(s: &'a Scan, prefix: &str) -> &'a Value {
    let all = with_purl(s, prefix);
    assert_eq!(all.len(), 1, "expected one component {prefix}: {:?}", all.iter().map(|c| &c["purl"]).collect::<Vec<_>>());
    all[0]
}

/// The control on the branch: `text` occurs in the CycloneDX document, so
/// the fixture reaches the field and an absent credential means stripped.
fn assert_reaches(s: &Scan, text: &str) {
    assert!(s.raw[0].contains(text), "the clean URL {text} is not in the CycloneDX document");
}

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

const CMAKE_GIT: &str = "FetchContent_Declare(waybill-fixture-cred GIT_REPOSITORY https://{CRED}git.example/org/waybill-fixture-cred.git GIT_TAG v1.0.0)\nFetchContent_MakeAvailable(waybill-fixture-cred)\n";
const CMAKE_URL: &str = "FetchContent_Declare(waybill-fixture-cred URL https://{CRED}dl.example/waybill-fixture-cred-1.0.0.tar.gz)\nFetchContent_MakeAvailable(waybill-fixture-cred)\n";
const CMAKE_GH: &str = "FetchContent_Declare(waybill-fixture-cred GIT_REPOSITORY https://{CRED}github.com/waybill-org/waybill-fixture-cred.git GIT_TAG v1.0.0)\nFetchContent_MakeAvailable(waybill-fixture-cred)\n";

fn cmake(body: &str) -> String {
    format!("{ROOT}{body}")
}

// ---------------------------------------------------------------------
// US1 (T006): D-cases
// ---------------------------------------------------------------------

#[test]
fn d01_cmake_git_repository() {
    let s = d_case(&[("CMakeLists.txt", &cmake(CMAKE_GIT))], "CMakeLists.txt");
    let c = one(&s, "pkg:generic/waybill-fixture-cred@");
    assert_eq!(prop(c, "waybill:download-url"), Some("https://git.example/org/waybill-fixture-cred.git"));
}

#[test]
fn d01u_cmake_url() {
    let s = d_case(&[("CMakeLists.txt", &cmake(CMAKE_URL))], "CMakeLists.txt");
    assert_reaches(&s, "https://dl.example/waybill-fixture-cred-1.0.0.tar.gz");
}

#[test]
fn d03_cmake_github_repository() {
    let s = d_case(&[("CMakeLists.txt", &cmake(CMAKE_GH))], "CMakeLists.txt");
    one(&s, "pkg:github/waybill-org/waybill-fixture-cred@v1.0.0");
}

const DATASET: &str = "set(TEST_MATRICES\n  WaybillFixtureA/waybill-fixture-m1\n)\nset(TEST_MD5HASH\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n)\nlist(LENGTH TEST_MATRICES len)\nmath(EXPR len1 \"${len} - 1\")\nforeach(i RANGE 0 ${len1})\n  list(GET TEST_MATRICES ${i} m)\n  list(GET TEST_MD5HASH ${i} md5)\n  file(DOWNLOAD https://{CRED}dl.example/MM/${m}.tar.gz ${CMAKE_MATRICES_DIR}/m.tar.gz)\n  file(MD5 ${CMAKE_MATRICES_DIR}/m.tar.gz hash)\nendforeach()\n";

#[test]
fn d02_cmake_dataset_download() {
    let s = d_case(&[("CMakeLists.txt", "project(waybill-fixture-app)\n"), ("cmake/m.cmake", DATASET)], "m.cmake");
    assert_reaches(&s, "dl.example/MM/WaybillFixtureA/waybill-fixture-m1.tar.gz");
}

const BAZEL: &str = "http_archive(\n    name = \"waybill_fixture_cred\",\n    urls = [\"https://{CRED}dl.example/waybill-fixture-cred/archive/1.0.0.tar.gz\"],\n    sha256 = \"abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890abc\",\n)\n";

#[test]
fn d04_bazel_http_archive() {
    let s = d_case(&[("WORKSPACE.bazel", BAZEL)], "WORKSPACE.bazel");
    assert_reaches(&s, "https://dl.example/waybill-fixture-cred/archive/1.0.0.tar.gz");
}

#[test]
fn d04g_bazel_git_repository() {
    let s = d_case(
        &[("WORKSPACE.bazel", "git_repository(\n    name = \"waybill_fixture_rules\",\n    remote = \"https://{CRED}git.example/org/waybill-fixture-rules.git\",\n    commit = \"eb39649a76b87e8451baf75d10ce82ca3a3d5601\",\n)\n")],
        "WORKSPACE.bazel",
    );
    assert_reaches(&s, "https://git.example/org/waybill-fixture-rules.git");
}

const UV_INDEX: &str = "version = 1\nrequires-python = \">=3.10\"\n\n[[package]]\nname = \"waybill-fixture-reg\"\nversion = \"1.0.0\"\nsource = { registry = \"https://{CRED}internal.pypi.example/simple\" }\n";
const UV_GIT: &str = "version = 1\nrequires-python = \">=3.10\"\n\n[[package]]\nname = \"waybill-fixture-git\"\nversion = \"0.5.0\"\nsource = { git = \"https://{CRED}git.example/org/waybill-fixture-git.git\", rev = \"abc123def456\" }\n";

#[test]
fn d05_uv_lock_index() {
    let s = d_case(&[("uv.lock", UV_INDEX)], "uv.lock");
    assert_reaches(&s, "https://internal.pypi.example/simple");
}

#[test]
fn d06_uv_lock_git_source() {
    let s = d_case(&[("uv.lock", UV_GIT)], "uv.lock");
    assert_reaches(&s, "https://git.example/org/waybill-fixture-git.git@abc123def456");
}

#[test]
fn d06u_uv_lock_url_source() {
    let lock = "version = 1\nrequires-python = \">=3.10\"\n\n[[package]]\nname = \"waybill-fixture-url\"\nversion = \"1.0.0\"\nsource = { url = \"https://{CRED}dl.example/waybill_fixture_url-1.0.0.tar.gz\" }\n";
    let s = d_case(&[("uv.lock", lock)], "uv.lock");
    assert_reaches(&s, "https://dl.example/waybill_fixture_url-1.0.0.tar.gz");
}

const FLAKE: &str = r#"{
  "nodes": {
    "waybill-fixture-input": {
      "locked": {
        "lastModified": 1781000000,
        "narHash": "sha256-j08lBbqaYwjsW0xAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        "rev": "a32edd7654519351e48e80372a928df336394670",
        "type": "tarball",
        "url": "https://{CRED}dl.example/waybill-fixture-input.tar.gz"
      },
      "original": { "type": "tarball", "url": "https://{CRED}dl.example/waybill-fixture-input.tar.gz" }
    },
    "root": { "inputs": { "waybill-fixture-input": "waybill-fixture-input" } }
  },
  "root": "root", "version": 7
}
"#;

#[test]
fn d07_flake_lock_input() {
    let s = d_case(&[("flake.lock", FLAKE)], "flake.lock");
    assert_reaches(&s, "https://dl.example/waybill-fixture-input.tar.gz");
}

const PEX: &str = r#"{
  "pex_version": "2.10.0",
  "locked_resolves": [
    {
      "locked_requirements": [
        {
          "project_name": "waybill-fixture-url",
          "version": "1.0.0",
          "requires_dists": [],
          "artifacts": [
            {
              "algorithm": "sha256",
              "hash": "3333333333333333333333333333333333333333333333333333333333333333",
              "url": "https://{CRED}mirror.example.test/wheels/waybill_fixture_url-1.0.0-py3-none-any.whl"
            }
          ]
        }
      ]
    }
  ]
}
"#;

#[test]
fn d08_pants_pex_lockfile() {
    let s = d_case(&[("3rdparty/python/default.lock", PEX)], "default.lock");
    assert_reaches(&s, "https://mirror.example.test/wheels/waybill_fixture_url-1.0.0-py3-none-any.whl");
}

const COURSIER: &str = "# --- BEGIN PANTS LOCKFILE METADATA: DO NOT EDIT OR REMOVE ---\n# {\n#   \"version\": 1,\n#   \"generated_with_requirements\": []\n# }\n# --- END PANTS LOCKFILE METADATA ---\n\n[[entries]]\ndirectDependencies = []\ndependencies = []\nfile_name = \"internal-source-1.0.0.jar\"\n\n[entries.coord]\ngroup = \"dev.waybill.fixture\"\nartifact = \"internal-source\"\nversion = \"1.0.0\"\nurl = \"https://{CRED}internal-mirror.example.test/dev/waybill/fixture/internal-source/1.0.0/internal-source-1.0.0.jar\"\n\n[entries.file_digest]\nfingerprint = \"00000000000000000000000000000000000000000000000000000000000000a3\"\nserialized_bytes_length = 400\n";

#[test]
fn d09_pants_coursier_lockfile() {
    let s = d_case(&[("3rdparty/jvm/default.lock", COURSIER)], "default.lock");
    assert_reaches(&s, "https://internal-mirror.example.test/dev/waybill/fixture/internal-source/1.0.0/internal-source-1.0.0.jar");
}

const PODFILE_LOCK: &str = "PODS:\n- WaybillFixtureFork (1.5.0)\n\nDEPENDENCIES:\n- WaybillFixtureFork (from `https://{CRED}git.example/org/waybill-fixture-fork.git`, branch `main`)\n\nEXTERNAL SOURCES:\n  WaybillFixtureFork:\n    :git: \"https://{CRED}git.example/org/waybill-fixture-fork.git\"\n    :branch: \"main\"\n\nCHECKOUT OPTIONS:\n  WaybillFixtureFork:\n    :commit: \"eb39649a76b87e8451baf75d10ce82ca3a3d5601\"\n    :git: \"https://{CRED}git.example/org/waybill-fixture-fork.git\"\n\nPODFILE CHECKSUM: deadbeef\nCOCOAPODS: 1.15.2\n";

#[test]
fn d12_cocoapods_git_source() {
    let s = d_case(&[("Podfile.lock", PODFILE_LOCK)], "Podfile.lock");
    assert_reaches(&s, "git.example/org/waybill-fixture-fork.git");
}

const PUBSPEC: &str = "name: waybill_fixture_app\nversion: 0.1.0\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  waybill_fixture_lib: ^2.0.0\n";
const PUB_HOSTED: &str = "packages:\n  waybill_fixture_lib:\n    dependency: \"direct main\"\n    description:\n      name: waybill_fixture_lib\n      sha256: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n      url: \"https://{CRED}pub.example.com\"\n    source: hosted\n    version: \"2.0.0\"\n";
const PUB_GIT: &str = "packages:\n  waybill_fixture_lib:\n    dependency: \"direct main\"\n    description:\n      path: \"plugins/waybill_fixture_lib\"\n      ref: \"master\"\n      resolved-ref: \"eb39649a76b87e8451baf75d10ce82ca3a3d5601\"\n      url: \"https://{CRED}git.example/org/waybill-fixture-lib.git\"\n    source: git\n    version: \"0.1.0\"\n";

#[test]
fn d13_dart_hosted_repository() {
    let s = d_case(&[("pubspec.yaml", PUBSPEC), ("pubspec.lock", PUB_HOSTED)], "pubspec.lock");
    assert_reaches(&s, "pub.example.com");
}

#[test]
fn d13g_dart_git_source() {
    let s = d_case(&[("pubspec.yaml", PUBSPEC), ("pubspec.lock", PUB_GIT)], "pubspec.lock");
    assert_reaches(&s, "git.example/org/waybill-fixture-lib.git");
}

const COMPOSER_JSON: &str = r#"{"name":"waybill-fixture/app","version":"1.0.0","require":{"waybill-fixture/lib":"^2.0"}}"#;
const COMPOSER_DIST: &str = r#"{"content-hash":"x","packages":[{"name":"waybill-fixture/lib","version":"2.0.0","type":"library","dist":{"type":"zip","url":"https://{CRED}repo.example.com/dist/waybill-fixture/lib/abc.zip","shasum":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}],"packages-dev":[]}"#;
const COMPOSER_VCS: &str = r#"{"content-hash":"x","packages":[{"name":"waybill-fixture/lib","version":"dev-main","type":"library","source":{"type":"git","url":"https://{CRED}git.example/org/waybill-fixture-lib.git","reference":"eb39649a76b87e8451baf75d10ce82ca3a3d5601"}}],"packages-dev":[]}"#;

#[test]
fn d14_composer_self_hosted_repository() {
    let s = d_case(&[("composer.json", COMPOSER_JSON), ("composer.lock", COMPOSER_DIST)], "composer.lock");
    assert_reaches(&s, "repo.example.com");
}

#[test]
fn d14v_composer_vcs_source() {
    let json = COMPOSER_JSON.replace("^2.0", "dev-main");
    let s = d_case(&[("composer.json", &json), ("composer.lock", COMPOSER_VCS)], "composer.lock");
    assert_reaches(&s, "git.example/org/waybill-fixture-lib.git");
}

const MIX_EXS: &str = "defmodule WaybillFixture.MixProject do\n  def project, do: [app: :waybill_fixture, version: \"0.1.0\", deps: deps()]\n  defp deps do\n    [{:waybill_fixture_fork, git: \"https://{CRED}git.example/org/waybill-fixture-fork.git\", ref: \"main\"}]\n  end\nend\n";
const MIX_LOCK: &str = "%{\n  \"waybill_fixture_fork\": {:git, \"https://{CRED}git.example/org/waybill-fixture-fork.git\", \"eb39649a76b87e8451baf75d10ce82ca3a3d5601\", [ref: \"main\"]},\n}\n";

#[test]
fn d15_elixir_lockfile_git() {
    let s = d_case(&[("mix.exs", MIX_EXS), ("mix.lock", MIX_LOCK)], "mix.lock");
    assert_reaches(&s, "git.example/org/waybill-fixture-fork.git");
}

#[test]
fn d15d_elixir_declared_git() {
    let s = d_case(&[("mix.exs", MIX_EXS)], "mix.exs");
    assert_reaches(&s, "git.example/org/waybill-fixture-fork.git");
}

const REBAR_CONFIG: &str = "{deps, [\n    {waybill_fixture_dep, {git, \"https://{CRED}git.example/org/waybill-fixture-dep.git\", {tag, \"v1.2.3\"}}}\n]}.\n";
const REBAR_LOCK: &str = "{\"1.2.0\",\n[{<<\"waybill_fixture_dep\">>,{git,\"https://{CRED}git.example/org/waybill-fixture-dep.git\",{tag,\"v1.2.3\"}},0}]}.\n";

#[test]
fn d16_erlang_lockfile_git() {
    let s = d_case(&[("rebar.config", REBAR_CONFIG), ("rebar.lock", REBAR_LOCK)], "rebar.lock");
    assert_reaches(&s, "git.example/org/waybill-fixture-dep.git");
}

#[test]
fn d16d_erlang_declared_git() {
    let s = d_case(&[("rebar.config", REBAR_CONFIG)], "rebar.config");
    assert_reaches(&s, "git.example/org/waybill-fixture-dep.git");
}

#[test]
fn d17_requirements_direct_url() {
    let s = d_case(
        &[("requirements.txt", "waybill-fixture-cred @ https://{CRED}dl.example/archive/waybill_fixture_cred-1.0.0.tar.gz\n")],
        "requirements.txt",
    );
    assert_reaches(&s, "https://dl.example/archive/waybill_fixture_cred-1.0.0.tar.gz");
}

const RECIPE: &str = "LICENSE = \"MIT\"\nSUMMARY = \"waybill fixture\"\nSRC_URI = \"git://{CRED}git.example/org/waybill-fixture-widget.git;branch=main;protocol=https\"\nSRCREV = \"abc123def456abc123def456abc123def456abcd\"\n";

#[test]
fn d19_yocto_src_uri() {
    let s = d_case(&[("recipes-z/waybill-fixture-widget_1.0.bb", RECIPE)], "waybill-fixture-widget_1.0.bb");
    assert_reaches(&s, "git://git.example/org/waybill-fixture-widget.git;branch=main;protocol=https");
}

// ---------------------------------------------------------------------
// Guards (G-cases): declared URLs that are read but never emitted. They
// pass on `main` and on the branch.
// ---------------------------------------------------------------------

/// `purl_prefix` is the positive control: the reader fired, so an absent
/// credential is not a reader that read nothing.
fn g_case(files: &[(&str, &str)], purl_prefix: &str) {
    let (fx, path) = tree(files, CRED);
    let s = scan3(&fx, &path, &[]);
    assert!(!with_purl(&s, purl_prefix).is_empty(), "control: no component {purl_prefix}");
    assert_no_cred(&s);
}

#[test]
fn g1_gemfile_lock_remote() {
    g_case(&[("Gemfile.lock", "GEM\n  remote: https://{CRED}gems.example/\n  specs:\n    waybill-fixture-present (1.0.0)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  waybill-fixture-present\n")], "pkg:gem/waybill-fixture-present@");
}

#[test]
fn g2_cargo_lock_registry() {
    g_case(&[("Cargo.lock", "version = 4\n[[package]]\nname = \"waybill-fixture-dep\"\nversion = \"1.0.0\"\nsource = \"registry+https://{CRED}crates.example/index\"\n")], "pkg:cargo/waybill-fixture-dep@");
}

#[test]
fn g3_requirements_index_url() {
    g_case(&[("requirements.txt", "--index-url https://{CRED}pypi.example/simple/\nwaybill-fixture-dep==2.31.0\n")], "pkg:pypi/waybill-fixture-dep@");
}

#[test]
fn g4_package_lock_resolved() {
    g_case(&[
        ("package.json", r#"{"name":"waybill-fixture-app","version":"1.0.0","dependencies":{"waybill-fixture-dep":"^11.0.0"}}"#),
        (
            "package-lock.json",
            r#"{"name":"waybill-fixture-app","version":"1.0.0","lockfileVersion":3,"packages":{"":{"name":"waybill-fixture-app","version":"1.0.0","dependencies":{"waybill-fixture-dep":"^11.0.0"}},"node_modules/waybill-fixture-dep":{"version":"11.0.0","resolved":"https://{CRED}npm.example/waybill-fixture-dep/-/waybill-fixture-dep-11.0.0.tgz"}}}"#,
        ),
    ], "pkg:npm/waybill-fixture-dep@");
}

#[test]
fn g5_helm_dependency_repository() {
    g_case(&[(
        "Chart.yaml",
        "name: waybill-fixture-chart\nversion: 1.0.0\ntype: application\ndependencies:\n  - name: waybill-fixture-dep\n    version: 11.0.0\n    repository: https://{CRED}charts.example/stable\n",
    )], "pkg:helm/charts.example/waybill-fixture-dep@");
}

// ---------------------------------------------------------------------
// S-1 (FR-006, planning amendment A1): the bare `git` user of an ssh URL
// is stripped without a warning, and nothing else changes.
// ---------------------------------------------------------------------

#[test]
fn s1_ssh_git_user_changes_that_url_alone() {
    let body = |user: &str| {
        cmake(&format!(
            "FetchContent_Declare(waybill-fixture-ssh GIT_REPOSITORY ssh://{user}git.example/org/waybill-fixture-ssh.git GIT_TAG v1.0.0)\nFetchContent_MakeAvailable(waybill-fixture-ssh)\n"
        ))
    };
    let (fx_git, p_git) = tree(&[("CMakeLists.txt", &body("git@"))], "");
    let (fx_none, p_none) = tree(&[("CMakeLists.txt", &body(""))], "");
    let with_user = scan3(&fx_git, &p_git, &[]);
    let without = scan3(&fx_none, &p_none, &[]);
    assert_not_warned(&with_user);
    parity(&with_user);
    for (name, a, b) in [
        ("cdx", &with_user.cdx, &without.cdx),
        ("spdx23", &with_user.spdx23, &without.spdx23),
        ("spdx3", &with_user.spdx3, &without.spdx3),
    ] {
        let (a, b) = (masked(a), masked(b));
        assert!(a == b, "S-1 {name}: the ssh git user changed more than its URL; {}", masked_difference(&b, &a).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// C-ctl (T003, FR-006): every directory-scan fixture with clean URLs, in one
// tree, is byte-identical to the golden generated from `main`'s binary
// (`WAYBILL_DECLARED_URL_BIN=<main binary> WAYBILL_UPDATE_DECLARED_URL_GOLDENS=1`).
// ---------------------------------------------------------------------

const GOLDEN_ENV: &str = "WAYBILL_UPDATE_DECLARED_URL_GOLDENS";

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/declared_url_credentials/ctl-golden")
}

fn ctl_files() -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = vec![
        ("CMakeLists.txt".into(), cmake(CMAKE_GIT)),
        ("cmake/m.cmake".into(), DATASET.into()),
        ("WORKSPACE.bazel".into(), BAZEL.into()),
        ("py-index/uv.lock".into(), UV_INDEX.into()),
        ("py-git/uv.lock".into(), UV_GIT.into()),
        ("nix/flake.lock".into(), FLAKE.into()),
        ("3rdparty/python/default.lock".into(), PEX.into()),
        ("3rdparty/jvm/default.lock".into(), COURSIER.into()),
        ("ios/Podfile.lock".into(), PODFILE_LOCK.into()),
        ("dart-hosted/pubspec.yaml".into(), PUBSPEC.into()),
        ("dart-hosted/pubspec.lock".into(), PUB_HOSTED.into()),
        ("dart-git/pubspec.yaml".into(), PUBSPEC.into()),
        ("dart-git/pubspec.lock".into(), PUB_GIT.into()),
        ("php-dist/composer.json".into(), COMPOSER_JSON.into()),
        ("php-dist/composer.lock".into(), COMPOSER_DIST.into()),
        ("php-vcs/composer.json".into(), COMPOSER_JSON.replace("^2.0", "dev-main")),
        ("php-vcs/composer.lock".into(), COMPOSER_VCS.into()),
        ("ex/mix.exs".into(), MIX_EXS.into()),
        ("ex/mix.lock".into(), MIX_LOCK.into()),
        ("erl/rebar.config".into(), REBAR_CONFIG.into()),
        ("erl/rebar.lock".into(), REBAR_LOCK.into()),
        ("requirements.txt".into(), "waybill-fixture-cred @ https://{CRED}dl.example/archive/waybill_fixture_cred-1.0.0.tar.gz\n".into()),
        ("recipes-z/waybill-fixture-widget_1.0.bb".into(), RECIPE.into()),
    ];
    f.sort();
    f
}

#[test]
fn c_ctl_clean_urls_are_byte_identical_to_main() {
    let files = ctl_files();
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let (fx, path) = tree(&refs, "");
    let s = scan3(&fx, &path, &[]);
    assert_not_warned(&s);
    parity(&s);
    let dir = golden_dir();
    let docs = [("cdx.json", &s.cdx), ("spdx-2.3.json", &s.spdx23), ("spdx-3.json", &s.spdx3)];
    if std::env::var_os(GOLDEN_ENV).is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, doc) in docs {
            std::fs::write(dir.join(name), serde_json::to_string_pretty(&masked(doc)).unwrap() + "\n").unwrap();
        }
        return;
    }
    for (name, doc) in docs {
        let want: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        let got = masked(doc);
        assert!(got == want, "C-ctl {name} differs from main's golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// Rows found by the T001 addendum sweep (measurements/inventory.md):
// free-text requirement strings and a PURL built from a URL.
// ---------------------------------------------------------------------

#[test]
fn d20_npm_url_dependency_range() {
    let s = d_case(
        &[(
            "package.json",
            r#"{"name":"waybill-fixture-npm","version":"1.0.0","dependencies":{"waybill-fixture-a":"git+https://{CRED}git.example/org/waybill-fixture-a.git"}}"#,
        )],
        "package.json",
    );
    assert_reaches(&s, "git+https://git.example/org/waybill-fixture-a.git");
}

#[test]
fn d21_pyproject_direct_url_constraint() {
    let s = d_case(
        &[(
            "pyproject.toml",
            "[project]\nname = \"waybill-fixture-py\"\nversion = \"0.1.0\"\ndependencies = [\"waybill-fixture-d @ https://{CRED}dl.example/waybill-fixture-d-1.0.tar.gz\"]\n",
        )],
        "pyproject.toml",
    );
    assert_reaches(&s, "https://dl.example/waybill-fixture-d-1.0.tar.gz");
}

#[test]
fn d22_swift_package_resolved_location() {
    let s = d_case(
        &[(
            "Package.resolved",
            r#"{"pins":[{"identity":"waybill-fixture-j","kind":"remoteSourceControl","location":"https://{CRED}git.example/org/waybill-fixture-j.git","state":{"revision":"eb39649a76b87e8451baf75d10ce82ca3a3d5601","version":"1.0.0"}}],"version":2}"#,
        )],
        "Package.resolved",
    );
    one(&s, "pkg:swift/git.example/org/waybill-fixture-j@1.0.0");
}

// ---------------------------------------------------------------------
// US2 (T018): removing a credential does not change what a component is.
// Each case scans a credentialled and a clean tree and requires the same
// masked documents in all three formats.
// ---------------------------------------------------------------------

fn assert_same(a: &Scan, b: &Scan, case: &str) {
    for (name, x, y) in [("cdx", &a.cdx, &b.cdx), ("spdx23", &a.spdx23, &b.spdx23), ("spdx3", &a.spdx3, &b.spdx3)] {
        let (x, y) = (masked(x), masked(y));
        assert!(x == y, "{case} {name}: the credentialled tree differs from the clean one; {}", masked_difference(&y, &x).unwrap_or_default());
    }
}

/// A super-project with a submodule of a non-GitHub repository and a
/// conditional fetch of the same repository (spec 1538 R9), the fetch's URL
/// carrying `cred`.
fn submodule_and_fetch(cred: &str) -> (GitFixture, PathBuf) {
    let fx = GitFixture::new();
    fx.repo("tree");
    fx.write("tree/.gitmodules", "[submodule \"d\"]\n\tpath = extern/waybill-fixture-dep\n\turl = https://git.example/org/waybill-fixture-dep.git\n");
    fx.write(
        "tree/CMakeLists.txt",
        &cmake(&format!(
            "if(NOT TARGET waybill_fixture_dep)\nFetchContent_Declare(waybill-fixture-dep GIT_REPOSITORY https://{cred}git.example/org/waybill-fixture-dep.git GIT_TAG ${{FX_UNSET}})\nendif()\n"
        )),
    );
    let path = fx.path("tree");
    fx.git(&path, &["add", ".gitmodules"]);
    fx.add_gitlink("tree", "extern/waybill-fixture-dep", &"a".repeat(40));
    (fx, path)
}

#[test]
fn i1_a_credentialled_fetch_merges_with_its_submodule() {
    let (fx_c, p_c) = submodule_and_fetch(CRED);
    let (fx_n, p_n) = submodule_and_fetch("");
    let cred = scan3(&fx_c, &p_c, &[]);
    let clean = scan3(&fx_n, &p_n, &[]);
    assert_no_cred(&cred);
    assert_eq!(with_purl(&clean, "pkg:generic/waybill-fixture-dep@").len(), 1, "control: the clean tree merges to one component");
    assert_same(&cred, &clean, "I-1");
}

#[test]
fn i2_a_credentialled_github_fetch_keeps_its_purl() {
    let (fx_c, p_c) = tree(&[("CMakeLists.txt", &cmake(CMAKE_GH))], CRED);
    let (fx_n, p_n) = tree(&[("CMakeLists.txt", &cmake(CMAKE_GH))], "");
    assert_same(&scan3(&fx_c, &p_c, &[]), &scan3(&fx_n, &p_n, &[]), "I-2");
}

/// Research R3: host case, default port and dot segments survive stripping.
#[test]
fn i3_stripping_keeps_the_rest_of_the_url() {
    let body = "FetchContent_Declare(waybill-fixture-case GIT_REPOSITORY https://{CRED}Git.Example:443/Org/a/../Waybill-Fixture-Case.git GIT_TAG v1.0.0)\nFetchContent_MakeAvailable(waybill-fixture-case)\n";
    let (fx_c, p_c) = tree(&[("CMakeLists.txt", &cmake(body))], "wbfxtoken7c41@");
    let (fx_n, p_n) = tree(&[("CMakeLists.txt", &cmake(body))], "");
    let clean = scan3(&fx_n, &p_n, &[]);
    assert_reaches(&clean, "https://Git.Example:443/Org/a/../Waybill-Fixture-Case.git");
    assert_same(&scan3(&fx_c, &p_c, &[]), &clean, "I-3");
}
