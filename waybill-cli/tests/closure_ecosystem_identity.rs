//! Milestone 1366 (#1365) — C208, the document-scope record of what the
//! closure's ecosystem identity pass did.
//!
//! This suite drives the real binary against a stub `nix` and asserts on the
//! **emitted documents**, the same way the milestone-1278 and -1282 closure
//! suites do.
//!
//! **It runs with `--no-deps-dev`, deliberately.** That is the path the one
//! corpus target which exercises a closure takes, and it is the only path a
//! hermetic test can take: confirmation would otherwise reach the live
//! registry, which would make this suite slow, flaky, and dependent on what
//! PyPI happens to hold. The confirming path has its own stubbed tests in
//! `scan_cmd::closure_ecosystem_identity_tests`, where a mock registry answers
//! 200 and 404 on demand — and without those, a green run of THIS file would
//! say nothing at all about whether confirmation works.
//!
//! The one exception is the #1442 section at the end, which needs a document
//! that actually carries C207. It enables deps.dev but answers every candidate
//! from a disk cache seeded in a temp directory, so the scan confirms without
//! reaching the network.
//!
//! The fixture is JSON handed to a stub `nix`. **It is never evaluated by a
//! real one**: on 2026-11-04 that rewrote a committed `flake.lock` and broke
//! 11 tests in an unrelated file.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The revision the fixture `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

const C208: &str = "waybill:closure-ecosystem-identity-outcome";
const C207: &str = "waybill:closure-ecosystem-identity";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

/// The milestone-1366 fixture: three PyPI candidates across both attribute
/// carriers, plus the setup hook, bootstrap variant and pname-less member the
/// rule must exclude.
fn closure_json() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_ecosystem/closure.json");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// A closure with no Python in it at all, for the FR-008 contrast.
const CLOSURE_WITHOUT_CANDIDATES: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-plain-0.1.0",
      "env": {
        "pname": "waybill-fixture-plain",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-dep"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-dep.drv": {
      "name": "waybill-fixture-dep-1.0",
      "env": { "pname": "waybill-fixture-dep", "version": "1.0" },
      "outputs": { "out": { "path": "h1-dep" } }
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

    /// Every component PURL in the CDX document.
    fn purls(&self) -> Vec<&str> {
        self.cdx["components"]
            .as_array()
            .map(|a| a.iter().filter_map(|c| c["purl"].as_str()).collect())
            .unwrap_or_default()
    }

    /// Does any component anywhere carry the C207 disclosure, in any format?
    fn any_c207(&self) -> bool {
        let in_cdx_property = self.cdx["components"].as_array().is_some_and(|a| {
            a.iter().any(|c| {
                c["properties"].as_array().is_some_and(|ps| {
                    ps.iter().any(|p| p["name"].as_str() == Some(C207))
                })
            })
        });
        // The native half of a derivation: a concluded PURL in
        // `evidence.identity[]`, which only the m1366 pass writes.
        let in_cdx_native = self.cdx["components"].as_array().is_some_and(|a| {
            a.iter().any(|c| {
                c["evidence"]["identity"].as_array().is_some_and(|ids| {
                    ids.iter().any(|i| {
                        i["field"].as_str() == Some("purl") && i.get("concludedValue").is_some()
                    })
                })
            })
        });
        // Exact field match, never a substring: `C207` is a strict PREFIX of
        // `C208`, so `contains(C207)` matches the document-scope record and
        // reports a disclosure that is not there. The first version of this
        // helper did exactly that and failed against correct code.
        let in_spdx23 = self.spdx23["annotations"].as_array().is_some_and(|a| {
            a.iter().any(|x| {
                x["comment"]
                    .as_str()
                    .and_then(|c| envelope_value(c, C207))
                    .is_some()
            })
        });
        let in_spdx3 = self.spdx3["@graph"].as_array().is_some_and(|g| {
            g.iter().any(|el| {
                el["statement"]
                    .as_str()
                    .and_then(|c| envelope_value(c, C207))
                    .is_some()
            })
        });
        in_cdx_property || in_cdx_native || in_spdx23 || in_spdx3
    }
}

/// Pull the value out of a `waybill-annotation/v1` envelope.
///
/// One field per annotation. The milestone-1278 suite records why this is
/// written against the goldens rather than guessed: a helper that looked for a
/// `values` map found nothing and read as "SPDX 2.3 does not emit the
/// annotation" while the annotation was there all along.
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
    scan_closure_with(closure, Confirmation::Disabled)
}

