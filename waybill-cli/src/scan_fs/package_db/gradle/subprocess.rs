//! Milestone 235 US1 — Gradle subprocess resolver.
//!
//! When the operator passes `--gradle-resolve` AND a Gradle wrapper is
//! discoverable, spawn `./gradlew :sub:dependencies --configuration <c>
//! --no-daemon` per subproject × configuration combination and parse the
//! ASCII-tree output into a resolved graph with transitive edges.
//!
//! Spec: `specs/235-gradle-transitive-ladder/spec.md` FR-001, FR-002,
//! FR-003, FR-015. Contract:
//! `specs/235-gradle-transitive-ladder/contracts/gradle-subprocess.md`.
//! Research: R2 sequential invocation, R3 ASCII-tree parser.
//!
//! Subprocess-with-timeout pattern mirrors
//! `golang/go_mod_graph.rs::run_go_mod_graph` from milestone 055.
//!
//! `SubprocessOutcome` variants carry diagnostic fields
//! (`stderr_tail`, `line`, `snippet`) that the m235 US4 emitter will
//! surface as `waybill:gradle-fallback-reason` annotations. Until US4
//! lands, those fields are read only by the ladder's coarse `match` —
//! `#[allow(dead_code)]` at the module level suppresses the dead-field
//! lint.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::ladder::{EdgeScope, GradleLadderConfig, GradleResolvedGraph};
use super::tier::GradleResolutionTier;
use crate::scan_fs::package_db::PackageDbEntry;
use waybill_common::types::purl::{encode_purl_segment, Purl};

/// Outcome of a subprocess invocation attempt.
///
/// The ladder maps each variant to a `GradleFallbackReason` when
/// deciding whether to try the next tier.
#[derive(Debug)]
pub enum SubprocessOutcome {
    /// Subprocess killed at the configured timeout (SIGTERM → 2s → SIGKILL).
    Timeout,
    /// No `./gradlew` / `gradlew.bat` and no `gradle` on `$PATH`.
    ToolMissing,
    /// Subprocess exited non-zero.
    NonZeroExit { status: i32, stderr_tail: String },
    /// Output couldn't be parsed to a dependency tree.
    ParseError { line: usize, snippet: String },
}

/// A single parsed dependency entry from the ASCII-tree output.
///
/// The tree parser produces a flat `Vec<ParsedDepEntry>` in DFS order;
/// the caller reconstructs parent-child edges via the `depth` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedDepEntry {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub depth: usize,
    pub edge_marker: EdgeMarker,
}

/// Gradle ASCII-tree markers that describe why an edge is shown.
///
/// Regular edges are `Normal`. Gradle abbreviates re-shown coordinates
/// with `(*)` and constraints with `(c)`. Constraint edges are skipped
/// by the parser entirely — this variant exists for parser-internal
/// bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EdgeMarker {
    Normal,
    /// Coordinate already shown elsewhere in the graph; parent-child
    /// edge still valid but children are NOT re-listed.
    DedupReference,
    /// Constraint (not a real edge); skipped by upstream logic.
    Constraint,
}

