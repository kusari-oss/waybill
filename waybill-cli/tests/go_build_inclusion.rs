//! Integration tests for milestone 112 US1 — always-on
//! `mikebom:build-inclusion: unknown` markers on Go modules discovered
//! via the lower-fidelity fallback paths (T009).
//!
//! Fixture shape (hermetic, no network, no `go` toolchain needed):
//!
//! * `go.sum` lists three modules; one of them
//!   (`github.com/graph-resolved/dep`) also has its `.mod` file staged
//!   in a tempdir `GOMODCACHE`, so the milestone-055 graph resolver
//!   resolves it via the cache walk (step 2). The other two are
//!   reachable only through the milestone-091 go.sum flat fallback
//!   (step 5) and therefore carry `mikebom:resolver-step:
//!   go-sum-fallback`.
//! * No compiled binary → no BuildInfo → the marker pass treats every
//!   fallback module as participation-unknown (spec FR-001).
//!
//! Expected emission (contracts/annotations.md):
//!
//! * fallback modules → `mikebom:build-inclusion: unknown` in all
//!   three formats; NO native scope field in CDX (FR-002).
//! * cache-resolved module + main module → no marker.
//! * component count is identical to the pre-feature scan (FR-011:
//!   the pass never adds or removes components).

use std::path::Path;
use std::process::Command;

/// Lay down the hermetic fixture: `app/go.mod` + `app/go.sum` with two
/// fallback-only modules and one cache-resolvable module, plus a
/// `gomodcache/` tree holding that module's `.mod` file in the
/// `cache/download/<path>/@v/<version>.mod` layout `GoModCache`
/// expects.
fn write_fixture(root: &Path) {
    let app = root.join("app");
    std::fs::create_dir_all(&app).expect("create app dir");
    std::fs::write(
        app.join("go.mod"),
        "module example.com/sourceonly\n\
         go 1.22\n\
         require (\n\
         \tgithub.com/never-linked/fake v9.9.9\n\
         \tgithub.com/graph-resolved/dep v1.2.3\n\
         )\n",
    )
    .expect("write go.mod");
    std::fs::write(
        app.join("go.sum"),
        concat!(
            "github.com/never-linked/fake v9.9.9 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            "github.com/also-never-linked/other v1.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            "github.com/graph-resolved/dep v1.2.3 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
        ),
    )
    .expect("write go.sum");
    let modcache_v = root.join("gomodcache/cache/download/github.com/graph-resolved/dep/@v");
    std::fs::create_dir_all(&modcache_v).expect("create gomodcache dirs");
    std::fs::write(
        modcache_v.join("v1.2.3.mod"),
        "module github.com/graph-resolved/dep\ngo 1.22\n",
    )
    .expect("write cached .mod");
}

/// Scan the fixture once, emitting all three formats. An isolated
/// `HOME` and a fixture-local `GOMODCACHE` make the resolver outcome
/// independent of the developer's real Go cache.
/// `MIKEBOM_NO_GO_MOD_WHY=1` keeps the US1 markers toolchain-free once
/// the milestone-112 US2 classification pass lands (inert before then).
fn scan_three_formats() -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let fixture = tempfile::tempdir().expect("fixture tempdir");
    write_fixture(fixture.path());
    let out = tempfile::tempdir().expect("output tempdir");
    let cdx_path = out.path().join("out.cdx.json");
    let spdx23_path = out.path().join("out.spdx.json");
    let spdx3_path = out.path().join("out.spdx3.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");

    let bin = env!("CARGO_BIN_EXE_mikebom");
    let output = Command::new(bin)
        .env("HOME", fake_home.path())
        .env("GOMODCACHE", fixture.path().join("gomodcache"))
        .env("MIKEBOM_NO_GO_MOD_WHY", "1")
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture.path().join("app"))
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parse = |p: &Path| -> serde_json::Value {
        let raw = std::fs::read_to_string(p)
            .unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
    };
    (parse(&cdx_path), parse(&spdx23_path), parse(&spdx3_path))
}

const FALLBACK_PURLS: [&str; 2] = [
    "pkg:golang/github.com/never-linked/fake@v9.9.9",
    "pkg:golang/github.com/also-never-linked/other@v1.0.0",
];
const RESOLVED_PURL: &str = "pkg:golang/github.com/graph-resolved/dep@v1.2.3";

/// CDX property lookup: `Some(value)` when the component carries the
/// named property, `None` otherwise.
fn cdx_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component["properties"].as_array()?.iter().find_map(|p| {
        if p["name"].as_str() == Some(name) {
            p["value"].as_str()
        } else {
            None
        }
    })
}

fn cdx_component_by_purl<'a>(
    cdx: &'a serde_json::Value,
    purl: &str,
) -> &'a serde_json::Value {
    cdx["components"]
        .as_array()
        .expect("components array")
        .iter()
        .find(|c| c["purl"].as_str() == Some(purl))
        .unwrap_or_else(|| panic!("component {purl} must be present"))
}

