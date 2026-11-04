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

    fn scope(&self, name: &str) -> Option<&str> {
        self.require(name)["scope"].as_str()
    }
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
            "--output",
        ])
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
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
    let cdx: Value =
        serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    Scan { cdx, _dir: dir, _cache: cache }
}

/// A diagnostic, not an assertion. Run it with `--ignored --nocapture` to see
/// what the fixture actually produces before trusting any assertion about it.
///
/// It exists because the alternative is writing assertions from what the
/// fixture was *intended* to produce, and milestone 1275's F7 is what that
/// costs: a fixture whose collision was protected by two rules at once, so
/// the test could not fail and nobody noticed for a milestone.
#[test]
#[ignore = "diagnostic; run with --ignored --nocapture"]
fn m1277_what_the_fixture_actually_produces() {
    let s = scan();
    println!("\n{:<34} {:<16} {:<10} {}", "name", "role", "scope", "purl");
    for c in s.components() {
        println!(
            "{:<34} {:<16} {:<10} {}",
            c["name"].as_str().unwrap_or("?"),
            prop(c, "waybill:closure-role").unwrap_or("-"),
            c["scope"].as_str().unwrap_or("-"),
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
