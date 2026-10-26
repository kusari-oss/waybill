//! Feature 1269 (#1209) US4 — the per-ecosystem reach of transitive
//! non-runtime scope, for the ecosystems FR-015 names that have **no
//! corpus target**: gem, composer, mix, rebar, and uv's PEP 735
//! `[dependency-groups]` shape.
//!
//! The 17 corpus targets are `go-cobra`, `go-opentelemetry`,
//! `haskell-aeson`, `haskell-language-server`,
//! `haskell-security-advisories`, `image-postgres16`, `maven-guice`,
//! `nix-closure-moat`, `npm-express`, `pants-clojure-polyglot`,
//! `pants-example-django`, `pants-example-golang`,
//! `pants-example-javascript`, `pants-example-jvm`,
//! `pants-example-python`, `python-flask` and `rust-ripgrep`. None of
//! them is a gem, composer, mix or rebar project, so **every claim
//! this file makes rests on a synthetic fixture and says so** (US4
//! acceptance scenario 2). A synthetic fixture can show that a code
//! path fires; it cannot show that real projects in the wild have the
//! shape the fixture has.
//!
//! Fixtures use `waybill-fixture-*` coordinates rather than real
//! package names, so an advisory scan over this repository cannot
//! match them.
//!
//! Two outcomes are recorded here, and the distinction is the point:
//!
//! - **gem and composer are already correct**, and the evidence is
//!   that the transitive carries a non-runtime scope with **no** C199
//!   `waybill:lifecycle-scope-derivation` marker — i.e. the reader
//!   declared it and the 1269 pass had nothing to do. That is the
//!   FR-013 "byte-identical" shape, reproduced per ecosystem.
//! - **uv is fixed**, and its test is the one that shows this feature
//!   doing work: the dev-group member is `optional` because the uv
//!   reader said so, its transitive is `optional` **with** the C199
//!   marker because this pass inferred it, and the runtime branch is
//!   untouched. It is the `pydantic` shape from #1209 reproduced small
//!   enough to assert on — the real pin is not a corpus target, which
//!   is why the issue's own 103 could never be a regression test.
//! - **mix and rebar are not reached**, and the evidence is that their
//!   readers emit no dependency edge from a lockfile entry to its own
//!   transitives (`depends: Vec::new()` at every lock-entry
//!   construction site in `elixir.rs` and `erlang.rs`). A propagation
//!   pass over a graph with no edges to walk is a no-op by
//!   construction. The gap is upstream of this feature.
//!
//! The mix and rebar tests are characterization tests: they assert the
//! gap, so that **closing it makes them fail** and forces
//! `specs/1269-transitive-dev-scope/measurements/per-ecosystem-record.md`
//! to be updated rather than silently going stale.
//!
//! Only CycloneDX is read. The three-format claim does not need three
//! scans: C42 `waybill:lifecycle-scope` is a `SymmetricEqual` parity
//! row, so a value present in CDX and absent in either SPDX form fails
//! `holistic_parity` instead. Asserting it here as well would restate
//! a check that already exists.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

const DERIVATION_KEY: &str = "waybill:lifecycle-scope-derivation";
const SCOPE_KEY: &str = "waybill:lifecycle-scope";

fn fixture(ecosystem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scope_propagation")
        .join(ecosystem)
}

fn scan_cdx(ecosystem: &str) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture(ecosystem))
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{ecosystem}: scan failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap()
}

fn component<'a>(cdx: &'a Value, purl: &str) -> &'a Value {
    cdx.get("components")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("no components array"))
        .iter()
        .find(|c| c.get("purl").and_then(Value::as_str) == Some(purl))
        .unwrap_or_else(|| {
            let present: Vec<&str> = cdx["components"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|c| c.get("purl").and_then(Value::as_str))
                .collect();
            panic!("{purl} absent; document holds {present:?}")
        })
}

