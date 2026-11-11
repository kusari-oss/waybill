//! #1429 — the cargo activation set is keyed by package identity.
//!
//! `cargo metadata` reports each resolved dependency twice over:
//! `resolve.nodes[].deps[].name` is the *library* name (hyphens become
//! underscores, and a `package = "…"` rename appears under the rename),
//! while `deps[].pkg` is the package id. The classifier compares the
//! activation set against Cargo.lock `(name, version)` pairs, so the set
//! has to be built from the package id. Built from the library name, a
//! default-enabled optional dependency called `waybill-fixture-hy` was
//! emitted `scope: excluded` with `waybill:optional-derivation =
//! cargo-optional-true`.
//!
//! Every dependency resolves from a local directory source standing in
//! for crates.io, so neither `cargo fetch` nor waybill's own
//! `cargo metadata --offline` needs the network. The approach is the
//! one #1417 (PR #1424) takes in `cargo_optional_feature_resolve.rs`,
//! copied rather than shared because that helper derives crate names
//! from manifest keys and so cannot express a rename or a second
//! version of one crate, which are two of the cases here.

use std::path::Path;
use std::process::Command;

fn waybill_bin() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn require_cargo() -> bool {
    Command::new("cargo").arg("--version").output().is_ok()
}

/// One stand-in crate in the local directory source.
struct VendoredCrate {
    name: &'static str,
    version: &'static str,
    /// Body of the crate's own `[dependencies]` table, if any.
    dependencies: &'static str,
}

const fn leaf(name: &'static str, version: &'static str) -> VendoredCrate {
    VendoredCrate {
        name,
        version,
        dependencies: "",
    }
}

/// Write `crates` into a directory source in `parent` and point
/// crates.io at it through `parent/.cargo/config.toml`.
///
/// `parent` is the directory above the scanned workspace: cargo finds
/// the config by walking up from its working directory, both for
/// `cargo fetch` here and for waybill's `cargo metadata` subprocess,
/// while the scan sees only the workspace. Cargo.lock still records
/// `source = "registry+https://github.com/rust-lang/crates.io-index"`,
/// so waybill reads these entries as it reads real ones.
fn vendor_registry(parent: &Path, crates: &[VendoredCrate]) {
    let vendor = parent.join("waybill-fixture-vendor");
    for c in crates {
        // `<name>-<version>` lets two versions of one crate coexist,
        // which is the layout `cargo vendor` itself uses for them.
        let dir = vendor.join(format!("{}-{}", c.name, c.version));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let mut manifest = format!(
            "[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"2021\"\n",
            c.name, c.version
        );
        if !c.dependencies.is_empty() {
            manifest.push_str("\n[dependencies]\n");
            manifest.push_str(c.dependencies);
        }
        std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
        // Required in a directory source; an empty `files` map and a
        // null `package` checksum verify nothing, which suffices here.
        std::fs::write(
            dir.join(".cargo-checksum.json"),
            r#"{"files":{},"package":null}"#,
        )
        .unwrap();
    }
    std::fs::create_dir_all(parent.join(".cargo")).unwrap();
    std::fs::write(
        parent.join(".cargo/config.toml"),
        format!(
            "[source.crates-io]\nreplace-with = \"waybill-fixture-vendor\"\n\n\
             [source.waybill-fixture-vendor]\ndirectory = '{}'\n",
            vendor.display()
        ),
    )
    .unwrap();
}

/// Build a single-package workspace at `ws_root` against `crates` and
/// run `cargo fetch` to produce its Cargo.lock.
fn build_workspace(ws_root: &Path, cargo_toml: &str, crates: &[VendoredCrate]) {
    vendor_registry(ws_root.parent().unwrap(), crates);
    std::fs::create_dir_all(ws_root.join("src")).unwrap();
    std::fs::write(ws_root.join("Cargo.toml"), cargo_toml).unwrap();
    std::fs::write(ws_root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let out = Command::new("cargo")
        .arg("fetch")
        .current_dir(ws_root)
        .output()
        .expect("spawn cargo fetch");
    assert!(
        out.status.success(),
        "cargo fetch failed: stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Scan `ws_root` online, so the `cargo metadata` path runs, and
/// return the CycloneDX document and stderr.
fn scan(ws_root: &Path) -> (serde_json::Value, String) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let cmd_out = Command::new(waybill_bin())
        .args([
            "sbom",
            "scan",
            "--path",
            ws_root.to_str().unwrap(),
            "--format",
            "cyclonedx-json",
            "--output",
            out.to_str().unwrap(),
            "--no-deep-hash",
        ])
        .env_remove("WAYBILL_OFFLINE")
        .output()
        .expect("spawn waybill");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    assert!(cmd_out.status.success(), "scan failed. stderr:\n{stderr}");
    // Runtime is also what the no-evidence fallback yields (#1272), and
    // a single-package workspace recovers activation from its manifest
    // when the subprocess fails (#1302). Either would make the runtime
    // assertions below pass without exercising the `cargo metadata`
    // set this file is about.
    assert!(
        !stderr.contains("cargo metadata failed"),
        "these tests are about the resolved-activation path; a fallback makes \
         them vacuous. stderr:\n{stderr}"
    );
    let cdx = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    (cdx, stderr)
}

fn component<'a>(
    cdx: &'a serde_json::Value,
    name: &str,
    version: &str,
) -> &'a serde_json::Value {
    cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name && c["version"] == version)
        .unwrap_or_else(|| panic!("{name}@{version} present. cdx: {cdx:#}"))
}

