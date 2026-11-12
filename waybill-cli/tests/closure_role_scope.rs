//! Milestone 1277 (#1342) — a closure member that only builds the artifact
//! must not read as part of it.
//!
//! Drives the closure tier through the stub `nix` that
//! `nix_config_closure.rs` established, with a package-set cache seeded the
//! way `nix_haskell_resolution_m926.rs` does. **No test reads the host's
//! `~/.cache/waybill/nixpkgs/`** (Principle VII), and every name is
//! `waybill-fixture-*` because real coordinates in fixtures trip the
//! advisory scan (#285, #640).
//!
//! The cache is **seeded**, not merely redirected. Pointing
//! `WAYBILL_NIXPKGS_CACHE` at an empty directory does not make a test
//! offline — it makes `enrich` fetch 16 MB. That error was made in milestone
//! 1275 (findings F8) and again in 1276 (F8), so it is written down here.
//!
//! # The fixture's own closure, and what each member is for
//!
//! | member | referenced via | role | why it is here |
//! |---|---|---|---|
//! | `roletext` | `buildInputs` | `artifact-input` | FR-002: must be left alone |
//! | `roledual` | both fields | `both` | FR-002: must be left alone |
//! | `roletool` | `nativeBuildInputs` | `build-tooling` | FR-001, the plain case |
//! | `roletoolhask` | `nativeBuildInputs` | `build-tooling` | FR-005's **control**: Hackage, GHC-built, *not* nulled, so it must still be classified |
//! | `roletoolboot` | `nativeBuildInputs` | `build-tooling` | #1478: Hackage, GHC-built, with a **nulled** name. A closure derivation is never GHC's own copy (that lives inside the compiler's derivation), so it is classified like `roletoolhask`. FR-005's exemption fires only on the compiler's copy; the unit tests in `resolve/closure_role_scope.rs` cover it |
//! | `roletest` | `buildInputs`, and only a `buildable: False` stanza declares it | `artifact-input` | FR-002's real shape: already `excluded` from the m1269 propagation, and must stay so |
//! | `rolepatch` | nothing | `unreferenced` | FR-004 — and it carries `patches`, without which it is not emitted at all |
//!
//! `rolepatch` needs that `patches` entry because `emit::emits_as_component`
//! readmits only the patch-appliers among `unreferenced` members. Every
//! `unreferenced` component in a real document is therefore a patch-applier
//! — measured, 17 of them on `nix-closure-moat` — which is why FR-004's
//! clarification settled on `excluded`: the component *is* the patch, and
//! the patch does not ship even though the patched code does.
//!
//! `roletoolhask` is the member that makes FR-005's test able to fail. With
//! only `roletoolboot` in a tooling role, deleting the exemption would
//! change one component and nothing would distinguish "the exemption works"
//! from "the feature never classifies Hackage packages". Milestone 1275's
//! F7 is that mistake made for real: the test its spec called most
//! important was protected by two independent rules at once, so it could not
//! fail.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod common;

/// The revision the fixture `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

const CLOSURE_JSON: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-roleproj-0.1.0",
      "env": {
        "pname": "waybill-fixture-roleproj",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-roletext /nix/store/h5-roledual /nix/store/h7-roletest",
        "nativeBuildInputs": "/nix/store/h2-roletool /nix/store/h3-roletoolhask /nix/store/h4-roletoolboot /nix/store/h5-roledual"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-roletext.drv": {
      "name": "waybill-fixture-roletext-2.0.2",
      "env": {
        "pname": "waybill-fixture-roletext",
        "version": "2.0.2",
        "nativeBuildInputs": "/nix/store/zzz-ghc-99.0.1"
      },
      "outputs": { "out": { "path": "h1-roletext" } }
    },
    "d-roletool.drv": {
      "name": "waybill-fixture-roletool-1.0.0",
      "env": {
        "pname": "waybill-fixture-roletool",
        "version": "1.0.0",
        "nativeBuildInputs": "/nix/store/yyy-gcc-13.2.0"
      },
      "outputs": { "out": { "path": "h2-roletool" } }
    },
    "d-roletoolhask.drv": {
      "name": "waybill-fixture-roletoolhask-3.3.3",
      "env": {
        "pname": "waybill-fixture-roletoolhask",
        "version": "3.3.3",
        "nativeBuildInputs": "/nix/store/zzz-ghc-99.0.1"
      },
      "outputs": { "out": { "path": "h3-roletoolhask" } }
    },
    "d-roletoolboot.drv": {
      "name": "waybill-fixture-roletoolboot-2.2.2",
      "env": {
        "pname": "waybill-fixture-roletoolboot",
        "version": "2.2.2",
        "nativeBuildInputs": "/nix/store/zzz-ghc-99.0.1"
      },
      "outputs": { "out": { "path": "h4-roletoolboot" } }
    },
    "d-roledual.drv": {
      "name": "waybill-fixture-roledual-4.4.4",
      "env": {
        "pname": "waybill-fixture-roledual",
        "version": "4.4.4",
        "nativeBuildInputs": "/nix/store/zzz-ghc-99.0.1"
      },
      "outputs": { "out": { "path": "h5-roledual" } }
    },
    "d-roletest.drv": {
      "name": "waybill-fixture-roletest-6.6.6",
      "env": {
        "pname": "waybill-fixture-roletest",
        "version": "6.6.6",
        "nativeBuildInputs": "/nix/store/zzz-ghc-99.0.1"
      },
      "outputs": { "out": { "path": "h7-roletest" } }
    },
    "d-rolepatch.drv": {
      "name": "waybill-fixture-rolepatch-5.5.5",
      "env": {
        "pname": "waybill-fixture-rolepatch",
        "version": "5.5.5",
        "patches": "/nix/store/p1-CVE-2024-0001.patch",
        "nativeBuildInputs": "/nix/store/yyy-gcc-13.2.0"
      },
      "outputs": { "out": { "path": "h6-rolepatch" } }
    }
  },
  "version": 3
}"#;

