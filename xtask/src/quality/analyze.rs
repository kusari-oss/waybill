// milestone 770 — T013: independent structural analysis of an emitted
// CycloneDX document.
//
// This module is the reason the milestone exists in the shape it does.
// waybill emits a `waybill:graph-completeness` self-assessment; three of
// eighteen trial targets reported `complete` while being structurally
// FLAT (research R3). A self-report cannot catch a bug in the thing
// reporting, so flatness is derived here from the document's own
// relationship structure and the self-report is recorded beside it,
// never consulted.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    /// Components carrying a `purl` — real packages.
    pub pkgs: u64,
    /// Components with no `purl` — m133 file-tier content (shell scripts
    /// and similar). Counted separately because it moves for entirely
    /// different reasons (research R5).
    pub files: u64,
    pub edges: u64,
    pub nodes_with_out_edges: u64,
    pub max_depth: u64,
    pub flat: bool,
    /// waybill's own assessment, verbatim. Recorded, never gated.
    pub graph_completeness: Option<String>,
}

pub fn analyze(doc: &Value) -> Analysis {
    let components = doc.get("components").and_then(Value::as_array);
    let (mut pkgs, mut files) = (0u64, 0u64);
    if let Some(cs) = components {
        for c in cs {
            match c.get("purl").and_then(Value::as_str) {
                Some(p) if !p.is_empty() => pkgs += 1,
                _ => files += 1,
            }
        }
    }

    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut edges = 0u64;
    let mut nodes_with_out_edges = 0u64;
    if let Some(deps) = doc.get("dependencies").and_then(Value::as_array) {
        for d in deps {
            let Some(r) = d.get("ref").and_then(Value::as_str) else {
                continue;
            };
            let targets: Vec<&str> = d
                .get("dependsOn")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if !targets.is_empty() {
                nodes_with_out_edges += 1;
            }
            edges += targets.len() as u64;
            adj.entry(r).or_default().extend(targets);
        }
    }

    let root = doc
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("bom-ref"))
        .and_then(Value::as_str);

    let max_depth = root.map(|r| bfs_max_depth(&adj, r)).unwrap_or(0);

    let graph_completeness = doc
        .get("metadata")
        .and_then(|m| m.get("properties"))
        .and_then(Value::as_array)
        .and_then(|props| {
            props
                .iter()
                .find(|p| p.get("name").and_then(Value::as_str) == Some("waybill:graph-completeness"))
                .and_then(|p| p.get("value").and_then(Value::as_str))
                .map(str::to_string)
        });

    Analysis {
        pkgs,
        files,
        edges,
        nodes_with_out_edges,
        max_depth,
        // A document whose components all hang directly off the root —
        // or that has no edges at all — is flat. Depth <= 1 is exact and
        // needs no calibration, unlike a ratio threshold.
        flat: max_depth <= 1,
        graph_completeness,
    }
}

fn bfs_max_depth(adj: &HashMap<&str, Vec<&str>>, root: &str) -> u64 {
    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert(root);
    let mut q: VecDeque<(&str, u64)> = VecDeque::new();
    q.push_back((root, 0));
    let mut max = 0u64;
    while let Some((cur, d)) = q.pop_front() {
        if let Some(next) = adj.get(cur) {
            for n in next {
                if seen.insert(n) {
                    max = max.max(d + 1);
                    q.push_back((n, d + 1));
                }
            }
        }
    }
    max
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(components: Value, dependencies: Value, root: Option<&str>) -> Value {
        let mut meta = json!({});
        if let Some(r) = root {
            meta["component"] = json!({ "bom-ref": r });
        }
        json!({ "metadata": meta, "components": components, "dependencies": dependencies })
    }

    #[test]
    fn deep_graph_is_not_flat() {
        let d = doc(
            json!([{"purl":"pkg:x/a"},{"purl":"pkg:x/b"},{"purl":"pkg:x/c"}]),
            json!([
                {"ref":"root","dependsOn":["a"]},
                {"ref":"a","dependsOn":["b"]},
                {"ref":"b","dependsOn":["c"]}
            ]),
            Some("root"),
        );
        let a = analyze(&d);
        assert_eq!(a.max_depth, 3);
        assert!(!a.flat);
        assert_eq!(a.edges, 3);
        assert_eq!(a.nodes_with_out_edges, 3);
    }

    #[test]
    fn star_graph_is_flat() {
        let d = doc(
            json!([{"purl":"pkg:x/a"},{"purl":"pkg:x/b"}]),
            json!([{"ref":"root","dependsOn":["a","b"]}]),
            Some("root"),
        );
        let a = analyze(&d);
        assert_eq!(a.max_depth, 1);
        assert!(a.flat);
        assert_eq!(a.nodes_with_out_edges, 1);
    }

    #[test]
    fn empty_dependencies_is_flat_at_depth_zero() {
        let d = doc(json!([{"purl":"pkg:x/a"}]), json!([]), Some("root"));
        let a = analyze(&d);
        assert_eq!(a.max_depth, 0);
        assert!(a.flat, "nothing hangs off anything — correctly flat");
        assert_eq!(a.edges, 0);
    }

    #[test]
    fn missing_root_bom_ref_yields_depth_zero() {
        let d = doc(
            json!([{"purl":"pkg:x/a"}]),
            json!([{"ref":"a","dependsOn":["b"]}]),
            None,
        );
        let a = analyze(&d);
        assert_eq!(a.max_depth, 0);
        assert!(a.flat);
        // Edges are still counted even with no root to walk from.
        assert_eq!(a.edges, 1);
    }

    #[test]
    fn components_split_by_purl_presence() {
        let d = doc(
            json!([
                {"purl":"pkg:x/a"},
                {"name":"build.sh"},
                {"purl":""},
                {"purl":"pkg:x/b"}
            ]),
            json!([]),
            Some("root"),
        );
        let a = analyze(&d);
        assert_eq!(a.pkgs, 2, "empty purl counts as file-tier");
        assert_eq!(a.files, 2);
    }

    /// The load-bearing test: a document that self-reports `complete`
    /// while being structurally flat must still measure as flat.
    #[test]
    fn self_report_never_influences_flatness() {
        let mut d = doc(
            json!([{"purl":"pkg:x/a"}]),
            json!([{"ref":"root","dependsOn":["a"]}]),
            Some("root"),
        );
        d["metadata"]["properties"] =
            json!([{"name":"waybill:graph-completeness","value":"complete"}]);
        let a = analyze(&d);
        assert_eq!(a.graph_completeness.as_deref(), Some("complete"));
        assert!(a.flat, "self-report must not override the structural measurement");
    }

    #[test]
    fn cycles_do_not_hang_the_walk() {
        let d = doc(
            json!([]),
            json!([
                {"ref":"root","dependsOn":["a"]},
                {"ref":"a","dependsOn":["b"]},
                {"ref":"b","dependsOn":["a"]}
            ]),
            Some("root"),
        );
        let a = analyze(&d);
        assert_eq!(a.max_depth, 2);
    }

    #[test]
    fn absent_keys_are_tolerated() {
        let a = analyze(&json!({}));
        assert_eq!(a.pkgs, 0);
        assert_eq!(a.files, 0);
        assert_eq!(a.edges, 0);
        assert!(a.flat);
        assert!(a.graph_completeness.is_none());
    }
}
