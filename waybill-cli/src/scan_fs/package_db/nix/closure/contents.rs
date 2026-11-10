//! Milestone 1378 (#1396) — what is compiled into a closure member's output.
//!
//! The closure tier enumerates derivations. A Go or Rust program's
//! dependencies are not derivations: nixpkgs fetches them as one bundle and
//! compiles them into the binary. Measured, a closure scan of `gh` listed 0
//! Go modules where the binary holds 163, and of `ripgrep` 0 crates where the
//! binary records 42.
//!
//! This pass reads each member's outputs that are **already present** in the
//! store and attaches what the binaries declare. It never builds or fetches
//! (spec FR-002): realising an output runs a build, and that is a different
//! decision from reading a file that is there.
//!
//! Because presence is a property of the machine and not of the project, the
//! pass says what it did not read as loudly as what it did — per member
//! (C214) and for the document (C213). On a developer machine about one
//! output in ten is present; on a fresh runner, none.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use waybill_common::resolution::{
    ArtifactPresence, EnrichmentProvenance, Relationship, RelationshipType, ResolutionEvidence,
    ResolutionTechnique, ResolvedComponent,
};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::classify::DerivationRole;
use super::derivation::{store_basename, RawDerivation};
use super::emit;
use super::ClassifiedClosure;
use crate::scan_fs::binary::embedded::{self, SelfDescription, SelfEcosystem};
use crate::scan_fs::package_db::PackageDbEntry;

/// C213 — document scope: how many members were read.
pub(crate) const ANN_NIX_CLOSURE_CONTENTS: &str = "waybill:nix-closure-contents";
/// C214 — per member: whether its outputs were examined.
pub(crate) const ANN_CLOSURE_CONTENTS: &str = "waybill:closure-contents";
/// C215 — per member: the identity a binary in its output reports for itself.
pub(crate) const ANN_BINARY_IDENTITY: &str = "waybill:closure-binary-identity";

/// Where outputs are looked for: nix's own `NIX_STORE_DIR`, else `/nix/store`.
pub(crate) fn store_dir() -> PathBuf {
    match std::env::var_os("NIX_STORE_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from("/nix/store"),
    }
}

/// Whether a member's outputs were examined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReadState {
    /// Every declared output was present and examined. Says nothing about
    /// whether anything was found: a C program is `Read` and yields nothing.
    Read,
    /// Some outputs present and examined, some absent.
    Partial,
    /// No declared output is present.
    Absent,
    /// A present output could not be examined.
    Failed,
    /// The member declares no output path at all.
    NoOutputs,
}

impl ReadState {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Partial => "partial",
            Self::Absent => "absent",
            Self::Failed => "failed",
            Self::NoOutputs => "no-outputs",
        }
    }

    /// Whether the strict opt-in accepts this state for the closure root.
    ///
    /// `Partial` passes: which output holds the program is not knowable from
    /// the closure, and a root whose `out` was fetched while its `doc` was
    /// not has been read where it matters.
    pub(crate) fn satisfies_strict(self) -> bool {
        matches!(self, Self::Read | Self::Partial)
    }

    fn from_counts(declared: usize, present: usize, failed: bool) -> Self {
        if declared == 0 {
            Self::NoOutputs
        } else if failed {
            Self::Failed
        } else if present == 0 {
            Self::Absent
        } else if present == declared {
            Self::Read
        } else {
            Self::Partial
        }
    }
}

/// What became of the closure root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootState {
    Member(ReadState),
    /// The closure names no single root, or the root is not a component.
    None,
}

impl RootState {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Member(s) => s.wire(),
            Self::None => "none",
        }
    }
}

/// C213.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContentsRecord {
    pub(crate) members: usize,
    pub(crate) read: usize,
    pub(crate) partial: usize,
    pub(crate) absent: usize,
    pub(crate) failed: usize,
    pub(crate) no_outputs: usize,
    /// Members from which at least one module was recovered.
    pub(crate) yielded: usize,
    /// Distinct recovered modules per ecosystem. Both keys are always
    /// present: a zero says "looked, found none", which an absent key cannot.
    pub(crate) modules: BTreeMap<&'static str, usize>,
    pub(crate) root: RootState,
}

impl ContentsRecord {
    /// Any member whose contents this scan cannot vouch for.
    pub(crate) fn has_unread(&self) -> bool {
        self.absent + self.failed + self.partial > 0
    }

    pub(crate) fn is_consistent(&self) -> bool {
        self.members == self.read + self.partial + self.absent + self.failed + self.no_outputs
            && self.yielded <= self.read + self.partial + self.failed
    }

    pub(crate) fn wire(&self) -> String {
        // Inserted in sorted order so the bytes do not depend on whether
        // `serde_json` preserves insertion order.
        let mut modules = serde_json::Map::new();
        for (k, v) in &self.modules {
            modules.insert((*k).to_string(), (*v).into());
        }
        let mut o = serde_json::Map::new();
        o.insert("absent".into(), self.absent.into());
        o.insert("failed".into(), self.failed.into());
        o.insert("members".into(), self.members.into());
        o.insert("modules".into(), serde_json::Value::Object(modules));
        o.insert("no-outputs".into(), self.no_outputs.into());
        o.insert("partial".into(), self.partial.into());
        o.insert("read".into(), self.read.into());
        o.insert("root".into(), self.root.wire().into());
        o.insert("schema".into(), "waybill-nix-closure-contents/v1".into());
        o.insert("yielded".into(), self.yielded.into());
        serde_json::Value::Object(o).to_string()
    }
}

