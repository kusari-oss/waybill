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
}

/// C222 from an SPDX 2.3 or SPDX 3 document's annotations.
fn spdx_coverage(doc: &Value) -> Option<Value> {
    let anns: Vec<&Value> = doc["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(doc["@graph"].as_array().into_iter().flatten().filter(|e| e["type"] == "Annotation"))
        .collect();
    anns.iter().find_map(|a| {
        let env = a["comment"].as_str().or_else(|| a["statement"].as_str())?;
        let env: Value = serde_json::from_str(env).ok()?;
        if env["field"] != "waybill:git-submodule-coverage" {
            return None;
        }
        serde_json::from_str(env["value"].as_str()?).ok()
    })
}

fn skipped(cov: &Value, reason: &str) -> u64 {
    cov["skipped"][reason].as_u64().unwrap_or(u64::MAX)
}

/// SC-002 and FR-001 (#1515's monorepo/vendored shape): a top-level
/// `.gitmodules` declaring three paths, two with gitlinks, and a vendored
/// project's own `.gitmodules` repeating one and adding one with nothing
/// behind it.
#[test]
fn sc002_declarations_without_gitlinks_are_skipped() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write(
        "app/.gitmodules",
        "[submodule \"x\"]\n\tpath = projects/a/third_party/x\n\turl = https://github.com/waybill-org/waybill-fixture-x.git\n\
         [submodule \"y\"]\n\tpath = projects/a/third_party/y\n\turl = https://github.com/waybill-org/waybill-fixture-y.git\n\
         [submodule \"z\"]\n\tpath = projects/a/third_party/z\n\turl = https://github.com/waybill-org/waybill-fixture-z.git\n",
    );
    fx.write(
        "app/projects/a/.gitmodules",
        "[submodule \"x\"]\n\tpath = third_party/x\n\turl = https://github.com/waybill-org/waybill-fixture-x.git\n\
         [submodule \"w\"]\n\tpath = third_party/w\n\turl = https://github.com/waybill-org/waybill-fixture-w.git\n",
    );
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules", "projects/a/.gitmodules"]);
    fx.add_gitlink("app", "projects/a/third_party/x", &"a".repeat(40));
    fx.add_gitlink("app", "projects/a/third_party/y", &"b".repeat(40));
    let docs = scan(&fx, &app);
    let purls: Vec<String> = submodules(&docs.cdx).into_iter().map(|(p, _, _)| p).collect();
    assert_eq!(
        purls,
        vec![
            format!("pkg:github/waybill-org/waybill-fixture-x@{}", "a".repeat(40)),
            format!("pkg:github/waybill-org/waybill-fixture-y@{}", "b".repeat(40)),
        ]
    );
    let cov = coverage(&docs.cdx).expect("C222 present");
    assert_eq!(cov["pinned"], 2);
    assert_eq!(skipped(&cov, "no-gitlink"), 2, "z (top level) and w (vendored); {cov}");
    assert_eq!(skipped(&cov, "no-repository-data"), 0, "{cov}");
}

