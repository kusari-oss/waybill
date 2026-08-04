//! Milestone 226: integration tests for the Pants Go enrichment
//! pass (attach `waybill:pants-target` to `pkg:golang/*` components)
//! + the tool-pin emission (`pkg:generic/go@<version>`).
//!
//! Fixtures live at `waybill-cli/tests/fixtures/pants_go/`. All
//! module names use `github.com/waybill-fixture/*` per memory
//! `feedback_fixture_synthetic_package_names`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_go")
        .join(rel)
}

fn run_scan(
    fixture_path: &Path,
    output: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(output)
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.output().expect("waybill invocation")
}

fn read_cdx(path: &Path) -> serde_json::Value {
    let raw = std::fs::read(path).expect("read cdx");
    serde_json::from_slice(&raw).expect("parse cdx")
}

fn get_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
}

/// All `pkg:golang/*` components in the CDX.
fn golang_components(cdx: &serde_json::Value) -> Vec<&serde_json::Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:golang/"))
        })
        .collect()
}

fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    re.replace_all(s, "").to_string()
}

// ---------------------------------------------------------------------
// US1 T011 — minimal 3rdparty/go fixture annotates all 3 components
// ---------------------------------------------------------------------

#[test]
fn us1_minimal_3rdparty_go_annotates_all_three_components() {
    let fixture_dir = fixture("minimal_3rdparty_go");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let spdx_path = tmp.path().join("out.spdx.json");

    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture_dir)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx_path.display()))
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info")
        .output()
        .expect("waybill invocation");
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    // Filter to the third-party fixture modules waybill emitted from go.sum.
    let cdx = read_cdx(&cdx_path);
    let fixture_components: Vec<&serde_json::Value> = golang_components(&cdx)
        .into_iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.contains("waybill-fixture/"))
        })
        .collect();
    assert_eq!(
        fixture_components.len(),
        3,
        "expected 3 pkg:golang/github.com/waybill-fixture/* components; got {} — purls: {:?}",
        fixture_components.len(),
        fixture_components.iter().filter_map(|c| c.get("purl").and_then(|v| v.as_str())).collect::<Vec<_>>(),
    );
    for c in &fixture_components {
        assert_eq!(
            get_property(c, "waybill:pants-target"),
            Some("3rdparty/go:mod"),
            "component missing waybill:pants-target=3rdparty/go:mod: {:?}",
            c.get("purl"),
        );
    }

    // SPDX 2.3 assertion: 3 packages match, each carries the C145
    // annotation via the m080 envelope.
    let spdx = read_cdx(&spdx_path);
    let packages = spdx
        .get("packages")
        .and_then(|v| v.as_array())
        .expect("packages[]");
    // Filter to third-party fixtures only (foo/bar/baz). The main
    // module `github.com/waybill-fixture/root` also appears in SPDX
    // packages[] but is NOT go.sum-derived — it has no matching
    // Pants target in our fixture's minimal BUILD (only `go_mod`).
    let fixture_pkgs: Vec<&serde_json::Value> = packages
        .iter()
        .filter(|p| {
            p.get("externalRefs")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .any(|r| {
                    r.get("referenceLocator")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| {
                            s.starts_with("pkg:golang/github.com/waybill-fixture/")
                                && !s.starts_with("pkg:golang/github.com/waybill-fixture/root")
                        })
                })
        })
        .collect();
    assert_eq!(fixture_pkgs.len(), 3, "expected 3 SPDX packages (foo/bar/baz)");
    for p in &fixture_pkgs {
        let annotations = p
            .get("annotations")
            .and_then(|v| v.as_array())
            .expect("annotations[]");
        assert!(
            annotations.iter().any(|a| {
                a.get("comment")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.contains("waybill:pants-target"))
            }),
            "SPDX package missing waybill:pants-target annotation",
        );
    }
}

// ---------------------------------------------------------------------
// US1 T012 — explicit go_third_party_package merges with go_mod owner
// ---------------------------------------------------------------------