/// How a scan reaches confirmation.
enum Confirmation {
    /// `--no-deps-dev`: the corpus path, and this file's default.
    Disabled,
    /// deps.dev enabled, but answered entirely from a disk cache seeded in
    /// this directory, so the scan confirms without reaching the network.
    SeededCache(tempfile::TempDir),
}

/// A deps.dev disk cache that answers the fixture's three candidates: alpha
/// and gamma exist, nonesuch is a recorded absence.
///
/// The entry layout is the one `enrich::deps_dev_disk_cache` writes. It is
/// restated here because an integration test cannot reach that module, and a
/// wrong restatement cannot pass silently: a missed key falls through to the
/// live registry, which holds none of these synthetic names, so nothing
/// confirms and the CONTROL in every test that uses this seed goes red.
fn seeded_deps_dev_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for (name, version, present) in [
        ("waybill-fixture-alpha", "1.2.3", true),
        ("waybill-fixture-gamma", "0.4", true),
        ("waybill-fixture-nonesuch", "9.9.9", false),
    ] {
        let key = format!("pypi\u{1f}{name}\u{1f}{version}");
        let digest = Sha256::digest(key.as_bytes());
        let file: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
        let record = if present {
            serde_json::json!({ "licenses": [], "links": [] })
        } else {
            Value::Null
        };
        let entry = serde_json::json!({
            "v": 1,
            "key": key,
            "fetched_at": now,
            "max_age_secs": 86_400,
            "record": record,
        });
        std::fs::write(dir.path().join(format!("{file}.json")), entry.to_string()).unwrap();
    }
    dir
}