/// FR-006, FR-010 and the checkout-differs edge case: an initialised
/// submodule's own submodule is emitted at the commit the inner repository
/// pins, depended on by the outer component; after `deinit`, it is gone.
#[test]
fn nested_submodules_follow_initialised_only() {
    let fx = GitFixture::new();
    let inner_commit = fx.repo("inner");
    let first_outer = fx.repo("outer");
    fx.add_submodule("outer", "inner", "third_party/inner");
    let outer_commit = fx.git(&fx.path("outer"), &["rev-parse", "HEAD"]);
    assert_ne!(first_outer, outer_commit);
    fx.repo("app");
    fx.add_submodule("app", "outer", "third_party/outer");
    let app = fx.path("app");
    fx.git(&app, &["submodule", "-q", "update", "--init", "--recursive"]);

    let docs = scan(&fx, &app);
    let subs = submodules(&docs.cdx);
    let outer_purl = subs.iter().find(|(p, _, _)| p.contains("/outer@")).map(|s| s.0.clone()).expect("outer");
    let inner_purl = subs.iter().find(|(p, _, _)| p.contains("/inner@")).map(|s| s.0.clone()).expect("inner");
    assert!(outer_purl.contains(&format!("@{outer_commit}")), "{outer_purl}");
    assert!(inner_purl.contains(&format!("@{inner_commit}")), "{inner_purl}");
    let outer_deps: Vec<String> = docs.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["ref"] == outer_purl.as_str())
        .and_then(|d| d["dependsOn"].as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    assert!(outer_deps.contains(&inner_purl), "outer depends on inner; got {outer_deps:?}");
    assert!(!root_depends_on(&docs.cdx).contains(&inner_purl), "the root does not depend on the nested submodule directly");

    // The checkout moved without a commit: the pinned commit is kept, with a WARN.
    fx.git(&app.join("third_party/outer"), &["checkout", "-q", &first_outer]);
    let moved = scan(&fx, &app);
    assert!(submodules(&moved.cdx).iter().any(|(p, _, _)| p == &outer_purl), "the pinned commit is still emitted");
    assert!(moved.stderr.contains("checkout differs"), "a WARN reports the moved checkout");

    // After deinit, the nested submodule is not enumerated.
    fx.git(&app.join("third_party/outer"), &["checkout", "-q", &outer_commit]);
    fx.git(&app, &["submodule", "-q", "deinit", "-f", "third_party/outer"]);
    let after = scan(&fx, &app);
    let after_subs: Vec<String> = submodules(&after.cdx).into_iter().map(|(p, _, _)| p).collect();
    assert_eq!(after_subs, vec![outer_purl], "only the outer gitlink remains");
}

/// A gitlink with no `.gitmodules` entry is emitted without a URL and
/// counted under `no-url` (and `pinned`).
#[test]
fn gitlink_without_declaration() {
    let fx = GitFixture::new();
    fx.repo("app");
    fx.write("app/.gitmodules", "[submodule \"x\"]\n\tpath = third_party/x\n\turl = https://github.com/waybill-org/waybill-fixture-x.git\n");
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules"]);
    fx.add_gitlink("app", "third_party/x", &"c".repeat(40));
    fx.add_gitlink("app", "vendor/waybill-fixture-orphan", &"d".repeat(40));
    let docs = scan(&fx, &app);
    let purls: Vec<String> = submodules(&docs.cdx).into_iter().map(|(p, _, _)| p).collect();
    assert!(purls.contains(&format!("pkg:generic/waybill-fixture-orphan@{}", "d".repeat(40))), "{purls:?}");
    let cov = coverage(&docs.cdx).expect("C222 present");
    assert_eq!(cov["pinned"], 2, "{cov}");
    assert_eq!(skipped(&cov, "no-url"), 1, "{cov}");
}

/// SC-009: with no repository data, nothing is pinned and C222 says why,
/// identically in all three formats; with it, every entry is pinned.
#[test]
fn sc009_no_repository_data() {
    let fx = GitFixture::new();
    app_with_github_submodule(&fx);
    let bare = fx.fresh_clone("app", "bare");
    std::fs::remove_dir_all(bare.join(".git")).unwrap();
    let docs = scan(&fx, &bare);
    assert!(submodules(&docs.cdx).is_empty());
    let cdx = coverage(&docs.cdx).expect("C222 in CycloneDX");
    assert_eq!(cdx["pinned"], 0);
    assert_eq!(skipped(&cdx, "no-repository-data"), 1, "{cdx}");
    assert_eq!(spdx_coverage(&docs.spdx23).as_ref(), Some(&cdx), "SPDX 2.3 agrees");
    assert_eq!(spdx_coverage(&docs.spdx3).as_ref(), Some(&cdx), "SPDX 3 agrees");

    let with_git = fx.fresh_clone("app", "with-git");
    let cov = coverage(&scan(&fx, &with_git).cdx).expect("C222");
    assert_eq!(cov["pinned"], 1);
    assert_eq!(skipped(&cov, "no-repository-data"), 0);
}

