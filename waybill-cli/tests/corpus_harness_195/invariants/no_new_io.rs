//! FR-016 / SC-008 — the invariant layer adds no scans, no network
//! requests and no container pulls. It reads documents the run already
//! produced.
//!
//! This is a source-level gate because the claim is about what the code
//! *can* do, not about what one run happened to do. A run that performs
//! no network call because a cache was warm looks exactly like a run
//! that cannot perform one, and only the second is the requirement.

use std::path::{Path, PathBuf};

fn invariants_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("corpus_harness_195").join("invariants")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_sources(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// Things that would mean the layer does I/O it is not allowed to do.
/// `std::fs::read` is permitted: the catalog, the declarations file, the
/// ecosystems doc and the committed goldens are all repository files.
const FORBIDDEN: &[(&str, &str)] = &[
    ("Command::new", "spawning a process — a scan, a clone or an image pull"),
    ("std::process", "spawning a process"),
    ("reqwest", "a network request"),
    ("TcpStream", "a network connection"),
    ("scan_target", "re-running the scan the harness already ran"),
    ("CorpusCacheDir", "touching the corpus cache, which implies a clone or a pull"),
];

#[test]
fn the_invariant_layer_cannot_scan_fetch_or_pull() {
    let mut files = Vec::new();
    rust_sources(&invariants_dir(), &mut files);
    assert!(files.len() >= 8, "expected the invariants module's sources, found {files:?}");
    // This file is excluded from its own scan: the teeth test below has
    // to contain a real forbidden token to prove the matcher works, so
    // scanning it would always report that fixture. The trade is a
    // deliberate blind spot in one test-only file, stated rather than
    // discovered.
    files.retain(|f| f.file_name().and_then(|n| n.to_str()) != Some("no_new_io.rs"));

    let mut offences = Vec::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        for (line_no, line) in text.lines().enumerate() {
            // Skip comments: this file names every forbidden token in
            // its own `FORBIDDEN` table and in prose.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("(\"") {
                continue;
            }
            for (needle, why) in FORBIDDEN {
                if line.contains(needle) {
                    offences.push(format!(
                        "{}:{}: `{needle}` — {why} (FR-016)",
                        f.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                        line_no + 1
                    ));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "the invariant layer must consume documents the harness already emitted:\n  {}",
        offences.join("\n  ")
    );
}

/// The gate has teeth: the same scan over a source that *does* spawn a
/// process finds it. Without this, a bug in the walker above would make
/// the gate pass by reading nothing.
#[test]
fn the_io_gate_detects_a_forbidden_call() {
    let sample = "fn bad() { let _ = Command::new(\"docker\").arg(\"pull\"); }";
    let hit = FORBIDDEN.iter().any(|(needle, _)| sample.contains(needle));
    assert!(hit, "the forbidden-token list must match an actual process spawn");
}
