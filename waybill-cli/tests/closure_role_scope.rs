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
//! | `roletoolboot` | `nativeBuildInputs` | `build-tooling` | FR-005: Hackage, GHC-built, **nulled**, so exempt |
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

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
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
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h3-roletoolhask" } }
    },
    "d-roletoolboot.drv": {
      "name": "waybill-fixture-roletoolboot-2.2.2",
      "env": {
        "pname": "waybill-fixture-roletoolboot",
        "version": "2.2.2",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h4-roletoolboot" } }
    },
    "d-roledual.drv": {
      "name": "waybill-fixture-roledual-4.4.4",
      "env": {
        "pname": "waybill-fixture-roledual",
        "version": "4.4.4",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h5-roledual" } }
    },
    "d-roletest.drv": {
      "name": "waybill-fixture-roletest-6.6.6",
      "env": {
        "pname": "waybill-fixture-roletest",
        "version": "6.6.6",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
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
    _dir: tempfile::TempDir,
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
            .find_map(|st| envelope_field(st))
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
        _dir: dir,
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
        "\n{:<30} {:<15} {:<9} {:<36} {}",
        "name", "role", "scope", "classification-source", "purl"
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
#[test]
fn m1277_the_exemption_fires_on_the_nulled_member_and_only_on_it() {
    let s = scan();

    assert_eq!(s.role("waybill-fixture-roletoolboot"), Some("build-tooling"));
    assert_eq!(
        s.scope("waybill-fixture-roletoolboot"),
        None,
        "the compiler supplies this package in every build, so no format may \
         report it absent from the artifact (FR-005); invariant I9 asserts the same"
    );

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

/// T024. All three states of C203 on one document, including the **absent**
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
    assert_eq!(
        s.source("waybill-fixture-roletoolboot"),
        None,
        "exempt: the feature declined to classify it, so it must not claim to have"
    );
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
    ] {
        let cdx = s.cdx_source(name);
        assert!(cdx.is_some(), "{name} carries no C203 in CycloneDX");
        assert_eq!(cdx, s.spdx23_source(name), "{name}: CDX vs SPDX 2.3");
        assert_eq!(cdx, s.spdx3_source(name), "{name}: CDX vs SPDX 3");
    }
    // And the exempt member carries it in none of the three.
    for f in ["cdx", "spdx23", "spdx3"] {
        assert_eq!(
            s.source_in(f, "waybill-fixture-roletoolboot"),
            None,
            "the exempt member must carry no C203 in {f} either"
        );
    }
}
