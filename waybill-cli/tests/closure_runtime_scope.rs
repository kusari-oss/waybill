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
//! `nix path-info -r` from a listing the test supplies and makes no network
//! access. Every name is `waybill-fixture-*` (#285, #640).
//!
//! # Fixtures (`tests/fixtures/nix_closure_runtime/`)
//!
//! | file | shape | what it exercises |
//! |---|---|---|
//! | `closure-runtime.json` | curl / openssh | `xml` is an `artifact-input` the output does not reference (marked); `dnsroot` is `build-tooling` but referenced (unmarked); `bootlib` is a compiler-supplied boot library |
//! | `closure-static.json` | shellcheck / ormolu | a static Haskell executable whose libraries are in its static build's `buildInputs` and propagate; `hspec` is a test-only dependency |
//! | `closure-embedded.json` | gh | Go modules read from a runtime binary and from a build tool's binary |

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

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

/// A runtime listing in `nix path-info -r`'s plain form.
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

    // CONTROL: no store holds the output, so the role decides and marks it.
    let control = scan_with(base(&closure));
    assert_eq!(
        control.c216().expect("C216")["unavailable-reason"],
        "not-in-any-store",
        "the stub answered `path '…' is not valid` from every store"
    );
    assert!(
        control.marked().contains("waybill-fixture-dnsroot"),
        "without a runtime answer the role rule marks it -- otherwise this test \
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
            "method": "runtime-closure",
            "schema": 1,
            "static-c-linking-detected": false,
            "static-link-source": "none",
            "store": "local",
            "unavailable-reason": null,
            "unmatched-runtime-paths": 1,
            "verified": true,
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
    assert_ne!(c216["method"], "runtime-closure", "{c216}");
    assert_eq!(c216["unavailable-reason"], "verification-failed", "{c216}");
    assert_eq!(c216["store"], Value::Null);
    assert_eq!(c216["verified"], Value::Null);
    assert!(
        !run.marked().contains("waybill-fixture-xml"),
        "the unverified listing must not decide anything: `xml` is marked only \
         by the runtime closure, and an `artifact-input` under either fallback"
    );
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
