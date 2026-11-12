//! Milestone 1378 (#1396) — the Go modules and Rust crates compiled into a
//! nix-built binary appear in a `--nix-closure` document.
//!
//! The closure tier enumerates derivations, and a Go or Rust program's
//! dependencies are not derivations: measured, a closure scan of `gh` listed
//! 0 Go modules where the binary holds 163. The tier now reads each member's
//! outputs that are already present in the store.
//!
//! **Every assertion is on the emitted documents**, in all three formats.
//! The unit tests in `closure/contents.rs` cover the pass; what only an
//! end-to-end run can show is that the modules, their edges and the read
//! record survive emission identically everywhere.
//!
//! Nothing here touches a real nix or a real store. A stub `nix` answers
//! `derivation show` from a fixture, and `NIX_STORE_DIR` points the scan at a
//! temp directory holding binaries synthesized below. The binaries use
//! synthetic names only.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";
const APP: &str = "h0-waybill-fixture-app-1.2.3";
const LIB: &str = "h1-waybill-fixture-lib-2.0.0";
const TOOL: &str = "h2-waybill-fixture-tool-3.0.0";

const C213: &str = "waybill:nix-closure-contents";
const C214: &str = "waybill:closure-contents";
const C215: &str = "waybill:closure-binary-identity";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

fn closure() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_contents/app.json");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ---- synthetic binaries -------------------------------------------------

fn uvarint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
    out
}

/// A file the Go BuildInfo reader accepts: ELF magic, padding past the size
/// floor, then an inline BuildInfo blob.
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

