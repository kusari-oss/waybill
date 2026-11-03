//! Read an hpack `package.yaml` (milestone 1273, issue #1278).
//!
//! # Why this exists
//!
//! `package.yaml` is how most stack-based Haskell projects declare their
//! dependencies, and a project that `.gitignore`s its generated `*.cabal` is
//! following hpack's own documented workflow. Before this module, such a
//! project emitted **zero** components and the document asserted
//! `waybill:graph-completeness=complete` over that nothing — while the reader
//! was dispatched to the file and said nothing about it. Milestone 143's Q3
//! chose "detect and warn, do not parse" to avoid a second source of truth;
//! milestone 1273's FR-001 reverses that deliberately, in the spec, because
//! the cost it avoided is smaller than publishing a project's dependency set
//! as empty.
//!
//! # Why this lowers into `CabalManifest` instead of modelling hpack
//!
//! Measured, not assumed (`specs/1273-haskell-nixpkgs-truthfulness/measurements/hpack-semantics.sh`,
//! hpack 0.38.3, 12 cases). **hpack does not evaluate `when:` conditions.** It
//! prunes a literal `false` and lowers everything else — `os(darwin)`, and even
//! `flag(x)` where `x`'s default is declared in the same file — to a Cabal `if`
//! block, verbatim:
//!
//! ```text
//!   when:                           library
//!     - condition: flag(fast)   ->    build-depends: base
//!       dependencies: [vector]        if flag(fast)
//!                                       build-depends: vector
//! ```
//!
//! So a `when:` block *is* a Cabal conditional, and the `.cabal` reader already
//! evaluates those against declared flag defaults (`stanza_default_buildable`,
//! milestone 955 / PR #1030). Writing a second evaluator here would let one
//! project get two answers depending on which file declared its dependencies —
//! which is the defect class this milestone exists to remove, not a style
//! preference.
//!
//! # The three measured rules
//!
//! 1. **Inheritance reaches every stanza kind.** A top-level `dependencies`
//!    list renders into `library`, `executables`, `tests` *and* `benchmarks`.
//!    A per-component list is **additive**, not a replacement.
//! 2. **For a repeated name the last declaration wins outright**, replacing
//!    the earlier constraint entirely. There is no intersection — `text >= 1.0`
//!    plus `text < 3` renders `text <3` — and a bare per-component entry
//!    *discards* a constraint the top level declared. Measured both across the
//!    inheritance boundary and within one list, so this is one ordered merge
//!    rather than a precedence between levels.
//! 3. **`source-dirs` cannot change the dependency set.** It is read for
//!    evidence and module purposes only.
//!
//! One measured behaviour is deliberately NOT reproduced: hpack rewrites the
//! constraints it renders, turning `base >= 4 && < 5` into `base ==4.*`. That
//! is a cosmetic normalisation of an equivalent range. waybill records the
//! range **as the manifest declares it**, which is also what the `.cabal`
//! reader does for `build-depends` (milestone 895 FR-007); reproducing the
//! normalisation would be work in service of matching an artifact this feature
//! no longer reads.
//!
//! # What is not modelled, and is announced instead (FR-001b)
//!
//! `!include` and `defaults:`. Both are announced in the emitted document
//! rather than passed over, so a manifest using one yields a partial answer
//! that says it is partial.
//!
//! They are not the same kind of construct, which the probe also measured.
//! `!include` resolves from a **sibling file on disk** and needs no network.
//! `defaults:` retrieves YAML from a remote repository, and given an
//! unresolvable one hpack refuses the *whole manifest* and renders nothing:
//!
//! ```text
//! Invalid value for "defaults"! File
//! https://raw.githubusercontent.com/<owner>/<repo>/<ref>/.hpack/defaults.yaml
//! does not exist!
//! ```
//!
//! There is therefore no partial answer to reproduce offline, and FR-001c
//! forbids waybill from fetching it: a manifest reader is not the place to
//! introduce a network dependency, and it could not be honoured under
//! `--offline` at all.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{CabalManifest, CabalStanza, DeclaredDep, ManifestOrigin, StanzaKind};

