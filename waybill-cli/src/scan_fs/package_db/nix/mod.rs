//! Milestone 925 (#946) — Nix `flake.lock` reader.
//!
//! waybill claimed none of a Nix repository's files. On the reference
//! repository `waybill repo report` recorded 65 of 70 files unclaimed, every
//! Nix file among them — so the artefact that actually pins the build was
//! absent from the emitted document.
//!
//! `flake.lock` pins every input by exact revision with a hash. That is a
//! stronger pin than most lockfiles waybill already reads. This reader emits
//! one source-tier component per pinned input, connected to whatever declares
//! it.
//!
//! Scope is the lockfile. No Nix evaluation, no subprocess, no network
//! (SC-002). Resolving package versions *through* a pinned input — taking a
//! Haskell project from 0 resolved versions to 12 with source hashes — is a
//! separate feature (#947); it needs network, a revision-keyed cache, and a
//! decision about GHC boot libraries whose version belongs to the compiler.

pub(crate) mod identity;
pub(crate) mod lockfile;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

use super::PackageDbEntry;
use lockfile::FlakeLockDocument;

/// Internal marker on an input the lockfile's root node declares directly,
/// consumed by [`attach_inputs_to_projects`] and never emitted (filtered by
/// `root_selector::is_internal_emission_key`).
pub const ROOT_INPUT_KEY: &str = "waybill:nix-root-input";

#[derive(Debug, Default)]
pub(crate) struct NixDiscoveredPaths {
    pub(crate) flake_locks: Vec<PathBuf>,
}

fn on_nix_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(state) = ctx.state::<Mutex<NixDiscoveredPaths>>(ReaderId::NIX) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.flake_locks.push(path.to_path_buf());
}

