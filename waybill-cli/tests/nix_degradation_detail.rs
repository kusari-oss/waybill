//! Milestone 1274 US2 (#1283 part 1) — the detail names the line that
//! explains the failure.
//!
//! The reason being specific matters most for the one cause US1 names. The
//! *detail* matters for every other cause, which is why this story was not
//! dropped: the same blind prefix mis-reports a syntax error or a nixpkgs
//! type error just as badly, and there is no classifier coming for those.
//!
//! Measured at 0.14.0 on the closure path: the recorded detail was
//! `warning: Git tree '/private/var/.../cabal2nix' has uncommitted changes`
//! — nix's first line — while the line that explained the failure sat 855
//! characters past the end of the window.
//!
//! See `nix_ifd_reason.rs` for why these drive a stub `nix` over committed
//! real stderr rather than a live `callCabal2nix` flake.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const SYSTEM: &str = "aarch64-darwin";
const CLAUSE: &str = "because the option 'allow-import-from-derivation' is disabled";

/// A closure `nix derivation show` would answer with, for the control case.
const CLOSURE_JSON: &str = r#"{"derivations":{"aaa-lib-1.0.drv":{"name":"lib-1.0","env":{"pname":"lib","version":"1.0"},"outputs":{"out":{"path":"hash1-lib-1.0"}}},"bbb-app-2.0.drv":{"name":"app-2.0","env":{"pname":"app","version":"2.0","buildInputs":"/nix/store/hash1-lib-1.0"},"outputs":{"out":{"path":"hash2-app-2.0"}}}},"version":3}"#;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn measured_stderr(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_ifd")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The selection rule, reimplemented in the test from the specification
/// rather than imported.
///
/// `waybill-cli` is a binary crate, so an integration test cannot call
/// `eval::stderr::salient_stderr` — and that is the better arrangement here
/// anyway: importing the implementation would let a wrong rule agree with
/// itself. This is an independent statement of what FR-007 requires, which
/// the emitted document is then checked against.
fn expected_detail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    let start = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with("error") || l.contains(": error:"))
        .unwrap_or(0);
    lines[start..].join("\n").chars().take(300).collect()
}

struct Stub {
    /// `None` makes `derivation show` succeed, which is the control.
    stderr: Option<String>,
}

impl Stub {
    fn failing_with(stderr: String) -> Self {
        Self { stderr: Some(stderr) }
    }
    fn succeeding() -> Self {
        Self { stderr: None }
    }
    fn script(&self, stderr_file: &Path) -> String {
        let show = match &self.stderr {
            Some(_) => format!("cat '{}' >&2; exit 1", stderr_file.display()),
            // Real nix prints the git-tree warning on a SUCCESSFUL
            // evaluation too, so the control must carry one. Without it this
            // control passed while a mutation that degraded any evaluation
            // whose stderr mentioned a warning went uncaught here — caught
            // only by the sibling suite's real-nix tests, which is luck
            // rather than coverage.
            None => format!(
                "echo \"warning: Git tree '/FIXTURE' has uncommitted changes\" >&2; echo '{CLOSURE_JSON}'"
            ),
        };
        // m1423 (T006): the `path-info` arm is intended. No store holds the
        // evaluated output, so the scope pass takes its fallback path.
        format!(
            "#!/bin/sh\ncase \"$*\" in\n\
             \x20 *builtins.currentSystem*) printf '%s' '{SYSTEM}' ;;\n\
             \x20 \"config show\"*) echo 'allow-dirty = true'; echo 'allow-import-from-derivation = false' ;;\n\
             \x20 *\"#packages.{SYSTEM}\") echo '[\"default\"]' ;;\n\
             \x20 *\"#packages\") echo '[\"{SYSTEM}\"]' ;;\n\
             \x20 \"derivation show\"*) {show} ;;\n\
             \x20 \"path-info\"*) echo \"error: path '/nix/store/stub' is not valid\" >&2; exit 1 ;;\n\
             \x20 *) echo 'stub nix: unhandled invocation' >&2; exit 1 ;;\n\
             esac\n"
        )
    }
}

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