/// A minimal ELF64 with a `.dep-v0` section: what cargo-auditable writes.
/// `crates` is `(name, version, source)`; the first is the root.
fn cargo_binary(crates: &[(&str, &str, &str)]) -> Vec<u8> {
    let packages: Vec<Value> = crates
        .iter()
        .enumerate()
        .map(|(i, (name, version, source))| {
            let mut p = serde_json::json!({
                "name": name, "version": version, "source": source, "kind": "runtime",
            });
            if i == 0 {
                p["root"] = Value::Bool(true);
                p["dependencies"] = serde_json::json!((1..crates.len()).collect::<Vec<_>>());
            }
            p
        })
        .collect();
    let json = serde_json::json!({ "packages": packages }).to_string();
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(json.as_bytes()).unwrap();
    let data = enc.finish().unwrap();

    let mut shstrtab = vec![0u8];
    let name_off = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".dep-v0\0");
    let shstr_off = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".shstrtab\0");
    let data_off = 64u64;
    let pad = 2048usize.saturating_sub(data.len());
    let strtab_off = data_off + data.len() as u64 + pad as u64;
    let shoff = strtab_off + shstrtab.len() as u64;

    let mut f = Vec::new();
    f.extend_from_slice(b"\x7fELF");
    f.extend_from_slice(&[2, 1, 1, 0]);
    f.extend_from_slice(&[0u8; 8]);
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(&62u16.to_le_bytes());
    f.extend_from_slice(&1u32.to_le_bytes());
    f.extend_from_slice(&0u64.to_le_bytes());
    f.extend_from_slice(&0u64.to_le_bytes());
    f.extend_from_slice(&shoff.to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes());
    f.extend_from_slice(&64u16.to_le_bytes());
    f.extend_from_slice(&0u16.to_le_bytes());
    f.extend_from_slice(&0u16.to_le_bytes());
    f.extend_from_slice(&64u16.to_le_bytes());
    f.extend_from_slice(&3u16.to_le_bytes());
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(&data);
    f.extend(std::iter::repeat_n(0u8, pad));
    f.extend_from_slice(&shstrtab);
    let mut sh = |name: u32, ty: u32, off: u64, size: u64| {
        f.extend_from_slice(&name.to_le_bytes());
        f.extend_from_slice(&ty.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
        f.extend_from_slice(&off.to_le_bytes());
        f.extend_from_slice(&size.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&1u64.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
    };
    sh(0, 0, 0, 0);
    sh(name_off, 1, data_off, data.len() as u64);
    sh(shstr_off, 3, strtab_off, shstrtab.len() as u64);
    f
}

fn put(store: &Path, output: &str, file: &str, bytes: &[u8]) {
    let dir = store.join(output).join("bin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), bytes).unwrap();
}

/// The root holds a Go program; the build tool holds a Rust one. Both embed
/// `shared`-style dependencies of their own ecosystem.
fn populated_store() -> tempfile::TempDir {
    let store = tempfile::tempdir().unwrap();
    put(
        store.path(),
        APP,
        "app",
        &go_binary(
            "example.test/waybill-fixture-app",
            &[
                ("example.test/waybill-fixture-goa", "v1.0.0"),
                ("example.test/waybill-fixture-gob", "v0.2.0"),
            ],
        ),
    );
    put(
        store.path(),
        TOOL,
        "tool",
        &cargo_binary(&[
            ("waybill-fixture-tool", "3.0.0", "local"),
            ("waybill-fixture-cratea", "0.5.0", "crates.io"),
        ]),
    );
    store
}

// ---- harness ------------------------------------------------------------

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

fn stub_script(closure: &str) -> String {
    format!(
        r##"#!/bin/sh
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
"##
    )
}

struct Run {
    output: Output,
    cdx_path: PathBuf,
    spdx23_path: PathBuf,
    spdx3_path: PathBuf,
    _dir: tempfile::TempDir,
    _cache: tempfile::TempDir,
}

impl Run {
    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }
    fn read(&self, p: &Path) -> Value {
        assert!(p.exists(), "no document at {}.\nstderr:\n{}", p.display(), self.stderr());
        serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
    }
    fn cdx(&self) -> Value {
        self.read(&self.cdx_path)
    }
    fn spdx23(&self) -> Value {
        self.read(&self.spdx23_path)
    }
    fn spdx3(&self) -> Value {
        self.read(&self.spdx3_path)
    }
}

/// `--no-deps-dev` throughout: hermetic, and it is the path on which a
/// candidate identity must be recorded and NOT promoted.
fn scan(store: &Path, extra: &[&str]) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let cache = seed_cache();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let nix = bin.join("nix");
    std::fs::write(&nix, stub_script(&closure())).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let cdx_path = dir.path().join("out.cdx.json");
    let spdx23_path = dir.path().join("out.spdx23.json");
    let spdx3_path = dir.path().join("out.spdx3.json");
    let output = Command::new(binary_path())
        .args(["sbom", "scan", "--path", project().to_str().unwrap()])
        .args(["--no-deep-hash", "--no-deps-dev", "--nix-closure"])
        .args(["--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.display()))
        .args(extra)
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
        .env("WAYBILL_NIXPKGS_CACHE", cache.path())
        .env("NIX_STORE_DIR", store)
        .output()
        .unwrap();
    Run { output, cdx_path, spdx23_path, spdx3_path, _dir: dir, _cache: cache }
}

// ---- document readers ---------------------------------------------------

fn cdx_components(doc: &Value) -> &Vec<Value> {
    doc["components"].as_array().unwrap()
}

fn cdx_prop<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))?["value"]
        .as_str()
}

fn cdx_component<'a>(doc: &'a Value, name: &str) -> &'a Value {
    let found: Vec<&Value> = cdx_components(doc)
        .iter()
        .filter(|c| c["name"].as_str() == Some(name))
        .collect();
    assert_eq!(found.len(), 1, "exactly one component named {name}, got {}", found.len());
    found[0]
}

