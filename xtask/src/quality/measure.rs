// milestone 770 — T012: the timed waybill invocation.
//
// Contract xtask-quality-cli.md § C-4. Only this subprocess is timed
// (FR-009) — fetch, scoring and analysis are all excluded, which is what
// makes wall time attributable to waybill rather than to the network.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::quality::config::Target;

pub enum ScanOutcome {
    Ok { wall_ms: u64, document: PathBuf },
    Failed { detail: String },
    TimedOut { budget_secs: u64 },
}

/// Run `waybill --offline sbom scan` against `checkout`, writing the CDX
/// document into `out_dir`. Returns the wall time of the scan only.
pub fn scan(
    waybill_bin: &Path,
    target: &Target,
    checkout: &Path,
    out_dir: &Path,
    isolated_home: &Path,
    timeout_secs: u64,
) -> ScanOutcome {
    let doc = out_dir.join(format!("{}.cdx.json", target.name));
    let mut cmd = Command::new(waybill_bin);
    // C-4.1: --offline is a GLOBAL flag and must precede `sbom scan`.
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(checkout)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", doc.display()))
        .arg("--root-name")
        .arg(target.name.as_str())
        .arg("--root-version")
        .arg(target.pin.short())
        // C-4.4: give every scan an empty home directory so measurements
        // describe the repository under test and not the host's caches.
        //
        // ALL THREE are required for Go: waybill's module-cache discovery in
        // golang/graph_resolver.rs falls back $GOMODCACHE -> $GOPATH ->
        // $HOME/go/pkg/mod, so pinning only the first lets a warm host cache
        // leak in through the others. Observed 2026-09-11: a developer
        // machine with a 16 GB module cache measured go-cobra at depth 2 and
        // go-kubernetes at 1752 edges, while a clean CI runner produced flat
        // graphs (depth 1, 490 edges) from the identical commit.
        //
        // Redirecting HOME is BROADER than Go. It also removes m663's
        // local-cache-probe resolver (~/.m2, ~/.cargo, ...) and m108's
        // fingerprint cache under ~/.cache/waybill/. That is intentional —
        // a regression detector must not read host state — but it means
        // these measurements are a COLD-CACHE FLOOR for every ecosystem,
        // not just Go. Verified 2026-09-11 across all 18 corpus targets:
        // only the two Go targets moved (5 of 108 metrics).
        //
        // Note the three variables do NOT address the same directory:
        // GOPATH=<d> makes Go look in <d>/pkg/mod, and HOME=<d> means
        // <d>/go/pkg/mod — neither is <d> itself. Harmless while all three
        // are empty, but a trap if anyone later pre-populates this scratch
        // dir expecting all three to see it.
        .env("GOMODCACHE", isolated_home)
        .env("GOPATH", isolated_home)
        .env("HOME", isolated_home)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // C-4.3: no tier filter, no --file-inventory override. The corpus
    // measures waybill as an ordinary user invokes it.

    let start = Instant::now();
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ScanOutcome::Failed { detail: format!("spawn failed: {e}") },
    };

    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let r = child.wait_with_output();
        let _ = tx.send(r);
    });

    let budget = Duration::from_secs(timeout_secs);
    match rx.recv_timeout(budget) {
        Ok(Ok(out)) => {
            let wall_ms = start.elapsed().as_millis() as u64;
            let _ = handle.join();
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let detail = stderr
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("(no stderr)")
                    .chars()
                    .take(200)
                    .collect();
                return ScanOutcome::Failed { detail };
            }
            if !doc.exists() {
                return ScanOutcome::Failed { detail: "scan exited 0 but emitted no document".into() };
            }
            ScanOutcome::Ok { wall_ms, document: doc }
        }
        Ok(Err(e)) => ScanOutcome::Failed { detail: format!("wait failed: {e}") },
        Err(_) => {
            // Timed out. The child is orphaned deliberately rather than
            // blocking the run; the OS reaps it. Recording the budget is
            // what the operator needs (FR-014).
            ScanOutcome::TimedOut { budget_secs: timeout_secs }
        }
    }
}
