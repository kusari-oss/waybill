//! Milestone 1275 (#1282) — a closure member that is a known Hackage package
//! is identified as one.
//!
//! Drives the closure tier through the stub `nix` that `nix_config_closure.rs`
//! established, with a package-set cache seeded the way
//! `nix_haskell_resolution_m926.rs` does. **No test reads the host's
//! `~/.cache/waybill/nixpkgs/`** (Principle VII), and every name is
//! `waybill-fixture-*` because real coordinates in fixtures trip the advisory
//! scan (#285, #640).
//!
//! The fixture closure is built so that every one of the five identity
//! outcomes is reachable, including `signals-disagree`, which real data has
//! never produced (measured 0 of 105).
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision both fixture `flake.lock`s pin.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// The fixture closure.
///
/// `d-root` references every member through `buildInputs`, because a
/// derivation nothing references is `unreferenced` and
/// `closure::emit::components` does not emit it (C182). The GHC signal is a
/// store path in each member's own `nativeBuildInputs`.
///
/// The members, and what each one is for:
///
/// | member | closure version | set version | outcome |
/// |---|---|---|---|
/// | `aeson` | 2.2.4.1 | 2.2.4.1 | `confirmed`; declares `text` + `bootlib` |
/// | `text` | 2.0.2 | 2.0.2 | `confirmed`; the in-document edge target |
/// | `oddlic` | 3.1.4 | 3.1.4 | `confirmed`; its license will not canonicalise |
/// | `shell` | 5.3p9 | 0.1.8 | `version-disagrees` — the collision |
/// | `notghc` | 1.0.0 | 1.0.0 | `signals-disagree` — exact pair, no GHC |
/// | `autoconf` | 2.72 | — | `not-in-package-set` |
/// | `noversion` | (none) | — | `no-version-recorded` |
const CLOSURE_JSON: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-root-0.1.0",
      "env": {
        "pname": "waybill-fixture-root",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-aeson /nix/store/h2-text /nix/store/h3-shell /nix/store/h4-autoconf /nix/store/h5-noversion /nix/store/h6-notghc /nix/store/h7-oddlic"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-aeson.drv": {
      "name": "waybill-fixture-aeson-2.2.4.1",
      "env": {
        "pname": "waybill-fixture-aeson",
        "version": "2.2.4.1",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h1-aeson" } }
    },
    "d-text.drv": {
      "name": "waybill-fixture-text-2.0.2",
      "env": {
        "pname": "waybill-fixture-text",
        "version": "2.0.2",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h2-text" } }
    },
    "d-shell.drv": {
      "name": "waybill-fixture-shell-5.3p9",
      "env": {
        "pname": "waybill-fixture-shell",
        "version": "5.3p9",
        "nativeBuildInputs": "/nix/store/yyy-gcc-13.2.0"
      },
      "outputs": { "out": { "path": "h3-shell" } }
    },
    "d-autoconf.drv": {
      "name": "waybill-fixture-autoconf-2.72",
      "env": {
        "pname": "waybill-fixture-autoconf",
        "version": "2.72",
        "nativeBuildInputs": "/nix/store/yyy-gcc-13.2.0"
      },
      "outputs": { "out": { "path": "h4-autoconf" } }
    },
    "d-noversion.drv": {
      "name": "waybill-fixture-noversion",
      "env": { "pname": "waybill-fixture-noversion" },
      "outputs": { "out": { "path": "h5-noversion" } }
    },
    "d-notghc.drv": {
      "name": "waybill-fixture-notghc-1.0.0",
      "env": {
        "pname": "waybill-fixture-notghc",
        "version": "1.0.0",
        "nativeBuildInputs": "/nix/store/yyy-gcc-13.2.0"
      },
      "outputs": { "out": { "path": "h6-notghc" } }
    },
    "d-oddlic.drv": {
      "name": "waybill-fixture-oddlic-3.1.4",
      "env": {
        "pname": "waybill-fixture-oddlic",
        "version": "3.1.4",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h7-oddlic" } }
    }
  },
  "version": 3
}"#;

/// The seeded `hackage-packages.nix`.
///
/// `waybill-fixture-aeson` declares `waybill-fixture-bootlib`, which is
/// deliberately **absent from the closure** — that is FR-012c's dangling
/// target, and the compiler configuration below nulls it so it is identified
/// as a GHC boot library.
const PACKAGES: &str = r#"
  waybill-fixture-aeson = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-aeson";
      version = "2.2.4.1";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
      libraryHaskellDepends = [ waybill-fixture-text waybill-fixture-bootlib ];
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-text = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-text";
      version = "2.0.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-oddlic = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-oddlic";
      version = "3.1.4";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = "GPL";
  }) { };
  waybill-fixture-shell = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-shell";
      version = "0.1.8";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-notghc = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-notghc";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-bootlib = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-bootlib";
      version = "1.1.1";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
"#;

/// The compiler configuration. `bootlib` is nulled, which is how milestone
/// 926 identifies a GHC boot library — from the revision's own configuration
/// rather than from a list someone maintains.
const CONFIG: &str = "self: super: {\n  waybill-fixture-bootlib = null;\n}\n";

