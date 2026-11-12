//! Milestone 1423 (#1423) — a `--nix-closure` document's not-in-artifact
//! marks come from the evaluated output's runtime closure, plus static
//! linking, rather than from the build-graph role.
//!
//! Measured before this milestone: 951 grype findings sat on components
//! outside the runtime closure across five targets, and only 175 of them were
//! on marked components. The role is a property of the whole build graph, so
//! a library *any* derivation links read as runtime (curl's closure presented
//! python3, libxml2 and pcre2 that way).
//!
//! Every assertion is on the emitted documents, in all three formats, through
//! the shared stub `nix` (`common/nix_stub.rs`), which answers
//! `nix path-info -r --json` from a listing the test supplies and makes no network
//! access. Every name is `waybill-fixture-*` (#285, #640).
//!
//! # Fixtures (`tests/fixtures/nix_closure_runtime/`)
//!
//! | file | shape | what it exercises |
//! |---|---|---|
//! | `closure-runtime.json` | curl / openssh | `xml` is an `artifact-input` the output does not reference (marked); `dnsroot` is `build-tooling` but referenced (unmarked); `bootlib` is a compiler-supplied boot library |
//! | `closure-static.json` | shellcheck / ormolu | a static Haskell executable whose libraries are in its static build's `buildInputs` and propagate; `hspec` is a test-only dependency. The wrapper root names the static build only in `inputs.drvs`, so under US2 the walk reaches only the root |
//! | `closure-static-namesake.json` | pandoc-cli / cachix (#1459) | a static Haskell executable linking Haskell `zlib`, which shares its name with a build-only C `zlib`, and `fsnotify`, whose derivation propagates `hfsevents` while the package set declares something else |
//! | `closure-embedded.json` | gh | Go modules read from a runtime binary and from a build tool's binary |
//! | `closure-walk.json` | US2 | `perl` is `both` but reached only through a reached member's `nativeBuildInputs`; `xml` is an `artifact-input` of the build tool only |
//! | `closure-walk-libc.json` | US2, #1467 | the app and `ssl` reach the libc only through their `stdenv` → C compiler wrapper → `libc_lib`; a build tool's own stdenv names another libc; `unistring` is built by a bootstrap stage whose compiler names the libc the final one was built from |
//! | `closure-tworoots.json` | US2 | two unconsumed derivations, so no evaluated package to walk from |
//! | `closure-outputs.json` | postgrest (#1458) | `closure-runtime.json` with the app's `outputs` headed by `out`, the library, while the attribute installs `bin`; each output gets its own `path-info` listing |
//! | `closure-tests-library.json` | slack-web / openai (#1283) | the evaluated package is the project (`project-tests`), a library built with its test suite: `hspec` is in its `buildInputs`, its library dependency `aeson` in `propagatedBuildInputs`; `aeson`'s own test dependency is in `aeson`'s `buildInputs`, and `text`, which propagates nothing (hashable's shape), has `tasty` in its own |
//! | `closure-doc-outputs.json` | hlint / moat on aarch64-darwin (#1455) | members whose only runtime paths are their own `doc` or `devdoc` output, or a path only a `doc`, `man`, `info` or `devdoc` output references; a member referenced by a documentation output and by an `out`; a package named `-doc` whose output is `out` |
//! | `closure-lib-links.json` | moat on aarch64-darwin (#1455, second rule) | an app whose `lib/links` farm points at a library its dylib loads, one loaded only by that library (dyld's load-chain rpaths), one named in `propagated-build-inputs`, and a test framework (with its own farm) that only the farm names; the files are built by `lib_links_store` |
//! | `closure-lib-links-haskell.json` | #1479 review | `closure-lib-links.json` with the app and `decl` as Haskell libraries, so the static-link set, seeded from the shipping app, follows its `propagatedBuildInputs` |
//! | `closure-tests-executable.json` | slacklinker (#1283) | the same project as an executable-only build: `aeson` and `hspec` share `buildInputs`, and `hspec` was built at a version the package set disagrees with |
//!
//! US4 reuses `closure-runtime.json`, adding a CVE-named patch to one member
//! the recipe keeps and one it drops (`us4_closure`).

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
mod sbom_edit_support;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const C203: &str = "waybill:scope-classification-source";
const C216: &str = "waybill:closure-scope-method";
const SUBSTITUTER: &str = "https://cache.waybill-fixture.test";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_runtime")
}

fn fixture(name: &str) -> String {
    let path = dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A runtime listing, one store path per line. The stub prints it as
/// `nix path-info -r --json` does, with the references the test supplies.
fn listing(basenames: &[&str]) -> String {
    basenames.iter().map(|b| format!("/nix/store/{b}")).collect::<Vec<_>>().join("\n")
}

/// curl's shape: the app's `bin` output, two libraries it links, the
/// `build-tooling` member it references at runtime, and a darwin system path
/// no closure member produces.
fn curl_runtime() -> String {
    listing(&["h-app-bin", "h-ssl", "h-z", "h-dns", "zz-libSystem-1"])
}

/// The compiler configuration: `bootlib` nulled, which is how milestone 926
/// identifies a GHC boot library.
const BOOT_CONFIG: &str = "self: super: {\n  waybill-fixture-bootlib = null;\n}\n";

struct Run {
    scan: Scan,
}

fn scan_with(stub: NixStub<'_>) -> Run {
    Run { scan: stub.scan(&["--nix-closure", "--no-deps-dev"]) }
}

fn base<'a>(closure: &'a str) -> NixStub<'a> {
    NixStub { project: dir().join("project"), closure, ..Default::default() }
}

// ---- reading the three documents ------------------------------------------

impl Run {
    fn cdx_components(&self) -> Vec<&Value> {
        fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
            if let Some(cs) = v["components"].as_array() {
                for c in cs {
                    out.push(c);
                    walk(c, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.scan.cdx, &mut out);
        out
    }

    fn cdx(&self, name: &str) -> &Value {
        self.cdx_components()
            .into_iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| {
                let names: Vec<String> = self
                    .cdx_components()
                    .iter()
                    .filter_map(|c| c["name"].as_str().map(str::to_string))
                    .collect();
                panic!("no component named {name}; the document holds {names:?}")
            })
    }

    /// Names CycloneDX marks `scope: excluded`.
    fn cdx_marked(&self) -> BTreeSet<String> {
        self.cdx_components()
            .into_iter()
            .filter(|c| c["scope"] == "excluded")
            .filter_map(|c| c["name"].as_str().map(str::to_string))
            .collect()
    }

    /// Names on the member side of every SPDX 2.3 `BUILD_TOOL_OF` edge.
    fn spdx23_marked(&self) -> BTreeSet<String> {
        let names: HashMap<&str, &str> = self.scan.spdx23["packages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| Some((p["SPDXID"].as_str()?, p["name"].as_str()?)))
            .collect();
        self.scan.spdx23["relationships"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["relationshipType"] == "BUILD_TOOL_OF")
            .filter_map(|r| names.get(r["spdxElementId"].as_str()?).map(|n| n.to_string()))
            .collect()
    }

    /// Names on the `to` side of every SPDX 3 `usesTool` relationship scoped
    /// `build`.
    fn spdx3_marked(&self) -> BTreeSet<String> {
        let graph = self.scan.spdx3["@graph"].as_array().unwrap();
        let names: HashMap<&str, &str> = graph
            .iter()
            .filter_map(|e| Some((e["spdxId"].as_str()?, e["name"].as_str()?)))
            .collect();
        graph
            .iter()
            .filter(|e| e["type"] == "LifecycleScopedRelationship")
            .filter(|e| e["relationshipType"] == "usesTool" && e["scope"] == "build")
            .flat_map(|e| e["to"].as_array().cloned().unwrap_or_default())
            .filter_map(|t| t.as_str().and_then(|t| names.get(t)).map(|n| n.to_string()))
            .collect()
    }

    /// The marked set over the fixture's own closure members and modules,
    /// asserted equal across the three formats (FR-009).
    ///
    /// Restricted to fixture names because the flake input itself (`nixpkgs`,
    /// from `flake.lock`) carries CDX `scope: excluded` through a different
    /// mechanism that SPDX does not express as a tool relationship; that is
    /// #1428's population, not this feature's.
    fn marked(&self) -> BTreeSet<String> {
        let ours = |s: &BTreeSet<String>| -> BTreeSet<String> {
            s.iter()
                .filter(|n| n.starts_with("waybill-fixture-") || n.starts_with("example.test/"))
                .cloned()
                .collect()
        };
        let cdx = ours(&self.cdx_marked());
        assert_eq!(ours(&self.spdx23_marked()), cdx, "SPDX 2.3 BUILD_TOOL_OF set vs CDX scope:excluded");
        assert_eq!(ours(&self.spdx3_marked()), cdx, "SPDX 3 usesTool/build set vs CDX scope:excluded");
        cdx
    }

    fn cdx_prop<'a>(c: &'a Value, key: &str) -> Option<&'a str> {
        c["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == key)
            .and_then(|p| p["value"].as_str())
    }

    /// C203 per component name, in each format, asserted equal.
    fn c203(&self, name: &str) -> Option<String> {
        let cdx = Self::cdx_prop(self.cdx(name), C203).map(str::to_string);
        let spdx23 = self.spdx23_envelope(name, C203);
        let spdx3 = self.spdx3_envelope(name, C203);
        assert_eq!(spdx23, cdx, "C203 on {name}: SPDX 2.3 vs CDX");
        assert_eq!(spdx3, cdx, "C203 on {name}: SPDX 3 vs CDX");
        cdx
    }

    fn spdx23_envelope(&self, name: &str, field: &str) -> Option<String> {
        self.scan.spdx23["packages"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == name)?["annotations"]
            .as_array()?
            .iter()
            .filter_map(|a| a["comment"].as_str())
            .find_map(|raw| envelope(raw, field))
    }

    fn spdx3_envelope(&self, name: &str, field: &str) -> Option<String> {
        let graph = self.scan.spdx3["@graph"].as_array()?;
        let id = graph.iter().find(|e| e["name"] == name)?["spdxId"].as_str()?.to_string();
        graph
            .iter()
            .filter(|e| e["type"] == "Annotation")
            .filter(|e| e["subject"].as_str() == Some(id.as_str()))
            .filter_map(|e| e["statement"].as_str())
            .find_map(|raw| envelope(raw, field))
    }

    /// C216 in each format, asserted equal, parsed.
    fn c216(&self) -> Option<Value> {
        let cdx = self.scan.cdx["metadata"]["properties"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["name"] == C216))
            .and_then(|p| p["value"].as_str())
            .map(str::to_string);
        let spdx23 = self.scan.spdx23["annotations"]
            .as_array()
            .and_then(|a| a.iter().filter_map(|x| x["comment"].as_str()).find_map(|r| envelope(r, C216)));
        let spdx3 = self.scan.spdx3["@graph"].as_array().and_then(|g| {
            g.iter()
                .filter(|e| e["type"] == "Annotation")
                .filter_map(|e| e["statement"].as_str())
                .find_map(|r| envelope(r, C216))
        });
        assert_eq!(spdx23, cdx, "C216: SPDX 2.3 vs CDX");
        assert_eq!(spdx3, cdx, "C216: SPDX 3 vs CDX");
        cdx.map(|s| serde_json::from_str(&s).unwrap())
    }

    /// Every component carrying a closure role, by name.
    fn closure_members(&self) -> BTreeSet<String> {
        self.cdx_components()
            .into_iter()
            .filter(|c| Self::cdx_prop(c, "waybill:closure-role").is_some())
            .filter_map(|c| c["name"].as_str().map(str::to_string))
            .collect()
    }

    fn nix_log(&self) -> &str {
        &self.scan.nix_log
    }
}

/// The value of a milestone-071 annotation envelope, when it carries `field`.
fn envelope(raw: &str, field: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    (v.get("field")?.as_str()? == field).then(|| v.get("value")?.as_str().map(str::to_string))?
}

fn names(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// ---- US1: the runtime closure decides (FR-001, FR-003, FR-004, FR-007) ----

/// FR-001 in all three formats: members outside the runtime closure are
/// marked, members inside it are not, whatever their role.
#[test]
fn us1_runtime_members_unmarked_and_others_marked() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub { runtime: Some(curl_runtime()), ..base(&closure) });
    assert_eq!(
        run.marked(),
        names(&[
            // An `artifact-input` the output does not reference: python3,
            // libxml2 and pcre2 on curl.
            "waybill-fixture-xml",
            "waybill-fixture-py",
            // Not a Hackage component without the package set, so not
            // compiler-supplied, and not referenced.
            "waybill-fixture-bootlib",
        ]),
        "stderr:\n{}",
        run.scan.stderr
    );
    for n in ["waybill-fixture-ssl", "waybill-fixture-z", "waybill-fixture-xml", "waybill-fixture-py"] {
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
}