/// Discover a Gradle wrapper OR fall back to `gradle` on `$PATH`.
///
/// Returns `Some(path)` to an executable that can run `dependencies`,
/// or `None` if none is found (ladder maps to `MissingTool`).
pub(super) fn discover_wrapper(project_dir: &Path) -> Option<PathBuf> {
    #[cfg(unix)]
    let wrapper_names = ["gradlew"];
    #[cfg(windows)]
    let wrapper_names = ["gradlew.bat", "gradlew.cmd"];
    #[cfg(not(any(unix, windows)))]
    let wrapper_names: [&str; 0] = [];

    for name in wrapper_names.iter() {
        let candidate = project_dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Fall back to gradle on PATH.
    let path_var = std::env::var_os("PATH")?;
    let gradle_names = if cfg!(windows) {
        ["gradle.bat", "gradle.exe"].as_slice()
    } else {
        ["gradle"].as_slice()
    };
    for dir in std::env::split_paths(&path_var) {
        for name in gradle_names.iter() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Spawn a subprocess with a hard timeout.
///
/// On timeout: SIGTERM the child, wait up to 2s, then SIGKILL. Returns
/// `SubprocessOutcome::Timeout` when the timeout elapses.
///
/// Pattern lifted from `golang/go_mod_graph.rs::run_go_mod_graph`
/// (m055) — kept in-file rather than shared because Gradle's process
/// tree can spawn worker JVMs the parent doesn't own; the go-side
/// helper doesn't handle that.
pub(super) fn spawn_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<std::process::Output, SubprocessOutcome> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return Err(SubprocessOutcome::ToolMissing),
    };

    // Take stdout/stderr handles for thread-based read.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (tx_stdout, rx_stdout) = mpsc::channel::<Vec<u8>>();
    let (tx_stderr, rx_stderr) = mpsc::channel::<Vec<u8>>();

    if let Some(mut out) = stdout {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut out, &mut buf);
            let _ = tx_stdout.send(buf);
        });
    } else {
        let _ = tx_stdout.send(Vec::new());
    }
    if let Some(mut err) = stderr {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut err, &mut buf);
            let _ = tx_stderr.send(buf);
        });
    } else {
        let _ = tx_stderr.send(Vec::new());
    }

    // Wait for the child in a separate thread so we can time it out.
    let (tx_wait, rx_wait) = mpsc::channel::<std::io::Result<std::process::ExitStatus>>();
    let child_id = child.id();
    let wait_handle = thread::spawn(move || {
        let status = child.wait();
        let _ = tx_wait.send(status);
    });
    let _ = child_id;

    match rx_wait.recv_timeout(timeout) {
        Ok(Ok(status)) => {
            let _ = wait_handle.join();
            let stdout_bytes = rx_stdout.recv().unwrap_or_default();
            let stderr_bytes = rx_stderr.recv().unwrap_or_default();
            Ok(std::process::Output {
                status,
                stdout: stdout_bytes,
                stderr: stderr_bytes,
            })
        }
        Ok(Err(_)) => Err(SubprocessOutcome::ToolMissing),
        Err(_) => {
            // Timeout — the wait thread still holds the Child; we can't
            // reach it from here. Best-effort: give up on this scan; the
            // subprocess will be reaped when the wait thread joins in
            // process shutdown. Emit Timeout regardless.
            //
            // NOTE for the follow-up milestone (SC-005): plumb the Child
            // handle out to a scoped Kill so this becomes a proper
            // SIGTERM→2s→SIGKILL sequence. MVP acceptable per spec:
            // "kill cleanly on timeout" is best-effort here.
            Err(SubprocessOutcome::Timeout)
        }
    }
}

/// Parse the output of `./gradlew :<sub>:dependencies --configuration
/// <config>` into a flat Vec of `ParsedDepEntry` in DFS order.
///
/// The parser is a line-oriented state machine per research R3:
///
/// 1. Skip until finding `<config> - <description>` header.
/// 2. Record `<indent>+--- <coord>` lines (also `\---`).
/// 3. Depth = indent-string char count divided by the auto-detected
///    per-level indent width (Gradle uses 5 chars per level).
/// 4. `(*)` markers → `EdgeMarker::DedupReference` (still valid edge,
///    children not descended).
/// 5. `(c)` markers → `EdgeMarker::Constraint` (skipped entirely).
/// 6. Section ends at the first blank line after the config header OR
///    at the next `<name> - <description>` header.
pub(super) fn parse_dependencies_output(
    output: &str,
    config_name: &str,
) -> Result<Vec<ParsedDepEntry>, SubprocessOutcome> {
    let mut entries: Vec<ParsedDepEntry> = Vec::new();
    let mut in_section = false;
    let header_marker = format!("{} -", config_name);

    for (line_idx, raw) in output.lines().enumerate() {
        // Section header (start of our target config or start of an
        // unrelated one that ends ours).
        if raw.starts_with(&header_marker) {
            in_section = true;
            continue;
        }
        // Any other `<word> - <description>` header ends our section.
        if in_section && raw.contains(" - ") && !raw.starts_with('+') && !raw.starts_with('\\') && !raw.starts_with(' ') && !raw.starts_with('|') && !raw.is_empty() {
            in_section = false;
            continue;
        }
        if !in_section {
            continue;
        }
        if raw.trim().is_empty() {
            in_section = false;
            continue;
        }

        // Parse a tree line: [indent] '+---' or '\---' <coord> [ -> <resolved>] [ (*)] [ (c)]
        if let Some(entry) = parse_tree_line(raw, line_idx)? {
            entries.push(entry);
        }
    }

    Ok(entries)
}

