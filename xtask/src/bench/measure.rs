// milestone 669 - see specs/669-bench-harness/plan.md
// Per-invocation wall-clock + peak-RSS + output-size measurement.
//
// T019: measure_one() spawns a Command, polls sysinfo::Process::memory()
//       at ~10 Hz on a background thread, enforces a per-invocation
//       timeout (Q3 default 5 min; caller-configurable).
// T020: unit tests over measure_one (sleep 0.5 → Success ~500ms; sleep 10
//       with 1s timeout → Timeout).
// T021: parse_output_metadata() reads an emitted CDX file and returns
//       (output_bytes, component_count) via serde_json.
//
// sysinfo 0.39.6 API pattern (verified from ~/.cargo/registry source):
//   let mut sys = System::new();
//   sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
//   sys.process(pid).map(|p| p.memory())  // returns BYTES (not KB)
// Sample.max_rss_kb is bytes/1024.

use std::error::Error;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::bench::schema::ExitStatus;

/// Result of one measured child-process invocation. Populated by
/// [`measure_one`]. The runner (T022) collects 6 of these per fixture-
/// mode (1 warmup + 5 timed) and derives the `BenchResult` from the
/// timed 5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// Wall-clock duration from spawn to child exit (or timeout kill),
    /// in milliseconds. Derived from `Instant::now()` diffs, not from
    /// child-reported timestamps.
    pub wall_clock_ms: u64,
    /// Peak resident-set size observed via the sysinfo memory-polling
    /// thread, in kilobytes.
    pub max_rss_kb: u64,
    /// Byte count of the child's captured stdout. Note: waybill scans
    /// write their SBOM output to `--output <path>` files, not stdout,
    /// so this is usually near zero. The BenchResult's `output_bytes`
    /// dimension is populated separately from disk-file sizes by
    /// [`parse_output_metadata`].
    pub output_bytes: u64,
    /// Terminal state of the child process.
    pub exit_status: ExitStatus,
}

/// Post-run metadata derived from an emitted SBOM output file.
/// Returned by [`parse_output_metadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputMeta {
    /// Size of the SBOM file on disk, in bytes.
    pub output_bytes: u64,
    /// `.components` array length in the CycloneDX JSON. Absent
    /// `components` key is treated as zero, not an error.
    pub component_count: u64,
}

/// Spawn `cmd` as a child, measure wall-clock + peak RSS + stdout
/// bytes, kill the child if it exceeds `timeout`. Returns a
/// [`Sample`] populated per contract data-model.md §2 dimensions.
///
/// Ownership of `cmd` is consumed because `Command::spawn` requires
/// `&mut self` and each measurement wants a fresh child; the caller
/// (T022) rebuilds the Command between warmup + 5 timed passes.
pub fn measure_one(
    mut cmd: Command,
    timeout: Duration,
) -> Result<Sample, Box<dyn Error>> {
    // Pipe stdio so the child can't spam our terminal + we can measure
    // stdout bytes. Reader threads drain the pipes concurrently so the
    // child can't block on a full pipe buffer.
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let start = Instant::now();
    let mut child = cmd.spawn()?;
    let pid = child.id();

    // Memory-polling thread: refreshes just this child's PID at
    // ~10 Hz (100ms sleep between snapshots) and tracks the maximum
    // RSS observed. Stops on stop_tx signal + returns max in bytes.
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let mem_thread = thread::spawn(move || {
        let mut sys = System::new();
        let mut max_rss_bytes: u64 = 0;
        let pid_p = Pid::from_u32(pid);
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            sys.refresh_processes(ProcessesToUpdate::Some(&[pid_p]), true);
            if let Some(proc) = sys.process(pid_p) {
                let rss = proc.memory();
                if rss > max_rss_bytes {
                    max_rss_bytes = rss;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        max_rss_bytes
    });

    // Concurrent stdout/stderr drainers so the child can't block on
    // full pipe buffers (SBOM output can be MBs; even 64 KiB pipe
    // buffers fill up in ms).
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut p) = stdout_pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut p) = stderr_pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    // Wait loop: polls try_wait() every 50ms, kills the child on
    // timeout expiry. Kill semantics: `SIGKILL` on Unix, terminates on
    // Windows — no cleanup grace period, matches spec Q3 intent
    // (per-fixture 5-min hard cap).
    let terminal_status: Option<std::process::ExitStatus> = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait(); // reap zombie
                    break None;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    };

    let wall_clock_ms = start.elapsed().as_millis() as u64;

    // Stop + join memory poller.
    let _ = stop_tx.send(());
    let max_rss_bytes = mem_thread.join().unwrap_or(0);
    let max_rss_kb = max_rss_bytes / 1024;

    // Join drainers (they exit naturally when the pipes close on
    // child exit or kill).
    let stdout_bytes_vec = stdout_thread.join().unwrap_or_default();
    let _ = stderr_thread.join(); // stderr size not part of Sample surface

    let output_bytes = stdout_bytes_vec.len() as u64;

    let exit_status = match terminal_status {
        None => ExitStatus::Timeout,
        Some(s) if s.success() => ExitStatus::Success,
        Some(_) => ExitStatus::NonZeroExitCode,
    };

    Ok(Sample {
        wall_clock_ms,
        max_rss_kb,
        output_bytes,
        exit_status,
    })
}