// ───────────────────────────────────────────────────────────────────────
// T020 — what the reader could not do (FR-003, FR-001b)
// ───────────────────────────────────────────────────────────────────────

/// Why a `package.yaml` that was found could not be read (FR-003).
///
/// A typed enum rather than a bare string (Principle IV); rendered at the
/// emission boundary only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnreadableReason {
    /// `serde_yaml` refused the document.
    MalformedYaml(String),
    /// Read as YAML, but not as a manifest — no `name:`.
    NotAManifest(String),
    /// Could not be opened.
    Unreadable(String),
}

impl UnreadableReason {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::MalformedYaml(_) => "malformed-yaml",
            Self::NotAManifest(_) => "not-a-manifest",
            Self::Unreadable(_) => "unreadable",
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::MalformedYaml(d) | Self::NotAManifest(d) | Self::Unreadable(d) => d,
        }
    }
}

/// A construct outside FR-001a's modelled subset that could change the
/// dependency set, so a manifest using it yields a knowably partial answer
/// (FR-001b).
///
/// A `when:` whose condition cannot be evaluated is deliberately NOT here.
/// Probe A5 measured that hpack does not evaluate those either — it lowers
/// them to a Cabal `if` — and the `.cabal` reader already declines rather
/// than guesses on an unevaluable condition. That is a complete answer about
/// a conditional dependency, not a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UnmodelledConstruct {
    /// `defaults:`. Resolving it is a network fetch (FR-001c).
    Defaults,
    /// `!include`. Measured to resolve from a sibling file on disk, so a
    /// cheaper later win than `Defaults` — but still not modelled here.
    Include,
}

impl UnmodelledConstruct {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Defaults => "defaults",
            Self::Include => "include",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// T023 — what the reader did, for FR-003 / FR-005 / FR-018
// ───────────────────────────────────────────────────────────────────────

/// The hpack reader's own account of a scan.
///
/// This is the carrier that gets the unread-manifest fact from the reader to
/// the completeness pass. `compute_graph_completeness` cannot infer it:
/// `trivially_complete()` is correct for a dependency-free project and wrong
/// for one whose manifest went unread, and at that point in the code the
/// reason the component set is empty is out of scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HpackReadOutcome {
    /// Every `package.yaml` dispatched to this reader.
    pub(crate) seen: Vec<PathBuf>,
    /// Parsed and lowered.
    pub(crate) read: Vec<PathBuf>,
    /// Present, dispatched, and not read (FR-003).
    pub(crate) unreadable: Vec<(PathBuf, UnreadableReason)>,
    /// Read, but knowably partial (FR-001b).
    pub(crate) partial: Vec<(PathBuf, UnmodelledConstruct)>,
    /// A sibling hpack-generated `*.cabal` suppressed because the manifest is
    /// the source of truth (FR-002).
    pub(crate) suppressed_cabal: Vec<PathBuf>,
}

impl HpackReadOutcome {
    pub(crate) fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// The document-scope value (FR-018). Keys sort through `serde_json::Map`,
    /// which is a `BTreeMap` here, so a golden can pin the string.
    pub(crate) fn to_document_value(&self) -> serde_json::Value {
        let path = |p: &PathBuf| p.display().to_string();
        serde_json::json!({
            "manifests-seen": self.seen.iter().map(path).collect::<Vec<_>>(),
            "manifests-read": self.read.iter().map(path).collect::<Vec<_>>(),
            "manifests-unread": self
                .unreadable
                .iter()
                .map(|(p, r)| serde_json::json!({
                    "file": path(p),
                    "reason": r.as_str(),
                    "detail": r.detail(),
                }))
                .collect::<Vec<_>>(),
            "manifests-partial": self
                .partial
                .iter()
                .map(|(p, c)| serde_json::json!({
                    "file": path(p),
                    "unmodelled-construct": c.as_str(),
                }))
                .collect::<Vec<_>>(),
            "generated-cabal-suppressed": self
                .suppressed_cabal
                .iter()
                .map(path)
                .collect::<Vec<_>>(),
        })
    }
}

// ───────────────────────────────────────────────────────────────────────
// T019 — the wire types
// ───────────────────────────────────────────────────────────────────────

