//! Milestone 1282 (#1282) — the closure members waybill does not name.
//!
//! 1,145 of 1,749 derivations on the pinned closure declare no `pname`, and
//! the tier emitted **nothing** for any of them: 65% of the closure invisible.
//! This suite drives the real binary against a stub `nix` and asserts on the
//! **emitted documents**, never on the classifier's internals — `member.rs`
//! unit-tests the classification, and what this exists to catch is the join
//! between a role and the component a consumer finally reads.
//!
//! **The gate that hid them was never a `pname` test.** `pname()` falls back
//! to the derivation's own `name`, so it returns a value for all 1,145; the
//! C182 role filter dropped them. A test written against the wrong mechanism
//! would pass while the feature emitted nothing.
//!
//! The fixtures are JSON handed to a stub `nix`. **None is ever evaluated by
//! a real one**: on 2026-11-04 that rewrote a committed `flake.lock` and
//! broke 11 tests in an unrelated file.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision the fixture `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// The m1277 fixture project, reused as m1278's suite reuses it: it already
/// pins the nixpkgs revision the cache is seeded for, and a second
/// near-identical flake would be one more thing to keep in step for no new
/// coverage.
fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/closure_role_scope/project")
}

/// A closure fixture, read from disk.
///
/// **Never evaluated by a real `nix`.** On 2026-11-04 evaluating a fixture
/// flake rewrote its committed `flake.lock` and broke 11 tests in an
/// unrelated file; these are JSON documents handed to a stub, and the stub is
/// the only thing on `PATH` named `nix` while a scan runs.
fn closure(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_members")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).unwrap();
    // Empty is enough: this milestone asserts nothing about Hackage identity,
    // and an absent cache would make that pass degrade for an unrelated
    // reason — which would then look like a finding about field coverage.
    std::fs::write(rev.join("hackage-packages.nix"), "{ }\n").unwrap();
    for s in ["9.4.x", "9.6.x", "9.8.x", "9.10.x", "9.12.x"] {
        std::fs::write(rev.join(format!("configuration-ghc-{s}.nix")), "{ }\n").unwrap();
    }
    dir
}

fn stub_script(log: &Path, closure: &str) -> String {
    format!(
        r##"#!/bin/sh
echo "$*" >> '{log}'
case "$*" in
  *builtins.currentSystem*) printf '%s' 'aarch64-darwin' ;;
  "config show"*) echo 'allow-dirty = true'; echo 'allow-import-from-derivation = false' ;;
  "derivation show"*"#packages.aarch64-darwin.default") cat <<'CLOSUREEOF'
{closure}
CLOSUREEOF
  ;;
  *"#packages.aarch64-darwin") echo '["default"]' ;;
  *"#packages") echo '["aarch64-darwin"]' ;;
  "derivation show"*) echo "error: flake 'stub' does not provide attribute" >&2; exit 1 ;;
  *) echo 'stub nix: unhandled invocation' >&2; exit 1 ;;
esac
"##,
        log = log.display(),
        closure = closure,
    )
}

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    _dir: tempfile::TempDir,
    _cache: tempfile::TempDir,
}

impl Scan {
    fn components(&self) -> &Vec<Value> {
        self.cdx["components"].as_array().unwrap()
    }
}

impl Scan {
    /// CycloneDX `dependencies[]`, flattened to `(ref, dependsOn)` pairs.
    fn cdx_edges(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for e in self.cdx["dependencies"].as_array().into_iter().flatten() {
            let Some(from) = e["ref"].as_str() else { continue };
            for to in e["dependsOn"].as_array().into_iter().flatten() {
                if let Some(t) = to.as_str() {
                    out.push((from.to_string(), t.to_string()));
                }
            }
        }
        out
    }
    /// The PURL of every component, for the endpoint-resolution property.
    fn purls(&self) -> std::collections::HashSet<String> {
        self.components()
            .iter()
            .filter_map(|c| c["purl"].as_str().map(str::to_string))
            .collect()
    }
    /// Every ref that legitimately names the subject rather than a component.
    ///
    /// **Both the PURL and the bom-ref**, because the two differ: CycloneDX's
    /// `dependencies[].ref` for the subject is the bom-ref (`name@version`)
    /// while `metadata.component.purl` is a PURL. An earlier version of this
    /// helper took `dependencies[0].ref` instead, which is whichever entry
    /// sorts first and is usually a component — it reported two dangling
    /// edges that a direct scan of the same fixture does not have.
    fn doc_root_refs(&self) -> std::collections::HashSet<String> {
        let c = &self.cdx["metadata"]["component"];
        ["purl", "bom-ref"]
            .iter()
            .filter_map(|k| c[*k].as_str().map(str::to_string))
            .collect()
    }
}

