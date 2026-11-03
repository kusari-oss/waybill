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
/// | `shell` | 5.3p9 | 0.1.8 | `version-disagrees` — the collision, not GHC-built |
/// | `ghcshell` | 7.7.7 | 0.2.0 | `version-disagrees` — a collision that **is** GHC-built, so only the version rule protects it |
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
        "buildInputs": "/nix/store/h1-aeson /nix/store/h2-text /nix/store/h3-shell /nix/store/h4-autoconf /nix/store/h5-noversion /nix/store/h6-notghc /nix/store/h7-oddlic /nix/store/h8-ghcshell"
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
    "d-ghcshell.drv": {
      "name": "waybill-fixture-ghcshell-7.7.7",
      "env": {
        "pname": "waybill-fixture-ghcshell",
        "version": "7.7.7",
        "nativeBuildInputs": "/nix/store/zzz-ghc-9.4.8"
      },
      "outputs": { "out": { "path": "h8-ghcshell" } }
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
/// `waybill-fixture-aeson` declares two targets the closure does not contain:
///
/// - `waybill-fixture-bootlib`, which the compiler configuration below nulls,
///   so milestone 926 identifies it as a GHC boot library. FR-012c says emit
///   it as a versionless compiler-supplied component.
/// - `waybill-fixture-absent`, which is **not** nulled and not in the set at
///   all. There is no basis on which to emit a component for it, so its edge
///   is omitted and counted. Mutation M13 needed this case: with only
///   `bootlib` absent, deleting the boot-library check changed nothing and the
///   check that stops waybill inventing a package from a bare name was
///   untested.
const PACKAGES: &str = r#"
  waybill-fixture-aeson = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-aeson";
      version = "2.2.4.1";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
      libraryHaskellDepends = [ waybill-fixture-text waybill-fixture-bootlib waybill-fixture-absent ];
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
  waybill-fixture-ghcshell = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-ghcshell";
      version = "0.2.0";
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

    /// Both endpoints of every **edge** in the CDX dependency graph.
    ///
    /// CycloneDX emits a `dependencies` entry for every component, so an entry
    /// whose `dependsOn` is empty is a node declaration rather than an edge.
    /// Counting those as endpoints made `t036` fail on `autoconf`, which has
    /// no edges at all and simply appears in the graph as itself.
    fn all_edge_endpoints(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(deps) = self.cdx["dependencies"].as_array() {
            for d in deps {
                let targets: Vec<&str> = d["dependsOn"]
                    .as_array()
                    .map(|ts| ts.iter().filter_map(|t| t.as_str()).collect())
                    .unwrap_or_default();
                if targets.is_empty() {
                    continue;
                }
                if let Some(r) = d["ref"].as_str() {
                    out.push(r.to_string());
                }
                out.extend(targets.into_iter().map(str::to_string));
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

// ---------------------------------------------------------------
// US2 — a name collision does not mint a wrong identity, and says so
// ---------------------------------------------------------------

/// C5 / FR-004 / SC-003 — **the most important test in this milestone.**
///
/// A closure member whose name is in the pinned set at a *different* version
/// keeps `pkg:generic`, and no component anywhere in the document carries a
/// `pkg:hackage` PURL whose version the set does not record for that name.
///
/// In the real world this is GNU `bash 5.3p9` against Hackage's `bash 0.1.8`,
/// `curl 8.20.0` against `1.3.8`, and the compiler `ghc 9.4.8` against a
/// Hackage package called `ghc` at `9.14.1` — 26 of 276 measured members. An
/// over-matching implementation would mint `pkg:hackage/bash@5.3p9`, a
/// coordinate belonging to someone else that a vulnerability feed would then
/// answer for the wrong software.
#[test]
fn t020_a_name_collision_is_never_given_a_hackage_identity() {
    let s = resolved_scan();

    // The collision itself stays generic.
    let c = s.require("waybill-fixture-shell");
    assert_eq!(
        c["purl"], "pkg:generic/waybill-fixture-shell@5.3p9",
        "a name collision was given a Hackage coordinate"
    );

    // A collision that IS GHC-built, so the version rule is the ONLY thing
    // standing between it and a wrong coordinate. Mutation found this gap:
    // with only `shell` in the fixture, relaxing the version comparison to a
    // name match (M4) was still caught -- by the GHC signal, which rejects
    // all 26 real collisions on its own (research R5). So the fixture could
    // not test FR-004 in isolation, and "matching on name alone is forbidden"
    // went unasserted while looking asserted.
    let g = s.require("waybill-fixture-ghcshell");
    assert_eq!(
        g["purl"], "pkg:generic/waybill-fixture-ghcshell@7.7.7",
        "a GHC-built name collision was reclassified: the version rule did not hold"
    );

    // And the document-wide invariant: every `pkg:hackage` component's
    // version is one the pinned set records for that name. Asserted over the
    // whole document rather than over the one member, because the failure
    // this guards against is a rule that over-matches somewhere else.
    let allowed: std::collections::BTreeMap<&str, &str> = [
        ("waybill-fixture-aeson", "2.2.4.1"),
        ("waybill-fixture-text", "2.0.2"),
        ("waybill-fixture-oddlic", "3.1.4"),
        ("waybill-fixture-shell", "0.1.8"),
        ("waybill-fixture-ghcshell", "0.2.0"),
        ("waybill-fixture-notghc", "1.0.0"),
        ("waybill-fixture-bootlib", "1.1.1"),
    ]
    .into_iter()
    .collect();
    for (name, version) in s.hackage_components() {
        // A versionless Hackage component is the compiler-supplied shape
        // (FR-012c) and carries no version to check.
        if version.is_empty() {
            continue;
        }
        match allowed.get(name.as_str()) {
            Some(set_version) => assert_eq!(
                &version, set_version,
                "{name} carries pkg:hackage@{version}, but the pinned set \
                 records {set_version} for that name"
            ),
            None => panic!("{name} carries a pkg:hackage PURL but is not in the pinned set"),
        }
    }
}

/// C6 / FR-005 — identity rests on two independent signals, and disagreement
/// does not mint one.
///
/// The fixture's `notghc` member is an exact `(name, version)` pair whose
/// derivation references no GHC. Real data has never produced this row —
/// measured 0 of 105 — which is exactly why the fixture has to.
#[test]
fn t021_an_exact_pair_without_the_ghc_signal_is_not_reclassified() {
    let s = resolved_scan();
    let c = s.require("waybill-fixture-notghc");
    assert_eq!(
        c["purl"], "pkg:generic/waybill-fixture-notghc@1.0.0",
        "an exact pair was reclassified on the package set alone"
    );
    assert_eq!(
        s.prop(c, ANN_IDENTITY).as_deref(),
        Some("signals-disagree"),
        "the disagreement must be recorded, not merely acted on"
    );
}

/// C9 / FR-008 / SC-005 — no two components share one identity.
///
/// Research R7 says this holds by construction: `merge_closure_components`
/// keys on `(name, version)` and reclassification changes neither. "Satisfied
/// by construction" is a claim about code that can change, so it gets a test.
#[test]
fn t022_no_two_components_share_one_identity() {
    let s = resolved_scan();
    let mut seen: std::collections::BTreeMap<String, usize> = Default::default();
    for c in s.components() {
        if let Some(p) = c["purl"].as_str() {
            *seen.entry(p.to_string()).or_default() += 1;
        }
    }
    let dupes: Vec<(&String, &usize)> = seen.iter().filter(|(_, n)| **n > 1).collect();
    assert!(dupes.is_empty(), "components sharing a PURL: {dupes:?}");

    // Also by (name, version), which is what the merge keys on: two
    // components with one identity under different PURLs is the same defect
    // wearing a disguise.
    let mut by_identity: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for c in s.components() {
        let key = (
            c["name"].as_str().unwrap_or_default().to_string(),
            c["version"].as_str().unwrap_or_default().to_string(),
        );
        *by_identity.entry(key).or_default() += 1;
    }
    let id_dupes: Vec<(&(String, String), &usize)> =
        by_identity.iter().filter(|(_, n)| **n > 1).collect();
    assert!(id_dupes.is_empty(), "components sharing (name, version): {id_dupes:?}");
}

/// US2 scenario 2 — the collision is auditable rather than silent.
///
/// A same-named Hackage package exists and was deliberately not used, and the
/// document says so. Without this the decision is invisible: the component
/// looks identical to one nothing was ever attempted for.
#[test]
fn t023_a_collision_records_that_it_was_considered_and_declined() {
    let s = resolved_scan();
    let c = s.require("waybill-fixture-shell");
    assert_eq!(
        s.prop(c, ANN_IDENTITY).as_deref(),
        Some("version-disagrees"),
        "a declined collision must say a same-named package existed"
    );
    // The value must distinguish this from "nothing of that name exists",
    // which calls for entirely different operator action.
    assert_ne!(s.prop(c, ANN_IDENTITY).as_deref(), Some("not-in-package-set"));
}

// ---------------------------------------------------------------
// US3 — an unresolved closure member says why
// ---------------------------------------------------------------

/// C8 / FR-007 / SC-004 — **every** remaining `pkg:generic` closure member
/// carries a reason.
///
/// Asserted as a count equal to the population, not as "some reasons exist".
/// A check that did not run looks identical to a check that passed, and 190 of
/// 276 real members are correctly generic — an operator reading one cannot
/// otherwise tell "waybill looked and this is not a Haskell package" from
/// "waybill did not look".
#[test]
fn t028_every_generic_closure_member_carries_a_reason() {
    let s = resolved_scan();
    let generic: Vec<&Value> = s
        .components()
        .iter()
        .filter(|c| {
            c["purl"].as_str().map(|p| p.starts_with("pkg:generic/")).unwrap_or(false)
                && s.prop(c, "waybill:closure-role").is_some()
        })
        .collect();
    assert!(!generic.is_empty(), "the fixture produced no generic closure members");

    let explained: Vec<&&Value> =
        generic.iter().filter(|c| s.prop(c, ANN_IDENTITY).is_some()).collect();
    let unexplained: Vec<&str> = generic
        .iter()
        .filter(|c| s.prop(c, ANN_IDENTITY).is_none())
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(
        explained.len(),
        generic.len(),
        "{} of {} generic closure members carry no reason: {unexplained:?}",
        generic.len() - explained.len(),
        generic.len()
    );
}

/// FR-007 — the five outcomes are distinguishable from each other.
///
/// "Carries a reason" is satisfied by a single constant string; the
/// requirement is that an operator can tell the outcomes apart, because they
/// call for different action. `no-version-recorded` in particular must not
/// collapse into a failed lookup: the lookup was *impossible*.
#[test]
fn t029_the_five_outcomes_are_distinguishable_on_the_wire() {
    let s = resolved_scan();
    let outcome = |name: &str| s.prop(s.require(name), ANN_IDENTITY);

    assert_eq!(outcome("waybill-fixture-aeson").as_deref(), Some("confirmed"));
    assert_eq!(outcome("waybill-fixture-shell").as_deref(), Some("version-disagrees"));
    assert_eq!(outcome("waybill-fixture-notghc").as_deref(), Some("signals-disagree"));
    assert_eq!(outcome("waybill-fixture-autoconf").as_deref(), Some("not-in-package-set"));
    assert_eq!(
        outcome("waybill-fixture-noversion").as_deref(),
        Some("no-version-recorded"),
        "a member nix recorded no version for must say the lookup was impossible"
    );

    // All five distinct, so none of the above passes by coincidence.
    let seen: std::collections::BTreeSet<String> = [
        "waybill-fixture-aeson",
        "waybill-fixture-shell",
        "waybill-fixture-notghc",
        "waybill-fixture-autoconf",
        "waybill-fixture-noversion",
    ]
    .iter()
    .filter_map(|n| outcome(n))
    .collect();
    assert_eq!(seen.len(), 5, "outcomes are not distinct: {seen:?}");
}

/// C10 / FR-009 — an unavailable package set degrades; it does not fail.
///
/// Asserted on a positive signal: the recorded `package-set-unavailable`
/// value. Asserting the *absence* of Hackage PURLs would also pass if the
/// pass had crashed, which is the opposite of what this checks.
#[test]
fn t030_an_unavailable_package_set_degrades_and_says_so() {
    // The `closure_only` fixture, deliberately, and the reason is hermeticity.
    //
    // With `project` and no cache this test FAILED with `not-in-package-set`,
    // and the verdict was right: `project` has a `.cabal`, so `enrich` runs,
    // and `enrich` is not offline -- it fetched the real 16 MB
    // `hackage-packages.nix` from the pinned revision over the network and
    // handed the pass a genuine package set in which no `waybill-fixture-*`
    // name appears. Correct behaviour, a non-hermetic test, and not the
    // requirement under test.
    //
    // `closure_only` has no `.cabal`, so `is_enrichable` finds nothing and
    // `enrich` returns before any retrieval. The pass then loads the set
    // cache-only, misses, and degrades -- which is the FR-009 path, reached
    // without touching the network.
    let s = scan(&fixture("closure_only"), None, CLOSURE_JSON, &[]);

    let c = s.require("waybill-fixture-aeson");
    assert_eq!(
        c["purl"], "pkg:generic/waybill-fixture-aeson@2.2.4.1",
        "a member was reclassified with no package set to reclassify it from"
    );
    assert_eq!(
        s.prop(c, ANN_IDENTITY).as_deref(),
        Some("package-set-unavailable"),
        "the document must record that the pass did not run"
    );

    // And the scan succeeded -- `scan` already asserts a zero exit, so
    // reaching here at all is that assertion. What is checked here is that
    // the document is whole rather than truncated.
    assert!(!s.components().is_empty());
    assert!(s.spdx23["packages"].as_array().map(|p| !p.is_empty()).unwrap_or(false));
    assert!(s.spdx3["@graph"].is_array() || s.spdx3["element"].is_array());
}

/// Research R2 — the pass must NOT inherit `enrich`'s gates.
///
/// `enrich` returns `None` before reading anything when no component is
/// enrichable, and `is_enrichable` is `is_haskell(c) && c.version.is_empty()`.
/// A repository whose only Haskell content arrives through the derivation
/// closure therefore gets no enrichment at all — and research R2 is explicit
/// that this is the scan #1282 is about, so a pass that inherited the gate
/// would skip exactly the documents the feature exists to fix.
///
/// Nothing else in the suite covers this: every other test uses the `project`
/// fixture, where `enrich` runs and hands its set over. Here `enrich` declines
/// and the pass must load the set from the per-revision cache itself.
#[test]
fn t030b_the_pass_runs_where_the_enrichment_declines_to() {
    let cache = seed_cache();
    let s = scan(&fixture("closure_only"), Some(cache.path()), CLOSURE_JSON, &[]);

    let c = s.require("waybill-fixture-aeson");
    assert_eq!(
        c["purl"], "pkg:hackage/waybill-fixture-aeson@2.2.4.1",
        "the pass skipped a closure-only scan — it inherited enrich's gate"
    );
    assert_eq!(s.prop(c, ANN_IDENTITY).as_deref(), Some("confirmed"));

    // And the license and hash came from the cache, with no `enrich` to have
    // loaded it: the cache-only load is doing real work, not returning an
    // empty set that happens to produce the same `pkg:generic` output.
    assert!(
        !c["licenses"].as_array().map(Vec::is_empty).unwrap_or(true),
        "no license: the cache-only load produced an empty set"
    );
    assert!(
        !c["hashes"].as_array().map(Vec::is_empty).unwrap_or(true),
        "no hash: the cache-only load produced an empty set"
    );
}

// ---------------------------------------------------------------
// FR-012 — declared dependency edges
// ---------------------------------------------------------------

/// C11 / FR-012 / SC-005a — a reclassified member carries out-edges from the
/// dependencies the pinned set **declares**, and every target is a component
/// present in the document.
///
/// Measured on the real closure: 278 declared, 265 resolving by name. Today
/// zero closure-appended component carries any out-edge.
#[test]
fn t035_a_reclassified_member_carries_declared_out_edges() {
    let s = resolved_scan();
    let aeson = "pkg:hackage/waybill-fixture-aeson@2.2.4.1";
    let targets = s.depends_on(aeson);
    assert!(
        !targets.is_empty(),
        "a confirmed member carries no out-edges; document deps: {:?}",
        s.cdx["dependencies"]
    );
    assert!(
        targets.iter().any(|t| t.starts_with("pkg:hackage/waybill-fixture-text")),
        "the declared dependency on text is missing; got {targets:?}"
    );

    // Every endpoint in the graph resolves to a component in the document
    // (invariant I2). A dangling target validates in CycloneDX, so this is
    // asserted rather than assumed.
    let purls: std::collections::BTreeSet<&str> =
        s.components().iter().filter_map(|c| c["purl"].as_str()).collect();
    let root = s.cdx["metadata"]["component"]["purl"].as_str().unwrap_or_default();
    for e in s.all_edge_endpoints() {
        assert!(
            purls.contains(e.as_str()) || e == root,
            "edge endpoint {e} is not a component in the document"
        );
    }
}

/// C12 / FR-012a / SC-005b — no `dependsOn` edge is derived from a store
/// reference.
///
/// Milestone 1066 decided a store reference drives a component's *role* and
/// nothing else, and that decision stands: the two facts travel in two
/// carriers so neither misrepresents the other. The fixture's root references
/// every member through `buildInputs`, so if store references produced edges
/// the root would depend on all of them — including `autoconf`, which the
/// package set declares nothing about.
#[test]
fn t036_no_edge_is_derived_from_a_store_reference() {
    let s = resolved_scan();

    // `autoconf` is referenced by the root derivation's `buildInputs` and is
    // declared by nothing in the package set. An edge to it could only have
    // come from a store reference.
    let autoconf = s.require("waybill-fixture-autoconf")["purl"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    for e in s.all_edge_endpoints() {
        assert_ne!(
            e, autoconf,
            "an edge names a component only a store reference could have reached"
        );
    }

    // And `shell`, the collision: also store-referenced, also undeclared.
    let shell = s.require("waybill-fixture-shell")["purl"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        !s.all_edge_endpoints().contains(&shell),
        "a store-referenced collision gained an edge"
    );
}

/// C13 / FR-012b — an unreclassified member gains no edges.
///
/// Edges follow the declaration, and a member the set does not identify has no
/// declaration to follow. Inventing one from its store references is what C12
/// forbids.
#[test]
fn t037_an_unreclassified_member_gains_no_edges() {
    let s = resolved_scan();
    for name in [
        "waybill-fixture-shell",
        "waybill-fixture-ghcshell",
        "waybill-fixture-notghc",
        "waybill-fixture-autoconf",
        "waybill-fixture-noversion",
    ] {
        let purl = s.require(name)["purl"].as_str().unwrap_or_default().to_string();
        assert!(
            s.depends_on(&purl).is_empty(),
            "{name} is not reclassified but carries out-edges: {:?}",
            s.depends_on(&purl)
        );
    }
}

/// C14 / FR-012c — a declared dependency the closure does not contain becomes
/// a versionless compiler-supplied component.
///
/// Neither dangling nor dropped. Measured: 13 of 278 targets absent, and all
/// three distinct ones (`directory` ×11, `ghc-heap`, `pretty`) are GHC boot
/// libraries — the population of #1340 and #966. The fixture's `bootlib` is
/// declared by `aeson`, absent from the closure, and nulled in the compiler
/// configuration, which is how milestone 926 identifies a boot library.
#[test]
fn t038_a_dangling_target_becomes_a_compiler_supplied_component() {
    let s = resolved_scan();
    let boot = s.require("waybill-fixture-bootlib");

    assert_eq!(
        boot["version"].as_str().unwrap_or_default(),
        "",
        "a compiler-supplied component must carry no version"
    );
    assert_eq!(
        boot["purl"], "pkg:hackage/waybill-fixture-bootlib",
        "expected a versionless Hackage PURL"
    );

    // The versionless component must explain its absent version or it fails
    // the `versionless_explained` invariant. C170's `compiler-supplied` is
    // exactly that value.
    let reason = s
        .prop(boot, "waybill:haskell-version-unresolved-reason")
        .or_else(|| s.prop(boot, "waybill:unresolved-reason"))
        .or_else(|| s.prop(boot, "waybill:version-absence"));
    assert_eq!(
        reason.as_deref(),
        Some("compiler-supplied"),
        "a versionless component with no accepted reason fails the \
         versionless_explained invariant"
    );

    // And the edge to it exists rather than having been dropped.
    let targets = s.depends_on("pkg:hackage/waybill-fixture-aeson@2.2.4.1");
    assert!(
        targets.iter().any(|t| t == "pkg:hackage/waybill-fixture-bootlib"),
        "the declared edge to the boot library was dropped; got {targets:?}"
    );
}

/// FR-012c's negative half — a declared target that is absent and is **not**
/// a boot library gets no component invented for it.
///
/// Emitting one would assert a package exists on the strength of a name
/// appearing in another package's dependency list, which is the same class of
/// claim FR-004 forbids for identity. The edge is omitted and counted, so the
/// omission is reported rather than silent (#1218).
#[test]
fn t038b_an_absent_non_boot_target_gets_no_invented_component() {
    let s = resolved_scan();
    assert!(
        s.component("waybill-fixture-absent").is_none(),
        "a component was invented for a name that is only a dependency string"
    );
    assert!(
        !s.all_edge_endpoints().iter().any(|e| e.contains("waybill-fixture-absent")),
        "an edge points at a component that does not exist"
    );
    // The boot library beside it IS emitted, so this is not passing because
    // the whole dangling-target path is dead.
    assert!(
        s.component("waybill-fixture-bootlib").is_some(),
        "the boot-library path did not run, so this test proves nothing"
    );

    // And the omission is REPORTED, not silent. Mutation M12 — deleting the
    // counter — survived until this assertion existed, because the counter is
    // a diagnostic rather than a document fact and nothing looked at it. A
    // count that quietly omits edges is the shape #1218 was about, so
    // "reported" has to be asserted somewhere or it is only intended.
    // Matched on the message rather than the field: tracing wraps field
    // names in ANSI escapes, so `dangling_non_boot=1` never appears as a
    // plain substring. The message is enough — M12 leaves the counter at
    // zero, so the `> 0` guard never fires and this line is never printed.
    assert!(
        s.stderr.contains("their edges are omitted because emitting a"),
        "the omitted edge was not reported on stderr; got: {}",
        s.stderr
    );
}

/// C15 / #980 — the PURL rewrite carried its edges, asserted **directly**.
///
/// Not inferred from an edge count. #980's failure mode is silent in all three
/// formats: CycloneDX keeps an edge pointing at the old PURL, SPDX drops the
/// relationship outright, and neither errors — so a count can match while the
/// graph is orphaned.
#[test]
fn t039_no_relationship_refers_to_a_replaced_purl() {
    let s = resolved_scan();

    // Every PURL this pass replaced, by construction: a reclassified member's
    // generic coordinate.
    let replaced: Vec<String> = ["waybill-fixture-aeson@2.2.4.1",
                                 "waybill-fixture-text@2.0.2",
                                 "waybill-fixture-oddlic@3.1.4"]
        .iter()
        .map(|nv| format!("pkg:generic/{nv}"))
        .collect();

    for old in &replaced {
        assert!(
            !s.all_edge_endpoints().contains(old),
            "a relationship still refers to {old}, a PURL this pass replaced"
        );
        assert!(
            !s.components().iter().any(|c| c["purl"].as_str() == Some(old.as_str())),
            "{old} is still a component; the rewrite did not happen"
        );
    }

    // The same assertion in SPDX 2.3, where the failure mode is a DROPPED
    // relationship rather than a stale one -- so it is checked by counting
    // the graph rather than by looking for the old identifier.
    let rel_count = s.spdx23["relationships"].as_array().map(Vec::len).unwrap_or(0);
    assert!(rel_count > 0, "SPDX 2.3 carries no relationships at all");
}

/// T045 — the three formats agree about the dependency graph.
///
/// The cheapest check that catches a fabricated or orphaned edge, and it found
/// two defects in milestone 866. #980's failure mode is asymmetric by
/// construction: CycloneDX keeps an edge pointing at a replaced PURL while
/// SPDX **drops** the relationship, so a count that matches across formats is
/// evidence neither happened, and one that does not is evidence one did.
#[test]
fn t045_the_three_formats_agree_about_the_declared_edges() {
    let s = resolved_scan();
    let aeson = "pkg:hackage/waybill-fixture-aeson@2.2.4.1";

    // CycloneDX: the member's own `dependsOn` set.
    let cdx: std::collections::BTreeSet<String> = s.depends_on(aeson).into_iter().collect();
    assert!(!cdx.is_empty(), "CycloneDX carries no edges for the member");

    // SPDX 2.3: DEPENDS_ON relationships whose source is the member's
    // package. The SPDXID is content-derived, so the package is located by
    // its PURL in `externalRefs` rather than by guessing the identifier.
    let spdx_id = s.spdx23["packages"]
        .as_array()
        .and_then(|ps| {
            ps.iter().find(|p| {
                p["externalRefs"]
                    .as_array()
                    .map(|rs| rs.iter().any(|r| r["referenceLocator"] == aeson))
                    .unwrap_or(false)
            })
        })
        .and_then(|p| p["SPDXID"].as_str())
        .map(str::to_string)
        .expect("the member has no SPDX 2.3 package");

    let spdx_edges = s.spdx23["relationships"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .filter(|r| {
                    r["spdxElementId"] == spdx_id.as_str()
                        && r["relationshipType"] == "DEPENDS_ON"
                })
                .count()
        })
        .unwrap_or(0);

    assert_eq!(
        spdx_edges,
        cdx.len(),
        "CycloneDX reports {} out-edges for the member and SPDX 2.3 reports {spdx_edges}; \
         an asymmetry here is #980's signature",
        cdx.len()
    );
}

/// The synthesised boot-library component satisfies the corpus I7 invariant.
///
/// `orphan_explained` requires every component unreachable from the root to
/// carry one of a closed set of markers. FR-012c's components are unreachable
/// **by construction** — nothing depends on a boot library except the closure
/// members that are themselves unreachable from the root — and they carry no
/// `waybill:closure-role`, because they are not closure members: nix never put
/// them in the closure.
///
/// Milestone 1275 therefore added C170 to that marker set, in
/// `generate/orphan_reason.rs::already_explained` and in the invariant's own
/// `is_explained`, which its comment requires to mirror each other. This test
/// is the guard at the document level: if the component stops carrying an
/// accepted marker, `nix-closure-moat` gains three I7 offenders where it has
/// none, and the tempting fix becomes declaring a defect this milestone
/// created rather than fixing it.
#[test]
fn t056_a_synthesised_boot_component_carries_an_accepted_orphan_marker() {
    let s = resolved_scan();
    let boot = s.require("waybill-fixture-bootlib");

    // The marker set `orphan_explained::is_explained` accepts, verbatim.
    let accepted = [
        ("waybill:orphan-reason", None),
        ("waybill:version-absence", Some("by-construction")),
        ("waybill:component-tier", Some("file")),
        ("waybill:unresolved-reason", None),
        ("waybill:closure-role", None),
        ("waybill:haskell-version-unresolved-reason", None),
    ];
    let satisfied: Vec<&str> = accepted
        .iter()
        .filter(|(k, want)| match (s.prop(boot, k), want) {
            (Some(v), Some(w)) => v == *w,
            (Some(_), None) => true,
            (None, _) => false,
        })
        .map(|(k, _)| *k)
        .collect();
    assert!(
        !satisfied.is_empty(),
        "the synthesised boot component carries no accepted orphan marker, so it \
         is unreachable-and-unexplained (I7); it has: {:?}",
        boot["properties"]
    );
    assert!(
        satisfied.contains(&"waybill:haskell-version-unresolved-reason"),
        "expected C170 to be the marker; got {satisfied:?}"
    );

    // And it really is unreachable: nothing in the document depends on it
    // except reclassified closure members, which are not reachable from the
    // root either. If this stops holding the test above is still correct but
    // is no longer guarding anything, and the comment should change with it.
    let root = s.cdx["metadata"]["component"]["purl"].as_str().unwrap_or_default();
    assert!(
        !s.depends_on(root).iter().any(|t| t == "pkg:hackage/waybill-fixture-bootlib"),
        "the root now depends on a boot library; this test's premise has changed"
    );
}
