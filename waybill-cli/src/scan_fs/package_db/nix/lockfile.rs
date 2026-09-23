//! Milestone 925 — `flake.lock` parsing.
//!
//! The lockfile is JSON. Nothing here evaluates Nix, shells out, or touches
//! the network (FR-013, SC-002).
//!
//! Shapes below were measured against five real lockfiles during Phase 0
//! research, not inferred from documentation. The two findings that changed
//! the design:
//!
//! - A value in a node's `inputs` map is **either** a string (the key of
//!   another node) **or** an array (a `follows` alias path). Designing from a
//!   single-input lockfile would have produced a parser that treats every
//!   value as a node key and fabricates or panics on any real-world flake.
//!   Encoded as [`InputEdge`] so the alias case cannot be forgotten
//!   (Principle IV).
//! - A `tarball` input carries a `rev` despite having no `owner`/`repo`, so it
//!   is identifiable rather than skippable. That affected 2 of 5 samples.

use std::collections::BTreeMap;
use std::path::Path;

/// The schema version this parser understands.
///
/// All five measured lockfiles report 7. That tells us 7 is current, not that
/// it is the only version we will ever meet — the field exists precisely
/// because the format is expected to change, so an unknown value is reported
/// rather than parsed on optimistic assumptions (FR-010).
pub(crate) const SUPPORTED_VERSION: u64 = 7;

/// Why a lockfile produced no inputs.
///
/// `UnrecognisedVersion` and `Malformed` take the same fallback path but are
/// distinguished in the diagnostic: "I do not know this version" and "this is
/// not valid JSON" are different things to whoever reads the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParseFailure {
    UnrecognisedVersion { found: u64 },
    Malformed { reason: String },
}

impl std::fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnrecognisedVersion { found } => write!(
                f,
                "unrecognised flake.lock schema version {found} (this parser understands {SUPPORTED_VERSION})"
            ),
            Self::Malformed { reason } => write!(f, "malformed flake.lock: {reason}"),
        }
    }
}

/// One edge in a node's `inputs` map.
///
/// The discriminated shape is the whole point — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum InputEdge {
    /// A plain string: the key of another node in `nodes`.
    NodeRef(String),
    /// An array: a `follows` alias path through the input graph. Resolves to
    /// an existing pin and MUST NOT mint a second component (FR-004).
    Follows(Vec<String>),
}

/// A resolved pin. This is what becomes a component.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct LockedRef {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) repo: Option<String>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) rev: Option<String>,
    #[serde(default, rename = "narHash")]
    pub(crate) nar_hash: Option<String>,
    #[serde(default, rename = "lastModified")]
    pub(crate) last_modified: Option<i64>,
}

/// The reference as the author wrote it, before resolution. Explanatory only.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct OriginalRef {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) repo: Option<String>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    /// A moving branch or tag, e.g. `nixos-unstable`.
    #[serde(default, rename = "ref")]
    pub(crate) git_ref: Option<String>,
    #[serde(default)]
    pub(crate) rev: Option<String>,
}

impl OriginalRef {
    /// Whether this original adds anything the locked reference does not.
    ///
    /// An `original` that already names the locked revision is not a
    /// difference worth recording — emitting it would be noise (FR-006, and
    /// User Story 3's second acceptance scenario).
    pub(crate) fn differs_from(&self, locked: &LockedRef) -> bool {
        match (&self.rev, &locked.rev) {
            (Some(a), Some(b)) if a == b => false,
            _ => self.git_ref.is_some() || self.rev.as_deref() != locked.rev.as_deref(),
        }
    }

    /// The moving part, when there is one.
    pub(crate) fn moving_ref(&self) -> Option<&str> {
        self.git_ref.as_deref()
    }
}

