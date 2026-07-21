// Module-level `#[allow(dead_code)]`: the `classify_workspace_edge` +
// `merge_workspace_replaces` helpers are the documented public-API
// surface for the Q1 hybrid workspace-attribution fix (FR-002 +
// FR-005). They're exercised by the unit tests in the same module
// but not yet consumed by production code — the T025-T027 wiring
// tasks (FR-007 empirical investigation) are follow-on work per
// spec.md Assumptions §7. Once T025-T027 land, this allow is removed.
#![allow(dead_code)]

//! Milestone 161 — Go workspace-mode (`go.work`) parser + attribution helpers.
//!
//! Provides `parse_go_work()` for reading `go.work` files, the
//! `WorkspaceMode` document-scope signal (C112), and the
//! `EdgeDisposition` classifier used by the Q1 hybrid workspace-edge
//! disposition rule.
//!
//! See specs/161-go-workspace-edges/ for the design.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::scan_fs::package_db::golang::module_id::ModuleId;

// --------------------------------------------------------------------
// E1 — WorkspaceMode (C112 doc-scope signal)
// --------------------------------------------------------------------

/// Detection outcome for `go.work` at the scanned root. Drives the
/// `waybill:go-workspace-mode` (C112) document-scope annotation.
///
/// Per Q2 clarification 2026-07-04: `Detected { use_count: 0 }` is
/// a legal state (empty-but-valid workspace scaffolding) — the file
/// is syntactically valid but has zero `use` members. Treating it as
/// `Malformed` would false-positive consumer defect-detection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum WorkspaceMode {
    /// `go.work` file present, N use directives parsed successfully.
    /// N MAY be zero.
    Detected { use_count: usize },
    /// No `go.work` file at the scanned root, OR `GOWORK=off` in
    /// scan env. Default variant.
    #[default]
    Absent,
    /// `go.work` file present but parser rejected it. Reason string
    /// names the failure class from the closed-but-extensible vocab.
    Malformed { reason: String },
}

impl WorkspaceMode {
    /// Wire value for `waybill:go-workspace-mode` (C112).
    /// Per Q2 clarification: empty-use case yields
    /// `detected: 0 use-modules` (not `malformed:`).
    pub fn as_wire_str(&self) -> String {
        match self {
            Self::Detected { use_count } => {
                format!("detected: {use_count} use-modules")
            }
            Self::Absent => "absent".to_string(),
            Self::Malformed { reason } => format!("malformed: {reason}"),
        }
    }

    /// True iff the scan should apply workspace-attribution semantics.
    /// `Absent` + `Malformed` both fall through to the milestone-055
    /// non-workspace resolution path.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Detected { .. })
    }
}

// --------------------------------------------------------------------
// E2 — GoWorkDocument (parsed representation)
// --------------------------------------------------------------------

/// Parsed contents of a `go.work` file.
///
/// - `go_version` — the `go X.Y[.Z]` line, if present.
/// - `use_paths` — paths from `use ( ... )` or single-line `use` directives.
///   Stored as-declared (relative to the go.work file's parent dir); the
///   caller in `legacy::read` canonicalizes against the workspace root.
/// - `replaces` — `replace <old> => <new>` directives. Same shape as
///   milestone-002 `GoModDocument.replaces` (key = `(path, version)`;
///   value = `(new_path, new_version)`) for reuse of downstream apply
///   logic per FR-005.
#[derive(Clone, Debug, Default)]
pub struct GoWorkDocument {
    pub go_version: Option<String>,
    pub use_paths: Vec<PathBuf>,
    pub replaces: HashMap<(String, String), (String, String)>,
}

// --------------------------------------------------------------------
// E3 — EdgeDisposition (Q1 hybrid classifier output)
// --------------------------------------------------------------------

/// Per-edge decision produced by the Q1 hybrid workspace-attribution
/// classifier. Consumed by the post-resolution sweep in `legacy::read`.
///
/// - `Keep` — edge target is either not workspace-internal or has an
///   already-resolved version; retain as-is.
/// - `Resolve` — edge target IS workspace-internal AND source's own
///   require block names the target; rewrite target version to
///   `sibling_version`.
/// - `Suppress` — edge target IS workspace-internal AND source's own
///   require block does NOT name the target; drop the edge (FR-002
///   truthful attribution).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EdgeDisposition {
    Keep,
    Resolve { sibling_version: String },
    Suppress { reason: String },
}

// --------------------------------------------------------------------
// go.work parser (T005)
// --------------------------------------------------------------------