/// FR-001 in both directions, on the shape of openssh's `dns-root-data`:
/// the role says `build-tooling`, the runtime closure references it, and the
/// runtime closure wins. The control is the same scan without a runtime
/// answer, where the role rule still marks it.
#[test]
fn us1_build_tooling_member_inside_runtime_closure_is_unmarked() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub { runtime: Some(curl_runtime()), ..base(&closure) });
    let dns = run.cdx("waybill-fixture-dnsroot");
    assert_eq!(Run::cdx_prop(dns, "waybill:closure-role"), Some("build-tooling"));
    assert!(!run.marked().contains("waybill-fixture-dnsroot"), "referenced, so it ships");
    assert_eq!(run.c203("waybill-fixture-dnsroot").as_deref(), Some("runtime-closure"));

    // CONTROL: no store holds the output, so the walk decides, and it reaches
    // `dnsroot` only through `nativeBuildInputs`, so it marks it.
    let control = scan_with(base(&closure));
    assert_eq!(
        control.c216().expect("C216")["unavailable-reason"],
        "not-in-any-store",
        "the stub answered `path '…' is not valid` from every store"
    );
    assert!(
        control.marked().contains("waybill-fixture-dnsroot"),
        "without a runtime answer the walk marks it -- otherwise this test \
         could not tell the runtime closure clearing a mark from no mark at all"
    );
}

#[test]
fn us1_local_store_answer_records_store_local() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub { runtime: Some(curl_runtime()), ..base(&closure) });
    let c216 = run.c216().expect("C216 present on a closure document");
    assert_eq!(
        c216,
        serde_json::json!({
            // The runtime closure lists the C libraries itself (#1467).
            "c-runtime-libraries": null,
            // Nothing in this closure is reached only through a documentation
            // output (#1455).
            "doc-only-excluded": {"members": 0, "output-names": ["devdoc", "doc", "info", "man"], "paths": 0},
            // No output in this store has a lib/links folder (#1455).
            "lib-links-excluded": {"members": 0, "not-assessed": null, "paths": 0, "rule": "unloaded-lib-links"},
            "method": "runtime-closure",
            // The stub refuses the output selection, so the default output
            // was queried and the record says so (#1458).
            "outputs": ["bin"],
            "outputs-source": "default-output",
            "schema": 1,
            "static-c-linking-detected": false,
            "static-link-roots": {"declared-edges": 0, "derivation-fields": 0},
            "static-link-source": "none",
            "store": "local",
            "unavailable-reason": null,
            "unmatched-runtime-paths": 1,
            "verified": true,
            "walk-outcome": null,
        })
    );
    assert!(
        !run.nix_log().contains("store verify"),
        "the local store holds only paths it built or verified; nothing to verify.\n{}",
        run.nix_log()
    );
}

#[test]
fn us1_substituter_answer_is_verified_and_recorded() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub {
        runtime: Some(curl_runtime()),
        runtime_store: Some(SUBSTITUTER.to_string()),
        substituters: vec![SUBSTITUTER.to_string()],
        ..base(&closure)
    });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "runtime-closure");
    assert_eq!(c216["store"], SUBSTITUTER);
    assert_eq!(c216["verified"], true);
    assert!(run.nix_log().contains(&format!("store verify --no-contents -r --store {SUBSTITUTER}")));
    assert!(run.marked().contains("waybill-fixture-xml"));
    assert!(!run.marked().contains("waybill-fixture-dnsroot"));
}

/// FR-004: `nix path-info --store` does not check signatures, so an answer
/// whose verification fails is never used.
#[test]
fn us1_unverified_substituter_answer_is_not_used() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub {
        runtime: Some(curl_runtime()),
        runtime_store: Some(SUBSTITUTER.to_string()),
        substituters: vec![SUBSTITUTER.to_string()],
        verify_ok: false,
        ..base(&closure)
    });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}");
    assert_eq!(c216["unavailable-reason"], "verification-failed", "{c216}");
    assert_eq!(c216["store"], Value::Null);
    assert_eq!(c216["verified"], Value::Null);
    assert!(
        !run.marked().contains("waybill-fixture-xml"),
        "the unverified listing must not decide anything: the runtime closure \
         marks `xml`, and the walk reaches it through the app's `buildInputs`"
    );
    assert_eq!(run.c203("waybill-fixture-xml").as_deref(), Some("reachability-walk"));
}

/// FR-004 / SC-005: every remote query is bounded, so an unreachable cache
/// costs ~2 s rather than nix's ~79 s default (q5).
#[test]
fn us1_remote_queries_carry_connect_timeout_and_one_attempt() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub {
        runtime: Some(curl_runtime()),
        runtime_store: Some(SUBSTITUTER.to_string()),
        substituters: vec![SUBSTITUTER.to_string()],
        ..base(&closure)
    });
    let remote: Vec<&str> =
        run.nix_log().lines().filter(|l| l.contains("--store")).collect();
    assert_eq!(remote.len(), 2, "one query and one verification:\n{}", run.nix_log());
    for line in &remote {
        assert!(line.contains("--option connect-timeout 2"), "{line}");
        assert!(line.contains("--option download-attempts 1"), "{line}");
    }
    let local: Vec<&str> = run
        .nix_log()
        .lines()
        .filter(|l| l.starts_with("path-info") && !l.contains("--store"))
        .collect();
    assert_eq!(local.len(), 1, "the local store is asked first, once");
    assert!(
        !local[0].contains("--option"),
        "the daemon refuses these as restricted settings for an untrusted user (q9)"
    );
}

/// FR-006: the exemptions survive, and the boot library is exempt even
/// though the runtime closure does not reference it.
#[test]
fn us1_exempt_members_keep_exemptions() {
    let closure = fixture("closure-runtime.json");
    let packages = fixture("hackage-packages.nix");
    // `project-boot` declares a Haskell dependency, which is what engages the
    // enrichment that hands the boot set to the scope pass.
    let run = scan_with(NixStub {
        project: dir().join("project-boot"),
        runtime: Some(curl_runtime()),
        package_set: Some((&packages, BOOT_CONFIG)),
        ..base(&closure)
    });
    assert_eq!(run.c203("waybill-fixture-app").as_deref(), Some("exempt-evaluated-package"));
    assert_eq!(run.c203("waybill-fixture-bootlib").as_deref(), Some("exempt-compiler-supplied"));
    let marked = run.marked();
    assert!(!marked.contains("waybill-fixture-app"));
    assert!(!marked.contains("waybill-fixture-bootlib"));
    // CONTROL: the exemption is narrow -- the other unreferenced members are
    // still marked in the same scan.
    assert!(marked.contains("waybill-fixture-xml") && marked.contains("waybill-fixture-py"));
}

/// FR-008: every closure member carries exactly one C203 value, runtime
/// members included, and the value never disagrees with the native mark.
#[test]
fn us1_every_closure_member_has_c203() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub { runtime: Some(curl_runtime()), ..base(&closure) });
    let marked = run.marked();
    let members = run.closure_members();
    assert_eq!(members.len(), 7, "{members:?}");
    for m in &members {
        let v = run.c203(m).unwrap_or_else(|| panic!("{m} carries no C203"));
        if v == "statically-linked" || v.starts_with("exempt-") {
            assert!(!marked.contains(m), "{m} is {v} and marked");
        }
    }
}

// ---- US1: static linking (FR-002) -----------------------------------------

fn static_runtime() -> String {
    // The static build is in the runtime closure; none of its libraries are.
    listing(&["h-sc-bin", "h-SC", "h-gmp"])
}

const HASKELL_LIBS: [&str; 5] = [
    "waybill-fixture-aeson",
    "waybill-fixture-text",
    "waybill-fixture-fgl",
    "waybill-fixture-hspec",
    "waybill-fixture-hunit",
];

