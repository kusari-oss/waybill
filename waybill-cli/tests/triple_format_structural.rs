//! Triple-format structural-correctness sibling test (milestone 094).
//!
//! Replaces `triple_format_perf.rs`'s wall-clock perf assertion as the
//! per-PR signal — that test was repeatedly flaking on macOS-latest at
//! 14-22% measured-reduction vs a 25% threshold. This file's tests
//! catch the SAME class of regression (single-pass dispatch breakage
//! that causes the scan pipeline to run N times instead of 1) using a
//! deterministic side-channel signal instead of wall-clock timing:
//!
//!   - The production code at `waybill-cli/src/cli/scan_cmd.rs:1413`
//!     emits `tracing::info!(..., "scan starting")` exactly once per
//!     scan-pipeline invocation.
//!   - We capture stderr from `waybill sbom scan` and count "scan
//!     starting" occurrences. A correct triple-format invocation
//!     produces count == 1; a broken-single-pass implementation would
//!     produce count == 3.
//!   - Plus a byte-equivalence sibling that confirms triple-format
//!     output matches single-format output for each format (catches
//!     dispatch-correctness regressions even if single-pass count is
//!     wrong).
//!
//! Zero wall-clock semantics. Binary pass/fail. Deterministic — the 3
//! assertions either hold or they don't, independent of CI runner
//! thermal state, scheduler jitter, or page-cache contents.
//!
//! See `specs/094-deflake-perf-tests/` for the full rationale + the
//! 100-iteration determinism check that gates this test's quality.
//!
//! The actual wall-clock perf test remains in `triple_format_perf.rs`
//! but is now `#[ignore]`'d; the dedicated `.github/workflows/perf.yml`
//! lane runs it on demand (PR `perf` label or scheduled nightly).

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

mod common;

use common::normalize::{
    apply_fake_home_env, normalize_cdx_for_golden, normalize_spdx23_for_golden,
    normalize_spdx3_for_golden,
};
use common::{bin, workspace_root};

/// Format-specific normalizer signature, shared by the three byte-equivalence
/// comparison rows in `triple_format_outputs_byte_match_three_sequential`.
/// Hoisted to a type alias because clippy's `clippy::type_complexity` lint
/// flags an inline `fn(&str, &Path) -> String` in tuple position.
type NormalizeFn = fn(&str, &std::path::Path) -> String;

// ---------------------------------------------------------------------
// Fixture-build helpers — duplicated from `triple_format_perf.rs`
// because both files are standalone test targets and this test's
// determinism requirement makes a shared `tests/common/mod.rs` helper
// out-of-scope for milestone 094 (FR-008 forbids non-test changes).
// ---------------------------------------------------------------------

struct ImageFile {
    path: &'static str,
    content: Vec<u8>,
}