/// True when the SPDX 2.3 package carries a milestone-112
/// `mikebom:build-inclusion` annotation envelope with the given value.
fn spdx23_has_build_inclusion(package: &serde_json::Value, value: &str) -> bool {
    package["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["comment"].as_str())
        .filter_map(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .any(|env| {
            env["schema"].as_str() == Some("mikebom-annotation/v1")
                && env["field"].as_str() == Some("mikebom:build-inclusion")
                && env["value"].as_str() == Some(value)
        })
}

fn spdx23_package_by_purl<'a>(
    spdx: &'a serde_json::Value,
    purl: &str,
) -> &'a serde_json::Value {
    spdx["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .find(|p| {
            p["externalRefs"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["referenceLocator"].as_str() == Some(purl))
        })
        .unwrap_or_else(|| panic!("SPDX 2.3 package {purl} must be present"))
}

/// Map a PURL to the SPDX 3 `software_Package` element's `spdxId`.
fn spdx3_package_id<'a>(graph: &'a [serde_json::Value], purl: &str) -> &'a str {
    graph
        .iter()
        .find(|e| {
            e["type"].as_str() == Some("software_Package")
                && e["software_packageUrl"].as_str() == Some(purl)
        })
        .and_then(|e| e["spdxId"].as_str())
        .unwrap_or_else(|| panic!("SPDX 3 package {purl} must be present"))
}

/// True when an SPDX 3 `Annotation` element targets `subject_id` with
/// a `mikebom:build-inclusion` envelope carrying the given value.
fn spdx3_has_build_inclusion(
    graph: &[serde_json::Value],
    subject_id: &str,
    value: &str,
) -> bool {
    graph
        .iter()
        .filter(|e| e["type"].as_str() == Some("Annotation"))
        .filter(|e| e["subject"].as_str() == Some(subject_id))
        .filter_map(|e| e["statement"].as_str())
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .any(|env| {
            env["schema"].as_str() == Some("mikebom-annotation/v1")
                && env["field"].as_str() == Some("mikebom:build-inclusion")
                && env["value"].as_str() == Some(value)
        })
}

#[test]
fn unknown_marker_in_all_three_formats() {
    let (cdx, spdx23, spdx3) = scan_three_formats();

    // ---- CDX 1.6 (FR-001 / FR-002) --------------------------------
    for purl in FALLBACK_PURLS {
        let c = cdx_component_by_purl(&cdx, purl);
        assert_eq!(
            cdx_property(c, "mikebom:build-inclusion"),
            Some("unknown"),
            "{purl}: fallback-discovered module must carry the unknown marker",
        );
        assert_eq!(
            cdx_property(c, "mikebom:resolver-step"),
            Some("go-sum-fallback"),
            "{purl}: fixture invariant — module must be fallback-discovered",
        );
        // FR-002: Unknown never sets a native scope field. CDX `scope`
        // defaults to "required" when absent; emitting nothing is the
        // contract (excluded would falsely claim NotNeeded).
        assert!(
            c.get("scope").is_none(),
            "{purl}: unknown-marked component must carry NO scope field, got {:?}",
            c["scope"],
        );
    }
    let resolved = cdx_component_by_purl(&cdx, RESOLVED_PURL);
    assert_eq!(
        cdx_property(resolved, "mikebom:build-inclusion"),
        None,
        "graph-resolved module must NOT carry a build-inclusion marker",
    );
    assert_eq!(
        cdx_property(resolved, "mikebom:resolver-step"),
        None,
        "fixture invariant — cache-resolved module is not fallback-discovered",
    );

    // FR-011: the marker pass annotates in place; it never adds or
    // removes components. Pre-feature expectation for this fixture:
    // exactly the three go.sum modules (the main module collapses into
    // metadata.component per milestone 084). Issue #364 adds one
    // synthetic `pkg:golang/stdlib@...` component per Go scan; exclude
    // it from the FR-011 count because it's orthogonal to the marker-
    // pass invariant under test.
    let golang_count = cdx["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:golang/") && !p.starts_with("pkg:golang/stdlib"))
        })
        .count();
    assert_eq!(
        golang_count, 3,
        "component count must match the pre-feature scan (FR-011)",
    );

    // ---- SPDX 2.3 parity bridge (FR-003) ---------------------------
    for purl in FALLBACK_PURLS {
        let p = spdx23_package_by_purl(&spdx23, purl);
        assert!(
            spdx23_has_build_inclusion(p, "unknown"),
            "{purl}: SPDX 2.3 package must carry the unknown annotation envelope",
        );
    }
    assert!(
        !spdx23_has_build_inclusion(
            spdx23_package_by_purl(&spdx23, RESOLVED_PURL),
            "unknown"
        ),
        "graph-resolved SPDX 2.3 package must NOT carry the annotation",
    );

    // ---- SPDX 3 parity bridge (FR-003) -----------------------------
    let graph = spdx3["@graph"].as_array().expect("@graph array");
    for purl in FALLBACK_PURLS {
        let id = spdx3_package_id(graph, purl);
        assert!(
            spdx3_has_build_inclusion(graph, id, "unknown"),
            "{purl}: SPDX 3 package must have an unknown-marker Annotation element",
        );
    }
    let resolved_id = spdx3_package_id(graph, RESOLVED_PURL);
    assert!(
        !spdx3_has_build_inclusion(graph, resolved_id, "unknown"),
        "graph-resolved SPDX 3 package must NOT have a build-inclusion Annotation",
    );
}

