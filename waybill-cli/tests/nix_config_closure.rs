//! Milestone 1066 (#1052) — closure SBOMs for Nix system-configuration flakes.
//!
//! Most tests drive the closure tier through a stub `nix` that answers each
//! argv the way a scenario needs and logs what it was asked, so they run in CI
//! without Nix and can assert exactly which attribute path was evaluated. The
//! `real_nix_*` tests use fixture flakes with no inputs and skip when `nix` is
//! not on PATH.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const CLOSURE_JSON: &str = r#"{"derivations":{"aaa-lib-1.0.drv":{"name":"lib-1.0","env":{"pname":"lib","version":"1.0"},"outputs":{"out":{"path":"hash1-lib-1.0"}}},"bbb-app-2.0.drv":{"name":"app-2.0","env":{"pname":"app","version":"2.0","buildInputs":"/nix/store/hash1-lib-1.0"},"outputs":{"out":{"path":"hash2-app-2.0"}}}},"version":3}"#;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_config_closure")
        .join(name)
}

/// What the stub `nix` answers. `None` listings fail with nix's measured
/// "does not provide attribute" wording.
#[derive(Clone)]
struct Stub {
    host_system: &'static str,
    /// `<root>#packages`: the flake's platforms, as a JSON array.
    packages: Option<&'static str>,
    /// `<root>#packages.<host>`: names under the host's platform.
    packages_host: Option<&'static str>,
    darwin: Option<&'static str>,
    nixos: Option<&'static str>,
    /// Flakeref suffixes (after `#`) whose `derivation show` succeeds.
    closures: Vec<&'static str>,
}

impl Stub {
    fn script(&self, log: &Path) -> String {
        let fail = |what: &str| {
            format!(
                "echo \"error: flake 'stub' does not provide attribute '{what}'\" >&2; exit 1"
            )
        };
        let listing = |suffix: &str, v: Option<&str>| match v {
            Some(json) => format!("  *\"#{suffix}\") echo '{json}' ;;\n"),
            None => format!("  *\"#{suffix}\") {} ;;\n", fail(suffix)),
        };
        let mut s = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\ncase \"$*\" in\n  *builtins.currentSystem*) printf '%s' '{}' ;;\n",
            log.display(),
            self.host_system
        );
        for c in &self.closures {
            s.push_str(&format!("  \"derivation show\"*\"#{c}\") echo '{CLOSURE_JSON}' ;;\n"));
        }
        s.push_str(&format!("  \"derivation show\"*) {} ;;\n", fail("requested path")));
        s.push_str(&listing(&format!("packages.{}", self.host_system), self.packages_host));
        s.push_str(&listing("packages", self.packages));
        s.push_str(&listing("darwinConfigurations", self.darwin));
        s.push_str(&listing("nixosConfigurations", self.nixos));
        // Anything else (other tiers' nix calls) fails cleanly (analysis U2).
        s.push_str("  *) echo 'stub nix: unhandled invocation' >&2; exit 1 ;;\nesac\n");
        s
    }
}

struct Scan {
    stderr: String,
    argv: Vec<String>,
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

impl Scan {
    fn closure_queries(&self) -> Vec<&str> {
        self.argv
            .iter()
            .filter(|a| a.starts_with("derivation show"))
            .map(|a| a.rsplit('#').next().unwrap_or(""))
            .collect()
    }
    fn listed(&self, suffix: &str) -> bool {
        self.argv
            .iter()
            .any(|a| a.starts_with("eval") && a.ends_with(&format!("#{suffix}")))
    }
    fn doc_value(&self, field: &str) -> [Option<String>; 3] {
        let env = |raw: &str| -> Option<String> {
            let v: Value = serde_json::from_str(raw).ok()?;
            (v["field"].as_str() == Some(field)).then(|| v["value"].as_str().map(str::to_string))?
        };
        let cdx = self.cdx["metadata"]["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"].as_str() == Some(field))
            .and_then(|p| p["value"].as_str().map(str::to_string));
        let spdx23 = self.spdx23["annotations"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|a| env(a["comment"].as_str()?));
        let spdx3 = self.spdx3["@graph"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|e| env(e["statement"].as_str()?));
        [cdx, spdx23, spdx3]
    }
    /// The C184 `attribute`, after checking all three formats agree.
    fn closure_attribute(&self) -> Option<String> {
        let [a, b, c] = self.doc_value("waybill:nix-closure");
        assert_eq!(a, b, "C184 CycloneDX vs SPDX 2.3");
        assert_eq!(a, c, "C184 CycloneDX vs SPDX 3");
        let v: Value = serde_json::from_str(&a?).ok()?;
        v["attribute"].as_str().map(str::to_string)
    }
}