/// `git-refused` (safe.directory) and `git-unavailable` (no git on PATH):
/// nothing pinned, the reason recorded, the scan still succeeds.
#[test]
fn git_refused_and_git_unavailable() {
    let fx = GitFixture::new();
    app_with_github_submodule(&fx);
    let app = fx.path("app");

    let probe = Command::new("git")
        .current_dir(&app)
        .env("HOME", fx.home())
        .env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .unwrap();
    if probe.status.code() == Some(128) {
        let docs = scan_env(&fx, &app, &[("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1")]);
        assert!(submodules(&docs.cdx).is_empty());
        let cov = coverage(&docs.cdx).expect("C222");
        assert_eq!(skipped(&cov, "git-refused"), 1, "{cov}");
    } else {
        eprintln!("skip git-refused half: this git does not honour GIT_TEST_ASSUME_DIFFERENT_OWNER (exit {:?})", probe.status.code());
    }

    let empty_path = fx.path("empty-path");
    std::fs::create_dir_all(&empty_path).unwrap();
    let docs = scan_env(&fx, &app, &[("PATH", empty_path.to_str().unwrap())]);
    assert!(submodules(&docs.cdx).is_empty());
    let cov = coverage(&docs.cdx).expect("C222");
    assert_eq!(skipped(&cov, "git-unavailable"), 1, "{cov}");
}

/// SC-006 / FR-014: C55, C57 and C222 agree across the three formats, read
/// with the parity suite's own extractors, on a nested tree and on the
/// SC-002 monorepo tree. Control: each extractor finds something.
#[test]
fn sc006_cross_format_parity() {
    let fx = GitFixture::new();
    fx.repo("inner");
    fx.repo("outer");
    fx.add_submodule("outer", "inner", "third_party/inner");
    fx.repo("app");
    fx.add_submodule("app", "outer", "third_party/outer");
    let app = fx.path("app");
    fx.git(&app, &["submodule", "-q", "update", "--init", "--recursive"]);

    fx.repo("mono");
    fx.write(
        "mono/.gitmodules",
        "[submodule \"x\"]\n\tpath = projects/a/third_party/x\n\turl = https://github.com/waybill-org/waybill-fixture-x.git\n\
         [submodule \"z\"]\n\tpath = projects/a/third_party/z\n\turl = https://github.com/waybill-org/waybill-fixture-z.git\n",
    );
    let mono = fx.path("mono");
    fx.git(&mono, &["add", ".gitmodules"]);
    fx.add_gitlink("mono", "projects/a/third_party/x", &"e".repeat(40));

    for tree in [app, mono] {
        let docs = scan(&fx, &tree);
        for row in ["C55", "C57", "C222"] {
            let e = waybill::parity::extractors::EXTRACTORS
                .iter()
                .find(|e| e.row_id == row)
                .unwrap_or_else(|| panic!("{row} registered"));
            let (a, b, c) = ((e.cdx)(&docs.cdx), (e.spdx23)(&docs.spdx23), (e.spdx3)(&docs.spdx3));
            assert!(!a.is_empty(), "{row} on {}: CycloneDX extractor found nothing", tree.display());
            assert_eq!(a, b, "{row} on {}: CycloneDX and SPDX 2.3 disagree", tree.display());
            assert_eq!(a, c, "{row} on {}: CycloneDX and SPDX 3 disagree", tree.display());
        }
    }
}