// ---------------------------------------------------------------------
// Milestone 112 US2 (T018) — stub-toolchain end-to-end classification.
//
// A fake `go` shell script prepended to PATH answers the three
// subprocess shapes the scanner issues:
//
//   - `go version`    → toolchain-availability probe, exit 0;
//   - `go mod graph`  → milestone-055 step 1; exit 1 so every go.sum
//     module degrades to the flat fallback (the population the
//     classification pass queries);
//   - `go list all`   → reliability preflight, exit 0;
//   - `go mod why -m -vendor <paths…>` → canned verdict sections.
//
// Hermetic: no network, no real toolchain, real subprocess plumbing.
// ---------------------------------------------------------------------

#[cfg(unix)]
mod stub_toolchain {
    use super::{cdx_component_by_purl, cdx_property};
    use std::path::Path;
    use std::process::Command;

    /// Write `script` as an executable `go` shim in its own tempdir;
    /// returns the dir (keep it alive for the Command's duration).
    pub(super) fn write_go_stub(script: &str) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("stub tempdir");
        let path = dir.path().join("go");
        std::fs::write(&path, script).expect("write stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod stub");
        dir
    }

    /// Scan `scan_path` (CDX only) with the stub dir prepended to PATH
    /// and classification ENABLED (no `MIKEBOM_NO_GO_MOD_WHY`).
    fn scan_cdx_with_stub(scan_path: &Path, stub_dir: &Path) -> serde_json::Value {
        let out = tempfile::tempdir().expect("output tempdir");
        let cdx_path = out.path().join("out.cdx.json");
        let fake_home = tempfile::tempdir().expect("fake-home tempdir");
        let empty_cache = tempfile::tempdir().expect("empty-cache tempdir");
        let real_path = std::env::var("PATH").expect("PATH set");

        let bin = env!("CARGO_BIN_EXE_mikebom");
        let output = Command::new(bin)
            .env("PATH", format!("{}:{real_path}", stub_dir.to_string_lossy()))
            .env("HOME", fake_home.path())
            .env("GOMODCACHE", empty_cache.path().join("empty"))
            .env_remove("MIKEBOM_NO_GO_MOD_WHY")
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(scan_path)
            .arg("--output")
            .arg(&cdx_path)
            .arg("--no-deep-hash")
            .output()
            .expect("mikebom should run");
        assert!(
            output.status.success(),
            "scan failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = std::fs::read_to_string(&cdx_path).expect("read cdx");
        serde_json::from_str(&raw).expect("valid cdx JSON")
    }

    pub(super) const STUB_THREE_VERDICTS: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)
        shift 4   # drop: mod why -m -vendor
        for m in "$@"; do
          echo "# $m"
          case "$m" in
            *never-linked*)
              echo "(main module does not need module $m)" ;;
            *test-only*)
              echo "example.com/sourceonly"
              echo "example.com/sourceonly.test"
              echo "$m" ;;
            *)
              echo "example.com/sourceonly"
              echo "$m" ;;
          esac
        done
        exit 0 ;;
    esac ;;