/// The seeded `hackage-packages.nix`. The three members whose PURL must be
/// `pkg:hackage` are declared here at the exact version the closure records,
/// which is what milestone 1275's identity pass requires to confirm them.
const PACKAGES: &str = r#"
  waybill-fixture-roletext = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-roletext";
      version = "2.0.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-roletoolhask = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-roletoolhask";
      version = "3.3.3";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-roletoolboot = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-roletoolboot";
      version = "2.2.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-roletest = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-roletest";
      version = "6.6.6";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-roledual = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-roledual";
      version = "4.4.4";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
"#;

/// Nulls exactly one name. That is how milestone 926 identifies a GHC boot
/// library — from the pinned revision's own configuration rather than from a
/// list someone maintains — and it is the only difference between
/// `roletoolboot` and its control `roletoolhask`.
const CONFIG: &str =
    "self: super: {\n  waybill-fixture-roletoolboot = null;\n}\n";

const SERIES: &[&str] =
    &["9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x"];

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/closure_role_scope")
        .join(name)
}

/// Seed the per-revision cache so the pass runs with no network.
fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).unwrap();
    std::fs::write(rev.join("hackage-packages.nix"), PACKAGES).unwrap();
    for s in SERIES {
        std::fs::write(rev.join(format!("configuration-ghc-{s}.nix")), CONFIG).unwrap();
    }
    dir
}

fn stub_script(log: &Path) -> String {
    // `r##"` rather than `r#"`: the script contains `*"#packages`, whose
    // `"#` would close an `r#"` raw string.
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
        closure = CLOSURE_JSON,
    )
}

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    dir: tempfile::TempDir,
    _cache: tempfile::TempDir,
}

impl Scan {
    fn components(&self) -> &Vec<Value> {
        self.cdx["components"].as_array().unwrap()
    }

    fn component(&self, name: &str) -> Option<&Value> {
        self.components().iter().find(|c| c["name"] == name)
    }

    fn require(&self, name: &str) -> &Value {
        self.component(name).unwrap_or_else(|| {
            let names: Vec<&str> =
                self.components().iter().filter_map(|c| c["name"].as_str()).collect();
            panic!("no component named {name}; the document holds {names:?}")
        })
    }

    fn role(&self, name: &str) -> Option<&str> {
        prop(self.require(name), "waybill:closure-role")
    }

    fn source(&self, name: &str) -> Option<&str> {
        prop(self.require(name), "waybill:scope-classification-source")
    }

    fn scope(&self, name: &str) -> Option<&str> {
        self.require(name)["scope"].as_str()
    }
}

impl Scan {
    /// C203 as CycloneDX spells it.
    fn cdx_source(&self, name: &str) -> Option<String> {
        self.source(name).map(str::to_string)
    }

    /// C203 as SPDX 2.3 spells it: a `waybill:*` annotation whose comment
    /// holds the milestone-071 envelope.
    fn spdx23_source(&self, name: &str) -> Option<String> {
        envelope_value(
            self.spdx23["packages"].as_array()?,
            |p| p["name"].as_str() == Some(name),
            "annotations",
            "comment",
        )
    }

    /// C203 as SPDX 3 spells it: an `Annotation` element whose `statement`
    /// holds the envelope, subject to the element for this component.
    fn spdx3_source(&self, name: &str) -> Option<String> {
        let graph = self.spdx3["@graph"].as_array()?;
        let subject = graph
            .iter()
            .find(|e| e["name"].as_str() == Some(name))?["spdxId"]
            .as_str()?
            .to_string();
        graph
            .iter()
            .filter(|e| e["type"] == "Annotation")
            .filter(|e| {
                e["subject"].as_str() == Some(subject.as_str())
                    || e["subject"]["@id"].as_str() == Some(subject.as_str())
            })
            .filter_map(|e| e["statement"].as_str())
            .find_map(envelope_field)
    }

    /// Dispatch by format name, for the loop that asserts absence in all
    /// three.
    fn source_in(&self, format: &str, name: &str) -> Option<String> {
        match format {
            "cdx" => self.cdx_source(name),
            "spdx23" => self.spdx23_source(name),
            "spdx3" => self.spdx3_source(name),
            other => panic!("unknown format {other}"),
        }
    }
}

/// Pull the C203 value out of a milestone-071 annotation envelope.
///
/// The envelope's shape, read off a real document rather than assumed:
///
/// ```json
/// {"schema":"waybill-annotation/v1","field":"waybill:…","value":"…"}
/// ```
///
/// The first draft of this decoder looked for a nested `waybill:annotation`
/// object with a `name` key and returned `None` for every annotation, which
/// made the cross-format test fail against a *correct* emission. The
/// diagnostic beside it (`m1277_raw_spdx_annotation_shapes`) is kept for
/// exactly that reason.
fn envelope_field(raw: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    if v.get("field")?.as_str()? != "waybill:scope-classification-source" {
        return None;
    }
    Some(v.get("value")?.as_str()?.to_string())
}