/// A field hpack accepts as either a scalar or a sequence.
#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
    #[default]
    None,
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s],
            Self::Many(v) => v,
            Self::None => Vec::new(),
        }
    }
}

/// A scalar hpack accepts as a string but YAML may hand us as a number.
///
/// `version: 0.1` is the shape that found this: unquoted in YAML it is a
/// **float**, and a plain `Option<String>` field rejects it with
/// `invalid type: floating point 0.1, expected a string`. Real hpack accepts
/// it, so refusing the manifest means reporting a perfectly good project as
/// unreadable.
///
/// Found by a real corpus target rather than by a fixture. Every synthetic
/// manifest in this milestone's own tests used a quoted or three-component
/// version, so 15 fixtures and a mutation pass all missed it — which is a
/// fair reminder that a fixture set written by the same person as the parser
/// shares its blind spots.
#[derive(Debug, Clone)]
struct Scalar(String);

impl<'de> serde::Deserialize<'de> for Scalar {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = serde_yaml::Value::deserialize(d)?;
        let s = match &v {
            serde_yaml::Value::String(s) => s.clone(),
            serde_yaml::Value::Number(n) => n.to_string(),
            serde_yaml::Value::Bool(b) => b.to_string(),
            other => {
                return Err(serde::de::Error::custom(format!(
                    "expected a scalar, got {other:?}"
                )))
            }
        };
        Ok(Scalar(s))
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HpackFlag {
    #[allow(dead_code)]
    description: Option<String>,
    #[allow(dead_code)]
    manual: bool,
    default: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct HpackComponent {
    dependencies: OneOrMany,
    build_tools: OneOrMany,
    /// Measured incapable of changing the dependency set (Probe A6); read for
    /// evidence and module purposes only.
    #[allow(dead_code)]
    source_dirs: OneOrMany,
    when: Vec<HpackWhen>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HpackWhen {
    /// A YAML scalar: the literal `true`/`false`, or a Cabal condition
    /// expression such as `os(darwin)` or `flag(fast)`.
    condition: Option<serde_yaml::Value>,
    dependencies: OneOrMany,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct HpackManifest {
    name: Option<Scalar>,
    version: Option<Scalar>,
    license: Option<Scalar>,
    dependencies: OneOrMany,
    build_tools: OneOrMany,
    flags: BTreeMap<String, HpackFlag>,
    when: Vec<HpackWhen>,
    library: Option<HpackComponent>,
    internal_libraries: BTreeMap<String, HpackComponent>,
    executable: Option<HpackComponent>,
    executables: BTreeMap<String, HpackComponent>,
    tests: BTreeMap<String, HpackComponent>,
    benchmarks: BTreeMap<String, HpackComponent>,
}

// ───────────────────────────────────────────────────────────────────────
// T021 — parse and lower
// ───────────────────────────────────────────────────────────────────────

/// Read one `package.yaml` and lower it into the representation the `.cabal`
/// reader already consumes.
///
/// The parse order is measured, not chosen for tidiness (T004):
///
/// 1. Parse the text to `serde_yaml::Value`. An unresolved `!include` survives
///    here as `Value::Tagged` — it is **not** an error at this level.
/// 2. Walk the `Value` for unmodelled constructs, and *remove* a tagged entry
///    so step 3 can succeed.
/// 3. Deserialize the cleaned `Value` into `HpackManifest`.
///
/// Taking the typed path first instead would fail with
/// `invalid type: string "common.yaml", expected a sequence` — an error that
/// says nothing about `!include`, and would therefore report a *partial*
/// manifest (FR-001b) as an *unparseable* one (FR-003), telling the operator
/// to fix a file that is not broken.
pub(crate) fn read_manifest(
    path: &Path,
) -> Result<(CabalManifest, Vec<UnmodelledConstruct>), UnreadableReason> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| UnreadableReason::Unreadable(e.to_string()))?;

    let mut value: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|e| UnreadableReason::MalformedYaml(e.to_string()))?;

    let mut found: Vec<UnmodelledConstruct> = Vec::new();
    if value.get("defaults").is_some_and(|v| !v.is_null()) {
        found.push(UnmodelledConstruct::Defaults);
    }
    if strip_tagged(&mut value) {
        found.push(UnmodelledConstruct::Include);
    }
    found.sort();
    found.dedup();

    let parsed: HpackManifest = serde_yaml::from_value(value)
        .map_err(|e| UnreadableReason::NotAManifest(e.to_string()))?;
    if parsed.name.is_none() {
        return Err(UnreadableReason::NotAManifest(
            "no `name:` field; not an hpack package manifest".to_string(),
        ));
    }

    Ok((lower(parsed), found))
}

/// Remove every mapping entry whose value carries a YAML tag, returning
/// whether any were found.
///
/// Removal rather than replacement: a `Value::Null` left in place would fail
/// the typed deserialize for a sequence-shaped field, where an absent key is
/// covered by `#[serde(default)]`. The dependency it named is lost, which is
/// exactly what FR-001b announces.
fn strip_tagged(v: &mut serde_yaml::Value) -> bool {
    match v {
        serde_yaml::Value::Mapping(map) => {
            let tagged: Vec<serde_yaml::Value> = map
                .iter()
                .filter(|(_, val)| matches!(val, serde_yaml::Value::Tagged(_)))
                .map(|(k, _)| k.clone())
                .collect();
            let mut any = !tagged.is_empty();
            for k in tagged {
                map.remove(&k);
            }
            for (_, val) in map.iter_mut() {
                any |= strip_tagged(val);
            }
            any
        }
        serde_yaml::Value::Sequence(seq) => {
            let mut any = false;
            for item in seq.iter_mut() {
                if matches!(item, serde_yaml::Value::Tagged(_)) {
                    any = true;
                    *item = serde_yaml::Value::Null;
                } else {
                    any |= strip_tagged(item);
                }
            }
            any
        }
        _ => false,
    }
}

/// One dependency entry as hpack writes it: `name`, or `name <constraint>`.
fn split_entry(raw: &str) -> Option<(String, Option<String>)> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let (name, rest) = match t.find(|c: char| c.is_whitespace()) {
        Some(i) => (&t[..i], t[i..].trim()),
        None => (t, ""),
    };
    if name.is_empty() {
        return None;
    }
    Some((
        name.to_lowercase(),
        if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        },
    ))
}