esac
exit 1
"##;

    /// Single-main-module fixture: three go.sum modules, all reachable
    /// only via the flat fallback (empty GOMODCACHE, `go mod graph`
    /// stubbed to fail).
    pub(super) fn write_three_verdict_fixture(root: &Path) {
        let app = root.join("app");
        std::fs::create_dir_all(&app).expect("create app dir");
        std::fs::write(
            app.join("go.mod"),
            "module example.com/sourceonly\n\
             go 1.22\n\
             require (\n\
             \tgithub.com/never-linked/fake v9.9.9\n\
             \tgithub.com/test-only/dep v2.0.0\n\
             \tgithub.com/prod/dep v1.0.0\n\
             )\n",
        )
        .expect("write go.mod");
        std::fs::write(
            app.join("go.sum"),
            concat!(
                "github.com/never-linked/fake v9.9.9 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
                "github.com/test-only/dep v2.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
                "github.com/prod/dep v1.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            ),
        )
        .expect("write go.sum");
    }

    const NOT_NEEDED_PURL: &str = "pkg:golang/github.com/never-linked/fake@v9.9.9";
    const TEST_ONLY_PURL: &str = "pkg:golang/github.com/test-only/dep@v2.0.0";
    const PROD_PURL: &str = "pkg:golang/github.com/prod/dep@v1.0.0";

    #[test]
    fn classification_applies_all_three_verdicts_end_to_end() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());
        let stub = write_go_stub(STUB_THREE_VERDICTS);
        let cdx = scan_cdx_with_stub(&fixture.path().join("app"), stub.path());

        // NotNeeded: kept in components[] with the native excluded
        // scope + both milestone-112 properties (FR-005, T016).
        let not_needed = cdx_component_by_purl(&cdx, NOT_NEEDED_PURL);
        assert_eq!(
            not_needed["scope"].as_str(),
            Some("excluded"),
            "NotNeeded component must carry native scope: excluded",
        );
        assert_eq!(
            cdx_property(not_needed, "mikebom:build-inclusion"),
            Some("not-needed"),
        );
        assert_eq!(
            cdx_property(not_needed, "mikebom:build-inclusion-derivation"),
            Some("go-mod-why"),
        );

        // TestOnly: test-scoped with the go-mod-why derivation (FR-006).
        let test_only = cdx_component_by_purl(&cdx, TEST_ONLY_PURL);
        assert_eq!(
            cdx_property(test_only, "mikebom:lifecycle-scope"),
            Some("test"),
            "TestOnly component must be test-scoped",
        );
        assert_eq!(
            cdx_property(test_only, "mikebom:lifecycle-scope-derivation"),
            Some("go-mod-why"),
        );
        assert_eq!(
            cdx_property(test_only, "mikebom:build-inclusion"),
            None,
            "TestOnly must not carry a build-inclusion marker",
        );

        // ProdNeeded: no marker, no derivation, no scope — emission
        // unchanged from the pre-feature shape (FR-011).
        let prod = cdx_component_by_purl(&cdx, PROD_PURL);
        assert!(prod.get("scope").is_none(), "prod component must have no scope");
        assert_eq!(cdx_property(prod, "mikebom:build-inclusion"), None);
        assert_eq!(
            cdx_property(prod, "mikebom:build-inclusion-derivation"),
            None,
        );
        assert_eq!(
            cdx_property(prod, "mikebom:lifecycle-scope-derivation"),
            None,
        );

        // SC-002: with every queried module classified, NO component
        // carries the unknown marker.
        for c in cdx["components"].as_array().expect("components array") {
            assert_ne!(
                cdx_property(c, "mikebom:build-inclusion"),
                Some("unknown"),
                "no component may carry the unknown marker after full \
                 classification (SC-002): {}",
                c["purl"],
            );
        }
    }

    /// Stub for the two-main-module fixture: app1 needs the shared
    /// module, app2 does not (`go mod why` answers per-cwd — the
    /// runner sets cwd to each main module's directory).
    const STUB_TWO_MAIN_MODULES: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)
        shift 4
        for m in "$@"; do
          echo "# $m"
          case "$(pwd)" in
            *app1*)
              echo "example.com/app1"
              echo "$m" ;;
            *)
              echo "(main module does not need module $m)" ;;
          esac
        done
        exit 0 ;;
    esac ;;
esac
exit 1
"##;

    /// Spec edge case: a module needed by only ONE of two main modules
    /// in the scanned tree is NOT excluded (needed-by-ANY wins).
    #[test]
    fn module_needed_by_any_main_module_is_not_excluded() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        for app in ["app1", "app2"] {
            let dir = fixture.path().join(app);
            std::fs::create_dir_all(&dir).expect("create app dir");
            std::fs::write(
                dir.join("go.mod"),
                format!(
                    "module example.com/{app}\n\
                     go 1.22\n\
                     require github.com/shared/dep v1.0.0\n"
                ),
            )
            .expect("write go.mod");
            std::fs::write(
                dir.join("go.sum"),
                "github.com/shared/dep v1.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            )
            .expect("write go.sum");
        }
        let stub = write_go_stub(STUB_TWO_MAIN_MODULES);
        let cdx = scan_cdx_with_stub(fixture.path(), stub.path());

        let shared =
            cdx_component_by_purl(&cdx, "pkg:golang/github.com/shared/dep@v1.0.0");
        assert!(
            shared.get("scope").is_none(),
            "module needed by ANY main module must not be excluded, got {:?}",
            shared["scope"],
        );
        assert_eq!(
            cdx_property(shared, "mikebom:build-inclusion"),
            None,
            "needed-by-any module must carry no build-inclusion marker",
        );
    }
}

// ---------------------------------------------------------------------
// Milestone 112 US3 (T021) — degrade matrix. Every failure class must
// (1) leave the scan exit status 0 with a valid SBOM (SC-003), (2) fall
// back to `mikebom:build-inclusion: unknown` markers — never a false
// `excluded`/`not-needed` — and (3) surface the failure class via the
// FR-013 observability lines on stderr
// (contracts/go-toolchain-invocation.md).
// ---------------------------------------------------------------------

#[cfg(unix)]
mod degrade_matrix {
    use super::stub_toolchain::{write_go_stub, write_three_verdict_fixture};
    use super::{cdx_component_by_purl, cdx_property};
    use std::path::Path;
    use std::process::Command;