/// FR-001 — discovery. No `on_dir` and no `descend_into`: the file is found
/// wherever the shared walker already goes (research R6).
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/flake.lock"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::NIX,
        state: Some(Arc::new(Mutex::new(NixDiscoveredPaths::default()))),
        patterns,
        on_file: Some(on_nix_file),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_paths(registration: &ReaderRegistration) -> NixDiscoveredPaths {
    let Some(state_arc) = registration.state.as_ref() else {
        return NixDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<NixDiscoveredPaths>>() else {
        return NixDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Build the components for one lockfile.
///
/// Each lockfile governs the directory that contains it (FR-011). One never
/// speaks for another directory — the rule #938 established for Haskell
/// lockfiles, applied here from the start rather than after a defect.
fn emit_for_lockfile(path: &Path, doc: &FlakeLockDocument) -> Vec<PackageDbEntry> {
    let source_path = path.to_string_lossy().into_owned();

    // FR-004 — a `follows` alias names an existing pin. Collect the node keys
    // aliases resolve to so no second component is minted for one pin.
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let root_inputs = doc.root_input_keys();

    for (node_key, node, locked) in doc.emittable_nodes() {
        let Some(id) = identity::identify(node_key, locked) else {
            // FR-003 / FR-002: unpublishable type, or no revision. Recorded
            // rather than passed over silently (Principle VIII).
            tracing::debug!(
                lockfile = %path.display(),
                input = node_key,
                kind = %locked.kind,
                "nix: input not emitted — no publishable identity at a known revision"
            );
            continue;
        };

        // `waybill:source-type` is NOT written here. The typed `source_type`
        // field below is its carrier, and the emitters render both — writing
        // both is what makes the property appear twice on every component
        // (#940). Every other reader that sets both has the same defect; this
        // one does not add to it.
        let mut extra: BTreeMap<String, serde_json::Value> = BTreeMap::new();

        // FR-009a — the NAR hash, verbatim, SRI prefix intact, in an
        // annotation and never in a native checksum field. It is a SHA-256
        // over a NAR *serialization of a directory tree*, not over the
        // component's bytes, and it is base64 rather than hex; a native
        // checksum field would be wrong in both semantics and encoding, and a
        // consumer that verified it would be misled by a value that looks
        // correct. Principle IX over Principle V, settled in clarification Q2.
        if let Some(nar) = locked.nar_hash.as_deref() {
            extra.insert("waybill:nix-nar-hash".to_string(), json!(nar));
        }

        // FR-006 — what was asked for, when it differs from what was resolved.
        //
        // Two carriers because these are two facts. The pin STATE answers
        // "would re-locking move this?" for every input; the ref NAME says
        // which branch or tag, and only exists when the author wrote one.
        // Folding them into one string would make a value of `default-branch`
        // indistinguishable from a branch actually called `default-branch`.
        if let Some(orig) = node.original.as_ref() {
            let state = orig.pin_state(locked);
            if let Some(s) = state.as_annotation() {
                extra.insert("waybill:nix-original-pin-state".to_string(), json!(s));
            }
            if let lockfile::OriginalPinState::NamedRef(r) = state {
                extra.insert("waybill:nix-original-ref".to_string(), json!(r));
            }
        }

        if let Some(url) = id.source_url.as_deref() {
            extra.insert("waybill:source-url".to_string(), json!(url));
        }

        // FR-007 — which inputs the project itself declares is a fact of the
        // lockfile's root node, so it is recorded here rather than inferred
        // later from edge shape. Internal only; see `ROOT_INPUT_KEY`.
        if root_inputs.contains(node_key) {
            extra.insert(ROOT_INPUT_KEY.to_string(), json!(true));
        }

        // FR-008 — an input that declares its own inputs gets an edge to each,
        // rather than having them flattened onto the root. `depends` carries
        // names; the scan orchestrator resolves them against the entries found
        // in this same scan. `Follows` edges resolve to the node they alias, so
        // an alias contributes an edge without contributing a component.
        let depends: Vec<String> = node
            .inputs
            .values()
            .filter_map(|edge| doc.resolve(edge))
            .filter_map(|target_key| {
                doc.nodes
                    .get(target_key)
                    .and_then(|n| n.locked.as_ref())
                    .and_then(|l| identity::identify(target_key, l))
                    .map(|t| t.name)
            })
            .collect();
        let depends = {
            let mut d = depends;
            d.sort();
            d.dedup();
            d
        };

        out.push(PackageDbEntry {
            depends_ecosystem: None,
            purl: id.purl,
            name: id.name,
            version: id.version,
            arch: None,
            source_path: source_path.clone(),
            depends,
            maintainer: None,
            licenses: Vec::new(),
            // FR-007a — a flake input is part of the BUILD ENVIRONMENT, not of
            // the project's dependency closure. This makes
            // `apply_lifecycle_scope_to_edges` rewrite any edge pointing here
            // from `DependsOn` to `BuildDependsOn`, which emits SPDX 2.3
            // `BUILD_DEPENDENCY_OF` and a filterable CycloneDX non-runtime
            // scope. A consumer filtering to runtime drops these on that signal.
            lifecycle_scope: Some(waybill_common::resolution::LifecycleScope::Build),
            requirement_ranges: Vec::new(),
            source_type: Some(id.source_type.to_string()),
            buildinfo_status: None,
            // The lockfile states what the build resolves to: a stronger claim
            // than a declared range, weaker than an observation of a built
            // artefact.
            sbom_tier: Some("source".to_string()),
            evidence_kind: Some("nix-flake-lock".to_string()),
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
            shade_relocation: None,
            extra_annotations: extra,
            binary_role: None,
            build_inclusion: None,
        });
    }

    // SC-006 — deterministic order, independent of anything the scan happened
    // to produce. `nodes` is a JSON object; its iteration order is not a
    // guarantee. This is #948's failure mode, where SPDX 2.3 inherited an
    // unstable order and two scans of one tree differed in bytes.
    out.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));
    out
}

pub(crate) fn finalize(paths: NixDiscoveredPaths) -> Vec<PackageDbEntry> {
    let mut lock_paths = paths.flake_locks;
    lock_paths.sort();

    let mut out = Vec::new();
    for path in &lock_paths {
        match lockfile::parse_flake_lock(path) {
            Ok(doc) => {
                if doc.nodes.len() <= 1 {
                    // A lockfile with no inputs is a truthful statement that
                    // nothing is pinned, not an error.
                    tracing::debug!(
                        path = %path.display(),
                        "nix: flake.lock pins no inputs"
                    );
                }
                out.extend(emit_for_lockfile(path, &doc));
            }
            Err(failure) => {
                // FR-010 — warn naming the file, continue. A malformed
                // lockfile must not perturb any other ecosystem's output
                // (SC-007).
                tracing::warn!(
                    path = %path.display(),
                    error = %failure,
                    "nix: flake.lock not read; no inputs emitted from it"
                );
            }
        }
    }
    out
}

/// FR-007 — attach the flake's own inputs to the project that builds with them.
///
/// Runs after every reader, because the thing an input attaches to comes from a
/// different reader entirely: the project's main module. A reader cannot emit
/// this edge from where it sits — relationships are built from `entry.depends`
/// resolved by name with `from` set to the entry's own PURL, and the document
/// root is chosen by the root selector at emit time.
///
/// Exactly the inputs the lockfile's root node declares are attached, as
/// recorded by the reader in [`ROOT_INPUT_KEY`]. This is read from the
/// lockfile, not inferred from edge shape: an input the root declares AND
/// another input reaches through `follows` (nixpkgs, almost always) is still a
/// direct input of the project. An input only other inputs declare keeps its
/// edge from that declarer alone (FR-008).
///
/// The edge is emitted as `DependsOn` and rewritten to `BuildDependsOn` by
/// `apply_lifecycle_scope_to_edges`, because every emitted input carries
/// `LifecycleScope::Build`.
pub(crate) fn attach_inputs_to_projects(
    components: &[waybill_common::resolution::ResolvedComponent],
    relationships: &mut Vec<waybill_common::resolution::Relationship>,
) {
    use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};

    let flake_inputs: Vec<&waybill_common::resolution::ResolvedComponent> = components
        .iter()
        .filter(|c| c.source_type.as_deref() == Some("nix-flake-input"))
        .filter(|c| c.extra_annotations.get(ROOT_INPUT_KEY) == Some(&json!(true)))
        .collect();
    if flake_inputs.is_empty() {
        return;
    }

    let main_modules: Vec<(&Path, &waybill_common::resolution::ResolvedComponent)> = components
        .iter()
        .filter(|c| {
            c.source_type
                .as_deref()
                .is_some_and(|t| t.ends_with("main-module"))
        })
        .filter_map(|c| {
            c.evidence
                .source_file_paths
                .first()
                .and_then(|p| Path::new(p).parent())
                .map(|d| (d, c))
        })
        .collect();
    if main_modules.is_empty() {
        return;
    }

    let mut added = 0usize;
    for input in &flake_inputs {
        let Some(flake_path) = input.evidence.source_file_paths.first() else {
            continue;
        };
        let Some(flake_dir) = Path::new(flake_path).parent() else {
            continue;
        };
        let owners = owners_of(flake_dir, &main_modules);
        if owners.is_empty() {
            tracing::debug!(
                lockfile = %flake_path,
                input = %input.purl.as_str(),
                "nix: no single project owns this flake's directory; input left unattached"
            );
            continue;
        }
        for main in owners {
            if main.purl.as_str() == input.purl.as_str() {
                continue;
            }
            relationships.push(Relationship {
                from: main.purl.as_str().to_string(),
                to: input.purl.as_str().to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                    source: flake_path.clone(),
                    data_type: "nix-flake-input".to_string(),
                },
            });
            added += 1;
        }
    }
    if added > 0 {
        tracing::info!(
            edges = added,
            "nix: attached flake inputs to the projects they build (FR-007, build-scoped)"
        );
    }
}