/// Find the annotation on the matching package and decode its envelope.
fn envelope_value(
    items: &[Value],
    matches: impl Fn(&Value) -> bool,
    list: &str,
    field: &str,
) -> Option<String> {
    items
        .iter()
        .find(|i| matches(i))?
        .get(list)?
        .as_array()?
        .iter()
        .filter_map(|a| a.get(field)?.as_str())
        .find_map(envelope_field)
}

fn prop<'a>(component: &'a Value, key: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == key)
        .and_then(|p| p["value"].as_str())
}

/// Scan the fixture with `--nix-closure` through the stub `nix`.
fn scan() -> Scan {
    scan_with(&[])
}

/// As `scan`, with extra CLI arguments.
fn scan_with(extra: &[&str]) -> Scan {
    let dir = tempfile::tempdir().unwrap();
    let cache = seed_cache();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let nix = bin.join("nix");
    std::fs::write(&nix, stub_script(&dir.path().join("nix.log"))).unwrap();
    #[cfg(unix)]
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
    let read = |p: &Path| -> Value {
        serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
    };
    Scan {
        cdx: read(&cdx_path),
        spdx23: read(&spdx23_path),
        spdx3: read(&spdx3_path),
        dir,
        _cache: cache,
    }
}

/// All three documents from one scan. The same thing `scan()` returns — the
/// alias exists so the cross-format test reads as what it is.
fn scan_all() -> Scan {
    scan()
}

/// A diagnostic, not an assertion. Run it with `--ignored --nocapture` to see
/// what the fixture actually produces before trusting any assertion about it.
///
/// It exists because the alternative is writing assertions from what the
/// fixture was *intended* to produce, and milestone 1275's F7 is what that
/// costs: a fixture whose collision was protected by two rules at once, so
/// the test could not fail and nobody noticed for a milestone.
/// A second diagnostic: the raw SPDX annotation shapes, so the decoding in
/// this file is written from what the emitters produce rather than from what
/// the envelope is assumed to look like.
/// A third diagnostic: the SPDX native relationships this feature adds, in
/// both formats, so T043/T044's assertions are written from the emission
/// rather than from the vocabulary.
/// A fourth diagnostic: every property on the members that matter, so an
/// invariant written against this document is written against what it holds.
#[test]
#[ignore = "diagnostic; run with --ignored --nocapture"]
fn m1277_properties_on_the_decisive_members() {
    let s = scan();
    for name in [
        "waybill-fixture-roletoolboot",
        "waybill-fixture-roletoolhask",
        "waybill-fixture-rolepatch",
        "base",
    ] {
        println!("\n--- {name} ---");
        let c = s.require(name);
        println!("  scope = {:?}", c["scope"].as_str());
        for p in c["properties"].as_array().into_iter().flatten() {
            println!(
                "  {} = {}",
                p["name"].as_str().unwrap_or("?"),
                p["value"].as_str().unwrap_or("?")
            );
        }
    }
    println!();
}

#[test]
#[ignore = "diagnostic; run with --ignored --nocapture"]
fn m1277_raw_spdx_tool_relationships() {
    let s = scan();
    println!("\n--- SPDX 2.3 BUILD_TOOL_OF ---");
    for r in s.spdx23["relationships"].as_array().unwrap() {
        if r["relationshipType"] == "BUILD_TOOL_OF" {
            println!(
                "  {} {} {}",
                r["spdxElementId"].as_str().unwrap_or("?"),
                r["relationshipType"].as_str().unwrap_or("?"),
                r["relatedSpdxElement"].as_str().unwrap_or("?")
            );
        }
    }
    println!("\n--- SPDX 3 usesTool / LifecycleScopedRelationship ---");
    for e in s.spdx3["@graph"].as_array().unwrap() {
        if e["relationshipType"] == "usesTool" {
            println!(
                "  type={} scope={} from={} to={}",
                e["type"].as_str().unwrap_or("?"),
                e["scope"].as_str().unwrap_or("-"),
                e["from"].as_str().unwrap_or("?"),
                e["to"][0].as_str().unwrap_or("?")
            );
        }
    }
    println!();
}

#[test]
#[ignore = "diagnostic; run with --ignored --nocapture"]
fn m1277_raw_spdx_annotation_shapes() {
    let s = scan();
    println!("\n--- SPDX 2.3: package `waybill-fixture-roletool` ---");
    let pkg = s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill-fixture-roletool")
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&pkg["annotations"]).unwrap());
    println!("\n--- SPDX 3: Annotation elements mentioning the row ---");
    for e in s.spdx3["@graph"].as_array().unwrap() {
        let st = e["statement"].as_str().unwrap_or("");
        if st.contains("scope-classification-source") {
            println!("{}", serde_json::to_string_pretty(e).unwrap());
            break;
        }
    }
    println!();
}

#[test]
#[ignore = "diagnostic; run with --ignored --nocapture"]
fn m1277_what_the_fixture_actually_produces() {
    let s = scan();
    println!(
        "\n{:<30} {:<15} {:<9} {:<36} purl",
        "name", "role", "scope", "classification-source"
    );
    for c in s.components() {
        println!(
            "{:<30} {:<15} {:<9} {:<36} {}",
            c["name"].as_str().unwrap_or("?"),
            prop(c, "waybill:closure-role").unwrap_or("-"),
            c["scope"].as_str().unwrap_or("-"),
            prop(c, "waybill:scope-classification-source").unwrap_or("-"),
            c["purl"].as_str().unwrap_or("-"),
        );
    }
    println!();
}