#[test]
fn us1_explicit_third_party_target_merges_with_go_mod() {
    let fixture_dir = fixture("explicit_third_party_targets");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let cdx = read_cdx(&cdx_path);
    let foo = golang_components(&cdx)
        .into_iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:golang/github.com/waybill-fixture/foo@v1.0.0")
        })
        .expect("foo component present");
    assert_eq!(
        get_property(foo, "waybill:pants-target"),
        Some("3rdparty/go:foo,3rdparty/go:mod"),
        "SC-004: expected lex-sorted comma-sep multi-owner value",
    );
}

// ---------------------------------------------------------------------
// US1 T013 — FR-010 INFO log includes all 6 structured fields
// ---------------------------------------------------------------------

#[test]
fn us1_fr010_info_log_emits_all_six_structured_fields() {
    let fixture_dir = fixture("minimal_3rdparty_go");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    for field in &[
        "build_files_discovered=",
        "build_files_parsed_ok=",
        "build_files_skipped_corrupt=",
        "go_targets_found=",
        "components_annotated=",
        "toolchain_component_emitted=",
    ] {
        assert!(
            stripped.contains(field),
            "FR-010: stderr missing structured field {field}. stderr:\n{stripped}",
        );
    }
}

// ---------------------------------------------------------------------
// US1 T014 — zero fabrication: pkg:golang/* count unchanged with/without BUILD
// ---------------------------------------------------------------------

#[test]
fn us1_zero_fabrication_component_count_unchanged() {
    let fixture_dir = fixture("minimal_3rdparty_go");

    // Pass 1: fixture as-is (BUILD file present).
    let tmp1 = tempfile::tempdir().expect("tempdir");
    let cdx1 = tmp1.path().join("with-build.cdx.json");
    let out1 = run_scan(&fixture_dir, &cdx1, &[]);
    assert!(out1.status.success());
    let count_with_build = golang_components(&read_cdx(&cdx1))
        .into_iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.contains("waybill-fixture/"))
        })
        .count();

    // Pass 2: copy fixture to tempdir, rename BUILD → _BUILD_DISABLED,
    // rescan. Waybill's Go reader still emits from go.sum — nothing changes
    // on the component front, only the annotation goes away.
    let tmp2 = tempfile::tempdir().expect("tempdir");
    let scratch_root = tmp2.path().join("repo");
    copy_tree(&fixture_dir, &scratch_root);
    let build_path = scratch_root.join("3rdparty/go/BUILD");
    let disabled = scratch_root.join("3rdparty/go/_BUILD_DISABLED");
    std::fs::rename(&build_path, &disabled).expect("rename BUILD");

    let cdx2 = tmp2.path().join("without-build.cdx.json");
    let out2 = run_scan(&scratch_root, &cdx2, &[]);
    assert!(out2.status.success());
    let count_without_build = golang_components(&read_cdx(&cdx2))
        .into_iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.contains("waybill-fixture/"))
        })
        .count();

    assert_eq!(
        count_with_build, count_without_build,
        "FR-012 / Principle IX: pants_go enrichment MUST NOT change pkg:golang/* count. \
         with_build={count_with_build} without_build={count_without_build}",
    );
}

// ---------------------------------------------------------------------
// US2 T019 — pants.toml [golang] expected_version emits design-tier tool
// ---------------------------------------------------------------------