fn scan_closure(closure_json: &str) -> Scan {
    scan_closure_with_args(closure_json, &[])
}

/// The same, with extra CLI arguments — for the `--tier` interaction (T058).
fn scan_closure_with_args(closure_json: &str, extra: &[&str]) -> Scan {
    let dir = tempfile::tempdir().unwrap();
    let cache = seed_cache();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let nix = bin.join("nix");
    std::fs::write(&nix, stub_script(&dir.path().join("nix.log"), closure_json)).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let cdx_path = dir.path().join("out.cdx.json");
    let spdx23_path = dir.path().join("out.spdx23.json");
    let spdx3_path = dir.path().join("out.spdx3.json");
    let out = Command::new(binary_path())
        .args([
            "sbom",
            "scan",
            "--path",
            project().to_str().unwrap(),
            "--no-deep-hash",
            "--nix-closure",
            "--format",
            "cyclonedx-json",
            "--format",
            "spdx-2.3-json",
            "--format",
            "spdx-3-json",
            "--output",
        ])
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.display()))
        .args(extra)
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
        .env("WAYBILL_NIXPKGS_CACHE", cache.path())
        .output()
        .unwrap();
    assert!(
        cdx_path.exists(),
        "scan produced no document.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    Scan {
        cdx: read(&cdx_path),
        spdx23: read(&spdx23_path),
        spdx3: read(&spdx3_path),
        _dir: dir,
        _cache: cache,
    }
}

/// A scan of the same project with **no** `--nix-closure`, for FR-009.
///
/// The stub `nix` is still on `PATH` and would serve a closure if asked. Not
/// asking is the point: the tier must not run, so nothing it contributes can
/// reach the document.
fn scan_without_closure() -> Scan {
    let dir = tempfile::tempdir().unwrap();
    let cache = seed_cache();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let nix = bin.join("nix");
    std::fs::write(
        &nix,
        stub_script(&dir.path().join("nix.log"), &closure("closure.json")),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let cdx_path = dir.path().join("out.cdx.json");
    let spdx23_path = dir.path().join("out.spdx23.json");
    let spdx3_path = dir.path().join("out.spdx3.json");
    let out = Command::new(binary_path())
        .args([
            "sbom",
            "scan",
            "--path",
            project().to_str().unwrap(),
            "--no-deep-hash",
            "--format",
            "cyclonedx-json",
            "--format",
            "spdx-2.3-json",
            "--format",
            "spdx-3-json",
            "--output",
        ])
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.display()))
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
        .env("WAYBILL_NIXPKGS_CACHE", cache.path())
        .output()
        .unwrap();
    assert!(
        cdx_path.exists(),
        "scan produced no document.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    Scan {
        cdx: read(&cdx_path),
        spdx23: read(&spdx23_path),
        spdx3: read(&spdx3_path),
        _dir: dir,
        _cache: cache,
    }
}

/// The member-role annotation (C206).
const MEMBER_ROLE: &str = "waybill:closure-member-role";

impl Scan {
    /// Components carrying a member role — exactly the population m1282 adds.
    fn added(&self) -> Vec<&Value> {
        self.components()
            .iter()
            .filter(|c| {
                c["properties"].as_array().is_some_and(|ps| {
                    ps.iter().any(|p| p["name"] == MEMBER_ROLE)
                })
            })
            .collect()
    }

    fn member_role_of(&self, c: &Value) -> Option<String> {
        c["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == MEMBER_ROLE)?["value"]
            .as_str()
            .map(str::to_string)
    }

    fn role_counts(&self) -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for c in self.added() {
            if let Some(r) = self.member_role_of(c) {
                *m.entry(r).or_insert(0) += 1;
            }
        }
        m
    }

    /// The C184 document-scope record, parsed.
    fn closure_record(&self) -> Value {
        let raw = self.cdx["metadata"]["properties"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "waybill:nix-closure")
            .expect("the closure record must be present")["value"]
            .as_str()
            .unwrap();
        serde_json::from_str(raw).unwrap()
    }

