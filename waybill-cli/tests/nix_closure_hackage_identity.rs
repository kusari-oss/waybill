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