fn build_synthetic_image(files: &[ImageFile]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut layer_bytes = Vec::new();
    {
        let mut layer_tar = tar::Builder::new(&mut layer_bytes);
        for f in files {
            let mut header = tar::Header::new_ustar();
            header.set_path(f.path).expect("set_path");
            header.set_size(f.content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            layer_tar
                .append(&header, f.content.as_slice())
                .expect("tar append");
        }
        layer_tar.finish().expect("layer finish");
    }
    let manifest = r#"[{"Config":"config.json","RepoTags":["waybill-perf-triple-structural:latest"],"Layers":["layer0/layer.tar"]}]"#;
    let tar_path = dir.path().join("image.tar");
    let file = std::fs::File::create(&tar_path).expect("create image.tar");
    {
        let mut outer = tar::Builder::new(file);
        let mut mh = tar::Header::new_ustar();
        mh.set_path("manifest.json").unwrap();
        mh.set_size(manifest.len() as u64);
        mh.set_mode(0o644);
        mh.set_cksum();
        outer
            .append(&mh, manifest.as_bytes())
            .expect("outer append manifest");
        let mut lh = tar::Header::new_ustar();
        lh.set_path("layer0/layer.tar").unwrap();
        lh.set_size(layer_bytes.len() as u64);
        lh.set_mode(0o644);
        lh.set_cksum();
        outer
            .append(&lh, layer_bytes.as_slice())
            .expect("outer append layer");
        outer.into_inner().expect("outer finish").flush().expect("flush");
    }
    (dir, tar_path)
}

/// Small synthetic image — enough to exercise the scan pipeline but
/// fast (~1s per invocation). Smaller than the wall-clock perf
/// fixture because we don't care about timing here.
fn build_structural_fixture() -> (tempfile::TempDir, PathBuf) {
    let mut files: Vec<ImageFile> = Vec::new();

    files.push(ImageFile {
        path: "etc/os-release",
        content: b"ID=debian\nVERSION_ID=12\nVERSION_CODENAME=bookworm\n".to_vec(),
    });

    let mut dpkg = String::new();
    for i in 0..20 {
        use std::fmt::Write as _;
        write!(
            dpkg,
            "Package: pkg-{i:04}\n\
             Status: install ok installed\n\
             Version: 1.{i}.0\n\
             Architecture: amd64\n\
             Maintainer: Debian <debian@example.org>\n\n",
        )
        .unwrap();
    }
    files.push(ImageFile {
        path: "var/lib/dpkg/status",
        content: dpkg.into_bytes(),
    });

    build_synthetic_image(&files)
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

/// FR-005 / SC-003: a single `waybill sbom scan --format A,B,C`
/// invocation runs the scan pipeline exactly once. The structural
/// signal is the `"scan starting"` log line emitted at
/// `scan_cmd.rs:1413`; we count its occurrences in captured stderr.
#[test]
fn triple_format_invokes_scan_pipeline_exactly_once() {
    let (_guard, image) = build_structural_fixture();
    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg(&image)
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!(
            "cyclonedx-json={}",
            tmp.path().join("out.cdx.json").display()
        ))
        .arg("--output")
        .arg(format!(
            "spdx-2.3-json={}",
            tmp.path().join("out.spdx.json").display()
        ))
        .arg("--output")
        .arg(format!(
            "spdx-3-json={}",
            tmp.path().join("out.spdx3.json").display()
        ))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("waybill runs");
    assert!(
        out.status.success(),
        "triple-format scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let count = stderr.matches("scan starting").count();
    assert_eq!(
        count, 1,
        "triple-format MUST invoke the scan pipeline exactly once \
         (single-pass dispatch). Saw {count} `scan starting` log lines. \
         stderr (last 2k chars):\n{}",
        &stderr[stderr.len().saturating_sub(2048)..]
    );
}

/// FR-005 sanity check: three sequential single-format invocations
/// produce three pipeline starts. Confirms the signal mechanism works
/// in BOTH directions (i.e., the log line still fires when scanning;
/// catches an upstream log-message rename regression that would
/// silently make Test 1 fail-closed without surfacing why).
#[test]
fn three_sequential_invocations_emit_three_pipeline_starts() {
    let (_guard, image) = build_structural_fixture();
    let mut total = 0;
    for fmt in &["cyclonedx-json", "spdx-2.3-json", "spdx-3-json"] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fake_home = tempfile::tempdir().expect("fake-home");
        let mut cmd = Command::new(bin());
        apply_fake_home_env(&mut cmd, fake_home.path());
        cmd.arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--image")
            .arg(&image)
            .arg("--format")
            .arg(fmt)
            .arg("--output")
            .arg(format!(
                "{fmt}={}",
                tmp.path().join("out").display()
            ))
            .arg("--no-deep-hash");
        let out = cmd.output().expect("waybill runs");
        assert!(
            out.status.success(),
            "{fmt} scan failed: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        total += stderr.matches("scan starting").count();
    }
    assert_eq!(
        total, 3,
        "expected 3 `scan starting` log lines across 3 single-format \
         invocations (one per subprocess); got {total}",
    );
}

/// FR-005 dispatch-correctness check: each emitted format in a
/// triple-format invocation byte-matches the output of a separate
/// single-format invocation (after normalization). Catches dispatch
/// regressions that produce wrong output even when single-pass count
/// is correct.
#[test]
fn triple_format_outputs_byte_match_three_sequential() {
    let (_guard, image) = build_structural_fixture();
    let workspace = workspace_root();

    // Triple-format invocation (single subprocess).
    let triple_tmp = tempfile::tempdir().expect("tempdir");
    let triple_home = tempfile::tempdir().expect("fake-home");
    let mut triple_cmd = Command::new(bin());
    apply_fake_home_env(&mut triple_cmd, triple_home.path());
    triple_cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg(&image)
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!(
            "cyclonedx-json={}",
            triple_tmp.path().join("out.cdx.json").display()
        ))
        .arg("--output")
        .arg(format!(
            "spdx-2.3-json={}",
            triple_tmp.path().join("out.spdx.json").display()
        ))
        .arg("--output")
        .arg(format!(
            "spdx-3-json={}",
            triple_tmp.path().join("out.spdx3.json").display()
        ))
        .arg("--no-deep-hash");
    let triple_out = triple_cmd.output().expect("waybill runs");
    assert!(
        triple_out.status.success(),
        "triple-format scan failed: stderr={}",
        String::from_utf8_lossy(&triple_out.stderr)
    );

    // Three sequential single-format invocations (each its own subprocess).
    type FormatRow = (&'static str, &'static str, NormalizeFn);
    let formats: [FormatRow; 3] = [
        ("cyclonedx-json", "out.cdx.json", normalize_cdx_for_golden),
        ("spdx-2.3-json", "out.spdx.json", normalize_spdx23_for_golden),
        ("spdx-3-json", "out.spdx3.json", normalize_spdx3_for_golden),
    ];

    for (fmt, fname, normalize) in formats {
        let single_tmp = tempfile::tempdir().expect("tempdir");
        let single_home = tempfile::tempdir().expect("fake-home");
        let mut single_cmd = Command::new(bin());
        apply_fake_home_env(&mut single_cmd, single_home.path());
        single_cmd
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--image")
            .arg(&image)
            .arg("--format")
            .arg(fmt)
            .arg("--output")
            .arg(format!(
                "{fmt}={}",
                single_tmp.path().join(fname).display()
            ))
            .arg("--no-deep-hash");
        let single_out = single_cmd.output().expect("waybill runs");
        assert!(
            single_out.status.success(),
            "single-format {fmt} scan failed: stderr={}",
            String::from_utf8_lossy(&single_out.stderr)
        );

        let triple_path = triple_tmp.path().join(fname);
        let single_path = single_tmp.path().join(fname);
        let triple_body =
            std::fs::read_to_string(&triple_path).expect("read triple output");
        let single_body =
            std::fs::read_to_string(&single_path).expect("read single output");

        let triple_norm = normalize(&strip_image_temp_dir(&triple_body), &workspace);
        let single_norm = normalize(&strip_image_temp_dir(&single_body), &workspace);

        assert_eq!(
            triple_norm, single_norm,
            "triple-format `{fmt}` output MUST byte-match the single-format \
             `{fmt}` output after normalization. A divergence indicates a \
             dispatch-correctness regression in single-pass emission.",
        );
    }
}

/// Strip the dynamic image-extraction temp-dir suffix from `waybill:source-files`
/// property values. Each `waybill sbom scan --image <tar>` invocation extracts the
/// image to a fresh `tempfile::tempdir()` named `waybill-image-<random>`, so two
/// independent subprocess invocations against the same image produce SBOMs whose
/// `waybill:source-files` properties differ only in that random segment. The
/// per-format `normalize_*_for_golden` helpers don't mask this (they mask only
/// `WAYBILL_FIXTURES_DIR` and the workspace root) — so this helper does a
/// pre-pass regex-replace to collapse `waybill-image-<random>` to a stable
/// placeholder before normalization. Preserves dispatch-correctness signal:
/// component lists, dep edges, PURL bodies, and hashes still compared
/// byte-for-byte; only the per-invocation extraction-path randomness is masked.
fn strip_image_temp_dir(body: &str) -> String {
    // Pattern: `waybill-image-` followed by tempfile's random suffix
    // (typically alphanumeric, up to ~30 chars on Unix). Greedy-match
    // up to the next `/` since the rootfs subpath always follows.
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(idx) = rest.find("waybill-image-") {
        out.push_str(&rest[..idx]);
        out.push_str("waybill-image-PLACEHOLDER");
        // Advance past the random suffix (everything up to the next `/`).
        let after_prefix = &rest[idx + "waybill-image-".len()..];
        if let Some(slash_idx) = after_prefix.find('/') {
            rest = &after_prefix[slash_idx..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out
}
