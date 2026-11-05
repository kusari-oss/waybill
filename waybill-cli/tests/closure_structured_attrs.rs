//! Milestone 1278 (#1348) — a closure member's attributes are read wherever
//! nix put them.
//!
//! `nix derivation show` has two carriers for a derivation's attributes: a
//! flat `env` map of strings, and a typed `structuredAttrs` object when
//! nixpkgs sets `__structuredAttrs = true`. The reader only ever read the
//! first, so on the measured corpus closure 554 dependency entries across 421
//! of 1,749 derivations read as absent — and absent is indistinguishable from
//! "nothing depends on this".
//!
//! **Every role assertion here is on the emitted `waybill:closure-role`, never
//! on a parsed field.** The store-path join is a second, silent failure: output
//! paths arrive without the `/nix/store/` prefix while the fields referencing
//! them arrive with it, so comparing them unstripped resolves nothing without
//! erroring. A field-level assertion is green while every member is classified
//! `unreferenced`, which is how the first version of that analysis reported
//! 1,275 of 1,275 unreferenced and read as a finding rather than a bug.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision the fixture `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// A closure exercising both carriers at once.
///
/// `flatdep` is referenced through the flat carrier and `typeddep` through the
/// typed one, by the same fields, so the two are a matched pair: whatever the
/// reader does for one it must do for the other. `unconsulted` declares its
/// dependency in `defaultNativeBuildInputs`, which classification does not
/// read — the FR-014 diagnostic must name that field without its member's role
/// changing.
///
/// **That field was `depsBuildTarget` until milestone 1279 (#1346), which
/// consults it.** A test whose premise is "this attribute is unread" has to be
/// re-pointed when the set of unread attributes shrinks, and the six failures
/// it caused were the correct answer rather than a regression.
/// `defaultNativeBuildInputs` is a deliberate long-term choice, not the next
/// arbitrary name: FR-012 excludes stdenv's injected attributes on purpose, so
/// it is unlikely to be consulted by a later milestone for the same reason it
/// is not consulted now.
/// `disagree` declares the same field in both carriers with different values.
const CLOSURE_JSON: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-roleproj-0.1.0",
      "env": {
        "pname": "waybill-fixture-roleproj",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-flatdep /nix/store/ha-typedidentity",
        "nativeBuildInputs": "/nix/store/h2-flattool"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-flatdep.drv": {
      "name": "waybill-fixture-flatdep-1.0",
      "env": { "pname": "waybill-fixture-flatdep", "version": "1.0" },
      "outputs": { "out": { "path": "h1-flatdep" } }
    },
    "d-flattool.drv": {
      "name": "waybill-fixture-flattool-1.0",
      "env": { "pname": "waybill-fixture-flattool", "version": "1.0" },
      "outputs": { "out": { "path": "h2-flattool" } }
    },
    "d-typedconsumer.drv": {
      "name": "waybill-fixture-typedconsumer-1.0",
      "env": { "out": "/nix/store/h3-typedconsumer" },
      "structuredAttrs": {
        "pname": "waybill-fixture-typedconsumer",
        "version": "1.0",
        "buildInputs": ["/nix/store/h4-typeddep"],
        "nativeBuildInputs": ["/nix/store/h5-typedtool"],
        "propagatedBuildInputs": [],
        "doCheck": false,
        "outputChecks": { "out": { "disallowedReferences": [] } }
      },
      "outputs": { "out": { "path": "h3-typedconsumer" } }
    },
    "d-typeddep.drv": {
      "name": "waybill-fixture-typeddep-2.0",
      "env": { "pname": "waybill-fixture-typeddep", "version": "2.0" },
      "outputs": { "out": { "path": "h4-typeddep" } }
    },
    "d-typedtool.drv": {
      "name": "waybill-fixture-typedtool-2.0",
      "env": { "pname": "waybill-fixture-typedtool", "version": "2.0" },
      "outputs": { "out": { "path": "h5-typedtool" } }
    },
    "d-emptylist.drv": {
      "name": "waybill-fixture-emptylist-1.0",
      "env": { "out": "/nix/store/h6-emptylist" },
      "structuredAttrs": {
        "pname": "waybill-fixture-emptylist",
        "version": "1.0",
        "buildInputs": []
      },
      "outputs": { "out": { "path": "h6-emptylist" } }
    },
    "d-unconsulted.drv": {
      "name": "waybill-fixture-unconsulted-1.0",
      "env": { "out": "/nix/store/h7-unconsulted" },
      "structuredAttrs": {
        "pname": "waybill-fixture-unconsulted",
        "version": "1.0",
        "defaultNativeBuildInputs": ["/nix/store/h8-unreachable"]
      },
      "outputs": { "out": { "path": "h7-unconsulted" } }
    },
    "d-unreachable.drv": {
      "name": "waybill-fixture-unreachable-1.0",
      "env": { "pname": "waybill-fixture-unreachable", "version": "1.0" },
      "outputs": { "out": { "path": "h8-unreachable" } }
    },
    "d-typedidentity.drv": {
      "name": "fallback-name-not-the-pname",
      "env": { "out": "/nix/store/ha-typedidentity" },
      "structuredAttrs": {
        "pname": "waybill-fixture-typedidentity",
        "version": "7.0"
      },
      "outputs": { "out": { "path": "ha-typedidentity" } }
    },
    "d-typedpatched.drv": {
      "name": "waybill-fixture-typedpatched-3.0",
      "env": { "out": "/nix/store/h9-typedpatched" },
      "structuredAttrs": {
        "pname": "waybill-fixture-typedpatched",
        "version": "3.0",
        "patches": ["/nix/store/hp-CVE-2026-9999-1.patch", "/nix/store/hp-tidy.patch"]
      },
      "outputs": { "out": { "path": "h9-typedpatched" } }
    }
  },
  "version": 4
}"#;

