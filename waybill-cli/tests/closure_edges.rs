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

    // **Milestone 1282 changed the shape of this answer, not the answer.**
    // `fetch.drv` declares no `pname`, and until m1282 that meant it was
    // neither emitted nor a stop, so the only possible edge was the
    // pass-through `pkga -> pkgb`. FR-013 makes `source` and `package`
    // members stopping points, and this one is named
    // `waybill-fixture-pkgb-1.0.tar.gz` — version-shaped, no content hash —
    // so the classifier calls it a `package` and the walk stops there.
    //
    // pkgb is still reachable; it is reached *through* the fetch rather than
    // around it, which is the spec's "the graph changes, and that is
    // accepted". What this test exists to catch — a member behind a
    // pname-less derivation being stranded — is still caught, because a
    // stranded pkgb has no inbound edge at all.
    let fetch: Vec<&String> = edges
        .iter()
        .filter(|(f, _)| f == pkga)
        .map(|(_, t)| t)
        .filter(|t| t.contains("tar.gz"))
        .collect();
    assert_eq!(
        fetch.len(),
        1,
        "pkga must reach the fetch directly now that it is an endpoint. \
         edges touching either: {:?}",
        edges
            .iter()
            .filter(|(f, t)| f.contains("pkg") && t.contains("pkg"))
            .collect::<Vec<_>>()
    );
    assert!(
        edges.iter().any(|(f, t)| f == fetch[0] && t == pkgb),
        "pkgb must still be reachable, through the fetch. got {edges:?}"
    );

    // The invariant that did NOT change: an edge may only name a component
    // the document contains. The fetch is now one, so the test is that it
    // resolves rather than that it is absent.
    let present = s.purls();
    for (_, t) in &edges {
        if t.contains("tar.gz") {
            assert!(present.contains(t), "edge names an absent component: {t}");
        }
    }
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
    // The subject is a legitimate endpoint and is not a component.
    known.extend(s.doc_root_refs());
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

/// T019 / FR-004 — the anchor, and the one case it does not reach.
///
/// On a closure whose root derivation **is** representable as a component the
/// anchor needs no special handling: the root's projected edges resolve
/// through the same identity join as every other node's. Measured on the
/// `nix-closure-moat` corpus target, that is the real case —
/// `unanchored_root_edges = 0`, because the root `moat-0.1` is already
/// `pkg:hackage/moat@0.1` from the Haskell manifest reader.
///
/// This fixture is the other case, and it is deliberately the awkward one: a
/// root that no reader emits. Its projected edges are **dropped and counted**
/// rather than silently lost, and its targets stay orphans. FR-004 was refined
/// to say so once the implementation showed why: anchoring such a root needs
/// `target_ref`, which each emitter derives from `select_root` and main-module
/// detection, so it belongs beside
/// `graph_completeness::anchor_retained_mainmod_edges` and not in the reader.
/// Tracked as a follow-up rather than built for a case never observed.
#[test]
fn m1280_t019_a_root_with_no_component_drops_its_edges_rather_than_faking_them() {
    let s = scan_closure(&closure("anchor.json"));

    // CONTROL: both members really are emitted, so "they have no incoming
    // edge" is a statement about the anchor and not about emission.
    for member in ["waybill-fixture-anchora", "waybill-fixture-anchorb"] {
        assert!(
            s.role(member).is_some(),
            "{member} must be emitted as a closure component; got {:?}",
            s.closure_purls()
        );
    }
    // CONTROL: the root really is not a component. If some reader emitted it,
    // this fixture would be exercising the resolved path instead.
    assert!(
        !s.purls().iter().any(|p| p.contains("waybill-fixture-root")),
        "the root must not be a component, or this tests the wrong branch"
    );

    let cl = s.closure_purls();
    let incoming: Vec<(String, String)> = s
        .cdx_edges()
        .into_iter()
        .filter(|(_, t)| cl.contains(t.as_str()))
        .collect();
    assert!(
        incoming.is_empty(),
        "with an unrepresentable root there is nothing legitimate to anchor \
         these to, and inventing an edge from the subject would assert a \
         dependency nix did not state. got {incoming:?}"
    );
}

// ---------------------------------------------------------------------
// User Story 2 — the orphan population explains itself
// ---------------------------------------------------------------------