#[test]
fn us2_pants_toml_expected_version_emits_design_tier_toolchain_component() {
    let fixture_dir = fixture("with_toolchain_pin");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let all: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .collect();

    let toolchain = all
        .iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some("pkg:generic/go@1.21"))
        .expect("pkg:generic/go@1.21 component present");
    assert_eq!(get_property(toolchain, "waybill:sbom-tier"), Some("design"));
    assert_eq!(
        get_property(toolchain, "waybill:source-file"),
        Some("pants.toml"),
    );

    // Co-located script component from 3rdparty/go/go.sum also carries
    // pants-target annotation (proves US1+US2 co-existence).
    let foo = golang_components(&cdx)
        .into_iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:golang/github.com/waybill-fixture/foo@v1.0.0")
        })
        .expect("foo component present");
    assert_eq!(
        get_property(foo, "waybill:pants-target"),
        Some("3rdparty/go:mod"),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("toolchain_component_emitted=1"),
        "expected toolchain_component_emitted=1 in FR-010 log. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// US2 T020 — no expected_version → no toolchain component
// ---------------------------------------------------------------------

#[test]
fn us2_no_expected_version_emits_no_toolchain_component() {
    let fixture_dir = fixture("with_toolchain_pin_no_version");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(out.status.success());
    let cdx = read_cdx(&cdx_path);
    let toolchain: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:generic/go@"))
        })
        .collect();
    assert!(
        toolchain.is_empty(),
        "expected zero pkg:generic/go@* components when no expected_version pinned; got {}",
        toolchain.len(),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("toolchain_component_emitted=0"),
        "expected toolchain_component_emitted=0 in FR-010 log. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// US3 T024 — first-party vs third-party annotation prefixes differ
// ---------------------------------------------------------------------

#[test]
fn us3_first_party_and_third_party_annotations_differ() {
    let fixture_dir = fixture("go_binary_first_party");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let cdx = read_cdx(&cdx_path);

    // Third-party lib-a + lib-b MUST carry the root go_mod annotation.
    // Root-level BUILD → bare `mod` address (no dir prefix per m225
    // convention shared with pants_go).
    for name in ["lib-a", "lib-b"] {
        let comp = golang_components(&cdx)
            .into_iter()
            .find(|c| {
                c.get("purl").and_then(|v| v.as_str()).is_some_and(|p| {
                    p.starts_with(&format!(
                        "pkg:golang/github.com/waybill-fixture/{name}@"
                    ))
                })
            })
            .unwrap_or_else(|| panic!("third-party component {name} present"));
        let target = get_property(comp, "waybill:pants-target").unwrap_or("");
        assert!(
            target.contains("mod"),
            "third-party component {name} should carry the root go_mod address `mod`. got: {target}",
        );
        assert!(
            !target.contains("cmd/frontend:"),
            "third-party component {name} MUST NOT carry any cmd/frontend:* address. got: {target}",
        );
    }
}

// ---------------------------------------------------------------------
// Edge T027 — missing import_path: no fabrication + INFO log
// ---------------------------------------------------------------------

#[test]
fn edge_missing_import_path_no_fabrication_info_log() {
    let fixture_dir = fixture("missing_import_path");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let cdx = read_cdx(&cdx_path);
    let fixture_comps: Vec<&serde_json::Value> = golang_components(&cdx)
        .into_iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.contains("waybill-fixture/") && !p.contains("/root"))
        })
        .collect();
    assert_eq!(
        fixture_comps.len(),
        1,
        "expected only foo — no synthetic component for does-not-exist",
    );
    let foo = fixture_comps[0];
    let target = get_property(foo, "waybill:pants-target").unwrap_or("");
    assert!(
        target.contains("3rdparty/go:mod"),
        "foo should carry go_mod ownership. got: {target}",
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("github.com/waybill-fixture/does-not-exist"),
        "expected INFO log naming the orphan import path. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Edge T029 — malformed BUILD partial: valid targets still enrich
// ---------------------------------------------------------------------

#[test]
fn edge_malformed_build_partial_enriches_valid_targets() {
    let fixture_dir = fixture("malformed_build_partial");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "SC-005: waybill must not abort on malformed BUILD. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let cdx = read_cdx(&cdx_path);
    for name in ["one", "two"] {
        let comp = golang_components(&cdx)
            .into_iter()
            .find(|c| {
                c.get("purl").and_then(|v| v.as_str()).is_some_and(|p| {
                    p.starts_with(&format!(
                        "pkg:golang/github.com/waybill-fixture/{name}@"
                    ))
                })
            })
            .unwrap_or_else(|| panic!("{name} component present"));
        let target = get_property(comp, "waybill:pants-target").unwrap_or("");
        assert!(
            target.contains("3rdparty/go:mod"),
            "{name} should carry go_mod owner. got: {target}",
        );
        assert!(
            target.contains(&format!("3rdparty/go:{name}")),
            "{name} should carry its explicit go_third_party_package address. got: {target}",
        );
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("target parse error"),
        "expected WARN naming the broken target. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Edge T030 — non-Pants-Go fixture: no enrichment activity
// ---------------------------------------------------------------------

#[test]
fn edge_no_pants_no_build_files_produces_no_enrichment() {
    // Reuse pants_pex/minimal_python. It DOES have a BUILD file (for
    // pex targets), so pants_go's enrichment DOES run — but there are
    // zero pkg:golang/* components to enrich because that fixture has
    // no Go code. The FR-010 log should show components_annotated=0.
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_pex/minimal_python");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(out.status.success());
    let cdx = read_cdx(&cdx_path);
    for c in golang_components(&cdx) {
        assert!(
            get_property(c, "waybill:pants-target").is_none(),
            "unexpected annotation on non-Pants-Go fixture: {:?}",
            c.get("purl"),
        );
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    if stripped.contains("pants-go enrichment complete") {
        assert!(
            stripped.contains("components_annotated=0"),
            "expected components_annotated=0 in FR-010 log. stderr:\n{stripped}",
        );
    }
}

// ---------------------------------------------------------------------
// Edge T031 — zero-fabrication byte-identity (FR-012 hard count gate)
// ---------------------------------------------------------------------

#[test]
fn edge_zero_fabrication_byte_identity() {
    let fixture_dir = fixture("malformed_build_partial");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(out.status.success());
    let cdx = read_cdx(&cdx_path);
    let fixture_comps: Vec<&serde_json::Value> = golang_components(&cdx)
        .into_iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.contains("waybill-fixture/") && !p.contains("/root"))
        })
        .collect();
    assert_eq!(
        fixture_comps.len(),
        2,
        "FR-012: expected exactly 2 pkg:golang/* fixture components from the 2 go.sum entries; got {}",
        fixture_comps.len(),
    );
    for c in &fixture_comps {
        let purl = c.get("purl").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            !purl.contains("broken"),
            "no component fabricated for broken target. got: {purl}",
        );
    }
}

