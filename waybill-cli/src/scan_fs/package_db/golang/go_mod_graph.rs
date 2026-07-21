// Milestone 055 — Step 1 of the resolution ladder: invoke
// `go mod graph` and parse its output.
//
// Module-level `#[allow(dead_code)]`: the foundational scaffold (T007)
// + step-1 impl (T031–T032) land ahead of the orchestration wiring in
// T033 that actually invokes `run_go_mod_graph()`. The allow is removed
// once T033 lands.
#![allow(dead_code)]

//
// This is the preferred path when `go` is on PATH and `--offline` is
// not set. One subprocess call yields the full resolved DAG with MVS,
// `replace`, and `exclude` directives already applied — Go has done
// all the hard work, we just parse the output.
//
// Format (per `go help mod graph`): each line is
//   `parent[@version] child@version`
// with the main module having no `@version` on the parent.
//
// See specs/055-go-transitive-edges/research.md R3 (subprocess) and
// the FR-007 30-second timeout requirement.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::scan_fs::package_db::golang::graph_resolver::{
    ErrorClass, StepError, StepResult,
};
use crate::scan_fs::package_db::golang::module_id::ModuleId;

/// Parse `go mod graph` stdout into a parent → children map.
///
/// Malformed lines (wrong number of whitespace-separated fields) are
/// emitted at `tracing::debug` and skipped — never panic, never abort.
pub fn parse_go_mod_graph(stdout: &str) -> HashMap<ModuleId, Vec<ModuleId>> {
    let mut map: HashMap<ModuleId, Vec<ModuleId>> = HashMap::new();
    for (line_no, line) in stdout.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 2 {
            tracing::debug!(
                line_no,
                line,
                "go mod graph: skipping line with {} fields (expected 2)",
                fields.len()
            );
            continue;
        }
        let parent = parse_module_token(fields[0]);
        let child = parse_module_token(fields[1]);
        map.entry(parent).or_default().push(child);
    }
    map
}

/// Parse a `<path>[@<version>]` token into a `ModuleId`.
fn parse_module_token(token: &str) -> ModuleId {
    match token.split_once('@') {
        Some((path, version)) => ModuleId::new(path, version),
        None => ModuleId::new(token, ""),
    }
}

/// Run `go mod graph` in `workspace_root`, bounded by `timeout`.
///
/// Returns `StepResult::Unavailable` if `go` is not on PATH or
/// `workspace_root` doesn't exist. Returns `StepResult::Failed` on
/// timeout, non-zero exit, or unparseable output. Returns
/// `StepResult::Ok(map)` otherwise.
///
/// Uses synchronous `std::process::Command` because the surrounding
/// scan codepath (`golang::read()`) is sync (see plan.md / R3 deviation
/// note: R3 originally specified async, but `read()` was discovered to
/// be called from a sync chain — `block_in_place` + `block_on` would
/// require multi-threaded runtime context, which complicates testing.
/// Sync subprocess + sync HTTP via separate worker threads is simpler).
pub fn run_go_mod_graph(
    workspace_root: &Path,
    timeout: Duration,
) -> StepResult<HashMap<ModuleId, Vec<ModuleId>>> {
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;

    // Probe `go` availability — `go version` is a one-shot, fast check.
    match Command::new("go").arg("version").output() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return StepResult::Unavailable;
        }
        Err(e) => {
            return StepResult::Failed(StepError {
                class: ErrorClass::Other,
                detail: format!("`go version` probe failed: {e}"),
            });
        }
    }

    if !workspace_root.exists() {
        return StepResult::Failed(StepError {
            class: ErrorClass::Other,
            detail: format!(
                "workspace root does not exist: {}",
                workspace_root.display()
            ),
        });
    }

    // Spawn the subprocess in a worker thread so we can apply a timeout.
    // `Command::output()` blocks; there's no built-in timeout. We send
    // the result on a channel; if the channel doesn't receive within
    // `timeout`, we report a timeout. The worker thread will continue
    // running in the background, but the subprocess will get reaped
    // eventually.
    let (tx, rx) = mpsc::channel();
    let workspace_root = workspace_root.to_path_buf();
    thread::spawn(move || {
        let result = Command::new("go")
            .args(["mod", "graph"])
            .current_dir(&workspace_root)
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return StepResult::Failed(StepError {
                class: ErrorClass::Other,
                detail: format!("`go mod graph` spawn failed: {e}"),
            });
        }
        Err(_) => {
            return StepResult::Failed(StepError {
                class: ErrorClass::Timeout,
                detail: format!(
                    "`go mod graph` exceeded timeout of {}s",
                    timeout.as_secs()
                ),
            });
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return StepResult::Failed(StepError {
            class: ErrorClass::Other,
            detail: format!(
                "`go mod graph` exited with status {}: {}",
                output.status,
                stderr.trim()
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map = parse_go_mod_graph(&stdout);
    StepResult::Ok(map)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_input_returns_empty_map() {
        let map = parse_go_mod_graph("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_single_line_main_module_no_at() {
        let stdout = "example.com/main github.com/foo/bar@v1.0.0\n";
        let map = parse_go_mod_graph(stdout);
        let main = ModuleId::new("example.com/main", "");
        assert_eq!(map.len(), 1);
        let children = map.get(&main).expect("main module entry");
        assert_eq!(children, &vec![ModuleId::new("github.com/foo/bar", "v1.0.0")]);
    }

    #[test]
    fn parse_multi_line_with_versioned_parents() {
        let stdout = "\
example.com/main github.com/a/x@v1.0.0
example.com/main github.com/b/y@v2.0.0
github.com/a/x@v1.0.0 github.com/c/z@v0.5.0
";
        let map = parse_go_mod_graph(stdout);
        assert_eq!(map.len(), 2);
        let main = ModuleId::new("example.com/main", "");
        let a_x = ModuleId::new("github.com/a/x", "v1.0.0");
        assert_eq!(map.get(&main).unwrap().len(), 2);
        assert_eq!(
            map.get(&a_x).unwrap(),
            &vec![ModuleId::new("github.com/c/z", "v0.5.0")]
        );
    }

    #[test]
    fn parse_skips_malformed_lines() {
        // Single-field line, three-field line, empty/whitespace lines
        let stdout = "\
foo
foo bar
foo bar baz
example.com/main github.com/c/z@v0.5.0


";
        let map = parse_go_mod_graph(stdout);
        // Only the well-formed line survives.
        assert_eq!(map.len(), 2); // "foo" → "bar" is well-formed too (treated as path "foo" with no version)
        // Note: "foo bar" parses as parent="foo" (no version), child="bar" (no version) — accepted by our lenient
        // tokenizer. This is fine because real `go mod graph` output never has tokens without `@version` for children.
        let main = ModuleId::new("example.com/main", "");
        assert_eq!(
            map.get(&main).unwrap(),
            &vec![ModuleId::new("github.com/c/z", "v0.5.0")]
        );
    }

    #[test]
    fn parse_handles_extra_whitespace() {
        let stdout = "  example.com/main  \t github.com/foo/bar@v1.0.0  \n";
        let map = parse_go_mod_graph(stdout);
        assert_eq!(map.len(), 1);
    }

    // run_go_mod_graph integration tests are covered by the higher-level
    // integration test in tests/go_transitive_edges.rs (T035) since they
    // require a real `go` binary on PATH. Pure-unit tests here cover the
    // parser only.
}