/// How a member's binary-reported identity was settled (the C215 `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityStatus {
    /// The registry knows this name at this version; the member's primary
    /// identifier is now the candidate.
    Confirmed,
    /// The registry answered and does not know it.
    Declined,
    /// Not asked, or the answer could not be had.
    Unchecked(UncheckedReason),
    /// The member's binaries name more than one program.
    Ambiguous,
    /// The closure gives the member no version to join the name to.
    NoVersion,
    /// The member already carries an ecosystem identifier.
    Existing,
    /// Another component already holds the candidate identifier.
    Collision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UncheckedReason {
    Disabled,
    Unreachable,
    Unkeyable,
}

impl UncheckedReason {
    fn wire(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Unreachable => "unreachable",
            Self::Unkeyable => "unkeyable",
        }
    }
}

impl IdentityStatus {
    fn wire(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Declined => "declined",
            Self::Unchecked(_) => "unchecked",
            Self::Ambiguous => "ambiguous",
            Self::NoVersion => "no-version",
            Self::Existing => "existing",
            Self::Collision => "collision",
        }
    }
}

/// A member whose binaries describe themselves, and what follows from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdentityFinding {
    /// Index of the member in the component slice `read` was given.
    pub(crate) member: usize,
    pub(crate) ecosystem: Option<SelfEcosystem>,
    /// The module path or crate name the binary reported.
    pub(crate) name: Option<String>,
    /// The closure's version in the form the ecosystem's registry keys on
    /// (Go: with the `v` the closure does not carry).
    pub(crate) version: Option<String>,
    pub(crate) candidate: Option<Purl>,
    /// The version the binary reports for itself, only when it differs from
    /// the closure's.
    pub(crate) binary_version: Option<String>,
    /// Settled without asking, when `Some`. `None` means: ask the registry.
    pub(crate) settled: Option<IdentityStatus>,
}

impl IdentityFinding {
    /// deps.dev's system name for this candidate.
    pub(crate) fn registry_system(&self) -> Option<&'static str> {
        Some(match self.ecosystem? {
            SelfEcosystem::Golang => "golang",
            SelfEcosystem::Cargo => "cargo",
        })
    }
}

/// What the pass found.
#[derive(Debug)]
pub(crate) struct ContentsOutcome {
    /// Recovered modules not already present in the component set.
    pub(crate) components: Vec<ResolvedComponent>,
    /// `(member index, module PURL)`. Held by index rather than by PURL
    /// because the member's PURL may still be promoted; materialise with
    /// [`ContentsOutcome::relationships`] once identities are settled.
    edges: BTreeSet<(usize, String)>,
    pub(crate) identities: Vec<IdentityFinding>,
    pub(crate) record: ContentsRecord,
    /// `name version` of the closure root, for messages.
    pub(crate) root_label: Option<String>,
}

impl ContentsOutcome {
    /// Member → module dependency edges, read against the members' final
    /// identifiers.
    pub(crate) fn relationships(&self, components: &[ResolvedComponent]) -> Vec<Relationship> {
        self.edges
            .iter()
            .filter_map(|(member, module)| {
                let from = components.get(*member)?.purl.as_str();
                (from != module).then(|| Relationship {
                    from: from.to_string(),
                    to: module.clone(),
                    // FR-007 — the ordinary dependency relationship, the one
                    // construct all three formats share with one meaning.
                    relationship_type: RelationshipType::DependsOn,
                    provenance: EnrichmentProvenance {
                        source: "nix-closure-contents".to_string(),
                        // Not `nix-build-graph`: this edge was read from the
                        // binary's own record of what was linked into it, so
                        // it is a declared edge and scope propagation may
                        // traverse it.
                        data_type: "embedded-build-info".to_string(),
                    },
                })
            })
            .collect()
    }
}

/// How a derivation and the component it became are matched.
///
/// A member that declares a `pname` is matched on `(name, version)`, the join
/// the edge builder already uses and one that survives the identity passes
/// rewriting a PURL. A `pname`-less member is matched on the hash-bearing
/// PURL it was emitted under, because 56 of them are named `source` and share
/// the pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum MemberKey {
    Named(String, String),
    Hashed(String),
}

fn key_of_derivation(drv: &RawDerivation, key: &str) -> Option<MemberKey> {
    if emit::is_hash_keyed_member(drv) {
        let (coordinate, _) = emit::coordinate(drv, key)?;
        let purl = Purl::new(&coordinate).ok()?;
        Some(MemberKey::Hashed(purl.as_str().to_string()))
    } else {
        Some(MemberKey::Named(
            drv.pname()?.to_string(),
            drv.version().unwrap_or_default().to_string(),
        ))
    }
}