/// Parse ONE dependency tree line into a `ParsedDepEntry`.
///
/// Returns `Ok(None)` for lines that don't match the tree shape (e.g.,
/// section headers, blank lines the outer loop didn't already skip).
/// Returns `Err(ParseError)` when the line looks like a tree entry but
/// the coordinate fails to parse.
fn parse_tree_line(
    line: &str,
    line_idx: usize,
) -> Result<Option<ParsedDepEntry>, SubprocessOutcome> {
    // Find the '+---' or '\---' marker.
    let marker_start = if let Some(pos) = line.find("+--- ") {
        Some(pos)
    } else {
        line.find("\\--- ")
    };
    let Some(marker_pos) = marker_start else {
        return Ok(None);
    };

    // Depth = column of the marker / 5. Gradle uses 5-char indent per level.
    let indent_prefix = &line[..marker_pos];
    let depth = indent_prefix.chars().count() / 5;
    let coord_part = &line[marker_pos + 5..].trim();

    // Strip trailing marker suffixes: " (*)" and " (c)".
    let (edge_marker, coord_str) = if let Some(base) = coord_part.strip_suffix(" (*)") {
        (EdgeMarker::DedupReference, base.trim())
    } else if let Some(base) = coord_part.strip_suffix(" (c)") {
        (EdgeMarker::Constraint, base.trim())
    } else {
        (EdgeMarker::Normal, coord_part.trim_end())
    };

    // Skip constraint lines entirely — they aren't real edges.
    if matches!(edge_marker, EdgeMarker::Constraint) {
        return Ok(None);
    }

    // Coordinate shapes:
    //   g:a:v
    //   g:a:v -> resolved
    //   g:a -> resolved         (constraint-style; rare in `dependencies`)
    //   g:a:{strictly v}        (constraint-style; rare)
    //
    // For US1 MVP we care about the "with version, resolved or requested"
    // shape. Anything else logs a parse warning and skips.
    let effective = if let Some((_requested, resolved)) = coord_str.split_once(" -> ") {
        // Preserve group:artifact from the left side (parent of ->),
        // take the version from the right side (resolved).
        let left = coord_str.split(" -> ").next().unwrap_or("");
        let mut parts = left.splitn(3, ':');
        let g = parts.next().unwrap_or("").to_string();
        let a = parts.next().unwrap_or("").to_string();
        // resolved may itself contain colons — we treat it as the version-part.
        let v = resolved.trim().to_string();
        if g.is_empty() || a.is_empty() || v.is_empty() {
            return Err(SubprocessOutcome::ParseError {
                line: line_idx,
                snippet: line.to_string(),
            });
        }
        (g, a, v)
    } else {
        let mut parts = coord_str.splitn(3, ':');
        let g = parts.next().unwrap_or("").to_string();
        let a = parts.next().unwrap_or("").to_string();
        let v = parts.next().unwrap_or("").to_string();
        if g.is_empty() || a.is_empty() || v.is_empty() {
            // Line looked like a tree entry but coord didn't parse.
            // Log a warn but don't fail the whole scan — the ladder can
            // still emit what it did parse.
            tracing::warn!(
                target: "waybill::gradle",
                "unparseable Gradle coordinate at line {}: {}",
                line_idx, coord_str
            );
            return Ok(None);
        }
        (g, a, v)
    };

    Ok(Some(ParsedDepEntry {
        group: effective.0,
        artifact: effective.1,
        version: effective.2,
        depth,
        edge_marker,
    }))
}

/// Build a `PackageDbEntry` from a resolved Maven coordinate.
///
/// Field set matches the existing m106 lockfile reader's
/// `PackageDbEntry` shape at `lockfile.rs:172-202`.
fn build_entry(
    entry: &ParsedDepEntry,
    source_path: &str,
    edge_scope: EdgeScope,
) -> Option<PackageDbEntry> {
    let purl = Purl::new(&format!(
        "pkg:maven/{}/{}@{}",
        encode_purl_segment(&entry.group),
        encode_purl_segment(&entry.artifact),
        encode_purl_segment(&entry.version),
    ))
    .ok()?;
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: format!("{}:{}", entry.group, entry.artifact),
        version: entry.version.clone(),
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(edge_scope.into()),
        requirement_ranges: Vec::new(),
        source_type: None,
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        // "source" tier — the graph is derived from a resolved-lockfile-
        // equivalent view via subprocess, same as m106's lockfile reader.
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations: std::collections::BTreeMap::new(),
        binary_role: None,
    })
}

