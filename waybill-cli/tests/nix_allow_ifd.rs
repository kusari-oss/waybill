//! Milestone 1283 (#1283) — `--nix-allow-ifd`, the opt-in that lets a Nix
//! evaluation build derivations (import-from-derivation), verified.
//!
//! Every scan here drives the shared stub `nix` (`common/nix_stub.rs`), which
//! records each argv and makes no network access. The stub models an IFD
//! flake with `ifd_required`: every invocation naming the project's flake
//! fails with nix's **measured** refusal stderr
//! (`fixtures/nix_ifd/cabal2nix-refusal.stderr`) unless its argv allows IFD.
//! A real `callCabal2nix` flake would need a 300 MB nixpkgs fetch and a GHC
//! (`fixtures/nix_ifd/README.md`). Every name is `waybill-fixture-*`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const C190: &str = "waybill:nix-closure-degraded";
const C184: &str = "waybill:nix-closure";
const C216: &str = "waybill:closure-scope-method";
const C217: &str = "waybill:nix-evaluation-policy";
const IFD: &str = "allow-import-from-derivation";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_runtime")
}

fn fixture(name: &str) -> String {
    let path = dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn base<'a>(closure: &'a str) -> NixStub<'a> {
    NixStub { project: dir().join("project"), closure, ..Default::default() }
}

fn scan(stub: &NixStub<'_>, extra: &[&str]) -> Scan {
    let mut argv = vec!["--nix-closure", "--no-deps-dev"];
    argv.extend_from_slice(extra);
    stub.scan(&argv)
}

/// The value of a milestone-071 annotation envelope, when it carries `field`.
fn envelope(raw: &str, field: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    (v.get("field")?.as_str()? == field).then(|| v.get("value")?.as_str().map(str::to_string))?
}

/// A document-scope row in each format, asserted equal across the three.
fn doc_row(s: &Scan, name: &str) -> Option<String> {
    let cdx = s.cdx["metadata"]["properties"]
        .as_array()
        .and_then(|ps| ps.iter().find(|p| p["name"] == name))
        .and_then(|p| p["value"].as_str())
        .map(str::to_string);
    let spdx23 = s.spdx23["annotations"]
        .as_array()
        .and_then(|a| a.iter().filter_map(|x| x["comment"].as_str()).find_map(|r| envelope(r, name)));
    let spdx3 = s.spdx3["@graph"].as_array().and_then(|g| {
        g.iter()
            .filter(|e| e["type"] == "Annotation")
            .filter_map(|e| e["statement"].as_str())
            .find_map(|r| envelope(r, name))
    });
    assert_eq!(spdx23, cdx, "{name}: SPDX 2.3 vs CycloneDX");
    assert_eq!(spdx3, cdx, "{name}: SPDX 3 vs CycloneDX");
    cdx
}

fn reason(s: &Scan) -> Option<String> {
    let raw = doc_row(s, C190)?;
    let v: Value = serde_json::from_str(&raw).unwrap();
    Some(v["reason"].as_str().unwrap().to_string())
}

/// The argv lines that carry the import-from-derivation setting, with the
/// value that follows it.
fn ifd_values(log: &str) -> Vec<(String, String)> {
    log.lines()
        .filter_map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            let i = words.iter().position(|w| *w == IFD)?;
            Some((l.to_string(), words.get(i + 1).copied().unwrap_or("").to_string()))
        })
        .collect()
}

/// Every recorded argv carrying the setting carries `want`, and there is at
/// least one of each evaluating kind, so the assertion is not vacuous.
fn assert_every_ifd_value(s: &Scan, want: &str) {
    let values = ifd_values(&s.nix_log);
    assert!(
        values.iter().any(|(l, _)| l.starts_with("config show")),
        "the pre-flight ran: {}",
        s.nix_log
    );
    assert!(
        values.iter().any(|(l, _)| l.starts_with("derivation show")),
        "the closure was asked for: {}",
        s.nix_log
    );
    for (line, v) in &values {
        assert_eq!(v, want, "every Nix call carries {IFD} {want}: {line}");
    }
}

fn ifd_flake<'a>(closure: &'a str) -> NixStub<'a> {
    NixStub { ifd_required: true, ..base(closure) }
}