    const ALL_PURLS: [&str; 3] = [
        "pkg:golang/github.com/never-linked/fake@v9.9.9",
        "pkg:golang/github.com/test-only/dep@v2.0.0",
        "pkg:golang/github.com/prod/dep@v1.0.0",
    ];

    /// Scan with full control over PATH and extra env vars; returns the
    /// parsed CDX document plus the scan's stderr so tests can assert
    /// on the FR-013 summary/warn lines. Asserts exit status 0 and
    /// parseable JSON output (SC-003) for every degrade case.
    pub(super) fn scan_with_env(
        scan_path: &Path,
        path_value: &str,
        extra_env: &[(&str, &str)],
    ) -> (serde_json::Value, String) {
        let out = tempfile::tempdir().expect("output tempdir");
        let cdx_path = out.path().join("out.cdx.json");
        let fake_home = tempfile::tempdir().expect("fake-home tempdir");
        let empty_cache = tempfile::tempdir().expect("empty-cache tempdir");

        let bin = env!("CARGO_BIN_EXE_mikebom");
        let mut cmd = Command::new(bin);
        cmd.env("PATH", path_value)
            .env("HOME", fake_home.path())
            .env("GOMODCACHE", empty_cache.path().join("empty"))
            .env_remove("MIKEBOM_NO_GO_MOD_WHY")
            // The FR-013 lines are info/warn level; pin the default
            // filter regardless of the developer's RUST_LOG.
            .env_remove("RUST_LOG");
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let output = cmd
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(scan_path)
            .arg("--output")
            .arg(&cdx_path)
            .arg("--no-deep-hash")
            .output()
            .expect("mikebom should run");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            output.status.success(),
            "degraded scan must still exit 0 (SC-003): stderr={stderr}",
        );
        let raw = std::fs::read_to_string(&cdx_path).expect("read cdx");
        let cdx = serde_json::from_str(&raw).expect("valid cdx JSON");
        (cdx, stderr)
    }

    /// Every fixture module must fall back to the unknown marker and
    /// none may be falsely excluded (the silent-false-negative guard).
    fn assert_all_unknown(cdx: &serde_json::Value) {
        for purl in ALL_PURLS {
            let c = cdx_component_by_purl(cdx, purl);
            assert_eq!(
                cdx_property(c, "mikebom:build-inclusion"),
                Some("unknown"),
                "{purl}: degraded analysis must fall back to the unknown marker",
            );
            assert!(
                c.get("scope").is_none(),
                "{purl}: degraded analysis must never set a native scope, got {:?}",
                c["scope"],
            );
        }
    }

    /// (a) No `go` anywhere on PATH → skip reason `no-toolchain`; the
    /// Part B unknown markers still apply.
    #[test]
    fn no_toolchain_on_path_degrades_to_unknown_markers() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());
        // PATH is ONLY an empty dir — no real `go` reachable.
        let empty_path_dir = tempfile::tempdir().expect("empty PATH dir");
        let (cdx, stderr) = scan_with_env(
            &fixture.path().join("app"),
            &empty_path_dir.path().to_string_lossy(),
            &[],
        );
        assert_all_unknown(&cdx);
        assert!(
            stderr.contains("skipped=no-toolchain"),
            "FR-013 summary must report skipped=no-toolchain: {stderr}",
        );
        assert!(
            stderr.contains("unknown_marked=3"),
            "FR-013 summary must count the three unknown markers: {stderr}",
        );
    }

    const STUB_WHY_FAILS: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)   exit 1 ;;
    esac ;;
esac
exit 1
"##;

    /// (b) `go mod why` exits non-zero → per-chunk `subprocess-error`
    /// degrade; the chunk's modules fall back to unknown.
    #[test]
    fn mod_why_subprocess_error_degrades_to_unknown_markers() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());
        let stub = write_go_stub(STUB_WHY_FAILS);
        let real_path = std::env::var("PATH").expect("PATH set");
        let (cdx, stderr) = scan_with_env(
            &fixture.path().join("app"),
            &format!("{}:{real_path}", stub.path().to_string_lossy()),
            &[],
        );
        assert_all_unknown(&cdx);
        assert!(
            stderr.contains("subprocess-error"),
            "per-degrade warn line must name the subprocess-error class: {stderr}",
        );
        assert!(
            stderr.contains("unresolved=3") && stderr.contains("skipped=none"),
            "FR-013 summary must show all modules unresolved without a \
             whole-scan skip: {stderr}",
        );
    }

    const STUB_WHY_HANGS: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)   sleep 30; exit 0 ;;
    esac ;;