/// One entry in `nodes`. The root node is distinguished only by being named
/// in `.root`; it carries `inputs` but no `locked`/`original`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct FlakeNode {
    #[serde(default)]
    pub(crate) locked: Option<LockedRef>,
    #[serde(default)]
    pub(crate) original: Option<OriginalRef>,
    #[serde(default)]
    pub(crate) inputs: BTreeMap<String, InputEdge>,
}

/// A parsed lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlakeLockDocument {
    pub(crate) nodes: BTreeMap<String, FlakeNode>,
    pub(crate) root_key: String,
}

#[derive(serde::Deserialize)]
struct RawLock {
    version: u64,
    #[serde(default)]
    nodes: BTreeMap<String, FlakeNode>,
    #[serde(default)]
    root: Option<String>,
}

impl FlakeLockDocument {
    /// Resolve an [`InputEdge`] to the node key it ultimately names.
    ///
    /// A `Follows` path walks `inputs` from the root: `["a", "b"]` means the
    /// node reached by following input `a` of the root, then input `b` of
    /// that node. Returns `None` when the path does not resolve, which is a
    /// malformed lockfile rather than something to guess at.
    pub(crate) fn resolve(&self, edge: &InputEdge) -> Option<&str> {
        // Every returned slice is borrowed from `self.nodes`, never from
        // `edge` — the caller keeps the document alive, not the edge.
        match edge {
            InputEdge::NodeRef(k) => self.key_of(k),
            InputEdge::Follows(path) => {
                let mut current = self.key_of(&self.root_key)?;
                for segment in path {
                    let next = self.nodes.get(current)?.inputs.get(segment)?;
                    current = match next {
                        InputEdge::NodeRef(k) => self.key_of(k)?,
                        // A follows pointing at another follows. The path is
                        // bounded by the lockfile, so this terminates.
                        InputEdge::Follows(_) => self.resolve(next)?,
                    };
                }
                Some(current)
            }
        }
    }

    /// The map's own copy of a node key, so the borrow outlives the lookup.
    fn key_of(&self, candidate: &str) -> Option<&str> {
        self.nodes.get_key_value(candidate).map(|(k, _)| k.as_str())
    }

    /// Every node that should become a component, in deterministic order.
    ///
    /// Excludes the root (it is the project, not an input) and any node whose
    /// pin cannot be identified. `nodes` is a `BTreeMap` so iteration is
    /// already ordered; emission sorts again on the identifier because the key
    /// is not the identifier (SC-006).
    pub(crate) fn emittable_nodes(&self) -> Vec<(&str, &FlakeNode, &LockedRef)> {
        self.nodes
            .iter()
            .filter(|(key, _)| *key != &self.root_key)
            .filter_map(|(key, node)| {
                node.locked.as_ref().map(|l| (key.as_str(), node, l))
            })
            .collect()
    }
}

/// Parse a `flake.lock`.
pub(crate) fn parse_flake_lock(path: &Path) -> Result<FlakeLockDocument, ParseFailure> {
    let text = std::fs::read_to_string(path).map_err(|e| ParseFailure::Malformed {
        reason: format!("read failed: {e}"),
    })?;
    parse_flake_lock_str(&text)
}

pub(crate) fn parse_flake_lock_str(text: &str) -> Result<FlakeLockDocument, ParseFailure> {
    let raw: RawLock = serde_json::from_str(text).map_err(|e| ParseFailure::Malformed {
        reason: e.to_string(),
    })?;

    if raw.version != SUPPORTED_VERSION {
        return Err(ParseFailure::UnrecognisedVersion { found: raw.version });
    }

    let root_key = raw.root.ok_or_else(|| ParseFailure::Malformed {
        reason: "no `root` key".to_string(),
    })?;

    if !raw.nodes.contains_key(&root_key) {
        return Err(ParseFailure::Malformed {
            reason: format!("`root` names `{root_key}`, which is absent from `nodes`"),
        });
    }

    Ok(FlakeLockDocument {
        nodes: raw.nodes,
        root_key,
    })
}
