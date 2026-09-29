//! Milestone 1034 (#971 part A) — the opt-in `nix eval` resolution tier.
//!
//! These tests exercise the tier through the binary, which is the only place
//! the flag surface, the degradation decisions and the emitted annotations
//! meet. Unit tests cover the pieces; this covers whether they are wired
//! together.
//!
//! Several assert a *positive* signal alongside the obvious one. Scanning the
//! import-from-derivation fixture and finding nothing built, for instance,
//! proves nothing on its own — a tier that never ran also builds nothing, and
//! the two are indistinguishable from the exit code. Where that trap exists it
//! is called out at the assertion.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_eval")
        .join(name)
}

struct ScanResult {
    ok: bool,
    stderr: String,
    doc: Option<Value>,
}

impl ScanResult {
    /// Document-scope `waybill:nix-eval-*` properties, as a name→value map.
    fn tier_properties(&self) -> Vec<(String, String)> {
        self.doc
            .as_ref()
            .and_then(|d| d.get("metadata")?.get("properties")?.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|p| {
                let n = p.get("name")?.as_str()?.to_string();
                let v = p.get("value")?.as_str()?.to_string();
                n.starts_with("waybill:nix-eval").then_some((n, v))
            })
            .collect()
    }

    fn degraded_reason(&self) -> Option<String> {
        self.tier_properties()
            .into_iter()
            .find(|(n, _)| n == "waybill:nix-eval-degraded")
            .map(|(_, v)| v)
    }
}