fn key_of_component(c: &ResolvedComponent) -> Option<MemberKey> {
    c.extra_annotations.get(emit::ANN_CLOSURE_ROLE)?;
    if c.extra_annotations.contains_key(super::member::ANN_MEMBER_ROLE) {
        Some(MemberKey::Hashed(c.purl.as_str().to_string()))
    } else {
        Some(MemberKey::Named(c.name.clone(), c.version.clone()))
    }
}

#[derive(Default)]
struct MemberOutputs {
    outputs: Vec<PathBuf>,
    /// Any derivation behind this identity goes into the artifact.
    ships: bool,
    is_root: bool,
}

/// Read the present outputs of every closure member.
///
/// Stamps C214 on each member in `components`. Returns the recovered modules,
/// the edges to them, the identities binaries reported, and the C213 record.
pub(crate) fn read(
    closure: &ClassifiedClosure,
    components: &mut [ResolvedComponent],
    store: &Path,
) -> ContentsOutcome {
    // 1. Which outputs belong to which member.
    let mut members: BTreeMap<MemberKey, MemberOutputs> = BTreeMap::new();
    // Milestone 1377 (#1398) — the evaluated package: the closure's sole
    // unconsumed derivation, emitted whatever its role. `None` when several
    // derivations are unconsumed and the closure does not say which was
    // evaluated; then no member is the root.
    let evaluated = closure.graph.evaluated.as_deref();
    for (key, drv) in &closure.raw.derivations {
        let Some(role) = closure.roles.get(key).copied() else {
            continue;
        };
        let is_root = evaluated == Some(key.as_str());
        if !is_root && !emit::emits_as_component(role, drv) {
            continue;
        }
        let Some(mk) = key_of_derivation(drv, key) else {
            continue;
        };
        let slot = members.entry(mk).or_default();
        slot.outputs
            .extend(drv.output_paths().map(|p| store.join(store_basename(p))));
        // The evaluated package is the artifact, whatever its role says
        // about who references it (nothing does).
        slot.ships |=
            is_root || matches!(role, DerivationRole::ArtifactInput | DerivationRole::Both);
        slot.is_root |= is_root;
    }

    // 2. Which component each member became. First match wins, as it does
    //    for the edge builder's join.
    let mut index: HashMap<MemberKey, usize> = HashMap::new();
    for (i, c) in components.iter().enumerate() {
        if let Some(k) = key_of_component(c) {
            index.entry(k).or_insert(i);
        }
    }
    let existing_purls: HashMap<String, usize> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.purl.as_str().to_string(), i))
        .collect();

    let mut record = ContentsRecord {
        members: 0,
        read: 0,
        partial: 0,
        absent: 0,
        failed: 0,
        no_outputs: 0,
        yielded: 0,
        modules: BTreeMap::from([("cargo", 0usize), ("golang", 0usize)]),
        root: RootState::None,
    };
    let mut root_label = None;
    let mut edges: BTreeSet<(usize, String)> = BTreeSet::new();
    // module PURL → (entry, reached by a member that ships)
    let mut recovered: BTreeMap<String, (PackageDbEntry, bool)> = BTreeMap::new();
    let mut identities = Vec::new();

    // 3. Read.
    for (mk, m) in &members {
        let Some(&member) = index.get(mk) else {
            // Emitted by the closure tier but not found in the slice: the
            // name yielded no valid PURL. Nothing to stamp or attach to.
            continue;
        };
        let declared = m.outputs.len();
        let mut present = 0usize;
        let mut failed = false;
        let mut selfs: Vec<SelfDescription> = Vec::new();
        let mut yielded = false;
        for output in &m.outputs {
            // `symlink_metadata`, so an output that is itself a symlink
            // counts as present without following it out of the store.
            if output.symlink_metadata().is_err() {
                continue;
            }
            present += 1;
            let found = embedded::embedded_contents(output);
            failed |= found.failed;
            selfs.extend(found.self_descriptions);
            for entry in found.modules {
                yielded = true;
                let purl = entry.purl.as_str().to_string();
                edges.insert((member, purl.clone()));
                recovered
                    .entry(purl)
                    .and_modify(|(_, ships)| *ships |= m.ships)
                    .or_insert((entry, m.ships));
            }
        }

        let state = ReadState::from_counts(declared, present, failed);
        record.members += 1;
        match state {
            ReadState::Read => record.read += 1,
            ReadState::Partial => record.partial += 1,
            ReadState::Absent => record.absent += 1,
            ReadState::Failed => record.failed += 1,
            ReadState::NoOutputs => record.no_outputs += 1,
        }
        record.yielded += usize::from(yielded);
        if m.is_root {
            record.root = RootState::Member(state);
            let c = &components[member];
            root_label = Some(if c.version.is_empty() {
                c.name.clone()
            } else {
                format!("{} {}", c.name, c.version)
            });
        }
        components[member].extra_annotations.insert(
            ANN_CLOSURE_CONTENTS.to_string(),
            serde_json::Value::String(state.wire().to_string()),
        );

        if let Some(finding) = identity_finding(member, &components[member], &selfs, &existing_purls)
        {
            identities.push(finding);
        }
    }

    // 4. Recovered modules become components, unless the document already
    //    has one at that identifier — then only the edge is new.
    let mut new_components = Vec::new();
    for (purl, (entry, ships)) in recovered {
        if let Some(kind) = module_ecosystem(&entry.purl) {
            *record.modules.entry(kind).or_default() += 1;
        }
        if existing_purls.contains_key(&purl) {
            continue;
        }
        new_components.push(component_from_entry(&entry, ships));
    }

    debug_assert!(record.is_consistent(), "C213 counts must add up: {record:?}");
    ContentsOutcome {
        components: new_components,
        edges,
        identities,
        record,
        root_label,
    }
}