/// Merge dependency entries for one stanza.
///
/// **Last-wins per name, replacing the constraint entirely** (Probe A3).
/// There is no intersection: `text >= 1.0` then `text < 3` yields `< 3`, and
/// a bare entry *discards* a constraint an earlier one declared. Measured
/// both across the inheritance boundary and within a single list, so this is
/// one ordered merge rather than a precedence between levels.
fn merge_entries(ordered: &[String]) -> Vec<DeclaredDep> {
    let mut order: Vec<String> = Vec::new();
    let mut chosen: HashMap<String, Option<String>> = HashMap::new();
    for raw in ordered {
        let Some((name, range)) = split_entry(raw) else {
            continue;
        };
        if !chosen.contains_key(&name) {
            order.push(name.clone());
        }
        chosen.insert(name, range);
    }
    order
        .into_iter()
        .map(|name| {
            let range = chosen.get(&name).cloned().flatten();
            DeclaredDep {
                // Deliberately empty. Mutation M5 — zeroing this field —
                // SURVIVED, which sent me to look: the cross-stanza fold in
                // `haskell.rs` recomputes `all_ranges` from every constraint
                // collected for the name before emission, so anything set
                // here is overwritten. Leaving a populated-looking value
                // would imply this field is load-bearing on the hpack path
                // when it is not. `range` is the one that carries.
                all_ranges: Vec::new(),
                name,
                range,
                executable: None,
            }
        })
        .collect()
}

/// How a `when:` condition resolves under the manifest's own flag defaults.
enum WhenVerdict {
    /// Contributes unconditionally.
    Always,
    /// hpack prunes the block; the dependencies do not exist.
    Pruned,
    /// Not built under default flags: an optional mark, through the same
    /// `default_buildable: false` path a `buildable: False` stanza takes.
    NotInDefaultBuild,
}