// ---- US1: FR-001, FR-004, FR-002 ----

/// FR-001 and FR-004 together, plus FR-002's negative half, measured on one
/// document so the four roles are compared under identical conditions.
///
/// The `artifact-input` and `both` rows are the ones that make this test
/// more than a tautology: a pass that marked everything would satisfy the
/// first two assertions and fail these.
#[test]
fn m1277_only_the_roles_that_do_not_ship_are_classified() {
    let s = scan();

    // FR-001 — nix referenced it only through a tooling field.
    assert_eq!(s.role("waybill-fixture-roletool"), Some("build-tooling"));
    assert_eq!(
        s.scope("waybill-fixture-roletool"),
        Some("excluded"),
        "a build tool must not read as part of the artifact"
    );

    // FR-004 — referenced by neither field set. Emitted at all only because
    // it applies a patch: `emit::emits_as_component` readmits exactly the
    // patch-appliers among `unreferenced` members.
    assert_eq!(s.role("waybill-fixture-rolepatch"), Some("unreferenced"));
    assert_eq!(
        s.scope("waybill-fixture-rolepatch"),
        Some("excluded"),
        "the patched code ships; the patch does not, and the component is the patch"
    );

    // FR-002 — neither of these establishes runtime presence, so this
    // feature asserts nothing about them. `ARTIFACT_FIELDS` includes
    // `buildInputs`, through which nixpkgs' Haskell builder passes test
    // dependencies: `hspec` is `artifact-input` on real data and does not
    // ship.
    assert_eq!(s.role("waybill-fixture-roletext"), Some("artifact-input"));
    assert_eq!(
        s.scope("waybill-fixture-roletext"),
        None,
        "an artifact-input member must keep an absent scope"
    );
    assert_eq!(s.role("waybill-fixture-roledual"), Some("both"));
    assert_eq!(
        s.scope("waybill-fixture-roledual"),
        None,
        "a member referenced both ways has an artifact path"
    );
}

/// FR-002 on the shape it exists for, which the other tests do not reach.
///
/// `roletest` is `artifact-input` by role **and** already carries `excluded`
/// from the milestone-1269 propagation, because only a `buildable: False`
/// stanza declares it. That is the position ten real components are in:
/// `hspec`, `hspec-core`, `HUnit`, `QuickCheck` and their family are
/// `artifact-input` because nixpkgs' Haskell builder routes test
/// dependencies through `buildInputs`, and dev-scoped because the cabal
/// graph says test-only.
///
/// The fixture reaches that shape through a `buildable: False` stanza, which
/// yields `LifecycleScope::Optional` rather than the `Development` the real
/// ten carry. Measured, not assumed — the first draft of this test asserted
/// `development` and failed. The difference does not matter to FR-002, which
/// is about preserving whatever the propagation set, not about which variant
/// it set: both are non-runtime, both emit `excluded`, and the thing under
/// test is that this feature leaves the value alone.
///
/// So this is the assertion that catches a pass which "helpfully" wrote the
/// scope it found rather than leaving it: the value must be the propagation's,
/// unchanged. A test using a member with *no* prior scope cannot tell the two
/// apart.
#[test]
fn m1277_an_artifact_input_member_keeps_the_scope_the_propagation_gave_it() {
    let s = scan();
    assert_eq!(s.role("waybill-fixture-roletest"), Some("artifact-input"));
    assert_eq!(
        s.scope("waybill-fixture-roletest"),
        Some("excluded"),
        "the m1269 propagation set this; FR-002 requires it byte-identical"
    );
    assert_eq!(
        prop(s.require("waybill-fixture-roletest"), "waybill:lifecycle-scope"),
        Some("optional"),
        "and its lifecycle scope is the propagation's too, not this feature's"
    );
}

/// FR-008's local half. A component with no closure role is not this
/// feature's business, whatever else it carries.
///
/// Two such components are in this document for different reasons, and both
/// matter: `nixpkgs` already carries `excluded` from its `build` lifecycle
/// scope, so it proves the pass does not disturb a scope it did not set;
/// and `base` is a milestone-1275 synthetic compiler-supplied component,
/// which has no role because it is not a closure member at all.
#[test]
fn m1277_a_component_with_no_closure_role_is_not_touched() {
    let s = scan();
    assert_eq!(s.role("nixpkgs"), None, "the nixpkgs component carries no role");
    assert_eq!(
        s.scope("nixpkgs"),
        Some("excluded"),
        "its pre-existing lifecycle-derived scope must survive untouched"
    );
    assert_eq!(s.role("base"), None, "a synthetic boot component is not a closure member");
    assert_eq!(s.scope("base"), None);
}

// ---- US2: FR-005, FR-005a, FR-006 ----