/// The same closure with every typed declaration moved to the flat carrier.
///
/// The control for the matched pair: if the reader is right, these two
/// closures must produce the same roles for the corresponding members. Without
/// it, "the typed member got a role" could be true for the wrong reason.
const CLOSURE_JSON_ALL_FLAT: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-roleproj-0.1.0",
      "env": {
        "pname": "waybill-fixture-roleproj",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-flatdep",
        "nativeBuildInputs": "/nix/store/h2-flattool"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-flatdep.drv": {
      "name": "waybill-fixture-flatdep-1.0",
      "env": { "pname": "waybill-fixture-flatdep", "version": "1.0" },
      "outputs": { "out": { "path": "h1-flatdep" } }
    },
    "d-flattool.drv": {
      "name": "waybill-fixture-flattool-1.0",
      "env": { "pname": "waybill-fixture-flattool", "version": "1.0" },
      "outputs": { "out": { "path": "h2-flattool" } }
    },
    "d-typedconsumer.drv": {
      "name": "waybill-fixture-typedconsumer-1.0",
      "env": {
        "pname": "waybill-fixture-typedconsumer",
        "version": "1.0",
        "buildInputs": "/nix/store/h4-typeddep",
        "nativeBuildInputs": "/nix/store/h5-typedtool"
      },
      "outputs": { "out": { "path": "h3-typedconsumer" } }
    },
    "d-typeddep.drv": {
      "name": "waybill-fixture-typeddep-2.0",
      "env": { "pname": "waybill-fixture-typeddep", "version": "2.0" },
      "outputs": { "out": { "path": "h4-typeddep" } }
    },
    "d-typedtool.drv": {
      "name": "waybill-fixture-typedtool-2.0",
      "env": { "pname": "waybill-fixture-typedtool", "version": "2.0" },
      "outputs": { "out": { "path": "h5-typedtool" } }
    }
  },
  "version": 3
}"#;

/// A closure in which every declaration is in a field classification reads.
///
/// Drives the absent half of the FR-009 gating matrix. Version 3 and flat
/// throughout, i.e. the world before structured attributes.
const CLOSURE_JSON_NO_GAP: &str = CLOSURE_JSON_ALL_FLAT;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// The m1277 fixture project is reused deliberately: it already pins the
/// nixpkgs revision the cache is seeded for, and a second near-identical flake
/// would be one more thing to keep in step for no new coverage.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/closure_role_scope")
        .join(name)
}

fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).unwrap();
    // Empty is enough: this milestone asserts nothing about Hackage identity,
    // and an absent cache would make the pass degrade for an unrelated reason.
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

    /// The emitted role of a component, by name. `None` when the component is
    /// absent — which this feature can change, so the distinction matters.
    fn role(&self, name: &str) -> Option<&str> {
        self.prop(name, "waybill:closure-role")
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

    fn doc_prop(&self, name: &str) -> Option<&str> {
        self.cdx["metadata"]["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some(name))?
            .get("value")?
            .as_str()
    }

    /// The C204 disclosure, parsed.
    fn read_outcome(&self) -> Option<Value> {
        serde_json::from_str(self.doc_prop("waybill:nix-closure-read")?).ok()
    }

    /// Every `aggregate` value in the CDX `compositions` array that makes a
    /// claim about dependency relationships.
    fn dependency_aggregates(&self) -> Vec<&str> {
        self.cdx["compositions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|c| c.get("dependencies").is_some())
                    .filter_map(|c| c["aggregate"].as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn spdx23_annotation(&self, name: &str) -> Option<String> {
        annotation_value(&self.spdx23["annotations"], name)
    }

    fn spdx3_annotation(&self, name: &str) -> Option<String> {
        let graph = self.spdx3["@graph"].as_array()?;
        for el in graph {
            if el["type"].as_str() != Some("Annotation") {
                continue;
            }
            if let Some(v) = envelope_value(el["statement"].as_str()?, name) {
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

/// Pull the value out of a `waybill-annotation/v1` envelope, if it carries
/// the named field.
///
/// The envelope is one field per annotation —
/// `{"schema":"waybill-annotation/v1","field":"<name>","value":"<value>"}` —
/// verified against the committed goldens rather than assumed. The first
/// version of this helper looked for a `values` map and found nothing, which
/// read as "SPDX 2.3 does not emit the annotation" when the annotation was
/// there all along. A helper that cannot find a thing looks exactly like a
/// thing that is not emitted.
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

fn scan_closure(closure: &str) -> Scan {
    let dir = tempfile::tempdir().unwrap();
    let cache = seed_cache();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let nix = bin.join("nix");
    std::fs::write(&nix, stub_script(&dir.path().join("nix.log"), closure)).unwrap();
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
            fixture("project").to_str().unwrap(),
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
// User Story 1 — the role reflects every way nix references a member
// ---------------------------------------------------------------------

/// T018 — the control every assertion below depends on.
///
/// A scan that classified nothing would satisfy every "is not wrongly
/// classified" assertion vacuously, so prove first that the closure was read
/// and that something was referenced.
#[test]
fn m1278_the_closure_was_read_and_something_was_referenced() {
    let s = scan_closure(CLOSURE_JSON);
    let summary = s.doc_prop("waybill:nix-closure").expect("C184 absent — the closure tier degraded, so nothing below is evidence");
    let v: Value = serde_json::from_str(summary).unwrap();
    assert_eq!(v["derivations"].as_u64(), Some(11), "{summary}");
    let referenced = v["roles"]["artifact-input"].as_u64().unwrap_or(0)
        + v["roles"]["build-tooling"].as_u64().unwrap_or(0)
        + v["roles"]["both"].as_u64().unwrap_or(0);
    assert!(
        referenced > 0,
        "nothing was referenced at all — the store-path join is broken, which \
         is silent rather than an error: {summary}"
    );
}

/// T012 — the matched pair, asserted on the role.
#[test]
fn m1278_a_typed_declaration_earns_the_same_role_a_flat_one_would() {
    let typed = scan_closure(CLOSURE_JSON);
    let flat = scan_closure(CLOSURE_JSON_ALL_FLAT);

    assert_eq!(
        flat.role("waybill-fixture-typeddep"),
        Some("artifact-input"),
        "the all-flat control must classify the member, or the comparison \
         below has no baseline"
    );
    assert_eq!(
        typed.role("waybill-fixture-typeddep"),
        Some("artifact-input"),
        "a member referenced through `structuredAttrs.buildInputs` must be an \
         artifact input, exactly as the flat carrier makes it. Before #1348 \
         this read `unreferenced`, and `unreferenced` is what a member looks \
         like when nothing depends on it"
    );
    assert_eq!(
        typed.role("waybill-fixture-typedtool"),
        Some("build-tooling"),
        "tooling fields resolve from the typed carrier too — otherwise the \
         artifact/tooling split silently collapses to artifact-only"
    );
    assert_eq!(
        typed.role("waybill-fixture-flatdep"),
        Some("artifact-input"),
        "the flat carrier is unchanged by this feature"
    );
}

/// T014 — an empty typed list is not a reference.
#[test]
fn m1278_an_empty_typed_list_references_nothing() {
    let s = scan_closure(CLOSURE_JSON);
    // `emptylist` declares `buildInputs: []`, so nothing may be credited to
    // it. Measured: that shape is on all 56 `source` derivations of the real
    // closure, and crediting it would manufacture edges that read as a better
    // census.
    assert_eq!(
        s.role("waybill-fixture-emptylist"),
        None,
        "an unreferenced member with no patch is not emitted, so an empty \
         list must not have promoted it into the document"
    );
}

/// T015 — shapes that are not string lists must not panic or resolve.
#[test]
fn m1278_a_typed_value_that_is_not_a_string_list_is_harmless() {
    // `typedconsumer` carries `doCheck: false` and a nested `outputChecks`
    // map beside its dependency arrays. If either resolved, or panicked, the
    // scan would not have produced a document at all — which the control in
    // `scan_closure` already asserts — so the surviving claim is that the
    // dependency arrays beside them still resolved correctly.
    let s = scan_closure(CLOSURE_JSON);
    assert_eq!(s.role("waybill-fixture-typeddep"), Some("artifact-input"));
}

/// T017 — an unrecognized format version changes nothing (FR-013).
#[test]
fn m1278_an_unknown_format_version_resolves_exactly_as_a_known_one() {
    let bumped = CLOSURE_JSON.replace("\"version\": 4", "\"version\": 99");
    let s = scan_closure(&bumped);
    assert_eq!(
        s.role("waybill-fixture-typeddep"),
        Some("artifact-input"),
        "a version list is an allowlist; resolution must not consult it"
    );
    let outcome = s.read_outcome().expect("C204 absent");
    assert_eq!(
        outcome["format-version"].as_u64(),
        Some(99),
        "the version is recorded even when unrecognized — that is the point \
         of recording it"
    );
}

// ---------------------------------------------------------------------
// User Story 2 — patch evidence is complete
// ---------------------------------------------------------------------

/// T022 + T023 — patches declared in the typed carrier contribute.
#[test]
fn m1278_typed_patches_contribute_evidence_and_their_cve() {
    let s = scan_closure(CLOSURE_JSON);
    let summary = s.doc_prop("waybill:nix-closure").unwrap();
    let v: Value = serde_json::from_str(summary).unwrap();
    assert_eq!(
        v["patches"].as_u64(),
        Some(2),
        "two patches are declared, both in `structuredAttrs.patches`: {summary}"
    );
    assert_eq!(
        v["distinct-cves"].as_u64(),
        Some(1),
        "the CVE-naming filename must reach the distinct-CVE record, or the \
         one claim the closure tier makes about remediation under-reports: \
         {summary}"
    );
    assert_eq!(
        v["patches-without-cve"].as_u64(),
        Some(1),
        "and the no-CVE count must still be reported, or the CVE count reads \
         as coverage: {summary}"
    );
}

// ---------------------------------------------------------------------
// User Story 3 — a typed-only identity still becomes a component
// ---------------------------------------------------------------------

/// T026 — an identity declared only in the typed carrier is emitted.
///
/// The member has to be *referenced* for this to be observable at all: an
/// `unreferenced` member that applies no patch is dropped by design, so a
/// typed-only identity in that role would be absent for a reason that has
/// nothing to do with its name. The first version of this test asserted on
/// exactly such a member and failed for the wrong reason.
///
/// `typedidentity` carries a derivation `name` that is deliberately NOT its
/// `pname`, so the assertion distinguishes "read the typed `pname`" from
/// "fell back to the derivation name".
#[test]
fn m1278_an_identity_declared_only_typed_becomes_a_component() {
    let s = scan_closure(CLOSURE_JSON);
    assert_eq!(
        s.role("waybill-fixture-typedidentity"),
        Some("artifact-input"),
        "a member whose `pname` lives only in `structuredAttrs` must be \
         emitted under that name — before #1348 it had no readable name and \
         was skipped entirely, which leaves no trace to notice"
    );
    assert_eq!(
        s.role("fallback-name-not-the-pname"),
        None,
        "and it must be emitted under the typed `pname`, not under the \
         derivation's own `name` — otherwise the fallback is masquerading as \
         a fix"
    );
}

// ---------------------------------------------------------------------
// User Story 4 — the next shape change is loud, not silent
// ---------------------------------------------------------------------

/// T031 — the gating matrix. Absence must mean "nothing skipped".
#[test]
fn m1278_the_disclosure_is_absent_when_everything_was_consulted() {
    let s = scan_closure(CLOSURE_JSON_NO_GAP);
    assert_eq!(
        s.read_outcome(),
        None,
        "a closure whose every declaration was consulted must carry no \
         disclosure — otherwise its presence stops meaning anything and \
         absence stops meaning 'nothing was skipped'"
    );
    assert!(
        s.doc_prop("waybill:nix-closure").is_some(),
        "CONTROL: the closure was still read, so the absence above is the \
         gating and not a degraded tier"
    );
}

/// T031 — and present, with the measured detail, when something was skipped.
#[test]
fn m1278_the_disclosure_names_the_field_it_could_not_consult() {
    let s = scan_closure(CLOSURE_JSON);
    let outcome = s.read_outcome().expect("C204 absent despite an unconsulted declaration");
    assert_eq!(
        outcome["fields-unconsulted"].as_array().unwrap(),
        &vec![Value::String("defaultNativeBuildInputs".into())],
        "the field is named by shape, not by an allowlist: {outcome}"
    );
    assert_eq!(outcome["declarations-unconsulted"].as_u64(), Some(1), "{outcome}");
    assert_eq!(outcome["derivations-affected"].as_u64(), Some(1), "{outcome}");
    assert_eq!(outcome["format-version"].as_u64(), Some(4), "{outcome}");
}

/// T038 / SC-014 — the diagnostic observes, it does not classify.
#[test]
fn m1278_an_unconsulted_field_does_not_change_a_members_role() {
    let s = scan_closure(CLOSURE_JSON);
    // `unreachable` is named ONLY by `defaultNativeBuildInputs`, a field classification
    // does not read. Reporting it must not promote it.
    assert_eq!(
        s.role("waybill-fixture-unreachable"),
        None,
        "reporting a field must not credit it — if this fires, the census is \
         no longer attributable to the attribute-shape fix alone and SC-004 \
         is unmeasurable"
    );
}

/// T051 — SC-012 / FR-012, asserted rather than inferred.
#[test]
fn m1278_the_disclosure_records_the_format_version_it_read() {
    let s = scan_closure(CLOSURE_JSON);
    let outcome = s.read_outcome().unwrap();
    assert_eq!(
        outcome["format-version"].as_u64(),
        Some(4),
        "which output shape a document was built from must be a fact in the \
         document: this milestone's own committed census arrived mislabelled \
         for want of exactly this field"
    );
}

/// T030 / SC-015 — the native claim this milestone exists to retract.
///
/// **Expected to fail until the demotion lands.** The committed
/// `nix-closure-moat` golden carries
/// `{"aggregate": "complete", "dependencies": [...]}` — CycloneDX's own way of
/// saying "no further dependencies are known to exist" — over a closure that
/// skipped 554 declarations. A test for a defect that exists today, written
/// after the fix, would prove nothing.
#[test]
fn m1278_a_partially_read_closure_makes_no_native_completeness_claim() {
    let s = scan_closure(CLOSURE_JSON);
    assert!(
        s.read_outcome().is_some(),
        "CONTROL: this closure really does have an unconsulted declaration, \
         so the assertion below is about the demotion and not about a closure \
         that happened to be complete"
    );
    let aggregates = s.dependency_aggregates();
    assert!(
        !aggregates.contains(&"complete"),
        "a document that skipped a declaration must not claim its dependency \
         relationships are complete. CycloneDX defines `complete` as 'no \
         further relationships ... are known to exist', and a consumer that \
         has never heard of waybill reads that field and believes it. \
         Aggregates found: {aggregates:?}"
    );
}

// ---------------------------------------------------------------------
// Cross-format parity
// ---------------------------------------------------------------------

/// SC-011 — the same payload in all three formats.
#[test]
fn m1278_the_disclosure_is_identical_in_all_three_formats() {
    let s = scan_closure(CLOSURE_JSON);
    let cdx = s.doc_prop("waybill:nix-closure-read").expect("absent from CDX");
    let spdx23 = s.spdx23_annotation("waybill:nix-closure-read").expect("absent from SPDX 2.3");
    let spdx3 = s.spdx3_annotation("waybill:nix-closure-read").expect("absent from SPDX 3");
    assert_eq!(cdx, spdx23, "CDX and SPDX 2.3 disagree");
    assert_eq!(cdx, spdx3, "CDX and SPDX 3 disagree");
    // SPDX 2.3 has no native completeness field, so there the annotation's
    // PRESENCE is the fact. Nothing is added to its payload to compensate:
    // an unequal payload would cost exactly the check above.
}