/// Read an emitted CycloneDX JSON at `cdx_path` and return its byte
/// size + `.components` array length. A missing/empty `components`
/// key is treated as zero components (not an error) — waybill emits
/// documents without components for empty scans.
///
/// The runner (T022) uses this to populate the `BenchResult`'s
/// `output_bytes` + `component_count` fields. For triple-format
/// modes, the runner calls this once per emitted format file and
/// sums the byte totals; the component-count is the CDX file's,
/// since SPDX 2.3 / SPDX 3 emit the same component set with format-
/// specific shape wrappers.
pub fn parse_output_metadata(cdx_path: &Path) -> Result<OutputMeta, Box<dyn Error>> {
    let bytes = std::fs::read(cdx_path)?;
    let output_bytes = bytes.len() as u64;
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    let component_count = doc
        .get("components")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    Ok(OutputMeta {
        output_bytes,
        component_count,
    })
}

// ────────────────────────────────────────────────────────────────
// T020 + T021 tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ────────────────────────────────────────────────────────────
    // T020 — measure_one wall-clock + timeout behavior
    // ────────────────────────────────────────────────────────────
    //
    // Gated to Unix: the tests spawn `/bin/sleep`, which doesn't
    // exist on Windows. Windows-equivalent coverage lands separately
    // if/when the m100 windows-host CI lane runs xtask benches.

    #[cfg(unix)]
    #[test]
    fn measure_one_records_wall_clock_and_success_on_short_sleep() {
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("0.5");
        let s = measure_one(cmd, Duration::from_secs(30)).unwrap();
        assert_eq!(s.exit_status, ExitStatus::Success);
        // Allow generous bounds: process spawn + reap overhead adds
        // jitter (esp. on loaded macOS runners per the memory-note
        // about Spotlight bloating _dyld_start).
        assert!(
            s.wall_clock_ms >= 480,
            "wall_clock_ms={} < 480ms floor",
            s.wall_clock_ms
        );
        assert!(
            s.wall_clock_ms <= 2000,
            "wall_clock_ms={} > 2s ceiling (host too loaded?)",
            s.wall_clock_ms
        );
    }

    #[cfg(unix)]
    #[test]
    fn measure_one_returns_timeout_when_child_exceeds_deadline() {
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("10");
        let s = measure_one(cmd, Duration::from_secs(1)).unwrap();
        assert_eq!(s.exit_status, ExitStatus::Timeout);
        // Wall-clock lands near the timeout (~1s) with some slack for
        // the poll interval + kill delivery.
        assert!(
            s.wall_clock_ms >= 950,
            "wall_clock_ms={} < 950ms (timeout should be ~1s)",
            s.wall_clock_ms
        );
        assert!(
            s.wall_clock_ms <= 2000,
            "wall_clock_ms={} > 2s (kill should be prompt)",
            s.wall_clock_ms
        );
    }

    #[cfg(unix)]
    #[test]
    fn measure_one_records_nonzero_exit_code() {
        // `/bin/sh -c 'exit 7'` gives a deterministic non-zero exit.
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg("exit 7");
        let s = measure_one(cmd, Duration::from_secs(30)).unwrap();
        assert_eq!(s.exit_status, ExitStatus::NonZeroExitCode);
    }

    #[cfg(unix)]
    #[test]
    fn measure_one_captures_stdout_bytes() {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg("printf 'hello-fixture'");
        let s = measure_one(cmd, Duration::from_secs(30)).unwrap();
        assert_eq!(s.exit_status, ExitStatus::Success);
        assert_eq!(s.output_bytes, "hello-fixture".len() as u64);
    }

    // ────────────────────────────────────────────────────────────
    // T021 — parse_output_metadata over a CDX JSON file
    // ────────────────────────────────────────────────────────────

    fn write_json(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn parse_output_metadata_counts_components_and_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let body = r#"{"bomFormat":"CycloneDX","specVersion":"1.5","components":[
            {"type":"library","name":"waybill-fixture-a","version":"1.0.0"},
            {"type":"library","name":"waybill-fixture-b","version":"2.0.0"},
            {"type":"library","name":"waybill-fixture-c","version":"3.0.0"}
        ]}"#;
        let p = write_json(tmp.path(), "out.cdx.json", body);
        let meta = parse_output_metadata(&p).unwrap();
        assert_eq!(meta.component_count, 3);
        assert_eq!(meta.output_bytes, body.len() as u64);
    }

    #[test]
    fn parse_output_metadata_treats_missing_components_as_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let body = r#"{"bomFormat":"CycloneDX","specVersion":"1.5"}"#;
        let p = write_json(tmp.path(), "empty.cdx.json", body);
        let meta = parse_output_metadata(&p).unwrap();
        assert_eq!(meta.component_count, 0);
        assert_eq!(meta.output_bytes, body.len() as u64);
    }

    #[test]
    fn parse_output_metadata_treats_null_components_as_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let body = r#"{"components":null}"#;
        let p = write_json(tmp.path(), "null.cdx.json", body);
        let meta = parse_output_metadata(&p).unwrap();
        assert_eq!(meta.component_count, 0);
    }

    #[test]
    fn parse_output_metadata_rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("does-not-exist.json");
        assert!(parse_output_metadata(&p).is_err());
    }

    #[test]
    fn parse_output_metadata_rejects_malformed_json() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write_json(tmp.path(), "bad.json", "{not-valid-json");
        assert!(parse_output_metadata(&p).is_err());
    }
}