/// Every GHC series the resolver may ask for when a flake names none.
const SERIES: &[&str] =
    &["9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x"];

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_hackage")
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

/// A stub `nix` answering the closure query for `packages.<host>.default`.
fn stub_script(log: &Path, closure_json: &str) -> String {
    // `r##"` rather than `r#"`: the script contains `*"#packages`, and the
    // `"#` in it closes an `r#"` raw string.
    format!(
        r##"#!/bin/sh
echo "$*" >> '{log}'
case "$*" in
  *builtins.currentSystem*) printf '%s' 'aarch64-darwin' ;;
  "config show"*) echo 'allow-dirty = true'; echo 'allow-import-from-derivation = false' ;;
  "derivation show"*"#packages.aarch64-darwin.default") cat <<'CLOSUREEOF'
{closure_json}
CLOSUREEOF
  ;;
  *"#packages.aarch64-darwin") echo '["default"]' ;;
  *"#packages") echo '["aarch64-darwin"]' ;;
  "derivation show"*) echo "error: flake 'stub' does not provide attribute" >&2; exit 1 ;;
  *) echo 'stub nix: unhandled invocation' >&2; exit 1 ;;
esac
"##,
        log = log.display(),
        closure_json = closure_json,
    )
}

struct Scan {
    stderr: String,
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

impl Scan {
    /// Every component in the CycloneDX document.
    fn components(&self) -> &Vec<Value> {
        self.cdx["components"].as_array().unwrap()
    }

    /// The component with this name, if the document holds one.
    fn component(&self, name: &str) -> Option<&Value> {
        self.components().iter().find(|c| c["name"] == name)
    }

    /// The component with this name; panics with the document's names when
    /// absent, because "no such component" is otherwise an unhelpful failure.
    fn require(&self, name: &str) -> &Value {
        self.component(name).unwrap_or_else(|| {
            let names: Vec<&str> =
                self.components().iter().filter_map(|c| c["name"].as_str()).collect();
            panic!("no component named {name}; document holds {names:?}")
        })
    }

