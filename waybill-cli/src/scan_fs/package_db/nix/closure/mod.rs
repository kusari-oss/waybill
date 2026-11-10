//! Milestone 1035 (#1034, #1040) — the Nix derivation closure as SBOM content.
//!
//! Milestone 1034's tier asks nix for package *versions*. This asks for the
//! whole derivation graph, which holds two things that tier cannot reach.
//!
//! **Components no manifest mentions.** Measured on two real Haskell
//! libraries: 216 and 218 artifact inputs present in the closure and absent
//! from today's output, against the 53 and 190 components waybill emits.
//!
//! **Evidence that a vulnerability was already fixed.** nixpkgs backports
//! security patches without moving the version string — `unzip 6.0` carries 11
//! CVEs across 26 patches in both closures. No version-keyed SBOM can express
//! that in either direction.
//!
//! Unlike milestone 1034, this evaluates the **scanned project's own flake**,
//! so repository-authored expressions do run. The safety machinery in
//! `super::eval` — pre-flight, argv guard, budget, degradation reasons — is
//! reused unchanged and becomes load-bearing rather than precautionary.

pub(crate) mod classify;
/// Milestone 1378 (#1396) — what is compiled into a member's output.
pub(crate) mod contents;
pub(crate) mod derivation;
pub(crate) mod ecosystem_identity;
pub(crate) mod emit;
pub(crate) mod graph;
pub(crate) mod identifiers;
/// Milestone 1282 — the four-role vocabulary for members declaring no
/// `pname`. Distinct from [`classify`], which assigns the C182 role to every
/// derivation; see that module's own doc comment for why they are separate.
pub(crate) mod member;
/// Milestone 1423 — the derivation ↔ component join, shared by `contents` and
/// the scope pass.
pub(crate) mod member_key;
pub(crate) mod patches;
/// Milestone 1423 (#1423) — the evaluated output's runtime closure.
pub(crate) mod runtime;
/// Milestone 1423 (#1423) — Haskell libraries statically linked into a
/// runtime member.
pub(crate) mod static_link;
pub(crate) mod summary;
/// Milestone 1423 (#1423), US2 — the reachability walk used when no store
/// holds the evaluated output.
pub(crate) mod walk;

use std::path::Path;
use std::time::Duration;

pub(crate) mod licenses;

use super::eval::invoke::{
    argv_is_safe, is_safe_attribute_name, run_bounded, verify_ifd_refusal_with_config,
};
use super::eval::preflight::IFD_SETTING;
use super::eval::reason::DegradationReason;
use super::eval::stderr::{classify_failure, salient_stderr};
use super::eval::result::NixSystem;
use classify::DerivationRole;
use derivation::RawClosure;

/// Default budget for acquiring and querying a closure.
///
/// **Provisional**, and labelled so for the same reason milestone 1034's is:
/// the budget spans acquisition as well as query, because nix fetches during
/// evaluation and there is no seam to bound separately. Measured warm-store
/// cost is 1.09s / 5.7MB and 1.07s / 7.4MB on two real projects; cold-store
/// cost is unmeasured and needs a clean runner (research T-R3).
pub(crate) const PROVISIONAL_CLOSURE_BUDGET_SECS: u64 = 300;

/// Milestone 1066 (#1052, research R1): first segments that make a
/// `--nix-closure-attr` value a full output path. A flake's own top-level
/// outputs cannot be listed in pure mode (`nix eval <flake>#` resolves to the
/// default package), so the rule is lexical over the standard output names.
const STANDARD_OUTPUTS: &[&str] = &[
    "packages",
    "legacyPackages",
    "checks",
    "devShells",
    "apps",
    "formatter",
    "overlays",
    "hydraJobs",
    "templates",
    "lib",
    "nixosConfigurations",
    "nixosModules",
    "darwinConfigurations",
    "darwinModules",
    "homeConfigurations",
    "homeManagerModules",
    "defaultPackage",
    "defaultApp",
    "devShell",
];

/// What `--nix-closure-attr` asked for (contracts/attribute-selection.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttrRequest {
    /// Flag absent: `packages.<system>.default`, or a sole system
    /// configuration when the flake has no `packages` output (FR-002).
    Auto,
    /// A name under `packages.<system>`: today's meaning.
    PackageName(String),
    /// A full output path, evaluated as given.
    FullPath(String),
}