esac
exit 1
"##;

    /// (c) `go mod why` outlives the shared budget → `budget-exhausted`
    /// skip; the test shortens the budget via the contract's
    /// `MIKEBOM_GO_MOD_WHY_BUDGET_MS` override so it stays fast.
    #[test]
    fn budget_exhaustion_degrades_to_unknown_markers() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());
        let stub = write_go_stub(STUB_WHY_HANGS);
        let real_path = std::env::var("PATH").expect("PATH set");
        let (cdx, stderr) = scan_with_env(
            &fixture.path().join("app"),
            &format!("{}:{real_path}", stub.path().to_string_lossy()),
            &[("MIKEBOM_GO_MOD_WHY_BUDGET_MS", "300")],
        );
        assert_all_unknown(&cdx);
        assert!(
            stderr.contains("budget-exhausted"),
            "per-degrade warn line must name the budget-exhausted class: {stderr}",
        );
        assert!(
            stderr.contains("skipped=budget-exhausted"),
            "FR-013 summary must report skipped=budget-exhausted: {stderr}",
        );
    }

    const STUB_PARTIAL_OUTPUT: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)
        shift 4   # drop: mod why -m -vendor
        for m in "$@"; do
          case "$m" in
            *prod*)
              echo "# $m"
              echo "example.com/sourceonly"
              echo "$m" ;;
          esac
        done
        exit 0 ;;
    esac ;;
esac
exit 1
"##;

    /// (d) `go mod why` succeeds but answers for only SOME queried
    /// modules → answered modules keep their verdicts, the rest fall
    /// back to unknown.
    #[test]
    fn partial_output_keeps_classified_verdicts_rest_unknown() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());
        let stub = write_go_stub(STUB_PARTIAL_OUTPUT);
        let real_path = std::env::var("PATH").expect("PATH set");
        let (cdx, stderr) = scan_with_env(
            &fixture.path().join("app"),
            &format!("{}:{real_path}", stub.path().to_string_lossy()),
            &[],
        );

        // The answered module is classified ProdNeeded: no marker.
        let prod = cdx_component_by_purl(&cdx, ALL_PURLS[2]);
        assert_eq!(
            cdx_property(prod, "mikebom:build-inclusion"),
            None,
            "answered module must keep its prod verdict (no marker)",
        );
        assert!(prod.get("scope").is_none());

        // The two unanswered modules fall back to unknown.
        for purl in &ALL_PURLS[..2] {
            let c = cdx_component_by_purl(&cdx, purl);
            assert_eq!(
                cdx_property(c, "mikebom:build-inclusion"),
                Some("unknown"),
                "{purl}: unanswered module must fall back to unknown",
            );
            assert!(c.get("scope").is_none());
        }
        assert!(
            stderr.contains("prod=1") && stderr.contains("unresolved=2"),
            "FR-013 summary must reflect the partial classification: {stderr}",
        );
    }

    const STUB_PREFLIGHT_FAILS: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 1 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)
        shift 4   # drop: mod why -m -vendor
        for m in "$@"; do
          echo "# $m"
          echo "(main module does not need module $m)"
        done
        exit 0 ;;
    esac ;;
esac
exit 1
"##;

    /// (e) `go list all` preflight fails → skip reason
    /// `unresolvable-packages` and ZERO verdicts accepted, even though
    /// the stub's `go mod why` would happily report every module as
    /// not-needed (the silent-false-negative guard).
    #[test]
    fn preflight_failure_rejects_all_verdicts() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());
        let stub = write_go_stub(STUB_PREFLIGHT_FAILS);
        let real_path = std::env::var("PATH").expect("PATH set");
        let (cdx, stderr) = scan_with_env(
            &fixture.path().join("app"),
            &format!("{}:{real_path}", stub.path().to_string_lossy()),
            &[],
        );
        assert_all_unknown(&cdx);
        for c in cdx["components"].as_array().expect("components array") {
            assert_ne!(
                cdx_property(c, "mikebom:build-inclusion"),
                Some("not-needed"),
                "preflight failure must reject every not-needed verdict: {}",
                c["purl"],
            );
        }
        assert!(
            stderr.contains("skipped=unresolvable-packages"),
            "FR-013 summary must report skipped=unresolvable-packages: {stderr}",
        );
        assert!(
            stderr.contains("analyzed=0"),
            "no verdicts may be accepted after a failed preflight: {stderr}",
        );
    }
}

// ---------------------------------------------------------------------
// Milestone 112 US3 (T022) — FR-012 offline env pinning. With
// `--offline`, every `go mod why` child must see GOPROXY=off,
// GOFLAGS=-mod=mod, GOTOOLCHAIN=local so the toolchain can neither hit
// the network nor self-upgrade.
// ---------------------------------------------------------------------

#[cfg(unix)]
mod offline_env {
    use super::degrade_matrix::scan_with_env;
    use super::stub_toolchain::{write_go_stub, write_three_verdict_fixture};

    #[test]
    fn offline_scan_pins_go_env_for_mod_why_children() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_three_verdict_fixture(fixture.path());