    fn orphan_reason_of(&self, c: &Value) -> Option<String> {
        c["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == "waybill:orphan-reason")?["value"]
            .as_str()
            .map(str::to_string)
    }
}

fn scan() -> Scan {
    scan_closure(&closure("closure.json"))
}

// ---------------------------------------------------------------------
// CONTROL — without this every assertion below can hold vacuously
// ---------------------------------------------------------------------

#[test]
fn m1282_control_the_pname_less_members_are_emitted_at_all() {
    // Before this milestone the answer was zero, and zero makes every
    // "each added component carries X" assertion below true for the wrong
    // reason. The fixture holds 9 pname-less members merging to 8 components
    // (two share a name and a content hash).
    let s = scan();
    let added = s.added();
    assert_eq!(
        added.len(),
        8,
        "expected 8 added components, got {}: {:?}",
        added.len(),
        added.iter().map(|c| &c["name"]).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------
// User Story 1 — a consumer can see the sources a build consumed
// ---------------------------------------------------------------------

#[test]
fn m1282_us1_a_fixed_output_member_carries_its_content_hash_in_its_identity() {
    // US1 scenario 1, FR-003.
    let s = scan();
    let srcs: Vec<_> = s
        .added()
        .into_iter()
        .filter(|c| s.member_role_of(c).as_deref() == Some("source"))
        .collect();
    assert!(!srcs.is_empty(), "no source components emitted");
    for c in srcs {
        let purl = c["purl"].as_str().unwrap();
        assert!(
            purl.contains(".e") || purl.len() > "pkg:generic/source".len() + 8,
            "source identity carries no hash: {purl}"
        );
    }
}

#[test]
fn m1282_us1_identical_content_yields_one_component() {
    // US1 scenario 2, FR-003, SC-010. The two `waybill-fixture-tarball`
    // members record the SAME hash under the two different spellings —
    // `outputs[].hash` and the older `env.outputHash`. Reading only the
    // latter undercounts the population fourfold, so this also guards T008.
    let s = scan();
    let tarballs: Vec<_> = s
        .added()
        .into_iter()
        .filter(|c| c["name"].as_str() == Some("waybill-fixture-tarball"))
        .collect();
    assert_eq!(
        tarballs.len(),
        1,
        "the same content fetched twice is ONE artifact; got {:?}",
        tarballs.iter().map(|c| &c["purl"]).collect::<Vec<_>>()
    );
}

#[test]
fn m1282_us1_same_name_different_content_yields_two_distinct_components() {
    // US1 scenario 3, FR-002, SC-014 — and the single most important
    // assertion in this file. On the real closure 56 members are named
    // `source`; the merge key was `(name, version)`, these declare no
    // version, so without the re-key all 56 collapse into one component.
    let s = scan();
    let sources: Vec<_> = s
        .added()
        .into_iter()
        .filter(|c| c["name"].as_str() == Some("source"))
        .collect();
    assert_eq!(
        sources.len(),
        2,
        "two different contents must be two components; got {:?}",
        sources.iter().map(|c| &c["purl"]).collect::<Vec<_>>()
    );

    // **Without reference to any qualifier.** A hash carried only as a
    // qualifier would let tooling that compares without them see one
    // component where the closure holds many — the collision FR-002 exists
    // to prevent, re-created by the identifier meant to prevent it.
    let bare: std::collections::HashSet<&str> = sources
        .iter()
        .map(|c| {
            let p = c["purl"].as_str().unwrap();
            p.split('?').next().unwrap()
        })
        .collect();
    assert_eq!(
        bare.len(),
        2,
        "identities must differ before the '?': {bare:?}"
    );
}

#[test]
fn m1282_us1_the_name_field_is_never_rewritten() {
    // FR-016 — the identifier carries uniqueness, the name carries
    // legibility. A reader must not have to read a digest to learn what
    // something is.
    let s = scan();
    for c in s.added() {
        let name = c["name"].as_str().unwrap();
        assert!(
            !name.contains(".e") && !name.contains("sha256"),
            "the hash leaked into the name field: {name}"
        );
    }
    let names: std::collections::HashSet<&str> =
        s.added().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(names.contains("source"), "got {names:?}");
    assert!(names.contains("waybill-fixture-strncpy"), "got {names:?}");
}

#[test]
fn m1282_us1_a_source_url_is_carried_natively() {
    // FR-008 — as a CycloneDX `externalReference` of type `distribution`,
    // not a `waybill:` property: Principle V, standards-native fields first.
    let s = scan();
    let src = s
        .added()
        .into_iter()
        .find(|c| c["name"].as_str() == Some("source")
            && c["externalReferences"].is_array())
        .expect("a source component with external references");
    let urls: Vec<&str> = src["externalReferences"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["type"].as_str() == Some("distribution"))
        .filter_map(|r| r["url"].as_str())
        .collect();
    assert!(
        urls.iter().any(|u| u.contains("waybill-fixture-a.tar.gz")),
        "no distribution URL carried; got {urls:?}"
    );
}

#[test]
fn m1282_t049_two_scans_of_one_closure_emit_identical_component_sets() {
    // SC-004. Asserted on the sorted identifier list: a set comparison would
    // hide an ordering change that makes goldens churn.
    let sorted = |s: &Scan| {
        let mut v: Vec<String> = s
            .components()
            .iter()
            .filter_map(|c| c["purl"].as_str().map(str::to_string))
            .collect();
        v.sort();
        v
    };
    assert_eq!(sorted(&scan()), sorted(&scan()));
}

// ---------------------------------------------------------------------
// User Story 2 — a consumer can tell a source from a build script
// ---------------------------------------------------------------------

#[test]
fn m1282_us2_a_write_text_member_is_marked_scaffolding() {
    // US2 scenario 1.
    let s = scan();
    let b = s
        .added()
        .into_iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-builder"))
        .expect("the builder component");
    assert_eq!(s.member_role_of(b).as_deref(), Some("scaffolding"));
}

#[test]
fn m1282_us2_every_added_component_carries_exactly_one_role() {
    // US2 scenario 2, SC-002.
    let s = scan();
    for c in s.added() {
        let n = c["properties"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["name"] == MEMBER_ROLE)
            .count();
        assert_eq!(n, 1, "component {:?} carries {n} roles", c["name"]);
    }

    // And the four counts partition the set, asserted per role rather than
    // on the total: a classifier returning one role for everything still
    // sums correctly.
    let counts = s.role_counts();
    assert_eq!(counts.get("source"), Some(&3), "{counts:?}");
    assert_eq!(counts.get("package"), Some(&1), "{counts:?}");
    assert_eq!(counts.get("scaffolding"), Some(&2), "{counts:?}");
    assert_eq!(counts.get("fragment"), Some(&2), "{counts:?}");
    assert_eq!(counts.values().sum::<usize>(), s.added().len());
}

#[test]
fn m1282_us2_filtering_to_sources_and_packages_leaves_no_scaffolding() {
    // US2 scenario 3, FR-007, SC-005.
    let s = scan();
    let kept: Vec<_> = s
        .added()
        .into_iter()
        .filter(|c| {
            matches!(
                s.member_role_of(c).as_deref(),
                Some("source") | Some("package")
            )
        })
        .collect();
    assert_eq!(kept.len(), 4, "3 sources + 1 package");
    for c in &kept {
        let r = s.member_role_of(c);
        assert!(r.as_deref() != Some("scaffolding") && r.as_deref() != Some("fragment"));
    }

    // **The other half of FR-007, and the one an earlier reading got wrong.**
    // The filter must keep the components carrying NO member role — those are
    // the named packages the closure tier already emitted. A filter expressed
    // as "role is source or package" alone discards every one of them, which
    // is the opposite of what a consumer asking for sources and packages
    // wants.
    // Counted among CLOSURE components specifically. Counting the whole
    // document would be satisfied by the project's own Haskell components
    // and would stay green even if the closure tier emitted nothing but
    // m1282 members — which is exactly what an earlier fixture did.
    let closure_without_role = s
        .components()
        .iter()
        .filter(|c| {
            c["properties"].as_array().is_some_and(|ps| {
                ps.iter().any(|p| p["name"] == "waybill:closure-role")
            })
        })
        .filter(|c| s.member_role_of(c).is_none())
        .count();
    assert!(
        closure_without_role > 0,
        "the closure must emit at least one component predating m1282,          or the two populations cannot be told apart here"
    );
}

#[test]
fn m1282_us2_the_role_is_carried_in_all_three_formats() {
    // The role has no native carrier in any of the three — CDX
    // `component.type` is an artifact kind and `scope` is about the build's
    // need, SPDX `primaryPackagePurpose` has `SOURCE` but nothing for
    // scaffolding or a bootstrap fragment — so it is an annotation
    // everywhere, and this asserts the three agree rather than a
    // native/annotation split.
    let s = scan();
    let cdx = s.role_counts().values().sum::<usize>();
    assert_eq!(cdx, 8);

    let spdx23 = s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            p["annotations"].as_array().is_some_and(|as_| {
                as_.iter()
                    .any(|a| a["comment"].as_str().is_some_and(|c| c.contains(MEMBER_ROLE)))
            })
        })
        .count();
    assert_eq!(spdx23, cdx, "SPDX 2.3 disagrees with CycloneDX");

    let spdx3 = s.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["type"].as_str() == Some("Annotation")
                && e["statement"].as_str().is_some_and(|c| c.contains(MEMBER_ROLE))
        })
        .count();
    assert_eq!(spdx3, cdx, "SPDX 3 disagrees with CycloneDX");
}

