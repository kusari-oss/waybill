//! BitBake recipe walker (milestone 107 US4, FR-007, FR-008).
//!
//! Walks the scan target for `.bb` recipe files in Yocto/OE layer
//! directories and emits one component per recipe, drawn from the
//! filename pattern `<name>_<version>.bb` without parsing the recipe
//! body. This is the lowest-authority Yocto reader — recipes declared
//! by a layer may never have been selected by any image build — but
//! it's the only signal for a layer-tree scan with no build artifacts
//! present (security researchers auditing a vendor `meta-*/` layer
//! before adoption).
//!
//! Per FR-007: filename-only emission, no BitBake variable expansion.
//! Recipes whose filenames contain unexpanded `${...}` (typically
//! shared-base recipes like `${PN}_${PV}.bb`) are silently skipped
//! with a `tracing::warn!` per FR-008.
//!
//! Per FR-010 precedence: `BitbakeRecipe` is the lowest tier (2) —
//! installed-DB readers and image-manifest readers both outrank it.

use std::path::{Path, PathBuf};

use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::super::PackageDbEntry;

const RECIPE_FILENAME_REGEX: &str =
    r"^(?P<name>[a-zA-Z0-9_\-\+\.]+)_(?P<version>[a-zA-Z0-9_\-\+\.\~]+)\.bb$";

/// Walk the scan target for `.bb` recipe files and emit one
/// `PackageDbEntry` per recipe. Bounded to depth 8 (matches the
/// established source-tree-walker convention) to avoid runaway
/// traversal in deep monorepos.
/// Milestone 114: delegates to `scan_fs::walk::safe_walk`.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let Ok(regex) = Regex::new(RECIPE_FILENAME_REGEX) else {
        return Vec::new();
    };
    // Milestone 128 FR-006: build the layer.conf index first so each
    // recipe can be attributed to its nearest-ancestor layer.
    let layer_index = super::layer_conf::build_index(rootfs, exclude_set);

    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 8,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if !path.is_file() {
            return;
        }
        let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
            return;
        };
        if !filename.ends_with(".bb") {
            return;
        }
        if let Some(entry) = process_recipe(path, filename, &regex, &layer_index) {
            out.push(entry);
        }
    });

    // Milestone 128 FR-007: emit one synthesized layer-root
    // PackageDbEntry per detected `conf/layer.conf`. Each carries
    // `mikebom:component-role: "main-module"` so milestone-127's
    // root-selector ladder elects it via the FR-002 repo-root
    // tiebreaker — making the BOM subject identify the layer
    // collection name (e.g., `meta-balena-rust`) instead of the
    // generic `<basename>@0.0.0` fallback.
    for layer in &layer_index {
        out.push(build_layer_root_entry(layer));
    }

    // Milestone 128 FR-009: emit DEPENDS_ON-style relationships by
    // populating each recipe's `depends` field with the names that
    // resolve to other recipes in this scan. The scan orchestrator
    // already converts `depends` strings to relationships per the
    // existing milestone-105 pipeline. We also flag unresolved
    // entries via `mikebom:depends-unresolved` so consumers see
    // closure gaps.
    resolve_recipe_depends(&mut out);

    // Milestone 128 FR-008: bbappend match-and-annotate pass. Build
    // the BbAppendIndex by walking for `.bbappend` files; for each
    // recipe component, look up matching appends and emit
    // `mikebom:bbappend-applied` listing the append paths. Orphan
    // appends (no matching recipe in scan) emit warn logs + are
    // recorded in the orphans Vec; no phantom components per
    // Constitution Principle VIII completeness.
    let mut bbappend_idx = super::bbappend::build_from_walk(rootfs, exclude_set);
    let recipe_keys: std::collections::BTreeSet<(String, String)> = out
        .iter()
        .filter(|e| {
            e.extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str())
                == Some("bitbake-recipe")
        })
        .map(|e| {
            // Use the recipe-identity name if FR-002a's host-typed
            // PURL fired (the component name is then the upstream
            // repo, not the recipe name); fall back to the component
            // name otherwise.
            let name = e
                .extra_annotations
                .get("mikebom:yocto-recipe-name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| e.name.clone());
            let version = e
                .extra_annotations
                .get("mikebom:yocto-recipe-version")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| e.version.clone());
            (name, version)
        })
        .collect();
    super::bbappend::finalize_orphans(&mut bbappend_idx, &recipe_keys);
    for entry in out.iter_mut() {
        let role = entry
            .extra_annotations
            .get("mikebom:source-mechanism")
            .and_then(|v| v.as_str());
        if role != Some("bitbake-recipe") {
            continue;
        }
        let name = entry
            .extra_annotations
            .get("mikebom:yocto-recipe-name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| entry.name.clone());
        let version = entry
            .extra_annotations
            .get("mikebom:yocto-recipe-version")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| entry.version.clone());
        let appends = bbappend_idx.appends_for(&name, &version);
        if !appends.is_empty() {
            let arr: Vec<serde_json::Value> = appends
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect();
            entry.extra_annotations.insert(
                "mikebom:bbappend-applied".to_string(),
                serde_json::Value::Array(arr),
            );
        }
    }

    out
}