        let dump_dir = tempfile::tempdir().expect("dump tempdir");
        let dump_path = dump_dir.path().join("env.txt");
        let script = format!(
            r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)
        printf 'GOPROXY=%s\nGOFLAGS=%s\nGOTOOLCHAIN=%s\n' \
          "$GOPROXY" "$GOFLAGS" "$GOTOOLCHAIN" > "{dump}"
        shift 4   # drop: mod why -m -vendor
        for m in "$@"; do
          echo "# $m"
          echo "example.com/sourceonly"
          echo "$m"
        done
        exit 0 ;;
    esac ;;
esac
exit 1
"##,
            dump = dump_path.to_string_lossy(),
        );
        let stub = write_go_stub(&script);
        let real_path = std::env::var("PATH").expect("PATH set");
        // scan_with_env always passes --offline.
        let (_cdx, _stderr) = scan_with_env(
            &fixture.path().join("app"),
            &format!("{}:{real_path}", stub.path().to_string_lossy()),
            &[],
        );

        let dumped =
            std::fs::read_to_string(&dump_path).expect("stub must have dumped its env");
        for line in ["GOPROXY=off", "GOFLAGS=-mod=mod", "GOTOOLCHAIN=local"] {
            assert!(
                dumped.contains(line),
                "offline `go mod why` child must see {line} (FR-012); got:\n{dumped}",
            );
        }
    }
}

// ---------------------------------------------------------------------
// Milestone 112 US3 (T023) — FR-008/SC-004 byte-identity regression.
// A fixture WITHOUT fallback-discovered modules has nothing for either
// milestone-112 pass to touch: the Part B marker pass finds no
// fallback population and the classification pass has an empty query.
// The emitted bytes must therefore be identical whether classification
// is disabled (`MIKEBOM_NO_GO_MOD_WHY=1` — the pre-feature emission
// shape) or enabled with a working toolchain.
// ---------------------------------------------------------------------

#[cfg(unix)]
mod byte_identity {
    use super::stub_toolchain::{write_go_stub, STUB_THREE_VERDICTS};
    use super::{cdx_component_by_purl, cdx_property};
    use std::path::Path;
    use std::process::Command;

    /// Single go.sum module whose `.mod` is staged in GOMODCACHE, so
    /// the milestone-055 resolver settles it via the cache walk — NO
    /// fallback-discovered modules anywhere in the scan.
    fn write_no_fallback_fixture(root: &Path) {
        let app = root.join("app");
        std::fs::create_dir_all(&app).expect("create app dir");
        std::fs::write(
            app.join("go.mod"),
            "module example.com/sourceonly\n\
             go 1.22\n\
             require github.com/graph-resolved/dep v1.2.3\n",
        )
        .expect("write go.mod");
        std::fs::write(
            app.join("go.sum"),
            "github.com/graph-resolved/dep v1.2.3 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
        )
        .expect("write go.sum");
        let modcache_v =
            root.join("gomodcache/cache/download/github.com/graph-resolved/dep/@v");
        std::fs::create_dir_all(&modcache_v).expect("create gomodcache dirs");
        std::fs::write(
            modcache_v.join("v1.2.3.mod"),
            "module github.com/graph-resolved/dep\ngo 1.22\n",
        )
        .expect("write cached .mod");
    }

    /// Scan and return the RAW emitted CDX bytes (no normalization)
    /// with the timestamp pinned so the only legitimate run-to-run
    /// variance left is the random serialNumber.
    fn scan_raw(fixture: &Path, stub_dir: &Path, disable_classification: bool) -> String {
        let out = tempfile::tempdir().expect("output tempdir");
        let cdx_path = out.path().join("out.cdx.json");
        let fake_home = tempfile::tempdir().expect("fake-home tempdir");
        let real_path = std::env::var("PATH").expect("PATH set");

        let bin = env!("CARGO_BIN_EXE_mikebom");
        let mut cmd = Command::new(bin);
        cmd.env("PATH", format!("{}:{real_path}", stub_dir.to_string_lossy()))
            .env("HOME", fake_home.path())
            .env("GOMODCACHE", fixture.join("gomodcache"))
            .env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
        if disable_classification {
            cmd.env("MIKEBOM_NO_GO_MOD_WHY", "1");
        } else {
            cmd.env_remove("MIKEBOM_NO_GO_MOD_WHY");
        }
        let output = cmd
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(fixture.join("app"))
            .arg("--output")
            .arg(&cdx_path)
            .arg("--no-deep-hash")
            .output()
            .expect("mikebom should run");
        assert!(
            output.status.success(),
            "scan failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read_to_string(&cdx_path).expect("read cdx")
    }

    /// Mask the per-scan random serialNumber so the remaining bytes
    /// can be compared exactly.
    fn mask_serial(raw: &str) -> String {
        let json: serde_json::Value = serde_json::from_str(raw).expect("valid cdx JSON");
        let serial = json["serialNumber"].as_str().expect("serialNumber present");
        raw.replace(serial, "urn:uuid:MASKED")
    }

    #[test]
    fn no_fallback_fixture_is_byte_identical_with_feature_disabled() {
        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_no_fallback_fixture(fixture.path());
        let stub = write_go_stub(STUB_THREE_VERDICTS);

        let disabled = scan_raw(fixture.path(), stub.path(), true);
        let enabled = scan_raw(fixture.path(), stub.path(), false);

        // Fixture invariant: the module is cache-resolved, not
        // fallback-discovered.
        let cdx: serde_json::Value =
            serde_json::from_str(&disabled).expect("valid cdx JSON");
        let dep =
            cdx_component_by_purl(&cdx, "pkg:golang/github.com/graph-resolved/dep@v1.2.3");
        assert_eq!(
            cdx_property(dep, "mikebom:resolver-step"),
            None,
            "fixture invariant — no module may be fallback-discovered",
        );

        // FR-008: zero milestone-112 artifacts in either emission.
        for raw in [&disabled, &enabled] {
            assert!(
                !raw.contains("build-inclusion"),
                "no-fallback scan must emit zero milestone-112 artifacts",
            );
        }

        // SC-004: emission with the feature disabled (the pre-feature
        // shape) is byte-identical to emission with classification
        // enabled — the passes must not perturb output when there is
        // nothing to mark or classify.
        assert_eq!(
            mask_serial(&disabled),
            mask_serial(&enabled),
            "no-fallback fixture emission must be byte-identical with \
             classification disabled vs enabled (FR-008/SC-004)",
        );
    }
}

// ---------------------------------------------------------------------
// Milestone 112 US3 (T024) — env-gated REAL-toolchain e2e. Skipped by
// default (like the docker-daemon and OCI-network gates); opt in with
// `MIKEBOM_GO_TOOLCHAIN_E2E=1` on a host with `go` installed.
//
// The fixture needs no network even against the real toolchain: a
// stdlib-only main module plus a go.sum-only entry (the exact shape of
// the kusari-cli anchor case — sums retained for a module outside the
// build list). `go list all` passes without downloads and
// `go mod why -m -vendor` answers "(main module does not need to
// vendor module …)" offline — verified on go 1.26.2.
// ---------------------------------------------------------------------

mod real_toolchain_e2e {
    use super::{cdx_component_by_purl, cdx_property};
    use std::process::Command;