// ---------------------------------------------------------------------
// User Story 3 — the document stays usable at triple the count
// ---------------------------------------------------------------------

#[test]
fn m1282_us3_a_source_behind_scaffolding_is_the_endpoint() {
    // FR-013's pass-through, and the edge case that makes the graph change
    // SHAPE rather than merely grow. `app` reaches `source`(B) only through
    // `waybill-fixture-builder`, which is scaffolding and therefore never an
    // endpoint: the walk passes through it and the edge names the source.
    //
    // This is the coverage the milestone-1280 graph test gave up when
    // `fetch.drv` became a stopping point; it is preserved here rather than
    // lost.
    let s = scan();
    let builder = s
        .added()
        .into_iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-builder"))
        .unwrap();
    let builder_purl = builder["purl"].as_str().unwrap().to_string();
    let edges = s.cdx_edges();

    // **CONTROL, and it caught a vacuous pass.** The first version of this
    // fixture declared its dependencies only in `inputs.drvs`; the C182
    // classifier reads *declared attributes*, so every derivation came back
    // `unreferenced`, nothing was emitted to hang an edge on, and the
    // closure contributed **zero** edges. "Scaffolding is never an endpoint"
    // was true of an empty set, and this test passed while proving nothing.
    //
    // It was found by dumping the emitted record, not by reading the test.
    let closure_edges: Vec<_> = edges
        .iter()
        .filter(|(f, t)| f.contains("pkg:generic/") || t.contains("pkg:generic/"))
        .collect();
    assert!(
        !closure_edges.is_empty(),
        "the closure must contribute at least one edge, or this proves nothing"
    );

    assert!(
        !edges.iter().any(|(_, to)| *to == builder_purl),
        "scaffolding must never be an edge endpoint; got {edges:?}"
    );
}