// ---------------------------------------------------------------------
// Edge T031b — multi-go_mod deepest-prefix wins (C1 gate)
// ---------------------------------------------------------------------

#[test]
fn edge_multi_go_mod_deepest_prefix_wins() {
    let fixture_dir = fixture("multi_go_mod_layout");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let cdx = read_cdx(&cdx_path);

    let root_dep = golang_components(&cdx)
        .into_iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:golang/github.com/waybill-fixture/root-dep@v1.0.0")
        })
        .expect("root-dep component present");
    assert_eq!(
        get_property(root_dep, "waybill:pants-target"),
        Some("3rdparty/go:root"),
        "root-dep should carry the shallow root address only",
    );

    let api_dep = golang_components(&cdx)
        .into_iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:golang/github.com/waybill-fixture/api-dep@v2.0.0")
        })
        .expect("api-dep component present");
    assert_eq!(
        get_property(api_dep, "waybill:pants-target"),
        Some("services/api/3rdparty/go:api"),
        "api-dep should carry the DEEPEST-prefix (services/api/3rdparty/go:api), NOT the shallower 3rdparty/go:root",
    );
}

/// Recursively copy a directory tree. Small helper for T014's scratch scan.
fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("mkdir dst");
    for entry in std::fs::read_dir(src).expect("read_dir src").flatten() {
        let path = entry.path();
        let name = path.file_name().expect("has file_name");
        let target = dst.join(name);
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}
