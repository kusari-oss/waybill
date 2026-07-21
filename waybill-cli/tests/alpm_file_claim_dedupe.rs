//! Milestone 135 US3 — binary walker skips pacman-claimed file paths,
//! preventing duplicate `pkg:generic/<binary>` emission alongside the
//! `pkg:alpm/<distro>/<binary>` component.
//!
//! Covers SC-004 and US3 acceptance scenarios.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn write_pkg(rootfs: &Path, dir_name: &str, desc_body: &str, files_body: Option<&str>) {
    let pkg_dir = rootfs.join("var/lib/pacman/local").join(dir_name);
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(pkg_dir.join("desc"), desc_body).unwrap();
    if let Some(files) = files_body {
        std::fs::write(pkg_dir.join("files"), files).unwrap();
    }
}

/// Write a minimal valid ELF64 file at `path`. The waybill binary
/// walker only inspects the first ~52 bytes for the ELF header; an
/// empty body after the header is sufficient to make the walker emit
/// a file-level binary component for unclaimed paths.
fn write_minimal_elf(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    // ELF64 little-endian header (64 bytes):
    let mut bytes = vec![0u8; 64];
    bytes[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    bytes[4] = 2; // EI_CLASS = ELFCLASS64
    bytes[5] = 1; // EI_DATA = ELFDATA2LSB
    bytes[6] = 1; // EI_VERSION = EV_CURRENT
    bytes[16] = 2; // e_type = ET_EXEC
    bytes[17] = 0;
    bytes[18] = 0x3e; // e_machine = EM_X86_64
    bytes[19] = 0;
    bytes[20] = 1; // e_version = EV_CURRENT
    bytes[21] = 0;
    bytes[22] = 0;
    bytes[23] = 0;
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

fn run_scan(rootfs: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(rootfs)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn purls_for_basename(doc: &Value, name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(components) = doc.get("components").and_then(|v| v.as_array()) {
        for c in components {
            if c.get("name").and_then(|v| v.as_str()) == Some(name) {
                if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
                    out.push(p.to_string());
                }
            }
        }
    }
    out
}

#[test]
fn pacman_owned_binary_emits_one_component_no_generic_duplicate() {
    // SC-004 — a binary owned by a pacman package emits exactly one
    // component (the alpm one) — no `pkg:generic/bash` duplicate.
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
        Some("%FILES%\nusr/\nusr/bin/\nusr/bin/bash\n"),
    );
    write_minimal_elf(&tmp.path().join("usr/bin/bash"));

    let doc = run_scan(tmp.path());
    let bash_purls = purls_for_basename(&doc, "bash");
    let alpm_count = bash_purls
        .iter()
        .filter(|p| p.starts_with("pkg:alpm/"))
        .count();
    let generic_count = bash_purls
        .iter()
        .filter(|p| p.starts_with("pkg:generic/"))
        .count();
    assert_eq!(alpm_count, 1, "expected one alpm bash, got {bash_purls:?}");
    assert_eq!(
        generic_count, 0,
        "binary walker MUST NOT emit pkg:generic/bash for pacman-owned path; got {bash_purls:?}",
    );
}

#[test]
fn unclaimed_binary_still_surfaces_via_walker() {
    // US3 acceptance scenario 2 — file-claim only suppresses claimed
    // paths. An ELF at an unclaimed path continues to emit via the
    // generic-binary walker per the milestone-004 behavior.
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(
        tmp.path(),
        "bash-5.2.026-1",
        "%NAME%\nbash\n\n%VERSION%\n5.2.026-1\n\n%ARCH%\nx86_64\n",
        Some("%FILES%\nusr/bin/bash\n"),
    );
    write_minimal_elf(&tmp.path().join("usr/bin/bash"));
    // Unclaimed custom binary at /opt/custom-tool — no pacman package
    // declares ownership.
    write_minimal_elf(&tmp.path().join("opt/custom-tool"));

    let doc = run_scan(tmp.path());

    // bash: 1 component (alpm) — no generic duplicate.
    let bash_purls = purls_for_basename(&doc, "bash");
    let bash_generic = bash_purls
        .iter()
        .filter(|p| p.starts_with("pkg:generic/"))
        .count();
    assert_eq!(bash_generic, 0, "pacman-owned bash must not emit as generic");

    // custom-tool: SOME component must surface (the walker may name it
    // via path basename or content-sha256; we just require it appears).
    // Look for any component whose source files mention "custom-tool".
    let components = doc.get("components").and_then(|v| v.as_array()).unwrap();
    let custom_tool_seen = components.iter().any(|c| {
        let name_match =
            c.get("name").and_then(|v| v.as_str()) == Some("custom-tool");
        let path_match = c
            .get("properties")
            .and_then(|v| v.as_array())
            .map(|props| {
                props.iter().any(|p| {
                    let is_paths_prop = matches!(
                        p.get("name").and_then(|v| v.as_str()),
                        Some("waybill:file-paths") | Some("waybill:source-files")
                    );
                    let v = p.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    is_paths_prop && v.contains("custom-tool")
                })
            })
            .unwrap_or(false);
        name_match || path_match
    });
    assert!(
        custom_tool_seen,
        "unclaimed /opt/custom-tool must surface via the walker (file-claim only suppresses claimed paths)",
    );
}
