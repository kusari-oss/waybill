//! Milestone 1274 US1 (#1283 part 1) — a refusal says it was a refusal.
//!
//! waybill refuses import-from-derivation on purpose (#1034, #1114). Measured
//! at 0.14.0, two flakes built from the same `.cabal` — one handwritten, one
//! using `callCabal2nix`, which needs IFD — produced documents an operator
//! could not tell apart: both said `evaluation-failed`, and the detail was
//! nix's leading git warning. A deliberate policy decision read as a bug in
//! waybill.
//!
//! # Why a stub `nix` rather than a committed `callCabal2nix` flake
//!
//! A deliberate departure from the task plan, recorded rather than silently
//! taken. A real `callCabal2nix` flake needs the pinned nixpkgs **fetched**
//! (300 MB+) and a GHC, which would make this the first fixture in the nix
//! suites to require network — `nix_config_closure.rs` keeps its `real_nix_*`
//! fixtures input-free precisely to avoid that.
//!
//! What is committed instead is stronger than a flake: **nix's own stderr,
//! captured verbatim** from the real refusal, at
//! `tests/fixtures/nix_ifd/*.stderr`. The stub replays it, so these tests are
//! hermetic *and* anchored to a measurement rather than to a hand-written
//! approximation of what nix says. The end-to-end run against real nix is the
//! committed probe at `specs/1274-nix-degradation-reason/measurements/`,
//! whose job is exactly to notice when nix's wording moves.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Reported by the stub as the host platform, so the attribute paths it is
/// asked for are the ones it answers for, on any host.
const SYSTEM: &str = "aarch64-darwin";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// nix's real stderr, captured from the live refusal and committed.
fn measured_stderr(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_ifd")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// A stub `nix` that gets the tier as far as evaluating a closure and then
/// fails with whatever stderr the scenario supplies.
///
/// The stderr is passed through a *file* rather than interpolated into the
/// script: nix's messages are full of single quotes, and a shell-quoted
/// heredoc would mangle the one thing these tests are about.
struct Stub {
    stderr: String,
    /// Whether `nix config show` reports the refusal in effect (#1114).
    /// False reaches `ifd-refusal-unverified`, which is a *different* reason.
    ifd_supported: bool,
}

impl Stub {
    fn failing_with(stderr: String) -> Self {
        Self { stderr, ifd_supported: true }
    }

    fn script(&self, stderr_file: &Path) -> String {
        format!(
            "#!/bin/sh\ncase \"$*\" in\n\
             \x20 *builtins.currentSystem*) printf '%s' '{SYSTEM}' ;;\n\
             \x20 \"config show\"*) echo 'allow-dirty = true'; {ifd} ;;\n\
             \x20 *\"#packages.{SYSTEM}\") echo '[\"default\"]' ;;\n\
             \x20 *\"#packages\") echo '[\"{SYSTEM}\"]' ;;\n\
             \x20 \"derivation show\"*) cat '{err}' >&2; exit 1 ;;\n\
             \x20 *) echo 'stub nix: unhandled invocation' >&2; exit 1 ;;\n\
             esac\n",
            ifd = if self.ifd_supported {
                "echo 'allow-import-from-derivation = false'"
            } else {
                ":"
            },
            err = stderr_file.display(),
        )
    }
}

struct Scan {
    stderr: String,
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

const C190: &str = "waybill:nix-closure-degraded";

impl Scan {
    /// The C190 value, after checking all three formats agree. The parity
    /// extractors assert this too; asserted here because a per-format
    /// divergence would make every assertion below format-specific without
    /// saying so.
    fn c190(&self) -> Option<Value> {
        let env = |raw: &str| -> Option<String> {
            let v: Value = serde_json::from_str(raw).ok()?;
            (v["field"].as_str() == Some(C190)).then(|| v["value"].as_str().map(str::to_string))?
        };
        let cdx = self.cdx["metadata"]["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"].as_str() == Some(C190))
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
        assert_eq!(cdx, spdx23, "C190 CycloneDX vs SPDX 2.3");
        assert_eq!(cdx, spdx3, "C190 CycloneDX vs SPDX 3");
        let raw = cdx?;
        Some(serde_json::from_str(&raw).unwrap_or_else(|e| {
            panic!("C190 is a JSON object since m1274: {raw:?} ({e})")
        }))
    }

    fn reason(&self) -> Option<String> {
        let v = self.c190()?;
        Some(v["reason"].as_str().expect("C190 carries a reason").to_string())
    }
}

/// A scan root the walker sees as a flake; the stub answers for it.
fn flake_root() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("flake.nix"), "{ outputs = { self }: { }; }\n").unwrap();
    d
}