/// Evaluate one `when:` condition.
///
/// Measured (Probe A5): hpack prunes a **literal `false`** and lowers
/// everything else — including `flag(x)` against a default declared in the
/// same file — to a Cabal `if`. So a YAML boolean is decided here, and any
/// other condition goes to `eval_cabal_condition`, the evaluator the `.cabal`
/// reader already uses. An unevaluable condition declines rather than
/// guesses, matching `stanza_default_buildable`.
fn verdict(cond: Option<&serde_yaml::Value>, flags: &HashMap<String, bool>) -> WhenVerdict {
    match cond {
        None => WhenVerdict::Always,
        Some(serde_yaml::Value::Bool(true)) => WhenVerdict::Always,
        Some(serde_yaml::Value::Bool(false)) => WhenVerdict::Pruned,
        Some(v) => match v.as_str() {
            None => WhenVerdict::Always,
            Some(expr) => match super::eval_cabal_condition(expr, flags) {
                Some(true) => WhenVerdict::Always,
                Some(false) => WhenVerdict::NotInDefaultBuild,
                None => WhenVerdict::Always,
            },
        },
    }
}

/// Lower a parsed manifest into `CabalManifest`.
///
/// Every downstream pass — scope assignment, the optional mark, the C151
/// reason, the main-module emission — then applies unchanged, which is the
/// point: a second representation is what would let one project get two
/// answers depending on which file declared its dependencies.
fn lower(m: HpackManifest) -> CabalManifest {
    let flags: HashMap<String, bool> = m
        .flags
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.default))
        .collect();

    let top_deps = m.dependencies.into_vec();
    let top_tools = m.build_tools.into_vec();
    let top_when = m.when;

    let mut stanzas: Vec<CabalStanza> = Vec::new();

    // Probe A1: the top-level list reaches EVERY stanza kind, and a
    // per-component list is additive. `tests:` and `benchmarks:` already map
    // to Development in `stanza_lifecycle_scope`, so they are included here
    // rather than special-cased.
    let mut components: Vec<(StanzaKind, Option<String>, HpackComponent)> = Vec::new();
    if let Some(lib) = m.library {
        components.push((StanzaKind::Library, None, lib));
    }
    for (label, c) in m.internal_libraries {
        components.push((StanzaKind::Library, Some(label), c));
    }
    if let Some(exe) = m.executable {
        components.push((StanzaKind::Executable, m.name.clone().map(|s| s.0), exe));
    }
    for (label, c) in m.executables {
        components.push((StanzaKind::Executable, Some(label), c));
    }
    for (label, c) in m.tests {
        components.push((StanzaKind::TestSuite, Some(label), c));
    }
    for (label, c) in m.benchmarks {
        components.push((StanzaKind::Benchmark, Some(label), c));
    }

    // A manifest with no component stanzas at all still declares a dependency
    // set through its top-level list; hpack would render nothing, but the
    // declaration is real and dropping it is #1278 in miniature. Model it as
    // a library.
    if components.is_empty() && !(top_deps.is_empty() && top_tools.is_empty()) {
        components.push((StanzaKind::Library, None, HpackComponent::default()));
    }

    for (kind, label, c) in components {
        let mut ordered = top_deps.clone();
        let mut tools = top_tools.clone();
        let mut conditional: Vec<String> = Vec::new();

        // Top-level `when:` applies to every stanza; the component's own
        // follows it in the merge order.
        for w in top_when.iter().chain(c.when.iter()) {
            match verdict(w.condition.as_ref(), &flags) {
                WhenVerdict::Always => {
                    ordered.extend(clone_entries(&w.dependencies));
                }
                WhenVerdict::Pruned => {}
                WhenVerdict::NotInDefaultBuild => {
                    conditional.extend(clone_entries(&w.dependencies));
                }
            }
        }
        ordered.extend(c.dependencies.into_vec());
        tools.extend(c.build_tools.into_vec());

        stanzas.push(CabalStanza {
            kind,
            label: label.clone(),
            build_depends: merge_entries(&ordered),
            build_tool_depends: merge_entries(&tools),
            default_buildable: true,
        });

        // A `when:` block excluded under the declared flag defaults becomes a
        // stanza of the same kind that is NOT default-buildable, which is the
        // shape `names_in_default_build` and the optional-mark derivation
        // already consume. This is the agreement contract (C6): the same
        // dependency declared by a `buildable: False` stanza in a `*.cabal`
        // reaches the identical classification.
        if !conditional.is_empty() {
            stanzas.push(CabalStanza {
                kind,
                label: Some(match &label {
                    Some(l) => format!("{l}-when-disabled"),
                    None => "when-disabled".to_string(),
                }),
                build_depends: merge_entries(&conditional),
                build_tool_depends: Vec::new(),
                default_buildable: false,
            });
        }
    }

    CabalManifest {
        name: m.name.map(|s| s.0),
        version: m.version.map(|s| s.0),
        license: m
            .license
            .as_ref()
            .and_then(|l| {
                waybill_common::types::license::SpdxExpression::try_canonical(&l.0).ok()
            }),
        stanzas,
        hpack_generated: false,
        skipped_entries: 0,
        origin: ManifestOrigin::Hpack,
    }
}