const C190: &str = "waybill:nix-closure-degraded";

impl Scan {
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
        serde_json::from_str(&cdx?).ok()
    }
    fn detail(&self) -> Option<String> {
        Some(self.c190()?["detail"].as_str()?.to_string())
    }
    fn components(&self) -> usize {
        self.cdx["components"].as_array().map_or(0, Vec::len)
    }
}

fn flake_root() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("flake.nix"), "{ outputs = { self }: { }; }\n").unwrap();
    d
}

fn run(stub: &Stub) -> Scan {
    let root = flake_root();
    let dir = tempfile::tempdir().unwrap();
    let err_file = dir.path().join("nix.stderr");
    std::fs::write(&err_file, stub.stderr.clone().unwrap_or_default()).unwrap();
    let nix = dir.path().join("nix");
    std::fs::write(&nix, stub.script(&err_file)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = tempfile::tempdir().unwrap();
    let f = |n: &str| -> PathBuf { out.path().join(n) };
    let o = Command::new(binary_path())
        .args(["sbom", "scan", "--no-deep-hash", "--nix-closure", "--path"])
        .arg(root.path())
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
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "the scan must still succeed: {stderr}");
    let read = |n: &str| serde_json::from_slice(&std::fs::read(f(n)).unwrap()).unwrap();
    Scan {
        cdx: read("o.cdx.json"),
        spdx23: read("o.spdx.json"),
        spdx3: read("o.spdx3.json"),
    }
}

// ───────────────────────────────────────────────────────────────────────
// T022 / FR-002a, FR-007, C3, SC-002 — the detail reaches the document
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1274_us2_the_document_carries_the_explanatory_line() {
    let s = run(&Stub::failing_with(measured_stderr("cabal2nix-refusal.stderr")));
    let d = s.detail().expect("FR-002a — the degradation carries a detail");
    assert!(d.contains(CLAUSE), "the document, with the log discarded: {d:?}");
}

// ───────────────────────────────────────────────────────────────────────
// T023 / FR-007, C4 — the cause, not the stack trace
// ───────────────────────────────────────────────────────────────────────