    /// A component's `properties[]` value for a property name.
    fn prop(&self, component: &Value, name: &str) -> Option<String> {
        component["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == name)
            .and_then(|p| p["value"].as_str())
            .map(str::to_string)
    }

    /// Every component carrying a `pkg:hackage` PURL, as (name, version).
    fn hackage_components(&self) -> Vec<(String, String)> {
        self.components()
            .iter()
            .filter(|c| {
                c["purl"].as_str().map(|p| p.starts_with("pkg:hackage/")).unwrap_or(false)
            })
            .map(|c| {
                (
                    c["name"].as_str().unwrap_or_default().to_string(),
                    c["version"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    /// The `dependsOn` targets of a component's PURL, as PURL strings.
    fn depends_on(&self, purl: &str) -> Vec<String> {
        self.cdx["dependencies"]
            .as_array()
            .map(|deps| {
                deps.iter()
                    .filter(|d| d["ref"] == purl)
                    .filter_map(|d| d["dependsOn"].as_array())
                    .flatten()
                    .filter_map(|t| t.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every `ref` and `dependsOn` endpoint in the CDX dependency graph.
    fn all_edge_endpoints(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(deps) = self.cdx["dependencies"].as_array() {
            for d in deps {
                if let Some(r) = d["ref"].as_str() {
                    out.push(r.to_string());
                }
                if let Some(ts) = d["dependsOn"].as_array() {
                    out.extend(ts.iter().filter_map(|t| t.as_str()).map(str::to_string));
                }
            }
        }
        out
    }
}

/// Scan a fixture with the stub `nix` and a seeded package-set cache.
///
/// `cache: None` points the cache at a path that cannot exist, so a test
/// never falls back to the developer's `$HOME` (Principle VII).
fn scan(root: &Path, cache: Option<&Path>, closure_json: &str, extra: &[&str]) -> Scan {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("argv.log");
    std::fs::write(&log, "").unwrap();
    let nix = dir.path().join("nix");
    std::fs::write(&nix, stub_script(&log, closure_json)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = tempfile::tempdir().unwrap();
    let f = |n: &str| out.path().join(n);
    let mut cmd = Command::new(binary_path());
    cmd.args(["sbom", "scan", "--no-deep-hash", "--nix-closure", "--path"])
        .arg(root)
        // Suppress every OTHER network enrichment. `--offline` cannot be used:
        // it also disables the feature under test.
        .args(["--no-deps-dev", "--no-clearly-defined"])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", f("o.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", f("o.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", f("o.spdx3.json").display()))
        .args(extra)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()));
    match cache {
        Some(p) => {
            cmd.env("WAYBILL_NIXPKGS_CACHE", p);
        }
        None => {
            cmd.env("WAYBILL_NIXPKGS_CACHE", dir.path().join("__no_cache__"));
        }
    }
    let o = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "scan failed: {stderr}");
    let read = |n: &str| serde_json::from_slice(&std::fs::read(f(n)).unwrap()).unwrap();
    Scan {
        stderr,
        cdx: read("o.cdx.json"),
        spdx23: read("o.spdx.json"),
        spdx3: read("o.spdx3.json"),
    }
}

/// The common case: a project with a declared dependency, a seeded cache.
fn resolved_scan() -> Scan {
    let cache = seed_cache();
    scan(&fixture("project"), Some(cache.path()), CLOSURE_JSON, &[])
}

const ANN_IDENTITY: &str = "waybill:closure-identity";

// ---------------------------------------------------------------
// US1 — a Haskell package in the closure is identified as one
// ---------------------------------------------------------------

/// C2 / FR-001 / SC-001 — an exact pair built by a GHC gets a Hackage PURL.
#[test]
fn t011_an_exact_pair_built_by_ghc_carries_a_hackage_purl() {
    let s = resolved_scan();
    let c = s.require("waybill-fixture-aeson");
    assert_eq!(
        c["purl"], "pkg:hackage/waybill-fixture-aeson@2.2.4.1",
        "the closure member kept its generic PURL"
    );
    assert_eq!(c["version"], "2.2.4.1", "FR-011: the version must not be rewritten");
}

/// C3 / FR-002 — it carries the revision's license, canonical or not.
#[test]
fn t012_a_confirmed_member_carries_the_revisions_license() {
    let s = resolved_scan();
    let c = s.require("waybill-fixture-aeson");
    let licenses = c["licenses"].as_array().expect("no licenses array");
    let ids: Vec<&str> = licenses
        .iter()
        .filter_map(|l| l["license"]["id"].as_str().or_else(|| l["license"]["name"].as_str()))
        .collect();
    assert!(ids.contains(&"MIT"), "expected MIT, got {ids:?}");

    // FR-002's second half: a declaration that will not canonicalise is
    // PRESERVED, not dropped. Bare "GPL" is the measured shape -- 438 of them
    // at the pinned revision (m1273).
    let odd = s.require("waybill-fixture-oddlic");
    let odd_licenses = odd["licenses"].as_array().expect("no licenses array on oddlic");
    assert!(
        !odd_licenses.is_empty(),
        "an uncanonicalisable license was dropped rather than preserved"
    );
}

/// C4 / FR-003 — it carries the revision's hash, hex, or no hash at all.
#[test]
fn t013_a_confirmed_member_carries_the_revisions_hash() {
    let s = resolved_scan();
    let c = s.require("waybill-fixture-aeson");
    let hashes = c["hashes"].as_array().expect("no hashes array");
    let sha256: Vec<&str> = hashes
        .iter()
        .filter(|h| h["alg"] == "SHA-256")
        .filter_map(|h| h["content"].as_str())
        .collect();
    assert_eq!(sha256.len(), 1, "expected exactly one SHA-256, got {hashes:?}");
    let v = sha256[0];
    assert_eq!(v.len(), 64, "a SHA-256 must be 64 hex characters, got {v:?}");
    assert!(
        v.chars().all(|c| c.is_ascii_hexdigit()),
        "a hash must be hex or absent, never malformed: {v:?}"
    );
}

/// US1 scenario 2 — exactly one component for the reclassified identity.
#[test]
fn t014_reclassification_leaves_exactly_one_component_for_the_identity() {
    let s = resolved_scan();
    let named: Vec<&Value> = s
        .components()
        .iter()
        .filter(|c| c["name"] == "waybill-fixture-aeson")
        .collect();
    assert_eq!(
        named.len(),
        1,
        "expected one component, found {}: {:?}",
        named.len(),
        named.iter().map(|c| &c["purl"]).collect::<Vec<_>>()
    );
    // The count alone passes BEFORE the feature exists, because the single
    // component is the `pkg:generic` one. Asserting which identity survived
    // makes this fail in the before-state for the right reason and still
    // catch a `pkg:generic` twin in the after-state -- which is what US1's
    // second scenario is actually about.
    assert_eq!(named[0]["purl"], "pkg:hackage/waybill-fixture-aeson@2.2.4.1");
}

/// C16 / FR-013 / FR-013a — a closure-derived Hackage component is
/// distinguishable from a manifest-derived one, and stays so through the PURL
/// rewrite.
///
/// The free-by-construction reading is that FR-011 leaves the annotations
/// alone. That is exactly the "satisfied by construction" claim research R7
/// said gets a test anyway, and preservation THROUGH a rewrite is the class of
/// claim #980 showed fails silently.
#[test]
fn t014a_a_reclassified_component_still_says_it_came_from_the_closure() {
    let s = resolved_scan();
    let c = s.require("waybill-fixture-aeson");
    assert_eq!(c["purl"], "pkg:hackage/waybill-fixture-aeson@2.2.4.1");
    assert!(
        s.prop(c, "waybill:closure-role").is_some(),
        "the closure-role annotation did not survive the PURL rewrite"
    );
    assert_eq!(
        s.prop(c, ANN_IDENTITY).as_deref(),
        Some("confirmed"),
        "a closure-derived Hackage component must say the pass confirmed it"
    );
    assert_eq!(c["purl"].as_str().map(|p| p.starts_with("pkg:hackage/")), Some(true));
}
