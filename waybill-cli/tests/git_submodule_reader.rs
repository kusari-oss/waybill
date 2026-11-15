//! Spec 1530 (#1515): git submodules as components. Every repository is
//! built in a tempdir from local `file://` repositories; nothing is
//! fetched (tests/common/git_fixture.rs).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;
use std::process::Command;

use common::git_fixture::GitFixture;
use serde_json::Value;

const GH_URL: &str = "https://github.com/waybill-org/waybill-fixture-dep.git";

/// The three documents of one scan.
struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    stderr: String,
}

fn scan_env(fx: &GitFixture, path: &Path, envs: &[(&str, &str)]) -> Docs {
    let out_dir = tempfile::tempdir().unwrap();
    let (c, s2, s3) = (
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    );
    let mut cmd = Command::new(common::bin());
    cmd.env("HOME", fx.home())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", fx.home().join(".gitconfig"))
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()));
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    Docs {
        cdx: read(&c),
        spdx23: read(&s2),
        spdx3: read(&s3),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn scan(fx: &GitFixture, path: &Path) -> Docs {
    scan_env(fx, path, &[])
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// CycloneDX components the git-submodule reader emitted, as
/// `(purl, version, build-reference)`, sorted.
fn submodules(cdx: &Value) -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = cdx["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| prop(c, "waybill:source-mechanism") == Some("git-submodule"))
        .map(|c| {
            (
                c["purl"].as_str().unwrap_or("").to_string(),
                c["version"].as_str().unwrap_or("").to_string(),
                prop(c, "waybill:build-reference").unwrap_or("").to_string(),
            )
        })
        .collect();
    v.sort();
    v
}

fn root_depends_on(cdx: &Value) -> Vec<String> {
    let root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or("").to_string();
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["ref"] == root.as_str())
        .and_then(|d| d["dependsOn"].as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn coverage(cdx: &Value) -> Option<Value> {
    cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == "waybill:git-submodule-coverage")
        .and_then(|p| serde_json::from_str(p["value"].as_str()?).ok())
}

/// A super-project `app` pinning `dep` at `third_party/waybill-fixture-dep`,
/// with its `.gitmodules` URL rewritten to a synthetic GitHub URL.
fn app_with_github_submodule(fx: &GitFixture) -> String {
    let dep_commit = fx.repo("dep");
    fx.repo("app");
    fx.add_submodule("app", "dep", "third_party/waybill-fixture-dep");
    let app = fx.path("app");
    fx.git(&app, &["config", "-f", ".gitmodules", "submodule.third_party/waybill-fixture-dep.url", GH_URL]);
    fx.git(&app, &["commit", "-q", "-am", "url"]);
    dep_commit
}

/// SC-001 (#1515's reproduction): one component, pinned to the gitlink's
/// commit, both checked out and on a fresh clone with the submodule
/// uninitialised; anchored to the root.
#[test]
fn sc001_fresh_clone_pins_the_gitlink_commit() {
    let fx = GitFixture::new();
    let dep_commit = app_with_github_submodule(&fx);
    let want = format!("pkg:github/waybill-org/waybill-fixture-dep@{dep_commit}");
    let fresh = fx.fresh_clone("app", "fresh");
    assert!(
        std::fs::read_dir(fresh.join("third_party/waybill-fixture-dep")).unwrap().next().is_none(),
        "control: the fresh clone's submodule is uninitialised"
    );
    for path in [fx.path("app"), fresh] {
        let docs = scan(&fx, &path);
        let subs = submodules(&docs.cdx);
        assert_eq!(subs.len(), 1, "{}: {subs:?}", path.display());
        assert_eq!(subs[0].0, want, "{}", path.display());
        assert_eq!(subs[0].1, dep_commit);
        assert!(root_depends_on(&docs.cdx).contains(&want), "the root depends on the submodule");
    }
}

/// FR-010 / research R5 (#1009's lesson): the root keeps its edge to a
/// top-level submodule even when it already has other edges, so the
/// emitters' "root depends on every parentless component" fallback does not
/// fire. Control: the root really does have another edge.
#[test]
fn root_with_other_edges_still_depends_on_the_submodule() {
    let fx = GitFixture::new();
    let dep_commit = app_with_github_submodule(&fx);
    fx.write(
        "app/pyproject.toml",
        "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\ndependencies = [\"waybill-fixture-req==2.0.0\"]\n",
    );
    let app = fx.path("app");
    fx.git(&app, &["add", "pyproject.toml"]);
    fx.git(&app, &["commit", "-q", "-m", "pyproject"]);
    let docs = scan(&fx, &app);
    let root_deps = root_depends_on(&docs.cdx);
    assert!(
        root_deps.iter().any(|d| d.contains("waybill-fixture-req")),
        "control: the root has another edge, so the parentless fallback cannot fire; got {root_deps:?}"
    );
    let want = format!("pkg:github/waybill-org/waybill-fixture-dep@{dep_commit}");
    assert!(root_deps.contains(&want), "the root depends on the submodule; got {root_deps:?}");
    let spdx_rel = docs.spdx23["relationships"].to_string();
    assert!(spdx_rel.contains("DEPENDS_ON"), "SPDX 2.3 carries dependency relationships");
}

/// SC-010: a program the scanned repository names in its own config never
/// runs. Positive control: `git ls-files` run directly does run it.
#[cfg(unix)]
#[test]
fn sc010_repository_config_runs_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let fx = GitFixture::new();
    app_with_github_submodule(&fx);
    let app = fx.path("app");
    let marker = fx.path("PAYLOAD-RAN");
    let payload = fx.path("payload.sh");
    std::fs::write(&payload, format!("#!/bin/sh\necho ran >> '{}'\nexit 0\n", marker.display())).unwrap();
    std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hooks = fx.path("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for h in ["post-checkout", "post-index-change", "reference-transaction"] {
        std::fs::copy(&payload, hooks.join(h)).unwrap();
    }
    let p = payload.to_str().unwrap();
    fx.git(&app, &["config", "core.fsmonitor", p]);
    fx.git(&app, &["config", "core.pager", p]);
    fx.git(&app, &["config", "core.hooksPath", hooks.to_str().unwrap()]);

    let docs = scan(&fx, &app);
    assert!(!marker.exists(), "a program the scanned repository configured ran during the scan");
    assert_eq!(submodules(&docs.cdx).len(), 1, "the submodule is still emitted");

    // Positive control: the payload is live, so its absence above means
    // something.
    let _ = Command::new("git").current_dir(&app).env("HOME", fx.home()).args(["ls-files", "--stage"]).output();
    assert!(marker.exists(), "control: `git ls-files` runs the configured fsmonitor");
}

/// SC-004: credentials in a submodule URL appear nowhere in any format.
#[test]
fn sc004_credentials_never_emitted() {
    let fx = GitFixture::new();
    fx.repo("app");
    let commit = "1111111111111111111111111111111111111111";
    fx.write(
        "app/.gitmodules",
        "[submodule \"x\"]\n\tpath = third_party/x\n\turl = https://waybill-fixture-user:waybill-fixture-token@example.invalid/waybill-fixture-x.git\n",
    );
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules"]);
    fx.add_gitlink("app", "third_party/x", commit);
    let docs = scan(&fx, &app);
    assert_eq!(submodules(&docs.cdx).len(), 1);
    for (fmt, doc) in [("cdx", &docs.cdx), ("spdx23", &docs.spdx23), ("spdx3", &docs.spdx3)] {
        let s = doc.to_string();
        assert!(!s.contains("waybill-fixture-token"), "{fmt} carries the token");
        assert!(!s.contains("waybill-fixture-user"), "{fmt} carries the user");
    }
    assert!(!docs.stderr.contains("waybill-fixture-token"), "the log carries the token");
}

/// SC-008: C57 is `declared-and-used` for a path reference and for a
/// `find_package` name match, and `declared-only` for neither.
#[test]
fn sc008_build_reference_classification() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write(
        "app/.gitmodules",
        "[submodule \"a\"]\n\tpath = third_party/waybill-fixture-a\n\turl = https://github.com/waybill-org/waybill-fixture-a.git\n\
         [submodule \"b\"]\n\tpath = third_party/waybill-fixture-b\n\turl = https://github.com/waybill-org/waybill-fixture-b.git\n\
         [submodule \"c\"]\n\tpath = third_party/waybill-fixture-c\n\turl = https://github.com/waybill-org/waybill-fixture-c.git\n",
    );
    fx.write(
        "app/CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.25)\nproject(p)\nadd_subdirectory(third_party/waybill-fixture-a)\nfind_package(waybill-fixture-b)\n",
    );
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules", "CMakeLists.txt"]);
    for (i, n) in ["a", "b", "c"].iter().enumerate() {
        fx.add_gitlink("app", &format!("third_party/waybill-fixture-{n}"), &format!("{}", i + 1).repeat(40));
    }
    let docs = scan(&fx, &app);
    let got: Vec<(String, String)> = submodules(&docs.cdx).into_iter().map(|(p, _, b)| (p, b)).collect();
    assert_eq!(
        got,
        vec![
            (format!("pkg:github/waybill-org/waybill-fixture-a@{}", "1".repeat(40)), "declared-and-used".into()),
            (format!("pkg:github/waybill-org/waybill-fixture-b@{}", "2".repeat(40)), "declared-and-used".into()),
            (format!("pkg:github/waybill-org/waybill-fixture-c@{}", "3".repeat(40)), "declared-only".into()),
        ]
    );
    let _ = coverage;
}