#[test]
fn m1282_us3_no_edge_names_a_component_that_is_absent() {
    // SC-009, SC-011, FR-013 — the property that must hold whatever the
    // graph's shape, and the one a projection-level test cannot establish.
    let s = scan();
    let present = s.purls();
    let roots = s.doc_root_refs();
    let mut dangling = Vec::new();
    for (from, to) in s.cdx_edges() {
        for end in [&from, &to] {
            if !present.contains(end) && !roots.contains(end) {
                dangling.push(end.clone());
            }
        }
    }
    assert!(dangling.is_empty(), "edges name absent components: {dangling:?}");
}

#[test]
fn m1282_t036_every_unreachable_added_component_carries_the_new_reason() {
    // FR-014, SC-012 — and the assertion T054 exists to make possible.
    //
    // Every member this feature adds is `unreferenced`, and
    // `already_explained` treats that C182 value as a sufficient
    // explanation. Without the narrowing these components would be
    // "explained" while carrying NO reason, this test would be the only
    // thing that could tell, and the invariant harness would have agreed
    // with the defect.
    let s = scan();
    let reached: std::collections::HashSet<String> =
        s.cdx_edges().into_iter().map(|(_, to)| to).collect();

    let mut unreachable = 0usize;
    for c in s.added() {
        let purl = c["purl"].as_str().unwrap();
        if reached.contains(purl) {
            continue;
        }
        unreachable += 1;
        let role = s.member_role_of(c);
        let reason = s.orphan_reason_of(c);
        match role.as_deref() {
            // Orphans by construction: FR-013 keeps these out of the graph.
            Some("scaffolding") | Some("fragment") => assert_eq!(
                reason.as_deref(),
                Some("closure-member-not-an-edge-endpoint"),
                "unreachable {:?} carries the wrong reason (or none)",
                c["name"]
            ),
            // A `source` or `package` IS an eligible endpoint, so one that
            // nothing reaches is unreachable for a different reason and must
            // not claim this one. It must still carry *a* reason — an
            // unexplained orphan is the defect FR-014 removes.
            _ => {
                assert!(
                    reason.is_some(),
                    "unreachable {:?} carries no reason at all",
                    c["name"]
                );
                assert_ne!(
                    reason.as_deref(),
                    Some("closure-member-not-an-edge-endpoint"),
                    "{:?} is an eligible endpoint and must not claim otherwise",
                    c["name"]
                );
            }
        }
    }
    // CONTROL: there must BE unreachable added components, or the loop
    // above asserted nothing. 2 scaffolding + 2 fragments at minimum.
    assert!(
        unreachable >= 4,
        "expected the scaffolding and fragments to be orphans; got {unreachable}"
    );
}