fn property<'a>(comp: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    comp.get("properties")?
        .as_array()?
        .iter()
        .find(|p| p["name"] == key)?
        .get("value")?
        .as_str()
}

fn assert_runtime(comp: &serde_json::Value, what: &str) {
    let scope = comp.get("scope").and_then(|s| s.as_str());
    assert!(
        scope.is_none() || scope == Some("runtime"),
        "{what} is enabled by `default` and ships, so it must be runtime \
         (or absent, which defaults to runtime); got {scope:?}. component: {comp:#}"
    );
    assert_eq!(
        property(comp, "waybill:optional-derivation"),
        None,
        "{what} must not carry waybill:optional-derivation. component: {comp:#}"
    );
}

fn assert_excluded(comp: &serde_json::Value, what: &str) {
    assert_eq!(
        comp.get("scope").and_then(|s| s.as_str()),
        Some("excluded"),
        "{what} is optional and no feature enables it, so it must be excluded. \
         component: {comp:#}"
    );
    assert_eq!(
        property(comp, "waybill:optional-derivation"),
        Some("cargo-optional-true"),
        "component: {comp:#}"
    );
}

#[test]
fn hyphenated_default_enabled_optional_dep_is_runtime() {
    if !require_cargo() {
        eprintln!("skipping: cargo binary not on PATH");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path().join("ws");
    build_workspace(
        &ws,
        r#"[package]
name = "waybill-fixture-1429-hyphen"
version = "0.1.0"
edition = "2021"

[dependencies]
waybill-fixture-hy = { version = "1", optional = true }

[features]
default = ["waybill-fixture-hy"]
"#,
        &[leaf("waybill-fixture-hy", "1.0.0")],
    );
    let (cdx, _) = scan(&ws);
    assert_runtime(
        component(&cdx, "waybill-fixture-hy", "1.0.0"),
        "waybill-fixture-hy",
    );
}

#[test]
fn renamed_default_enabled_optional_dep_is_runtime() {
    if !require_cargo() {
        eprintln!("skipping: cargo binary not on PATH");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path().join("ws");
    build_workspace(
        &ws,
        r#"[package]
name = "waybill-fixture-1429-rename"
version = "0.1.0"
edition = "2021"

[dependencies]
alias = { package = "waybill-fixture-real", version = "1", optional = true }

[features]
default = ["alias"]
"#,
        &[leaf("waybill-fixture-real", "1.0.0")],
    );
    let (cdx, _) = scan(&ws);
    assert_runtime(
        component(&cdx, "waybill-fixture-real", "1.0.0"),
        "waybill-fixture-real (renamed to `alias`)",
    );
}

/// The control: the same hyphenated shape with no feature enabling the
/// dependency must still be excluded. Without it, a fix that marked
/// every optional dependency activated would pass the two tests above.
#[test]
fn hyphenated_optional_dep_not_enabled_stays_excluded() {
    if !require_cargo() {
        eprintln!("skipping: cargo binary not on PATH");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path().join("ws");
    build_workspace(
        &ws,
        r#"[package]
name = "waybill-fixture-1429-control"
version = "0.1.0"
edition = "2021"

[dependencies]
waybill-fixture-hy = { version = "1", optional = true }
waybill-fixture-off = { version = "1", optional = true }

[features]
default = ["waybill-fixture-hy"]
enable-off = ["dep:waybill-fixture-off"]
"#,
        &[
            leaf("waybill-fixture-hy", "1.0.0"),
            leaf("waybill-fixture-off", "1.0.0"),
        ],
    );
    let (cdx, _) = scan(&ws);
    assert_excluded(
        component(&cdx, "waybill-fixture-off", "1.0.0"),
        "waybill-fixture-off",
    );
    // In the same document the enabled one is runtime, so the result
    // above is a distinction the classifier drew, not a blanket answer.
    assert_runtime(
        component(&cdx, "waybill-fixture-hy", "1.0.0"),
        "waybill-fixture-hy",
    );
}

/// Two versions of one crate: `1.0.0` arrives transitively through a
/// non-optional dependency and ships, `2.0.0` is a direct optional
/// dependency that nothing enables. Keyed by name alone, either both
/// are activated or neither is; only `(name, version)` tells them apart.
#[test]
fn two_versions_of_one_crate_are_classified_separately() {
    if !require_cargo() {
        eprintln!("skipping: cargo binary not on PATH");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path().join("ws");
    build_workspace(
        &ws,
        r#"[package]
name = "waybill-fixture-1429-dup"
version = "0.1.0"
edition = "2021"

[dependencies]
waybill-fixture-mid = "1"
waybill-fixture-dup = { version = "2", optional = true }

[features]
enable-dup = ["dep:waybill-fixture-dup"]
"#,
        &[
            VendoredCrate {
                name: "waybill-fixture-mid",
                version: "1.0.0",
                dependencies: "waybill-fixture-dup = \"1\"\n",
            },
            leaf("waybill-fixture-dup", "1.0.0"),
            leaf("waybill-fixture-dup", "2.0.0"),
        ],
    );
    let (cdx, _) = scan(&ws);
    assert_runtime(
        component(&cdx, "waybill-fixture-dup", "1.0.0"),
        "waybill-fixture-dup@1.0.0 (transitive through waybill-fixture-mid)",
    );
    assert_excluded(
        component(&cdx, "waybill-fixture-dup", "2.0.0"),
        "waybill-fixture-dup@2.0.0",
    );
}