fn module_ecosystem(purl: &Purl) -> Option<&'static str> {
    match purl.ecosystem() {
        "golang" => Some("golang"),
        "cargo" => Some("cargo"),
        _ => None,
    }
}

/// The component a recovered module becomes.
///
/// Mirrors the field propagation the scan path applies to a reader entry
/// (`scan_fs/mod.rs`), so a module recovered here is the component a
/// standalone scan of the same binary would emit. A test pins the two
/// together field by field, because a mirror is exactly the kind of second
/// copy that drifts.
fn component_from_entry(entry: &PackageDbEntry, ships: bool) -> ResolvedComponent {
    ResolvedComponent {
        name: entry.name.clone(),
        version: entry.version.clone(),
        purl: entry.purl.clone(),
        evidence: ResolutionEvidence {
            technique: ResolutionTechnique::PackageDatabase,
            confidence: crate::scan_fs::PACKAGE_DB_CONFIDENCE,
            source_connection_ids: vec![],
            // The binary's absolute store path. There is no scan root to be
            // relative to: the output lives outside the scanned project.
            source_file_paths: std::iter::once(&entry.source_path)
                .chain(&entry.extra_source_paths)
                .cloned()
                .collect(),
            deps_dev_match: None,
        },
        licenses: entry.licenses.clone(),
        concluded_licenses: Vec::new(),
        hashes: entry.hashes.clone(),
        supplier: entry
            .maintainer
            .clone()
            .or_else(|| crate::scan_fs::supplier_from_purl(&entry.purl)),
        cpes: vec![],
        advisories: vec![],
        occurrences: vec![],
        lifecycle_scope: entry.lifecycle_scope,
        build_inclusion: entry.build_inclusion,
        requirement_ranges: entry.requirement_ranges.clone(),
        source_type: entry.source_type.clone(),
        sbom_tier: entry.sbom_tier.clone(),
        buildinfo_status: entry.buildinfo_status.clone(),
        evidence_kind: entry.evidence_kind.clone(),
        binary_class: entry.binary_class.clone(),
        binary_stripped: entry.binary_stripped,
        linkage_kind: entry.linkage_kind.clone(),
        detected_go: entry.detected_go,
        confidence: entry.confidence.clone(),
        binary_packed: entry.binary_packed.clone(),
        npm_role: entry.npm_role.clone(),
        raw_version: entry.raw_version.clone(),
        parent_purl: None,
        co_owned_by: entry.co_owned_by.clone(),
        shade_relocation: entry.shade_relocation,
        external_references: crate::scan_fs::external_refs_from_purl(
            &entry.purl,
            &entry.extra_annotations,
        ),
        extra_annotations: entry.extra_annotations.clone(),
        binary_role: entry.binary_role,
        // FR-010 — a module reached only through build-only members is not
        // in the artifact; one any shipped member reaches is left unclaimed,
        // exactly as a shipped member itself is.
        artifact_presence: (!ships).then_some(ArtifactPresence::NotInArtifact),
    }
}