fn cdx_doc_prop(doc: &Value, name: &str) -> Option<Value> {
    let raw = doc["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))?["value"]
        .as_str()?;
    serde_json::from_str(raw).ok()
}

fn is_module(purl: &str) -> bool {
    purl.starts_with("pkg:golang/") || purl.starts_with("pkg:cargo/")
}

/// Every string anywhere in the document that is a Go or cargo PURL. Format-
/// agnostic on purpose: the three formats put the identifier in different
/// places, and a reader per format would be three chances to look in the
/// wrong one and report an empty set as agreement.
fn module_purls(doc: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) if is_module(s) => {
                out.insert(s.clone());
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(doc, &mut out);
    out
}

/// `(from PURL, to PURL)` for every CycloneDX dependency edge into a module.
fn cdx_module_edges(doc: &Value) -> BTreeSet<(String, String)> {
    let by_ref: std::collections::HashMap<&str, &str> = cdx_components(doc)
        .iter()
        .filter_map(|c| Some((c["bom-ref"].as_str()?, c["purl"].as_str()?)))
        .collect();
    let mut out = BTreeSet::new();
    for d in doc["dependencies"].as_array().unwrap() {
        let Some(from) = d["ref"].as_str().and_then(|r| by_ref.get(r)) else { continue };
        for to in d["dependsOn"].as_array().into_iter().flatten() {
            if let Some(to) = to.as_str().and_then(|r| by_ref.get(r)) {
                if is_module(to) {
                    out.insert((from.to_string(), to.to_string()));
                }
            }
        }
    }
    out
}

fn spdx23_module_edges(doc: &Value) -> BTreeSet<(String, String)> {
    let purl_of = |p: &Value| -> Option<String> {
        p["externalRefs"].as_array()?.iter().find_map(|r| {
            (r["referenceType"].as_str() == Some("purl"))
                .then(|| r["referenceLocator"].as_str().map(str::to_string))
                .flatten()
        })
    };
    let by_id: std::collections::HashMap<&str, String> = doc["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| Some((p["SPDXID"].as_str()?, purl_of(p)?)))
        .collect();
    let mut out = BTreeSet::new();
    for r in doc["relationships"].as_array().unwrap() {
        if r["relationshipType"].as_str() != Some("DEPENDS_ON") {
            continue;
        }
        let (Some(from), Some(to)) = (
            r["spdxElementId"].as_str().and_then(|i| by_id.get(i)),
            r["relatedSpdxElement"].as_str().and_then(|i| by_id.get(i)),
        ) else {
            continue;
        };
        if is_module(to) {
            out.insert((from.clone(), to.clone()));
        }
    }
    out
}

fn spdx3_module_edges(doc: &Value) -> BTreeSet<(String, String)> {
    let graph = doc["@graph"].as_array().unwrap();
    let by_id: std::collections::HashMap<&str, &str> = graph
        .iter()
        .filter_map(|e| Some((e["spdxId"].as_str()?, e["software_packageUrl"].as_str()?)))
        .collect();
    let mut out = BTreeSet::new();
    for e in graph {
        if e["relationshipType"].as_str() != Some("dependsOn") {
            continue;
        }
        let Some(from) = e["from"].as_str().and_then(|i| by_id.get(i)) else { continue };
        for to in e["to"].as_array().into_iter().flatten() {
            if let Some(to) = to.as_str().and_then(|i| by_id.get(i)) {
                if is_module(to) {
                    out.insert((from.to_string(), to.to_string()));
                }
            }
        }
    }
    out
}

fn expected_modules() -> BTreeSet<String> {
    [
        "pkg:golang/example.test/waybill-fixture-goa@v1.0.0",
        "pkg:golang/example.test/waybill-fixture-gob@v0.2.0",
        "pkg:golang/stdlib@v1.22.1",
        "pkg:cargo/waybill-fixture-cratea@0.5.0?source=crates.io",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

// ---- User Story 1 -------------------------------------------------------

#[test]
fn us1_modules_compiled_into_present_outputs_are_the_members_dependencies() {
    let store = populated_store();
    let run = scan(store.path(), &[]);
    assert!(run.output.status.success(), "{}", run.stderr());
    let cdx = run.cdx();

    assert_eq!(module_purls(&cdx), expected_modules(), "CycloneDX");
    assert_eq!(module_purls(&run.spdx23()), expected_modules(), "SPDX 2.3");
    assert_eq!(module_purls(&run.spdx3()), expected_modules(), "SPDX 3");

    let app = "pkg:generic/waybill-fixture-app@1.2.3".to_string();
    let tool = "pkg:generic/waybill-fixture-tool@3.0.0".to_string();
    let expected_edges: BTreeSet<(String, String)> = [
        (app.clone(), "pkg:golang/example.test/waybill-fixture-goa@v1.0.0".to_string()),
        (app.clone(), "pkg:golang/example.test/waybill-fixture-gob@v0.2.0".to_string()),
        (app, "pkg:golang/stdlib@v1.22.1".to_string()),
        (tool, "pkg:cargo/waybill-fixture-cratea@0.5.0?source=crates.io".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(cdx_module_edges(&cdx), expected_edges, "CycloneDX edges");
    assert_eq!(spdx23_module_edges(&run.spdx23()), expected_edges, "SPDX 2.3 edges");
    assert_eq!(spdx3_module_edges(&run.spdx3()), expected_edges, "SPDX 3 edges");
}

/// FR-010 — what a build tool embeds does not ship.
#[test]
fn us1_a_build_tools_modules_are_not_presented_as_shipped() {
    let store = populated_store();
    let cdx = scan(store.path(), &[]).cdx();
    assert_eq!(
        cdx_component(&cdx, "waybill-fixture-cratea")["scope"].as_str(),
        Some("excluded"),
        "recovered from a `build-tooling` member only"
    );
    assert_eq!(
        cdx_component(&cdx, "example.test/waybill-fixture-goa").get("scope").and_then(Value::as_str),
        None,
        "recovered from the root, which is the artifact"
    );
    assert_eq!(
        cdx_component(&cdx, "waybill-fixture-app").get("scope").and_then(Value::as_str),
        None,
        "the root itself is never marked absent"
    );
    // The root is a component at all because milestone 1377 (#1398) emits
    // the evaluated package whatever its role; C182 still says, truthfully,
    // that nothing references it.
    assert_eq!(
        cdx_prop(cdx_component(&cdx, "waybill-fixture-app"), "waybill:closure-role"),
        Some("unreferenced")
    );
}

// ---- User Story 2 -------------------------------------------------------

#[test]
fn us2_an_empty_store_recovers_nothing_succeeds_and_says_so() {
    let store = tempfile::tempdir().unwrap();
    let run = scan(store.path(), &[]);
    assert!(run.output.status.success(), "an absent output is not a failure: {}", run.stderr());
    let cdx = run.cdx();

    assert!(module_purls(&cdx).is_empty());
    assert!(module_purls(&run.spdx23()).is_empty());
    assert!(module_purls(&run.spdx3()).is_empty());

    let record = cdx_doc_prop(&cdx, C213).expect("C213 is present even when nothing was read");
    assert_eq!(record["members"], 3);
    assert_eq!(record["read"], 0);
    assert_eq!(record["absent"], 3);
    assert_eq!(record["yielded"], 0);
    assert_eq!(record["root"], "absent");
    assert_eq!(record["modules"]["golang"], 0);
    assert_eq!(record["modules"]["cargo"], 0);
    for name in ["waybill-fixture-app", "waybill-fixture-lib", "waybill-fixture-tool"] {
        assert_eq!(cdx_prop(cdx_component(&cdx, name), C214), Some("absent"), "{name}");
    }
    assert!(
        run.stderr().contains("could not be read (absent)"),
        "FR-015a: the operator is told at scan time.\n{}",
        run.stderr()
    );
}

/// SC-009 — the present-store document and the empty-store document differ
/// in recovered modules, and the read record accounts for it.
#[test]
fn us2_the_read_record_explains_the_difference_between_two_machines() {
    let full = populated_store();
    let empty = tempfile::tempdir().unwrap();
    let with = scan(full.path(), &[]).cdx();
    let without = scan(empty.path(), &[]).cdx();

    let a = cdx_doc_prop(&with, C213).unwrap();
    assert_eq!(a["read"], 2);
    assert_eq!(a["absent"], 1);
    assert_eq!(a["yielded"], 2);
    assert_eq!(a["root"], "read");
    assert_eq!(a["modules"]["golang"], 3);
    assert_eq!(a["modules"]["cargo"], 1);
    assert_eq!(cdx_prop(cdx_component(&with, "waybill-fixture-app"), C214), Some("read"));
    assert_eq!(cdx_prop(cdx_component(&with, "waybill-fixture-tool"), C214), Some("read"));
    assert_eq!(cdx_prop(cdx_component(&with, "waybill-fixture-lib"), C214), Some("absent"));

    // The members are the same set on both machines; only what was read
    // differs, and only modules were added.
    let names = |d: &Value| -> BTreeSet<String> {
        cdx_components(d)
            .iter()
            .filter(|c| !c["purl"].as_str().is_some_and(is_module))
            .filter_map(|c| c["name"].as_str().map(str::to_string))
            .collect()
    };
    assert_eq!(names(&with), names(&without));
    // FR-021 — the subject is not this feature's to change.
    assert_eq!(with["metadata"]["component"], without["metadata"]["component"]);
}

#[test]
fn us2_the_read_record_is_the_same_in_all_three_formats() {
    let store = populated_store();
    let run = scan(store.path(), &[]);
    let expected = cdx_doc_prop(&run.cdx(), C213).unwrap();
    // The record's own bytes, wherever each format put them.
    fn find(v: &Value, out: &mut Vec<Value>) {
        match v {
            Value::String(s) if s.contains("waybill-nix-closure-contents/v1") => {
                // SPDX wraps the value in an annotation envelope.
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    let inner = match parsed.get("value") {
                        Some(Value::String(inner)) => serde_json::from_str(inner).ok(),
                        Some(other) => Some(other.clone()),
                        None => Some(parsed.clone()),
                    };
                    out.extend(inner);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| find(x, out)),
            Value::Object(o) => o.values().for_each(|x| find(x, out)),
            _ => {}
        }
    }
    for (label, doc) in [("SPDX 2.3", run.spdx23()), ("SPDX 3", run.spdx3())] {
        let mut found = Vec::new();
        find(&doc, &mut found);
        assert_eq!(found, vec![expected.clone()], "{label}");
    }
}

/// A member with several outputs, one of them present, is `partial` — and
/// that is not a failure, with or without the strict flag, when the member
/// is not the root.
#[test]
fn us2_a_member_with_only_some_outputs_present_is_partial() {
    let store = populated_store();
    std::fs::create_dir_all(store.path().join(LIB)).unwrap();
    let run = scan(store.path(), &["--nix-closure-require-root-contents"]);
    assert!(run.output.status.success(), "{}", run.stderr());
    let cdx = run.cdx();
    assert_eq!(
        cdx_prop(cdx_component(&cdx, "waybill-fixture-lib"), C214),
        Some("partial"),
        "`out` is present, `doc` is not"
    );
    let record = cdx_doc_prop(&cdx, C213).unwrap();
    assert_eq!(record["partial"], 1);
    assert_eq!(record["absent"], 0);
}

#[test]
fn us2_strict_fails_on_an_unread_root_and_writes_nothing() {
    let store = tempfile::tempdir().unwrap();
    let run = scan(store.path(), &["--nix-closure-require-root-contents"]);
    assert!(!run.output.status.success(), "must fail");
    assert!(
        run.stderr().contains("--nix-closure-require-root-contents")
            && run.stderr().contains("waybill-fixture-app 1.2.3")
            && run.stderr().contains("absent"),
        "names the flag, the root and the reason.\n{}",
        run.stderr()
    );
    for p in [&run.cdx_path, &run.spdx23_path, &run.spdx3_path] {
        assert!(!p.exists(), "no document may be written: {}", p.display());
    }
}

/// The control for the test above: the flag fails on an unread root, not on
/// its own presence.
#[test]
fn us2_strict_passes_when_the_root_was_read() {
    let store = populated_store();
    let run = scan(store.path(), &["--nix-closure-require-root-contents"]);
    assert!(run.output.status.success(), "{}", run.stderr());
    assert!(run.cdx_path.exists());
}

#[test]
fn us2_strict_passes_on_a_root_that_was_read_and_embeds_nothing() {
    let store = tempfile::tempdir().unwrap();
    let share = store.path().join(APP).join("share");
    std::fs::create_dir_all(&share).unwrap();
    std::fs::write(share.join("README"), "not a binary").unwrap();
    let run = scan(store.path(), &["--nix-closure-require-root-contents"]);
    assert!(run.output.status.success(), "{}", run.stderr());
    let cdx = run.cdx();
    assert_eq!(cdx_prop(cdx_component(&cdx, "waybill-fixture-app"), C214), Some("read"));
    assert!(module_purls(&cdx).is_empty());
}

#[test]
fn us2_the_strict_flag_requires_the_closure_flag() {
    let out = Command::new(binary_path())
        .args(["sbom", "scan", "--path", ".", "--nix-closure-require-root-contents"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--nix-closure"));
}

// ---- User Story 3 -------------------------------------------------------

/// With the registry not consulted, a binary's self-description is recorded
/// and the member keeps the identifier the closure gave it.
#[test]
fn us3_an_unconfirmed_identity_is_recorded_and_not_promoted() {
    let store = populated_store();
    let cdx = scan(store.path(), &[]).cdx();

    let app = cdx_component(&cdx, "waybill-fixture-app");
    assert_eq!(app["purl"].as_str(), Some("pkg:generic/waybill-fixture-app@1.2.3"));
    assert_eq!(app["version"].as_str(), Some("1.2.3"), "the closure's version");
    let id: Value = serde_json::from_str(cdx_prop(app, C215).expect("C215")).unwrap();
    assert_eq!(id["status"], "unchecked");
    assert_eq!(id["reason"], "disabled");
    assert_eq!(id["candidate"], "pkg:golang/example.test/waybill-fixture-app@v1.2.3");

    let tool = cdx_component(&cdx, "waybill-fixture-tool");
    let id: Value = serde_json::from_str(cdx_prop(tool, C215).expect("C215")).unwrap();
    assert_eq!(id["candidate"], "pkg:cargo/waybill-fixture-tool@3.0.0");

    // FR-011 — the program is one component. Neither the Go main module nor
    // the cargo root crate appears beside it.
    assert!(
        !module_purls(&cdx).iter().any(|p| p.contains("waybill-fixture-app")
            || p.contains("waybill-fixture-tool")),
        "{:?}",
        module_purls(&cdx)
    );
    assert!(cdx_prop(cdx_component(&cdx, "waybill-fixture-lib"), C215).is_none());
}

// ---- User Story 4 -------------------------------------------------------

#[test]
fn us4_go_and_rust_contents_are_both_flat_top_level_dependencies() {
    let store = populated_store();
    let cdx = scan(store.path(), &[]).cdx();
    let modules: Vec<&Value> = cdx_components(&cdx)
        .iter()
        .filter(|c| c["purl"].as_str().is_some_and(is_module))
        .collect();
    assert_eq!(modules.len(), 4, "CONTROL: all four are TOP-LEVEL components");
    for c in cdx_components(&cdx) {
        assert!(
            c.get("components").is_none(),
            "nothing is nested under {}",
            c["name"]
        );
    }
    for m in modules {
        assert_eq!(
            m["bom-ref"].as_str(),
            m["purl"].as_str(),
            "a plain bom-ref, not a composite naming a parent binary"
        );
    }
}