fn scan_closure_with(closure: &str, confirmation: Confirmation) -> Scan {
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
    let mut cmd = {
            // #1046: never reach Hackage from a test (Principle VII).
            let mut c = Command::new(binary_path());
            c.env("WAYBILL_HACKAGE_URL", "http://127.0.0.1:9");
            c
        };
    cmd.args([
        "sbom",
        "scan",
        "--path",
        project().to_str().unwrap(),
        "--no-deep-hash",
        "--nix-closure",
    ]);
    match &confirmation {
        // Hermetic, and the corpus path. See this file's header.
        Confirmation::Disabled => {
            cmd.arg("--no-deps-dev");
        }
        Confirmation::SeededCache(cache_dir) => {
            // Every request the cache does not answer goes to a proxy nothing
            // listens on. A miss therefore fails at the transport, leaves the
            // candidate unchecked and trips the CONTROL, instead of quietly
            // reaching the live registry.
            const DEAD_PROXY: &str = "http://127.0.0.1:1";
            cmd.env("WAYBILL_DEPS_DEV_CACHE_DIR", cache_dir.path())
                .env_remove("WAYBILL_DEPS_DEV_NO_CACHE")
                .env("HTTPS_PROXY", DEAD_PROXY)
                .env("HTTP_PROXY", DEAD_PROXY)
                .env("ALL_PROXY", DEAD_PROXY)
                .env_remove("NO_PROXY")
                .env_remove("no_proxy");
        }
    }
    let out = cmd
        .args([
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

/// The control every assertion below depends on.
///
/// A scan whose closure tier degraded emits no C208 at all, and would satisfy
/// "nothing was re-identified" vacuously. Prove the closure was read first.
#[test]
fn m1366_the_closure_was_read() {
    let s = scan_closure(&closure_json());
    let summary = s
        .doc_prop("waybill:nix-closure")
        .expect("C184 absent — the closure tier degraded, so nothing below is evidence");
    let v: Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(v["derivations"].as_u64(), Some(8), "{summary}");
}

// ---------------------------------------------------------------------
// FR-008 — "nothing to do" and "did nothing" are different documents
// ---------------------------------------------------------------------

/// The whole of FR-008, as a pair.
///
/// A boolean would make these two documents identical. They are not the same
/// statement: the first says this closure holds no Python packages, the second
/// says it holds three and the pass was told not to look. A reader who cannot
/// tell them apart cannot tell a clean result from a disabled one — which is
/// the shape of `feedback_zero_results_can_mean_broken`.
#[test]
fn m1366_a_closure_with_no_candidates_differs_from_a_pass_that_did_not_run() {
    let with = scan_closure(&closure_json());
    let without = scan_closure(CLOSURE_WITHOUT_CANDIDATES);

    let a: Value = serde_json::from_str(&with.doc_prop(C208).expect("C208 absent")).unwrap();
    let b: Value = serde_json::from_str(&without.doc_prop(C208).expect("C208 absent")).unwrap();

    assert_eq!(a["candidates"].as_u64(), Some(3), "{a}");
    assert_eq!(a["unchecked"].as_u64(), Some(3), "{a}");
    assert_eq!(a["reason"].as_str(), Some("disabled"), "{a}");

    assert_eq!(b["candidates"].as_u64(), Some(0), "{b}");
    assert_eq!(b["unchecked"].as_u64(), Some(0), "{b}");
    assert_eq!(b["reason"].as_str(), None, "a clean result carries no reason: {b}");

    assert_ne!(a, b);
}

/// The counts add up, in the emitted document rather than only in the struct.
#[test]
fn m1366_the_emitted_counts_are_consistent() {
    let s = scan_closure(&closure_json());
    let v: Value = serde_json::from_str(&s.doc_prop(C208).expect("C208 absent")).unwrap();
    let n = |k: &str| v[k].as_u64().unwrap_or_default();
    assert_eq!(
        n("confirmed") + n("declined") + n("unchecked"),
        n("candidates"),
        "{v}"
    );
    assert_eq!(v["schema"].as_str(), Some("waybill-closure-ecosystem-identity/v1"));
    assert_eq!(v["ecosystem"].as_str(), Some("pypi"));
}

// ---------------------------------------------------------------------
// Cross-format parity (C208)
// ---------------------------------------------------------------------

/// SC — the same payload in all three formats.
#[test]
fn m1366_the_record_is_identical_in_all_three_formats() {
    let s = scan_closure(&closure_json());
    let cdx = s.doc_prop(C208).expect("absent from CDX");
    let spdx23 = s.spdx23_annotation(C208).expect("absent from SPDX 2.3");
    let spdx3 = s.spdx3_annotation(C208).expect("absent from SPDX 3");
    assert_eq!(cdx, spdx23, "CDX and SPDX 2.3 disagree");
    assert_eq!(cdx, spdx3, "CDX and SPDX 3 disagree");
}

/// C208 is absent entirely when no closure was taken, so its presence alone
/// says the pass was reached.
#[test]
fn m1366_no_closure_means_no_record() {
    let dir = tempfile::tempdir().unwrap();
    let cdx_path = dir.path().join("out.cdx.json");
    let out = {
            // #1046: never reach Hackage from a test (Principle VII).
            let mut c = Command::new(binary_path());
            c.env("WAYBILL_HACKAGE_URL", "http://127.0.0.1:9");
            c
        }
        .args(["sbom", "scan", "--path", project().to_str().unwrap(), "--no-deep-hash"])
        .args(["--format", "cyclonedx-json", "--output"])
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .output()
        .unwrap();
    assert!(cdx_path.exists(), "{}", String::from_utf8_lossy(&out.stderr));
    let doc: Value = serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    let has = doc["metadata"]["properties"]
        .as_array()
        .is_some_and(|a| a.iter().any(|p| p["name"].as_str() == Some(C208)));
    assert!(!has, "a scan with no --nix-closure must carry no C208");
}

// ---------------------------------------------------------------------
// US2 — nothing is invented
// ---------------------------------------------------------------------

/// FR-002 and FR-005, on the emitted document.
///
/// With confirmation disabled nothing may be re-identified, and in particular
/// no component may acquire a `pkg:pypi` identifier or the C207 disclosure.
#[test]
fn m1366_with_confirmation_disabled_no_component_is_re_identified() {
    let s = scan_closure(&closure_json());

    // CONTROL: the candidates ARE present as components, so "no pkg:pypi"
    // below is a statement about identity rather than about an empty document.
    let purls = s.purls();
    assert!(
        purls
            .iter()
            .any(|p| p.contains("waybill-fixture-alpha")),
        "the candidate components must be emitted, or this test asserts \
         nothing; got {purls:?}"
    );

    assert!(
        !purls.iter().any(|p| p.starts_with("pkg:pypi/")),
        "nothing confirmed, so nothing may carry an ecosystem identifier: {purls:?}"
    );
    assert!(
        !s.any_c207(),
        "the disclosure must be absent wherever no identity was derived"
    );
}

// ---------------------------------------------------------------------
// #1442 — a document carrying C207 is schema-valid CycloneDX 1.6
// ---------------------------------------------------------------------

fn confirmed_scan() -> Scan {
    let s = scan_closure_with(
        &closure_json(),
        Confirmation::SeededCache(seeded_deps_dev_cache()),
    );
    // CONTROL. Every assertion in this section is about components that were
    // re-identified. If the seeded cache missed, nothing confirmed and a
    // schema check would pass over a document with no C207 in it — which is
    // exactly the document the existing schema tests already validate, and
    // how #1442 shipped.
    let purls = s.purls();
    for want in [
        "pkg:pypi/waybill-fixture-alpha@1.2.3",
        "pkg:pypi/waybill-fixture-gamma@0.4",
    ] {
        assert!(
            purls.contains(&want),
            "{want} absent, so confirmation did not run and nothing below is \
             evidence; got {purls:?}"
        );
    }
    assert!(
        purls.contains(&"pkg:generic/waybill-fixture-nonesuch@9.9.9"),
        "the recorded absence must decline nonesuch; got {purls:?}"
    );
    s
}

/// The CDX component carrying `purl`, wherever it nests.
fn cdx_component<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    fn walk<'a>(cs: &'a Value, purl: &str) -> Option<&'a Value> {
        for c in cs.as_array()? {
            if c["purl"].as_str() == Some(purl) {
                return Some(c);
            }
            if let Some(found) = walk(&c["components"], purl) {
                return Some(found);
            }
        }
        None
    }
    walk(&doc["components"], purl)
}