fn run(stub: &Stub, extra: &[&str]) -> Scan {
    let root = flake_root();
    let dir = tempfile::tempdir().unwrap();
    let err_file = dir.path().join("nix.stderr");
    std::fs::write(&err_file, &stub.stderr).unwrap();
    let nix = dir.path().join("nix");
    std::fs::write(&nix, stub.script(&err_file)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = tempfile::tempdir().unwrap();
    let f = |n: &str| -> PathBuf { out.path().join(n) };
    let o = Command::new(binary_path())
        .args(["sbom", "scan", "--no-deep-hash", "--nix-closure", "--path"])
        .arg(root.path())
        .args(extra)
        .args([
            "--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json",
        ])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", f("o.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", f("o.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", f("o.spdx3.json").display()))
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "the scan must still succeed: {stderr}");
    let read = |n: &str| serde_json::from_slice(&std::fs::read(f(n)).unwrap()).unwrap();
    Scan {
        stderr,
        cdx: read("o.cdx.json"),
        spdx23: read("o.spdx.json"),
        spdx3: read("o.spdx3.json"),
    }
}

// ───────────────────────────────────────────────────────────────────────
// T010 / FR-001, FR-002, C2, SC-001 — the whole feature
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1274_us1_refusal_is_named_in_the_document() {
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")), &[]);
    assert_eq!(
        s.reason().as_deref(),
        Some("import-from-derivation-required"),
        "measured at 0.14.0: `evaluation-failed`, indistinguishable from a \
         broken flake"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T011 / FR-005, FR-005a, C6, SC-004 — THE NEGATIVE TEST
// ───────────────────────────────────────────────────────────────────────

/// An over-matching classifier turns one uninformative answer into one
/// *misleading* answer, which FR-005 calls worse than the status quo.
///
/// The stderr here is real (research R2): a flake whose `nixConfig` requests
/// `allow-import-from-derivation` and then fails on a missing attribute. The
/// setting's bare name **is** in its stderr, in a warning. `slack-web` sets
/// this option, so this is ordinary use rather than a contrivance.
#[test]
fn m1274_us1_an_unrelated_failure_keeps_the_generic_reason() {
    let raw = measured_stderr("nixconfig-unrelated.stderr");
    assert!(
        raw.contains("allow-import-from-derivation"),
        "the premise of this test: the setting's name IS present"
    );
    assert!(
        !raw.contains("because the option 'allow-import-from-derivation' is disabled"),
        "...and the consequence clause is NOT"
    );
    let s = run(&Stub::failing_with(raw), &[]);
    assert_eq!(
        s.reason().as_deref(),
        Some("evaluation-failed"),
        "a classifier keyed on the bare setting name reports a missing \
         attribute as a policy refusal"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T012 / FR-005c, C7 — the refusal wins, and the other error is kept
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1274_us1_a_refusal_alongside_another_error_is_still_a_refusal() {
    // Appended AFTER the clause, so the last error line is the other error —
    // which is the case precedence has to handle, and the case a classifier
    // riding on the selection rule gets wrong.
    let both = format!(
        "{}error: attribute 'waybillFixtureSomethingElse' missing\n       at /FIXTURE/flake.nix:9:1\n",
        measured_stderr("cabal2nix-refusal.stderr")
    );
    let s = run(&Stub::failing_with(both), &[]);
    let v = s.c190().expect("degraded");
    assert_eq!(v["reason"], "import-from-derivation-required", "the refusal wins");
    assert!(
        v["detail"].as_str().unwrap().contains("waybillFixtureSomethingElse"),
        "the other error's line is retained in the detail: {v}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T013 / FR-005b, FR-006, C8 — a reworded clause degrades to generic
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1274_us1_a_reworded_clause_degrades_to_generic() {
    let reworded = measured_stderr("cabal2nix-refusal.stderr").replace(
        "because the option 'allow-import-from-derivation' is disabled",
        "because import-from-derivation is not permitted",
    );
    let s = run(&Stub::failing_with(reworded), &[]);
    let v = s.c190().expect("still degraded, with no panic and no failed scan");
    assert_eq!(v["reason"], "evaluation-failed");
    assert!(
        v["detail"].as_str().unwrap().contains("not permitted"),
        "and the detail still reaches the line, so a nix rewording does not \
         leave the operator with nothing: {v}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T014 / FR-013, C10 — the published wire strings are unchanged
// ───────────────────────────────────────────────────────────────────────

/// `ifd-refusal-unverified` is asserted specifically. It means the refusal
/// could not be *confirmed in effect* — close to the opposite of the new
/// reason — and conflating the two would tell an operator their `nix` is too
/// old when in fact their flake needs a grant.
#[test]
fn m1274_us1_other_reasons_keep_their_wire_strings() {
    let s = run(
        &Stub {
            stderr: measured_stderr("cabal2nix-refusal.stderr"),
            ifd_supported: false,
        },
        &[],
    );
    assert_eq!(
        s.reason().as_deref(),
        Some("ifd-refusal-unverified"),
        "a nix that does not confirm the setting stops the tier BEFORE the \
         flake is touched, so the refusal-shaped stderr is never even read"
    );

    // `--offline` is refused before any nix process starts.
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")), &["--offline"]);
    assert_eq!(s.reason().as_deref(), Some("offline-requested"));
    assert!(
        s.c190().unwrap().get("detail").is_none(),
        "a reason with nothing to add carries no `detail` key"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T015 / FR-003, C13 — the log says it too
// ───────────────────────────────────────────────────────────────────────

/// Separate from the document assertions, which read the emitted JSON with
/// the log discarded. Milestone 1273 found that one assertion covering both
/// halves lets either rot.
#[test]
fn m1274_us1_the_reason_is_logged_too() {
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")), &[]);
    assert!(
        s.stderr.contains("import-from-derivation-required"),
        "the log carries the reason: {}",
        s.stderr
    );
    assert!(
        s.stderr.contains("nix-closure: degrading"),
        "and still names the degradation: {}",
        s.stderr
    );
}

// ───────────────────────────────────────────────────────────────────────
// FR-002a shape — asserted here as well as in the unit tests, because the
// unit tests cannot see what survives serialization into three formats.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1274_us1_the_document_value_is_an_object_in_all_three_formats() {
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")), &[]);
    let v = s.c190().expect("degraded");
    let obj = v.as_object().expect("an object");
    assert_eq!(obj.len(), 2, "reason and detail, nothing else: {v}");
    assert!(obj["reason"].is_string() && obj["detail"].is_string());
    assert!(!v.to_string().contains("null"), "omitted, never null: {v}");
}

// ───────────────────────────────────────────────────────────────────────
// #1446 — a requested closure that degrades says so at WARN
// ───────────────────────────────────────────────────────────────────────

/// The phrase that states the consequence. Asserted verbatim: the point of
/// the line is that an operator who asked for a closure reads, in words,
/// that they did not get one.
const CONSEQUENCE: &str =
    "no Nix closure components were added; the document contains only manifest-derived components";

/// The one WARN line the degraded tier emits, if any.
fn closure_warn_line(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .find(|l| l.contains("WARN") && l.contains("nix-closure: degrading"))
}

/// Measured on `slack-web` (#1446): `--nix-closure` degraded on
/// import-from-derivation, the scan exited 0 with the manifest-only document,
/// and the only log line was INFO.
#[test]
fn issue_1446_a_requested_closure_that_degrades_warns() {
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")), &[]);
    let line = closure_warn_line(&s.stderr)
        .unwrap_or_else(|| panic!("no WARN line for the degraded closure:\n{}", s.stderr));
    assert!(line.contains("import-from-derivation-required"), "names the reason: {line}");
    assert!(line.contains(CONSEQUENCE), "states the consequence: {line}");
    assert!(
        line.contains("callCabal2nix") && line.contains("would build during evaluation"),
        "explains what import-from-derivation means here: {line}"
    );
}

/// Control for the level chosen for `--offline`: WARN, because the scan still
/// did not do what `--nix-closure` asked. The import-from-derivation sentence
/// is reason-specific and must not appear here.
#[test]
fn issue_1446_offline_keeps_the_chosen_level() {
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")), &["--offline"]);
    let line = closure_warn_line(&s.stderr)
        .unwrap_or_else(|| panic!("no WARN line for the offline refusal:\n{}", s.stderr));
    assert!(line.contains("offline-requested"), "{line}");
    assert!(line.contains(CONSEQUENCE), "{line}");
    assert!(!line.contains("callCabal2nix"), "reason-specific text leaked: {line}");
}

/// Control: without `--nix-closure` nothing was asked for, so nothing warns.
#[test]
fn issue_1446_no_warning_when_the_closure_was_not_requested() {
    let root = flake_root();
    let out = tempfile::tempdir().unwrap();
    let o = Command::new(binary_path())
        .args(["sbom", "scan", "--no-deep-hash", "--offline", "--path"])
        .arg(root.path())
        .arg("--output")
        .arg(out.path().join("o.cdx.json"))
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{stderr}");
    assert!(stderr.contains("scan complete"), "the scan ran and logged: {stderr}");
    assert!(
        !stderr.contains("nix-closure: degrading"),
        "an unrequested closure is not a degradation: {stderr}"
    );
}