/// The projects a flake in `flake_dir` belongs to (FR-011).
///
/// Every main module whose manifest sits in the flake's own directory — more
/// than one when several ecosystems share it. Failing that, the nearest main
/// module below it, but only when that is unambiguous: a root flake over
/// several sibling packages does not belong to whichever one sorts first, and
/// guessing would misattribute the input rather than leave it visibly orphaned.
/// A project ABOVE the flake never owns it.
///
/// Components are compared as paths, not strings, so `/r/foo` is not taken to
/// contain `/r/foobar`.
fn owners_of<'a>(
    flake_dir: &Path,
    main_modules: &[(&Path, &'a waybill_common::resolution::ResolvedComponent)],
) -> Vec<&'a waybill_common::resolution::ResolvedComponent> {
    let same_dir: Vec<_> = main_modules
        .iter()
        .filter(|(dir, _)| *dir == flake_dir)
        .map(|(_, c)| *c)
        .collect();
    if !same_dir.is_empty() {
        return same_dir;
    }
    let below: Vec<(usize, &waybill_common::resolution::ResolvedComponent)> = main_modules
        .iter()
        .filter_map(|(dir, c)| {
            dir.strip_prefix(flake_dir)
                .ok()
                .map(|rel| (rel.components().count(), *c))
        })
        .collect();
    let Some(nearest) = below.iter().map(|(depth, _)| *depth).min() else {
        return Vec::new();
    };
    let at_nearest: Vec<_> = below
        .iter()
        .filter(|(depth, _)| *depth == nearest)
        .map(|(_, c)| *c)
        .collect();
    if at_nearest.len() == 1 {
        at_nearest
    } else {
        Vec::new()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::lockfile::*;
    use super::*;

    /// Verbatim shape of a real `flake.lock`, including the `original` naming a
    /// moving branch. Copied from a measured lockfile, not hand-written — a
    /// parser fixture that did not come from the tool it parses is testing the
    /// fixture, which is how #937 shipped a parser that matched nothing real.
    const REAL_SINGLE_INPUT: &str = r#"{
      "nodes": {
        "nixpkgs": {
          "locked": {
            "lastModified": 1780749050,
            "narHash": "sha256-3av0pIjlOWQ6rDbNOmpUSvbNnJkGORQKKjb4LtCZsIY=",
            "owner": "NixOS", "repo": "nixpkgs",
            "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb",
            "type": "github"
          },
          "original": {
            "owner": "NixOS", "ref": "nixos-unstable", "repo": "nixpkgs", "type": "github"
          }
        },
        "root": { "inputs": { "nixpkgs": "nixpkgs" } }
      },
      "root": "root",
      "version": 7
    }"#;

    /// A `follows` alias and a nested input — the two shapes Phase 0 found
    /// that a single-input lockfile cannot exercise.
    const REAL_FOLLOWS_AND_NESTED: &str = r#"{
      "nodes": {
        "flake-parts": {
          "inputs": { "nixpkgs-lib": ["nixpkgs"] },
          "locked": { "owner": "hercules-ci", "repo": "flake-parts",
                      "rev": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                      "narHash": "sha256-AAAA", "type": "github" },
          "original": { "owner": "hercules-ci", "repo": "flake-parts", "type": "github" }
        },
        "nixpkgs": {
          "locked": { "owner": "NixOS", "repo": "nixpkgs",
                      "rev": "cccccccccccccccccccccccccccccccccccccccc",
                      "narHash": "sha256-BBBB", "type": "github" },
          "original": { "owner": "NixOS", "ref": "nixos-unstable", "repo": "nixpkgs", "type": "github" }
        },
        "root": { "inputs": { "flake-parts": "flake-parts", "nixpkgs": "nixpkgs" } }
      },
      "root": "root",
      "version": 7
    }"#;

    #[test]
    fn a_follows_entry_parses_as_follows_not_as_a_node_key() {
        let doc = parse_flake_lock_str(REAL_FOLLOWS_AND_NESTED).unwrap();
        let edge = doc.nodes["flake-parts"].inputs.get("nixpkgs-lib").unwrap();
        assert!(
            matches!(edge, InputEdge::Follows(_)),
            "an array-valued `inputs` entry is a follows alias. Treating it as a \
             node key is the defect Phase 0 research R3 exists to prevent; got {edge:?}"
        );
        // And it resolves to the pin it aliases, not to itself.
        assert_eq!(doc.resolve(edge), Some("nixpkgs"));
    }

    #[test]
    fn a_follows_alias_does_not_mint_a_second_component() {
        let doc = parse_flake_lock_str(REAL_FOLLOWS_AND_NESTED).unwrap();
        let entries = emit_for_lockfile(Path::new("/x/flake.lock"), &doc);
        let purls: Vec<_> = entries.iter().map(|e| e.purl.as_str()).collect();
        assert_eq!(
            purls.len(), 2,
            "two pins are declared; the `nixpkgs-lib` follows alias names one of \
             them and must not become a third component (FR-004). got {purls:?}"
        );
    }

    #[test]
    fn an_unrecognised_version_is_reported_not_parsed() {
        let bad = REAL_SINGLE_INPUT.replace("\"version\": 7", "\"version\": 99");
        match parse_flake_lock_str(&bad) {
            Err(ParseFailure::UnrecognisedVersion { found }) => assert_eq!(found, 99),
            other => panic!("expected UnrecognisedVersion, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_distinguished_from_an_unknown_version() {
        match parse_flake_lock_str("{ not json") {
            Err(ParseFailure::Malformed { .. }) => {}
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn the_nar_hash_is_annotated_and_never_a_native_hash() {
        let doc = parse_flake_lock_str(REAL_SINGLE_INPUT).unwrap();
        let entries = emit_for_lockfile(Path::new("/x/flake.lock"), &doc);
        let e = &entries[0];
        assert!(
            e.hashes.is_empty(),
            "FR-009: a narHash covers a NAR serialization, not the component's \
             bytes, and is base64 not hex — it must never populate a native \
             checksum field"
        );
        assert_eq!(
            e.extra_annotations.get("waybill:nix-nar-hash").and_then(|v| v.as_str()),
            Some("sha256-3av0pIjlOWQ6rDbNOmpUSvbNnJkGORQKKjb4LtCZsIY="),
            "the SRI prefix is preserved so the value stays self-describing"
        );
    }

    #[test]
    fn an_input_tracking_the_default_branch_says_so() {
        // The gap this test exists for: an `original` with neither `ref` nor
        // `rev` means "track the default branch", which is the MOST moving
        // reference there is — and it was emitting nothing at all. On a real
        // repository that silence covered five of six inputs, so a consumer
        // asking "would re-locking move this?" got no answer for any of them.
        let lock = REAL_SINGLE_INPUT.replace(
            r#""owner": "NixOS", "ref": "nixos-unstable", "repo": "nixpkgs", "type": "github""#,
            r#""owner": "NixOS", "repo": "nixpkgs", "type": "github""#,
        );
        let doc = parse_flake_lock_str(&lock).unwrap();
        let e = &emit_for_lockfile(Path::new("/x/flake.lock"), &doc)[0];
        assert_eq!(
            e.extra_annotations.get("waybill:nix-original-pin-state").and_then(|v| v.as_str()),
            Some("default-branch")
        );
        assert!(
            !e.extra_annotations.contains_key("waybill:nix-original-ref"),
            "no ref was written, so there is no name to report — the pin STATE \
             carries the fact, the ref NAME carries the name"
        );
    }

    #[test]
    fn the_pin_state_and_the_ref_name_are_separate_carriers() {
        // A branch literally called `default-branch` must stay distinguishable
        // from an input that names no branch at all. That is why these are two
        // keys rather than one string with a sentinel value.
        let lock = REAL_SINGLE_INPUT.replace(r#""ref": "nixos-unstable""#, r#""ref": "default-branch""#);
        let doc = parse_flake_lock_str(&lock).unwrap();
        let e = &emit_for_lockfile(Path::new("/x/flake.lock"), &doc)[0];
        assert_eq!(
            e.extra_annotations.get("waybill:nix-original-pin-state").and_then(|v| v.as_str()),
            Some("branch-or-tag"),
            "a branch NAMED `default-branch` is still a named branch"
        );
        assert_eq!(
            e.extra_annotations.get("waybill:nix-original-ref").and_then(|v| v.as_str()),
            Some("default-branch")
        );
    }

    #[test]
    fn the_original_ref_is_recorded_only_when_it_differs() {
        let doc = parse_flake_lock_str(REAL_SINGLE_INPUT).unwrap();
        let entries = emit_for_lockfile(Path::new("/x/flake.lock"), &doc);
        assert_eq!(
            entries[0].extra_annotations.get("waybill:nix-original-ref").and_then(|v| v.as_str()),
            Some("nixos-unstable"),
            "FR-006: a moving reference that has been pinned must be visible as such"
        );

        // `original` already exact -> nothing to record (US3 scenario 2).
        let exact = REAL_SINGLE_INPUT.replace(
            r#""owner": "NixOS", "ref": "nixos-unstable", "repo": "nixpkgs", "type": "github""#,
            r#""owner": "NixOS", "repo": "nixpkgs", "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb", "type": "github""#,
        );
        let doc2 = parse_flake_lock_str(&exact).unwrap();
        let e2 = emit_for_lockfile(Path::new("/x/flake.lock"), &doc2);
        assert!(
            !e2[0].extra_annotations.contains_key("waybill:nix-original-ref"),
            "an original that already names the locked revision adds nothing"
        );
        assert!(
            !e2[0].extra_annotations.contains_key("waybill:nix-original-pin-state"),
            "an exact revision cannot move, so there is no pin-state to report"
        );
    }

    fn rc(purl: &str, source_type: &str, path: &str) -> waybill_common::resolution::ResolvedComponent {
        use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        let p = waybill_common::types::purl::Purl::new(purl).unwrap();
        waybill_common::resolution::ResolvedComponent {
            build_inclusion: None,
            name: p.name().to_string(),
            version: p.version().unwrap_or("0.0.0").to_string(),
            purl: p,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 1.0,
                source_connection_ids: vec![],
                source_file_paths: vec![path.to_string()],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: Some(source_type.to_string()),
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    /// A flake input as the reader emits it for a root-declared input.
    fn root_input(mut c: waybill_common::resolution::ResolvedComponent) -> waybill_common::resolution::ResolvedComponent {
        c.extra_annotations.insert(ROOT_INPUT_KEY.to_string(), json!(true));
        c
    }

    fn edges(rels: &[waybill_common::resolution::Relationship]) -> Vec<(String, String)> {
        let mut v: Vec<_> = rels.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
        v.sort();
        v
    }

    /// FR-007 — a root-declared input attaches to the project that builds it.
    #[test]
    fn a_root_declared_input_attaches_to_the_projects_main_module() {
        let components = vec![
            rc("pkg:hackage/proj@1.0", "hackage-main-module", "/r/proj.cabal"),
            root_input(rc("pkg:github/o/nixpkgs@abc", "nix-flake-input", "/r/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&components, &mut rels);
        assert_eq!(rels.len(), 1, "expected one project->input edge, got {rels:?}");
        assert_eq!(rels[0].from, "pkg:hackage/proj@1.0");
        assert_eq!(rels[0].to, "pkg:github/o/nixpkgs@abc");
    }

    /// FR-008 — an input only another input declares is NOT re-parented.
    #[test]
    fn a_transitively_declared_input_is_not_attached_to_the_project() {
        let components = vec![
            rc("pkg:hackage/proj@1.0", "hackage-main-module", "/r/proj.cabal"),
            root_input(rc("pkg:github/o/parent@aaa", "nix-flake-input", "/r/flake.lock")),
            rc("pkg:github/o/child@bbb", "nix-flake-input", "/r/flake.lock"),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&components, &mut rels);
        assert_eq!(
            edges(&rels),
            vec![("pkg:hackage/proj@1.0".to_string(), "pkg:github/o/parent@aaa".to_string())],
            "the child is declared by its parent input, not the root (FR-008)"
        );
    }

    /// FR-007 — the slack-web shape. The root declares nixpkgs AND another input
    /// reaches it through `follows`. It is still the project's own input; an
    /// earlier version attached only inputs nothing else pointed at, and
    /// dropped this edge on every flake whose inputs follow the root's nixpkgs.
    #[test]
    fn a_root_input_that_is_also_followed_still_attaches_to_the_project() {
        use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};
        let components = vec![
            rc("pkg:hackage/proj@1.0", "hackage-main-module", "/r/proj.cabal"),
            root_input(rc("pkg:github/o/hooks@aaa", "nix-flake-input", "/r/flake.lock")),
            root_input(rc("pkg:github/o/nixpkgs@bbb", "nix-flake-input", "/r/flake.lock")),
        ];
        let mut rels = vec![Relationship {
            from: "pkg:github/o/hooks@aaa".to_string(),
            to: "pkg:github/o/nixpkgs@bbb".to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "/r/flake.lock".to_string(),
                data_type: "package-database-depends".to_string(),
            },
        }];
        attach_inputs_to_projects(&components, &mut rels);
        assert!(
            edges(&rels).contains(&(
                "pkg:hackage/proj@1.0".to_string(),
                "pkg:github/o/nixpkgs@bbb".to_string()
            )),
            "nixpkgs is root-declared; the follows edge from hooks does not make it \
             hooks' input instead of the project's. got {rels:?}"
        );
    }

    /// The reader marks exactly the root's inputs, with follows resolved.
    #[test]
    fn only_root_declared_inputs_carry_the_marker() {
        let doc = parse_flake_lock_str(REAL_FOLLOWS_AND_NESTED).unwrap();
        let marked: Vec<String> = emit_for_lockfile(Path::new("/x/flake.lock"), &doc)
            .into_iter()
            .filter(|e| e.extra_annotations.get(ROOT_INPUT_KEY) == Some(&json!(true)))
            .map(|e| e.name)
            .collect();
        assert_eq!(marked, vec!["flake-parts".to_string(), "nixpkgs".to_string()]);
    }

    #[test]
    fn the_marker_is_never_emitted() {
        assert!(crate::generate::root_selector::is_internal_emission_key(ROOT_INPUT_KEY));
    }

    /// FR-011 — a flake in one directory does not attach to another directory's
    /// project. The #938 scoping rule, applied to this edge too.
    #[test]
    fn a_flake_attaches_to_the_nearest_project_not_an_unrelated_one() {
        let components = vec![
            rc("pkg:hackage/outer@1.0", "hackage-main-module", "/r/outer.cabal"),
            rc("pkg:hackage/inner@2.0", "hackage-main-module", "/r/sub/inner.cabal"),
            root_input(rc("pkg:github/o/dep@abc", "nix-flake-input", "/r/sub/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&components, &mut rels);
        assert_eq!(rels.len(), 1);
        assert_eq!(
            rels[0].from, "pkg:hackage/inner@2.0",
            "the flake in /r/sub belongs to the project in /r/sub, not the outer one"
        );
    }

    /// The moat shape: a root flake beside the root project, with a nested
    /// example project. The flake is the root project's. An earlier version
    /// took the longest matching directory and gave it to the example.
    #[test]
    fn a_root_flake_belongs_to_the_root_project_not_a_nested_one() {
        let components = vec![
            rc("pkg:hackage/moat@0.1", "hackage-main-module", "/r/moat.cabal"),
            rc("pkg:hackage/readme@0.1", "hackage-main-module", "/r/examples/readme/readme.cabal"),
            root_input(rc("pkg:github/o/nixpkgs@abc", "nix-flake-input", "/r/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&components, &mut rels);
        assert_eq!(
            edges(&rels),
            vec![("pkg:hackage/moat@0.1".to_string(), "pkg:github/o/nixpkgs@abc".to_string())]
        );
    }

    /// With no project beside the flake, the single nearest one below it owns
    /// it; siblings at the same depth are ambiguous and own nothing.
    #[test]
    fn with_no_project_beside_the_flake_only_an_unambiguous_nearest_one_owns_it() {
        let single = vec![
            rc("pkg:hackage/a@1", "hackage-main-module", "/r/pkgs/a/a.cabal"),
            rc("pkg:hackage/deep@1", "hackage-main-module", "/r/pkgs/a/x/deep.cabal"),
            root_input(rc("pkg:github/o/nixpkgs@abc", "nix-flake-input", "/r/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&single, &mut rels);
        assert_eq!(
            edges(&rels),
            vec![("pkg:hackage/a@1".to_string(), "pkg:github/o/nixpkgs@abc".to_string())]
        );

        let siblings = vec![
            rc("pkg:hackage/a@1", "hackage-main-module", "/r/pkgs/a/a.cabal"),
            rc("pkg:hackage/b@1", "hackage-main-module", "/r/pkgs/b/b.cabal"),
            root_input(rc("pkg:github/o/nixpkgs@abc", "nix-flake-input", "/r/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&siblings, &mut rels);
        assert!(rels.is_empty(), "two equally near projects: guessing would misattribute. got {rels:?}");
    }

    /// Directories compare as paths: `/r/foo` does not contain `/r/foobar`.
    #[test]
    fn directory_matching_is_by_path_component_not_string_prefix() {
        let components = vec![
            rc("pkg:hackage/other@1", "hackage-main-module", "/r/foobar/other.cabal"),
            root_input(rc("pkg:github/o/nixpkgs@abc", "nix-flake-input", "/r/foo/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&components, &mut rels);
        assert!(rels.is_empty(), "got {rels:?}");
    }

    /// A project above the flake never owns it.
    #[test]
    fn a_project_above_the_flake_does_not_own_it() {
        let components = vec![
            rc("pkg:hackage/outer@1", "hackage-main-module", "/r/outer.cabal"),
            root_input(rc("pkg:github/o/nixpkgs@abc", "nix-flake-input", "/r/sub/flake.lock")),
        ];
        let mut rels = Vec::new();
        attach_inputs_to_projects(&components, &mut rels);
        assert!(rels.is_empty(), "got {rels:?}");
    }

    /// FR-007a — the edge must be build-scoped once the scope rewrite runs.
    /// Asserting only that an edge exists would pass under a plain DependsOn,
    /// which is the thing FR-007a forbids.
    #[test]
    fn every_emitted_input_carries_build_scope() {
        let doc = parse_flake_lock_str(REAL_SINGLE_INPUT).unwrap();
        for e in emit_for_lockfile(Path::new("/x/flake.lock"), &doc) {
            assert_eq!(
                e.lifecycle_scope,
                Some(waybill_common::resolution::LifecycleScope::Build),
                "a flake input is part of the build environment, not the \
                 dependency closure (FR-007a). Without Build scope the edge \
                 stays a plain DependsOn and claims the project depends on \
                 nixpkgs the way it depends on its libraries"
            );
        }
    }

    #[test]
    fn source_type_is_carried_once_not_twice() {
        // #940: a reader that sets BOTH the typed `source_type` field and a
        // `waybill:source-type` annotation makes the property appear twice on
        // every emitted component, because the emitters render both. This
        // reader carries it in the typed field only.
        let doc = parse_flake_lock_str(REAL_SINGLE_INPUT).unwrap();
        let e = &emit_for_lockfile(Path::new("/x/flake.lock"), &doc)[0];
        assert!(e.source_type.is_some(), "the typed field is the carrier");
        assert!(
            !e.extra_annotations.contains_key("waybill:source-type"),
            "writing the annotation as well duplicates the property on the wire"
        );
    }

    #[test]
    fn emission_order_is_deterministic() {
        let doc = parse_flake_lock_str(REAL_FOLLOWS_AND_NESTED).unwrap();
        let a: Vec<String> = emit_for_lockfile(Path::new("/x/flake.lock"), &doc)
            .iter().map(|e| e.purl.as_str().to_string()).collect();
        let b: Vec<String> = emit_for_lockfile(Path::new("/x/flake.lock"), &doc)
            .iter().map(|e| e.purl.as_str().to_string()).collect();
        assert_eq!(a, b);
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(a, sorted, "SC-006: order must be total over the identifier");
    }

    #[test]
    fn a_nested_input_is_an_edge_from_its_declarer_not_the_root() {
        let doc = parse_flake_lock_str(REAL_FOLLOWS_AND_NESTED).unwrap();
        let entries = emit_for_lockfile(Path::new("/x/flake.lock"), &doc);
        let fp = entries
            .iter()
            .find(|e| e.name == "flake-parts")
            .expect("flake-parts is emitted");
        assert_eq!(
            fp.depends,
            vec!["nixpkgs".to_string()],
            "FR-008: `flake-parts` declares `nixpkgs-lib`, which FOLLOWS to `nixpkgs`. \
             The edge belongs to flake-parts, not the root, and the alias must \
             resolve to the pin it names rather than to itself"
        );
    }
}
