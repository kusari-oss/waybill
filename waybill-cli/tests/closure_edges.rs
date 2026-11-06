//! Milestone 1280 (#1346) — the closure tier emits its dependency graph.
//!
//! The tier contributed components and **no dependency edges**, so every
//! closure component was an orphan by construction — measured, 352 of 406 on
//! the pinned golden. nix records the graph in `inputs.drvs`; waybill parsed
//! the *declared* attributes instead and used them only for the C182 role.
//!
//! **Every assertion here is on the emitted document**, never on the
//! projection's internals: the `.drv`-keyed projection is unit-tested in
//! `closure/graph.rs`, and what this suite exists to catch is the join between
//! that graph and the PURLs the document finally carries. A projection test
//! stays green while every edge names a component that is not there.
//!
//! The fixtures are JSON handed to a stub `nix`. **None is ever evaluated by a
//! real one**: on 2026-11-04 that rewrote a committed `flake.lock` and broke
//! 11 tests in an unrelated file.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision the fixture `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

const ROLE: &str = "waybill:closure-role";

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
        .join("tests/fixtures/nix_closure_edges")
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

    fn prop(&self, name: &str, prop: &str) -> Option<&str> {
        self.components()
            .iter()
            .find(|c| c["name"].as_str() == Some(name))?
            .get("properties")?
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some(prop))?
            .get("value")?
            .as_str()
    }

    /// The emitted role, by component name. `None` when the component is
    /// absent — which this feature changes, so the distinction matters.
    fn role(&self, name: &str) -> Option<&str> {
        self.prop(name, ROLE)
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

    fn spdx23_relationship_types(&self) -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for r in self.spdx23["relationships"].as_array().into_iter().flatten() {
            if let Some(t) = r["relationshipType"].as_str() {
                *m.entry(t.to_string()).or_insert(0) += 1;
            }
        }
        m
    }

    fn spdx3_relationship_count(&self) -> usize {
        self.spdx3["@graph"]
            .as_array()
            .map(|g| {
                g.iter()
                    .filter(|e| e["type"].as_str() == Some("Relationship"))
                    .count()
            })
            .unwrap_or(0)
    }

    /// The PURL of every component, for the endpoint-resolution property.
    fn purls(&self) -> std::collections::HashSet<String> {
        self.components()
            .iter()
            .filter_map(|c| c["purl"].as_str().map(str::to_string))
            .collect()
    }

    fn closure_purls(&self) -> std::collections::HashSet<String> {
        self.components()
            .iter()
            .filter(|c| {
                c["properties"].as_array().is_some_and(|ps| {
                    ps.iter().any(|p| p["name"] == "waybill:closure-role")
                })
            })
            .filter_map(|c| c["purl"].as_str().map(str::to_string))
            .collect()
    }

    fn doc_root_ref(&self) -> String {
        self.cdx["dependencies"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["ref"].as_str())
            .unwrap_or_default()
            .to_string()
    }
}