/// T031 + T032 are **unit tests in `generate/orphan_reason.rs`**, not here,
/// and the reason is a fixture limitation worth recording.
///
/// These fixtures cannot test root-reachability. Their root derivation is by
/// definition referenced by nothing, so it is never an emitted component, so
/// its projected edges are unanchored (FR-004b) and *every* member is
/// unreachable from the subject. A fixture where the root resolves would have
/// to be a project whose manifest reader emits it — which is the corpus, where
/// the end-to-end figure is measured instead: 25 unreachable closure
/// components, 20 of them carrying this code.
///
/// What stays here is the exclusion property, which the fixtures *can* test.
/// T033 / `contracts/orphan-reason.md` — the code's exclusions.
#[test]
fn m1280_t033_the_code_appears_on_no_other_population() {
    let s = scan_closure(&closure("stop_at_package.json"));
    const CODE: &str = "reachable-only-through-unemitted-package";
    let cl = s.closure_purls();
    for c in s.components() {
        let Some(purl) = c["purl"].as_str() else { continue };
        let has_code = c["properties"].as_array().is_some_and(|ps| {
            ps.iter().any(|p| {
                p["name"] == "waybill:orphan-reason" && p["value"] == CODE
            })
        });
        if !has_code {
            continue;
        }
        assert!(
            cl.contains(purl),
            "{purl} carries the code but is not a closure component; the \
             cause cannot apply to it"
        );
    }
}

// ---------------------------------------------------------------------
// User Story 3 — the roles are a filter, not a gate
// ---------------------------------------------------------------------

/// T037 — **milestone 1372 (#1366) supersedes half of milestone 1280's
/// FR-002, and this test now asserts the replacement.**
///
/// m1280's FR-002 has two halves. The first — *relationships must not be
/// derived from the declared dependency attributes* — is unchanged and is
/// asserted by the edge tests above. The second said the declared attributes
/// *"stay the sole input to the C182 role"*, and this test enforced it by
/// requiring `pkgb` to be `unreferenced`.
///
/// That half is retired. Measured on the pinned closure, it made 741 of
/// 1,275 derivations carry `unreferenced` while another derivation's
/// `inputs.drvs` named them — a label telling a consumer that nothing in the
/// build depends on `gzip 1.14` when three derivations do. The role is now a
/// function of the declared attributes **and** the build graph, with the
/// declared attributes still winning wherever they speak (FR-003a).
///
/// What this test protects now is the *bound* on that change: the graph may
/// only fill the gap the attributes leave.
#[test]
fn m1280_t037_the_graph_fills_only_the_gap_the_attributes_leave() {
    let s = scan_closure(&closure("through_fetch.json"));
    // `pkgb` is named by NO declared attribute and by `fetch.drv`'s
    // `inputs.drvs`. Before milestone 1372 it was `unreferenced`; it is now
    // tooling, because the build consumed it.
    assert_eq!(
        s.role("waybill-fixture-pkgb"),
        Some("build-tooling"),
        "pkgb is reached by an inputs.drvs edge and by no declared \
         attribute, so the graph is what gives it a role (m1372 FR-003)"
    );
    // CONTROL, and the bound: a derivation a declared attribute names keeps
    // exactly the role that attribute gives it. If this drifts to
    // `build-tooling` or `both`, the graph has started overriding declared
    // evidence rather than filling its gaps (m1372 FR-003a / SC-004a).
    assert_eq!(s.role("waybill-fixture-pkga"), Some("artifact-input"));
}