    #[test]
    fn real_go_toolchain_yields_not_needed_verdict() {
        if std::env::var("MIKEBOM_GO_TOOLCHAIN_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping: set MIKEBOM_GO_TOOLCHAIN_E2E=1 to run the \
                 real-toolchain e2e test"
            );
            return;
        }

        let fixture = tempfile::tempdir().expect("fixture tempdir");
        let app = fixture.path().join("app");
        std::fs::create_dir_all(&app).expect("create app dir");
        std::fs::write(
            app.join("go.mod"),
            "module example.com/e2e\n\ngo 1.22\n",
        )
        .expect("write go.mod");
        std::fs::write(app.join("main.go"), "package main\n\nfunc main() {}\n")
            .expect("write main.go");
        // go.sum-only module (not required by go.mod): outside the
        // build list, so it attaches via the milestone-091 flat
        // fallback and gets queried by the classification pass.
        std::fs::write(
            app.join("go.sum"),
            concat!(
                "gopkg.in/yaml.v3 v3.0.1 h1:fxVm/GzAzEWqLHuvctI91KS9hhNmmWOoWu0XTYJS7CA=\n",
                "gopkg.in/yaml.v3 v3.0.1/go.mod h1:K4uyk7z7BCEPqu6E+C64Yfv1cQ7kz7rIZviUmN+EgEM=\n",
            ),
        )
        .expect("write go.sum");

        let out = tempfile::tempdir().expect("output tempdir");
        let cdx_path = out.path().join("out.cdx.json");
        let fake_home = tempfile::tempdir().expect("fake-home tempdir");
        let bin = env!("CARGO_BIN_EXE_mikebom");
        let output = Command::new(bin)
            .env("HOME", fake_home.path())
            .env("GOMODCACHE", fake_home.path().join("no-gomodcache"))
            .env_remove("MIKEBOM_NO_GO_MOD_WHY")
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(&app)
            .arg("--output")
            .arg(&cdx_path)
            .arg("--no-deep-hash")
            .output()
            .expect("mikebom should run");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "scan must exit 0: stderr={stderr}");

        let raw = std::fs::read_to_string(&cdx_path).expect("read cdx");
        let cdx: serde_json::Value = serde_json::from_str(&raw).expect("valid cdx JSON");
        let yaml = cdx_component_by_purl(&cdx, "pkg:golang/gopkg.in/yaml.v3@v3.0.1");
        assert_eq!(
            cdx_property(yaml, "mikebom:build-inclusion"),
            Some("not-needed"),
            "real toolchain must classify the go.sum-only module as \
             not-needed; stderr={stderr}",
        );
        assert_eq!(yaml["scope"].as_str(), Some("excluded"));
        assert_eq!(
            cdx_property(yaml, "mikebom:build-inclusion-derivation"),
            Some("go-mod-why"),
        );
        assert!(
            stderr.contains("not_needed=1"),
            "FR-013 summary must count the verdict: {stderr}",
        );
    }
}