/// What a member's binaries say the member is, if anything.
fn identity_finding(
    member: usize,
    component: &ResolvedComponent,
    selfs: &[SelfDescription],
    existing_purls: &HashMap<String, usize>,
) -> Option<IdentityFinding> {
    let distinct: BTreeSet<(SelfEcosystem, &str)> =
        selfs.iter().map(|s| (s.ecosystem, s.name.as_str())).collect();
    let mut it = distinct.iter();
    let (ecosystem, name) = *it.next()?;
    let mut finding = IdentityFinding {
        member,
        ecosystem: None,
        name: None,
        version: None,
        candidate: None,
        binary_version: None,
        settled: None,
    };
    if it.next().is_some() {
        // Measured: `cargo-auditable` ships two binaries with different root
        // crates. Neither is "the" program.
        finding.settled = Some(IdentityStatus::Ambiguous);
        return Some(finding);
    }
    finding.ecosystem = Some(ecosystem);
    finding.name = Some(name.to_string());

    let closure_version = component.version.trim();
    if closure_version.is_empty() {
        finding.settled = Some(IdentityStatus::NoVersion);
        return Some(finding);
    }
    let bare = closure_version.strip_prefix('v').unwrap_or(closure_version);
    // Measured against deps.dev: Go's registry keys on `v2.93.0` and answers
    // the closure's `2.93.0` with the same 404 it gives a version that does
    // not exist. cargo keys on the bare form.
    let registry_version = match ecosystem {
        SelfEcosystem::Golang => format!("v{bare}"),
        SelfEcosystem::Cargo => bare.to_string(),
    };
    let coordinate = match ecosystem {
        SelfEcosystem::Golang => format!(
            "pkg:golang/{}@{}",
            name.split('/').map(encode_purl_segment).collect::<Vec<_>>().join("/"),
            encode_purl_segment(&registry_version)
        ),
        SelfEcosystem::Cargo => format!(
            "pkg:cargo/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(&registry_version)
        ),
    };
    finding.version = Some(registry_version);
    finding.candidate = Purl::new(&coordinate).ok();

    // The version the binary reports for itself, when it disagrees.
    finding.binary_version = selfs
        .iter()
        .filter_map(|s| s.version.as_deref())
        .find(|v| v.strip_prefix('v').unwrap_or(v) != bare)
        .map(str::to_string);

    let Some(candidate) = &finding.candidate else {
        finding.settled = Some(IdentityStatus::Unchecked(UncheckedReason::Unkeyable));
        return Some(finding);
    };
    if component.purl.ecosystem() != "generic" {
        finding.settled = Some(IdentityStatus::Existing);
    } else if existing_purls
        .get(candidate.as_str())
        .is_some_and(|&holder| holder != member)
    {
        finding.settled = Some(IdentityStatus::Collision);
    }
    Some(finding)
}