fn run(root: &Path, stub: Option<&Stub>, extra: &[&str]) -> Scan {
    let mut args = vec!["--nix-closure"];
    args.extend_from_slice(extra);
    run_args(root, stub, &args)
}

fn run_args(root: &Path, stub: Option<&Stub>, extra: &[&str]) -> Scan {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("argv.log");
    std::fs::write(&log, "").unwrap();
    let path = match stub {
        Some(s) => {
            let p = dir.path().join("nix");
            std::fs::write(&p, s.script(&log)).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            format!("{}:/usr/bin:/bin", dir.path().display())
        }
        None => std::env::var("PATH").unwrap_or_default(),
    };
    let out = tempfile::tempdir().unwrap();
    let f = |n: &str| out.path().join(n);
    let o = Command::new(binary_path())
        .args(["sbom", "scan", "--no-deep-hash", "--path"])
        .arg(root)
        .args(extra)
        .args(["--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", f("o.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", f("o.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", f("o.spdx3.json").display()))
        .env("PATH", path)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "scan failed: {stderr}");
    let read = |n: &str| serde_json::from_slice(&std::fs::read(f(n)).unwrap()).unwrap();
    Scan {
        argv: std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect(),
        stderr,
        cdx: read("o.cdx.json"),
        spdx23: read("o.spdx.json"),
        spdx3: read("o.spdx3.json"),
    }
}

/// A scan root that looks like a flake to the walker. The stub answers for it.
fn flake_root() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("flake.nix"), "{ outputs = { self }: { }; }\n").unwrap();
    d
}

fn nix_on_path() -> bool {
    Command::new("nix").arg("--version").output().is_ok()
}