/// SC-007's first half, on the shellcheck shape: every library linked into
/// the static executable is unmarked and says why.
#[test]
fn us1_static_haskell_libraries_are_runtime_and_statically_linked() {
    let closure = fixture("closure-static.json");
    let run = scan_with(NixStub { runtime: Some(static_runtime()), ..base(&closure) });
    let marked = run.marked();
    for lib in HASKELL_LIBS {
        assert!(!marked.contains(lib), "{lib} is linked into a runtime member");
        assert_eq!(run.c203(lib).as_deref(), Some("statically-linked"), "{lib}");
    }
    for n in ["waybill-fixture-ShellCheck", "waybill-fixture-gmp"] {
        assert!(!marked.contains(n));
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
}

/// SC-007's second half, on the ormolu shape: with the pinned package set,
/// the declared edges exclude the test-only dependency, so it is marked.
#[test]
fn us1_test_only_haskell_dep_marked_with_declared_edges() {
    let closure = fixture("closure-static.json");
    let packages = fixture("hackage-packages.nix");
    let run = scan_with(NixStub {
        runtime: Some(static_runtime()),
        package_set: Some((&packages, "{ }\n")),
        ..base(&closure)
    });
    let marked = run.marked();
    for n in ["waybill-fixture-hspec", "waybill-fixture-hunit"] {
        assert!(marked.contains(n), "{n} is test-only; stderr:\n{}", run.scan.stderr);
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    for n in ["waybill-fixture-aeson", "waybill-fixture-text", "waybill-fixture-fgl"] {
        assert!(!marked.contains(n), "{n} is a declared library dependency");
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
    assert_eq!(run.c216().unwrap()["static-link-source"], "declared-edges");
}

/// FR-002's fallback errs toward runtime: without the package set the
/// derivation fields cannot tell the test-only dependency apart, so it stays
/// runtime and the document says which source was used.
#[test]
fn us1_test_only_haskell_dep_kept_runtime_without_declared_edges() {
    let closure = fixture("closure-static.json");
    let run = scan_with(NixStub { runtime: Some(static_runtime()), ..base(&closure) });
    assert!(!run.marked().contains("waybill-fixture-hspec"));
    assert_eq!(run.c203("waybill-fixture-hspec").as_deref(), Some("statically-linked"));
    let c216 = run.c216().unwrap();
    assert_eq!(c216["static-link-source"], "derivation-fields", "{c216}");
    assert_eq!(c216["method"], "runtime-closure");
}

/// x86_64-linux's shape (`measurements/nix-behaviour/q10-*`): the wrapper's
/// own output and the C libraries the GHC runtime links, but **not** the
/// static Haskell build its binary was copied from. On aarch64-darwin the
/// static build is in the runtime closure; on Linux it is not.
fn linux_static_runtime() -> String {
    listing(&["h-sc-bin", "h-gmp"])
}

/// `alex`: a Haskell derivation the static build uses as a build tool
/// (`nativeBuildInputs`), which the root does not consume.
fn with_alex(closure: String) -> String {
    let closure = closure
        .replacen(
            "\"nativeBuildInputs\": \"/nix/store/zzz-ghc-9.4.8\",\n        \"buildInputs\": \"/nix/store/h-aeson",
            "\"nativeBuildInputs\": \"/nix/store/zzz-ghc-9.4.8 /nix/store/h-alex\",\n        \"buildInputs\": \"/nix/store/h-aeson",
            1,
        )
        .replacen(
            "\"d-gmp.drv\": { \"outputs\": [\"out\"] }\n        },",
            "\"d-gmp.drv\": { \"outputs\": [\"out\"] },\n          \"d-alex.drv\": { \"outputs\": [\"out\"] }\n        },",
            1,
        )
        .replacen(
            "\"d-gmp.drv\": {\n",
            "\"d-alex.drv\": {\n      \"name\": \"waybill-fixture-alex-3.5.1.0\",\n      \"env\": {\n        \"pname\": \"waybill-fixture-alex\",\n        \"version\": \"3.5.1.0\",\n        \"setupCompilerEnvironmentPhase\": \"runHook preSetupCompilerEnvironment\"\n      },\n      \"outputs\": { \"out\": { \"path\": \"h-alex\" } }\n    },\n    \"d-gmp.drv\": {\n",
            1,
        );
    assert_eq!(closure.matches("h-alex").count(), 2, "the fixture edits applied");
    assert_eq!(closure.matches("d-alex.drv").count(), 2, "the fixture edits applied");
    closure
}

/// `pandoc`: a Haskell build the **root** consumes through `inputs.drvs` and
/// names in `nativeBuildInputs`, with a Haskell library of its own. This is
/// the measured wrapper's shape: nixpkgs' shellcheck consumes `pandoc-cli`,
/// a Haskell generic-builder derivation, to render its man page
/// (`measurements/nix-behaviour/q10-*`). It runs on the build machine and is
/// not in the binary.
fn with_pandoc(closure: String) -> String {
    let closure = closure
        .replacen(
            "\"outputs\": \"bin out\",\n",
            "\"outputs\": \"bin out\",\n        \"nativeBuildInputs\": \"/nix/store/h-pandoc\",\n",
            1,
        )
        .replacen(
            "\"drvs\": { \"d-SC.drv\": { \"outputs\": [\"out\"] } },",
            "\"drvs\": { \"d-SC.drv\": { \"outputs\": [\"out\"] }, \"d-pandoc.drv\": { \"outputs\": [\"out\"] } },",
            1,
        )
        .replacen(
            "\"d-gmp.drv\": {\n",
            "\"d-pandoc.drv\": {\n      \"name\": \"waybill-fixture-pandoc-cli-3.7.0.2\",\n      \"env\": {\n        \"pname\": \"waybill-fixture-pandoc-cli\",\n        \"version\": \"3.7.0.2\",\n        \"setupCompilerEnvironmentPhase\": \"runHook preSetupCompilerEnvironment\",\n        \"buildInputs\": \"/nix/store/h-pdlib\"\n      },\n      \"inputs\": { \"drvs\": { \"d-pdlib.drv\": { \"outputs\": [\"out\"] } }, \"srcs\": [] },\n      \"outputs\": { \"out\": { \"path\": \"h-pandoc\" } }\n    },\n    \"d-pdlib.drv\": {\n      \"name\": \"waybill-fixture-pdlib-3.7.0.2\",\n      \"env\": {\n        \"pname\": \"waybill-fixture-pdlib\",\n        \"version\": \"3.7.0.2\",\n        \"setupCompilerEnvironmentPhase\": \"runHook preSetupCompilerEnvironment\"\n      },\n      \"outputs\": { \"out\": { \"path\": \"h-pdlib\" } }\n    },\n    \"d-gmp.drv\": {\n",
            1,
        );
    assert_eq!(closure.matches("h-pandoc").count(), 2, "the fixture edits applied");
    assert_eq!(closure.matches("d-pandoc.drv").count(), 2, "the fixture edits applied");
    closure
}

/// SC-007 / FR-002 on x86_64-linux's shape. The runtime closure holds the
/// wrapper and C libraries but not the static Haskell build, so nothing in it
/// is a Haskell-builder member. The evaluated root is runtime by definition,
/// and the static build it consumed through `inputs.drvs` is what its binary
/// was copied from: that build, and the libraries linked into it, are
/// runtime and `statically-linked`. Measured on the corpus before the fix:
/// `static-link-source: none`, every library linked into shellcheck marked.
///
/// CONTROLS, in the same scan: `alex`, a Haskell `build-tooling` member the
/// root does not consume, stays marked; `pandoc`, a Haskell build the root
/// consumes as a build tool (`nativeBuildInputs`), stays marked, and so does
/// its library.
#[test]
fn us1_linux_static_build_absent_from_runtime_closure_is_seeded_from_the_root() {
    let closure = with_pandoc(with_alex(fixture("closure-static.json")));
    let run = scan_with(NixStub { runtime: Some(linux_static_runtime()), ..base(&closure) });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "runtime-closure", "{c216}");
    assert_ne!(c216["static-link-source"], "none", "{c216}");
    assert_eq!(
        c216["static-link-source"], "derivation-fields",
        "no package set here, so the seeded build's own fields found its libraries: {c216}"
    );

    let marked = run.marked();
    for n in std::iter::once("waybill-fixture-ShellCheck").chain(HASKELL_LIBS) {
        assert!(
            !marked.contains(n),
            "{n} ships inside the wrapper's binary; marked: {marked:?}\nstderr:\n{}",
            run.scan.stderr
        );
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
    assert!(!marked.contains("waybill-fixture-gmp"), "in the runtime closure");
    assert_eq!(run.c203("waybill-fixture-gmp").as_deref(), Some("runtime-closure"));

    // CONTROL: not consumed by the root.
    assert_eq!(
        Run::cdx_prop(run.cdx("waybill-fixture-alex"), "waybill:closure-role"),
        Some("build-tooling"),
        "precondition: the control is in a tooling role"
    );
    assert!(marked.contains("waybill-fixture-alex"), "marked: {marked:?}");
    assert_eq!(run.c203("waybill-fixture-alex").as_deref(), Some("runtime-closure"));
    // CONTROL: consumed by the root, as a build tool.
    for n in ["waybill-fixture-pandoc-cli", "waybill-fixture-pdlib"] {
        assert!(marked.contains(n), "{n} runs on the build machine; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
}

/// The same Linux shape with the pinned package set: the seeded static build
/// is followed through its declared relations, which exclude the test-only
/// dependency, and C216 says so.
#[test]
fn us1_linux_static_build_seed_uses_declared_edges() {
    let closure = fixture("closure-static.json");
    let packages = fixture("hackage-packages.nix");
    let run = scan_with(NixStub {
        runtime: Some(linux_static_runtime()),
        package_set: Some((&packages, "{ }\n")),
        ..base(&closure)
    });
    let marked = run.marked();
    for n in ["waybill-fixture-hspec", "waybill-fixture-hunit"] {
        assert!(marked.contains(n), "{n} is test-only; stderr:\n{}", run.scan.stderr);
    }
    for n in ["waybill-fixture-ShellCheck", "waybill-fixture-aeson", "waybill-fixture-text", "waybill-fixture-fgl"] {
        assert!(!marked.contains(n), "{n}; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
    assert_eq!(run.c216().unwrap()["static-link-source"], "declared-edges");
}

/// `unlisted`: a second runtime Haskell executable the package set does not
/// hold, linking `fgl` through its own `buildInputs`. moat's shape: the
/// project is not in nixpkgs, everything else is.
fn with_unlisted(closure: String) -> String {
    let closure = closure
        .replacen(
            "\"d-gmp.drv\": { \"outputs\": [\"out\"] }\n        },",
            "\"d-gmp.drv\": { \"outputs\": [\"out\"] },\n          \"d-unl.drv\": { \"outputs\": [\"out\"] }\n        },",
            1,
        )
        .replacen(
            "\"d-gmp.drv\": {\n",
            "\"d-unl.drv\": {\n      \"name\": \"waybill-fixture-unlisted-1.0\",\n      \"env\": {\n        \"pname\": \"waybill-fixture-unlisted\",\n        \"version\": \"1.0\",\n        \"setupCompilerEnvironmentPhase\": \"runHook preSetupCompilerEnvironment\",\n        \"nativeBuildInputs\": \"/nix/store/zzz-ghc-9.4.8\",\n        \"buildInputs\": \"/nix/store/h-fgl\"\n      },\n      \"outputs\": { \"out\": { \"path\": \"h-unl\" } }\n    },\n    \"d-gmp.drv\": {\n",
            1,
        );
    assert_eq!(closure.matches("d-unl.drv").count(), 2, "the fixture edits applied");
    closure
}

/// #1456: the static-link source is chosen per root. One unconfirmed root
/// falls back to its derivation fields; the confirmed static build keeps its
/// declared relations, so its test-only dependency stays marked. C216 says
/// both happened, with a count for each, instead of `derivation-fields` for
/// the whole closure.
#[test]
fn a_mixed_static_link_source_is_reported_per_root() {
    let closure = with_unlisted(fixture("closure-static.json"));
    let packages = fixture("hackage-packages.nix");
    let run = scan_with(NixStub {
        runtime: Some(listing(&["h-sc-bin", "h-SC", "h-gmp", "h-unl"])),
        package_set: Some((&packages, "{ }\n")),
        ..base(&closure)
    });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["static-link-source"], "mixed", "{c216}\nstderr:\n{}", run.scan.stderr);
    assert_eq!(
        c216["static-link-roots"],
        serde_json::json!({"declared-edges": 1, "derivation-fields": 1}),
        "{c216}"
    );
    let marked = run.marked();
    for n in ["waybill-fixture-hspec", "waybill-fixture-hunit"] {
        assert!(marked.contains(n), "{n} is test-only for the confirmed root; marked: {marked:?}");
    }
    // CONTROL: the confirmed root's library dependencies, which `unlisted`'s
    // `fgl` overlaps, are still linked.
    for n in ["waybill-fixture-aeson", "waybill-fixture-text", "waybill-fixture-fgl"] {
        assert!(!marked.contains(n), "{n}; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
}

/// #1459's shape: the static build is in the runtime closure, as on
/// aarch64-darwin; none of its libraries are.
fn namesake_runtime() -> String {
    listing(&["h-zapp-bin", "h-ZX"])
}

fn namesake_scan(closure: &str, packages: &str) -> Run {
    scan_with(NixStub {
        runtime: Some(namesake_runtime()),
        package_set: Some((packages, "{ }\n")),
        ..base(closure)
    })
}

/// The component named `name` whose PURL starts with `prefix`, from CDX.
fn cdx_by_purl<'a>(run: &'a Run, name: &str, prefix: &str) -> &'a Value {
    run.cdx_components()
        .into_iter()
        .find(|c| c["name"] == name && c["purl"].as_str().is_some_and(|p| p.starts_with(prefix)))
        .unwrap_or_else(|| panic!("no {prefix} component named {name}"))
}

/// #1459: a Haskell library linked into the static executable shares its name
/// with a build-only C library. Measured on nixpkgs `a799d3e3`: Haskell
/// `zlib` 0.7.1.1 was marked on pandoc-cli, hledger and cachix while its C
/// namesake was not, because the declared edge `tls -> zlib` bound the name to
/// `pkg:generic/zlib@1.3.2`. The Haskell library is runtime and
/// `statically-linked`; the C library, which nothing at runtime references,
/// stays marked.
///
/// CDX only for the two namesakes: the three-format helpers look a component
/// up by name, which two components share here.
#[test]
fn us1_static_haskell_library_with_c_namesake_is_runtime() {
    let closure = fixture("closure-static-namesake.json");
    let packages = fixture("hackage-packages.nix");
    let run = namesake_scan(&closure, &packages);
    assert_eq!(run.c216().unwrap()["static-link-source"], "declared-edges");

    let hs = cdx_by_purl(&run, "waybill-fixture-zlib", "pkg:hackage/");
    assert_eq!(hs["version"], "0.7.1.1");
    assert_ne!(hs["scope"], "excluded", "linked into the binary; stderr:\n{}", run.scan.stderr);
    assert_eq!(Run::cdx_prop(hs, C203), Some("statically-linked"));

    let c = cdx_by_purl(&run, "waybill-fixture-zlib", "pkg:generic/");
    assert_eq!(c["version"], "1.3.2");
    assert_eq!(c["scope"], "excluded", "build-only C namesake; stderr:\n{}", run.scan.stderr);
    assert_eq!(Run::cdx_prop(c, C203), Some("runtime-closure"));

    // CONTROL: the declared edges still leave the test-only dependency out,
    // so the fix did not turn the root's walk into the derivation fields.
    assert!(run.marked().contains("waybill-fixture-hspec"));
}

/// #1459, cachix's `hfsevents`: the pinned package set's relations are
/// cabal2nix's Linux resolution, so `fsnotify` declares `hinotify`, while on
/// darwin its derivation propagates `hfsevents`, which is what is linked.
/// Below the root, a library's `propagatedBuildInputs` holds no test
/// dependencies (q6), so it is followed alongside the declared relations.
#[test]
fn us1_platform_conditional_haskell_dependency_is_runtime() {
    let closure = fixture("closure-static-namesake.json");
    let packages = fixture("hackage-packages.nix");
    let run = namesake_scan(&closure, &packages);
    let marked = run.marked();
    for n in ["waybill-fixture-fsnotify", "waybill-fixture-hfsevents"] {
        assert!(!marked.contains(n), "{n} is linked; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
}

// ---- 1283: a project's own test suite is not linked code -----------------

/// The three IFD corpus targets (slack-web, slacklinker, openai) are the first
/// whose evaluated package is the project itself, built with its test suite.
/// nixpkgs' generic builder puts a `doCheck` build's test dependencies in
/// `buildInputs`, and the static-link fallback followed that field at every
/// runtime Haskell member the package set did not confirm. Measured on the
/// CI documents: 26 test frameworks were C203 `statically-linked` while the
/// project's own manifest scoped them `development` (CDX `scope: excluded`),
/// which invariant I10 reports as `closure-statically-linked-but-marked`.
///
/// A library's link dependencies are in its `propagatedBuildInputs`; its
/// `buildInputs` holds its test and benchmark dependencies
/// (`specs/1283-nix-allow-ifd/measurements/invariants/`, aeson-2.2.4.1 at the
/// pinned revision). slack-web and openai are libraries, and so is every
/// runtime library whose own test suite reached the set (`tasty` here, from
/// `aeson`).
fn tests_library_runtime() -> String {
    listing(&["h-proj", "h-aeson", "h-text"])
}

/// slacklinker's shape: an executable-only build, so the runtime closure holds
/// none of its Haskell libraries and `buildInputs` mixes library and test
/// dependencies. Only the project's own manifest tells them apart.
fn tests_executable_runtime() -> String {
    listing(&["h-proj", "h-gmp"])
}

fn tests_scan(closure: &str, runtime: String, packages: &str) -> Run {
    scan_with(NixStub {
        project: dir().join("project-tests"),
        runtime: Some(runtime),
        package_set: Some((packages, "{ }\n")),
        ..base(closure)
    })
}

/// I10's converse check over the whole document: a member C203 calls
/// statically linked carries no not-in-artifact mark in any format.
fn assert_no_linked_member_is_marked(run: &Run) {
    let marked = run.marked();
    for c in run.cdx_components() {
        if Run::cdx_prop(c, C203) != Some("statically-linked") {
            continue;
        }
        assert_ne!(c["scope"], "excluded", "{} is statically-linked and marked", c["purl"]);
        let name = c["name"].as_str().unwrap_or_default();
        assert!(!marked.contains(name), "{name} is statically-linked and marked: {marked:?}");
    }
}

/// slack-web and openai: the library root's test dependencies, and those of a
/// runtime library, are decided by the runtime closure, which does not hold
/// them. The root's library dependencies are in it.
#[test]
fn a_library_roots_test_dependencies_are_not_statically_linked() {
    let closure = fixture("closure-tests-library.json");
    let packages = fixture("hackage-packages.nix");
    let run = tests_scan(&closure, tests_library_runtime(), &packages);
    let c216 = run.c216().unwrap();
    assert_eq!(c216["method"], "runtime-closure", "{c216}");
    // The project is not in the package set, so its root falls back, as on
    // the corpus. Nothing here was built by a GHC, which confirmation
    // requires, so `aeson` and `text` fall back too (#1456 counts them).
    assert_eq!(c216["static-link-source"], "derivation-fields", "{c216}");
    assert_eq!(
        c216["static-link-roots"],
        serde_json::json!({"declared-edges": 0, "derivation-fields": 3}),
        "{c216}"
    );
    let marked = run.marked();
    for n in [
        // The project's test suite, and what it links (`development`).
        "waybill-fixture-hspec",
        "waybill-fixture-hunit",
        // `aeson`'s own test suite, which no manifest of the project names.
        "waybill-fixture-quickcheck",
        // `text`'s test suite and its closure. `text` is hashable's shape: a
        // library whose dependencies are all boot packages, so it propagates
        // nothing, and only its install phase registering a package says it
        // is a library.
        "waybill-fixture-tasty",
        "waybill-fixture-ansi",
    ] {
        assert!(marked.contains(n), "{n} is a test dependency; marked: {marked:?}\n{}", run.scan.stderr);
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    // CONTROL: the library's own dependencies ship, by store reference.
    for n in ["waybill-fixture-aeson", "waybill-fixture-text"] {
        assert!(!marked.contains(n), "{n}; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    assert_no_linked_member_is_marked(&run);
}

/// slacklinker: an executable-only root, whose test dependencies sit in the
/// same `buildInputs` as its library dependencies. The project's manifest
/// declares `hspec` for its test suite only, so that edge is not followed; the
/// library dependencies still are.
///
/// `hspec` was built at 2.11.10 while the package set says 2.11.9, so the
/// closure member is a different component from the one the manifest
/// declares (slacklinker's `tmp-postgres` has this shape). The test edge is
/// recognised by the package name the manifest declares.
#[test]
fn an_executable_roots_declared_test_dependencies_are_not_statically_linked() {
    let closure = fixture("closure-tests-executable.json");
    let packages = fixture("hackage-packages.nix");
    let run = tests_scan(&closure, tests_executable_runtime(), &packages);
    let c216 = run.c216().unwrap();
    assert_eq!(c216["static-link-source"], "derivation-fields", "{c216}");
    assert_eq!(
        c216["static-link-roots"],
        serde_json::json!({"declared-edges": 0, "derivation-fields": 1}),
        "the project is the only runtime Haskell member, and it is unconfirmed: {c216}"
    );

    let built = cdx_by_purl(&run, "waybill-fixture-hspec", "pkg:generic/");
    assert_eq!(built["version"], "2.11.10");
    assert_eq!(built["scope"], "excluded", "test-only; stderr:\n{}", run.scan.stderr);
    assert_eq!(Run::cdx_prop(built, C203), Some("runtime-closure"));

    let marked = run.marked();
    assert!(marked.contains("waybill-fixture-hunit"), "linked only into hspec; marked: {marked:?}");
    assert_eq!(run.c203("waybill-fixture-hunit").as_deref(), Some("runtime-closure"));

    // CONTROL: the executable's library dependencies are still linked into
    // it, so the fallback was narrowed, not switched off.
    for n in ["waybill-fixture-aeson", "waybill-fixture-text"] {
        assert!(!marked.contains(n), "{n} is linked; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
    assert_no_linked_member_is_marked(&run);
}

/// The SPDX 2.3 relationship types naming `name`'s package as the
/// dependency, deduplicated and sorted.
fn spdx23_types_into(run: &Run, name: &str) -> BTreeSet<String> {
    let doc = &run.scan.spdx23;
    let id = doc["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["SPDXID"].as_str())
        .unwrap_or_else(|| panic!("no SPDX 2.3 package named {name}"))
        .to_string();
    doc["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| {
            let ty = r["relationshipType"].as_str()?;
            let into = (ty == "DEPENDS_ON" && r["relatedSpdxElement"] == id.as_str())
                || (ty.ends_with("_DEPENDENCY_OF") && r["spdxElementId"] == id.as_str());
            into.then(|| ty.to_string())
        })
        .collect()
}

/// The SPDX 3 lifecycle scopes on `dependsOn` relationships into `name`.
fn spdx3_scopes_into(run: &Run, name: &str) -> BTreeSet<String> {
    let graph = run.scan.spdx3["@graph"].as_array().unwrap();
    let id = graph
        .iter()
        .find(|e| e["name"] == name && e["type"] == "software_Package")
        .and_then(|e| e["spdxId"].as_str())
        .unwrap_or_else(|| panic!("no SPDX 3 package named {name}"))
        .to_string();
    graph
        .iter()
        .filter(|e| e["relationshipType"] == "dependsOn")
        .filter(|e| e["to"].as_array().is_some_and(|t| t.iter().any(|x| x == id.as_str())))
        .map(|e| e["scope"].as_str().unwrap_or("none").to_string())
        .collect()
}

/// #1465 — runtime evidence wins over a reader's lifecycle scope.
///
/// slack-web's shape: its test suite declares QuickCheck, so the Haskell
/// reader scoped it `development`, and aeson propagates it into the runtime
/// closure. The runtime closure decided it ships (C203 `runtime-closure`) and
/// left it unmarked, yet the reader's `development` alone put CycloneDX
/// `scope: excluded` on it and `DEV_DEPENDENCY_OF` on every edge into it in
/// SPDX 2.3. Here `text` takes QuickCheck's place: declared only by the test
/// suite, propagated by `aeson`, held by the runtime closure.
#[test]
fn a_test_declared_package_the_runtime_closure_holds_is_not_marked() {
    let closure = fixture("closure-tests-library.json");
    let packages = fixture("hackage-packages.nix");
    let run = scan_with(NixStub {
        project: dir().join("project-tests-ships"),
        runtime: Some(tests_library_runtime()),
        package_set: Some((&packages, "{ }\n")),
        ..base(&closure)
    });
    assert_eq!(run.c216().unwrap()["method"], "runtime-closure");

    let text = run.cdx("waybill-fixture-text");
    assert_eq!(run.c203("waybill-fixture-text").as_deref(), Some("runtime-closure"));
    assert_ne!(text["scope"], "excluded", "the runtime closure holds it: {text}\n{}", run.scan.stderr);
    assert_eq!(Run::cdx_prop(text, "waybill:lifecycle-scope"), None, "{text}");
    assert!(!run.marked().contains("waybill-fixture-text"));
    assert_eq!(
        spdx23_types_into(&run, "waybill-fixture-text"),
        names(&["DEPENDS_ON"]),
        "edges the reader typed from its scope agree with the component"
    );
    assert_eq!(spdx3_scopes_into(&run, "waybill-fixture-text"), names(&["none"]));

    // CONTROL: a test dependency the runtime closure does not hold keeps the
    // reader's scope and the closure's mark, in every format.
    let hspec = run.cdx("waybill-fixture-hspec");
    assert_eq!(run.c203("waybill-fixture-hspec").as_deref(), Some("runtime-closure"));
    assert_eq!(hspec["scope"], "excluded");
    assert_eq!(Run::cdx_prop(hspec, "waybill:lifecycle-scope"), Some("development"));
    assert!(run.marked().contains("waybill-fixture-hspec"));
    assert!(spdx23_types_into(&run, "waybill-fixture-hspec").contains("DEV_DEPENDENCY_OF"));
}

// ---- US1: embedded modules (FR-002) ---------------------------------------

fn uvarint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
    out
}

/// A file the Go BuildInfo reader accepts, as `closure_binary_contents.rs`
/// synthesizes it.
fn go_binary(main: &str, deps: &[(&str, &str)]) -> Vec<u8> {
    let mut mod_info = format!("path\t{main}/cmd\nmod\t{main}\t(devel)\t\n");
    for (name, version) in deps {
        mod_info.push_str(&format!("dep\t{name}\t{version}\th1:fixture=\n"));
    }
    let mut v = vec![0u8; 2048];
    v[..4].copy_from_slice(b"\x7fELF");
    v.extend_from_slice(b"\xff Go buildinf:");
    v.push(8);
    v.push(0x2);
    v.extend_from_slice(&[0u8; 16]);
    let build = b"go1.22.1";
    v.extend(uvarint(build.len() as u64));
    v.extend_from_slice(build);
    v.extend(uvarint(mod_info.len() as u64));
    v.extend_from_slice(mod_info.as_bytes());
    v
}

fn put(store: &Path, output: &str, file: &str, bytes: &[u8]) {
    let d = store.join(output).join("bin");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(file), bytes).unwrap();
}

/// A module inherits its host binary's runtime status: unmarked and
/// `statically-linked` in a runtime host, marked in a build tool.
#[test]
fn us1_embedded_go_module_inherits_runtime_host() {
    let store = tempfile::tempdir().unwrap();
    put(
        store.path(),
        "h0-waybill-fixture-app-1.2.3",
        "app",
        &go_binary("example.test/waybill-fixture-app", &[("example.test/waybill-fixture-goa", "v1.0.0")]),
    );
    put(
        store.path(),
        "h2-waybill-fixture-tool-3.0.0",
        "tool",
        &go_binary("example.test/waybill-fixture-tool", &[("example.test/waybill-fixture-toolmod", "v0.1.0")]),
    );
    let closure = fixture("closure-embedded.json");
    let run = scan_with(NixStub {
        runtime: Some(listing(&["h0-waybill-fixture-app-1.2.3", "h1-waybill-fixture-lib-2.0.0"])),
        store_dir: Some(store.path().to_path_buf()),
        ..base(&closure)
    });
    let marked = run.marked();
    let goa = "example.test/waybill-fixture-goa";
    let toolmod = "example.test/waybill-fixture-toolmod";
    assert!(!marked.contains(goa), "compiled into the runtime app; stderr:\n{}", run.scan.stderr);
    assert_eq!(run.c203(goa).as_deref(), Some("statically-linked"));
    assert!(marked.contains(toolmod), "compiled into a build tool only");
    assert_eq!(run.c203(toolmod).as_deref(), Some("runtime-closure"));
    // The hosts themselves, for the record.
    assert!(marked.contains("waybill-fixture-tool"));
    assert!(!marked.contains("waybill-fixture-lib"));
}

// ---- #1458: the outputs the attribute installs -----------------------------

/// postgrest's shape (`measurements/nix-behaviour/q11-*`): the derivation's
/// `outputs` starts with `out`, the library, whose closure holds what the
/// installed `bin` does not reference (754 paths against 129).
fn outputs_closure() -> String {
    fixture("closure-outputs.json")
}

/// One listing per output of the app, as `nix path-info -r` gives it for
/// that output alone: `out` (the library) references `xml` and `py`, `bin`
/// does not.
fn per_output<'a>() -> Vec<(&'a str, String)> {
    vec![
        ("h-app-lib", listing(&["h-app-lib", "h-ssl", "h-z", "h-xml", "h-py"])),
        ("h-app-bin", listing(&["h-app-bin", "h-ssl", "h-z"])),
        ("h-app-doc", listing(&["h-app-doc"])),
    ]
}

/// The `path-info` invocations the scan made.
fn path_info_calls(run: &Run) -> Vec<&str> {
    run.nix_log().lines().filter(|l| l.starts_with("path-info")).collect()
}

/// #1458: the runtime closure is that of the output the attribute selects
/// (postgrest is `outputSpecified`, `outputName = bin`), not the head of the
/// derivation's `outputs`, and C216 records which output and why.
#[test]
fn runtime_closure_follows_the_selected_output_not_the_head_of_outputs() {
    let closure = outputs_closure();
    let run = scan_with(NixStub {
        runtime_by_output: per_output(),
        selection: Some(r#"{"outputs":["bin"],"source":"output-specified"}"#),
        ..base(&closure)
    });
    assert_eq!(
        run.c216().expect("C216"),
        serde_json::json!({
            "c-runtime-libraries": null,
            // Nothing in this closure is reached only through a documentation
            // output (#1455).
            "doc-only-excluded": {"members": 0, "output-names": ["devdoc", "doc", "info", "man"], "paths": 0},
            // No output in this store has a lib/links folder (#1455).
            "lib-links-excluded": {"members": 0, "not-assessed": null, "paths": 0, "rule": "unloaded-lib-links"},
            "method": "runtime-closure",
            "outputs": ["bin"],
            "outputs-source": "output-specified",
            "schema": 1,
            "static-c-linking-detected": false,
            "static-link-roots": {"declared-edges": 0, "derivation-fields": 0},
            "static-link-source": "none",
            "store": "local",
            "unavailable-reason": null,
            "unmatched-runtime-paths": 0,
            "verified": true,
            "walk-outcome": null,
        }),
        "stderr:\n{}",
        run.scan.stderr
    );
    let calls = path_info_calls(&run);
    assert_eq!(calls, vec!["path-info -r --json /nix/store/h-app-bin"], "{}", run.nix_log());
    let marked = run.marked();
    for n in ["waybill-fixture-xml", "waybill-fixture-py"] {
        assert!(marked.contains(n), "{n}: only the library output references it; marked {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    for n in ["waybill-fixture-ssl", "waybill-fixture-z"] {
        assert!(!marked.contains(n), "{n}: the installed output references it");
    }
}

/// #1458, shellcheck's shape: `meta.outputsToInstall` names several outputs,
/// and the runtime closure is their union, asked in one query.
#[test]
fn runtime_closure_is_the_union_of_outputs_to_install() {
    let closure = outputs_closure();
    let run = scan_with(NixStub {
        runtime_by_output: per_output(),
        selection: Some(r#"{"outputs":["bin","doc"],"source":"outputs-to-install"}"#),
        ..base(&closure)
    });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["outputs"], serde_json::json!(["bin", "doc"]), "{c216}");
    assert_eq!(c216["outputs-source"], "outputs-to-install", "{c216}");
    assert_eq!(c216["unmatched-runtime-paths"], 0, "`doc` is the app's own output: {c216}");
    assert_eq!(
        path_info_calls(&run),
        vec!["path-info -r --json /nix/store/h-app-bin /nix/store/h-app-doc"],
        "{}",
        run.nix_log()
    );
    assert!(run.marked().contains("waybill-fixture-xml"));
}

/// #1458: when the selection cannot be evaluated (as the installable form of
/// curl's could not be, q9 b), the default output is queried and C216 says
/// the fallback happened. Also the CONTROL for the two tests above: the same
/// stub's listing for `out` leaves `xml` and `py` unmarked, so those tests
/// could tell the selected output from the head of `outputs`.
#[test]
fn an_unevaluable_selection_falls_back_to_the_default_output_and_says_so() {
    let closure = outputs_closure();
    let run = scan_with(NixStub { runtime_by_output: per_output(), ..base(&closure) });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "runtime-closure", "{c216}");
    assert_eq!(c216["outputs"], serde_json::json!(["out"]), "{c216}");
    assert_eq!(c216["outputs-source"], "default-output", "{c216}");
    assert!(run.nix_log().lines().any(|l| l.starts_with("eval") && l.contains("outputSpecified")));
    assert_eq!(path_info_calls(&run), vec!["path-info -r --json /nix/store/h-app-lib"], "{}", run.nix_log());
    let marked = run.marked();
    for n in ["waybill-fixture-xml", "waybill-fixture-py"] {
        assert!(!marked.contains(n), "{n}: the library output references it; marked {marked:?}");
    }
}

/// #1458: a selection naming an output the evaluated derivation does not
/// declare is not trusted for any of its names: the default output is used.
#[test]
fn a_selection_naming_an_unknown_output_falls_back() {
    let closure = outputs_closure();
    let run = scan_with(NixStub {
        runtime_by_output: per_output(),
        selection: Some(r#"{"outputs":["bin","man"],"source":"outputs-to-install"}"#),
        ..base(&closure)
    });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["outputs"], serde_json::json!(["out"]), "{c216}");
    assert_eq!(c216["outputs-source"], "default-output", "{c216}");
}

/// #1458: the selection is evaluated as purely as the closure itself:
/// import-from-derivation refused, no `--impure`, the same flakeref.
#[test]
fn the_selection_evaluation_is_pure_and_names_the_scanned_attribute() {
    let closure = outputs_closure();
    let run = scan_with(NixStub {
        runtime_by_output: per_output(),
        selection: Some(r#"{"outputs":["bin"],"source":"output-specified"}"#),
        ..base(&closure)
    });
    let evals: Vec<&str> =
        run.nix_log().lines().filter(|l| l.starts_with("eval") && l.contains("outputSpecified")).collect();
    assert_eq!(evals.len(), 1, "{}", run.nix_log());
    assert!(evals[0].contains("--option allow-import-from-derivation false"), "{}", evals[0]);
    assert!(!evals[0].contains("--impure"), "{}", evals[0]);
    assert!(evals[0].ends_with("#packages.aarch64-darwin.default"), "{}", evals[0]);
}

// ---- US2: the reachability walk (FR-005, FR-007) --------------------------

/// FR-005 / FR-007: with no store holding the output, scope comes from the
/// walk, and the document says the marks are an approximation and why.
#[test]
fn us2_no_store_answer_uses_walk() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(base(&closure));
    assert_eq!(
        run.c216().expect("C216 on a closure document"),
        serde_json::json!({
            // No member of this fixture names a C compiler (#1467).
            "c-runtime-libraries": "not-assessed",
            // The rule reads the runtime closure's references (#1455).
            "doc-only-excluded": null,
            "lib-links-excluded": null,
            "method": "reachability-walk",
            // Asked about, though no store held it.
            "outputs": ["bin"],
            "outputs-source": "default-output",
            "schema": 1,
            "static-c-linking-detected": false,
            "static-link-roots": {"declared-edges": 0, "derivation-fields": 0},
            "static-link-source": "none",
            "store": null,
            "unavailable-reason": "not-in-any-store",
            "unmatched-runtime-paths": null,
            "verified": null,
            "walk-outcome": "used",
        }),
        "stderr:\n{}",
        run.scan.stderr
    );
    // Every member the walk decided says so, marked or not; the evaluated
    // package keeps its exemption.
    for m in run.closure_members() {
        let want = if m == "waybill-fixture-app" { "exempt-evaluated-package" } else { "reachability-walk" };
        assert_eq!(run.c203(&m).as_deref(), Some(want), "{m}");
    }
}

/// FR-005: every member the walk does not reach is marked, every member it
/// reaches is not, in all three formats.
#[test]
fn us2_walk_marks_unreached_members() {
    let closure = fixture("closure-walk.json");
    let run = scan_with(base(&closure));
    assert_eq!(
        run.marked(),
        names(&["waybill-fixture-perl", "waybill-fixture-tool", "waybill-fixture-xml"]),
        "stderr:\n{}",
        run.scan.stderr
    );
    for n in ["waybill-fixture-ssl", "waybill-fixture-z"] {
        assert_eq!(run.c203(n).as_deref(), Some("reachability-walk"), "{n}");
    }
    assert_eq!(run.c216().unwrap()["method"], "reachability-walk");
}

/// FR-005: the walk follows runtime-linking fields only. `perl` is linked by
/// the build tool (so its role is `both`, which the role rule never marks)
/// and is reached from the root only through `ssl`'s `nativeBuildInputs`. So
/// is everything the tool links: `xml` is an `artifact-input` the role rule
/// leaves alone.
#[test]
fn us2_walk_follows_only_runtime_linking_fields() {
    let closure = fixture("closure-walk.json");
    let run = scan_with(base(&closure));
    let perl = run.cdx("waybill-fixture-perl");
    assert_eq!(
        Run::cdx_prop(perl, "waybill:closure-role"),
        Some("both"),
        "precondition: the role rule would leave perl unmarked"
    );
    assert_eq!(
        Run::cdx_prop(run.cdx("waybill-fixture-xml"), "waybill:closure-role"),
        Some("artifact-input")
    );
    let marked = run.marked();
    assert!(marked.contains("waybill-fixture-perl"), "reached only through nativeBuildInputs");
    assert!(marked.contains("waybill-fixture-xml"), "linked only by a build tool");
    // CONTROL: the propagated edge is followed, so the walk is not simply
    // stopping at the root's own fields.
    assert!(!marked.contains("waybill-fixture-z"), "propagated by a reached member");
}

/// #1467: no dependency field names the C library. A member reaches glibc
/// only through its `stdenv`, whose `defaultNativeBuildInputs` hold the C
/// compiler wrapper, whose `libc_lib` is the library every dynamically linked
/// output it produces loads (measured on curl, openssh, httpie, gh and
/// ripgrep at nixpkgs `a799d3e3`, x86_64-linux: the runtime closure holds
/// glibc for all five, and the walk marked it in all five).
///
/// CONTROLS, in the same scan: the compiler, the libc's headers and what the
/// libc itself lists stay marked, and so does the libc of the stdenv only a
/// build tool was built with. `unistring` is curl's `libunistring`: reached,
/// but built by a bootstrap stage whose compiler names the bootstrap libc
/// the final one was built from, which no measured runtime closure holds.
#[test]
fn us2_walk_keeps_the_libc_its_members_compiler_links() {
    let closure = fixture("closure-walk-libc.json");
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}");
    let marked = run.marked();
    assert!(
        !marked.contains("waybill-fixture-libc"),
        "the libc the app's and ssl's compiler links ships; marked: {marked:?}\nstderr:\n{}",
        run.scan.stderr
    );
    assert_eq!(run.c203("waybill-fixture-libc").as_deref(), Some("reachability-walk"));
    assert!(!marked.contains("waybill-fixture-unistring"), "reached through ssl");
    for n in [
        "waybill-fixture-gcc",
        "waybill-fixture-cc-wrapper",
        "waybill-fixture-headers",
        "waybill-fixture-bootlibc",
        "waybill-fixture-stage0libc",
        "waybill-fixture-tool",
    ] {
        assert!(marked.contains(n), "{n} does not ship; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("reachability-walk"), "{n}");
    }
    assert_eq!(
        c216["c-runtime-libraries"], "libc-only",
        "the compiler's runtime libraries and the libc's own references were not assessed: {c216}"
    );
}

/// The shellcheck wrapper on x86_64-linux is itself built with a C compiler,
/// so its own stdenv names glibc. That is not something the walk followed:
/// the wrapper still reaches only itself, and the walk is still set aside.
/// Counted as reached, the libc would make the walk "used" and mark every
/// library linked into the binary.
#[test]
fn us2_the_roots_own_libc_does_not_make_a_wrapper_walk_used() {
    let closure = fixture("closure-static.json")
        .replacen(
            "\"outputs\": \"bin out\",",
            "\"outputs\": \"bin out\",\n        \"stdenv\": \"/nix/store/h-std\",",
            1,
        )
        .replacen(
            "\"drvs\": { \"d-SC.drv\": { \"outputs\": [\"out\"] } },",
            "\"drvs\": { \"d-SC.drv\": { \"outputs\": [\"out\"] }, \"d-std.drv\": { \"outputs\": [\"out\"] } },",
            1,
        )
        .replacen(
            "\"d-gmp.drv\": {\n",
            "\"d-std.drv\": {\n      \"name\": \"waybill-fixture-stdenv\",\n      \"env\": { \"defaultNativeBuildInputs\": \"/nix/store/h-cc\" },\n      \"inputs\": { \"drvs\": { \"d-cc.drv\": { \"outputs\": [\"out\"] } }, \"srcs\": [] },\n      \"outputs\": { \"out\": { \"path\": \"h-std\" } }\n    },\n    \"d-cc.drv\": {\n      \"name\": \"waybill-fixture-cc-wrapper-15.2.0\",\n      \"env\": { \"pname\": \"waybill-fixture-cc-wrapper\", \"version\": \"15.2.0\", \"libc_lib\": \"/nix/store/h-libc\" },\n      \"inputs\": { \"drvs\": { \"d-libc.drv\": { \"outputs\": [\"out\"] } }, \"srcs\": [] },\n      \"outputs\": { \"out\": { \"path\": \"h-cc\" } }\n    },\n    \"d-libc.drv\": {\n      \"name\": \"waybill-fixture-libc-2.42\",\n      \"env\": { \"pname\": \"waybill-fixture-libc\", \"version\": \"2.42\" },\n      \"outputs\": { \"out\": { \"path\": \"h-libc\" } }\n    },\n    \"d-gmp.drv\": {\n",
            1,
        );
    assert_eq!(closure.matches("h-libc").count(), 2, "the fixture edits applied");
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "closure-role", "{c216}\nstderr:\n{}", run.scan.stderr);
    assert_eq!(c216["walk-outcome"], "reached-only-root", "{c216}");
    let marked = run.marked();
    for lib in HASKELL_LIBS {
        assert!(!marked.contains(lib), "{lib} is linked into the binary; marked: {marked:?}");
    }
}

/// #1467's other half: a walk whose members name no C compiler keeps no libc,
/// and the record says C runtime libraries were not assessed.
#[test]
fn us2_walk_without_a_compiler_libc_says_not_assessed() {
    let closure = fixture("closure-walk.json");
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}");
    assert_eq!(c216["c-runtime-libraries"], "not-assessed", "{c216}");
}

/// Embedded modules inherit a reached host's status under the walk, and are
/// decided by it (data-model rule 4).
#[test]
fn us2_embedded_module_inherits_reached_host() {
    let store = tempfile::tempdir().unwrap();
    put(
        store.path(),
        "h0-waybill-fixture-app-1.2.3",
        "app",
        &go_binary("example.test/waybill-fixture-app", &[("example.test/waybill-fixture-goa", "v1.0.0")]),
    );
    put(
        store.path(),
        "h1-waybill-fixture-lib-2.0.0",
        "lib",
        &go_binary("example.test/waybill-fixture-lib", &[("example.test/waybill-fixture-libmod", "v1.1.0")]),
    );
    put(
        store.path(),
        "h2-waybill-fixture-tool-3.0.0",
        "tool",
        &go_binary("example.test/waybill-fixture-tool", &[("example.test/waybill-fixture-toolmod", "v0.1.0")]),
    );
    let closure = fixture("closure-embedded.json");
    let run = scan_with(NixStub { store_dir: Some(store.path().to_path_buf()), ..base(&closure) });
    let marked = run.marked();
    let libmod = "example.test/waybill-fixture-libmod";
    let toolmod = "example.test/waybill-fixture-toolmod";
    assert!(!marked.contains(libmod), "compiled into a reached lib; stderr:\n{}", run.scan.stderr);
    assert_eq!(run.c203(libmod).as_deref(), Some("reachability-walk"));
    assert!(marked.contains(toolmod), "compiled into a build tool only");
    assert_eq!(run.c203(toolmod).as_deref(), Some("reachability-walk"));
    assert!(marked.contains("waybill-fixture-tool") && !marked.contains("waybill-fixture-lib"));
}

/// The spec's fallback "errs toward runtime, never toward dropping linked
/// code". A `justStaticExecutables` wrapper (shellcheck's shape) names its
/// static Haskell build only in `inputs.drvs` and an `installPhase` path,
/// never in a dependency field, so the walk from it reaches nothing but the
/// root. Used as is, it would mark every library compiled into the binary.
/// Such a walk is not used: the role rule decides, and C216 says why.
#[test]
fn us2_walk_that_reaches_only_the_root_is_not_used() {
    let closure = fixture("closure-static.json");
    let run = scan_with(base(&closure));
    // The marks first: they are the defect, and the record only reports it.
    let marked = run.marked();
    for lib in HASKELL_LIBS.iter().chain(&["waybill-fixture-gmp"]) {
        assert!(!marked.contains(*lib), "{lib} is linked into the binary; marked: {marked:?}");
        assert_ne!(run.c203(lib).as_deref(), Some("reachability-walk"), "{lib}");
    }
    assert_eq!(
        run.c216().expect("C216"),
        serde_json::json!({
            "c-runtime-libraries": "not-assessed",
            // The rule reads the runtime closure's references (#1455).
            "doc-only-excluded": null,
            "lib-links-excluded": null,
            "method": "closure-role",
            "outputs": ["bin"],
            "outputs-source": "default-output",
            "schema": 1,
            "static-c-linking-detected": false,
            // The consumed static build's libraries are kept runtime (T042),
            // found through its derivation fields: no package set here.
            "static-link-roots": {"declared-edges": 0, "derivation-fields": 1},
            "static-link-source": "derivation-fields",
            "store": null,
            "unavailable-reason": "not-in-any-store",
            "unmatched-runtime-paths": null,
            "verified": null,
            "walk-outcome": "reached-only-root",
        }),
        "stderr:\n{}",
        run.scan.stderr
    );
}

/// T042: when the walk is set aside, the static Haskell build the wrapper
/// consumes through `inputs.drvs`, and the libraries linked into it, are
/// runtime. Their code ships inside the wrapper's binary, and the role rule
/// alone would mark the build itself (`build-tooling`). Everything else stays
/// on the role rule.
///
/// CONTROL, in the same scan: a Haskell derivation the static build uses only
/// as a build tool (`nativeBuildInputs`), which the root does not consume. It
/// is `build-tooling` too, and stays marked by the role rule.
#[test]
fn us2_reached_only_root_keeps_the_consumed_static_build_runtime() {
    let closure = with_alex(fixture("closure-static.json"));
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "closure-role", "{c216}");
    assert_eq!(c216["walk-outcome"], "reached-only-root", "{c216}");

    let marked = run.marked();
    for n in std::iter::once("waybill-fixture-ShellCheck").chain(HASKELL_LIBS) {
        assert!(!marked.contains(n), "{n} ships inside the wrapper's binary; marked: {marked:?}");
        assert_eq!(run.c203(n).as_deref(), Some("statically-linked"), "{n}");
    }
    assert_eq!(
        Run::cdx_prop(run.cdx("waybill-fixture-ShellCheck"), "waybill:closure-role"),
        Some("build-tooling"),
        "precondition: the role rule alone would mark the static build"
    );

    // CONTROL: not consumed by the root, so the role rule still decides.
    assert_eq!(
        Run::cdx_prop(run.cdx("waybill-fixture-alex"), "waybill:closure-role"),
        Some("build-tooling"),
        "precondition: the control is in a tooling role"
    );
    assert!(marked.contains("waybill-fixture-alex"), "marked: {marked:?}");
    assert_eq!(run.c203("waybill-fixture-alex").as_deref(), Some("closure-role"));
    // gmp is linked too, but is not a Haskell library: the static-link logic
    // does not follow it, so the role rule decides (`artifact-input`, unmarked).
    assert!(!marked.contains("waybill-fixture-gmp"));
    assert_eq!(run.c203("waybill-fixture-gmp"), None);
}

/// T042, the build-tool side: a Haskell build the root consumes but names in
/// `nativeBuildInputs` (nixpkgs' shellcheck consumes `pandoc-cli` that way,
/// for its man page) is not in the binary. It is not kept runtime, and neither
/// is what is linked into it; the role rule decides them. The static build is
/// still consumed, so the walk is still set aside.
#[test]
fn us2_reached_only_root_does_not_keep_a_consumed_build_tool_runtime() {
    let closure = with_pandoc(fixture("closure-static.json"));
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["walk-outcome"], "reached-only-root", "{c216}");
    let marked = run.marked();
    assert_eq!(
        Run::cdx_prop(run.cdx("waybill-fixture-pandoc-cli"), "waybill:closure-role"),
        Some("build-tooling"),
        "precondition"
    );
    assert!(marked.contains("waybill-fixture-pandoc-cli"), "marked: {marked:?}");
    assert_eq!(run.c203("waybill-fixture-pandoc-cli").as_deref(), Some("closure-role"));
    assert_ne!(run.c203("waybill-fixture-pdlib").as_deref(), Some("statically-linked"));
    // CONTROL: the static build is still kept runtime in the same scan.
    assert_eq!(run.c203("waybill-fixture-ShellCheck").as_deref(), Some("statically-linked"));
}

/// CONTROL for `us2_walk_that_reaches_only_the_root_is_not_used`, changing
/// one variable: the same closure with
/// the wrapper also naming its static build in `buildInputs`. The walk now
/// reaches past the root, so it is used, and the libraries are runtime by
/// the walk rather than by the role rule.
#[test]
fn us2_walk_that_reaches_past_the_root_is_used() {
    let closure = fixture("closure-static.json").replacen(
        "\"installPhase\":",
        "\"buildInputs\": \"/nix/store/h-SC\",\n        \"installPhase\":",
        1,
    );
    assert!(closure.contains("\"buildInputs\": \"/nix/store/h-SC\""), "the fixture edit applied");
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}");
    assert_eq!(c216["walk-outcome"], "used", "{c216}");
    assert_eq!(c216["unavailable-reason"], "not-in-any-store", "{c216}");
    let marked = run.marked();
    for lib in HASKELL_LIBS {
        assert!(!marked.contains(lib), "{lib}; marked: {marked:?}");
        assert_eq!(run.c203(lib).as_deref(), Some("reachability-walk"), "{lib}");
    }
}

/// CONTROL on the trigger, gh's shape: a walk that reaches only the root is
/// still used when the root consumed no Haskell build. gh's walk reaches 1
/// of 788 derivations and is exact on its findings (55 true, 0 false); the
/// role rule leaves 185 false-runtime findings there
/// (`specs/1423-nix-runtime-scope/measurements/us2-walk-score/`). One
/// variable against `closure-walk.json`: the root no longer names `ssl` in
/// `buildInputs`, only in `inputs.drvs`.
#[test]
fn us2_walk_reaching_only_a_non_wrapper_root_is_used() {
    let closure = fixture("closure-walk.json").replacen("\"buildInputs\": \"/nix/store/w-ssl\",", "", 1);
    assert!(!closure.contains("\"buildInputs\": \"/nix/store/w-ssl\""), "the fixture edit applied");
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}");
    assert_eq!(c216["walk-outcome"], "used", "{c216}");
    assert_eq!(
        run.marked(),
        names(&[
            "waybill-fixture-perl",
            "waybill-fixture-ssl",
            "waybill-fixture-tool",
            "waybill-fixture-xml",
            "waybill-fixture-z",
        ]),
        "everything but the root is unreached, and marked"
    );
}

/// Data-model rule 5: with no single evaluated package there is no root to
/// walk from, so the role rule decides and C216 says why. No runtime query is
/// made either: there is no output to ask about.
#[test]
fn us2_no_evaluated_package_falls_back_to_closure_role() {
    let closure = fixture("closure-tworoots.json");
    let run = scan_with(base(&closure));
    assert_eq!(
        run.c216().expect("C216"),
        serde_json::json!({
            "c-runtime-libraries": "not-assessed",
            // The rule reads the runtime closure's references (#1455).
            "doc-only-excluded": null,
            "lib-links-excluded": null,
            "method": "closure-role",
            "outputs": null,
            "outputs-source": null,
            "schema": 1,
            "static-c-linking-detected": false,
            "static-link-roots": null,
            "static-link-source": null,
            "store": null,
            "unavailable-reason": "no-evaluated-package",
            "unmatched-runtime-paths": null,
            "verified": null,
            "walk-outcome": null,
        }),
        "stderr:\n{}",
        run.scan.stderr
    );
    assert!(!run.nix_log().contains("path-info"), "{}", run.nix_log());
    assert!(!run.nix_log().contains("outputSpecified"), "no output to select:\n{}", run.nix_log());
    let marked = run.marked();
    assert!(marked.contains("waybill-fixture-tool"), "build-tooling, marked by the role rule");
    assert!(!marked.contains("waybill-fixture-lib"), "artifact-input, left alone by the role rule");
    assert_eq!(run.c203("waybill-fixture-tool").as_deref(), Some("closure-role"));
    assert_eq!(run.c203("waybill-fixture-lib"), None, "the role rule decides nothing about it");
}

// ---- #1455: reached only through documentation outputs ---------------------

/// Every path in `closure-doc-outputs.json`'s runtime closure.
fn doc_outputs_listing() -> String {
    listing(&[
        "h-app-bin",
        "h-app-doc",
        "h-ssl",
        "h-ssl-doc",
        "h-shared",
        "h-testfw-doc",
        "h-shell",
        "h-readline",
        "h-z",
        "h-z-man",
        "h-z-info",
        "h-groff",
        "h-texinfo",
        "h-xml-devdoc",
        "h-gtkdoc",
        "h-api-doc",
        "h-apidep",
    ])
}

/// The references, shaped on aarch64-darwin's hlint and moat closures
/// (`measurements/doc-outputs/`): the installed `bin` references the app's
/// own `-doc` output (haddock's `package.conf.d` link), whose pages link
/// other packages' `-doc` outputs; one such output references a shell
/// (`ghc-doc` → `bash-interactive`), which references readline. One output
/// of each documentation name leads somewhere only it reaches. `shared` is
/// referenced by a documentation output and by `ssl`'s `out`, and
/// `api-doc` is a package whose name ends in `-doc` but whose output is
/// `out`.
const DOC_OUTPUT_REFS: &[(&str, &str)] = &[
    ("h-app-bin", "h-app-doc"),
    ("h-app-bin", "h-ssl"),
    ("h-app-bin", "h-z"),
    ("h-app-bin", "h-z-man"),
    ("h-app-bin", "h-z-info"),
    ("h-app-bin", "h-xml-devdoc"),
    ("h-app-bin", "h-api-doc"),
    ("h-app-doc", "h-ssl-doc"),
    ("h-app-doc", "h-testfw-doc"),
    ("h-ssl", "h-shared"),
    ("h-ssl-doc", "h-shared"),
    ("h-ssl-doc", "h-shell"),
    ("h-shell", "h-readline"),
    ("h-z-man", "h-groff"),
    ("h-z-info", "h-texinfo"),
    ("h-xml-devdoc", "h-gtkdoc"),
    ("h-api-doc", "h-apidep"),
];

/// The members the runtime closure holds only through a documentation output.
const DOC_ONLY: &[&str] = &[
    // its only runtime path is its own `-doc` output
    "waybill-fixture-testfw",
    // only through `ssl`'s `-doc`, directly and then transitively
    "waybill-fixture-shell",
    "waybill-fixture-readline",
    // only through `z`'s `man` and `info`, and `xml`'s `devdoc`
    "waybill-fixture-groff",
    "waybill-fixture-texinfo",
    "waybill-fixture-gtkdoc",
    // its only runtime path is its `devdoc` output
    "waybill-fixture-xml",
];

/// #1455, the maintainer's rule: a member the runtime closure reaches only
/// through `doc`, `devdoc`, `man` or `info` outputs does not ship. It is
/// marked in all three formats with C203 `runtime-closure`, and C216 says
/// which output names were treated as documentation and how many members
/// and paths the rule excluded.
#[test]
fn a_member_reached_only_through_documentation_outputs_does_not_ship() {
    let closure = fixture("closure-doc-outputs.json");
    let run = scan_with(NixStub {
        runtime: Some(doc_outputs_listing()),
        runtime_refs: DOC_OUTPUT_REFS.to_vec(),
        ..base(&closure)
    });
    assert_eq!(run.marked(), names(DOC_ONLY), "stderr:\n{}", run.scan.stderr);
    for n in DOC_ONLY {
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    for n in [
        "waybill-fixture-ssl",
        // a documentation output references it, and so does `ssl`'s `out`
        "waybill-fixture-shared",
        "waybill-fixture-z",
        // named `-doc`, but its output is `out`: the output name decides
        "waybill-fixture-api-doc",
        "waybill-fixture-apidep",
    ] {
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    let c216 = run.c216().expect("C216");
    assert_eq!(
        c216["doc-only-excluded"],
        serde_json::json!({
            "members": DOC_ONLY.len(),
            "output-names": ["devdoc", "doc", "info", "man"],
            // six documentation outputs, and the five paths only they reach
            "paths": 11,
        }),
        "{c216}"
    );
    assert_eq!(c216["unmatched-runtime-paths"], 0, "{c216}");
    assert_eq!(
        path_info_calls(&run),
        vec!["path-info -r --json /nix/store/h-app-bin"],
        "the references come from the same query as the listing\n{}",
        run.nix_log()
    );
}

/// CONTROL for the test above: the same closure and listing, with the
/// installed output referencing every path itself. The five members only a
/// documentation output led to now ship, which shows those marks came from
/// the references. The two whose only runtime path *is* a documentation
/// output stay marked: what ships of them is documentation.
#[test]
fn a_member_also_reached_through_a_non_documentation_output_ships() {
    let closure = fixture("closure-doc-outputs.json");
    let direct: Vec<(&str, &str)> = DOC_OUTPUT_REFS
        .iter()
        .map(|(_, to)| ("h-app-bin", *to))
        .collect();
    let run = scan_with(NixStub {
        runtime: Some(doc_outputs_listing()),
        runtime_refs: direct,
        ..base(&closure)
    });
    assert_eq!(
        run.marked(),
        names(&["waybill-fixture-testfw", "waybill-fixture-xml"]),
        "stderr:\n{}",
        run.scan.stderr
    );
    let c216 = run.c216().expect("C216");
    assert_eq!(
        c216["doc-only-excluded"],
        serde_json::json!({
            "members": 2,
            "output-names": ["devdoc", "doc", "info", "man"],
            "paths": 6,
        }),
        "{c216}"
    );
}

/// #1455: under `--json` a store that does not hold the output can answer
/// `{"<path>":null}` with exit 0. The corpus run's nix did, and the first
/// version of this change read that as a one-path runtime closure, marking
/// almost everything in moat and shellcheck. It is a miss: the walk decides,
/// and C216 says the output was in no store.
#[test]
fn a_null_json_answer_is_not_a_runtime_closure() {
    let closure = fixture("closure-runtime.json");
    let run = scan_with(NixStub {
        json_miss_is_null: true,
        substituters: vec![SUBSTITUTER.to_string()],
        ..base(&closure)
    });
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}\n{}", run.nix_log());
    assert_eq!(c216["unavailable-reason"], "not-in-any-store", "{c216}");
    assert!(
        run.nix_log().contains(&format!("--json --store {SUBSTITUTER}")),
        "the substituter was asked too, and answered null:\n{}",
        run.nix_log()
    );
    assert!(!run.marked().contains("waybill-fixture-ssl"), "the walk reaches the app's buildInputs");
}

/// The rule reads the runtime closure's references, so it applies only when
/// the runtime closure decided: the walk has neither, and C216 says so.
#[test]
fn the_documentation_rule_is_null_when_the_walk_decides() {
    let closure = fixture("closure-doc-outputs.json");
    let run = scan_with(base(&closure));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["method"], "reachability-walk", "{c216}");
    // Present and null: indexing an absent key also yields null.
    assert_eq!(c216.get("doc-only-excluded"), Some(&Value::Null), "{c216}");
}

// ---- #1455: references that exist only because of darwin's lib/links ------

const LL_APP: &str = "11111111111111111111111111111111-waybill-fixture-app-1.0";
const LL_USED: &str = "11111111111111111111111111111112-waybill-fixture-used-1.0";
const LL_TESTFW: &str = "11111111111111111111111111111113-waybill-fixture-testfw-2.11";
const LL_TESTDEP: &str = "11111111111111111111111111111114-waybill-fixture-testdep-0.3";
const LL_DECL: &str = "11111111111111111111111111111115-waybill-fixture-decl-1.0";
const LL_CHAIN: &str = "11111111111111111111111111111116-waybill-fixture-chain-1.0";

/// A minimal 64-bit little-endian Mach-O dylib whose only load commands are
/// `LC_LOAD_DYLIB` for each of `loads`, as `otool -L` would list them.
fn macho_dylib(loads: &[&str]) -> Vec<u8> {
    let mut cmds: Vec<u8> = Vec::new();
    for name in loads {
        let mut payload = name.as_bytes().to_vec();
        payload.push(0);
        while (24 + payload.len()) % 8 != 0 {
            payload.push(0);
        }
        let size = (24 + payload.len()) as u32;
        for v in [0x0c_u32, size, 24, 2, 0x0001_0000, 0x0001_0000] {
            cmds.extend_from_slice(&v.to_le_bytes());
        }
        cmds.extend_from_slice(&payload);
    }
    let mut out: Vec<u8> = Vec::new();
    for v in [0xfeed_facf_u32, 0x0100_000c, 0, 6, loads.len() as u32, cmds.len() as u32, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&cmds);
    out
}

/// The darwin shape moat's closure has, built as files in a store directory:
/// the app's `lib/links` farm points at every library its build environment
/// held, its dylib loads only `used`, `used` loads `chain` (resolved through
/// the app's farm, as dyld does through the loader's rpaths), `decl` is named
/// in `nix-support/propagated-build-inputs`, and `testfw` (with its own farm
/// entry for `testdep`) is in the farm and nowhere else. `app_loads` adds
/// install names the app's dylib loads.
fn lib_links_store(app_loads: &[&str]) -> tempfile::TempDir {
    use std::os::unix::fs::symlink;
    let store = tempfile::tempdir().unwrap();
    let s = store.path();
    let put = |out: &str, file: &str, bytes: &[u8]| {
        let p = s.join(out).join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    };
    let link = |out: &str, entry: &str, target_out: &str, target_file: &str| {
        let dir = s.join(out).join("lib/links");
        std::fs::create_dir_all(&dir).unwrap();
        symlink(s.join(target_out).join(target_file), dir.join(entry)).unwrap();
    };
    let mut loads = vec!["@rpath/libHSused.dylib", "/usr/lib/libSystem.B.dylib"];
    loads.extend_from_slice(app_loads);
    put(LL_APP, "lib/libHSapp.dylib", &macho_dylib(&loads));
    for (entry, out) in [
        ("libHSused.dylib", LL_USED),
        ("libHSchain.dylib", LL_CHAIN),
        ("libHStestfw.dylib", LL_TESTFW),
        ("libHSdecl.dylib", LL_DECL),
    ] {
        link(LL_APP, entry, out, &format!("lib/{entry}"));
    }
    let pbi = format!("{} {}\n", s.join(LL_DECL).display(), s.join(LL_USED).display());
    put(LL_APP, "nix-support/propagated-build-inputs", pbi.as_bytes());
    put(LL_USED, "lib/libHSused.dylib", &macho_dylib(&["@rpath/libHSchain.dylib"]));
    put(LL_CHAIN, "lib/libHSchain.dylib", &macho_dylib(&[]));
    put(LL_TESTFW, "lib/libHStestfw.dylib", &macho_dylib(&["@rpath/libHStestdep.dylib"]));
    link(LL_TESTFW, "libHStestdep.dylib", LL_TESTDEP, "lib/libHStestdep.dylib");
    put(LL_TESTDEP, "lib/libHStestdep.dylib", &macho_dylib(&[]));
    put(LL_DECL, "lib/libHSdecl.dylib", &macho_dylib(&[]));
    store
}

fn lib_links_listing() -> String {
    listing(&[LL_APP, LL_USED, LL_TESTFW, LL_TESTDEP, LL_DECL, LL_CHAIN])
}

/// The references nix reports: the app references every farm target, and
/// testfw its own farm target.
const LL_REFS: &[(&str, &str)] = &[
    (LL_APP, LL_USED),
    (LL_APP, LL_CHAIN),
    (LL_APP, LL_TESTFW),
    (LL_APP, LL_DECL),
    (LL_TESTFW, LL_TESTDEP),
];

fn lib_links_run(store: &tempfile::TempDir, runtime_store: Option<String>) -> Run {
    let closure = fixture("closure-lib-links.json");
    let subs = runtime_store.iter().cloned().collect();
    scan_with(NixStub {
        runtime: Some(lib_links_listing()),
        runtime_refs: LL_REFS.to_vec(),
        runtime_store,
        substituters: subs,
        store_dir: Some(store.path().to_path_buf()),
        ..base(&closure)
    })
}

/// #1455, the maintainer's second rule (option 1): a reference that exists
/// only because of a `lib/links` symlink does not ship its target, unless a
/// Mach-O file in a shipping output loads the dylib that symlink names. On
/// moat, that is what keeps hspec, tasty and QuickCheck in the runtime
/// closure (`measurements/lib-links/`).
#[test]
fn a_lib_links_entry_nothing_loads_does_not_ship_its_target() {
    let store = lib_links_store(&[]);
    let run = lib_links_run(&store, None);
    assert_eq!(
        run.marked(),
        names(&["waybill-fixture-testfw", "waybill-fixture-testdep"]),
        "stderr:\n{}",
        run.scan.stderr
    );
    for n in ["waybill-fixture-testfw", "waybill-fixture-testdep", "waybill-fixture-used"] {
        assert_eq!(run.c203(n).as_deref(), Some("runtime-closure"), "{n}");
    }
    let c216 = run.c216().expect("C216");
    assert_eq!(
        c216["lib-links-excluded"],
        serde_json::json!({
            "members": 2,
            "not-assessed": null,
            "paths": 2,
            "rule": "unloaded-lib-links",
        }),
        "{c216}"
    );
}

/// The recall guard (#1479 review): moat is a library, so its own library
/// dependencies ship by being linked into whatever consumes it, not by its
/// dylib loading them. Here the app is a Haskell library whose derivation
/// propagates `decl`, but nothing in the store names `decl` outside the app's
/// `lib/links` and nothing loads it. The `lib/links` rule sets its path
/// aside, and the static-link set, seeded from the shipping library, keeps
/// it runtime as statically linked. On moat every package in its
/// `package.conf.d` library closure ships (`measurements/lib-links/`).
#[test]
fn a_shipping_librarys_own_library_dependency_stays_runtime() {
    let store = lib_links_store(&[]);
    std::fs::remove_file(store.path().join(LL_APP).join("nix-support/propagated-build-inputs"))
        .unwrap();
    let closure = fixture("closure-lib-links-haskell.json");
    let run = scan_with(NixStub {
        runtime: Some(lib_links_listing()),
        runtime_refs: LL_REFS.to_vec(),
        store_dir: Some(store.path().to_path_buf()),
        ..base(&closure)
    });
    assert_eq!(
        run.marked(),
        names(&["waybill-fixture-testfw", "waybill-fixture-testdep"]),
        "stderr:\n{}",
        run.scan.stderr
    );
    assert_eq!(run.c203("waybill-fixture-decl").as_deref(), Some("statically-linked"));
    let c216 = run.c216().expect("C216");
    // `decl`'s path was set aside; the member was not, so it is not counted.
    assert_eq!(c216["lib-links-excluded"]["members"], 2, "{c216}");
    assert_eq!(c216["lib-links-excluded"]["paths"], 3, "{c216}");
}

/// The hard constraint: a dylib that a Mach-O file in a shipped output loads
/// ships, and so does what it loads in turn.
#[test]
fn a_dylib_a_shipped_binary_loads_still_ships() {
    let store = lib_links_store(&["@rpath/libHStestfw.dylib"]);
    let run = lib_links_run(&store, None);
    assert_eq!(run.marked(), BTreeSet::new(), "stderr:\n{}", run.scan.stderr);
    assert_eq!(run.c216().expect("C216")["lib-links-excluded"]["members"], 0);
}

/// A runtime closure from a substituter comes without its files, so which
/// file holds a reference cannot be told. The reference keeps counting and
/// C216 says the rule was not assessed.
#[test]
fn a_remote_runtime_closure_is_not_assessed_for_lib_links() {
    let store = lib_links_store(&[]);
    let run = lib_links_run(&store, Some(SUBSTITUTER.to_string()));
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["store"], SUBSTITUTER, "{c216}");
    assert_eq!(
        c216["lib-links-excluded"],
        serde_json::json!({
            "members": null,
            "not-assessed": "remote-store",
            "paths": null,
            "rule": "unloaded-lib-links",
        }),
        "{c216}"
    );
    assert_eq!(run.marked(), BTreeSet::new());
}

/// A shipping output that cannot be read could load anything, so nothing is
/// dropped and C216 says why.
#[test]
fn an_unreadable_shipping_output_leaves_lib_links_unassessed() {
    let store = lib_links_store(&[]);
    std::fs::remove_dir_all(store.path().join(LL_USED)).unwrap();
    let run = lib_links_run(&store, None);
    let c216 = run.c216().expect("C216");
    assert_eq!(c216["lib-links-excluded"]["not-assessed"], "unreadable-output", "{c216}");
    assert_eq!(c216["lib-links-excluded"]["members"], Value::Null, "{c216}");
    assert_eq!(run.marked(), BTreeSet::new());
}

// ---- US4: the runtime-only recipe (FR-011, FR-012) ------------------------

/// A CVE-named patch on a member the recipe keeps (`ssl`, in the runtime
/// closure) and on one it drops (`xml`, an `artifact-input` the output does
/// not reference). Each yields a CycloneDX backport pair (#1436): a
/// `not_affected` statement naming the subject with the patched member as
/// `waybill:vex-subcomponent`, and a version-scoped `exploitable` one.
const KEPT_CVE: &str = "CVE-2026-90031";
const DROPPED_CVE: &str = "CVE-2026-90032";

fn us4_closure() -> String {
    let mut c: Value = serde_json::from_str(&fixture("closure-runtime.json")).unwrap();
    c["derivations"]["d-ssl.drv"]["env"]["patches"] = Value::String(format!("/nix/store/p1-{KEPT_CVE}.patch"));
    c["derivations"]["d-xml.drv"]["env"]["patches"] = Value::String(format!("/nix/store/p2-{DROPPED_CVE}.patch"));
    c.to_string()
}

fn cdx_walk<'a>(cs: &'a [Value], out: &mut Vec<&'a Value>) {
    for c in cs {
        out.push(c);
        cdx_walk(c["components"].as_array().map(Vec::as_slice).unwrap_or(&[]), out);
    }
}

/// Every CycloneDX component, nested ones included; with the subject first
/// when `subject` is set.
fn cdx_all(doc: &Value, subject: bool) -> Vec<&Value> {
    let mut out = Vec::new();
    if subject {
        cdx_walk(std::slice::from_ref(&doc["metadata"]["component"]), &mut out);
    }
    cdx_walk(doc["components"].as_array().map(Vec::as_slice).unwrap_or(&[]), &mut out);
    out
}

/// SPDX 2.3 package SPDXID → PURL.
fn spdx23_purls(doc: &Value) -> HashMap<&str, &str> {
    doc["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = p["SPDXID"].as_str()?;
            let r = p["externalRefs"].as_array()?.iter().find(|r| r["referenceType"] == "purl")?;
            Some((id, r["referenceLocator"].as_str()?))
        })
        .collect()
}

/// SPDX 3 element IRI → PURL.
fn spdx3_purls(doc: &Value) -> HashMap<&str, &str> {
    doc["@graph"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| Some((e["spdxId"].as_str()?, e["software_packageUrl"].as_str()?)))
        .collect()
}

/// Every component PURL, in any of the three formats.
fn us4_purls(doc: &Value) -> BTreeSet<String> {
    match sbom_edit_support::format_of(doc) {
        "cdx" => cdx_all(doc, true).into_iter().filter_map(|c| c["purl"].as_str().map(str::to_string)).collect(),
        "spdx23" => spdx23_purls(doc).into_values().map(str::to_string).collect(),
        _ => spdx3_purls(doc).into_values().map(str::to_string).collect(),
    }
}

/// The subject's PURL, as each format names it.
fn us4_subject(doc: &Value) -> Option<String> {
    match sbom_edit_support::format_of(doc) {
        "cdx" => doc["metadata"]["component"]["purl"].as_str().map(str::to_string),
        "spdx23" => spdx23_purls(doc).get(doc["documentDescribes"][0].as_str()?).map(|p| p.to_string()),
        _ => {
            let graph = doc["@graph"].as_array()?;
            let root = graph.iter().find(|e| e["type"] == "SpdxDocument")?["rootElement"][0].as_str()?;
            spdx3_purls(doc).get(root).map(|p| p.to_string())
        }
    }
}

/// Components carrying the not-in-artifact mark, by the contract's rule for
/// each format (`contracts/sbom-edit-scope-excluded.md`), as PURLs.
fn us4_not_in_artifact(doc: &Value) -> BTreeSet<String> {
    match sbom_edit_support::format_of(doc) {
        "cdx" => cdx_all(doc, false)
            .into_iter()
            .filter(|c| c["scope"] == "excluded")
            .filter(|c| {
                !c["properties"].as_array().into_iter().flatten().any(|p| {
                    p["name"] == "waybill:lifecycle-scope" || p["name"] == "waybill:optional-derivation"
                })
            })
            .filter_map(|c| c["purl"].as_str().map(str::to_string))
            .collect(),
        "spdx23" => {
            let purl = spdx23_purls(doc);
            doc["relationships"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["relationshipType"] == "BUILD_TOOL_OF")
                .filter_map(|r| purl.get(r["spdxElementId"].as_str()?).map(|p| p.to_string()))
                .collect()
        }
        _ => {
            let purl = spdx3_purls(doc);
            doc["@graph"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| e["type"] == "LifecycleScopedRelationship")
                .filter(|e| e["relationshipType"] == "usesTool" && e["scope"] == "build")
                .flat_map(|e| e["to"].as_array().into_iter().flatten())
                .filter_map(|t| purl.get(t.as_str()?).map(|p| p.to_string()))
                .collect()
        }
    }
}

/// Every reference a relationship, dependency or VEX statement makes that
/// names nothing in the document.
fn us4_dangling(doc: &Value) -> Vec<String> {
    let mut dangling = Vec::new();
    match sbom_edit_support::format_of(doc) {
        "cdx" => {
            let refs: BTreeSet<&str> = cdx_all(doc, true).into_iter().filter_map(|c| c["bom-ref"].as_str()).collect();
            let mut check = |what: String, r: &str| {
                if !refs.contains(r) {
                    dangling.push(format!("{what}: {r}"));
                }
            };
            for d in doc["dependencies"].as_array().into_iter().flatten() {
                check("dependencies.ref".into(), d["ref"].as_str().unwrap_or_default());
                for t in d["dependsOn"].as_array().into_iter().flatten() {
                    check("dependencies.dependsOn".into(), t.as_str().unwrap_or_default());
                }
            }
            for v in doc["vulnerabilities"].as_array().into_iter().flatten() {
                for a in v["affects"].as_array().into_iter().flatten() {
                    check(format!("{} affects", v["id"]), a["ref"].as_str().unwrap_or_default());
                }
                for p in v["properties"].as_array().into_iter().flatten() {
                    if p["name"] == "waybill:vex-subcomponent" {
                        check(format!("{} subcomponent", v["id"]), p["value"].as_str().unwrap_or_default());
                    }
                }
            }
        }
        "spdx23" => {
            let ids: BTreeSet<&str> = ["packages", "files"]
                .iter()
                .flat_map(|k| doc[*k].as_array().into_iter().flatten())
                .filter_map(|p| p["SPDXID"].as_str())
                .chain(["SPDXRef-DOCUMENT", "NOASSERTION", "NONE"])
                .collect();
            let resolves = |id: &str| ids.contains(id) || id.starts_with("DocumentRef-");
            for r in doc["relationships"].as_array().into_iter().flatten() {
                for k in ["spdxElementId", "relatedSpdxElement"] {
                    let id = r[k].as_str().unwrap_or_default();
                    if !resolves(id) {
                        dangling.push(format!("{} {k}: {id}", r["relationshipType"]));
                    }
                }
            }
            for d in doc["documentDescribes"].as_array().into_iter().flatten() {
                let id = d.as_str().unwrap_or_default();
                if !resolves(id) {
                    dangling.push(format!("documentDescribes: {id}"));
                }
            }
        }
        _ => {
            let graph = doc["@graph"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let mut ids: BTreeSet<&str> = graph.iter().filter_map(|e| e["spdxId"].as_str()).collect();
            ids.extend(
                graph
                    .iter()
                    .flat_map(|e| e["import"].as_array().into_iter().flatten())
                    .filter_map(|m| m["externalSpdxId"].as_str()),
            );
            for e in graph {
                let kind = e["type"].as_str().unwrap_or_default();
                let mut named: Vec<&str> = Vec::new();
                if kind.ends_with("Relationship") {
                    named.extend(e["from"].as_str());
                    named.extend(e["to"].as_array().into_iter().flatten().filter_map(Value::as_str));
                    named.extend(e["security_assessedElement"].as_str());
                }
                if kind == "Annotation" {
                    named.extend(e["subject"].as_str());
                }
                if kind == "SpdxDocument" || kind == "software_Sbom" {
                    named.extend(e["rootElement"].as_array().into_iter().flatten().filter_map(Value::as_str));
                }
                for id in named {
                    if !ids.contains(id) {
                        dangling.push(format!("{kind} {}: {id}", e["relationshipType"]));
                    }
                }
            }
        }
    }
    dangling
}

/// The CycloneDX VEX statements as (id, analysis state).
fn us4_vex(doc: &Value) -> BTreeSet<(String, String)> {
    doc["vulnerabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let s = |x: &Value| x.as_str().unwrap_or_default().to_string();
            (s(&v["id"]), s(&v["analysis"]["state"]))
        })
        .collect()
}

fn us4_backport_pair(cve: &str) -> BTreeSet<(String, String)> {
    ["exploitable", "not_affected"].iter().map(|s| (cve.to_string(), s.to_string())).collect()
}

/// FR-011 / FR-012 / US4 scenario 1: the documented recipe,
/// `sbom edit <doc> --drop 'scope=excluded'`, turns each of a closure scan's
/// three documents into a valid runtime-only document. Each result validates
/// against its schema (SPDX 3 also through `spdx3-validate` when installed),
/// keeps the evaluated package as its subject, names nothing it no longer
/// holds, and holds no not-in-artifact member. The three formats lose the
/// same PURLs.
///
/// VEX about a kept member survives and VEX about a dropped member goes. Only
/// CycloneDX carries VEX in the document; SPDX's OpenVEX sidecar is not
/// edited (`docs/user-guide/sbom-edit.md`, "Limits").
#[test]
fn us4_drop_scope_excluded_yields_valid_runtime_document() {
    let closure = us4_closure();
    let run = scan_with(NixStub { runtime: Some(curl_runtime()), ..base(&closure) });
    assert_eq!(run.c216().expect("C216")["method"], "runtime-closure", "stderr:\n{}", run.scan.stderr);

    let subject = "pkg:generic/waybill-fixture-app@1.0";
    let dir = tempfile::tempdir().unwrap();
    let mut dropped_by_format: Vec<(&str, BTreeSet<String>)> = Vec::new();
    for (label, file, doc) in [
        ("cdx", "closure.cdx.json", &run.scan.cdx),
        ("spdx23", "closure.spdx.json", &run.scan.spdx23),
        ("spdx3", "closure.spdx3.json", &run.scan.spdx3),
    ] {
        // CONTROLS on the input: it is valid, it names this subject, and it
        // holds marked members, so every check below can fail.
        assert_eq!(sbom_edit_support::schema_errors(doc), BTreeSet::new(), "{label}: the scan itself fails its schema");
        assert_eq!(us4_subject(doc).as_deref(), Some(subject), "{label}: input subject");
        assert_eq!(us4_dangling(doc), Vec::<String>::new(), "{label}: the scan itself has references to nothing");
        let marked = us4_not_in_artifact(doc);
        assert!(!marked.is_empty(), "{label}: the scan marked nothing, so the recipe would have nothing to drop");

        let input = dir.path().join(file);
        std::fs::write(&input, serde_json::to_string_pretty(doc).unwrap()).unwrap();
        let e = sbom_edit_support::edit(&input, &["--drop", "scope=excluded"]);
        e.assert_ok();
        let out = e.json();

        assert_eq!(
            sbom_edit_support::schema_errors(&out),
            BTreeSet::new(),
            "{label}: the runtime-only document fails its schema"
        );
        if label == "spdx3" {
            sbom_edit_support::spdx3_validate_or_skip(&e.path);
        }
        assert_eq!(us4_subject(&out).as_deref(), Some(subject), "{label}: the subject moved");
        assert_eq!(us4_dangling(&out), Vec::<String>::new(), "{label}: references to nothing");
        assert_eq!(us4_not_in_artifact(&out), BTreeSet::new(), "{label}: a not-in-artifact member remains");

        let dropped: BTreeSet<String> = us4_purls(doc).difference(&us4_purls(&out)).cloned().collect();
        assert_eq!(dropped, marked, "{label}: the recipe dropped something other than the marked set");
        assert!(
            e.stderr.contains(&format!("drop-components: matched {},", dropped.len())),
            "{label}: the reported count disagrees with the {} PURLs removed: {}",
            dropped.len(),
            e.stderr
        );
        dropped_by_format.push((label, dropped));

        if label == "cdx" {
            // CONTROL: the scan wrote both backport pairs.
            let both: BTreeSet<_> = us4_backport_pair(KEPT_CVE).union(&us4_backport_pair(DROPPED_CVE)).cloned().collect();
            assert_eq!(us4_vex(doc), both, "input VEX");
            assert_eq!(
                us4_vex(&out),
                us4_backport_pair(KEPT_CVE),
                "FR-012: VEX about the kept member stays, VEX about the dropped one goes"
            );
            assert!(e.stderr.contains("vulnerability statement(s) removed"), "{}", e.stderr);
        }
    }

    // The same PURLs leave every format, and they are the members outside
    // the runtime closure (see `us1_runtime_members_unmarked_and_others_marked`).
    let cdx = &dropped_by_format[0].1;
    for (label, set) in &dropped_by_format[1..] {
        assert_eq!(set, cdx, "{label} dropped a different PURL set from CycloneDX");
    }
    let names: BTreeSet<&str> = cdx
        .iter()
        .filter_map(|p| p.strip_prefix("pkg:generic/")?.split('@').next())
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["waybill-fixture-bootlib", "waybill-fixture-py", "waybill-fixture-xml"]),
        "dropped: {cdx:?}"
    );
}