/// `OneOrMany` is consumed by value; a `when:` list is iterated twice (the
/// top-level one applies to every stanza), so its entries are cloned.
fn clone_entries(v: &OneOrMany) -> Vec<String> {
    match v {
        OneOrMany::One(s) => vec![s.clone()],
        OneOrMany::Many(items) => items.clone(),
        OneOrMany::None => Vec::new(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod include_tests {
    //! T004 — how `serde_yaml` treats an unresolved `!include` tag.
    //!
    //! `data-model.md` §1.1 recorded as **inference** that `serde_yaml`
    //! rejects the tag as a deserialization error, which would have made
    //! detection a scan of the document text before parsing. Measured, that
    //! inference is **wrong**, and wrong in a way that matters.

    /// `!include` survives into a `Value` as a `TaggedValue`. It is NOT an
    /// error at this level, so detection reads the parsed document rather
    /// than scanning the file text for a tag.
    #[test]
    fn t004_include_survives_into_a_value_as_a_tagged_value() {
        let doc = "name: waybill-fixture-inc\nversion: 0.1.0\nlibrary:\n  \
                   dependencies: !include \"common.yaml\"\n";
        let v: serde_yaml::Value = serde_yaml::from_str(doc).expect("parses as Value");
        let deps = v
            .get("library")
            .and_then(|l| l.get("dependencies"))
            .expect("library.dependencies present");
        let tagged = match deps {
            serde_yaml::Value::Tagged(t) => t,
            other => panic!("expected a TaggedValue, got {other:?}"),
        };
        assert_eq!(tagged.tag.to_string(), "!include");
        assert_eq!(tagged.value.as_str(), Some("common.yaml"));
    }

    /// The half that decides the parse order: deserializing an `!include`
    /// straight into a typed field **fails**, with a type error that says
    /// nothing about `!include`.
    ///
    /// Taking the typed path first would therefore report an `!include`
    /// manifest as *unparseable* (FR-003) when it is in fact *partial*
    /// (FR-001b) — two different requirements, two different signals, and
    /// the operator told to fix a file that is not broken. So the reader
    /// parses to `Value`, detects unmodelled constructs there, and
    /// deserializes the `Value` afterwards.
    #[test]
    fn t004_include_breaks_a_typed_deserialize_with_an_unrelated_error() {
        let doc = "name: waybill-fixture-inc\nversion: 0.1.0\nlibrary:\n  \
                   dependencies: !include \"common.yaml\"\n";
        #[derive(serde::Deserialize)]
        struct Lib {
            #[allow(dead_code)]
            dependencies: Vec<String>,
        }
        #[derive(serde::Deserialize)]
        struct M {
            #[allow(dead_code)]
            library: Lib,
        }
        let msg = match serde_yaml::from_str::<M>(doc) {
            Ok(_) => panic!("typed deserialize unexpectedly succeeded"),
            Err(e) => e.to_string(),
        };
        assert!(
            msg.contains("invalid type"),
            "expected a type error, got: {msg}"
        );
        assert!(
            !msg.contains("include"),
            "the error must NOT be about !include -- that is the whole point: \
             it is indistinguishable from a malformed manifest. Got: {msg}"
        );
    }
}