// ───────────────────────────────────────────────────────────────────────
// T012 (a) — the opt-in, verified: the IFD flake's closure is read
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t012a_the_opt_in_reads_an_ifd_flakes_closure() {
    let closure = fixture("closure-runtime.json");
    let s = scan(&ifd_flake(&closure), &["--nix-allow-ifd"]);
    assert_eq!(reason(&s), None, "no degradation; stderr:\n{}", s.stderr);
    assert!(doc_row(&s, C184).is_some(), "the closure was recorded");
    assert_every_ifd_value(&s, "true");
}

// ───────────────────────────────────────────────────────────────────────
// T012 (b) — FR-001: without the flag, every call refuses, as today
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t012b_without_the_flag_every_call_refuses() {
    let closure = fixture("closure-runtime.json");
    let s = scan(&base(&closure), &[]);
    assert_eq!(reason(&s), None, "a non-IFD flake still reads; stderr:\n{}", s.stderr);
    assert_every_ifd_value(&s, "false");

    // And the IFD flake degrades exactly as it did before this milestone.
    let s = scan(&ifd_flake(&closure), &[]);
    assert_eq!(reason(&s).as_deref(), Some("import-from-derivation-required"));
}

// ───────────────────────────────────────────────────────────────────────
// T012 (c) — FR-003: an allowance nix does not report is not assumed
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t012c_an_unverified_allowance_degrades_before_anything_is_evaluated() {
    let closure = fixture("closure-runtime.json");
    let s = scan(
        &NixStub { ifd_reported: Some("false"), ..ifd_flake(&closure) },
        &["--nix-allow-ifd"],
    );
    assert_eq!(reason(&s).as_deref(), Some("ifd-allowance-unverified"), "{}", s.stderr);
    let after: Vec<&str> = s
        .nix_log
        .lines()
        .skip_while(|l| !l.starts_with("config show"))
        .skip(1)
        .filter(|l| l.starts_with("eval") || l.starts_with("derivation"))
        .collect();
    assert!(after.is_empty(), "nothing is evaluated after a failed pre-flight: {after:?}");
}

// ───────────────────────────────────────────────────────────────────────
// T012 (d) — FR-010: a failed IFD build is its own reason
// ───────────────────────────────────────────────────────────────────────