/// T038 — both endpoints of a closure edge still carry their role, so a
/// consumer can partition the graph by role without waybill having
/// pre-partitioned it.
#[test]
fn m1280_t038_both_endpoints_of_a_closure_edge_carry_their_role() {
    let s = scan_closure(&closure("through_fetch.json"));
    let cl = s.closure_purls();
    let mut checked = 0;
    for (from, to) in s.cdx_edges() {
        if !cl.contains(from.as_str()) || !cl.contains(to.as_str()) {
            continue;
        }
        for purl in [&from, &to] {
            let c = s
                .components()
                .iter()
                .find(|c| c["purl"].as_str() == Some(purl.as_str()))
                .expect("endpoint resolves");
            assert!(
                c["properties"].as_array().is_some_and(|ps| ps
                    .iter()
                    .any(|p| p["name"] == "waybill:closure-role")),
                "{purl} is an endpoint of a closure edge and must carry its role"
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no closure-to-closure edge found to check");
}

// ---------------------------------------------------------------------
// Polish
// ---------------------------------------------------------------------

/// T040 / SC-007 — a cycle terminates.
///
/// Asserted on a **completed scan**, not on the absence of a hang: a
/// non-terminating walk does not fail a test, it times the suite out, and a CI
/// timeout reads as infrastructure rather than as this guard.
#[test]
fn m1280_t040_a_closure_containing_a_cycle_still_emits_a_document() {
    let s = scan_closure(&closure("cycle.json"));
    assert!(
        !s.components().is_empty(),
        "the scan completed but emitted nothing, so the guard was not what \
         terminated it"
    );
    assert!(
        s.role("waybill-fixture-cyca").is_some(),
        "the cycle's members must still be classified; got {:?}",
        s.closure_purls()
    );
}

/// T057 / SC-008 — a degraded closure records **no** edge count, not a zero.
///
/// C184's row makes this distinction for every other figure it carries: a zero
/// would read as "the closure held nothing" rather than "no closure was read".
/// The scan is given a `nix` that fails, so the tier degrades.
#[test]
fn m1280_t057_a_degraded_closure_records_no_edge_count() {
    let dir = tempfile::tempdir().unwrap();
    let cache = seed_cache();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let nix = bin.join("nix");
    // A `nix` that fails every invocation: the tier must degrade, not guess.
    std::fs::write(&nix, "#!/bin/sh\nexit 1\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let cdx_path = dir.path().join("out.cdx.json");
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
            "--output",
        ])
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
        .env("WAYBILL_NIXPKGS_CACHE", cache.path())
        .output()
        .unwrap();
    assert!(
        cdx_path.exists(),
        "a degraded closure must still emit a document.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cdx: Value = serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    let doc: std::collections::HashMap<&str, &str> = cdx["metadata"]["properties"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| Some((p["name"].as_str()?, p["value"].as_str()?)))
                .collect()
        })
        .unwrap_or_default();

    // CONTROL: the tier really did degrade, so "no C184" is about the record
    // and not about a scan that quietly succeeded.
    assert!(
        doc.contains_key("waybill:nix-closure-degraded"),
        "the stub `nix` must have made the tier degrade; doc props: {:?}",
        doc.keys().collect::<Vec<_>>()
    );
    assert!(
        !doc.contains_key("waybill:nix-closure"),
        "a degraded closure records no C184 at all -- so no `edges` key, and \
         certainly not `edges: 0`, which would read as a closure that held \
         nothing"
    );
}

/// T058 / #1360 — `--tier` does not filter the closure tier, and these edges
/// inherit that rather than introducing it.
///
/// `apply_tier_filter` runs at `scan_cmd.rs:4121`; the closure's components
/// land at 4728 and its edges at 4814, both after it. That is pre-existing for
/// components and is filed as #1360. What this asserts is the consequence for
/// the edges: both endpoints also arrive after the filter, so no dangling
/// endpoint is produced — which is the only way this could have been a defect
/// of *this* milestone rather than an inherited one.
#[test]
fn m1280_t058_tier_filtering_leaves_no_dangling_closure_edge() {
    let s = scan_closure_with_args(&closure("through_fetch.json"), &["--tier", "source-only"]);
    let mut known = s.purls();
    known.extend(s.doc_root_refs());
    let dangling: Vec<(String, String)> = s
        .cdx_edges()
        .into_iter()
        .filter(|(f, t)| !known.contains(f) || !known.contains(t))
        .collect();
    assert!(
        dangling.is_empty(),
        "a --tier scan must leave no edge pointing at a dropped component; \
         got {dangling:?}"
    );
}

/// T056 / SC-003 — the two native completeness carriers agree with each other.
///
/// CycloneDX `compositions[].aggregate` and SPDX 3 `Relationship.completeness`
/// are rendered from one shared helper
/// (`generate/cyclonedx/compositions.rs::dependency_claims`, shared since
/// m1069), so a disagreement is a defect rather than a difference between
/// formats. This is the half of SC-003 that an orphan-count assertion does not
/// reach, and no other task covered it.
///
/// **The other half — that the `orphaned-components-detected` clause
/// disappears — is corpus-measured, not asserted here.** These fixtures cannot
/// connect anything to the subject: their root derivation is referenced by
/// nothing, so it is never a component and its edges are unanchored (FR-004b).
/// On the corpus the clause's count falls 352 → 10.
#[test]
fn m1280_t056_the_native_completeness_carriers_agree() {
    let s = scan_closure(&closure("through_fetch.json"));

    let cdx_incomplete = s.cdx["compositions"].as_array().is_some_and(|cs| {
        cs.iter().any(|c| {
            c.get("dependencies").is_some() && c["aggregate"].as_str() == Some("incomplete")
        })
    });
    let cdx_complete = s.cdx["compositions"].as_array().is_some_and(|cs| {
        cs.iter().any(|c| {
            c.get("dependencies").is_some() && c["aggregate"].as_str() == Some("complete")
        })
    });
    // CONTROL: CycloneDX made *some* dependency-completeness claim, or the
    // comparison below holds between two absences.
    assert!(
        cdx_incomplete || cdx_complete,
        "CycloneDX made no dependency-aggregate claim at all; compositions={:?}",
        s.cdx["compositions"]
    );

    let spdx3_values: std::collections::BTreeSet<String> = s.spdx3["@graph"]
        .as_array()
        .map(|g| {
            g.iter()
                .filter(|e| e["type"].as_str() == Some("Relationship"))
                .filter_map(|e| e["completeness"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if spdx3_values.is_empty() {
        // SPDX 3 asserts nothing, which is only consistent with CycloneDX
        // also declining to claim completeness.
        assert!(
            !cdx_complete,
            "CycloneDX claims `complete` while SPDX 3 asserts no completeness \
             at all -- the two render from one helper, so this is a defect"
        );
    } else {
        let expected = if cdx_complete { "complete" } else { "incomplete" };
        assert!(
            spdx3_values.contains(expected),
            "CycloneDX says {expected} and SPDX 3 says {spdx3_values:?}; the \
             two render from one shared helper, so a disagreement is a defect \
             rather than a format difference"
        );
    }
}