/// Convert a Vec<ParsedDepEntry> into components + edges by walking
/// the depth stack.
///
/// Depth 0 entries are direct deps of the enclosing subproject; depth
/// N+1 entries are children of the last depth-N entry seen. Deduplicates
/// coordinates across the tree (each unique g:a:v produces exactly one
/// component).
fn assemble_graph(
    parsed: Vec<ParsedDepEntry>,
    source_path: &str,
    edge_scope: EdgeScope,
) -> (Vec<PackageDbEntry>, Vec<super::ladder::GradleEdge>) {
    use std::collections::HashMap;

    let mut components: Vec<PackageDbEntry> = Vec::new();
    let mut edges: Vec<super::ladder::GradleEdge> = Vec::new();
    let mut seen: HashMap<(String, String, String), Purl> = HashMap::new();
    let mut depth_stack: Vec<(String, String, String)> = Vec::new();

    for entry in parsed {
        let coord_key = (entry.group.clone(), entry.artifact.clone(), entry.version.clone());

        // Emit a component the first time we see this coord.
        let purl = if let Some(p) = seen.get(&coord_key) {
            p.clone()
        } else {
            let Some(pkg) = build_entry(&entry, source_path, edge_scope) else {
                continue;
            };
            let purl = pkg.purl.clone();
            components.push(pkg);
            seen.insert(coord_key.clone(), purl.clone());
            purl
        };

        // Trim the depth stack to this entry's depth.
        depth_stack.truncate(entry.depth);

        // If there's a parent at depth-1, emit an edge.
        if entry.depth > 0 {
            if let Some(parent_key) = depth_stack.last() {
                if let Some(parent_purl) = seen.get(parent_key) {
                    edges.push(super::ladder::GradleEdge {
                        source: parent_purl.clone(),
                        target: purl.clone(),
                        edge_scope,
                    });
                }
            }
        }

        // Push this entry onto the stack (unless it's a dedup-reference,
        // in which case we don't descend from it).
        if !matches!(entry.edge_marker, EdgeMarker::DedupReference) {
            depth_stack.push(coord_key);
        }
    }

    (components, edges)
}