#[test]
fn m1282_t056_the_orphan_count_is_a_lower_bound_not_an_equality() {
    // SC-012 as corrected. The spec asserted the orphan count EQUALS
    // scaffolding + fragment. Nothing measures that every `source` and
    // `package` is actually reached, and this fixture is the counter-example:
    // `waybill-fixture-mini-0.27.1` is a `package` — an edge endpoint — that
    // nothing consumes, so it is an orphan too.
    //
    // The excess is reported rather than asserted away.
    let s = scan();
    let reached: std::collections::HashSet<String> =
        s.cdx_edges().into_iter().map(|(_, to)| to).collect();
    let by_construction = s
        .added()
        .into_iter()
        .filter(|c| {
            matches!(
                s.member_role_of(c).as_deref(),
                Some("scaffolding") | Some("fragment")
            )
        })
        .count();
    let unreachable = s
        .added()
        .into_iter()
        .filter(|c| !reached.contains(c["purl"].as_str().unwrap()))
        .count();
    assert!(
        unreachable >= by_construction,
        "orphans ({unreachable}) must be at least the by-construction population ({by_construction})"
    );
    assert!(
        unreachable > by_construction,
        "this fixture holds an unconsumed `package`, so the excess must be \
         non-zero — an equality here would be the claim SC-012 used to make"
    );
}

#[test]
fn m1282_t035_the_c182_census_is_untouched() {
    // SC-013, FR-012. It counts DERIVATIONS, so emitting more components
    // cannot change it — this catches a change that RECLASSIFIES them as a
    // side effect.
    let s = scan();
    let rec = s.closure_record();
    assert_eq!(rec["derivations"], 11, "{rec}");
    let roles = rec["roles"].as_object().unwrap();
    let total: u64 = roles.values().map(|v| v.as_u64().unwrap()).sum();
    assert_eq!(
        total, 11,
        "the census must still total every derivation: {roles:?}"
    );
}

#[test]
fn m1282_t055_the_component_count_reconciles_without_the_census() {
    // FR-009. The reconciliation is `components-emitted` = components
    // carrying no member role + the four role counts. The C182 census is
    // NOT an addend: it counts derivations, and summing it with a component
    // record reconciles nothing.
    let s = scan();
    let rec = s.closure_record();
    let emitted = rec["components-emitted"].as_u64().unwrap();
    let without = rec["components-without-member-role"].as_u64().unwrap();
    let by_role: u64 = rec["member-roles"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_u64().unwrap())
        .sum();
    assert_eq!(
        emitted,
        without + by_role,
        "components-emitted must be the sum of the two component records: {rec}"
    );
    assert!(by_role > 0, "no members were added: {rec}");

    // Every role present even at zero: an absent key would make a closure
    // with no scaffolding indistinguishable from a classifier that stopped
    // producing that role.
    for role in ["source", "package", "scaffolding", "fragment"] {
        assert!(rec["member-roles"].get(role).is_some(), "missing {role}: {rec}");
    }
}

#[test]
fn m1282_t033_the_edge_figure_is_carried() {
    // FR-018 — a graph that silently changes size is indistinguishable from
    // one that broke, so the document carries the figure rather than leaving
    // a reader to infer it.
    let s = scan();
    let rec = s.closure_record();
    assert!(
        rec["edges"].as_u64().is_some(),
        "the edge count must be present: {rec}"
    );
}

#[test]
fn m1282_t038_a_scan_without_the_closure_flag_contributes_nothing() {
    // SC-006, FR-010. The stub `nix` is still on PATH and would serve a
    // closure if asked; not asking is the point.
    let s = scan_without_closure();
    assert!(
        s.added().is_empty(),
        "the tier must not run without --nix-closure; got {:?}",
        s.added().iter().map(|c| &c["name"]).collect::<Vec<_>>()
    );
}