/// Record how a finding was settled, promoting the member when confirmed.
///
/// The one place the member's identifier changes, and it changes only for
/// [`IdentityStatus::Confirmed`] (FR-012b): an unconfirmed join is recorded
/// and never presented as the identifier.
pub(crate) fn apply_identity(
    components: &mut [ResolvedComponent],
    finding: &IdentityFinding,
    status: IdentityStatus,
) {
    let Some(component) = components.get_mut(finding.member) else {
        return;
    };
    let mut o = serde_json::Map::new();
    if let Some(v) = &finding.binary_version {
        o.insert("binary-version".into(), v.clone().into());
    }
    if let Some(c) = &finding.candidate {
        o.insert("candidate".into(), c.as_str().into());
    }
    if let IdentityStatus::Unchecked(reason) = status {
        o.insert("reason".into(), reason.wire().into());
    }
    o.insert("status".into(), status.wire().into());
    component.extra_annotations.insert(
        ANN_BINARY_IDENTITY.to_string(),
        serde_json::Value::String(serde_json::Value::Object(o).to_string()),
    );
    if status == IdentityStatus::Confirmed {
        if let Some(candidate) = &finding.candidate {
            component.purl = candidate.clone();
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::classify::classify;
    use super::super::derivation::RawClosure;
    use super::*;
    use crate::scan_fs::binary::embedded::fixtures;

    /// A root program, a shipped library and a build tool. Bare-basename
    /// output paths, as nix writes them.
    const APP: &str = r#"{
      "derivations": {
        "d-app.drv": {
          "env": {"pname":"waybill-fixture-app","version":"1.2.3",
                  "buildInputs":"/nix/store/h-lib","nativeBuildInputs":"/nix/store/h-tool"},
          "outputs": {"out":{"path":"h-app"}},
          "inputs": {"drvs": {"d-lib.drv": {"outputs":["out"]}, "d-tool.drv": {"outputs":["out"]}}}
        },
        "d-lib.drv":  { "env": {"pname":"waybill-fixture-lib","version":"2.0.0"},
                        "outputs": {"out":{"path":"h-lib"}, "doc":{"path":"h-lib-doc"}} },
        "d-tool.drv": { "env": {"pname":"waybill-fixture-tool","version":"3.0.0"},
                        "outputs": {"out":{"path":"h-tool"}} }
      }
    }"#;

    fn classified(json: &str) -> ClassifiedClosure {
        let raw = RawClosure::parse(json).unwrap();
        let roles = classify(&raw);
        let graph = super::super::graph::project(&raw, &emit::emitted_drv_keys(&raw, &roles));
        ClassifiedClosure {
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph,
            identifiers: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
        }
    }

    fn go(main: &str, deps: &[(&str, &str)]) -> Vec<u8> {
        fixtures::go_binary(main, "(devel)", deps)
    }

    fn put(store: &Path, output: &str, file: &str, bytes: &[u8]) {
        let dir = store.join(output).join("bin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), bytes).unwrap();
    }

    fn state_of<'a>(c: &'a [ResolvedComponent], name: &str) -> Option<&'a str> {
        c.iter()
            .find(|x| x.name == name)?
            .extra_annotations
            .get(ANN_CLOSURE_CONTENTS)?
            .as_str()
    }

    fn role_of<'a>(c: &'a [ResolvedComponent], name: &str) -> Option<&'a str> {
        c.iter()
            .find(|x| x.name == name)?
            .extra_annotations
            .get(emit::ANN_CLOSURE_ROLE)?
            .as_str()
    }

    /// The precondition this pass stands on, delivered by milestone 1377
    /// (#1398): the evaluated package is a component, so the modules compiled
    /// into it have something to attach to. Pinned here as well as there
    /// because if it stopped holding, every test below about the root would
    /// pass over a member that is not in the slice.
    #[test]
    fn the_closure_root_is_a_component_and_its_edges_are_anchored() {
        let cc = classified(APP);
        let c = emit::components(&cc);
        assert_eq!(
            role_of(&c, "waybill-fixture-app"),
            Some("unreferenced"),
            "emitted because it is the evaluated package, not because of its role"
        );
        assert_eq!(role_of(&c, "waybill-fixture-lib"), Some("artifact-input"));
        assert_eq!(role_of(&c, "waybill-fixture-tool"), Some("build-tooling"));
        let (edges, unanchored) = super::super::graph::relationships(&cc.graph, &cc.raw, &c);
        assert_eq!(unanchored, 0, "the root resolves, so nothing is dropped");
        assert_eq!(
            edges
                .iter()
                .filter(|e| e.from == "pkg:generic/waybill-fixture-app@1.2.3")
                .count(),
            2,
            "root → lib and root → tool"
        );
    }

    #[test]
    fn an_empty_store_reads_nothing_and_says_so() {
        let store = tempfile::tempdir().unwrap();
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());

        assert!(out.components.is_empty());
        assert!(out.relationships(&c).is_empty());
        assert_eq!(out.record.members, 3);
        assert_eq!(out.record.absent, 3);
        assert_eq!(out.record.read, 0);
        assert_eq!(out.record.root, RootState::Member(ReadState::Absent));
        assert!(out.record.has_unread());
        assert_eq!(out.root_label.as_deref(), Some("waybill-fixture-app 1.2.3"));
        for name in ["waybill-fixture-app", "waybill-fixture-lib", "waybill-fixture-tool"] {
            assert_eq!(state_of(&c, name), Some("absent"), "{name}");
        }
        assert_eq!(
            out.record.wire(),
            r#"{"absent":3,"failed":0,"members":3,"modules":{"cargo":0,"golang":0},"no-outputs":0,"partial":0,"read":0,"root":"absent","schema":"waybill-nix-closure-contents/v1","yielded":0}"#
        );
    }

    #[test]
    fn a_present_root_yields_its_modules_as_its_dependencies() {
        let store = tempfile::tempdir().unwrap();
        put(
            store.path(),
            "h-app",
            "app",
            &go(
                "example.test/waybill-fixture-app",
                &[("example.test/waybill-fixture-liba", "v1.2.3")],
            ),
        );
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());

        let mut purls: Vec<&str> = out.components.iter().map(|x| x.purl.as_str()).collect();
        purls.sort_unstable();
        assert_eq!(
            purls,
            vec![
                "pkg:golang/example.test/waybill-fixture-liba@v1.2.3",
                "pkg:golang/stdlib@v1.22.1",
            ]
        );
        let rels = out.relationships(&c);
        assert_eq!(rels.len(), 2);
        assert!(rels
            .iter()
            .all(|r| r.from == "pkg:generic/waybill-fixture-app@1.2.3"
                && r.relationship_type == RelationshipType::DependsOn));
        assert_eq!(state_of(&c, "waybill-fixture-app"), Some("read"));
        assert_eq!(out.record.read, 1);
        assert_eq!(out.record.yielded, 1);
        assert_eq!(out.record.modules["golang"], 2);
        assert_eq!(out.record.modules["cargo"], 0);
        assert_eq!(out.record.root, RootState::Member(ReadState::Read));
        assert!(
            out.components.iter().all(|x| x.artifact_presence.is_none()
                && x.parent_purl.is_none()),
            "the root ships, so its modules are not marked absent; and nothing nests"
        );
    }

    /// FR-014 — read-and-empty is `read`, not `absent`.
    #[test]
    fn a_present_output_with_no_binary_is_read_and_yields_nothing() {
        let store = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(store.path().join("h-app/share")).unwrap();
        std::fs::write(store.path().join("h-app/share/README"), "text").unwrap();
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(state_of(&c, "waybill-fixture-app"), Some("read"));
        assert_eq!(out.record.read, 1);
        assert_eq!(out.record.yielded, 0);
        assert!(out.components.is_empty());
    }

    #[test]
    fn some_outputs_present_is_partial() {
        let store = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(store.path().join("h-lib")).unwrap();
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(
            state_of(&c, "waybill-fixture-lib"),
            Some("partial"),
            "`out` present, `doc` absent"
        );
        assert_eq!(out.record.partial, 1);
        assert!(ReadState::Partial.satisfies_strict());
        assert!(!ReadState::Absent.satisfies_strict());
        assert!(!ReadState::Failed.satisfies_strict());
        assert!(!ReadState::NoOutputs.satisfies_strict());
    }

    /// An output may be a single file (measured: `signing-utils`).
    #[test]
    fn a_single_file_output_is_read() {
        let store = tempfile::tempdir().unwrap();
        std::fs::write(
            store.path().join("h-app"),
            go(
                "example.test/waybill-fixture-app",
                &[("example.test/waybill-fixture-liba", "v1.2.3")],
            ),
        )
        .unwrap();
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(state_of(&c, "waybill-fixture-app"), Some("read"));
        assert_eq!(out.components.len(), 2);
    }

    #[test]
    fn a_member_with_no_declared_output_path_is_no_outputs() {
        const C: &str = r#"{
          "derivations": {
            "d-app.drv": {
              "env": {"pname":"waybill-fixture-app","version":"1.2.3"},
              "outputs": {"out":{}}
            }
          }
        }"#;
        let store = tempfile::tempdir().unwrap();
        let cc = classified(C);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(state_of(&c, "waybill-fixture-app"), Some("no-outputs"));
        assert_eq!(out.record.no_outputs, 1);
        assert_eq!(out.record.root, RootState::Member(ReadState::NoOutputs));
    }

    /// FR-008 and FR-010 together.
    #[test]
    fn a_module_shared_by_a_shipped_and_a_build_only_member_is_one_shipped_component() {
        let store = tempfile::tempdir().unwrap();
        let shared = ("example.test/waybill-fixture-shared", "v1.0.0");
        put(store.path(), "h-app", "app", &go("example.test/waybill-fixture-app", &[shared]));
        put(
            store.path(),
            "h-tool",
            "tool",
            &go(
                "example.test/waybill-fixture-tool",
                &[shared, ("example.test/waybill-fixture-toolonly", "v0.1.0")],
            ),
        );
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());

        let presence = |purl: &str| {
            out.components
                .iter()
                .find(|x| x.purl.as_str() == purl)
                .map(|x| x.artifact_presence)
        };
        assert_eq!(
            out.components
                .iter()
                .filter(|x| x.name == "example.test/waybill-fixture-shared")
                .count(),
            1,
            "one component however many members reach it"
        );
        assert_eq!(
            presence("pkg:golang/example.test/waybill-fixture-shared@v1.0.0"),
            Some(None),
            "a shipped member reaches it, so it is not marked absent"
        );
        assert_eq!(
            presence("pkg:golang/example.test/waybill-fixture-toolonly@v0.1.0"),
            Some(Some(ArtifactPresence::NotInArtifact)),
            "only the build tool reaches it"
        );
        let to_shared = out
            .relationships(&c)
            .into_iter()
            .filter(|r| r.to.ends_with("waybill-fixture-shared@v1.0.0"))
            .count();
        assert_eq!(to_shared, 2, "an edge from each member");
        assert_eq!(out.record.yielded, 2);
    }

    #[test]
    fn one_module_at_two_versions_under_two_members_is_two_components() {
        let store = tempfile::tempdir().unwrap();
        put(
            store.path(),
            "h-app",
            "app",
            &go(
                "example.test/waybill-fixture-app",
                &[("example.test/waybill-fixture-dep", "v1.0.0")],
            ),
        );
        put(
            store.path(),
            "h-tool",
            "tool",
            &go(
                "example.test/waybill-fixture-tool",
                &[("example.test/waybill-fixture-dep", "v2.0.0")],
            ),
        );
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(
            out.components
                .iter()
                .filter(|x| x.name == "example.test/waybill-fixture-dep")
                .count(),
            2
        );
    }

    /// FR-005 — the mirror in `component_from_entry` against the entry the
    /// standalone reader builds, field by field.
    #[test]
    fn a_recovered_module_carries_what_the_reader_entry_carries() {
        let store = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(store.path().join("h-app/bin")).unwrap();
        std::fs::write(
            store.path().join("h-app/bin/app"),
            fixtures::cargo_binary(&[
                ("waybill-fixture-app", "1.2.3", "local", "runtime"),
                ("waybill-fixture-dep", "0.5.0", "crates.io", "runtime"),
                ("waybill-fixture-build", "2.0.0", "crates.io", "build"),
            ]),
        )
        .unwrap();
        let entries = embedded::embedded_contents(&store.path().join("h-app")).modules;
        assert_eq!(entries.len(), 2, "CONTROL: the reader produced entries to compare");

        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(out.components.len(), entries.len());
        for e in &entries {
            let got = out
                .components
                .iter()
                .find(|x| x.purl == e.purl)
                .unwrap_or_else(|| panic!("{} missing", e.purl.as_str()));
            assert_eq!(got.name, e.name);
            assert_eq!(got.version, e.version);
            assert_eq!(got.sbom_tier, e.sbom_tier);
            assert_eq!(got.evidence_kind, e.evidence_kind);
            assert_eq!(got.confidence, e.confidence);
            assert_eq!(got.extra_annotations, e.extra_annotations);
            assert_eq!(got.evidence.source_file_paths, vec![e.source_path.clone()]);
            assert_eq!(got.parent_purl, None);
        }
        assert_eq!(out.record.modules["cargo"], 2);
    }

    // ---- identity -------------------------------------------------------

    fn finding_for(binary: &[u8]) -> (Vec<ResolvedComponent>, ContentsOutcome) {
        let store = tempfile::tempdir().unwrap();
        put(store.path(), "h-app", "app", binary);
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        (c, out)
    }

    #[test]
    fn a_go_candidate_joins_the_binarys_path_to_the_closures_version_with_a_v() {
        let (c, out) = finding_for(&go("example.test/waybill-fixture-app", &[]));
        assert_eq!(out.identities.len(), 1);
        let f = &out.identities[0];
        assert_eq!(c[f.member].name, "waybill-fixture-app");
        assert_eq!(
            f.candidate.as_ref().map(|p| p.as_str()),
            Some("pkg:golang/example.test/waybill-fixture-app@v1.2.3")
        );
        assert_eq!(f.version.as_deref(), Some("v1.2.3"));
        assert_eq!(f.registry_system(), Some("golang"));
        assert_eq!(f.binary_version, None, "`(devel)` is not a disagreement");
        assert_eq!(f.settled, None, "to be asked");
    }

    #[test]
    fn a_cargo_candidate_keeps_the_bare_version_and_records_a_disagreement() {
        let (_, out) = finding_for(&fixtures::cargo_binary(&[(
            "waybill-fixture-app",
            "1.2.2",
            "local",
            "runtime",
        )]));
        let f = &out.identities[0];
        assert_eq!(
            f.candidate.as_ref().map(|p| p.as_str()),
            Some("pkg:cargo/waybill-fixture-app@1.2.3"),
            "the closure's version, not the binary's"
        );
        assert_eq!(f.binary_version.as_deref(), Some("1.2.2"));
        assert!(
            out.components.is_empty(),
            "the root crate is the program: it is never a second component"
        );
    }

    #[test]
    fn two_programs_in_one_member_are_ambiguous() {
        let store = tempfile::tempdir().unwrap();
        put(store.path(), "h-app", "one", &go("example.test/waybill-fixture-one", &[]));
        put(store.path(), "h-app", "two", &go("example.test/waybill-fixture-two", &[]));
        let cc = classified(APP);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(out.identities[0].settled, Some(IdentityStatus::Ambiguous));
        assert_eq!(out.identities[0].candidate, None);
    }

    #[test]
    fn a_member_without_a_version_has_nothing_to_join() {
        const C: &str = r#"{
          "derivations": {
            "d-app.drv": {
              "env": {"pname":"waybill-fixture-app"},
              "outputs": {"out":{"path":"h-app"}}
            }
          }
        }"#;
        let store = tempfile::tempdir().unwrap();
        put(store.path(), "h-app", "app", &go("example.test/waybill-fixture-app", &[]));
        let cc = classified(C);
        let mut c = emit::components(&cc);
        let out = read(&cc, &mut c, store.path());
        assert_eq!(out.identities[0].settled, Some(IdentityStatus::NoVersion));
    }

    #[test]
    fn only_a_confirmed_identity_becomes_the_identifier() {
        let (mut c, out) = finding_for(&go(
            "example.test/waybill-fixture-app",
            &[("example.test/waybill-fixture-liba", "v1.2.3")],
        ));
        let f = &out.identities[0];
        let ann = |c: &[ResolvedComponent]| {
            c[f.member]
                .extra_annotations
                .get(ANN_BINARY_IDENTITY)
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };

        apply_identity(&mut c, f, IdentityStatus::Declined);
        assert_eq!(c[f.member].purl.as_str(), "pkg:generic/waybill-fixture-app@1.2.3");
        assert_eq!(
            ann(&c).as_deref(),
            Some(r#"{"candidate":"pkg:golang/example.test/waybill-fixture-app@v1.2.3","status":"declined"}"#)
        );

        apply_identity(&mut c, f, IdentityStatus::Unchecked(UncheckedReason::Disabled));
        assert_eq!(c[f.member].purl.as_str(), "pkg:generic/waybill-fixture-app@1.2.3");
        assert_eq!(
            ann(&c).as_deref(),
            Some(r#"{"candidate":"pkg:golang/example.test/waybill-fixture-app@v1.2.3","reason":"disabled","status":"unchecked"}"#)
        );

        apply_identity(&mut c, f, IdentityStatus::Confirmed);
        assert_eq!(
            c[f.member].purl.as_str(),
            "pkg:golang/example.test/waybill-fixture-app@v1.2.3"
        );
        // The edges follow the promoted identifier, because they are held by
        // index until now.
        let rels = out.relationships(&c);
        assert!(!rels.is_empty());
        assert!(rels
            .iter()
            .all(|r| r.from == "pkg:golang/example.test/waybill-fixture-app@v1.2.3"));
        assert_eq!(
            c.iter().filter(|x| x.name == "waybill-fixture-app").count(),
            1,
            "one component describes the program"
        );
    }

    #[test]
    fn store_dir_defaults_to_the_conventional_location() {
        // Not asserting the env-set branch here: mutating the environment in
        // a unit test races every other test in the process. The integration
        // tests set it on the child process instead.
        if std::env::var_os("NIX_STORE_DIR").is_none() {
            assert_eq!(store_dir(), PathBuf::from("/nix/store"));
        }
    }
}