fn cdx_property(component: &Value, name: &str) -> Option<String> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// #1442. Before the fix the C207 method object carried a key CycloneDX 1.6
/// does not allow, and every confirmed component made the document invalid.
#[test]
fn m1442_a_closure_scan_carrying_c207_is_schema_valid_cyclonedx() {
    let s = confirmed_scan();
    let errors = common::cdx_schema::cdx_validation_errors(&s.cdx);
    assert!(
        errors.is_empty(),
        "{} CycloneDX 1.6 schema error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

/// #1442. Moving the carrier must not lose the value: C207 is on exactly the
/// confirmed components, in CDX as in both SPDX formats.
#[test]
fn m1442_c207_is_still_carried_on_each_confirmed_component() {
    let s = confirmed_scan();
    for purl in [
        "pkg:pypi/waybill-fixture-alpha@1.2.3",
        "pkg:pypi/waybill-fixture-gamma@0.4",
    ] {
        let c = cdx_component(&s.cdx, purl).unwrap_or_else(|| panic!("{purl} not in CDX"));
        assert_eq!(
            cdx_property(c, C207).as_deref(),
            Some("versioned-interpreter-prefix"),
            "{purl} lost its C207 disclosure: {c}"
        );
        // The derived identifier stays recorded natively.
        let concluded: Vec<&str> = c["evidence"]["identity"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|i| i["field"].as_str() == Some("purl"))
                    .filter_map(|i| i["concludedValue"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(concluded, vec![purl], "{c}");
    }
    let declined = cdx_component(&s.cdx, "pkg:generic/waybill-fixture-nonesuch@9.9.9")
        .expect("nonesuch not in CDX");
    assert_eq!(cdx_property(declined, C207), None, "{declined}");

    let spdx23 = s.spdx23["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            s.spdx23["packages"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|p| p["annotations"].as_array().into_iter().flatten()),
        )
        .filter(|a| {
            a["comment"]
                .as_str()
                .and_then(|c| envelope_value(c, C207))
                .is_some()
        })
        .count();
    let spdx3 = s.spdx3["@graph"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|el| {
            el["statement"]
                .as_str()
                .and_then(|c| envelope_value(c, C207))
                .is_some()
        })
        .count();
    assert_eq!((spdx23, spdx3), (2, 2), "SPDX 2.3 / SPDX 3 C207 counts");
}

/// #1442. The project's own parity check reads C207 from the new CDX carrier
/// and finds it equal to both SPDX formats.
#[test]
fn m1442_parity_check_reports_c207_present_in_every_format() {
    let s = confirmed_scan();
    let dir = tempfile::tempdir().unwrap();
    for (f, v) in [
        ("waybill.cdx.json", &s.cdx),
        ("waybill.spdx.json", &s.spdx23),
        ("waybill.spdx3.json", &s.spdx3),
    ] {
        std::fs::write(dir.path().join(f), v.to_string()).unwrap();
    }
    let out = {
            // #1046: never reach Hackage from a test (Principle VII).
            let mut c = Command::new(binary_path());
            c.env("WAYBILL_HACKAGE_URL", "http://127.0.0.1:9");
            c
        }
        .args(["sbom", "parity-check", "--format", "json", "--scan-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let row = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["row_id"] == "C207")
        .expect("a C207 row");
    // A set of values: both confirmed components carry the one value.
    for f in ["cdx", "spdx23", "spdx3"] {
        assert_eq!(row[f]["kind"], "present", "{row}");
        assert_eq!(row[f]["value_count"], 1, "{row}");
    }
    assert_eq!(row["universal_parity"], true, "{row}");
}