/// Run a scan, optionally with `nix` removed from `PATH`.
fn scan(path: &Path, extra: &[&str], strip_nix: bool) -> ScanResult {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        // Deliberately NOT --offline: that flag refuses the tier outright,
        // because evaluation may fetch the pinned revision. A test harness
        // that passed it would exercise the refusal and nothing else.
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .args(extra);
    if strip_nix {
        // A PATH with no `nix` on it. Set rather than cleared so the binary
        // can still find whatever else it needs.
        cmd.env("PATH", "/usr/bin:/bin");
    }
    let out = cmd.output().unwrap();
    let doc = std::fs::read(&out_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    ScanResult {
        ok: out.status.success(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        doc,
    }
}

fn nix_on_path() -> bool {
    Command::new("nix").arg("--version").output().is_ok()
}

fn ifd_marker_in_store() -> bool {
    // The fixture's derivation name. Checking the store directly rather than
    // trusting an exit code: a build that happened leaves a path behind.
    std::fs::read_dir("/nix/store")
        .map(|d| {
            d.flatten()
                .any(|e| e.file_name().to_string_lossy().contains("waybill-ifd-marker"))
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------- US3, safety

#[test]
fn a_scan_builds_no_derivation() {
    // What this DOES prove: a `--nix-eval` scan of a project whose own flake
    // would build something on evaluation leaves nothing in the store.
    //
    // What it does NOT prove, and an earlier version of this test wrongly
    // implied: that import-from-derivation was *refused*. The tier as
    // implemented never evaluates the scanned repository's flake at all — it
    // reads the pinned revision out of `flake.lock` and evaluates
    // `github:NixOS/nixpkgs/<rev>` attributes (see `versions_expr`). The
    // fixture's IFD derivation is therefore never reached, and removing
    // `--option allow-import-from-derivation false` from the invocation does
    // not make this test fail. Verified by mutation.
    //
    // The refusal is still passed and still verified, because it becomes
    // load-bearing the moment the tier evaluates a project flake — which
    // research R8's attribute-path work and issues #1034 / #1040 all require.
    // Until then the property under test here is the weaker "a scan builds
    // nothing", and `a_nix_whose_config_omits_the_setting_is_refused_not_trusted`
    // is what actually exercises the safety gate.
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    assert!(
        !ifd_marker_in_store(),
        "the store already holds a waybill-ifd-marker; this test cannot \
         distinguish a refusal from a leftover — remove it and re-run"
    );

    let r = scan(&fixture("ifd"), &["--nix-eval"], false);

    assert!(r.ok, "the scan must succeed, not abort: {}", r.stderr);
    assert!(
        !ifd_marker_in_store(),
        "a scan built a derivation from the scanned tree"
    );

    // THE CONTROL. Everything above is also true of a tier that never ran.
    let props = r.tier_properties();
    assert!(
        props.iter().any(|(n, _)| n == "waybill:nix-eval-tier"),
        "no waybill:nix-eval-tier property — the tier did not run, so the \
         assertions above proved nothing. Properties seen: {props:?}"
    );
}

#[test]
fn a_nix_whose_config_omits_the_setting_is_refused_not_trusted() {
    // A stub `nix` that answers `config show` without the setting, which is
    // how a nix too old to support it behaves. waybill must decline to
    // evaluate rather than proceed unprotected: research R3 measured that an
    // unsupported --option is accepted, ignored, and exits 0.
    let dir = tempfile::tempdir().unwrap();
    let stub = dir.path().join("nix");
    std::fs::write(
        &stub,
        "#!/bin/sh\n\
         if [ \"$1\" = config ]; then echo 'allow-dirty = true'; exit 0; fi\n\
         echo 'stub nix: unexpected invocation' >&2; exit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let out = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture("ifd"))
        .arg("--no-deep-hash")
        .arg("--nix-eval")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "an unusable nix must degrade, not fail the scan: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: Value =
        serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap();
    let reason = doc["metadata"]["properties"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .find(|p| p["name"] == "waybill:nix-eval-degraded")
        .map(|p| p["value"].as_str().unwrap_or_default().to_string());
    assert_eq!(
        reason.as_deref(),
        Some("ifd-refusal-unverified"),
        "expected the refusal to be unverifiable against the stub; got {reason:?}"
    );
}

// ------------------------------------------------------------ US2, degradation

#[test]
fn an_absent_nix_degrades_and_the_scan_succeeds() {
    let r = scan(&fixture("ifd"), &["--nix-eval"], true);
    assert!(r.ok, "scan must succeed with nix absent: {}", r.stderr);
    assert_eq!(r.degraded_reason().as_deref(), Some("tool-absent"));
}

#[test]
fn the_flag_off_path_emits_no_tier_metadata_at_all() {
    // FR-002: without the flag nothing is started and nothing is recorded,
    // which is what makes the default path byte-identical.
    let r = scan(&fixture("ifd"), &[], false);
    assert!(r.ok, "{}", r.stderr);
    assert!(
        r.tier_properties().is_empty(),
        "flag-off scan carried tier metadata: {:?}",
        r.tier_properties()
    );
}

#[test]
fn degraded_output_differs_from_flag_off_only_by_tier_metadata() {
    // SC-003. Strip the two fields that vary per run, then compare.
    let strip = |mut d: Value| {
        d.as_object_mut().map(|o| o.remove("serialNumber"));
        d["metadata"].as_object_mut().map(|o| o.remove("timestamp"));
        if let Some(props) = d["metadata"]["properties"].as_array_mut() {
            props.retain(|p| {
                !p["name"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("waybill:nix-eval")
            });
        }
        d
    };
    let off = scan(&fixture("ifd"), &[], false);
    let degraded = scan(&fixture("ifd"), &["--nix-eval"], true);
    assert!(off.ok && degraded.ok);
    assert_eq!(
        strip(off.doc.unwrap()),
        strip(degraded.doc.unwrap()),
        "a degraded --nix-eval scan must match the flag-off scan apart from \
         its own metadata"
    );
}

// ------------------------------------------------------------- flag validation

#[test]
fn companion_flags_require_the_opt_in_and_are_argument_errors() {
    // Argument errors, NOT degradations: the operator mistyped an invocation,
    // which is a different thing from the environment failing to satisfy a
    // valid one.
    for args in [
        vec!["--nix-eval-system", "x86_64-linux"],
        vec!["--nix-eval-timeout-secs", "30"],
    ] {
        let out = Command::new(binary_path())
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(fixture("ifd"))
            .args(&args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?} should be rejected");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("--nix-eval"),
            "the error must name the missing flag: {stderr}"
        );
    }
}

#[test]
fn a_zero_budget_is_rejected_because_nothing_else_would_bound_it() {
    let out = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture("ifd"))
        .arg("--nix-eval")
        .arg("--nix-eval-timeout-secs")
        .arg("0")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("0 is not in"));
}

// --------------------------------------------- US4 / US5, per-component metadata

/// Per-component `waybill:nix-eval-*` values, as name→value pairs.
fn component_tier_props(doc: &Value) -> Vec<(String, String, String)> {
    doc["components"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|c| {
            let name = c["name"].as_str().unwrap_or_default().to_string();
            c["properties"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(move |p| {
                    let n = p["name"].as_str()?.to_string();
                    let v = p["value"].as_str()?.to_string();
                    n.starts_with("waybill:nix-eval")
                        .then(|| (name.clone(), n, v))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn every_haskell_component_carries_an_origin_when_the_tier_ran() {
    // C181's contract: absence must be unambiguous. If only the evaluated
    // components were marked, a bare component would be indistinguishable
    // from one in a scan where the tier never ran — two different claims.
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    let r = scan(&fixture("ifd"), &["--nix-eval"], false);
    assert!(r.ok, "{}", r.stderr);
    let doc = r.doc.expect("a document");

    let hackage: Vec<&Value> = doc["components"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|c| {
                    c["purl"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("pkg:hackage/")
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !hackage.is_empty(),
        "control: the fixture must produce hackage components, or this test \
         asserts nothing"
    );

    for c in &hackage {
        let origin = c["properties"]
            .as_array()
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p["name"] == "waybill:nix-eval-origin")
                    .and_then(|p| p["value"].as_str())
            });
        assert!(
            matches!(origin, Some("evaluated") | Some("file-parsed")),
            "component {} has origin {origin:?}; every hackage component must \
             carry one of the two closed values",
            c["name"]
        );
    }
}

#[test]
fn the_platform_is_recorded_both_as_a_scalar_and_inside_the_tier_record() {
    // C179 duplicates C178's `system` key deliberately: it is the fact that
    // changes what the document means, and a consumer should not have to
    // parse a nested JSON string out of a property value to find it. The two
    // must agree, or the duplication becomes a second source of truth.
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    let r = scan(&fixture("ifd"), &["--nix-eval"], false);
    let props = r.tier_properties();

    let scalar = props
        .iter()
        .find(|(n, _)| n == "waybill:nix-eval-system")
        .map(|(_, v)| v.clone())
        .expect("C179 must be present when the tier established a platform");
    let record: Value = serde_json::from_str(
        &props
            .iter()
            .find(|(n, _)| n == "waybill:nix-eval-tier")
            .map(|(_, v)| v.clone())
            .expect("C178"),
    )
    .expect("C178 carries a JSON object");

    assert_eq!(
        Some(scalar.as_str()),
        record["system"].as_str(),
        "C179 and C178.system disagree — the duplication has become a second \
         source of truth"
    );
}

#[test]
fn an_explicit_platform_is_honoured_rather_than_the_hosts() {
    // SC-005's mechanism. Results are platform-specific, so the document must
    // describe the platform asked for, not the one that ran the scan.
    if !nix_on_path() {
        eprintln!("skipping: no nix on PATH");
        return;
    }
    let r = scan(
        &fixture("ifd"),
        &["--nix-eval", "--nix-eval-system", "x86_64-linux"],
        false,
    );
    assert!(r.ok, "{}", r.stderr);
    let system = r
        .tier_properties()
        .into_iter()
        .find(|(n, _)| n == "waybill:nix-eval-system")
        .map(|(_, v)| v);
    assert_eq!(
        system.as_deref(),
        Some("x86_64-linux"),
        "the named platform must win over the host's"
    );
}

#[test]
fn the_flag_off_path_carries_no_per_component_tier_metadata() {
    let r = scan(&fixture("ifd"), &[], false);
    assert!(r.ok, "{}", r.stderr);
    let doc = r.doc.expect("a document");
    assert!(
        component_tier_props(&doc).is_empty(),
        "flag-off scan carried per-component tier metadata: {:?}",
        component_tier_props(&doc)
    );
}

#[test]
fn offline_refuses_the_tier_rather_than_reaching_the_network() {
    // `--offline` promises "disable all outbound network calls". Evaluation
    // resolves the pinned revision through `getFlake`, which fetches when the
    // nix store lacks it -- measured: `unpacking 'github:NixOS/nixpkgs/<rev>'
    // into the Git cache...`. Nix's own `--offline` does not prevent that; it
    // governs substituters, not flake inputs.
    //
    // So the two flags are mutually exclusive in effect, and `--offline` wins.
    // The alternative -- evaluating and hoping the store is warm -- keeps the
    // promise only by luck.
    let out = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture("ifd"))
        .arg("--offline")
        .arg("--no-deep-hash")
        .arg("--nix-eval")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg("cyclonedx-json=/dev/stdout")
        .output()
        .unwrap();
    assert!(out.status.success(), "the combination must not fail the scan");

    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let reason = doc["metadata"]["properties"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .find(|p| p["name"] == "waybill:nix-eval-degraded")
        .and_then(|p| p["value"].as_str())
        .map(str::to_string);
    assert_eq!(
        reason.as_deref(),
        Some("offline-requested"),
        "--offline must refuse the tier with its own reason code"
    );
}

// ------------------------------------------------ SC-006, the degradation matrix

/// Put a scripted `nix` on `PATH` and scan with the tier enabled.
///
/// The script receives the real argv, so it can answer `config show` one way
/// and `eval` another — which is how the reasons that live *past* the
/// pre-flight are reachable at all.
fn scan_with_stub_nix(script: &str) -> (bool, Option<String>) {
    let dir = tempfile::tempdir().unwrap();
    let stub = dir.path().join("nix");
    std::fs::write(&stub, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let out = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture("ifd"))
        .arg("--no-deep-hash")
        .arg("--nix-eval")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .output()
        .unwrap();
    let reason = std::fs::read(&out_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|d| {
            d["metadata"]["properties"]
                .as_array()?
                .iter()
                .find(|p| p["name"] == "waybill:nix-eval-degraded")?["value"]
                .as_str()
                .map(str::to_string)
        });
    (out.status.success(), reason)
}

/// A stub that passes the pre-flight, then behaves as the caller scripts.
fn stub(eval_behaviour: &str) -> String {
    format!(
        "#!/bin/sh\n\
         if [ \"$1\" = config ]; then echo 'allow-import-from-derivation = false'; exit 0; fi\n\
         if [ \"$1\" = eval ]; then\n{eval_behaviour}\nfi\n\
         exit 1\n"
    )
}

#[test]
fn tool_unusable_when_the_preflight_itself_cannot_run() {
    let (ok, reason) = scan_with_stub_nix(
        "#!/bin/sh\nif [ \"$1\" = config ]; then echo 'daemon not running' >&2; exit 1; fi\nexit 1\n",
    );
    assert!(ok, "an unusable nix must not fail the scan");
    assert_eq!(reason.as_deref(), Some("tool-unusable"));
}

#[test]
fn evaluation_failed_when_nix_exits_non_zero() {
    // `--raw` is the system probe and must succeed, or the run degrades for a
    // different reason and this test would pass while proving nothing.
    let (ok, reason) = scan_with_stub_nix(&stub(
        "  case \"$*\" in *--raw*) echo x86_64-linux; exit 0;; esac\n\
         \x20 echo 'error: something went wrong' >&2; exit 1",
    ));
    assert!(ok);
    assert_eq!(reason.as_deref(), Some("evaluation-failed"));
}

#[test]
fn no_evaluable_attribute_when_nix_reports_a_missing_attribute() {
    let (ok, reason) = scan_with_stub_nix(&stub(
        "  case \"$*\" in *--raw*) echo x86_64-linux; exit 0;; esac\n\
         \x20 echo \"error: attribute 'haskellPackages' missing\" >&2; exit 1",
    ));
    assert!(ok);
    assert_eq!(
        reason.as_deref(),
        Some("no-evaluable-attribute"),
        "a flake exposing nothing evaluable is a degradation, not an error \
         (haskell-language-server is the real-world case)"
    );
}

#[test]
fn revision_unfetchable_when_nix_cannot_acquire_the_pinned_revision() {
    let (ok, reason) = scan_with_stub_nix(&stub(
        "  case \"$*\" in *--raw*) echo x86_64-linux; exit 0;; esac\n\
         \x20 echo 'error: getFlake: unable to download' >&2; exit 1",
    ));
    assert!(ok);
    assert_eq!(reason.as_deref(), Some("revision-unfetchable"));
}