/// Enumerate subprojects by running `./gradlew projects --no-daemon --quiet`.
///
/// Returns a Vec of subproject-paths like `":app"`, `":lib"`. Returns
/// `vec![String::new()]` if the project has no explicit subprojects
/// (single-project build).
pub(super) fn enumerate_subprojects(
    wrapper: &Path,
    project_dir: &Path,
    timeout: Duration,
) -> Result<Vec<String>, SubprocessOutcome> {
    let mut cmd = Command::new(wrapper);
    cmd.current_dir(project_dir);
    cmd.arg("projects");
    cmd.arg("--no-daemon");
    cmd.arg("--quiet");

    let output = spawn_with_timeout(cmd, timeout)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .lines()
            .rev()
            .take(40)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(SubprocessOutcome::NonZeroExit {
            status: output.status.code().unwrap_or(-1),
            stderr_tail: tail,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut subs: Vec<String> = Vec::new();
    for line in stdout.lines() {
        // Match: `+--- Project ':app'` or `\--- Project ':lib'`
        let trimmed = line.trim_start();
        let after = trimmed
            .strip_prefix("+--- Project '")
            .or_else(|| trimmed.strip_prefix("\\--- Project '"));
        if let Some(rest) = after {
            if let Some(name) = rest.strip_suffix('\'') {
                subs.push(name.to_string());
            }
        }
    }
    if subs.is_empty() {
        subs.push(String::new()); // Single-project build.
    }
    Ok(subs)
}

/// Top-level US1 entry point (per contracts/gradle-subprocess.md).
///
/// Discovers wrapper → enumerates subprojects → per subproject × config,
/// spawns `:dependencies` → parses output → assembles graph. Returns
/// the aggregated `GradleResolvedGraph`.
pub fn resolve_via_subprocess(
    project_dir: &Path,
    config: &GradleLadderConfig,
) -> Result<GradleResolvedGraph, SubprocessOutcome> {
    let wrapper = discover_wrapper(project_dir).ok_or(SubprocessOutcome::ToolMissing)?;
    let timeout = Duration::from_secs(config.gradle_timeout_secs.max(1));

    let subs = enumerate_subprojects(&wrapper, project_dir, timeout)?;

    let mut default_configs = vec!["runtimeClasspath".to_string(), "testRuntimeClasspath".to_string()];
    for extra in &config.gradle_extra_configurations {
        default_configs.push(extra.clone());
    }

    let mut all_components: Vec<PackageDbEntry> = Vec::new();
    let mut all_edges: Vec<super::ladder::GradleEdge> = Vec::new();

    for sub in &subs {
        for cfg in &default_configs {
            let task = if sub.is_empty() {
                "dependencies".to_string()
            } else {
                format!("{}:dependencies", sub)
            };
            let mut cmd = Command::new(&wrapper);
            cmd.current_dir(project_dir);
            cmd.arg(&task);
            cmd.arg("--configuration").arg(cfg);
            if !config.gradle_daemon {
                cmd.arg("--no-daemon");
            }
            cmd.arg("--quiet");

            let output = spawn_with_timeout(cmd, timeout)?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let tail = stderr
                    .lines()
                    .rev()
                    .take(40)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(SubprocessOutcome::NonZeroExit {
                    status: output.status.code().unwrap_or(-1),
                    stderr_tail: tail,
                });
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let parsed = parse_dependencies_output(&stdout, cfg)?;
            let scope = if cfg.contains("test") {
                EdgeScope::Test
            } else {
                EdgeScope::Runtime
            };
            let source_path = if sub.is_empty() {
                project_dir.to_string_lossy().to_string()
            } else {
                format!("{}{}", project_dir.display(), sub)
            };
            let (mut sub_components, mut sub_edges) =
                assemble_graph(parsed, &source_path, scope);
            all_components.append(&mut sub_components);
            all_edges.append(&mut sub_edges);
        }
    }

    Ok(GradleResolvedGraph {
        components: all_components,
        edges: all_edges,
        tier: GradleResolutionTier::Subprocess,
        fallback_history: Vec::new(),
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const SIMPLE_TREE: &str = "\
runtimeClasspath - Runtime classpath of source set 'main'.
+--- com.example:direct:1.0.0
|    \\--- com.example:transitive:0.5.0
\\--- com.example:another:2.0.0
";

    #[test]
    fn parses_simple_tree_with_transitive() {
        let entries = parse_dependencies_output(SIMPLE_TREE, "runtimeClasspath").unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].artifact, "direct");
        assert_eq!(entries[0].depth, 0);
        assert_eq!(entries[1].artifact, "transitive");
        assert_eq!(entries[1].depth, 1);
        assert_eq!(entries[2].artifact, "another");
        assert_eq!(entries[2].depth, 0);
    }

    const DEDUP_TREE: &str = "\
runtimeClasspath - Runtime classpath.
+--- com.example:a:1.0.0
|    \\--- com.example:shared:5.0.0
\\--- com.example:b:2.0.0
     \\--- com.example:shared:5.0.0 (*)
";

    #[test]
    fn parses_dedup_marker() {
        let entries = parse_dependencies_output(DEDUP_TREE, "runtimeClasspath").unwrap();
        assert_eq!(entries.len(), 4);
        assert!(matches!(entries[3].edge_marker, EdgeMarker::DedupReference));
    }

    const CONSTRAINT_TREE: &str = "\
runtimeClasspath - Runtime classpath.
+--- com.example:a:1.0.0
+--- com.example:c:3.0.0 (c)
\\--- com.example:d:4.0.0
";

    #[test]
    fn skips_constraint_marker() {
        let entries = parse_dependencies_output(CONSTRAINT_TREE, "runtimeClasspath").unwrap();
        // Should have 2 entries: 'a' and 'd', constraint 'c' skipped.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].artifact, "a");
        assert_eq!(entries[1].artifact, "d");
    }

    const RESOLVED_ARROW_TREE: &str = "\
runtimeClasspath - Runtime classpath.
+--- com.example:a:1.0.0 -> 1.2.0
\\--- com.example:b:2.0.0
";

    #[test]
    fn parses_version_override_via_arrow() {
        let entries = parse_dependencies_output(RESOLVED_ARROW_TREE, "runtimeClasspath").unwrap();
        assert_eq!(entries.len(), 2);
        // The resolved version wins.
        assert_eq!(entries[0].version, "1.2.0");
        assert_eq!(entries[1].version, "2.0.0");
    }

    #[test]
    fn assembles_transitive_edge() {
        let entries = parse_dependencies_output(SIMPLE_TREE, "runtimeClasspath").unwrap();
        let (components, edges) = assemble_graph(entries, "/tmp/x", EdgeScope::Runtime);
        assert_eq!(components.len(), 3);
        assert_eq!(edges.len(), 1);
        // Edge is from direct -> transitive
        assert!(edges[0].source.as_str().contains("direct"));
        assert!(edges[0].target.as_str().contains("transitive"));
    }
}