/// FR-005 with the control beside it, which is the whole point of this test.
///
/// `roletoolboot` and `roletoolhask` differ in exactly one respect: the
/// pinned revision's compiler configuration nulls the first and not the
/// second. Same role, same ecosystem, same GHC-built signal, same position
/// in the closure. So if the exemption is deleted, the first assertion
/// fails; and if the exemption is widened to match everything, the second
/// fails. Neither can pass for a reason unrelated to FR-005.
///
/// Milestone 1275's F7 is why this is built this way: the test its spec
/// called most important was protected by two independent rules at once, so
/// it could not fail, and nobody noticed for a milestone.
///
/// #1478 corrected the premise. `roletoolboot` is a closure derivation, and a
/// separate derivation is never GHC's own copy of a boot library: that ships
/// inside the compiler's derivation. On the corpus the shape is real
/// (`os-string@2.0.10` built from Hackage beside GHC's `os-string@2.0.7`),
/// and exempting it overrode the evidence that scoped it. So the nulled name
/// no longer exempts it, and it must be classified exactly like its control.
/// The exemption's positive case, GHC's copy carrying C218, is a unit test.
#[test]
fn m1478_a_closure_build_of_a_nulled_name_is_classified_like_any_member() {
    let s = scan();

    assert_eq!(s.role("waybill-fixture-roletoolboot"), Some("build-tooling"));
    assert_eq!(
        s.scope("waybill-fixture-roletoolboot"),
        Some("excluded"),
        "a closure build of a boot-library name is not the compiler's copy (#1478)"
    );
    assert_eq!(s.source("waybill-fixture-roletoolboot"), Some("closure-role"));

    assert_eq!(s.role("waybill-fixture-roletoolhask"), Some("build-tooling"));
    assert_eq!(
        s.scope("waybill-fixture-roletoolhask"),
        Some("excluded"),
        "the control: identical in every respect except that the configuration \
         does not null it, so the exemption must not reach it"
    );
}

/// FR-005a and FR-006. Every C182 value survives unchanged, across all four
/// roles rather than only the exempt one — FR-006 says "every", and
/// asserting it for one role would leave three untested.
#[test]
fn m1277_every_closure_role_annotation_survives_unchanged() {
    let s = scan();
    for (name, role) in [
        ("waybill-fixture-roletext", "artifact-input"),
        ("waybill-fixture-roletest", "artifact-input"),
        ("waybill-fixture-roledual", "both"),
        ("waybill-fixture-roletool", "build-tooling"),
        ("waybill-fixture-roletoolhask", "build-tooling"),
        // The exempt one. The exemption withholds the classification's
        // native expression, not the evidence for it: a reader needs the
        // role to see *why* this member carries no marker.
        ("waybill-fixture-roletoolboot", "build-tooling"),
        ("waybill-fixture-rolepatch", "unreferenced"),
    ] {
        assert_eq!(s.role(name), Some(role), "C182 changed on {name}");
    }
}

// ---- US3: FR-009, C203 ----

/// T024. All four states of C203 on one document, including the **absent**
/// one, which is the state that silently passes if the annotation is emitted
/// unconditionally.
///
/// Milestone 1276's US3 shipped exactly that defect: it emitted its record on
/// every document, and the gate caught it as 35 byte-identity failures across
/// 11 ecosystems. The absence assertions below are the cheap version of that
/// gate, and they run in 1.5 seconds rather than 25 minutes.
#[test]
fn m1277_the_classification_source_has_three_states_and_absence_is_one() {
    let s = scan();

    // (1) the closure role alone.
    assert_eq!(s.source("waybill-fixture-roletool"), Some("closure-role"));
    assert_eq!(s.source("waybill-fixture-rolepatch"), Some("closure-role"));

    // (2) both analyses, independently. See T025's test for why this value
    // has to be reachable at all.
    assert_eq!(
        s.source("waybill-fixture-roletoolhask"),
        Some("closure-role+lifecycle-propagation")
    );

    // (3) absent — and for four distinct reasons, each of which would be a
    // separate defect if the record appeared.
    // (4) exempt -- reported as itself, not as absence. Since #1478 no
    // closure derivation is the compiler's copy, so this fixture has no
    // exempt member; the state is asserted by the unit tests in
    // `resolve/closure_role_scope.rs` (`m1478_*`). `roletoolboot`, a closure
    // build of a nulled name, takes state (1).
    assert_eq!(s.source("waybill-fixture-roletoolboot"), Some("closure-role"));
    assert_eq!(
        s.source("waybill-fixture-roletest"),
        None,
        "scoped by the propagation alone -- C62 carries that provenance, and \
         repeating it here would put this row on every dev-scoped component in \
         every ecosystem (FR-009's measured scope correction)"
    );
    assert_eq!(
        s.source("waybill-fixture-roletext"),
        None,
        "artifact-input and unscoped: nothing classified it"
    );
    assert_eq!(s.source("nixpkgs"), None, "carries no closure role at all");
}

/// T025. The `closure-role+lifecycle-propagation` value is reachable by
/// construction, not merely asserted to be correct.
///
/// `roletoolhask` sits in a `build-tooling` role **and** is declared only in
/// a `buildable: False` stanza, so both analyses reach it independently. That
/// combination had to be engineered: the first draft of this fixture produced
/// the value nowhere, which would have made the arm above dead code with a
/// passing test over it.
///
/// On the corpus the population is **zero** today — the ten already-scoped
/// components on `nix-closure-moat` are all in roles this feature skips — so
/// the corpus cannot stand in for this fixture.
#[test]
fn m1277_the_both_analyses_value_is_reachable_and_not_dead_code() {
    let s = scan();
    let c = s.require("waybill-fixture-roletoolhask");
    assert_eq!(s.role("waybill-fixture-roletoolhask"), Some("build-tooling"));
    assert_eq!(
        prop(c, "waybill:lifecycle-scope"),
        Some("optional"),
        "the propagation classified it too, through the non-buildable stanza"
    );
    assert_eq!(
        s.source("waybill-fixture-roletoolhask"),
        Some("closure-role+lifecycle-propagation"),
        "so the combined value is produced by a real configuration, not a literal"
    );
}

