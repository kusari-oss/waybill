//! Python optional extras: which ones are active, and so which
//! extras-gated dependency edges are real.
//!
//! A distribution can declare dependencies that apply only when one of its
//! extras is requested (`myst-parser; extra == "docs"`), and a requirement
//! can request extras (`requests[socks]`). An extras-gated dependency is
//! installed exactly when its extra is requested, by the project's own
//! requirements or by another dependency that is itself installed. So the
//! active extras are a fixed point over the dependency graph, computed
//! here once for every Python reader (#1163).
//!
//! Each reader turns its format into [`ExtraEdge`]s keyed by normalised
//! package name, supplies whatever top-level requests it knows, and keeps
//! an edge when [`ExtraEdge::is_active`] says so.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use super::normalize_pypi_name_for_purl;

/// One dependency of a package, as the extras fixed point sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExtraEdge {
    /// Normalised name of the dependency.
    pub(crate) target: String,
    /// Extras this edge requests on the target (`foo[a, b]` gives `{a, b}`).
    pub(crate) requests: BTreeSet<String>,
    /// Extras of the declaring package that gate this edge. Empty means
    /// the edge is unconditional; otherwise it applies when any is active.
    pub(crate) gated_by: BTreeSet<String>,
}

impl ExtraEdge {
    /// Parse a PEP 508 requirement (`requests[socks]>=2; extra == "net"`).
    /// `None` when it names no package.
    pub(crate) fn from_pep508(req: &str) -> Option<Self> {
        let (head, marker) = match req.split_once(';') {
            Some((h, m)) => (h, Some(m)),
            None => (req, None),
        };
        let name = pep508_name(head);
        if name.is_empty() {
            return None;
        }
        Some(Self {
            target: normalize_pypi_name_for_purl(name),
            requests: requested_extras(head),
            gated_by: marker.map(gating_extras).unwrap_or_default(),
        })
    }

    /// Whether this edge applies, given the extras active on the package
    /// that declares it.
    pub(crate) fn is_active(&self, declaring: Option<&BTreeSet<String>>) -> bool {
        self.gated_by.is_empty() || declaring.is_some_and(|a| !self.gated_by.is_disjoint(a))
    }
}

/// The project name at the start of a PEP 508 requirement.
pub(crate) fn pep508_name(req: &str) -> &str {
    let req = req.trim();
    let end = req
        .find(|c: char| {
            c.is_whitespace() || matches!(c, '[' | '(' | '<' | '>' | '=' | '!' | '~' | ';' | '@')
        })
        .unwrap_or(req.len());
    &req[..end]
}

/// PEP 685 extra-name normalisation: lowercase, and every run of `-`, `_`
/// or `.` becomes one `-`.
pub(crate) fn normalize_extra(extra: &str) -> String {
    let mut out = String::with_capacity(extra.len());
    let mut in_sep = false;
    for c in extra.trim().chars() {
        if matches!(c, '-' | '_' | '.') {
            if !in_sep {
                out.push('-');
            }
            in_sep = true;
        } else {
            out.extend(c.to_lowercase());
            in_sep = false;
        }
    }
    out
}

/// The extras a requirement asks for: `foo[a, b]>=1` gives `{a, b}`.
pub(crate) fn requested_extras(req: &str) -> BTreeSet<String> {
    let head = req.split(';').next().unwrap_or("");
    let Some((_, rest)) = head.split_once('[') else {
        return BTreeSet::new();
    };
    names(rest.split(']').next().unwrap_or("").split(','))
}

/// Normalised, non-empty extra names.
pub(crate) fn names<'a>(extras: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    extras
        .into_iter()
        .map(normalize_extra)
        .filter(|e| !e.is_empty())
        .collect()
}

/// The extras an environment marker is gated on: `extra == "docs"` gives
/// `{docs}`. Empty when the marker names no extra.
pub(crate) fn gating_extras(marker: &str) -> BTreeSet<String> {
    static EXTRA: OnceLock<regex::Regex> = OnceLock::new();
    #[allow(clippy::unwrap_used)] // literal pattern
    let re = EXTRA.get_or_init(|| regex::Regex::new(r#"extra\s*==\s*["']([^"']+)["']"#).unwrap());
    re.captures_iter(marker)
        .map(|c| normalize_extra(&c[1]))
        .collect()
}

/// The extras active on each package, keyed by normalised name.
///
/// `seeds` are the top-level requests the reader knows of (`foo[bar]`).
/// An active edge's requests are active on its target, which can activate
/// that target's gated edges in turn, so this runs to a fixed point. Sets
/// only grow, so mutually gated packages terminate.
pub(crate) fn active_extras(
    seeds: impl IntoIterator<Item = (String, BTreeSet<String>)>,
    graph: &HashMap<String, Vec<ExtraEdge>>,
) -> HashMap<String, BTreeSet<String>> {
    let mut active: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (name, extras) in seeds {
        if !extras.is_empty() {
            active.entry(name).or_default().extend(extras);
        }
    }
    loop {
        let mut changed = false;
        for (name, edges) in graph {
            let own = active.get(name).cloned();
            for edge in edges {
                if edge.requests.is_empty() || !edge.is_active(own.as_ref()) {
                    continue;
                }
                let set = active.entry(edge.target.clone()).or_default();
                let before = set.len();
                set.extend(edge.requests.iter().cloned());
                changed |= set.len() != before;
            }
        }
        if !changed {
            return active;
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn graph(edges: &[(&str, &[&str])]) -> HashMap<String, Vec<ExtraEdge>> {
        edges
            .iter()
            .map(|(name, reqs)| {
                (
                    normalize_pypi_name_for_purl(name),
                    reqs.iter().filter_map(|r| ExtraEdge::from_pep508(r)).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn parses_name_requests_and_gating() {
        let e = ExtraEdge::from_pep508("Shellingham[Fast_Mode] >=1; python_version >= \"3.8\" and (extra == 'CLI' or extra == \"all\")").unwrap();
        assert_eq!(e.target, "shellingham");
        assert_eq!(e.requests, names(["fast-mode"]));
        assert_eq!(e.gated_by, names(["cli", "all"]));
        assert!(ExtraEdge::from_pep508("requests").unwrap().gated_by.is_empty());
        assert!(ExtraEdge::from_pep508("  ; extra == 'x'").is_none());
    }

    #[test]
    fn activation_is_transitive_and_terminates() {
        let g = graph(&[
            ("click", &["shellingham[fast]; extra == \"shell-completion\"", "rich; extra == \"other\""]),
            ("shellingham", &["psutil; extra == 'fast'"]),
            ("setuptools", &["wheel[test]; extra == \"core\""]),
            ("wheel", &["setuptools[core]; extra == \"test\""]),
        ]);
        let active = active_extras([("click".to_string(), names(["Shell_Completion"]))], &g);
        assert_eq!(active.get("click"), Some(&names(["shell-completion"])));
        assert_eq!(active.get("shellingham"), Some(&names(["fast"])));
        assert_eq!(active.get("setuptools"), None, "nothing requested setuptools[core]");

        let looped = active_extras([("setuptools".to_string(), names(["core"]))], &g);
        assert_eq!(looped.get("wheel"), Some(&names(["test"])));
        assert_eq!(looped.get("setuptools"), Some(&names(["core"])));
    }
}
