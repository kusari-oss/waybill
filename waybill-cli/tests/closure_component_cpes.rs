//! Milestone 1367 (#1371) — CPE candidates for closure components.
//!
//! The synthesis pass had never seen a nix-closure component: it runs inside
//! `package_db::read_all`, and the closure's components are merged in later.
//! Six that match the curated table exactly by name — `openssl 3.6.2`,
//! `curl 8.20.0`, `zlib 1.3.2`, `sqlite 3.51.2`, `pcre2 10.46`,
//! `llvm 21.1.8` — carried nothing.
//!
//! **Zero CPEs is also what "this closure has no mapped software" looks
//! like.** Nothing failed, warned or logged for a whole milestone. So every
//! test here that asserts a CPE exists is preceded by a control proving the
//! population is non-empty; without one, a green run over a degraded closure
//! says nothing at all.
//!
//! Two requirements are **not** testable here and live in
//! `scan_cmd::cpe_coverage_tests` instead, because the pinned closure cannot
//! produce the input: no component on it both receives a `--supplement-cdx`
//! value and matches the curated table (FR-004), and no mapped component
//! carries a non-release version (FR-003).
//!
//! The fixture is JSON handed to a stub `nix`. **Never evaluated by a real
//! one**: on 2026-11-04 that rewrote a committed `flake.lock` and broke 11
//! tests in an unrelated file.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";
const C209: &str = "waybill:cpe-coverage";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

/// A closure holding software the curated table maps, software it does not,
/// and a member that must stay without a CPE.
///
/// `waybill-fixture-mapped` is named `zlib` deliberately: the curated table is
/// keyed on the component name, so a synthetic name cannot exercise the
/// mapping at all. The *version* is synthetic, so nothing here asserts
/// anything about a real zlib release.
const CLOSURE_JSON: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-cpeproj-0.1.0",
      "env": {
        "pname": "waybill-fixture-cpeproj",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-zlib /nix/store/h2-unmapped /nix/store/h3-src"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-zlib.drv": {
      "name": "zlib-9.9.9",
      "env": { "pname": "zlib", "version": "9.9.9" },
      "outputs": { "out": { "path": "h1-zlib" } }
    },
    "d-unmapped.drv": {
      "name": "waybill-fixture-unmapped-2.0",
      "env": { "pname": "waybill-fixture-unmapped", "version": "2.0" },
      "outputs": { "out": { "path": "h2-unmapped" } }
    },
    "d-src.drv": {
      "name": "zlib-9.9.9.tar.gz",
      "env": { "urls": "https://example.invalid/waybill-fixture-zlib.tar.gz" },
      "outputs": {
        "out": {
          "path": "h3-src",
          "hash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        }
      }
    }
  },
  "version": 4
}"#;

/// A closure with nothing the synthesizer could ever map, for the FR-008
/// contrast.
const CLOSURE_NOTHING_ELIGIBLE: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-bare-0.1.0",
      "env": {
        "pname": "waybill-fixture-bare",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-hook"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-hook.drv": {
      "name": "waybill-fixture-hook",
      "env": { "pname": "waybill-fixture-hook", "version": "26.05pre-git" },
      "outputs": { "out": { "path": "h1-hook" } }
    }
  },
  "version": 4
}"#;

fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).unwrap();
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
    fn doc_prop(&self, name: &str) -> Option<String> {
        self.cdx["metadata"]["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some(name))
            .and_then(|p| p["value"].as_str())
            .map(str::to_string)
    }

    fn spdx23_annotation(&self, name: &str) -> Option<String> {
        for a in self.spdx23["annotations"].as_array()? {
            if let Some(v) = envelope_value(a["comment"].as_str()?, name) {
                return Some(v);
            }
        }
        None
    }

    fn spdx3_annotation(&self, name: &str) -> Option<String> {
        for el in self.spdx3["@graph"].as_array()? {
            if el["type"].as_str() != Some("Annotation") {
                continue;
            }
            if let Some(v) = envelope_value(el["statement"].as_str()?, name) {
                return Some(v);
            }
        }
        None
    }

    fn component(&self, name: &str) -> Option<&Value> {
        self.cdx["components"]
            .as_array()?
            .iter()
            .find(|c| c["name"].as_str() == Some(name))
    }

    /// Every component carrying a CPE, as `(name, cpe)`.
    fn cpes(&self) -> Vec<(String, String)> {
        self.cdx["components"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        Some((
                            c["name"].as_str()?.to_string(),
                            c["cpe"].as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// SPDX 2.3 `externalRefs` of type `cpe23Type`, by package name.
    fn spdx23_cpe_refs(&self) -> Vec<String> {
        self.spdx23["packages"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p["externalRefs"].as_array())
                    .flatten()
                    .filter(|r| r["referenceType"].as_str() == Some("cpe23Type"))
                    .filter_map(|r| r["referenceLocator"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn spdx3_cpe_ids(&self) -> Vec<String> {
        serde_json::to_string(&self.spdx3)
            .unwrap_or_default()
            .match_indices("cpe:2.3:")
            .map(|(i, _)| i.to_string())
            .collect()
    }
}

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
            project().to_str().unwrap(),
            "--no-deep-hash",
            "--nix-closure",
            "--no-deps-dev",
            "--no-clearly-defined",
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
// Control
// ---------------------------------------------------------------------

/// **The control every assertion below depends on.**
///
/// A scan whose closure tier degraded emits no closure components at all, and
/// would satisfy "nothing has a wrong CPE" vacuously while also satisfying
/// "openssl has a CPE" never. Prove the population exists first.
#[test]
fn m1367_the_closure_was_read_and_holds_mappable_software() {
    let s = scan_closure(CLOSURE_JSON);
    let summary = s
        .doc_prop("waybill:nix-closure")
        .expect("C184 absent — the closure tier degraded, so nothing below is evidence");
    let v: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(v["derivations"].as_u64(), Some(4), "{summary}");
    assert!(
        s.component("zlib").is_some(),
        "the mappable component must be emitted, or every CPE assertion below \
         passes over an empty set"
    );
}

// ---------------------------------------------------------------------
// US1 — a consumer matches a closure's C libraries against NVD
// ---------------------------------------------------------------------

/// FR-001, FR-006 — the candidate reaches all three formats.
#[test]
fn m1367_a_mapped_closure_component_carries_a_cpe_in_every_format() {
    let s = scan_closure(CLOSURE_JSON);

    let cdx = s.component("zlib").and_then(|c| c["cpe"].as_str());
    assert_eq!(
        cdx,
        Some("cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:*"),
        "CycloneDX `component.cpe`; got {cdx:?}"
    );

    let refs = s.spdx23_cpe_refs();
    assert!(
        refs.iter().any(|r| r.contains("zlib:9.9.9")),
        "SPDX 2.3 cpe23Type externalRef; got {refs:?}"
    );
    assert!(
        !s.spdx3_cpe_ids().is_empty(),
        "SPDX 3 must carry at least one CPE external identifier"
    );
}

/// The PURL is untouched — a CPE rides alongside, never instead.
#[test]
fn m1367_the_purl_is_unchanged() {
    let s = scan_closure(CLOSURE_JSON);
    let c = s.component("zlib").unwrap();
    assert_eq!(
        c["purl"].as_str(),
        Some("pkg:generic/zlib@9.9.9"),
        "a consumer matching on the PURL must see no difference"
    );
}

// ---------------------------------------------------------------------
// US2 — a component that is not software does not acquire a CPE
// ---------------------------------------------------------------------

/// FR-002 — the source tarball is named after the library and is not it.
#[test]
fn m1367_a_closure_member_named_after_a_library_gets_no_cpe() {
    let s = scan_closure(CLOSURE_JSON);
    let members: Vec<&Value> = s.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["properties"].as_array().is_some_and(|ps| {
                ps.iter()
                    .any(|p| p["name"].as_str() == Some("waybill:closure-member-role"))
            })
        })
        .collect();
    assert!(!members.is_empty(), "the fixture must contain a member");
    assert!(
        members.iter().all(|c| c["cpe"].is_null()),
        "a tarball named `zlib-9.9.9.tar.gz` is not zlib: {:?}",
        members.iter().map(|c| &c["name"]).collect::<Vec<_>>()
    );
}

/// FR-007 — an unmapped name is silent.
#[test]
fn m1367_an_unmapped_component_gets_no_cpe() {
    let s = scan_closure(CLOSURE_JSON);
    let c = s.component("waybill-fixture-unmapped").unwrap();
    assert!(c["cpe"].is_null(), "got {:?}", c["cpe"]);
}

// ---------------------------------------------------------------------
// FR-008 — the record tells the three states apart
// ---------------------------------------------------------------------

/// The whole of FR-008, as a pair.
///
/// These two documents were **identical** before this milestone — zero CPEs
/// each — which is how the defect survived. They must not be identical now.
#[test]
fn m1367_mapped_and_nothing_eligible_are_different_documents() {
    let mapped = scan_closure(CLOSURE_JSON);
    let bare = scan_closure(CLOSURE_NOTHING_ELIGIBLE);

    let a: Value = serde_json::from_str(&mapped.doc_prop(C209).expect("C209 absent")).unwrap();
    let b: Value = serde_json::from_str(&bare.doc_prop(C209).expect("C209 absent")).unwrap();

    assert!(a["with_candidates"].as_u64().unwrap() >= 1, "{a}");
    // Milestone 1373 (#1375): `eligible_unmapped` is gone. It was wrong in
    // both directions -- it counted `pkg:hackage` components OSV already
    // addresses, and its release-shape gate excluded most `pkg:generic`
    // components, which are the ones no feed can reach.
    assert!(a["unaddressable"].as_u64().unwrap() >= 1, "{a}");
    assert!(a["eligible_unmapped"].is_null(), "the v1 field must be gone: {a}");

    // The distinguishing fact is `with_candidates`, not `eligible_unmapped`.
    //
    // An earlier version of this test asserted `eligible_unmapped == 0` for
    // the bare closure, on the assumption that everything in it carries a
    // nixpkgs pre-release version. That was wrong: the scanned project
    // contributes its own components, and the closure root
    // `waybill-fixture-bare 0.1.0` has a perfectly release-shaped version —
    // it is eligible, and simply has no mapping row. The code was right and
    // the assertion was over-specified.
    assert_eq!(
        b["with_candidates"].as_u64(),
        Some(0),
        "nothing in the bare closure is mappable: {b}"
    );

    assert_ne!(
        a, b,
        "these two documents were IDENTICAL before this milestone — zero CPEs          each — which is how the defect survived one"
    );
}

/// C209 is present on every scan, including one that found nothing.
#[test]
fn m1367_the_record_is_present_even_when_nothing_was_mapped() {
    let s = scan_closure(CLOSURE_NOTHING_ELIGIBLE);
    let v = s.doc_prop(C209).expect("C209 must be present on every scan");
    assert!(v.contains("waybill-cpe-coverage/v3"), "{v}");
}

/// Cross-format parity for the record.
#[test]
fn m1367_the_record_is_identical_in_all_three_formats() {
    let s = scan_closure(CLOSURE_JSON);
    let cdx = s.doc_prop(C209).expect("absent from CDX");
    let spdx23 = s.spdx23_annotation(C209).expect("absent from SPDX 2.3");
    let spdx3 = s.spdx3_annotation(C209).expect("absent from SPDX 3");
    assert_eq!(cdx, spdx23, "CDX and SPDX 2.3 disagree");
    assert_eq!(cdx, spdx3, "CDX and SPDX 3 disagree");
}

/// FR-005 — no component carries the same CPE twice.
#[test]
fn m1367_no_cpe_value_is_repeated_on_a_component() {
    let s = scan_closure(CLOSURE_JSON);
    for c in s.cdx["components"].as_array().unwrap() {
        let Some(props) = c["properties"].as_array() else {
            continue;
        };
        let Some(raw) = props
            .iter()
            .find(|p| p["name"].as_str() == Some("waybill:cpe-candidates"))
            .and_then(|p| p["value"].as_str())
        else {
            continue;
        };
        let list: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        for v in &list {
            assert!(seen.insert(v.clone()), "duplicate {v} on {:?}", c["name"]);
        }
    }
    // CONTROL: at least one component carried candidates at all.
    assert!(!s.cpes().is_empty(), "no component carried a CPE");
}

/// FR-003 / SC-003 — read off the emitted document, not off the guard.
///
/// The unit test covers the predicate; this covers the **property of the
/// document**, which is what SC-003 actually asserts. A guard that is correct
/// and never called would satisfy the former and not the latter.
#[test]
fn m1367_every_emitted_cpe_carries_a_release_version() {
    let s = scan_closure(CLOSURE_JSON);
    let cpes = s.cpes();
    // CONTROL: something carried a CPE, or this passes over an empty set.
    assert!(!cpes.is_empty(), "no component carried a CPE");

    for c in s.cdx["components"].as_array().unwrap() {
        if c["cpe"].is_null() {
            continue;
        }
        let version = c["version"].as_str().unwrap_or_default();
        let first = version.chars().next();
        assert!(
            first.is_some_and(|ch| ch.is_ascii_digit()),
            "{} carries a CPE with a non-release version {version:?}",
            c["name"]
        );
        assert!(
            !version.contains('+') && !version.contains("unstable") && !version.contains("pre-"),
            "{} carries a CPE with a nix-shaped version {version:?}",
            c["name"]
        );
    }
}

// ---------------------------------------------------------------------
// #1395 — the CPython interpreter a Python application ships with
// ---------------------------------------------------------------------

/// A Python application's closure, shaped like nixpkgs' at `a799d3e3`.
///
/// The interpreter's derivation is `python3-<v>` with pname `python3`; an
/// interpreter environment (`python3.withPackages`) is `python3-<v>-env`
/// with no pname at all; a Python package is `python3.13-<pkg>-<v>`; a
/// build hook is `python-<hook>`. Only the first is CPython.
///
/// `python3` is the real pname because the curated table is keyed on it; the
/// *version* is synthetic, so nothing here asserts anything about a real
/// CPython release.
const CLOSURE_PYTHON_APP: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-pyapp-0.1.0",
      "env": {
        "pname": "waybill-fixture-pyapp",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-python /nix/store/h2-env /nix/store/h3-pkg /nix/store/h4-hook"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-python.drv": {
      "name": "python3-9.9.9",
      "env": { "pname": "python3", "version": "9.9.9" },
      "outputs": { "out": { "path": "h1-python" } }
    },
    "d-env.drv": {
      "name": "python3-9.9.9-env",
      "env": { },
      "outputs": { "out": { "path": "h2-env" } }
    },
    "d-pkg.drv": {
      "name": "python3.13-waybill-fixture-pkg-2.0",
      "env": { "pname": "python3.13-waybill-fixture-pkg", "version": "2.0" },
      "outputs": { "out": { "path": "h3-pkg" } }
    },
    "d-hook.drv": {
      "name": "python-waybill-fixture-hook",
      "env": { "pname": "python-waybill-fixture-hook", "version": "26.05pre-git" },
      "outputs": { "out": { "path": "h4-hook" } }
    }
  },
  "version": 4
}"#;

/// The interpreter carries `python:python` — NVD's CPython vendor and
/// product — at its release version, in all three formats, recorded as
/// coming from the curated table.
#[test]
fn i1395_the_interpreter_in_a_closure_carries_the_cpython_cpe() {
    let s = scan_closure(CLOSURE_PYTHON_APP);
    // CONTROL: the closure was read, or the assertions below say nothing.
    assert!(
        s.doc_prop("waybill:nix-closure").is_some(),
        "C184 absent — the closure tier degraded, so nothing below is evidence"
    );

    let python = s
        .component("python3")
        .expect("the interpreter must be emitted as `python3`");
    assert_eq!(
        python["cpe"].as_str(),
        Some("cpe:2.3:a:python:python:9.9.9:*:*:*:*:*:*:*"),
        "CycloneDX `component.cpe`"
    );
    assert_eq!(python["purl"].as_str(), Some("pkg:generic/python3@9.9.9"));

    let sources: Vec<&str> = python["evidence"]["identity"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|i| i["field"].as_str() == Some("cpe"))
                .filter_map(|i| i["methods"][0]["value"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        sources,
        vec!["waybill curated CPE table"],
        "the CPE must be recorded as curated, not nixpkgs-declared"
    );

    let refs = s.spdx23_cpe_refs();
    assert!(
        refs.iter()
            .any(|r| r == "cpe:2.3:a:python:python:9.9.9:*:*:*:*:*:*:*"),
        "SPDX 2.3 cpe23Type externalRef; got {refs:?}"
    );
    assert!(
        serde_json::to_string(&s.spdx3)
            .unwrap()
            .contains("cpe:2.3:a:python:python:9.9.9:"),
        "SPDX 3 must carry the CPE"
    );
}

/// Nothing else in a Python application's closure becomes CPython: not the
/// interpreter environment, a Python package, a hook, nor the application.
#[test]
fn i1395_only_the_interpreter_gets_the_cpython_cpe() {
    let s = scan_closure(CLOSURE_PYTHON_APP);
    let cpes = s.cpes();
    // CONTROL: the interpreter carries it, so the population is not empty.
    assert!(
        cpes.iter().any(|(n, _)| n == "python3"),
        "the interpreter must carry a CPE, or this passes over an empty set: {cpes:?}"
    );
    let cpython: Vec<&(String, String)> = cpes
        .iter()
        .filter(|(_, c)| c.starts_with("cpe:2.3:a:python:python:"))
        .collect();
    assert_eq!(
        cpython.len(),
        1,
        "exactly one component is CPython; got {cpython:?}"
    );
    // The document subject is not in `components`; it must not be CPython
    // either.
    let subject = &s.cdx["metadata"]["component"];
    assert!(
        !subject["cpe"]
            .as_str()
            .is_some_and(|c| c.starts_with("cpe:2.3:a:python:python:")),
        "the subject {:?} got {:?}",
        subject["name"],
        subject["cpe"]
    );
}