fn measured(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_ifd").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn t012d_a_failed_ifd_build_is_distinct_from_the_refusal() {
    let closure = fixture("closure-runtime.json");
    let s = scan(
        &NixStub {
            closure_stderr: Some(measured("ifd-build-failed.stderr")),
            ..ifd_flake(&closure)
        },
        &["--nix-allow-ifd"],
    );
    assert_eq!(
        reason(&s).as_deref(),
        Some("import-from-derivation-build-failed"),
        "stderr:\n{}",
        s.stderr
    );
    let v: Value = serde_json::from_str(&doc_row(&s, C190).unwrap()).unwrap();
    assert!(
        v["detail"].as_str().unwrap().contains("waybill-fixture-ifd-broken.drv"),
        "the detail names the derivation that failed: {v}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T012 (e) — the flag needs a Nix tier
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t012e_the_flag_without_a_nix_tier_is_rejected() {
    let out = tempfile::tempdir().unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--no-deep-hash", "--offline", "--nix-allow-ifd", "--path"])
        .arg(dir().join("project"))
        .arg("--output")
        .arg(out.path().join("o.cdx.json"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success(), "clap must reject it: {stderr}");
    assert!(stderr.contains("required"), "{stderr}");
    assert!(stderr.contains("--nix-closure") && stderr.contains("--nix-eval"), "{stderr}");
    assert!(!out.path().join("o.cdx.json").exists(), "no document is written");

    // CONTROL: each tier alone admits it.
    for tier in ["--nix-closure", "--nix-eval"] {
        let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
            .args(["sbom", "scan", "--no-deep-hash", "--offline", "--nix-allow-ifd", tier])
            .arg("--path")
            .arg(dir().join("project"))
            .arg("--output")
            .arg(out.path().join("o.cdx.json"))
            .output()
            .unwrap();
        assert!(o.status.success(), "{tier}: {}", String::from_utf8_lossy(&o.stderr));
    }
}

// ───────────────────────────────────────────────────────────────────────
// T012 (f) — FR-004: the opt-in relaxes nothing else
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t012f_the_opt_in_never_accepts_flake_config_or_goes_impure() {
    let closure = fixture("closure-runtime.json");
    let s = scan(&ifd_flake(&closure), &["--nix-allow-ifd"]);
    assert!(doc_row(&s, C184).is_some(), "CONTROL: the tier ran: {}", s.stderr);
    for line in s.nix_log.lines() {
        assert!(!line.contains("--accept-flake-config"), "{line}");
        // The one impure call is host-platform detection, which evaluates a
        // builtin and nothing the repository controls.
        if !line.contains("builtins.currentSystem") {
            assert!(!line.contains("--impure"), "{line}");
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// T012 (f′) — research R1: the #1461 output selection allows IFD too
// ───────────────────────────────────────────────────────────────────────

fn selection_line(s: &Scan) -> String {
    s.nix_log
        .lines()
        .find(|l| l.starts_with("eval") && l.contains("outputSpecified"))
        .unwrap_or_else(|| panic!("no selection evaluation: {}", s.nix_log))
        .to_string()
}

fn outputs_stub<'a>(closure: &'a str) -> NixStub<'a> {
    let listing = |b: &[&str]| b.iter().map(|x| format!("/nix/store/{x}")).collect::<Vec<_>>().join("\n");
    NixStub {
        runtime_by_output: vec![
            ("h-app-lib", listing(&["h-app-lib", "h-ssl", "h-z", "h-xml", "h-py"])),
            ("h-app-bin", listing(&["h-app-bin", "h-ssl", "h-z"])),
            ("h-app-doc", listing(&["h-app-doc"])),
        ],
        selection: Some(r#"{"outputs":["bin"],"source":"output-specified"}"#),
        ..base(closure)
    }
}

#[test]
fn t012f_prime_the_output_selection_allows_ifd_under_the_opt_in() {
    let closure = fixture("closure-outputs.json");
    let s = scan(&NixStub { ifd_required: true, ..outputs_stub(&closure) }, &["--nix-allow-ifd"]);
    assert!(
        selection_line(&s).contains(&format!("{IFD} true")),
        "{}",
        selection_line(&s)
    );
    let c216: Value = serde_json::from_str(&doc_row(&s, C216).expect("C216")).unwrap();
    assert_eq!(
        c216["outputs-source"], "output-specified",
        "a refused selection would fall back to the default output: {c216}"
    );

    // Without the flag, on a flake that needs no IFD, it refuses.
    let s = scan(&outputs_stub(&closure), &[]);
    assert!(
        selection_line(&s).contains(&format!("{IFD} false")),
        "{}",
        selection_line(&s)
    );
}

// ───────────────────────────────────────────────────────────────────────
// T021 (g) — FR-005: --help states the trade where the decision is made
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t021g_help_names_the_flag_and_its_cautions() {
    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--help"])
        .output()
        .unwrap();
    assert!(o.status.success());
    let help = String::from_utf8_lossy(&o.stdout);
    // The flag's own entry: from the line that starts with it, up to the next
    // line that starts another option. Other entries mention the flag too.
    let lines: Vec<&str> = help.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with("--nix-allow-ifd"))
        .unwrap_or_else(|| panic!("flag absent:\n{help}"));
    let entry: String = std::iter::once(lines[at])
        .chain(lines[at + 1..].iter().copied().take_while(|l| !l.trim_start().starts_with('-')))
        .collect::<Vec<_>>()
        .join(" ");
    let entry = entry.split_whitespace().collect::<Vec<_>>().join(" ");
    for needle in ["BUILD", "untrusted flake", "unsandboxed"] {
        assert!(entry.contains(needle), "{needle:?} missing from the flag's help: {entry}");
    }
}

/// The WARN lines a scan logged.
fn warns(s: &Scan) -> Vec<&str> {
    s.stderr.lines().filter(|l| l.contains("WARN")).collect()
}

// ───────────────────────────────────────────────────────────────────────
// T021 (h) — FR-006: the refusal's WARN names the opt-in, with the caution
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t021h_the_ifd_degradation_warn_names_the_flag() {
    let closure = fixture("closure-runtime.json");
    let s = scan(&ifd_flake(&closure), &[]);
    let line = warns(&s)
        .into_iter()
        .find(|l| l.contains("import-from-derivation-required"))
        .unwrap_or_else(|| panic!("no degradation WARN:\n{}", s.stderr));
    assert!(line.contains("--nix-allow-ifd"), "{line}");
    assert!(line.contains("trust"), "{line}");
}

// ───────────────────────────────────────────────────────────────────────
// T021 (i), (j) — FR-013: an unsandboxed allowance warns once, and proceeds
// ───────────────────────────────────────────────────────────────────────

#[test]
fn t021i_sandbox_off_under_the_opt_in_warns_once_and_proceeds() {
    let closure = fixture("closure-runtime.json");
    let s = scan(&NixStub { sandbox: Some("false"), ..ifd_flake(&closure) }, &["--nix-allow-ifd"]);
    let sandbox: Vec<&str> =
        warns(&s).into_iter().filter(|l| l.contains("sandbox is off")).collect();
    assert_eq!(sandbox.len(), 1, "exactly one sandbox WARN:\n{}", s.stderr);
    assert!(sandbox[0].contains("--nix-allow-ifd"), "{}", sandbox[0]);
    assert!(doc_row(&s, C184).is_some(), "the scan proceeds and reads the closure");
    assert_eq!(reason(&s), None);
}

#[test]
fn t021j_no_sandbox_warn_when_the_sandbox_is_on_or_ifd_refused() {
    let closure = fixture("closure-runtime.json");
    let s = scan(&NixStub { sandbox: Some("true"), ..ifd_flake(&closure) }, &["--nix-allow-ifd"]);
    assert!(doc_row(&s, C184).is_some(), "CONTROL: the tier ran: {}", s.stderr);
    assert!(!s.stderr.contains("sandbox is off"), "{}", s.stderr);
    // Without the opt-in nothing is built, so an off sandbox is not news.
    let s = scan(&NixStub { sandbox: Some("false"), ..base(&closure) }, &[]);
    assert!(doc_row(&s, C184).is_some(), "CONTROL: the tier ran: {}", s.stderr);
    assert!(!s.stderr.contains("sandbox is off"), "{}", s.stderr);
}

// ───────────────────────────────────────────────────────────────────────
// T019 — FR-002, R1: the IFD value always comes from the policy
// ───────────────────────────────────────────────────────────────────────

/// Every `.rs` under the Nix reader.
fn nix_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            nix_sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// A source file with every `#[cfg(test)]` item removed. Live-nix tests
/// inside test modules (`eval/licenses.rs`) keep their literals.
///
/// Not "the text before the first `#[cfg(test)]`": `eval/invoke.rs` has a
/// test module in the middle, and the version evaluation's argv
/// (`resolving_argv`) is defined after it, so a cut there would exempt one of
/// the eight sites. The item is skipped by brace depth from its first `{`.
fn non_test_text(src: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    let mut opened = false;
    let mut depth = 0i64;
    for line in src.split_inclusive('\n') {
        if !skipping && line.trim_start().starts_with("#[cfg(test)]") {
            skipping = true;
            opened = false;
            depth = 0;
            continue;
        }
        if !skipping {
            out.push_str(line);
            continue;
        }
        for c in line.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        // An item without a body (`#[cfg(test)] use …;`) ends at its `;`.
        if (opened && depth <= 0) || (!opened && line.trim_end().ends_with(';')) {
            skipping = false;
        }
    }
    out
}

/// No `IFD_SETTING` in non-test code is followed by a string literal value.
/// Argv vectors are often one element per line, so the next token is found
/// after skipping whitespace and commas, across lines.
#[test]
fn t019_no_nix_call_site_hard_codes_the_ifd_value() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/scan_fs/package_db/nix");
    let mut files = Vec::new();
    nix_sources(&root, &mut files);
    files.sort();
    let mut uses = 0usize;
    let mut from_policy = 0usize;
    let mut literals = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let text = non_test_text(&src);
        let text = text.as_str();
        for (i, _) in text.match_indices("IFD_SETTING") {
            uses += 1;
            let next = text[i + "IFD_SETTING".len()..]
                .trim_start_matches(|c: char| c.is_whitespace() || c == ',');
            if next.starts_with("\"false\"") || next.starts_with("\"true\"") {
                let shown: String = next.chars().take(20).collect();
                literals.push(format!("{}: {shown}", f.display()));
            }
            if next.starts_with("ifd.policy().setting()") || next.starts_with("policy.setting()") {
                from_policy += 1;
            }
        }
    }
    assert!(
        literals.is_empty(),
        "the import-from-derivation value must come from IfdPolicy::setting: {literals:?}"
    );
    // CONTROL: the guard found the call sites, so it cannot pass by finding
    // none. The argv sites are the closure, the attribute listing, the
    // output selection, the pre-flight, the version, licence and declaration
    // evaluations — seven constructions covering research R1's eight sites,
    // whose eighth (`declarations/evaluate.rs:~237`) was a test-module copy
    // now built by the same `declarations_argv`.
    assert!(
        uses >= 8,
        "only {uses} uses of IFD_SETTING in non-test code across {} files",
        files.len()
    );
    assert!(from_policy >= 7, "only {from_policy} argv sites take the policy's value");
}