/// Synthesize a layer-root `PackageDbEntry` for FR-007 — the
/// component that milestone-127's root-selector elects as the BOM
/// subject for a Yocto meta-layer scan.
fn build_layer_root_entry(layer: &super::layer_conf::LayerConf) -> PackageDbEntry {
    let version = layer.version.clone().unwrap_or_else(|| "0.0.0".to_string());
    let purl_str = format!(
        "pkg:generic/{}@{}?openembedded=true&layer={}",
        encode_purl_segment(&layer.collection),
        encode_purl_segment(&version),
        encode_purl_segment(&layer.collection),
    );
    let purl = Purl::new(&purl_str)
        .or_else(|_| Purl::new(&format!(
            "pkg:generic/{}@{}",
            encode_purl_segment(&layer.collection),
            encode_purl_segment(&version),
        )))
        .expect("synthesized layer-root PURL must be valid");

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("yocto-layer-root".to_string()),
    );
    // FR-007 / milestone-127 interaction: the root selector's
    // `is_workspace_root` heuristic compares the PARENT of
    // `source-files[0]` to the scan root. For a layer rooted at
    // `<scan>/layer-a/conf/layer.conf`, parent(layer.conf) is
    // `.../conf/` (not the scan root). Instead, we point
    // source-files at the layer's ROOT directory (the directory
    // containing `conf/`) — its parent IS the scan root when the
    // layer is a direct child of `--path`. This makes the
    // milestone-127 selector mark each layer-root as
    // `is_workspace_root: true` so the FR-003 ecosystem-priority
    // tiebreaker can elect one as the BOM subject.
    let layer_root_dir = layer
        .source_path
        .parent()                                  // <layer>/conf/
        .and_then(|conf_dir| conf_dir.parent())    // <layer>/
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| layer.source_path.clone());
    extra_annotations.insert(
        "mikebom:source-files".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String(
            layer_root_dir.to_string_lossy().into_owned(),
        )]),
    );
    extra_annotations.insert(
        "mikebom:yocto-layer".to_string(),
        serde_json::Value::String(layer.collection.clone()),
    );
    if let Some(v) = layer.version.as_deref() {
        extra_annotations.insert(
            "mikebom:yocto-layer-version".to_string(),
            serde_json::Value::String(v.to_string()),
        );
    } else {
        extra_annotations.insert(
            "mikebom:yocto-layer-version-missing".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    if !layer.series_compat.is_empty() {
        extra_annotations.insert(
            "mikebom:yocto-layer-series".to_string(),
            serde_json::Value::Array(
                layer
                    .series_compat
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
        );
    }

    PackageDbEntry {
        build_inclusion: None,
        purl,
        name: layer.collection.clone(),
        version,
        arch: None,
        // Point source_path at the layer's ROOT dir (the directory
        // containing `conf/`), NOT the layer.conf file. milestone-127's
        // root-selector reads this via `evidence.source_file_paths[0]`
        // and compares `parent()` to the scan root. When the layer
        // root sits directly under `--path`, parent(layer-root) ==
        // scan-root → is_workspace_root = true → the FR-002/FR-003
        // tiebreaker elects this layer as the BOM subject.
        source_path: layer_root_dir.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
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
        sbom_tier: Some("design".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    }
}

/// FR-009: for each recipe component, walk its `mikebom:depends`
/// (the body-parsed DEPENDS list — stashed transiently) and
/// resolve each entry against the set of recipe NAMES present in
/// `entries`. Resolved entries populate the `depends` field
/// (which the scan orchestrator turns into `DEPENDS_ON`
/// relationships); unresolved entries get recorded on a
/// `mikebom:depends-unresolved` annotation.
fn resolve_recipe_depends(entries: &mut [PackageDbEntry]) {
    // Build a set of recipe component names present in this scan.
    let recipe_names: std::collections::BTreeSet<String> = entries
        .iter()
        .filter(|e| {
            e.extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str())
                == Some("bitbake-recipe")
        })
        .map(|e| e.name.clone())
        .collect();

    for entry in entries.iter_mut() {
        let role = entry
            .extra_annotations
            .get("mikebom:source-mechanism")
            .and_then(|v| v.as_str());
        if role != Some("bitbake-recipe") {
            continue;
        }
        // Stage 3: the body-parsed DEPENDS list was stashed on
        // `mikebom:depends-pending` (set by `process_recipe` below).
        // Resolve each entry against `recipe_names`.
        let raw_depends: Vec<String> = entry
            .extra_annotations
            .remove("mikebom:depends-pending")
            .and_then(|v| v.as_array().cloned())
            .map(|arr| {
                arr.into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if raw_depends.is_empty() {
            continue;
        }
        let mut resolved: Vec<String> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        for dep_name in &raw_depends {
            // BitBake DEPENDS entries can include `-native` /
            // `-nativesdk` suffixes; treat the base name as the
            // resolution target.
            let canon = dep_name
                .trim_end_matches("-native")
                .trim_end_matches("-nativesdk");
            if recipe_names.contains(canon) {
                resolved.push(canon.to_string());
            } else {
                unresolved.push(dep_name.clone());
            }
        }
        // Populate `depends` so the scan orchestrator emits
        // DEPENDS_ON edges via the existing milestone-105 pipeline.
        for r in resolved {
            if !entry.depends.contains(&r) {
                entry.depends.push(r);
            }
        }
        if !unresolved.is_empty() {
            entry.extra_annotations.insert(
                "mikebom:depends-unresolved".to_string(),
                serde_json::Value::Array(
                    unresolved
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
    }
}

fn process_recipe(
    path: &Path,
    filename: &str,
    regex: &Regex,
    layer_index: &[super::layer_conf::LayerConf],
) -> Option<PackageDbEntry> {
    // FR-008: silently skip recipes whose filenames carry unexpanded
    // BitBake variable expansion. The literal sequence `${` is the
    // canonical marker.
    if filename.contains("${") {
        tracing::warn!(
            path = %path.display(),
            "BitBake recipe filename contains unexpanded variable; skipping per FR-008"
        );
        return None;
    }

    let captures = regex.captures(filename);
    let (name, mut version, version_missing) = if let Some(caps) = captures {
        let name = caps.name("name")?.as_str().to_string();
        let version = caps.name("version")?.as_str().to_string();
        (name, version, false)
    } else {
        // `.bb` file with no `_<version>` segment (rare; e.g.,
        // `helloworld.bb`). Per data-model: emit with version="unknown"
        // and a `mikebom:version-status: "missing"` annotation rather
        // than dropping.
        let stem = filename.strip_suffix(".bb")?;
        if stem.is_empty() {
            return None;
        }
        (stem.to_string(), "unknown".to_string(), true)
    };

    // Milestone 128 T011: attempt body-parsing per FR-001..FR-005.
    // Body-parse failure → fall back to the milestone-107 filename-only
    // emission (preserves Constitution Principle VIII completeness
    // for malformed recipes).
    let body_metadata =
        super::recipe_body::parse_recipe_file(path, &name, &version);

    // Milestone 128 FR-018: when `PV` is literally "git" or contains
    // "AUTOINC", derive the emitted version segment from the SRCREV
    // first 12 hex chars. Always emit `mikebom:srcrev` with the full
    // SHA. When SRCREV is also absent, skip the component with a warn.
    let mut srcrev_full: Option<String> = None;
    if let Some(meta) = body_metadata.as_ref() {
        srcrev_full = meta.srcrev.clone();
        if version == "git" || version.contains("AUTOINC") {
            match meta.srcrev.as_deref() {
                Some(sha) if sha.len() >= 12 && sha.chars().take(12).all(|c| c.is_ascii_hexdigit()) => {
                    version = sha[..12].to_ascii_lowercase();
                }
                _ => {
                    tracing::warn!(
                        path = %path.display(),
                        pv = %version,
                        "Milestone 128 FR-018: recipe PV is git/AUTOINC but SRCREV is absent or malformed; skipping"
                    );
                    return None;
                }
            }
        }
    }

    let layer_name = detect_layer_name(path);

    // Milestone 128 FR-002a: when SRC_URI contains a git URI whose
    // host matches {github, gitlab, bitbucket, codeberg} AND SRCREV
    // is set, emit a host-typed PURL (`pkg:github/<owner>/<repo>@<srcrev>`,
    // etc.) instead of the FR-011 `pkg:generic/...` fallback. OSV's
    // commit + ecosystem queries return advisories directly against
    // host-typed PURLs.
    let (purl, host_typed_emission_name, host_typed_emission_version) =
        if let Some(meta) = body_metadata.as_ref() {
            if let Some((host_token, owner, repo)) =
                detect_host_typed_purl_inputs(meta.src_uris.iter().map(|s| s.as_str()))
            {
                if let Some(srcrev) = meta.srcrev.as_deref() {
                    if srcrev.len() >= 12
                        && srcrev.chars().take(12).all(|c| c.is_ascii_hexdigit())
                    {
                        let srcrev_short = srcrev[..12].to_ascii_lowercase();
                        let purl_str = format!(
                            "pkg:{}/{}/{}@{}",
                            host_token,
                            encode_purl_segment(&owner),
                            encode_purl_segment(&repo),
                            encode_purl_segment(&srcrev_short),
                        );
                        if let Ok(host_purl) = Purl::new(&purl_str) {
                            (host_purl, Some(repo), Some(srcrev_short))
                        } else {
                            (
                                build_bitbake_purl(&name, &version, layer_name.as_deref())?,
                                None,
                                None,
                            )
                        }
                    } else {
                        (
                            build_bitbake_purl(&name, &version, layer_name.as_deref())?,
                            None,
                            None,
                        )
                    }
                } else {
                    (
                        build_bitbake_purl(&name, &version, layer_name.as_deref())?,
                        None,
                        None,
                    )
                }
            } else {
                (
                    build_bitbake_purl(&name, &version, layer_name.as_deref())?,
                    None,
                    None,
                )
            }
        } else {
            (
                build_bitbake_purl(&name, &version, layer_name.as_deref())?,
                None,
                None,
            )
        };

    // When FR-002a's host-typed PURL fires, the emitted component's
    // name + version become the upstream (repo, srcrev-12-hex). The
    // recipe's original identity moves to annotations.
    let component_name = host_typed_emission_name
        .clone()
        .unwrap_or_else(|| name.clone());
    let component_version = host_typed_emission_version
        .clone()
        .unwrap_or_else(|| version.clone());

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("bitbake-recipe".to_string()),
    );
    if let Some(layer) = &layer_name {
        extra_annotations.insert(
            "mikebom:layer-name".to_string(),
            serde_json::Value::String(layer.clone()),
        );
    }
    if version_missing {
        extra_annotations.insert(
            "mikebom:version-status".to_string(),
            serde_json::Value::String("missing".to_string()),
        );
    }
    // Milestone 128 FR-002a: when the host-typed PURL fires, preserve
    // the recipe's filename-derived identity via annotations so
    // consumers can correlate "this is upstream artifact X, declared
    // via recipe Y at version Z."
    if host_typed_emission_name.is_some() {
        extra_annotations.insert(
            "mikebom:yocto-recipe-name".to_string(),
            serde_json::Value::String(name.clone()),
        );
        extra_annotations.insert(
            "mikebom:yocto-recipe-version".to_string(),
            serde_json::Value::String(version.clone()),
        );
    }

    // Milestone 128 FR-006 — attribute the recipe to its
    // nearest-ancestor `conf/layer.conf` per the Q2 clarification.
    if let Some(layer) = super::layer_conf::attribute_recipe(path, layer_index) {
        extra_annotations.insert(
            "mikebom:yocto-layer".to_string(),
            serde_json::Value::String(layer.collection.clone()),
        );
        if let Some(v) = layer.version.as_deref() {
            extra_annotations.insert(
                "mikebom:yocto-layer-version".to_string(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if !layer.series_compat.is_empty() {
            extra_annotations.insert(
                "mikebom:yocto-layer-series".to_string(),
                serde_json::Value::Array(
                    layer
                        .series_compat
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
    } else {
        // US3 AC#4 — no ancestor layer.conf found; continue without
        // layer annotations and emit a transparency warn.
        tracing::warn!(
            recipe = %path.display(),
            "Milestone 128 FR-006: recipe has no ancestor conf/layer.conf; layer attribution skipped"
        );
    }

    // Milestone 128 FR-017 + FR-019 — populate the
    // `mikebom:cpe-candidates` array with the raw recipe name plus
    // the openembedded-core-normalized CPE product name (when it
    // differs). One component per recipe; no Yocto-tooling-native
    // multi-component-per-vendor fan-out.
    {
        let raw = name.as_str();
        let normalized = super::cpe_name_map::cpe_product_for_recipe(raw);
        let mut candidates: Vec<String> = vec![raw.to_string()];
        if normalized != raw && !candidates.iter().any(|c| c == normalized) {
            candidates.push(normalized.to_string());
        }
        candidates.sort();
        candidates.dedup();
        let arr: Vec<serde_json::Value> = candidates
            .into_iter()
            .map(serde_json::Value::String)
            .collect();
        extra_annotations.insert(
            "mikebom:cpe-candidates".to_string(),
            serde_json::Value::Array(arr),
        );
    }

    // Milestone 128 — propagate body-parsed metadata onto the
    // emitted PackageDbEntry. Native-carrier fields (LICENSE,
    // HOMEPAGE, SUMMARY, externalReferences) live in their native
    // slots; the `mikebom:*` annotations carry parity-bridging
    // signals per FR-013.
    let mut licenses: Vec<waybill_common::types::license::SpdxExpression> = Vec::new();
    if let Some(meta) = body_metadata {
        if let Some(lic) = meta.license {
            licenses.push(lic);
        }
        if meta.license_closed {
            extra_annotations.insert(
                "mikebom:yocto-license-closed".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        if let Some(srcrev) = srcrev_full {
            extra_annotations.insert(
                "mikebom:srcrev".to_string(),
                serde_json::Value::String(srcrev),
            );
        }
        if !meta.srcrev_by_machine.is_empty() {
            let obj: serde_json::Map<String, serde_json::Value> = meta
                .srcrev_by_machine
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect();
            extra_annotations.insert(
                "mikebom:srcrev-by-machine".to_string(),
                serde_json::Value::Object(obj),
            );
        }
        if !meta.src_uris.is_empty() {
            let arr: Vec<serde_json::Value> = meta
                .src_uris
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();
            extra_annotations.insert(
                "mikebom:src-uri".to_string(),
                serde_json::Value::Array(arr),
            );
            // FR-002 AC#4: when ALL SRC_URI entries are file:// — local-only.
            if meta.src_uris.iter().all(|u| u.starts_with("file://")) {
                extra_annotations.insert(
                    "mikebom:src-uri-local-only".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
        }
        if !meta.unexpanded_vars.is_empty() {
            let arr: Vec<serde_json::Value> = meta
                .unexpanded_vars
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();
            extra_annotations.insert(
                "mikebom:yocto-unexpanded-vars".to_string(),
                serde_json::Value::Array(arr),
            );
        }
        if meta.overrides_merged {
            extra_annotations.insert(
                "mikebom:yocto-overrides-merged".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        // FR-009: stash DEPENDS for the post-pass `resolve_recipe_depends`
        // call site that resolves names against the scanned recipe
        // set + emits the unresolved bag.
        if !meta.depends.is_empty() {
            let arr: Vec<serde_json::Value> = meta
                .depends
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();
            extra_annotations.insert(
                "mikebom:depends-pending".to_string(),
                serde_json::Value::Array(arr),
            );
        }
        if !meta.rdepends.is_empty() {
            // For Stage-3 scope, RDEPENDS carries to a single
            // `mikebom:rdepends-unresolved` accumulator across all
            // pkg suffixes; full per-suffix resolution is deferred to
            // a follow-up if vuln-scanner coverage shows it matters.
            let mut all_rdeps: Vec<String> = Vec::new();
            for entries in meta.rdepends.values() {
                for d in entries {
                    if !all_rdeps.contains(d) {
                        all_rdeps.push(d.clone());
                    }
                }
            }
            let arr: Vec<serde_json::Value> =
                all_rdeps.into_iter().map(serde_json::Value::String).collect();
            extra_annotations.insert(
                "mikebom:rdepends-unresolved".to_string(),
                serde_json::Value::Array(arr),
            );
        }
        if !meta.class_extend.is_empty() {
            let arr: Vec<serde_json::Value> = meta
                .class_extend
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();
            extra_annotations.insert(
                "mikebom:yocto-class-extend".to_string(),
                serde_json::Value::Array(arr),
            );
        }
        // DESCRIPTION annotation when it differs from SUMMARY.
        if let (Some(desc), Some(sum)) = (meta.description.as_deref(), meta.summary.as_deref()) {
            let norm_diff = desc.split_whitespace().collect::<Vec<_>>()
                != sum.split_whitespace().collect::<Vec<_>>();
            if norm_diff {
                extra_annotations.insert(
                    "mikebom:yocto-description".to_string(),
                    serde_json::Value::String(desc.to_string()),
                );
            }
        } else if let Some(desc) = meta.description.as_deref() {
            // DESCRIPTION present but no SUMMARY — store it.
            extra_annotations.insert(
                "mikebom:yocto-description".to_string(),
                serde_json::Value::String(desc.to_string()),
            );
        }
    }

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: component_name,
        version: component_version,
        arch: None,
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses,
        lifecycle_scope: None,
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
        // R13: design-tier (declared but not necessarily built).
        sbom_tier: Some("design".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

/// Walk UP from the recipe's directory looking for the enclosing
/// `meta-<name>/` directory (the layer root). Returns the layer's
/// directory name without the `meta-` prefix? No — per contract, the
/// layer's BASENAME verbatim (e.g., `meta-mikebom-fixture`).
///
/// Fallback when no `meta-*/` ancestor is found: returns the path
/// component immediately above the first `recipes-*/` directory.
/// Returns None when neither pattern matches (caller emits no
/// `?layer=` qualifier and no `mikebom:layer-name` annotation).
fn detect_layer_name(recipe_path: &Path) -> Option<String> {
    // Strategy 1: walk up looking for `meta-<name>/`.
    let mut cursor = recipe_path.parent();
    while let Some(dir) = cursor {
        if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("meta-") || name == "meta" {
                return Some(name.to_string());
            }
        }
        cursor = dir.parent();
    }
    // Strategy 2: walk up looking for `recipes-*/` and return its
    // parent's basename.
    let mut last_dir: Option<PathBuf> = None;
    let mut cursor = recipe_path.parent().map(PathBuf::from);
    while let Some(dir) = &cursor {
        if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("recipes-") {
                // Return the parent's basename (the "layer root" by
                // structure even without a `meta-` prefix).
                return dir
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .map(str::to_string);
            }
        }
        last_dir = Some(dir.clone());
        cursor = dir.parent().map(PathBuf::from);
    }
    drop(last_dir);
    None
}

/// FR-002a — detect the host-typed PURL inputs from a recipe's
/// SRC_URI list. Returns `Some((host_token, owner, repo))` when one
/// of the SRC_URI entries is a git URI whose host is in the
/// recognized set and whose path yields an `<owner>/<repo>` shape.
/// Returns `None` otherwise — caller falls through to FR-011's
/// `pkg:generic/...` shape.
fn detect_host_typed_purl_inputs<'a>(
    src_uris: impl Iterator<Item = &'a str>,
) -> Option<(&'static str, String, String)> {
    for uri in src_uris {
        if let Some(parts) = parse_git_src_uri(uri) {
            return Some(parts);
        }
    }
    None
}

/// Parse one SRC_URI entry. Returns the `(host_token, owner, repo)`
/// triple when the entry is a `git://` / `git+https://` / `git+ssh://`
/// URI whose host is in the FR-002a recognized set
/// {github.com, gitlab.com, bitbucket.org, codeberg.org} and whose
/// path has at least `<owner>/<repo>` shape.
fn parse_git_src_uri(uri: &str) -> Option<(&'static str, String, String)> {
    // Strip BitBake-style trailing qualifiers (`;branch=`, `;protocol=`, etc.).
    let (uri_core, _qualifiers) = match uri.find(';') {
        Some(idx) => uri.split_at(idx),
        None => (uri, ""),
    };

    // Recognized git URI prefixes.
    let after_scheme = uri_core
        .strip_prefix("git://")
        .or_else(|| uri_core.strip_prefix("git+https://"))
        .or_else(|| uri_core.strip_prefix("git+ssh://"))
        .or_else(|| uri_core.strip_prefix("https://"))?;

    // For `git+ssh://`, the host segment may include `user@host`.
    let after_userhost = match after_scheme.find('@') {
        Some(idx) => &after_scheme[idx + 1..],
        None => after_scheme,
    };

    let (host, path) = after_userhost.split_once('/')?;
    if path.is_empty() {
        return None;
    }

    let host_token: &'static str = match host {
        "github.com" => "github",
        "gitlab.com" => "gitlab",
        "bitbucket.org" => "bitbucket",
        "codeberg.org" => "codeberg",
        _ => return None,
    };

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let owner = segments[0].to_string();
    let mut repo = segments[1].to_string();
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped.to_string();
    }
    Some((host_token, owner, repo))
}

/// Build the recipe PURL per milestone 128 FR-011: `pkg:generic/<name>@<version>?openembedded=true&layer=<collection>`.
///
/// Aligns with the upstream Yocto-tooling convention (the
/// 145-component balena-OS reference SBOM uses `pkg:generic/` for
/// every component). Migrated from milestone 107's
/// `pkg:bitbake/...?layer=...` shape, which used a mikebom-invented
/// type token not published in the purl-spec.
fn build_bitbake_purl(name: &str, version: &str, layer: Option<&str>) -> Option<Purl> {
    let purl_str = match layer {
        Some(l) => format!(
            "pkg:generic/{}@{}?openembedded=true&layer={}",
            encode_purl_segment(name),
            encode_purl_segment(version),
            encode_purl_segment(l)
        ),
        None => format!(
            "pkg:generic/{}@{}?openembedded=true",
            encode_purl_segment(name),
            encode_purl_segment(version),
        ),
    };
    Purl::new(&purl_str).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn fr002a_github_git_uri_extracts_owner_repo() {
        let parts = parse_git_src_uri("git://github.com/openssl/openssl.git;branch=master");
        assert_eq!(parts, Some(("github", "openssl".to_string(), "openssl".to_string())));
    }

    #[test]
    fn fr002a_github_git_plus_https_uri() {
        let parts = parse_git_src_uri("git+https://github.com/argoproj/argo-workflows.git;branch=main;protocol=https");
        assert_eq!(parts, Some(("github", "argoproj".to_string(), "argo-workflows".to_string())));
    }

    #[test]
    fn fr002a_gitlab_recognized() {
        let parts = parse_git_src_uri("git://gitlab.com/group/project.git");
        assert_eq!(parts, Some(("gitlab", "group".to_string(), "project".to_string())));
    }

    #[test]
    fn fr002a_bitbucket_recognized() {
        let parts = parse_git_src_uri("git://bitbucket.org/team/repo.git");
        assert_eq!(parts, Some(("bitbucket", "team".to_string(), "repo".to_string())));
    }

    #[test]
    fn fr002a_codeberg_recognized() {
        let parts = parse_git_src_uri("git+https://codeberg.org/me/proj.git");
        assert_eq!(parts, Some(("codeberg", "me".to_string(), "proj".to_string())));
    }

    #[test]
    fn fr002a_unrecognized_host_falls_through() {
        // openssl.org isn't in the recognized set.
        assert!(parse_git_src_uri("git://git.openssl.org/openssl.git").is_none());
        // self-hosted gitea
        assert!(parse_git_src_uri("git://gitea.example.com/me/proj.git").is_none());
    }

    #[test]
    fn fr002a_file_and_http_uris_not_git() {
        assert!(parse_git_src_uri("file:///tmp/widget.patch").is_none());
        assert!(parse_git_src_uri("http://example.com/widget-1.0.tar.gz").is_none());
    }

    #[test]
    fn fr002a_git_ssh_handles_user_at_host() {
        let parts = parse_git_src_uri("git+ssh://git@github.com/owner/repo.git");
        assert_eq!(parts, Some(("github", "owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn fr002a_strips_git_suffix_from_repo() {
        let parts = parse_git_src_uri("git://github.com/foo/bar.git");
        assert_eq!(parts.map(|t| t.2), Some("bar".to_string()));
    }

    #[test]
    fn fr002a_detect_from_multiple_src_uris_picks_first_git() {
        let uris = [
            "file://patch.patch",
            "git://github.com/me/proj.git",
            "http://example.com/x.tar.gz",
        ];
        let parts = detect_host_typed_purl_inputs(uris.iter().copied());
        assert_eq!(parts, Some(("github", "me".to_string(), "proj".to_string())));
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn extracts_name_and_version_from_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let recipe = tmp
            .path()
            .join("meta-mikebom-fixture")
            .join("recipes-mikebom")
            .join("mikebom-fixture-lib")
            .join("mikebom-fixture-lib_1.2.3.bb");
        touch(&recipe);
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-lib");
        assert_eq!(entries[0].version, "1.2.3");
    }

    #[test]
    fn emits_layer_qualifier_from_meta_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let recipe = tmp
            .path()
            .join("meta-mikebom-fixture")
            .join("recipes-mikebom")
            .join("mikebom-fixture-lib")
            .join("mikebom-fixture-lib_1.2.3.bb");
        touch(&recipe);
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        // PURL qualifiers are canonicalized to alphabetical order by
        // `Purl::new` — `layer` lex-sorts before `openembedded`.
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:generic/mikebom-fixture-lib@1.2.3?layer=meta-mikebom-fixture&openembedded=true"
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:layer-name")
                .and_then(|v| v.as_str()),
            Some("meta-mikebom-fixture")
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("bitbake-recipe")
        );
    }

    #[test]
    fn unexpanded_variables_skipped_silently() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-shared")
                .join("${PN}_${PV}.bb"),
        );
        // Also include one valid recipe to confirm the valid one still emits.
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-real")
                .join("mikebom-fixture-real_1.0.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-real");
    }

    #[test]
    fn version_only_filename_emits_unknown_version_annotation() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-noversion")
                .join("mikebom-fixture-noversion.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-noversion");
        assert_eq!(entries[0].version, "unknown");
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:version-status")
                .and_then(|v| v.as_str()),
            Some("missing")
        );
    }

    #[test]
    fn bbappend_and_bbclass_files_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-lib")
                .join("mikebom-fixture-lib_1.0.bbappend"),
        );
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("classes")
                .join("mikebom-fixture-helper.bbclass"),
        );
        // Add one real `.bb` to confirm walker is working but is just
        // ignoring the .bbappend and .bbclass.
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-lib")
                .join("mikebom-fixture-lib_1.0.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-lib");
    }

    #[test]
    fn git_version_suffix_preserved_in_version() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-lib")
                .join("mikebom-fixture-lib_1.2.3+git0abc123.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "1.2.3+git0abc123");
    }
}