pub(crate) fn classify_attr(value: Option<&str>) -> AttrRequest {
    let Some(v) = value else {
        return AttrRequest::Auto;
    };
    match v.split_once('.') {
        Some((first, _)) if STANDARD_OUTPUTS.contains(&first) => {
            AttrRequest::FullPath(v.to_string())
        }
        _ => AttrRequest::PackageName(v.to_string()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ConfigKind {
    Darwin,
    Nixos,
}

impl ConfigKind {
    fn output(self) -> &'static str {
        match self {
            Self::Darwin => "darwinConfigurations",
            Self::Nixos => "nixosConfigurations",
        }
    }
}

/// A named entry under `darwinConfigurations` or `nixosConfigurations`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SystemConfiguration {
    pub(crate) kind: ConfigKind,
    pub(crate) name: String,
}

impl SystemConfiguration {
    fn qualified(&self) -> String {
        format!("{}.{}", self.kind.output(), self.name)
    }

    /// The output whose build closure describes the machine.
    pub(crate) fn system_path(&self) -> String {
        match self.kind {
            ConfigKind::Darwin => format!("{}.system", self.qualified()),
            ConfigKind::Nixos => format!("{}.config.system.build.toplevel", self.qualified()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selection {
    PackageDefault,
    Configuration(SystemConfiguration),
    Degrade(DegradationReason),
}

/// The auto-selection rule (FR-002, contracts/attribute-selection.md).
///
/// `packages_output_present` is whether the flake has a `packages` output for
/// *any* platform, not the host's: deciding on the host's platform made the
/// same flake choose differently on different machines (analysis I1). The
/// listings are `None` when the output is absent or could not be listed.
/// Names that could not safely reach `nix` are dropped before counting (R6).
pub(crate) fn select(
    packages_output_present: bool,
    darwin: Option<Vec<String>>,
    nixos: Option<Vec<String>>,
) -> Selection {
    if packages_output_present {
        return Selection::PackageDefault;
    }
    let mut found: Vec<SystemConfiguration> = Vec::new();
    for (kind, names) in [(ConfigKind::Darwin, darwin), (ConfigKind::Nixos, nixos)] {
        for name in names.unwrap_or_default() {
            if is_safe_attribute_name(&name) {
                found.push(SystemConfiguration { kind, name });
            } else {
                tracing::info!(
                    output = kind.output(),
                    "nix-closure: ignoring a configuration whose name cannot be passed to nix safely"
                );
            }
        }
    }
    found.sort();
    match found.len() {
        0 => Selection::Degrade(DegradationReason::NoEvaluableAttribute),
        1 => Selection::Configuration(found.remove(0)),
        _ => Selection::Degrade(DegradationReason::AmbiguousSystemConfiguration {
            names: found
                .iter()
                .map(SystemConfiguration::qualified)
                .collect::<Vec<_>>()
                .join(", "),
            example: found[0].system_path(),
        }),
    }
}

/// What the operator asked for.
#[derive(Debug, Clone)]
pub(crate) struct ClosureConfig {
    /// What `--nix-closure-attr` named, classified (m1066).
    pub(crate) attribute: AttrRequest,
    pub(crate) budget: Duration,
}

impl ClosureConfig {
    pub(crate) fn from_flags(attribute: Option<String>, budget_secs: Option<u64>) -> Self {
        Self {
            attribute: classify_attr(attribute.as_deref()),
            budget: Duration::from_secs(
                budget_secs.unwrap_or(PROVISIONAL_CLOSURE_BUDGET_SECS),
            ),
        }
    }
}

/// A classified closure, ready for emission.
/// Milestone 1372 (#1366) — C210, what the build-graph signal did.
///
/// **Both counts are DERIVATIONS, not components.** The classifier works in
/// derivations and the role is a per-derivation fact; 100 newly admitted
/// derivations merge into roughly 45 components, so a reader comparing
/// `admitted` against the document's component count will find they disagree.
/// That gap is why the unit is stated here and in the catalog row rather than
/// left to be inferred.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GraphAdmissionRecord {
    /// Derivations that begin emitting because of this signal.
    pub(crate) admitted: usize,
    /// Derivations already emitting whose role the signal corrected.
    pub(crate) relabelled: usize,
}

impl GraphAdmissionRecord {
    pub(crate) fn wire(&self) -> String {
        let mut o = serde_json::Map::new();
        o.insert("schema".into(), "waybill-closure-graph-admission/v1".into());
        o.insert("admitted".into(), self.admitted.into());
        o.insert("relabelled".into(), self.relabelled.into());
        serde_json::Value::Object(o).to_string()
    }
}

#[derive(Debug)]
pub(crate) struct ClassifiedClosure {
    pub(crate) attribute: String,
    pub(crate) raw: RawClosure,
    pub(crate) roles: std::collections::BTreeMap<String, DerivationRole>,
    /// Milestone 1372 (#1366) — the derivations the build graph alone put in
    /// a role. Carried beside `roles` rather than derived from them, because
    /// a `BuildTooling` role does not say which signal produced it.
    pub(crate) graph_admitted: std::collections::BTreeSet<String>,
    /// C210 — the counts over `graph_admitted`, split by whether the
    /// derivation already emitted.
    pub(crate) graph_admission: GraphAdmissionRecord,
    /// What the reader saw but did not consult (#1348, FR-009), and which
    /// members those unconsulted attributes name (#1346, FR-002).
    ///
    /// Computed beside the roles and carried alongside them, never consulted
    /// while deciding one — the separation SC-014 asserts.
    ///
    /// The per-member index rides **inside** this struct rather than beside
    /// it as a third field, and deliberately: it is built from the same walk
    /// that produces `fields_unconsulted`, so a separate field could be
    /// populated from a separate pass and the two could then disagree about
    /// which attributes are unconsulted. That disagreement is exactly the
    /// defect the index exists to prevent — a consumer told a member is
    /// `unreferenced` while the disclosure says an attribute naming it went
    /// unread. One struct, one walk, one answer.
    pub(crate) read_outcome: super::closure::derivation::ReadOutcome,
    /// Milestone 1280 (#1346) — the dependency graph, projected onto the
    /// components this closure will contribute.
    ///
    /// Computed here because it depends only on the closure and the roles,
    /// and **keyed on `.drv` paths** because it must survive the
    /// milestone-1275 identity pass, which rewrites 104 of 381 component
    /// PURLs after emission (research R1). The conversion to PURL-keyed
    /// relationships happens at the call site, after that pass; carrying the
    /// graph is safe, carrying PURLs would not be.
    pub(crate) graph: graph::ProjectedGraph,
    /// Milestone 1281 (#1346) — declared licenses, keyed by `.drv` basename.
    ///
    /// Keyed on `.drv` paths for the same reason `graph` is: the
    /// milestone-1275 identity pass rewrites component PURLs after emission,
    /// so a PURL-keyed join would dangle where this one survives.
    ///
    /// Empty when the tier ran without a license lookup, which is why
    /// `license_coverage` is carried beside it — an empty map alone cannot
    /// say whether nothing was declared or nothing was asked.
    pub(crate) licenses: licenses::LicenseIndex,
    /// Why the license lookup produced nothing, when it failed (FR-006).
    pub(crate) degraded_licenses: Option<DegradationReason>,
    /// How many members were offered a license and how many received one.
    pub(crate) license_coverage: licenses::LicenseCoverage,
    /// Milestone 1377 (#1394) — CPEs nixpkgs declares, keyed by `.drv`
    /// basename, read from the same evaluation as the licenses, and their
    /// coverage (C184 `identifiers`).
    pub(crate) identifiers: identifiers::DeclaredIdentifiers,
    /// Milestone 1423 (#1423) — the evaluated output's runtime closure, or
    /// why it could not be had. Queried by output path, never by installable
    /// (`runtime` module docs).
    pub(crate) runtime: runtime::RuntimeOutcome,
}

impl ClassifiedClosure {
    /// Members in each role, for the document-scope record (FR-018).
    pub(crate) fn role_counts(&self) -> std::collections::BTreeMap<&'static str, usize> {
        let mut counts = std::collections::BTreeMap::new();
        for role in self.roles.values() {
            *counts.entry(role.wire()).or_insert(0) += 1;
        }
        counts
    }
}

/// Build the argv for a closure query.
///
/// Separated so the safety properties are assertable without running nix, and
/// so production and tests share one definition rather than two that drift.
///
/// The flake is addressed through the **CLI flakeref form**. That is not a
/// style choice: `builtins.getFlake` on a local path requires `--impure`
/// (measured, research R4), and `--impure` is refused by `argv_is_safe`.
fn closure_argv(flakeref: &str) -> Vec<&str> {
    vec![
        "derivation",
        "show",
        "-r",
        "--option",
        IFD_SETTING,
        "false",
        flakeref,
    ]
}

/// Build the argv that lists the attributes under `packages.<system>`.
fn attributes_argv(flakeref: &str) -> Vec<&str> {
    vec![
        "eval",
        "--json",
        "--option",
        IFD_SETTING,
        "false",
        "--apply",
        "builtins.attrNames",
        flakeref,
    ]
}

/// Decide whether the tier may run at all, before any nix process starts.
///
/// `--offline` refuses it, for milestone 1034's measured reason: resolving a
/// flake reference fetches when the store lacks it, and nix's own `--offline`
/// governs substituters rather than flake inputs. A promise of "no outbound
/// network calls" kept only when a cache happens to be warm is not a promise.
///
/// Separate from [`resolve`] so the refusal is assertable without a `nix` on
/// the bench, and so the *order* is unambiguous: this is checked first, and a
/// refusal here means no subprocess is spawned at all.
pub(crate) fn admission(enabled: bool, offline: bool) -> Result<(), Option<DegradationReason>> {
    if !enabled {
        // Not a degradation — nothing was asked for, and nothing is recorded.
        return Err(None);
    }
    if offline {
        return Err(Some(DegradationReason::OfflineRequested));
    }
    Ok(())
}

/// Take and classify the closure of one flake attribute.
///
/// Every failure degrades. The closure supplements the manifest-derived set
/// (spec FR-003a), so losing it costs only the supplement.
pub(crate) fn resolve(
    project: &Path,
    system: &NixSystem,
    cfg: &ClosureConfig,
) -> Result<ClassifiedClosure, DegradationReason> {
    // #1114 — safety first, before anything the scanned repository controls
    // is listed or evaluated, exactly as the m1034 tier gates. Requesting
    // `allow-import-from-derivation false` is not evidence it applied: a nix
    // without the setting accepts the option, ignores it and exits 0.
    //
    // Milestone 1423 (T014): the same listing carries the configured
    // `substituters` (q1), so the runtime query below reads them from here
    // rather than starting another `nix` process.
    let (_, config) = verify_ifd_refusal_with_config(cfg.budget)?;
    let substituters = runtime::substituters_from_config(&config);

    let root = project.display().to_string();
    let (attribute, flakeref) = match &cfg.attribute {
        AttrRequest::PackageName(n) => package_target(&root, system, n, cfg)?,
        AttrRequest::Auto => auto_target(&root, system, cfg)?,
        AttrRequest::FullPath(p) => full_path_target(&root, p)?,
    };

    let argv = closure_argv(&flakeref);
    guard(&argv)?;
    let out = run_bounded(&argv, cfg.budget)?;
    if !out.status_success {
        // A full path is not checked against a listing first, so a missing
        // one surfaces here. Nix 2.34 words it "does not provide attribute"
        // (measured); the m1034 classifier's "attribute"+"missing" does not
        // match that wording (analysis U1).
        if matches!(cfg.attribute, AttrRequest::FullPath(_))
            && out.stderr.contains("does not provide attribute")
        {
            tracing::info!(attribute = %attribute, "nix-closure: the flake does not provide that path");
            return Err(DegradationReason::NoEvaluableAttribute);
        }
        // T020 / T027 (m1274, #1283 part 1) — the one rule, not a second
        // one. This line used to be `stderr.trim().chars().take(200)`: a
        // blind prefix that returned nix's leading git warning and truncated
        // 855 characters before the line explaining the failure. The fix
        // already existed in `eval::stderr` — a hundred lines away in the
        // sibling module, with a doc comment quoting *this* milestone's
        // motivating cabal2nix message as its case — and had never been
        // applied here. Measured on the probe: the operator read
        // `evaluation-failed` with a git warning attached, for a refusal nix
        // had explained in full.
        return Err(classify_failure(out.stderr.trim()));
    }

    let raw = RawClosure::parse(&out.stdout).map_err(|e| {
        DegradationReason::EvaluationFailed(format!("closure json unparseable: {e}"))
    })?;
    let roles = classify::classify(&raw);
    let graph_admitted = classify::graph_admitted(&raw);
    let graph_admission = {
        // Split by what `emits_as_component` said BEFORE the signal existed:
        // a derivation whose role is the only reason it emits is admitted;
        // one that emitted anyway is relabelled.
        let mut rec = GraphAdmissionRecord::default();
        for key in &graph_admitted {
            let Some(drv) = raw.derivations.get(key) else { continue };
            if emit::emits_without_role(drv) {
                rec.relabelled += 1;
            } else {
                rec.admitted += 1;
            }
        }
        rec
    };
    let read_outcome = raw.read_outcome(&classify::consulted_fields());
    let graph = graph::project(&raw, &emit::emitted_drv_keys(&raw, &roles));
    tracing::info!(
        attribute = %attribute,
        derivations = raw.derivations.len(),
        format_version = ?read_outcome.format_version,
        "nix-closure: closure classified"
    );
    if read_outcome.has_gap() {
        // Logged as well as disclosed. The document is what a consumer reads
        // (Constitution XII.3), but an operator watching a scan should not
        // have to open the output to learn the reader skipped something.
        tracing::warn!(
            fields = %read_outcome
                .fields_unconsulted
                .iter()
                .cloned()
                .collect::<Vec<String>>()
                .join(", "),
            declarations = read_outcome.declarations_unconsulted,
            derivations = read_outcome.derivations_affected,
            "nix-closure: declarations were seen but not consulted"
        );
    }
    tracing::info!(
        edges = graph.edges.len(),
        root = graph.root.as_deref().unwrap_or("<none>"),
        // Milestone 1377 — `<none>` here with a root above means several
        // derivations are unconsumed, so no evaluated package is named and
        // the subject comes from the existing ladder.
        evaluated = graph.evaluated.as_deref().unwrap_or("<none>"),
        visits = graph.visits,
        "nix-closure: dependency graph projected"
    );
    // Milestone 1423 (#1423) — what ships. Never fails the closure: an
    // unavailable runtime closure is recorded and the scope pass falls back.
    let runtime = Some(runtime::query_closure(
        &raw,
        graph.evaluated.as_deref(),
        cfg.budget,
        &substituters,
    ));
    Ok(ClassifiedClosure {
        runtime,
        graph_admitted,
        graph_admission,
        attribute,
        raw,
        roles,
        read_outcome,
        graph,
        licenses: Default::default(),
        degraded_licenses: None,
        license_coverage: Default::default(),
        identifiers: Default::default(),
    })
}

/// Milestone 1281 (#1346) — attach declared licenses to a classified closure.
///
/// Separate from [`resolve`] and run after it, because it needs the pinned
/// nixpkgs revision, which comes from `flake.lock` rather than from the
/// closure. Degradation is deliberate and total: every failure leaves the
/// closure exactly as it arrived, so the document is still emitted with its
/// full component set and only the licenses are missing (spec FR-006).
///
/// The import-from-derivation refusal is **not** re-verified here. [`resolve`]
/// verifies it once per closure run before anything the repository controls is
/// evaluated, and this call inherits that; re-issuing it would be the second,
/// parallel set of controls FR-012 forbids.
pub(crate) fn attach_licenses(
    closure: &mut ClassifiedClosure,
    revision: &str,
    system: &NixSystem,
    budget: std::time::Duration,
) {
    // The names scope the evaluation; the store path is the join. A name that
    // resolves to the wrong attribute costs a lookup, never a wrong license
    // (spec FR-002, research R5).
    let names: Vec<String> = closure
        .raw
        .derivations
        .values()
        .filter_map(|d| d.pname())
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // FR-008 — eligibility is counted over the members that will be emitted,
    // not over every derivation, so coverage is a fraction a reader can
    // compare against the component count they actually receive.
    let emitted = emit::coverage_drv_keys(
        &closure.raw,
        &closure.roles,
        closure.graph.evaluated.as_deref(),
    );
    let wanted = emitted.len();
    // Milestone 1377 — the identifier join runs over the same population,
    // so `identifiers.wanted` always equals `licenses.wanted`.
    let members = || {
        emitted.iter().filter_map(|key| {
            let drv = closure.raw.derivations.get(*key)?;
            Some(identifiers::Member {
                key,
                pname: drv.pname()?,
                version: drv.version(),
            })
        })
    };

    if names.is_empty() {
        tracing::info!("nix-closure-licenses: no named members; nothing to look up");
        return;
    }

    let rows = match super::eval::licenses::resolve_licenses(revision, system, &names, budget) {
        Ok(rows) => rows,
        Err(reason) => {
            // Recorded on the closure, not only logged: an operator reading
            // the document must be able to tell why the licenses are absent
            // (FR-006, Constitution XII.3).
            tracing::info!(
                reason = reason.wire(),
                detail = %reason,
                "nix-closure-licenses: degrading; components are unaffected"
            );
            closure.license_coverage = licenses::LicenseCoverage {
                wanted,
                received: 0,
                withheld_ambiguous: 0,
            };
            closure.degraded_licenses = Some(reason);
            // Degrades exactly as licenses do: no declared CPEs, so every
            // member keeps the curated fallback, and `received: 0` says so.
            closure.identifiers = identifiers::DeclaredIdentifiers {
                index: Default::default(),
                coverage: identifiers::IdentifierCoverage {
                    wanted,
                    ..Default::default()
                },
            };
            return;
        }
    };

    let (index, withheld_ambiguous) = licenses::index(&rows);
    let declared = identifiers::join(&rows, members());
    tracing::info!(
        wanted = declared.coverage.wanted,
        received = declared.coverage.received,
        by_drv = declared.coverage.by_drv,
        by_name_version = declared.coverage.by_name_version,
        declared = declared.coverage.declared,
        rejected = declared.coverage.rejected,
        "nix-closure-identifiers: joined by store path, then by name and version"
    );
    closure.identifiers = declared;
    let received = emitted.iter().filter(|k| index.contains_key(**k)).count();
    tracing::info!(
        wanted,
        received,
        withheld_ambiguous,
        rows = rows.len(),
        revision,
        "nix-closure-licenses: joined by store path"
    );
    closure.licenses = index;
    closure.license_coverage = licenses::LicenseCoverage {
        wanted,
        received,
        withheld_ambiguous,
    };
}

/// `(attribute, flakeref)` for a name under `packages.<system>`: today's
/// path, admitted only if the flake's own listing contains the name.
fn package_target(
    root: &str,
    system: &NixSystem,
    name: &str,
    cfg: &ClosureConfig,
) -> Result<(String, String), DegradationReason> {
    let base = format!("{root}#packages.{}", system.as_str());
    // Which attributes exist? Answers FR-015b's "name what IS available"
    // without a second failure mode when the attribute is simply missing.
    let Some(available) = list_attributes(&base, cfg)? else {
        return Err(DegradationReason::NoEvaluableAttribute);
    };
    if !available.iter().any(|a| a == name) {
        tracing::info!(
            requested = %name,
            available = %available.join(", "),
            "nix-closure: requested attribute is absent"
        );
        return Err(DegradationReason::NoEvaluableAttribute);
    }
    Ok((name.to_string(), format!("{base}.{name}")))
}

/// Milestone 1066 (#1052): a full output path, evaluated as given, with no
/// `<system>` inserted: a configuration carries its own platform (FR-004).
/// There is no listing to admit it against, so it must pass the same
/// character check listed names do before it is interpolated into a flakeref
/// (FR-011).
fn full_path_target(root: &str, path: &str) -> Result<(String, String), DegradationReason> {
    if !is_safe_attribute_name(path) {
        tracing::info!("nix-closure: refusing unsafe attribute path");
        return Err(DegradationReason::NoEvaluableAttribute);
    }
    Ok((path.to_string(), format!("{root}#{path}")))
}

/// Milestone 1066 (#1052): no `--nix-closure-attr`. A flake with a `packages`
/// output for any platform takes today's path; only a flake with none has its
/// system configurations counted (FR-002, analysis I1).
fn auto_target(
    root: &str,
    system: &NixSystem,
    cfg: &ClosureConfig,
) -> Result<(String, String), DegradationReason> {
    let packages_output_present = list_attributes(&format!("{root}#packages"), cfg)?.is_some();
    let (darwin, nixos) = if packages_output_present {
        (None, None)
    } else {
        (
            list_attributes(&format!("{root}#darwinConfigurations"), cfg)?,
            list_attributes(&format!("{root}#nixosConfigurations"), cfg)?,
        )
    };
    match select(packages_output_present, darwin, nixos) {
        Selection::PackageDefault => package_target(root, system, "default", cfg),
        Selection::Configuration(c) => {
            let path = c.system_path();
            tracing::info!(attribute = %path, "nix-closure: the flake's only system configuration");
            let flakeref = format!("{root}#{path}");
            Ok((path, flakeref))
        }
        Selection::Degrade(reason) => Err(reason),
    }
}

/// `builtins.attrNames` of `flakeref`, or `None` when the output is absent or
/// could not be listed. Tool and budget failures still propagate.
fn list_attributes(
    flakeref: &str,
    cfg: &ClosureConfig,
) -> Result<Option<Vec<String>>, DegradationReason> {
    let argv = attributes_argv(flakeref);
    guard(&argv)?;
    let listed = run_bounded(&argv, cfg.budget)?;
    if !listed.status_success {
        tracing::debug!(
            flakeref,
            // The one rule here too (m1274, FR-008). This is a debug field
            // rather than a document detail, so it was not in the
            // milestone's scope — but it had the same defect in miniature,
            // and carving an exception for it in the "exactly one rule"
            // test is how the next copy gets in.
            stderr = %salient_stderr(listed.stderr.trim()),
            "nix-closure: output not listable"
        );
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(listed.stdout.trim()).unwrap_or_default()))
}

/// Refuse an argv that would hand evaluation settings to the scanned flake.
///
/// Reuses milestone 1044's guard rather than reimplementing it — two copies
/// would drift, and the drift would be exactly the thing the guard prevents.
/// It matters more here than it did there: milestone 1034 evaluates nixpkgs at
/// a pinned revision, while this evaluates the project's own flake, and real
/// flakes request `allow-import-from-derivation` through `nixConfig`.
fn guard(argv: &[&str]) -> Result<(), DegradationReason> {
    if argv_is_safe(argv) {
        return Ok(());
    }
    Err(DegradationReason::EvaluationFailed(
        "refusing to evaluate: the nix invocation would hand evaluation \
         settings to the scanned flake"
            .to_string(),
    ))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // ---- m1066 (#1052): attribute classification and auto-selection ----

    #[test]
    fn t002_classify_attr_follows_the_contract() {
        use AttrRequest::*;
        assert_eq!(classify_attr(None), Auto);
        for bare in ["default", "pkg-ghc96", "some_pkg.sub", "darwinConfigurations"] {
            assert_eq!(classify_attr(Some(bare)), PackageName(bare.into()), "{bare}");
        }
        for full in [
            "darwinConfigurations.laptop.system",
            "nixosConfigurations.web01.config.system.build.toplevel",
            "packages.x86_64-linux.hello",
            "homeConfigurations.me.activationPackage",
            "nixosConfigurations.a b",
            "packages.x#y",
        ] {
            assert_eq!(classify_attr(Some(full)), FullPath(full.into()), "{full}");
        }
    }

    fn names(v: &[&str]) -> Option<Vec<String>> {
        Some(v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn t003_a_packages_output_always_wins() {
        assert_eq!(select(true, names(&["laptop"]), None), Selection::PackageDefault);
    }

    #[test]
    fn t003_one_configuration_is_selected_at_its_system_output() {
        match select(false, names(&["laptop"]), None) {
            Selection::Configuration(c) => {
                assert_eq!(c.system_path(), "darwinConfigurations.laptop.system")
            }
            other => panic!("{other:?}"),
        }
        match select(false, None, names(&["web01"])) {
            Selection::Configuration(c) => assert_eq!(
                c.system_path(),
                "nixosConfigurations.web01.config.system.build.toplevel"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t003_several_configurations_degrade_sorted() {
        match select(false, names(&["laptop"]), names(&["web01"])) {
            Selection::Degrade(DegradationReason::AmbiguousSystemConfiguration { names, example }) => {
                assert_eq!(names, "darwinConfigurations.laptop, nixosConfigurations.web01");
                assert_eq!(example, "darwinConfigurations.laptop.system");
            }
            other => panic!("{other:?}"),
        }
        match select(false, names(&["zeta", "alpha"]), None) {
            Selection::Degrade(DegradationReason::AmbiguousSystemConfiguration { names, .. }) => {
                assert_eq!(names, "darwinConfigurations.alpha, darwinConfigurations.zeta")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t003_none_degrades_no_evaluable_attribute() {
        for (d, n) in [(None, None), (names(&[]), names(&[]))] {
            assert_eq!(
                select(false, d, n),
                Selection::Degrade(DegradationReason::NoEvaluableAttribute)
            );
        }
    }

    #[test]
    fn t003_unsafe_names_are_dropped_before_counting() {
        match select(false, names(&["ok", "a b", "x\"y"]), None) {
            Selection::Configuration(c) => assert_eq!(c.name, "ok"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_closure_argv_stays_pure_and_refuses_ifd() {
        let argv = closure_argv("/proj#packages.x86_64-linux.default");
        assert!(argv_is_safe(&argv), "closure argv must stay pure: {argv:?}");
        assert!(argv.contains(&IFD_SETTING));
        assert_eq!(
            argv.iter().position(|a| *a == IFD_SETTING).map(|i| argv[i + 1]),
            Some("false"),
            "the setting must be passed as false, not merely named"
        );
        assert!(
            !argv.contains(&"--impure"),
            "getFlake on a local path needs --impure; the CLI flakeref form \
             exists precisely to avoid it"
        );
    }

    #[test]
    fn the_attribute_listing_argv_is_guarded_too() {
        // Both invocations touch the project's flake, so both need the guard.
        assert!(argv_is_safe(&attributes_argv("/proj#packages.x86_64-linux")));
    }

    #[test]
    fn the_guard_rejects_what_it_exists_to_reject() {
        assert!(guard(&closure_argv("/p#a")).is_ok());
        assert!(guard(&["derivation", "show", "--accept-flake-config"]).is_err());
        assert!(guard(&["derivation", "show", "--impure"]).is_err());
    }

    #[test]
    fn offline_refuses_before_any_process_starts() {
        // The refusal must precede the subprocess, not follow it. Checking
        // after spawning would already have made the call `--offline`
        // promised would not happen.
        let refused = admission(true, true).unwrap_err().unwrap();
        assert_eq!(refused.wire(), "offline-requested");
    }

    #[test]
    fn the_flag_off_path_is_not_a_degradation() {
        // Nothing was asked for, so nothing is recorded — distinct from
        // asking and being refused, which a consumer must be able to tell
        // apart.
        assert!(admission(false, false).unwrap_err().is_none());
        assert!(admission(false, true).unwrap_err().is_none());
    }

    #[test]
    fn enabled_and_online_is_admitted() {
        assert!(admission(true, false).is_ok());
    }

    #[test]
    fn auto_unless_overridden() {
        // m1066: an absent flag is `Auto`, distinct from an explicit `default`,
        // so auto-selection never overrides what an operator asked for.
        assert_eq!(ClosureConfig::from_flags(None, None).attribute, AttrRequest::Auto);
        assert_eq!(
            ClosureConfig::from_flags(Some("default".into()), None).attribute,
            AttrRequest::PackageName("default".into())
        );
        assert_eq!(
            ClosureConfig::from_flags(Some("pkg-ghc96".into()), None).attribute,
            AttrRequest::PackageName("pkg-ghc96".into())
        );
    }
}
