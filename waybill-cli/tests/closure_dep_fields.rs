//! Milestone 1279 (#1346) — `unreferenced` must mean "nothing depends on this".
//!
//! The closure classifier read 7 of nixpkgs' dependency attributes. Measured
//! on the pinned x86_64-linux closure, 6 more were declared and populated,
//! carrying 230 declarations across 23 derivations — so a member whose only
//! dependency edge ran through one of them was reported exactly like a member
//! nothing depends on. The first is a fact about the closure; the second is a
//! fact about waybill, and a consumer could not tell them apart.
//!
//! This milestone does two things, and keeping them separate is the point:
//! four attributes move into classification, and three stdenv-injected ones
//! stay out and gain a **marker** instead. The marker is what lets the
//! census movement be attributed to the four, rather than to a change whose
//! size nobody can account for.
//!
//! **Every role assertion here is on the emitted `waybill:closure-role`,
//! never on a parsed field** — the same rule milestone 1278's suite states,
//! for the same reason. Output paths arrive without the `/nix/store/` prefix
//! while the fields referencing them arrive with it, so comparing them
//! unstripped resolves nothing without erroring. A field-level assertion
//! stays green while every member classifies as `unreferenced`.
//!
//! **A second trap this suite has to work around.** An `unreferenced` member
//! is not emitted as a component at all unless it applies a patch (C182,
//! spec FR-004a) — so the headline case is only *observable* on a
//! patch-applier, and the fixtures give the stdenv-named members a patch for
//! exactly that reason. Without it, "the marker is absent" and "the component
//! is absent" are the same observation, and every marker assertion below
//! would be vacuous.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision the fixture `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

