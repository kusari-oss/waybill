//! Milestone 118 (#343 / FR-008) — `waybill sbom scan --help` documents
//! `--exclude-path` and points operators at the user-guide CLI reference.

use std::process::Command;

#[test]
fn help_text_documents_exclude_path() {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let output = Command::new(bin)
        .arg("sbom")
        .arg("scan")
        .arg("--help")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "--help must exit zero; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("--help stdout must be UTF-8");
    assert!(
        stdout.contains("--exclude-path"),
        "--help must mention --exclude-path; got:\n{stdout}"
    );
    // The discoverability anchor — operators reading --help can follow
    // the pointer into the user-guide CLI reference for the full
    // troubleshooting matrix. FR-008 promises the SUBSTRING, not exact
    // wording — operators can rewrite the flag description without
    // breaking this test.
    assert!(
        stdout.contains("cli-reference"),
        "--help must point at cli-reference; got:\n{stdout}"
    );
}