fn scan_closure(closure_json: &str) -> Scan {
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
        stub_script(&dir.path().join("nix.log"), &closure("through_fetch.json")),
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

// ---------------------------------------------------------------------
// T017 — the control, which is also SC-001
// ---------------------------------------------------------------------

/// Before this milestone the closure contributed **zero** dependency edges, so
/// every assertion below would have passed over an empty set. Prove first that
/// edges exist and that they reach closure components.
#[test]
fn m1280_t017_the_closure_contributes_edges_that_reach_its_components() {
    let s = scan_closure(&closure("through_fetch.json"));
    let closure_purls = s.closure_purls();
    assert!(
        !closure_purls.is_empty(),
        "no closure components were emitted at all; every assertion in this \
         file would be vacuous"
    );
    let targets: std::collections::HashSet<String> =
        s.cdx_edges().into_iter().map(|(_, to)| to).collect();
    let reached: Vec<&String> = closure_purls.intersection(&targets).collect();
    assert!(
        !reached.is_empty(),
        "closure components exist but nothing points at any of them, which is \
         exactly the pre-1280 state. components={closure_purls:?}"
    );
}

// ---------------------------------------------------------------------
// User Story 1
// ---------------------------------------------------------------------

/// T018 / contract P1 — the pass-through rule, which is the feature.
///
/// `pkgb` is referenced by no declared attribute and is emitted only because
/// it applies a patch, so an edge to it can only come from walking *through*
/// the fetch derivation between it and `pkga`.
#[test]
fn m1280_t018_a_member_behind_a_fetch_is_reachable() {
    let s = scan_closure(&closure("through_fetch.json"));
    let edges = s.cdx_edges();
    let pkga = "pkg:generic/waybill-fixture-pkga@1.0";
    let pkgb = "pkg:generic/waybill-fixture-pkgb@1.0";
    assert!(
        s.purls().contains(pkgb),
        "pkgb must be a component before an edge to it can be asserted; \
         got {:?}",
        s.closure_purls()
    );
    assert!(
        edges.iter().any(|(f, t)| f == pkga && t == pkgb),
        "pkga reaches pkgb only through a derivation with no pname. \
         edges touching either: {:?}",
        edges
            .iter()
            .filter(|(f, t)| f.contains("pkg") && t.contains("pkg"))
            .collect::<Vec<_>>()
    );
    assert!(
        !edges.iter().any(|(_, t)| t.contains("tar.gz")),
        "no edge may point at a derivation that is not a component"
    );
}

/// T026 — the property, not the placement.
///
/// **This is the assertion that survived research R1's retraction.** R1
/// claimed edges built before the milestone-1275 identity pass would dangle
/// for 27% of the graph; they would not, because `apply_renames` moves
/// endpoints alongside identities (#980/#981). A test pinning the call site
/// would have encoded that wrong reason and still passed. This one holds
/// whatever the sequencing and whatever the rename convention.
#[test]
fn m1280_t026_every_edge_endpoint_resolves_to_a_component() {
    let s = scan_closure(&closure("through_fetch.json"));
    let mut known = s.purls();
    // The document root is a legitimate endpoint and is not a component.
    known.insert(s.doc_root_ref());
    let cl = s.closure_purls();
    let edges = s.cdx_edges();

    // Scoped to the edges THIS milestone contributes — those with a closure
    // component at either end.
    //
    // **Not scoped to dodge a failure in the feature.** Asserted over every
    // edge, this fixture reports 5 of 6 dangling, all of them the manifest
    // reader's: the fixture project's cabal file declares `base` and friends,
    // which that reader emits as versionless `pkg:hackage/base` edge targets
    // without emitting components for them. That is a property of the fixture
    // project, and the control is the real closure — a scan of the
    // `nix-closure-moat` corpus target measures **0 dangling of 3,907 edges**,
    // so the closure's own contribution resolves completely.
    let mine: Vec<&(String, String)> = edges
        .iter()
        .filter(|(f, t)| cl.contains(f.as_str()) || cl.contains(t.as_str()))
        .collect();
    assert!(
        !mine.is_empty(),
        "no closure edges to check, so this asserts nothing"
    );
    let dangling: Vec<&&(String, String)> = mine
        .iter()
        .filter(|(f, t)| !known.contains(f) || !known.contains(t))
        .collect();
    assert!(
        dangling.is_empty(),
        "every closure-edge endpoint must name something the document \
         carries; {} dangling of {}: {:?}",
        dangling.len(),
        mine.len(),
        &dangling[..dangling.len().min(5)]
    );
}

/// T020 / FR-003 — build variants of one identity emit one edge, not one per
/// derivation.
#[test]
fn m1280_t020_variant_merged_components_emit_one_edge() {
    let s = scan_closure(&closure("variants.json"));
    let variant = "pkg:generic/waybill-fixture-variant@1.0";
    let pkgc = "pkg:generic/waybill-fixture-pkgc@4.0";
    let matching: Vec<_> = s
        .cdx_edges()
        .into_iter()
        .filter(|(f, t)| f == variant && t == pkgc)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "two derivations share this identity and both name pkgc; the merge \
         must deduplicate. got {matching:?}"
    );
}

/// T021 / FR-007a — the role must not select a relationship type.
#[test]
fn m1280_t021_the_role_does_not_select_a_relationship_type() {
    let s = scan_closure(&closure("anchor.json"));
    // `anchora` is artifact-input and `anchorb` build-tooling, by the
    // fixture's declared attributes. Both must be reached the same way.
    let types = s.spdx23_relationship_types();
    assert!(
        types.contains_key("DEPENDS_ON"),
        "the new edges must use the plain dependency type; got {types:?}"
    );
    // CONTROL: the two roles really are different, or this asserts nothing.
    let roles: std::collections::BTreeSet<Option<&str>> =
        ["waybill-fixture-anchora", "waybill-fixture-anchorb"]
            .iter()
            .map(|n| s.role(n))
            .collect();
    assert_eq!(
        roles.len(),
        2,
        "the fixture must give its two members different roles; got {roles:?}"
    );
}

/// T022 / FR-007 — the edges appear in all three formats, natively, and in no
/// `waybill:` annotation.
#[test]
fn m1280_t022_the_edges_are_native_in_all_three_formats() {
    let s = scan_closure(&closure("through_fetch.json"));
    assert!(!s.cdx_edges().is_empty(), "CycloneDX carries none");
    let t23 = s.spdx23_relationship_types();
    assert!(
        t23.get("DEPENDS_ON").copied().unwrap_or(0) > 0,
        "SPDX 2.3 carries none; got {t23:?}"
    );
    assert!(s.spdx3_relationship_count() > 0, "SPDX 3 carries none");
    // No annotation may carry a dependency edge: all three formats model one
    // natively, which is why this milestone adds no catalogue row.
    let raw = serde_json::to_string(&s.cdx).unwrap();
    assert!(
        !raw.contains("waybill:closure-edge") && !raw.contains("waybill:closure-depends"),
        "a dependency edge must not be carried as a waybill: annotation"
    );
}

/// T023 / FR-009 — a scan without `--nix-closure` is unchanged.
#[test]
fn m1280_t023_no_closure_flag_means_no_closure_edges() {
    // The stub would serve a closure, but the flag is absent, so the tier
    // never runs and nothing it contributes can appear.
    let s = scan_without_closure();
    assert!(
        s.closure_purls().is_empty(),
        "a scan without --nix-closure must carry no closure components"
    );
    let edges = s.cdx_edges();
    assert!(
        !edges.iter().any(|(_, t)| t.contains("waybill-fixture-pkgb")),
        "nor any closure edge; got {edges:?}"
    );
}