/// Parser state.
enum ParserState {
    Toplevel,
    InUseBlock,
    InReplaceBlock,
}

/// Parse a `go.work` file body into a `WorkspaceMode` value.
///
/// Successful parses produce `WorkspaceMode::Detected { use_count }`
/// where `use_count` is the number of `use` directives found (may be
/// zero per Q2 clarification). Parse errors produce
/// `WorkspaceMode::Malformed { reason }` with a closed-but-extensible
/// 6-code vocabulary (see `contracts/annotations.md` §C112).
///
/// The parsed document itself is returned via `parse_go_work_full`;
/// this helper is a convenience for callers that only need the
/// annotation-value classification.
pub fn parse_go_work(body: &str) -> WorkspaceMode {
    match parse_go_work_full(body) {
        Ok(doc) => WorkspaceMode::Detected {
            use_count: doc.use_paths.len(),
        },
        Err(reason) => WorkspaceMode::Malformed { reason },
    }
}

/// Parse a `go.work` file body into a `GoWorkDocument`.
///
/// Returns `Err(reason)` where `reason` is one of the closed-vocab
/// codes on parse failure. Callers that only need the annotation
/// value should use `parse_go_work()`.
pub fn parse_go_work_full(body: &str) -> Result<GoWorkDocument, String> {
    let mut doc = GoWorkDocument::default();
    let mut state = ParserState::Toplevel;
    let mut seen_paths: std::collections::HashSet<PathBuf> =
        std::collections::HashSet::new();

    for raw_line in body.lines() {
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        match state {
            ParserState::Toplevel => {
                if let Some(rest) = line.strip_prefix("go ") {
                    doc.go_version = Some(rest.trim().to_string());
                } else if line == "use (" || line == "use(" {
                    state = ParserState::InUseBlock;
                } else if let Some(rest) = line.strip_prefix("use ") {
                    // Single-line `use "./path"` or `use ./path`.
                    let path = unquote(rest.trim());
                    if path.is_empty() {
                        return Err("invalid-use-path".to_string());
                    }
                    push_use_path(&mut doc, &mut seen_paths, path)?;
                } else if line == "replace (" || line == "replace(" {
                    state = ParserState::InReplaceBlock;
                } else if let Some(rest) = line.strip_prefix("replace ") {
                    // Single-line replace: `old[@ver] => new[@ver]`
                    parse_replace_line(&mut doc, rest.trim())?;
                } else {
                    return Err("unknown-directive".to_string());
                }
            }
            ParserState::InUseBlock => {
                if line == ")" {
                    state = ParserState::Toplevel;
                } else {
                    let path = unquote(&line);
                    if path.is_empty() {
                        return Err("invalid-use-path".to_string());
                    }
                    push_use_path(&mut doc, &mut seen_paths, path)?;
                }
            }
            ParserState::InReplaceBlock => {
                if line == ")" {
                    state = ParserState::Toplevel;
                } else {
                    parse_replace_line(&mut doc, &line)?;
                }
            }
        }
    }

    match state {
        ParserState::Toplevel => Ok(doc),
        ParserState::InUseBlock => Err("missing-use-close-paren".to_string()),
        ParserState::InReplaceBlock => {
            Err("missing-use-close-paren".to_string())
        }
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn push_use_path(
    doc: &mut GoWorkDocument,
    seen: &mut std::collections::HashSet<PathBuf>,
    path: String,
) -> Result<(), String> {
    let pb = PathBuf::from(path);
    if !seen.insert(pb.clone()) {
        return Err("duplicate-use-path".to_string());
    }
    doc.use_paths.push(pb);
    Ok(())
}

fn parse_replace_line(doc: &mut GoWorkDocument, line: &str) -> Result<(), String> {
    // Grammar: `<old_path>[@<old_ver>] => <new_path>[@<new_ver>]`
    let (lhs, rhs) = match line.split_once("=>") {
        Some(t) => t,
        None => return Err("invalid-replace-syntax".to_string()),
    };
    let (old_path, old_ver) = split_module_ref(lhs.trim());
    let (new_path, new_ver) = split_module_ref(rhs.trim());
    if old_path.is_empty() || new_path.is_empty() {
        return Err("invalid-replace-syntax".to_string());
    }
    doc.replaces.insert(
        (old_path, old_ver),
        (new_path, new_ver),
    );
    Ok(())
}

fn split_module_ref(s: &str) -> (String, String) {
    // go.mod/go.work replace grammar uses SPACE separator between
    // path and version: `<path> <version>` (version optional). The
    // `@` separator is only used in `go mod graph` output + PURLs,
    // not in go.mod source syntax.
    let mut parts = s.split_whitespace();
    let path = parts.next().unwrap_or("").to_string();
    let version = parts.next().unwrap_or("").to_string();
    (path, version)
}

// --------------------------------------------------------------------
// Q1 hybrid classifier (T025)
// --------------------------------------------------------------------

/// Classify a candidate workspace edge per the Q1 hybrid disposition rule.
///
/// Inputs:
/// - `source_requires` — the source module's own `require`-block module
///   paths (from that module's own `go.mod`, NOT the workspace root's).
/// - `target` — the candidate edge's target `ModuleId`.
/// - `use_modules_map` — canonical map of workspace-internal module paths
///   to their filesystem locations.
/// - `sibling_versions` — canonical map of workspace-internal module
///   paths to their declared module versions (from each `use`d module's
///   own `go.mod`). Empty version string means "unspecified" (Go's
///   default for workspace-internal modules).
///
/// Returns:
/// - `Keep` — target is not workspace-internal, OR target has an already-
///   resolved version (not `v0.0.0-unknown`).
/// - `Resolve { sibling_version }` — target IS workspace-internal AND
///   source's require block names the target; version overridden.
/// - `Suppress { reason }` — target IS workspace-internal AND source's
///   require block does NOT name the target (FR-002 false-attribution).
pub fn classify_workspace_edge(
    source_requires: &[String],
    target: &ModuleId,
    use_modules_map: &HashMap<String, PathBuf>,
    sibling_versions: &HashMap<String, String>,
) -> EdgeDisposition {
    // Only edges to workspace-internal targets with unresolved versions
    // are candidates for reclassification. Everything else stays.
    if !use_modules_map.contains_key(target.path()) {
        return EdgeDisposition::Keep;
    }
    let target_path = target.path();
    let target_ver = target.version();
    let is_unresolved =
        target_ver.is_empty() || target_ver == "v0.0.0-unknown";
    if !is_unresolved {
        return EdgeDisposition::Keep;
    }
    // Q1 hybrid arm: check whether the source's own require block
    // names this target.
    let source_names_target = source_requires.iter().any(|r| r == target_path);
    if source_names_target {
        let sibling_version = sibling_versions
            .get(target_path)
            .cloned()
            .unwrap_or_default();
        EdgeDisposition::Resolve { sibling_version }
    } else {
        EdgeDisposition::Suppress {
            reason: format!(
                "workspace-internal target not in source require block: {target_path}"
            ),
        }
    }
}

// --------------------------------------------------------------------
// Workspace-level replace merge (T008a per FR-005 apply pipeline)
// --------------------------------------------------------------------

/// Merge workspace-level `replace` directives into a per-project-root
/// `replaces` map per FR-005 + Go MVS semantics. Workspace-level entries
/// **override** module-level entries of the same `(path, version)` shape.
///
/// Both maps use the milestone-002 replace shape: key = `(old_path,
/// old_version)`, value = `(new_path, new_version)`. Conversion to the
/// resolver's `HashMap<ModuleId, ModuleId>` shape is done by the caller
/// after this merge — the caller has access to `ModuleId::new()`.
pub fn merge_workspace_replaces(
    module_level: &mut HashMap<(String, String), (String, String)>,
    workspace_level: &HashMap<(String, String), (String, String)>,
) {
    for (k, v) in workspace_level {
        // Workspace overrides module-level. `.insert` returns the
        // previous value if any — we don't use it; the overwrite is
        // the FR-005 semantic.
        module_level.insert(k.clone(), v.clone());
    }
}

// --------------------------------------------------------------------
// Unit tests (T029–T038 + T033a + T037a)
// --------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn t029_parse_go_work_minimal_two_use_paths() {
        // SC-009 (a): minimal go 1.24 + use ( . ./staging/foo ) returns
        // Detected with use_count == 2.
        let src = "go 1.24\nuse (\n    .\n    ./staging/foo\n)\n";
        assert_eq!(parse_go_work(src), WorkspaceMode::Detected { use_count: 2 });
    }

    #[test]
    fn t030_parse_go_work_missing_close_paren_reports_malformed() {
        // SC-009 (b): missing `)` after `use (` returns
        // Malformed { reason: "missing-use-close-paren" }.
        let src = "go 1.24\nuse (\n    ./foo\n";
        match parse_go_work(src) {
            WorkspaceMode::Malformed { reason } => {
                assert_eq!(reason, "missing-use-close-paren");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn t031_parse_go_work_empty_use_block() {
        // SC-009 (f) + Q2: empty `use ()` block returns
        // Detected { use_count: 0 } (not Malformed).
        let src = "go 1.24\nuse (\n)\n";
        assert_eq!(parse_go_work(src), WorkspaceMode::Detected { use_count: 0 });
    }

    #[test]
    fn t032_parse_go_work_replace_directive_single_line() {
        // SC-009 (c): single-line replace directive parses correctly.
        let src = "go 1.24\nreplace github.com/old/lib v1.0.0 => github.com/new/lib v2.0.0\n";
        let doc = parse_go_work_full(src).expect("parse ok");
        let key = (
            "github.com/old/lib".to_string(),
            "v1.0.0".to_string(),
        );
        let val = doc.replaces.get(&key).expect("replace present");
        assert_eq!(val.0, "github.com/new/lib");
        assert_eq!(val.1, "v2.0.0");
    }

    #[test]
    fn t032_parse_go_work_replace_directive_block_form() {
        // SC-009 (c): block-form replace parses correctly.
        let src = "\
go 1.24
replace (
    github.com/old/one v1.0.0 => github.com/new/one v2.0.0
    github.com/old/two => github.com/new/two v3.0.0
)
";
        let doc = parse_go_work_full(src).expect("parse ok");
        assert_eq!(doc.replaces.len(), 2);
        let val = doc
            .replaces
            .get(&("github.com/old/two".to_string(), String::new()))
            .expect("open-version replace present");
        assert_eq!(val.1, "v3.0.0");
    }

    #[test]
    fn t033_parse_go_work_handles_quoted_and_unquoted_paths() {
        // SC-009 (d): both quoted and unquoted use paths.
        let src = "use (\n    \"./quoted\"\n    ./unquoted\n)\n";
        let doc = parse_go_work_full(src).expect("parse ok");
        assert_eq!(doc.use_paths.len(), 2);
        assert!(doc.use_paths.iter().any(|p| p == &PathBuf::from("./quoted")));
        assert!(doc.use_paths.iter().any(|p| p == &PathBuf::from("./unquoted")));
    }

    #[test]
    fn t033a_use_dot_self_reference_yields_two_paths() {
        // SC-009 (j) + FR-003: `use ( . ./child )` returns 2 use-paths
        // including the workspace-root self-reference `.`. This is the
        // load-bearing regression guard for "workspace-root treated as
        // another use'd module".
        let src = "use (\n    .\n    ./child\n)\n";
        let doc = parse_go_work_full(src).expect("parse ok");
        assert_eq!(doc.use_paths.len(), 2, "must include . AND ./child");
        assert!(
            doc.use_paths.iter().any(|p| p == &PathBuf::from(".")),
            "workspace-root `.` self-reference must appear as a use path"
        );
        assert!(
            doc.use_paths.iter().any(|p| p == &PathBuf::from("./child")),
            "child directory must appear as a use path"
        );
        assert_eq!(
            parse_go_work(src),
            WorkspaceMode::Detected { use_count: 2 }
        );
    }

    #[test]
    fn t034_workspace_mode_wire_strings_are_stable() {
        // Contracts/annotations.md §C112 vocabulary.
        assert_eq!(
            WorkspaceMode::Detected { use_count: 47 }.as_wire_str(),
            "detected: 47 use-modules"
        );
        assert_eq!(WorkspaceMode::Absent.as_wire_str(), "absent");
        assert_eq!(
            WorkspaceMode::Malformed {
                reason: "missing-use-close-paren".to_string()
            }
            .as_wire_str(),
            "malformed: missing-use-close-paren"
        );
    }

    #[test]
    fn t035_classify_workspace_edge_keeps_non_workspace_target() {
        // SC-009 (g): target module path NOT in use_modules_map → Keep.
        let mut use_map = HashMap::new();
        use_map.insert(
            "k8s.io/api".to_string(),
            PathBuf::from("/tmp/api"),
        );
        let target = ModuleId::new("github.com/pkg/errors", "v0.9.1");
        let disp = classify_workspace_edge(
            &[],
            &target,
            &use_map,
            &HashMap::new(),
        );
        assert_eq!(disp, EdgeDisposition::Keep);
    }

    #[test]
    fn t036_classify_workspace_edge_resolves_when_source_names_target() {
        // Q1: target IS workspace-internal AND source's require block
        // names the target → Resolve to sibling's declared version.
        let mut use_map = HashMap::new();
        use_map.insert("k8s.io/api".to_string(), PathBuf::from("/tmp/api"));
        let mut sibling_versions = HashMap::new();
        sibling_versions.insert("k8s.io/api".to_string(), "v0.34.0".to_string());
        let target = ModuleId::new("k8s.io/api", "v0.0.0-unknown");
        let source_requires = vec!["k8s.io/api".to_string()];
        let disp = classify_workspace_edge(
            &source_requires,
            &target,
            &use_map,
            &sibling_versions,
        );
        assert_eq!(
            disp,
            EdgeDisposition::Resolve {
                sibling_version: "v0.34.0".to_string(),
            }
        );
    }

    #[test]
    fn t037_classify_workspace_edge_suppresses_false_leakage() {
        // SC-009 (h): target IS workspace-internal AND source's require
        // block does NOT name the target — the test-kubernetes false-
        // edge shape (k8s.io/api → kube-proxy where api's go.mod
        // doesn't require kube-proxy). Must Suppress.
        let mut use_map = HashMap::new();
        use_map.insert("k8s.io/api".to_string(), PathBuf::from("/tmp/api"));
        use_map.insert(
            "k8s.io/kube-proxy".to_string(),
            PathBuf::from("/tmp/kube-proxy"),
        );
        let target = ModuleId::new("k8s.io/kube-proxy", "v0.0.0-unknown");
        // Source is k8s.io/api. Its require block does NOT include
        // k8s.io/kube-proxy (only its real transitive deps).
        let source_requires = vec![
            "github.com/gogo/protobuf".to_string(),
            "gopkg.in/yaml.v3".to_string(),
        ];
        match classify_workspace_edge(
            &source_requires,
            &target,
            &use_map,
            &HashMap::new(),
        ) {
            EdgeDisposition::Suppress { reason } => {
                assert!(reason.contains("k8s.io/kube-proxy"));
            }
            other => panic!("expected Suppress, got {other:?}"),
        }
    }

    #[test]
    fn t037a_merge_workspace_replaces_workspace_precedence() {
        // FR-005 apply-pipeline: workspace-level replace overrides
        // module-level replace of the same `(path, version)` shape per
        // Go MVS semantics.
        let module_key = (
            "github.com/old/lib".to_string(),
            "v1.0.0".to_string(),
        );
        let mut module_level: HashMap<(String, String), (String, String)> =
            HashMap::new();
        module_level.insert(
            module_key.clone(),
            (
                "github.com/module-target/lib".to_string(),
                "v1.5.0".to_string(),
            ),
        );

        let mut workspace_level: HashMap<
            (String, String),
            (String, String),
        > = HashMap::new();
        workspace_level.insert(
            module_key.clone(),
            (
                "github.com/workspace-target/lib".to_string(),
                "v2.0.0".to_string(),
            ),
        );

        merge_workspace_replaces(&mut module_level, &workspace_level);

        let resolved = module_level.get(&module_key).expect("entry present");
        assert_eq!(
            resolved.0, "github.com/workspace-target/lib",
            "FR-005: workspace-level replace target MUST override module-level"
        );
        assert_eq!(resolved.1, "v2.0.0");
    }

    #[test]
    fn t038_is_active_predicate() {
        // Data-model.md E1: is_active() returns true iff Detected.
        assert!(WorkspaceMode::Detected { use_count: 3 }.is_active());
        assert!(WorkspaceMode::Detected { use_count: 0 }.is_active());
        assert!(!WorkspaceMode::Absent.is_active());
        assert!(!WorkspaceMode::Malformed {
            reason: "x".to_string()
        }
        .is_active());
    }

    #[test]
    fn unknown_directive_reports_malformed() {
        // Closed-vocab: unknown top-level directive → malformed.
        let src = "go 1.24\nrequire github.com/foo v1.0.0\n";
        match parse_go_work(src) {
            WorkspaceMode::Malformed { reason } => {
                assert_eq!(reason, "unknown-directive");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_use_path_reports_malformed() {
        // Closed-vocab: duplicate use path → malformed.
        let src = "use (\n    ./foo\n    ./foo\n)\n";
        match parse_go_work(src) {
            WorkspaceMode::Malformed { reason } => {
                assert_eq!(reason, "duplicate-use-path");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn invalid_replace_syntax_reports_malformed() {
        // Closed-vocab: missing => in replace → malformed.
        let src = "replace github.com/old/lib v1.0.0\n";
        match parse_go_work(src) {
            WorkspaceMode::Malformed { reason } => {
                assert_eq!(reason, "invalid-replace-syntax");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }
}