/// C4's whole point: an assertion that the detail merely *differs* from the
/// git warning passes on 855 characters of stack trace, which is no more
/// useful to an operator than the warning was.
#[test]
fn m1274_us2_the_detail_is_the_cause_not_the_stack_trace() {
    let raw = measured_stderr("cabal2nix-refusal.stderr");
    // The premise, asserted so a re-captured fixture cannot quietly make this
    // test vacuous: nix really does print the trace first and the cause last.
    let lines: Vec<&str> = raw.lines().collect();
    let errs: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with("error") || l.contains(": error:"))
        .map(|(i, _)| i)
        .collect();
    assert!(
        errs.len() >= 2 && lines[errs[0]].trim() == "error:",
        "the fixture must still open with a bare `error:` and carry the cause \
         later; error lines at {errs:?}"
    );
    assert!(
        lines[*errs.last().unwrap()].contains(CLAUSE),
        "...and the cause must still be on the LAST error line"
    );

    let s = run(&Stub::failing_with(raw));
    let d = s.detail().unwrap();
    assert!(d.contains(CLAUSE), "{d:?}");
    assert!(
        !d.contains("while evaluating"),
        "the stack trace must not fill the window: {d:?}"
    );
    assert!(
        !d.contains("uncommitted changes"),
        "and neither must the git warning: {d:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T024 / FR-008, C5, SC-005 — the closure path uses the one rule
// ───────────────────────────────────────────────────────────────────────

/// Byte-equality against an independent statement of the rule, rather than
/// "the detail is better than it was".
///
/// Before this milestone the closure path had its own
/// `stderr.trim().chars().take(200)` — a *second* rule, a hundred lines from
/// the first, which nobody had noticed disagreeing. Asserting equality with
/// the specified selection is what makes a third copy fail rather than merely
/// look different.
#[test]
fn m1274_us2_both_paths_produce_the_same_detail() {
    let raw = measured_stderr("cabal2nix-refusal.stderr");
    let s = run(&Stub::failing_with(raw.clone()));
    assert_eq!(
        s.detail().unwrap(),
        expected_detail(raw.trim()),
        "the closure path's detail must be the selection FR-007 specifies, \
         byte for byte"
    );
}

/// FR-008 at the source level: there must not be a second selection rule to
/// drift from the first. The defect this milestone fixes was invisible for
/// three milestones precisely because a copy compiles.
#[test]
fn m1274_us2_there_is_exactly_one_selection_rule() {
    let nix = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/scan_fs/package_db/nix");
    let mut offenders = Vec::new();
    let mut rules = 0usize;
    let mut stack = vec![nix.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            let whole = std::fs::read_to_string(&p).unwrap();
            // Production code only. A `#[cfg(test)]` module legitimately
            // restates the rule in order to compare it against the one it
            // replaced, and counting that would make the assertion unable to
            // distinguish a documented comparison from a shipped copy.
            let src = whole.split("#[cfg(test)]").next().unwrap_or("").to_string();
            let rel = p.strip_prefix(&nix).unwrap().display().to_string();
            for (i, line) in src.lines().enumerate() {
                // Skip the comments that document the old rule by quoting it.
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains("rposition(|l| is_error_line(l))") {
                    rules += 1;
                }
                // Any hand-rolled stderr truncation at all. Deliberately
                // without an exception for log fields: the one that was
                // found here WAS a log field, with the same defect, and an
                // exception would have hidden it.
                if line.contains("chars().take(") && line.contains("stderr") {
                    offenders.push(format!("{rel}:{}: {}", i + 1, line.trim()));
                }
            }
        }
    }
    assert_eq!(rules, 1, "one selection rule, not {rules}");
    assert!(
        offenders.is_empty(),
        "a second stderr truncation is a second rule waiting to drift:\n  {}",
        offenders.join("\n  ")
    );
}

// ───────────────────────────────────────────────────────────────────────
// T025 / FR-010, C9, SC-003 — a flake that evaluates is untouched
// ───────────────────────────────────────────────────────────────────────

/// The control is load-bearing. A `callCabal2nix` flake that degrades proves
/// nothing on its own, because a *broken fixture* degrades too.
///
/// SC-003's byte-identity claim about the real 1,170-derivation control flake
/// is checked by the committed probe, which runs real `nix`; what this test
/// holds is that nothing on the classifier's path changed whether a
/// successful evaluation stays successful.
#[test]
fn m1274_us2_a_flake_that_evaluates_is_untouched() {
    let s = run(&Stub::succeeding());
    assert!(
        s.c190().is_none(),
        "a successful closure records no degradation: {:?}",
        s.c190()
    );
    assert!(s.components() > 0, "and the closure's components are emitted");
}

// ───────────────────────────────────────────────────────────────────────
// T026 / FR-009 — an unidentifiable failure still records something
// ───────────────────────────────────────────────────────────────────────

/// An unparseable failure is not an excuse for silence. A detail an operator
/// has to interpret beats an absent one.
#[test]
fn m1274_us2_an_unidentifiable_failure_still_records_something() {
    let odd = "the nix daemon said something unusual\nand then stopped\n";
    assert!(
        !odd.lines().any(|l| l.trim_start().starts_with("error")),
        "the premise: no line matches the error predicate"
    );
    let s = run(&Stub::failing_with(odd.to_string()));
    let v = s.c190().expect("still degraded");
    assert_eq!(v["reason"], "evaluation-failed");
    let d = v["detail"].as_str().expect("FR-009 — something, not nothing");
    assert!(d.starts_with("the nix daemon said something unusual"), "{d:?}");
}