/// FR-010's local half: the value is identical in all three formats.
///
/// One shared helper computes it, so this cannot drift — but C19 is the
/// precedent for checking anyway. There, one format's value drifted in shape
/// from the other two and its parity extractor was hand-written to
/// compensate, so the catalog looked consistent while the documents were not,
/// and it surfaced only on Go CPEs.
#[test]
fn m1277_the_classification_source_agrees_across_all_three_formats() {
    let s = scan_all();
    for name in [
        "waybill-fixture-roletool",
        "waybill-fixture-rolepatch",
        "waybill-fixture-roletoolhask",
        "waybill-fixture-roletoolboot",
    ] {
        let cdx = s.cdx_source(name);
        assert!(cdx.is_some(), "{name} carries no C203 in CycloneDX");
        assert_eq!(cdx, s.spdx23_source(name), "{name}: CDX vs SPDX 2.3");
        assert_eq!(cdx, s.spdx3_source(name), "{name}: CDX vs SPDX 3");
    }
    // The exempt value's cross-format agreement was asserted here on
    // `roletoolboot` until #1478 made that member an ordinary closure build.
    // Every C203 value goes through the same emitters and the same
    // `SymmetricEqual` extractor, which the loop above exercises.
    for f in ["cdx", "spdx23", "spdx3"] {
        assert_eq!(
            s.source_in(f, "waybill-fixture-roletoolboot").as_deref(),
            Some("closure-role"),
            "{f}"
        );
    }
}

// ---- US4: FR-012, FR-013, FR-014 ----