fn property(component: &Value, name: &str) -> Option<String> {
    component
        .get("properties")
        .and_then(Value::as_array)?
        .iter()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|p| p.get("value").and_then(Value::as_str))
        .map(str::to_string)
}

/// Every component that names `purl` in its `dependsOn`. Read from the
/// emitted document, so it includes any edge an emitter added.
fn inbound(cdx: &Value, purl: &str) -> Vec<String> {
    cdx.get("dependencies")
        .and_then(Value::as_array)
        .map(|deps| {
            deps.iter()
                .filter(|d| {
                    d.get("dependsOn")
                        .and_then(Value::as_array)
                        .is_some_and(|t| {
                            t.iter().any(|v| v.as_str() == Some(purl))
                        })
                })
                .filter_map(|d| {
                    d.get("ref").and_then(Value::as_str).map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// **gem — already correct.** Synthetic fixture, no corpus target.
///
/// `Gemfile` puts `waybill-fixture-devtool` in `group :development`;
/// `Gemfile.lock` makes `waybill-fixture-devtool-core` its only
/// transitive. The gem reader classifies every spec outside the
/// production BFS closure as non-runtime — including transitives it
/// never saw a group for (`gem.rs`, the `None => Development` arm) —
/// so the transitive is scoped before the 1269 pass runs.
#[test]
fn gem_transitive_of_a_development_group_is_already_scoped_by_the_reader() {
    const DEVTOOL: &str = "pkg:gem/waybill-fixture-devtool@2.0.0";
    const DEVTOOL_CORE: &str = "pkg:gem/waybill-fixture-devtool-core@3.0.0";
    const RUNTIME_CORE: &str = "pkg:gem/waybill-fixture-runtime-core@1.0.0";

    let cdx = scan_cdx("gem");

    let devtool = component(&cdx, DEVTOOL);
    assert_eq!(property(devtool, SCOPE_KEY).as_deref(), Some("development"));

    let core = component(&cdx, DEVTOOL_CORE);
    assert_eq!(
        property(core, SCOPE_KEY).as_deref(),
        Some("development"),
        "the transitive of a :development group gem must be non-runtime"
    );
    assert_eq!(
        property(core, DERIVATION_KEY),
        None,
        "no C199 marker: the gem READER declared this scope, so gem's \
         outcome is `already correct` and not `fixed`. A marker \
         appearing here would mean the reader stopped doing it and the \
         1269 pass started covering for it — a different world, \
         recorded differently."
    );
    assert_eq!(
        inbound(&cdx, DEVTOOL_CORE),
        vec![DEVTOOL.to_string()],
        "the edge the pass WOULD have walked exists, which is what \
         makes `already correct` distinguishable from `not reached`"
    );

    let runtime_core = component(&cdx, RUNTIME_CORE);
    assert_eq!(
        property(runtime_core, SCOPE_KEY),
        None,
        "the control: a transitive of a default-group gem keeps runtime"
    );
}

/// **composer — already correct.** Synthetic fixture, no corpus target.
///
/// `composer.lock`'s `packages-dev[]` is the full dev closure that
/// composer itself resolved, so the transitive is listed there
/// alongside its parent and the reader scopes both
/// (`composer.rs::emit_lockfile_packages`).
#[test]
fn composer_transitive_in_packages_dev_is_already_scoped_by_the_reader() {
    const DEVTOOL: &str = "pkg:composer/waybill-fixture/devtool@2.0.0";
    const DEVTOOL_CORE: &str = "pkg:composer/waybill-fixture/devtool-core@3.0.0";
    const RUNTIME_CORE: &str = "pkg:composer/waybill-fixture/runtime-core@1.0.0";

    let cdx = scan_cdx("composer");

    assert_eq!(
        property(component(&cdx, DEVTOOL), SCOPE_KEY).as_deref(),
        Some("development")
    );

    let core = component(&cdx, DEVTOOL_CORE);
    assert_eq!(
        property(core, SCOPE_KEY).as_deref(),
        Some("development"),
        "a dev-closure member listed in packages-dev[] is non-runtime \
         whether or not anything in the document depends on it"
    );
    assert_eq!(
        property(core, DERIVATION_KEY),
        None,
        "no C199 marker: declared by the composer reader, not inferred"
    );

    assert_eq!(
        property(component(&cdx, RUNTIME_CORE), SCOPE_KEY),
        None,
        "the control: a packages[] transitive keeps runtime"
    );
}

/// **mix — not reached**, and the reason is a missing edge rather than
/// a scope decision. Synthetic fixture, no corpus target.
///
/// `mix.exs` declares `waybill_fixture_devtool` with `only: [:dev,
/// :test], runtime: false`, which the reader does read: that component
/// comes out `development`. Its transitive
/// `waybill_fixture_devtool_core` is in `mix.lock` and comes out with
/// no scope at all, because the mix reader constructs every lockfile
/// entry with `depends: Vec::new()` — `mix.lock` records hashes, not
/// a dependency graph, and the reader does not reconstruct one.
///
/// So the 1269 pass sees a component with **zero inbound edges** and
/// correctly declines: there is no path from a non-runtime component
/// to walk. Fixing this means giving the mix reader transitive edges,
/// which is a separate change.
///
/// Characterization test: when mix gains transitive edges this fails,
/// and the per-ecosystem record must move from *not reached* to
/// whatever is then true.
#[test]
fn mix_transitive_is_not_reached_because_the_reader_emits_no_edges() {
    const DEVTOOL: &str = "pkg:hex/waybill_fixture_devtool@2.0.0";
    const DEVTOOL_CORE: &str = "pkg:hex/waybill_fixture_devtool_core@3.0.0";

    let cdx = scan_cdx("mix");

    assert_eq!(
        property(component(&cdx, DEVTOOL), SCOPE_KEY).as_deref(),
        Some("development"),
        "the mix reader DOES read `only: [:dev, :test]` for a declared dep"
    );

    let core = component(&cdx, DEVTOOL_CORE);
    assert_eq!(
        inbound(&cdx, DEVTOOL_CORE),
        Vec::<String>::new(),
        "the gap: nothing depends on the transitive, so no non-runtime \
         path to it exists for the pass to follow. If this now has an \
         inbound edge the mix reader gained transitive edges and the \
         1269 per-ecosystem record is stale."
    );
    assert_eq!(
        property(core, SCOPE_KEY),
        None,
        "consequence of the gap, not a decision by this feature"
    );
    assert_eq!(property(core, DERIVATION_KEY), None);
}

/// **rebar — not reached**, same missing-edge cause, plus a second
/// gap worth recording separately. Synthetic fixture, no corpus target.
///
/// `rebar.config` declares `waybill_fixture_devtool` under the `test`
/// profile, and the reader's profile classifier (`erlang.rs`, the
/// `Some("dev") | Some("test") | Some("doc")` arm) applies to
/// **design-tier declared deps only** — entries that have no match in
/// `rebar.lock`. Here every dep IS in the lock, so the lock-tier
/// entries are built with `lifecycle_scope: Some(Runtime)` and the
/// profile is lost. The transitive additionally has no inbound edge,
/// `rebar.lock`'s level column notwithstanding.
///
/// Two independent reasons the pass cannot act, and FR-003 forbids it
/// from overriding the reader's `Runtime` even if an edge existed —
/// the same shape as #1272 on the cargo side.
#[test]
fn rebar_transitive_is_not_reached_because_the_lock_tier_loses_the_profile() {
    const DEVTOOL: &str = "pkg:hex/waybill_fixture_devtool@2.0.0";
    const DEVTOOL_CORE: &str = "pkg:hex/waybill_fixture_devtool_core@3.0.0";

    let cdx = scan_cdx("rebar");

    assert_eq!(
        property(component(&cdx, DEVTOOL), SCOPE_KEY),
        None,
        "gap 1: the `test`-profile dep is scoped runtime once it \
         resolves against rebar.lock, because the profile classifier \
         only runs on the design-tier path. If this now reads \
         `development` the rebar reader was fixed and the 1269 \
         per-ecosystem record is stale."
    );

    assert_eq!(
        property(component(&cdx, DEVTOOL_CORE), SCOPE_KEY),
        None,
        "gap 2: the transitive is runtime too, and with no non-runtime \
         seed anywhere in the document there is nothing to propagate"
    );
    assert_eq!(property(component(&cdx, DEVTOOL_CORE), DERIVATION_KEY), None);
}

/// **uv — fixed.** Synthetic fixture, no corpus target.
///
/// `pyproject.toml` declares `waybill-fixture-devtool` in a PEP 735
/// `[dependency-groups] dev` list; `uv.lock` records it under
/// `[package.dev-dependencies]` on the root and gives it one exclusive
/// transitive. The uv reader scopes the **member** `Optional`
/// (`waybill:optional-derivation = pep-735-dependency-groups:dev`) and
/// stops there — that stopping point is the defect #1209 describes.
///
/// This is the `pydantic` shape, small enough to assert on. The real
/// pin is not one of the 17 corpus targets, so the issue's own
/// observation (103 unscoped transitives on waybill 0.10.1) can never
/// be a regression test; this can.
///
/// It is also the only test in this file where C199 is **present**,
/// which is what separates *fixed* from *already correct*: the marker
/// says waybill inferred the scope rather than reading it.
#[test]
fn uv_transitive_of_a_pep_735_dependency_group_is_marked_by_the_pass() {
    const DEVTOOL: &str = "pkg:pypi/waybill-fixture-devtool@2.0.0";
    const DEVTOOL_CORE: &str = "pkg:pypi/waybill-fixture-devtool-core@3.0.0";
    const RUNTIME: &str = "pkg:pypi/waybill-fixture-runtime@1.5.0";
    const RUNTIME_CORE: &str = "pkg:pypi/waybill-fixture-runtime-core@1.0.0";

    let cdx = scan_cdx("uv");

    let devtool = component(&cdx, DEVTOOL);
    assert_eq!(
        property(devtool, SCOPE_KEY).as_deref(),
        Some("optional"),
        "the dependency-group MEMBER is what the uv reader already scoped"
    );
    assert_eq!(
        property(devtool, DERIVATION_KEY),
        None,
        "and it carries no marker, because the reader declared it"
    );

    let core = component(&cdx, DEVTOOL_CORE);
    assert_eq!(
        property(core, SCOPE_KEY).as_deref(),
        Some("optional"),
        "the defect in #1209: before this feature the transitive of a \
         dev-group member carried no scope and read as a runtime \
         dependency"
    );
    assert_eq!(
        property(core, DERIVATION_KEY).as_deref(),
        Some("transitive"),
        "C199 present — this scope was INFERRED by the propagation \
         pass. The one place in this file where the marker appears, and \
         what makes uv's outcome `fixed` rather than `already correct`."
    );
    assert_eq!(
        inbound(&cdx, DEVTOOL_CORE),
        vec![DEVTOOL.to_string()],
        "reached by exactly one declared edge, from a non-runtime parent"
    );

    // The runtime branch, in the same document, unmoved. Without this
    // the test would pass just as happily against a pass that marked
    // everything. The root itself is not checked here — CycloneDX
    // collapses it into `metadata.component`, so it is not a member of
    // `components[]`.
    for purl in [RUNTIME, RUNTIME_CORE] {
        let c = component(&cdx, purl);
        assert_eq!(
            property(c, SCOPE_KEY),
            None,
            "{purl} is runtime-reachable and must keep runtime scope"
        );
        assert_eq!(property(c, DERIVATION_KEY), None, "{purl}");
    }
}