const ROLE: &str = "waybill:closure-role";
const MARKER: &str = "waybill:closure-unclassified-refs";
const DISCLOSURE: &str = "waybill:nix-closure-read";

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
        .join("tests/fixtures/nix_closure_dep_fields")
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
  # m1423 (T006): intended. No store holds the evaluated output, so the scope
  # pass takes its fallback path rather than the runtime closure.
  "path-info"*) echo "error: path '/nix/store/stub' is not valid" >&2; exit 1 ;;
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

    fn names(&self) -> Vec<&str> {
        self.components()
            .iter()
            .filter_map(|c| c["name"].as_str())
            .collect()
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

    /// The C205 marker, parsed into the attribute names it carries.
    fn marker(&self, name: &str) -> Option<Vec<String>> {
        let raw = self.prop(name, MARKER)?;
        let v: Value = serde_json::from_str(raw)
            .unwrap_or_else(|e| panic!("{MARKER} on {name} is not a JSON array: {raw:?} ({e})"));
        Some(
            v.as_array()
                .unwrap_or_else(|| panic!("{MARKER} must be an array, got {raw:?}"))
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect(),
        )
    }

    fn doc_prop(&self, name: &str) -> Option<&str> {
        self.cdx["metadata"]["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some(name))?
            .get("value")?
            .as_str()
    }

    /// The C204 disclosure, parsed. `None` when nothing went unconsulted.
    fn disclosure(&self) -> Option<Value> {
        serde_json::from_str(self.doc_prop(DISCLOSURE)?).ok()
    }

    fn fields_unconsulted(&self) -> Vec<String> {
        self.disclosure()
            .and_then(|d| {
                d["fields-unconsulted"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|f| f.as_str().map(str::to_string))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// The annotation `field` on the SPDX 2.3 package named `component`.
    ///
    /// Scoped to the package on purpose. The first version of this helper took
    /// the first package carrying the field at all, and the cross-format test
    /// then compared one component's CDX value against a *different*
    /// component's SPDX value — reporting a parity failure where the two
    /// formats agreed exactly.
    fn spdx23_annotation(&self, component: &str, field: &str) -> Option<String> {
        self.spdx23["packages"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some(component))?
            .get("annotations")
            .and_then(|a| annotation_value(a, field))
    }

    /// The same, for SPDX 3: resolve the element by name, then the annotation
    /// whose `subject` is that element.
    fn spdx3_annotation(&self, component: &str, field: &str) -> Option<String> {
        let graph = self.spdx3["@graph"].as_array()?;
        let subject = graph
            .iter()
            .find(|el| el["name"].as_str() == Some(component))?
            .get("spdxId")?
            .as_str()?;
        for el in graph {
            if el["type"].as_str() != Some("Annotation")
                || el["subject"].as_str() != Some(subject)
            {
                continue;
            }
            if let Some(v) = envelope_value(el["statement"].as_str()?, field) {
                return Some(v);
            }
        }
        None
    }
}

fn annotation_value(annotations: &Value, name: &str) -> Option<String> {
    for a in annotations.as_array()? {
        if let Some(v) = envelope_value(a["comment"].as_str()?, name) {
            return Some(v);
        }
    }
    None
}

/// Pull the value out of a `waybill-annotation/v1` envelope.
///
/// One field per annotation — milestone 1278's suite learned this the hard
/// way: a helper looking for a `values` map found nothing, and that read as
/// "SPDX 2.3 does not emit the annotation" when it was there all along.
fn envelope_value(comment: &str, name: &str) -> Option<String> {
    let v: Value = serde_json::from_str(comment).ok()?;
    if v.get("field")?.as_str()? != name {
        return None;
    }
    Some(match v.get("value")? {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    })
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
    let out = {
            // #1046: never reach Hackage from a test (Principle VII).
            let mut c = Command::new(binary_path());
            c.env("WAYBILL_HACKAGE_URL", "http://127.0.0.1:9");
            c
        }
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

// ---------------------------------------------------------------------
// T015 — the control every assertion below depends on
// ---------------------------------------------------------------------

/// A scan that classified nothing satisfies every "is not wrongly classified"
/// assertion vacuously, and so does one whose store-path join resolved
/// nothing. Prove first that the closure was read and that something was
/// referenced — including through an attribute this milestone added, since
/// that is the claim the rest of the suite rests on.
#[test]
fn m1279_t015_the_closure_was_read_and_a_new_attribute_resolved() {
    let s = scan_closure(&closure("newly_consulted.json"));
    // Scoped to this fixture's own members. The scanned project contributes
    // components of its own (`waybill-fixture-roletest` and friends, from the
    // m1277 flake this suite reuses), so a count over every
    // `waybill-fixture-` name measures the project as well as the closure.
    let emitted: Vec<&str> = s
        .names()
        .into_iter()
        .filter(|n| {
            matches!(
                *n,
                "waybill-fixture-tt"
                    | "waybill-fixture-ttp"
                    | "waybill-fixture-bt"
                    | "waybill-fixture-pnbi"
                    | "waybill-fixture-ctt"
                    | "waybill-fixture-cttp"
                    | "waybill-fixture-cbt"
                    | "waybill-fixture-cpnbi"
            )
        })
        .collect();
    assert_eq!(
        emitted.len(),
        4,
        "the four referenced members are emitted and the four consumers -- \
         `unreferenced`, applying no patch -- are not; got {emitted:?}"
    );
    for member in [
        "waybill-fixture-tt",
        "waybill-fixture-ttp",
        "waybill-fixture-bt",
        "waybill-fixture-pnbi",
    ] {
        assert!(
            s.role(member).is_some(),
            "{member} must carry a role at all before its VALUE means anything"
        );
        assert_ne!(
            s.role(member),
            Some("unreferenced"),
            "{member} is named by a consulted attribute, so `unreferenced` here \
             would mean the basename join resolved nothing"
        );
    }
}

// ---------------------------------------------------------------------
// User Story 1 — the four newly-consulted attributes
// ---------------------------------------------------------------------

/// T009 / contract P1. One member per attribute, so each attribute's effect
/// is separable rather than a union nobody can attribute.
#[test]
fn m1279_t009_each_newly_consulted_attribute_credits_its_own_side() {
    let s = scan_closure(&closure("newly_consulted.json"));
    for (member, expected, attribute) in [
        ("waybill-fixture-tt", "artifact-input", "depsTargetTarget"),
        ("waybill-fixture-ttp", "artifact-input", "depsTargetTargetPropagated"),
        ("waybill-fixture-bt", "build-tooling", "depsBuildTarget"),
        ("waybill-fixture-pnbi", "build-tooling", "propagatedNativeBuildInputs"),
    ] {
        assert_eq!(
            s.role(member),
            Some(expected),
            "{member} is referenced only through {attribute}, which this \
             milestone classifies as {expected}"
        );
    }
}

/// T010 / contract P6. The deliberate remainder: stdenv's own machinery is
/// disclosed, never classified.
#[test]
fn m1279_t010_a_stdenv_default_earns_the_marker_and_no_role() {
    let s = scan_closure(&closure("stdenv_only.json"));
    for (member, attribute) in [
        ("waybill-fixture-dnbi", "defaultNativeBuildInputs"),
        ("waybill-fixture-dbi", "defaultBuildInputs"),
        ("waybill-fixture-enbi", "extraNativeBuildInputs"),
    ] {
        // CONTROL: the component exists. It is emitted only because it
        // applies a patch; were it absent, "no role" would be true for the
        // wrong reason and the marker assertion unreachable.
        assert_eq!(
            s.role(member),
            Some("unreferenced"),
            "{member} must be emitted and unclassified; names={:?}",
            s.names()
        );
        assert_eq!(
            s.marker(member),
            Some(vec![attribute.to_string()]),
            "{member} is named only by {attribute}, so the marker must say so \
             -- that is the whole of FR-012: disclose, do not classify"
        );
    }
}

/// T011 / contract P3. A declared-but-empty attribute credits nothing and is
/// not a gap. Four attributes are declared on every derivation of the darwin
/// closure and hold zero paths.
#[test]
fn m1279_t011_a_declared_but_empty_attribute_is_not_a_gap() {
    let s = scan_closure(&closure("attribute_states.json"));
    let unconsulted = s.fields_unconsulted();
    // CONTROL: the disclosure exists at all, so "not in it" is a real
    // observation rather than a missing document.
    assert!(
        !unconsulted.is_empty(),
        "the fixture declares one populated unconsulted attribute, so a \
         disclosure must be present for its absences to mean anything"
    );
    for empty in ["depsHostHost", "depsHostHostPropagated", "propagatedBuildInputs"] {
        assert!(
            !unconsulted.contains(&empty.to_string()),
            "{empty} is declared empty; reporting it as a gap would make the \
             disclosure cry wolf. got {unconsulted:?}"
        );
    }
    for attributes in s
        .names()
        .into_iter()
        .filter_map(|n| s.marker(n))
        .collect::<Vec<_>>()
    {
        assert!(
            !attributes.iter().any(|a| a == "depsHostHostPropagated"),
            "an empty attribute can have named no member"
        );
    }
}

/// T012 / contract P4 — the third state, which research R4 found and a
/// consulted-vs-unconsulted fixture would have missed. `depsBuildHost` and
/// `nativeCheckInputs` are consulted but declared on zero derivations of
/// either measured closure.
#[test]
fn m1279_t012_a_consulted_but_undeclared_attribute_is_not_a_gap() {
    let s = scan_closure(&closure("attribute_states.json"));
    let unconsulted = s.fields_unconsulted();
    for undeclared in ["depsBuildHost", "nativeCheckInputs"] {
        assert!(
            !unconsulted.contains(&undeclared.to_string()),
            "{undeclared} is declared nowhere in this closure, so it is neither \
             consulted nor skipped -- it is absent. got {unconsulted:?}"
        );
    }
}

/// T013 / contract P2. The union rule is what makes a member referenced both
/// ways `both` rather than a coin toss; three members move
/// `artifact-input`→`both` on the pinned closure, and the widening must not
/// disturb it.
#[test]
fn m1279_t013_the_union_rule_is_unchanged_by_the_widening() {
    let s = scan_closure(&closure("attribute_states.json"));
    assert_eq!(s.role("waybill-fixture-art"), Some("artifact-input"));
    assert_eq!(s.role("waybill-fixture-tool"), Some("build-tooling"));
    assert_eq!(
        s.role("waybill-fixture-both"),
        Some("both"),
        "named by one artifact-side and one tooling-side attribute"
    );
}

/// T014 / FR-013 — the marker **names** the attributes, and a member named by
/// two carries both. "Referenced by something waybill does not classify"
/// without saying what is a smaller version of the same defect.
#[test]
fn m1279_t014_the_marker_names_every_attribute_that_reached_the_member() {
    let s = scan_closure(&closure("stdenv_only.json"));
    assert_eq!(
        s.marker("waybill-fixture-two"),
        Some(vec![
            "defaultNativeBuildInputs".to_string(),
            "extraNativeBuildInputs".to_string(),
        ]),
        "sorted, and both -- a marker that kept only the first would under-report"
    );
}

/// T044 — the gating row every other marker test misses.
///
/// `contracts/unclassified-marker.md` says the marker is present whatever the
/// role, and every test above uses a member whose *only* reference is
/// unclassified. So a gating rule that emitted the marker only for
/// `unreferenced` members would pass all of them. This is that row.
#[test]
fn m1279_t044_a_member_with_a_role_still_carries_the_marker() {
    let s = scan_closure(&closure("attribute_states.json"));
    assert_eq!(
        s.role("waybill-fixture-both"),
        Some("both"),
        "the role must be present for 'role AND marker' to be the claim tested"
    );
    assert_eq!(
        s.marker("waybill-fixture-both"),
        Some(vec!["defaultNativeBuildInputs".to_string()]),
        "a reader comparing two members of the same role should be able to tell \
         that one of them also has an unread reference"
    );
    // And the complement: a member reached only through consulted attributes
    // carries no marker, or the marker would mean nothing.
    assert_eq!(s.marker("waybill-fixture-art"), None);
    assert_eq!(s.marker("waybill-fixture-tool"), None);
}

// ---------------------------------------------------------------------
// User Story 2 — propagation
// ---------------------------------------------------------------------

/// T022 — the direct case.
#[test]
fn m1279_t022_a_directly_propagated_native_input_is_tooling() {
    let s = scan_closure(&closure("propagation_chain.json"));
    assert_eq!(s.role("waybill-fixture-mid"), Some("build-tooling"));
}

/// T023 — the case that makes propagation different in kind.
///
/// `propagatedNativeBuildInputs` is the one unread attribute whose effect was
/// not local: it propagates to consumers, so leaving it unread understated
/// reachability by more than its own declaration count. A direct-only test
/// would miss that, so the fixture puts the member two hops away.
#[test]
fn m1279_t023_a_member_two_hops_of_propagation_away_is_still_tooling() {
    let s = scan_closure(&closure("propagation_chain.json"));
    assert_eq!(
        s.role("waybill-fixture-leaf"),
        Some("build-tooling"),
        "reachable only as mid's propagated input, where mid is root's -- \
         `unreferenced` here is the #1346 defect in miniature"
    );
}

// ---------------------------------------------------------------------
// User Story 3 — the disclosure and the consulted set agree
// ---------------------------------------------------------------------

/// T025 / SC-009 / contract P5 — the property a passing role test does not
/// cover. The classifier could read an attribute the disclosure still
/// reports as unconsulted, and a consumer would be told a gap exists where it
/// does not.
#[test]
fn m1279_t025_the_disclosure_is_the_exact_complement_of_the_consulted_set() {
    let s = scan_closure(&closure("attribute_states.json"));
    let unconsulted = s.fields_unconsulted();
    for consulted in [
        "buildInputs",
        "propagatedBuildInputs",
        "depsHostHost",
        "nativeBuildInputs",
        "depsBuildBuild",
        "depsBuildHost",
        "nativeCheckInputs",
        "depsTargetTarget",
        "depsTargetTargetPropagated",
        "depsBuildTarget",
        "propagatedNativeBuildInputs",
        "patches",
    ] {
        assert!(
            !unconsulted.contains(&consulted.to_string()),
            "{consulted} IS read, so naming it as a gap would be a false alarm \
             in the mechanism whose whole job is to be trustworthy when it \
             stays silent. got {unconsulted:?}"
        );
    }
    assert_eq!(
        unconsulted,
        vec!["defaultNativeBuildInputs".to_string()],
        "and the complement is exactly what is left"
    );
}

/// T026 — the remaining gap stays visible rather than being silently closed,
/// and an unpopulated attribute is correctly not a gap (FR-005).
#[test]
fn m1279_t026_every_populated_stdenv_default_is_still_disclosed() {
    let s = scan_closure(&closure("stdenv_only.json"));
    let unconsulted = s.fields_unconsulted();
    for populated in [
        "defaultNativeBuildInputs",
        "defaultBuildInputs",
        "extraNativeBuildInputs",
    ] {
        assert!(
            unconsulted.contains(&populated.to_string()),
            "{populated} is populated and unread, so the gap must stay visible. \
             got {unconsulted:?}"
        );
    }
    let s2 = scan_closure(&closure("attribute_states.json"));
    assert!(
        !s2.fields_unconsulted().contains(&"defaultBuildInputs".to_string()),
        "and on a closure that declares none of it, it is not a gap -- the \
         linux/darwin split is exactly this: `extraNativeBuildInputs` holds \
         paths on linux and none on darwin, `defaultBuildInputs` the reverse"
    );
}

/// T027 — a closure with nothing left unconsulted emits no disclosure at all.
/// Absence must mean "nothing was skipped", never "not checked".
#[test]
fn m1279_t027_nothing_unconsulted_emits_no_disclosure() {
    let s = scan_closure(&closure("newly_consulted.json"));
    assert_eq!(
        s.doc_prop(DISCLOSURE),
        None,
        "every attribute this fixture declares is now consulted, so a \
         zero-valued record would claim a gap that does not exist"
    );
    // CONTROL: this fixture's attributes *were* read — see T015 — so the
    // absence above is the disclosure's silence and not an unread closure.
    assert_eq!(s.role("waybill-fixture-tt"), Some("artifact-input"));
}

// ---------------------------------------------------------------------
// User Story 4 — the guards
// ---------------------------------------------------------------------

/// T030 / FR-007 — an attribute nixpkgs does not currently emit is reported
/// rather than ignored. This is what makes the marker's *absence*
/// trustworthy: an unread attribute cannot masquerade as an absent one.
#[test]
fn m1279_t030_an_attribute_nixpkgs_does_not_yet_emit_is_still_reported() {
    let s = scan_closure(&closure("future_attribute.json"));
    assert!(
        s.fields_unconsulted().contains(&"depsHostTarget".to_string()),
        "recognised by nixpkgs' naming convention, not by an enumerated list \
         -- an allowlist is what #1346 is about. got {:?}",
        s.fields_unconsulted()
    );
    assert_eq!(
        s.marker("waybill-fixture-future"),
        Some(vec!["depsHostTarget".to_string()]),
        "and the member it names carries it"
    );
}

/// T031 / SC-007 — the mutation. A widened field set no test is sensitive to
/// is not evidence that the widening does anything.
///
/// The constant cannot be edited from an integration test, so the mutation is
/// applied to the closure instead: rename the attribute to a name that is not
/// a dependency attribute at all, and the roles it was crediting must vanish.
/// That is the same claim — the role exists *because* that attribute is read.
#[test]
fn m1279_t031_mutation_the_propagation_roles_depend_on_the_widened_set() {
    let healthy = scan_closure(&closure("propagation_chain.json"));
    assert_eq!(healthy.role("waybill-fixture-leaf"), Some("build-tooling"));
    assert_eq!(healthy.role("waybill-fixture-mid"), Some("build-tooling"));

    let mutated = closure("propagation_chain.json")
        .replace("propagatedNativeBuildInputs", "waybillFixtureNotAnAttribute");
    let s = scan_closure(&mutated);
    for member in ["waybill-fixture-leaf", "waybill-fixture-mid"] {
        assert_eq!(
            s.role(member),
            None,
            "with the attribute unread, {member} is referenced by nothing and \
             is not emitted at all -- which is precisely the output #1346 \
             reports as indistinguishable from 'nothing depends on this'"
        );
    }
}

/// T032 / SC-011 — a member named only by an unclassified attribute has the
/// **same role** with and without the marker. This is what keeps the census
/// attributable to the four consulted attributes alone: if the marker could
/// move a role, the before/after figures would mix two changes.
#[test]
fn m1279_t032_the_marker_does_not_move_a_role() {
    let with = scan_closure(&closure("stdenv_only.json"));
    // The same closure with the attribute renamed to something that is not a
    // dependency attribute by nixpkgs' convention, so no marker is emitted.
    let without = scan_closure(
        &closure("stdenv_only.json").replace("defaultNativeBuildInputs", "waybillFixtureRenamed"),
    );
    assert_eq!(
        with.marker("waybill-fixture-dnbi"),
        Some(vec!["defaultNativeBuildInputs".to_string()])
    );
    assert_eq!(
        without.marker("waybill-fixture-dnbi"),
        None,
        "the control: the marker really is gone in the second arm, or the \
         comparison below holds trivially"
    );
    assert_eq!(
        with.role("waybill-fixture-dnbi"),
        without.role("waybill-fixture-dnbi"),
        "the role must be identical either way -- the marker is observational"
    );
}

/// The marker's payload is identical in all three formats (C205,
/// `SymmetricEqual`). A row differing in shape between formats forces its
/// extractor to compensate, which is how `c19_cdx` came to hand-split a
/// pipe-string.
#[test]
fn m1279_the_marker_is_identical_in_all_three_formats() {
    let s = scan_closure(&closure("stdenv_only.json"));
    let cdx = s.prop("waybill-fixture-two", MARKER).map(str::to_string);
    assert!(cdx.is_some(), "CDX must carry it for the comparison to mean anything");
    assert_eq!(
        cdx,
        s.spdx23_annotation("waybill-fixture-two", MARKER),
        "SPDX 2.3 must match CDX byte for byte"
    );
    assert_eq!(
        cdx,
        s.spdx3_annotation("waybill-fixture-two", MARKER),
        "SPDX 3 must match CDX byte for byte"
    );
    // CONTROL: the lookup really is per-component. Were it finding the first
    // package carrying the field at all, this member -- whose marker names a
    // different attribute -- would compare equal to the one above.
    assert_eq!(
        s.spdx23_annotation("waybill-fixture-dbi", MARKER).as_deref(),
        Some(r#"["defaultBuildInputs"]"#)
    );
}