impl Scan {
    /// SPDX 2.3 package id → name.
    fn spdx23_names(&self) -> std::collections::HashMap<String, String> {
        self.spdx23["packages"]
            .as_array()
            .map(|ps| {
                ps.iter()
                    .filter_map(|p| {
                        Some((
                            p["SPDXID"].as_str()?.to_string(),
                            p["name"].as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Names on the member side of every `BUILD_TOOL_OF` edge.
    fn spdx23_build_tools(&self) -> BTreeSet<String> {
        let names = self.spdx23_names();
        self.spdx23["relationships"]
            .as_array()
            .map(|rs| {
                rs.iter()
                    .filter(|r| r["relationshipType"] == "BUILD_TOOL_OF")
                    .filter_map(|r| names.get(r["spdxElementId"].as_str()?).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Names on the target side of every `usesTool` relationship whose
    /// scope is `build`.
    fn spdx3_build_tools(&self) -> BTreeSet<String> {
        let graph = match self.spdx3["@graph"].as_array() {
            Some(g) => g,
            None => return BTreeSet::new(),
        };
        let name_by_iri: std::collections::HashMap<&str, &str> = graph
            .iter()
            .filter_map(|e| Some((e["spdxId"].as_str()?, e["name"].as_str()?)))
            .collect();
        graph
            .iter()
            .filter(|e| e["relationshipType"] == "usesTool")
            .filter(|e| e["type"] == "LifecycleScopedRelationship")
            .filter(|e| e["scope"] == "build")
            .filter_map(|e| name_by_iri.get(e["to"][0].as_str()?).map(|n| (*n).to_string()))
            .collect()
    }
}

/// FR-012 and FR-013 together, because the thing worth asserting is that the
/// two formats name the **same set** — not merely that each emits something.
///
/// The directions differ on purpose and the test does not care: SPDX 2.3
/// spells it `member BUILD_TOOL_OF root` and SPDX 3 spells it
/// `root usesTool member`, because the two vocabularies put the subject on
/// opposite sides. Both accessors normalise to "the member that is a tool",
/// which is the claim.
#[test]
fn m1277_both_spdx_formats_name_the_same_build_tools() {
    let s = scan();
    let expected: BTreeSet<String> = [
        "waybill-fixture-roletool",
        "waybill-fixture-rolepatch",
        "waybill-fixture-roletoolhask",
        // #1478: a closure build of a nulled name is a build tool like any other.
        "waybill-fixture-roletoolboot",
    ]
    .iter()
    .map(|n| (*n).to_string())
    .collect();

    assert_eq!(
        s.spdx23_build_tools(),
        expected,
        "SPDX 2.3 BUILD_TOOL_OF set"
    );
    assert_eq!(
        s.spdx3_build_tools(),
        expected,
        "SPDX 3 usesTool/build set"
    );
}

/// FR-014, turned round by #1478. Every format must agree, and the likeliest
/// way that breaks is one emitter interpreting the resolve stage's field for
/// itself. This was the check that the exemption held in all three; since
/// #1478 the member it used is classified, so the same three emitters are
/// checked the other way: none of them may still honour the old exemption.
#[test]
fn m1478_a_closure_build_of_a_nulled_name_is_a_build_tool_in_all_three_formats() {
    let s = scan();
    let name = "waybill-fixture-roletoolboot";

    assert_eq!(s.scope(name), Some("excluded"), "CycloneDX");
    assert!(s.spdx23_build_tools().contains(name), "SPDX 2.3 BUILD_TOOL_OF");
    assert!(s.spdx3_build_tools().contains(name), "SPDX 3 usesTool");
    for f in ["cdx", "spdx23", "spdx3"] {
        assert_eq!(s.source_in(f, name).as_deref(), Some("closure-role"), "{f}");
    }
}

/// An `artifact-input` member is named as a build tool by neither format.
///
/// The complement of the test above, and the one that fails if the emitters
/// key off the closure role directly rather than off the classification the
/// pass made — a plausible shortcut, and wrong, because it would skip both
/// the exemption and FR-002.
#[test]
fn m1277_an_artifact_member_is_not_named_a_build_tool_in_either_format() {
    let s = scan();
    for name in ["waybill-fixture-roletext", "waybill-fixture-roledual"] {
        assert!(!s.spdx23_build_tools().contains(name), "SPDX 2.3 named {name}");
        assert!(!s.spdx3_build_tools().contains(name), "SPDX 3 named {name}");
    }
}

/// T046's second half — FR-013 against the pinned conformance validator,
/// not against the schema as I read it.
///
/// A new relationship class is exactly the change that round-trips cleanly
/// and fails conformance. Milestone 085 is the case: `scope` on a plain
/// `Relationship` serialises and parses fine, and fails JSON-Schema
/// validation, because the `LifecycleScopedRelationship_props` allOf branch
/// is not selected and `scope` becomes an unknown property. That defect was
/// invisible for several milestones because no fixture with a typed
/// dependency edge also ran the SPDX 3 gate.
///
/// Skips with a warning when the validator is absent, and fails hard under
/// `WAYBILL_REQUIRE_SPDX3_VALIDATOR=1`, which is how CI runs it — the
/// milestone-078 pattern, copied rather than reinvented.
#[test]
fn m1277_spdx3_validate_accepts_the_tool_relationships() {
    let s = scan();
    // Written where the validator can read it; `scan()`'s tempdir is alive
    // for the lifetime of `s`, so the path stays valid.
    let out = s.dir.path().join("conformance.spdx3.json");
    std::fs::write(&out, serde_json::to_vec_pretty(&s.spdx3).unwrap()).unwrap();

    // The emission must actually be present, or this gate passes vacuously.
    // A document with no `usesTool` element validates perfectly and proves
    // nothing about FR-013.
    assert_eq!(
        s.spdx3_build_tools().len(),
        4,
        "no tool relationships to validate -- this gate would have passed on an empty document"
    );

    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".venv/spdx3-validate/bin/spdx3-validate");
    if !bin.exists() {
        if std::env::var("WAYBILL_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() == Some("1") {
            panic!(
                "spdx3-validate not found at {} and WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 is set; \
                 run scripts/install-spdx3-validate.sh before re-running CI.",
                bin.display()
            );
        }
        eprintln!(
            "[closure_role_scope] WARN: spdx3-validate not found at {}; skipping the \
             conformance gate (set WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 to fail hard).",
            bin.display()
        );
        return;
    }
    let output = common::spdx3_validate::command(&bin)
        .arg("--quiet")
        .arg("-j")
        .arg(&out)
        .output()
        .expect("spdx3-validate must run when the binary exists");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && !combined.contains("Violation of type"),
        "spdx3-validate reported violations:\n{combined}"
    );
}

/// T048 / SC-011 — invariant I4's rule, applied to this fixture.
///
/// I4 asserts CycloneDX and SPDX 2.3 agree on the subject's direct
/// dependency set. This feature adds 97 SPDX relationships on the corpus and
/// none to CycloneDX's dependency graph, so it is the obvious way to break
/// I4 — and the reason it does not is worth pinning here rather than
/// discovering in a corpus refresh.
///
/// `BUILD_TOOL_OF` is **not** in the harness's
/// `SPDX_INVERSE_DEPENDENCY_TYPES`, so `spdx_dependency_edges` ignores it.
/// That list was right before this feature existed and is right now: a build
/// tool is not something the artifact depends on. Had `BUILD_TOOL_OF` been
/// added to it, SPDX would report 97 direct dependencies CycloneDX does not,
/// and I4 would fail on a document where both formats are correct.
///
/// The normalisation below is a transcription of the harness's own, because
/// a local re-implementation that happened to be laxer would pass while the
/// corpus failed.
#[test]
fn m1277_the_tool_edges_do_not_disturb_cross_format_dependency_parity() {
    let s = scan();

    // The inverse types the harness normalises, verbatim from
    // `corpus_harness_195/invariants/checks/mod.rs`.
    const INVERSE: &[&str] = &[
        "DEV_DEPENDENCY_OF",
        "OPTIONAL_DEPENDENCY_OF",
        "TEST_DEPENDENCY_OF",
        "BUILD_DEPENDENCY_OF",
        "PROVIDED_DEPENDENCY_OF",
        "RUNTIME_DEPENDENCY_OF",
        "DEPENDENCY_OF",
    ];

    let names = s.spdx23_names();
    let subject = s.spdx23["documentDescribes"][0]
        .as_str()
        .or_else(|| {
            s.spdx23["relationships"]
                .as_array()?
                .iter()
                .find(|r| r["relationshipType"] == "DESCRIBES")?["relatedSpdxElement"]
                .as_str()
        })
        .expect("SPDX 2.3 subject");

    let mut spdx_direct: BTreeSet<String> = BTreeSet::new();
    for r in s.spdx23["relationships"].as_array().unwrap() {
        let ty = r["relationshipType"].as_str().unwrap_or("");
        let from = r["spdxElementId"].as_str().unwrap_or("");
        let to = r["relatedSpdxElement"].as_str().unwrap_or("");
        let (src, dst) = if ty == "DEPENDS_ON" {
            (from, to)
        } else if INVERSE.contains(&ty) {
            (to, from)
        } else {
            continue; // BUILD_TOOL_OF lands here, which is the point
        };
        if src == subject {
            if let Some(n) = names.get(dst) {
                spdx_direct.insert(n.clone());
            }
        }
    }

    // CycloneDX side: the subject's `dependsOn` set, by name.
    let ref_to_name: std::collections::HashMap<&str, &str> = s
        .components()
        .iter()
        .filter_map(|c| Some((c["bom-ref"].as_str()?, c["name"].as_str()?)))
        .collect();
    let cdx_subject_ref = s.cdx["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("CDX subject");
    let mut cdx_direct: BTreeSet<String> = BTreeSet::new();
    for d in s.cdx["dependencies"].as_array().into_iter().flatten() {
        if d["ref"].as_str() != Some(cdx_subject_ref) {
            continue;
        }
        for t in d["dependsOn"].as_array().into_iter().flatten() {
            if let Some(n) = t.as_str().and_then(|r| ref_to_name.get(r)) {
                cdx_direct.insert((*n).to_string());
            }
        }
    }

    assert_eq!(
        cdx_direct, spdx_direct,
        "the subject's direct dependency set must be the same in both formats; \
         a BUILD_TOOL_OF edge counted as a dependency is how this breaks"
    );

    // And the tool edges are genuinely present, so the agreement above is
    // not the agreement of two documents that both lack them.
    assert_eq!(s.spdx23_build_tools().len(), 4);
}

/// The control research R1's design decision actually rests on, and which
/// nothing tested until mutant M5 survived.
///
/// Routing the classification through `LifecycleScope::Build` would have
/// reached the native carrier in all three formats for free. It was rejected
/// for two measured reasons. **One of them was wrong**: `include_dev` is
/// hard-coded `true` at the `sbom scan` call site — the `--include-dev` flag
/// was removed in milestone 052/part-3 — so the CycloneDX gate is always
/// open on the only path that produces closure roles, and a
/// `lifecycle_scope` route would not have been suppressed by it. M5 moving
/// the emission back inside that gate changed no output, which is how the
/// error surfaced.
///
/// The second reason is the real one, and this test is it: `--exclude-scope`
/// drops components by `lifecycle_scope` (`apply_exclude_scope_filter`), so
/// a classification carried there would make `--exclude-scope build` **delete**
/// these components from the document. That is the opposite of the intent —
/// the `unreferenced` ones exist in the output precisely because m1035
/// measured `jq` and `lua` in that role carrying 6 of one project's 18 CVEs.
#[test]
fn m1277_the_classification_survives_exclude_scope_build() {
    let s = scan_with(&["--exclude-scope", "build"]);

    for name in [
        "waybill-fixture-roletool",
        "waybill-fixture-rolepatch",
        "waybill-fixture-roletoolhask",
        "waybill-fixture-roletoolboot",
    ] {
        assert!(
            s.component(name).is_some(),
            "{name} was dropped by `--exclude-scope build`; a classification carried \
             on `lifecycle_scope` would be deletable, which is why this feature does \
             not carry it there"
        );
        assert_eq!(
            s.scope(name),
            Some("excluded"),
            "{name} kept its component but lost the marker"
        );
    }

    // `roletoolboot` was checked here as the exempt member; since #1478 it is
    // a classified build tool, and the loop above covers it.

    // And the control on the control: a component the propagation scoped
    // `build` IS droppable, which is what makes the flag meaningful and
    // proves the scan actually applied it rather than ignoring the argument.
    assert!(
        s.component("nixpkgs").is_none(),
        "`--exclude-scope build` must still drop a component the propagation \
         scoped `build` -- if nothing is dropped, the flag was not applied and \
         the assertions above prove nothing"
    );
}

/// #1445 — C184's component counts describe the document after every pass
/// that can remove a closure member, `--exclude-scope` included.
///
/// `waybill-fixture-roletest` is a closure member the milestone-1269
/// propagation scopes `optional`, so `--exclude-scope optional`
/// removes it after the closure record has been built.
#[test]
fn i1445_c184_component_counts_follow_exclude_scope() {
    let s = scan_with(&["--exclude-scope", "optional"]);
    // CONTROL: a closure member really was removed, or the count below is
    // compared against a document the flag did not change.
    assert!(
        s.component("waybill-fixture-roletest").is_none(),
        "`--exclude-scope optional` must drop roletest for this test to mean anything"
    );
    assert!(
        scan().component("waybill-fixture-roletest").is_some(),
        "roletest must be present without the flag"
    );

    let raw = s.cdx["metadata"]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill:nix-closure")
        .expect("C184 absent: the closure tier degraded")["value"]
        .as_str()
        .unwrap();
    let rec: Value = serde_json::from_str(raw).unwrap();
    let mut members: Vec<&Value> = s
        .components()
        .iter()
        .filter(|c| prop(c, "waybill:closure-role").is_some())
        .collect();
    if prop(&s.cdx["metadata"]["component"], "waybill:closure-role").is_some() {
        members.push(&s.cdx["metadata"]["component"]);
    }
    let with_member_role =
        members.iter().filter(|c| prop(c, "waybill:closure-member-role").is_some()).count();

    assert_eq!(
        rec["components-emitted"].as_u64(),
        Some(members.len() as u64),
        "components-emitted must count the closure members this document holds: {rec}"
    );
    assert_eq!(
        rec["components-without-member-role"].as_u64(),
        Some((members.len() - with_member_role) as u64),
        "{rec}"
    );
}