/// SC-011 / FR-009 / FR-015: a submodule of `ROCm/waybill-fixture-Mixed`
/// and a `FetchContent_Declare` of the same repository at the same commit
/// merge into one lowercase PURL, which keeps the submodule's C57 and stays
/// a root dependency even when the root has another edge (FR-010: the
/// merged component is recognised by C57, since C56 is not emitted,
/// #1558). Then ten scans are byte-identical (SC-005).
#[test]
fn sc011_submodule_and_fetchcontent_same_commit_merge() {
    let fx = GitFixture::new();
    fx.repo("app");
    let c = "5".repeat(40);
    fx.write(
        "app/.gitmodules",
        "[submodule \"m\"]\n\tpath = third_party/waybill-fixture-Mixed\n\turl = https://github.com/ROCm/waybill-fixture-Mixed.git\n",
    );
    fx.write(
        "app/CMakeLists.txt",
        &format!(
            "cmake_minimum_required(VERSION 3.25)\nproject(p)\ninclude(FetchContent)\n\
             FetchContent_Declare(waybill-fixture-mixed GIT_REPOSITORY https://github.com/ROCm/waybill-fixture-Mixed.git GIT_TAG {c})\n"
        ),
    );
    fx.write(
        "app/pyproject.toml",
        "[project]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\ndependencies = [\"waybill-fixture-req==2.0.0\"]\n",
    );
    let app = fx.path("app");
    fx.git(&app, &["add", ".gitmodules", "CMakeLists.txt", "pyproject.toml"]);
    fx.add_gitlink("app", "third_party/waybill-fixture-Mixed", &c);
    let docs = scan(&fx, &app);
    let want = format!("pkg:github/rocm/waybill-fixture-mixed@{c}");
    let matching: Vec<&Value> = docs.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["purl"].as_str().is_some_and(|p| p.contains("waybill-fixture-mixed")))
        .collect();
    assert_eq!(matching.len(), 1, "one merged component: {matching:?}");
    assert_eq!(matching[0]["purl"], want.as_str());
    assert_eq!(prop(matching[0], "waybill:source-mechanism"), Some("cmake-fetchcontent-git"));
    assert!(prop(matching[0], "waybill:build-reference").is_some(), "the submodule's C57 survives the merge");
    let root = root_depends_on(&docs.cdx);
    assert!(root.iter().any(|d| d.contains("waybill-fixture-req")), "control: the root has another edge");
    assert!(root.contains(&want), "the merged submodule stays a root dependency: {root:?}");
    for row in ["C55", "C57"] {
        let e = waybill::parity::extractors::EXTRACTORS.iter().find(|e| e.row_id == row).unwrap();
        let (a, b, cc) = ((e.cdx)(&docs.cdx), (e.spdx23)(&docs.spdx23), (e.spdx3)(&docs.spdx3));
        assert!(!a.is_empty(), "{row}: CycloneDX extractor found nothing");
        assert_eq!(a, b, "{row}: CycloneDX and SPDX 2.3 disagree");
        assert_eq!(a, cc, "{row}: CycloneDX and SPDX 3 disagree");
    }
    // `serialNumber` is a fresh UUID per document by CycloneDX's rule; mask
    // it, as `normalize_cdx_for_golden` does.
    let masked = |mut v: Value| {
        v["serialNumber"] = Value::Null;
        v
    };
    let first = masked(docs.cdx.clone());
    for i in 1..10 {
        let again = masked(scan(&fx, &app).cdx);
        if again != first {
            let diff: Vec<String> = diff_paths(&first, &again, "");
            panic!("run {i} differs at {diff:?}");
        }
    }
}

/// JSON paths at which two documents differ (test diagnostics).
fn diff_paths(a: &Value, b: &Value, at: &str) -> Vec<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter()
                .flat_map(|k| diff_paths(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &format!("{at}/{k}")))
                .collect()
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().flat_map(|(i, (p, q))| diff_paths(p, q, &format!("{at}/{i}"))).collect()
        }
        _ if a == b => vec![],
        _ => vec![at.to_string()],
    }
}