fn one_darwin() -> Stub {
    Stub {
        host_system: "aarch64-darwin",
        packages: None,
        packages_host: None,
        darwin: Some(r#"["laptop"]"#),
        nixos: None,
        closures: vec!["darwinConfigurations.laptop.system"],
    }
}

// ---------------------------------------------------------------
// US1 — one configuration, no flags
// ---------------------------------------------------------------

#[test]
fn t007_a_sole_configuration_is_selected() {
    let root = flake_root();
    let s = run(root.path(), Some(&one_darwin()), &[]);
    assert_eq!(s.closure_queries(), vec!["darwinConfigurations.laptop.system"], "{:?}", s.argv);
    assert_eq!(
        s.closure_attribute().as_deref(),
        Some("darwinConfigurations.laptop.system")
    );
}

#[test]
fn t008_real_nix_sole_darwin_configuration() {
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    let s = run(&fixture("one_darwin"), None, &[]);
    assert_eq!(
        s.closure_attribute().as_deref(),
        Some("darwinConfigurations.laptop.system"),
        "stderr: {}",
        s.stderr
    );
}

// ---------------------------------------------------------------
// US2 — several configurations; the operator names one
// ---------------------------------------------------------------

fn two_configs() -> Stub {
    Stub {
        darwin: Some(r#"["laptop"]"#),
        nixos: Some(r#"["web01"]"#),
        closures: vec![
            "darwinConfigurations.laptop.system",
            "nixosConfigurations.web01.config.system.build.toplevel",
        ],
        ..one_darwin()
    }
}

#[test]
fn t011a_several_configurations_degrade_and_name_them() {
    let root = flake_root();
    let s = run(root.path(), Some(&two_configs()), &[]);
    assert!(s.closure_queries().is_empty(), "nothing may be evaluated: {:?}", s.argv);
    assert!(s.stderr.contains("several-system-configurations"), "{}", s.stderr);
    let (d, n) = (
        s.stderr.find("darwinConfigurations.laptop").expect("darwin name"),
        s.stderr.find("nixosConfigurations.web01").expect("nixos name"),
    );
    assert!(d < n, "names must be listed sorted");
    assert!(s.stderr.contains("--nix-closure-attr"), "how to choose one");
    assert_eq!(s.closure_attribute(), None, "no closure recorded");
}

#[test]
fn t011b_a_full_path_evaluates_exactly_that_output() {
    let root = flake_root();
    let p = "nixosConfigurations.web01.config.system.build.toplevel";
    let s = run(root.path(), Some(&two_configs()), &["--nix-closure-attr", p]);
    assert_eq!(s.closure_queries(), vec![p], "{:?}", s.argv);
    assert!(
        !s.listed(&format!("packages.{}", "aarch64-darwin")),
        "a full path needs no packages listing: {:?}",
        s.argv
    );
    assert_eq!(s.closure_attribute().as_deref(), Some(p));
}

#[test]
fn t011c_an_absent_full_path_is_no_evaluable_attribute() {
    let root = flake_root();
    let s = run(
        root.path(),
        Some(&two_configs()),
        &["--nix-closure-attr", "darwinConfigurations.nope.system"],
    );
    assert_eq!(
        s.closure_queries(),
        vec!["darwinConfigurations.nope.system"],
        "the path itself must be queried, and its absence classified: {:?}",
        s.argv
    );
    assert!(s.stderr.contains("no-evaluable-attribute"), "{}", s.stderr);
    assert!(!s.stderr.contains("evaluation-failed"), "{}", s.stderr);
    assert_eq!(s.closure_attribute(), None);
}

#[test]
fn t011d_an_unsafe_full_path_never_reaches_nix() {
    let root = flake_root();
    let s = run(
        root.path(),
        Some(&two_configs()),
        &["--nix-closure-attr", "nixosConfigurations.web01#evil"],
    );
    assert!(s.closure_queries().is_empty(), "refused before nix: {:?}", s.argv);
    assert!(s.stderr.contains("no-evaluable-attribute"), "{}", s.stderr);
    assert!(s.stderr.contains("refusing unsafe attribute path"), "{}", s.stderr);
}

#[test]
fn t012_real_nix_two_configurations() {
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    let s = run(&fixture("two_configs"), None, &[]);
    assert!(s.stderr.contains("several-system-configurations"), "{}", s.stderr);
    assert_eq!(s.closure_attribute(), None);

    // FR-004: an x86_64-linux configuration evaluates on any host.
    let p = "nixosConfigurations.web01.config.system.build.toplevel";
    let s = run(&fixture("two_configs"), None, &["--nix-closure-attr", p]);
    assert_eq!(s.closure_attribute().as_deref(), Some(p), "stderr: {}", s.stderr);
}

// ---------------------------------------------------------------
// US3 — package flakes unchanged; host independence
// ---------------------------------------------------------------

fn package_and_config() -> Stub {
    Stub {
        packages: Some(r#"["aarch64-darwin","x86_64-linux"]"#),
        packages_host: Some(r#"["default","other"]"#),
        closures: vec!["packages.aarch64-darwin.default", "darwinConfigurations.laptop.system"],
        ..one_darwin()
    }
}

#[test]
fn t015_a_packages_output_wins_over_a_configuration() {
    let root = flake_root();
    let s = run(root.path(), Some(&package_and_config()), &[]);
    assert_eq!(s.closure_queries(), vec!["packages.aarch64-darwin.default"], "{:?}", s.argv);
    assert!(!s.listed("darwinConfigurations"), "configurations must not even be listed");
    assert_eq!(s.closure_attribute().as_deref(), Some("default"));
}

#[test]
fn t016_an_explicit_default_is_never_replaced_by_a_configuration() {
    let root = flake_root();
    let stub = Stub {
        packages_host: Some(r#"["other"]"#),
        ..package_and_config()
    };
    let s = run(root.path(), Some(&stub), &["--nix-closure-attr", "default"]);
    assert!(s.closure_queries().is_empty(), "{:?}", s.argv);
    assert!(!s.listed("darwinConfigurations"));
    assert!(s.stderr.contains("no-evaluable-attribute"), "{}", s.stderr);
}

#[test]
fn t017a_the_choice_does_not_depend_on_the_host() {
    let root = flake_root();
    let hosts = ["aarch64-darwin", "x86_64-linux"];
    let queries = |stub: &Stub| -> Vec<Vec<String>> {
        hosts
            .iter()
            .map(|h| {
                let mut st = stub.clone();
                st.host_system = h;
                let s = run(root.path(), Some(&st), &[]);
                s.closure_queries().iter().map(|q| q.to_string()).collect()
            })
            .collect()
    };
    // One configuration and two configurations: identical on both hosts.
    for stub in [one_darwin(), two_configs()] {
        let q = queries(&stub);
        assert_eq!(q[0], q[1], "the choice varied with the host");
    }
    // Linux-only packages plus a darwin configuration: the configuration is
    // never taken on either host (analysis I1).
    let linux_pkgs = Stub {
        packages: Some(r#"["x86_64-linux"]"#),
        packages_host: None,
        ..one_darwin()
    };
    for q in queries(&linux_pkgs) {
        assert!(
            !q.iter().any(|x| x.starts_with("darwinConfigurations")),
            "a flake with packages must never select a configuration: {q:?}"
        );
    }
}

#[test]
fn t017_real_nix_package_and_configuration() {
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    let s = run(&fixture("package_and_config"), None, &[]);
    assert_eq!(s.closure_attribute().as_deref(), Some("default"), "stderr: {}", s.stderr);
}

// ---------------------------------------------------------------
// FR-012 — C190 waybill:nix-closure-degraded (Constitution XII.3; #1115)
// ---------------------------------------------------------------

const C190: &str = "waybill:nix-closure-degraded";

fn c190(s: &Scan) -> Option<String> {
    let [a, b, c] = s.doc_value(C190);
    assert_eq!(a, b, "C190 CycloneDX vs SPDX 2.3");
    assert_eq!(a, c, "C190 CycloneDX vs SPDX 3");
    a
}

#[test]
fn t017b_a_degraded_closure_is_recorded_in_the_document() {
    let root = flake_root();
    // (a) several configurations
    let s = run(root.path(), Some(&two_configs()), &[]);
    assert_eq!(c190(&s).as_deref(), Some("several-system-configurations"));
    // (b) nothing evaluable
    let none = Stub { darwin: None, nixos: None, ..one_darwin() };
    let s = run(root.path(), Some(&none), &[]);
    assert_eq!(c190(&s).as_deref(), Some("no-evaluable-attribute"));
    // (c) refused before any nix process
    let s = run(root.path(), Some(&one_darwin()), &["--offline"]);
    assert_eq!(c190(&s).as_deref(), Some("offline-requested"));
}

#[test]
fn t017b_no_c190_when_a_closure_was_recorded_or_not_requested() {
    let root = flake_root();
    // (d) success
    let s = run(root.path(), Some(&one_darwin()), &[]);
    assert!(s.closure_attribute().is_some());
    assert_eq!(c190(&s), None);
    // (e) flag absent
    let s = run_args(root.path(), Some(&one_darwin()), &[]);
    assert_eq!(c190(&s), None);
}
