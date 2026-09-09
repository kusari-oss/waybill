// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// Operator-supplied tool set and the pinned target corpus.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Whether an invocation contacts a third-party service.
///
/// This drives the authoritative/indicative split. There is deliberately no
/// `Default`: an unset value is a config error, not an assumption. Treating
/// an enriched run as offline would present a third party's latency as our
/// measurement, which is one of the errors that motivated this harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    Offline,
    Enriched,
}

impl NetworkMode {
    pub fn is_enriched(self) -> bool {
        matches!(self, Self::Enriched)
    }
}

/// One tool to measure. Read from a gitignored local file; the committed
/// example names no specific competing tool (FR-015).
#[derive(Debug, Clone, Deserialize)]
pub struct ToolSpec {
    pub id: String,
    /// Executable plus arguments. `{target}` and `{out}` are substituted.
    pub argv: Vec<String>,
    /// How to obtain the tool's version string (FR-005).
    pub version_argv: Vec<String>,
    pub network: NetworkMode,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl ToolSpec {
    /// Substitute `{target}` and `{out}` into a concrete argv.
    pub fn resolve_argv(&self, target: &Path, out: &Path) -> Vec<String> {
        self.argv
            .iter()
            .map(|a| {
                a.replace("{target}", &target.display().to_string())
                    .replace("{out}", &out.display().to_string())
            })
            .collect()
    }
}

/// How a target's true package set is derived. Recorded with every score
/// computed from it; scores from different methods are never compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TruthMethod {
    /// Union of every `go.sum`. Always available offline, but a SUPERSET of
    /// what is built.
    GoSumUnion,
    /// Union of every `go.mod` require. Closer, still not the built set.
    GoModRequires,
    /// A committed expected list. Exact by construction.
    DeclaredExact,
}

impl TruthMethod {
    /// FR-008b. A superset changes what a score means: `found` rewards
    /// over-reporting and a tool correctly omitting unused modules scores
    /// worse. Every report built on one must say so.
    pub fn is_superset(self) -> bool {
        matches!(self, Self::GoSumUnion | Self::GoModRequires)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoSumUnion => "go-sum-union",
            Self::GoModRequires => "go-mod-requires",
            Self::DeclaredExact => "declared-exact",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TruthSpec {
    pub method: TruthMethod,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub name: String,
    pub repo: String,
    pub sha: String,
    pub ecosystem: String,
    /// Absent means: count this target, do not score it, and say so (FR-009).
    #[serde(default)]
    pub truth: Option<TruthSpec>,
}

#[derive(Debug, Deserialize)]
struct ToolFile {
    tools: Vec<ToolSpec>,
}

#[derive(Debug, Deserialize)]
struct TargetFile {
    targets: Vec<Target>,
}

pub fn load_tools(path: &Path) -> Result<Vec<ToolSpec>, Box<dyn Error>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "cannot read tool set at {}: {e}\n\
             \n\
             This file is gitignored and is not created for you, because it \
             names the specific tools you compare against and must not enter \
             a public repository. Start from the example:\n\
             \n\
             \tcp xtask/compare/tools.example.toml {}",
            path.display(),
            path.display()
        )
    })?;
    let parsed: ToolFile = toml::from_str(&text)
        .map_err(|e| format!("tool set at {} is not valid: {e}", path.display()))?;
    if parsed.tools.is_empty() {
        return Err(format!("tool set at {} defines no tools", path.display()).into());
    }
    let mut seen = std::collections::HashSet::new();
    for t in &parsed.tools {
        if !seen.insert(&t.id) {
            return Err(format!("duplicate tool id {:?}", t.id).into());
        }
    }
    Ok(parsed.tools)
}

pub fn load_targets(path: &Path) -> Result<Vec<Target>, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read targets at {}: {e}", path.display()))?;
    let parsed: TargetFile = toml::from_str(&text)
        .map_err(|e| format!("targets at {} are not valid: {e}", path.display()))?;
    Ok(parsed.targets)
}

/// Repository-root-relative default location of the pinned corpus.
pub fn default_targets_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("xtask/compare/targets.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_tool_set_round_trips() {
        let toml_text = r#"
            [[tools]]
            id = "waybill"
            argv = ["waybill", "scan", "{target}", "-o", "{out}"]
            version_argv = ["waybill", "--version"]
            network = "offline"
        "#;
        let parsed: ToolFile = toml::from_str(toml_text).expect("parse");
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.tools[0].network, NetworkMode::Offline);
    }

    #[test]
    fn unknown_network_mode_is_rejected_not_defaulted() {
        // Defaulting here would label a third party's latency as our own
        // authoritative measurement.
        let toml_text = r#"
            [[tools]]
            id = "x"
            argv = ["x"]
            version_argv = ["x", "--version"]
            network = "sometimes"
        "#;
        assert!(toml::from_str::<ToolFile>(toml_text).is_err());
    }

    #[test]
    fn missing_network_mode_is_rejected() {
        let toml_text = r#"
            [[tools]]
            id = "x"
            argv = ["x"]
            version_argv = ["x", "--version"]
        "#;
        assert!(toml::from_str::<ToolFile>(toml_text).is_err());
    }

    #[test]
    fn absent_tool_set_error_points_at_the_example() {
        let err = load_tools(Path::new("/nonexistent/tools.local.toml"))
            .expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("tools.example.toml"), "{msg}");
        assert!(msg.contains("gitignored"), "{msg}");
    }

    #[test]
    fn argv_substitution_fills_both_placeholders() {
        let spec = ToolSpec {
            id: "t".into(),
            argv: vec!["t".into(), "{target}".into(), "-o".into(), "{out}".into()],
            version_argv: vec![],
            network: NetworkMode::Offline,
            env: BTreeMap::new(),
        };
        let got = spec.resolve_argv(Path::new("/tree"), Path::new("/tmp/o.json"));
        assert_eq!(got, vec!["t", "/tree", "-o", "/tmp/o.json"]);
    }

    #[test]
    fn superset_methods_are_flagged() {
        assert!(TruthMethod::GoSumUnion.is_superset());
        assert!(TruthMethod::GoModRequires.is_superset());
        assert!(!TruthMethod::DeclaredExact.is_superset());
    }

    #[test]
    fn target_without_truth_parses_and_is_none() {
        let toml_text = r#"
            [[targets]]
            name = "x"
            repo = "https://example.test/x"
            sha = "abc"
            ecosystem = "cargo"
        "#;
        let parsed: TargetFile = toml::from_str(toml_text).expect("parse");
        assert!(parsed.targets[0].truth.is_none());
    }
}
