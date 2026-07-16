//! Milestone 142 — Scala/SBT ecosystem reader.
//!
//! Discovers SBT-managed Scala projects under the scan root via four
//! input artifacts:
//!
//! - `*.sbt.lock` (JSON lockfile, sbt-dependency-lock plugin) — source-tier
//!   per FR-002. Discovered via the `*.sbt.lock` glob with mandatory Q3
//!   content-shape validation (top-level `lockVersion` integer +
//!   `modules` array keys required). Schema versions 1 + 2 supported per
//!   research §R2; v2 adds per-module SHA-256 `checksums` (FR-011).
//!
//! - `build.sbt` (Scala-DSL build definition) — main-module emission
//!   source per FR-012 + design-tier fallback per FR-005. Regex-extracted
//!   per research §R4 (`name` / `version` / `organization` / `scalaVersion`
//!   settings + `libraryDependencies +=` / `libraryDependencies ++= Seq(...)`
//!   declarations + `lazy val ... = project.in(file("..."))` subproject
//!   declarations for Q2 multi-project union discovery).
//!
//! - `project/Dependencies.scala` (Scala source sidecar) — design-tier
//!   dep declarations per the `val foo = "group" %% "artifact" % "ver"`
//!   convention. v1 regex-extracts the common pattern; computed forms
//!   (`def foo(v: String) = ...`) silently drop per research §R7.
//!
//! - `project/build.properties` (Java-properties file) — SBT-version
//!   pin + optional embedded `scala.version=` key. Drives the Q1
//!   inference cascade's rung 2 when `build.sbt` lacks `scalaVersion`.
//!
//! Four source discriminators with per-source PURL shapes (research §R1):
//!
//! - **scala-sbt-lock** (modern Hex on Maven Central): `pkg:maven/<group>/<artifact>@<version>`.
//!   The lockfile's `name` field is authoritative — it already contains
//!   any Scala-version suffix (`_2.13` / `_3`) that the plugin's resolver
//!   appended at publish time. mikebom does NOT re-append.
//! - **scala-sbt-design** (build.sbt fallback): same PURL shape, but the
//!   Scala-version suffix is derived via the Q1 inference cascade:
//!   (1) explicit `scalaVersion := "..."` → (2) `project/build.properties`
//!   embedded `scala.version=` → (3) default `_2.13` with
//!   `mikebom:scala-version-source = "default-fallback"` per Q1. Each
//!   design-tier `%%` dep carries `mikebom:scala-version-source` for
//!   transparency.
//! - **scala-main-module** (per-subproject root component): one per
//!   surfaced subproject per Q2 union discovery. PURL
//!   `pkg:maven/<organization>/<name><scala-suffix>@<version>` with the
//!   same `%%` semantics; carries `mikebom:component-role = "main-module"`
//!   + F6-clarification `mikebom:scala-version-source` for transparency.
//! - Cross-built libraries (`cats-core_2.13` + `cats-core_3`) emit as
//!   distinct components per FR-003 — the PURLs differ in `name` slot
//!   per Maven Central reality.
//!
//! Multi-project SBT builds (Q2 union discovery): parse the root
//! `build.sbt` for `lazy val <name> = project.in(file("<path>"))` blocks
//! AND walk subdirs for `<subdir>/build.sbt` files; dedup by canonicalized
//! absolute path. `lazy val` declarations win on name+path when both
//! surfaces hit the same dir. Each surfaced subproject emits one
//! main-module + the implicit root project emits its own (Q2 Phase C).
//!
//! `mikebom:source-type` value-set follows the milestone-122/137-141
//! prefixed convention: `scala-sbt-lock` / `scala-sbt-design` /
//! `scala-main-module`. Distinguishes Scala-derived components from
//! milestone-070's `maven-pom`-prefixed values even though both readers
//! emit `pkg:maven/<group>/<artifact>@<version>` PURLs.
//!
//! Zero new Cargo dependencies — reuses workspace `regex`, `serde_json`,
//! `tracing`, `anyhow`. The paren-counted tokenizer mirrors
//! `elixir.rs::tokenize_mix_lock` + `erlang.rs::tokenize_rebar_lock` per
//! research §R4; factor to a shared `package_db/brace_tokenizer.rs` when
//! a 4th DSL-extracted ecosystem reader (e.g., Lua, OCaml) materializes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::hash::{ContentHash, HashAlgorithm};
use mikebom_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

const MAX_SCALA_WALK_DEPTH: usize = 12;

fn should_skip_descent(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".svn"
            | ".hg"
            | "target"
            | "_build"
            | "node_modules"
            | ".idea"
            | ".bloop"
            | ".metals"
            | ".bsp"
    )
}

// -----------------------------------------------------------------------
// Types (T003 + data-model §2)
// -----------------------------------------------------------------------

/// SBT declaration operator. Variants intentionally mirror the
/// SBT-DSL `%` / `%%` / `%%%` operator syntax; the `Percent` postfix is
/// the operator's name, not a coding-convention artifact (rename would
/// lose the spec-cross-reference value). Hence the local
/// `enum_variant_names` allow.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclKind {
    /// `"group" % "artifact" % "ver"` — pure-Java, NO Scala suffix.
    SinglePercent,
    /// `"group" %% "artifact" % "ver"` — Scala-version-suffixed via Q1 cascade.
    DoublePercent,
    /// `"group" %%% "artifact" % "ver"` — Scala.js / Scala Native, warn-and-skip.
    TriplePercent,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SbtLockEntry {
    org: String,
    name: String, // includes Scala-version suffix verbatim from the plugin
    version: String,
    configurations: Vec<String>,
    sha256: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DeclaredSbtDep {
    group: String,
    artifact: String, // BARE artifactId — suffix applied at PURL-build time
    declaration_kind: DeclKind,
    version: String, // raw version string (constraint preserved)
    configuration: Option<String>, // Some("Test") / Some("Provided") / None (Compile default)
    subproject: Option<String>,    // owning subproject name (None = root)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SbtSubproject {
    name: String,
    project_dir: PathBuf,
    build_sbt_path: Option<PathBuf>,
    declared_in_root: bool,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct SbtMainModule {
    organization: Option<String>,
    name_setting: Option<String>,
    version_setting: Option<String>,
    scala_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalaVersionSource {
    BuildSbtExplicit,
    BuildPropertiesEmbedded,
    DefaultFallback,
}

impl ScalaVersionSource {
    fn to_annotation_value(self) -> &'static str {
        match self {
            ScalaVersionSource::BuildSbtExplicit => "build-sbt-explicit",
            ScalaVersionSource::BuildPropertiesEmbedded => "build-properties-embedded",
            ScalaVersionSource::DefaultFallback => "default-fallback",
        }
    }
}

// -----------------------------------------------------------------------
// pub fn read — entry point (T012 / T018 / T024)
// -----------------------------------------------------------------------

pub fn read(
    rootfs: &Path,
    _include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();

    // Phase A — discover all artifacts.
    let lockfile_candidates = discover_sbt_locks(rootfs, exclude_set);
    let build_sbt_paths = discover_build_sbts(rootfs, exclude_set);
    let build_properties_paths = discover_build_properties(rootfs, exclude_set);
    let dependencies_scala_paths = discover_dependencies_scala(rootfs, exclude_set);

    // FR-006 / SC-004: clean no-op when no Scala artifacts present.
    if lockfile_candidates.is_empty()
        && build_sbt_paths.is_empty()
        && build_properties_paths.is_empty()
        && dependencies_scala_paths.is_empty()
    {
        return out;
    }

    // Phase B — parse all *.sbt.lock candidates (with Q3 content-shape gate).
    // Map: lockfile directory → entries.
    let mut lock_data: HashMap<PathBuf, Vec<SbtLockEntry>> = HashMap::new();
    for path in &lockfile_candidates {
        match parse_sbt_lock(path) {
            Ok(entries) => {
                if let Some(dir) = path.parent() {
                    lock_data
                        .entry(dir.to_path_buf())
                        .or_default()
                        .extend(entries);
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "scala: failed to parse *.sbt.lock; skipping (FR-007)",
                );
            }
        }
    }

    // Phase C — parse all build.sbt files. Map: build.sbt path → (main, deps).
    let mut build_sbt_data: HashMap<PathBuf, (SbtMainModule, Vec<DeclaredSbtDep>)> =
        HashMap::new();
    for path in &build_sbt_paths {
        match parse_build_sbt(path) {
            Ok(parsed) => {
                build_sbt_data.insert(path.clone(), parsed);
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "scala: failed to parse build.sbt; skipping (FR-007)",
                );
            }
        }
    }

    // Phase D — parse Dependencies.scala sidecars (T017 + T025).
    // Map: parent-project-dir → extracted deps (applies to that project tree).
    let mut deps_scala_data: HashMap<PathBuf, Vec<DeclaredSbtDep>> = HashMap::new();
    for path in &dependencies_scala_paths {
        if let Some(parent) = path.parent().and_then(|p| p.parent()) {
            let extracted = parse_dependencies_scala(path);
            if !extracted.is_empty() {
                deps_scala_data
                    .entry(parent.to_path_buf())
                    .or_default()
                    .extend(extracted);
            }
        }
    }

    // Phase E — emit lockfile-derived components (data-model §3.1).
    for entries in lock_data.values() {
        for entry in entries {
            let component = build_lockfile_component(entry);
            let purl_key = component.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(component);
            }
        }
    }

    // Phase F — Q2 union-discovery of subprojects (T023).
    let subprojects = discover_subprojects(rootfs, &build_sbt_paths, &build_sbt_data);

    // Phase G — for each surfaced subproject: emit main-module + design-tier
    // deps when no sibling lockfile present.
    for subproj in &subprojects {
        let canonical_dir = std::fs::canonicalize(&subproj.project_dir)
            .unwrap_or_else(|_| subproj.project_dir.clone());
        let has_lockfile = lock_data.keys().any(|lock_dir| {
            let canonical_lock_dir =
                std::fs::canonicalize(lock_dir).unwrap_or_else(|_| lock_dir.clone());
            canonical_lock_dir == canonical_dir
        });

        // Find this subproject's build.sbt parsed output (if any).
        let (main, declared_deps): (SbtMainModule, Vec<DeclaredSbtDep>) =
            match &subproj.build_sbt_path {
                Some(p) => build_sbt_data
                    .get(p)
                    .cloned()
                    .unwrap_or_else(|| (SbtMainModule::default(), Vec::new())),
                None => (SbtMainModule::default(), Vec::new()),
            };

        // Per F3 remediation: read project/build.properties for Q1 cascade rung 2.
        let build_properties_text = std::fs::read_to_string(
            subproj
                .project_dir
                .join("project")
                .join("build.properties"),
        )
        .ok();

        // Emit main-module (T016 + F6 scala-version-source annotation).
        if let Some(main_component) = build_main_module_component(
            subproj,
            &main,
            build_properties_text.as_deref(),
            has_lockfile,
        ) {
            let purl_key = main_component.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_component);
            }
        }

        // Design-tier emission (FR-005): only when no sibling lockfile.
        if !has_lockfile {
            // Compute Scala-version cascade once per subproject.
            let (scala_version, source) =
                derive_scala_version(&main, build_properties_text.as_deref());

            // Merge build.sbt deps with Dependencies.scala-derived deps for
            // this subproject (research §R7 + T025).
            let mut combined_deps = declared_deps.clone();
            if let Some(extra) = deps_scala_data.get(&canonical_dir) {
                combined_deps.extend(extra.iter().cloned());
            }
            for dep in &combined_deps {
                if let Some(component) = build_design_tier_component(
                    dep,
                    scala_version.as_deref(),
                    source,
                    subproj.build_sbt_path.as_deref().unwrap_or(&subproj.project_dir),
                ) {
                    let purl_key = component.purl.as_str().to_string();
                    if seen_purls.insert(purl_key) {
                        out.push(component);
                    }
                }
            }
        }
    }

    out
}

// -----------------------------------------------------------------------
// Discovery helpers (T009 + research §R10)
// -----------------------------------------------------------------------

fn discover_sbt_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_SCALA_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                return;
            };
            if name.ends_with(".sbt.lock") {
                out.push(path.to_path_buf());
            }
        }
    });
    out.sort();
    out
}

fn discover_build_sbts(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_SCALA_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("build.sbt") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn discover_build_properties(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_SCALA_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path.file_name().and_then(|s| s.to_str()) == Some("build.properties")
            && path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                == Some("project")
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn discover_dependencies_scala(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_SCALA_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path.file_name().and_then(|s| s.to_str()) == Some("Dependencies.scala")
            && path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                == Some("project")
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

// -----------------------------------------------------------------------
// Q3 content-shape validation (T005)
// -----------------------------------------------------------------------

fn validate_sbt_lock_shape(json: &serde_json::Value) -> bool {
    let obj = match json.as_object() {
        Some(o) => o,
        None => return false,
    };
    let has_lock_version = obj
        .get("lockVersion")
        .map(|v| v.is_i64() || v.is_u64())
        .unwrap_or(false);
    let has_modules = obj.get("modules").map(|v| v.is_array()).unwrap_or(false);
    has_lock_version && has_modules
}

// -----------------------------------------------------------------------
// *.sbt.lock parser (T010 + research §R2)
// -----------------------------------------------------------------------

fn parse_sbt_lock(path: &Path) -> anyhow::Result<Vec<SbtLockEntry>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("JSON parse failed: {e}"))?;
    if !validate_sbt_lock_shape(&json) {
        anyhow::bail!(
            "Q3 content-shape validation failed — missing top-level lockVersion and/or modules keys"
        );
    }
    let modules = match json.get("modules").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };
    let mut out: Vec<SbtLockEntry> = Vec::new();
    for module in modules {
        let org = match module.get("org").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let name = match module.get("name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let version = match module.get("version").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let configurations: Vec<String> = module
            .get("configurations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        // FR-011: extract SHA-256 from v2 checksums array.
        let sha256 = module
            .get("checksums")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find_map(|c| {
                    let ty = c.get("type").and_then(|v| v.as_str())?;
                    if ty.eq_ignore_ascii_case("SHA-256") {
                        c.get("checksum")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                    } else {
                        None
                    }
                })
            });
        out.push(SbtLockEntry {
            org,
            name,
            version,
            configurations,
            sha256,
        });
    }
    Ok(out)
}

// -----------------------------------------------------------------------
// build.sbt parser (T015 + research §R4)
// -----------------------------------------------------------------------

fn name_setting_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*name\s*:=\s*"([^"]+)""#).expect("static name regex")
    })
}

fn version_setting_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*version\s*:=\s*"([^"]+)""#).expect("static version regex")
    })
}

fn organization_setting_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*organization\s*:=\s*"([^"]+)""#)
            .expect("static organization regex")
    })
}

fn scala_version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*(?:ThisBuild\s*/\s*)?scalaVersion\s*:=\s*"([^"]+)""#)
            .expect("static scalaVersion regex")
    })
}

fn library_dependency_single_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Capture: 1 = group, 2 = % count (1-3), 3 = artifact, 4 = version,
        // 5 = optional configuration suffix (Test/Provided/Runtime/Compile/IntegrationTest).
        Regex::new(
            r#"libraryDependencies\s*\+=\s*"([^"]+)"\s*(%{1,3})\s*"([^"]+)"\s*%\s*"([^"]+)"(?:\s*%\s*([A-Za-z]+))?"#,
        )
        .expect("static libdep single regex")
    })
}

fn sbt_dep_tuple_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#""([^"]+)"\s*(%{1,3})\s*"([^"]+)"\s*%\s*"([^"]+)"(?:\s*%\s*([A-Za-z]+))?"#,
        )
        .expect("static dep tuple regex")
    })
}

fn lazy_val_project_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"lazy\s+val\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:\(?\s*)?project\s*\.\s*in\s*\(\s*file\s*\(\s*"([^"]+)"\s*\)\s*\)"#,
        )
        .expect("static lazy val regex")
    })
}

fn dependencies_val_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?:val|lazy\s+val)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"([^"]+)"\s*(%{1,3})\s*"([^"]+)"\s*%\s*"([^"]+)"(?:\s*%\s*([A-Za-z]+))?"#,
        )
        .expect("static deps.scala val regex")
    })
}

fn parse_build_sbt(path: &Path) -> anyhow::Result<(SbtMainModule, Vec<DeclaredSbtDep>)> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;

    let main = SbtMainModule {
        organization: organization_setting_re()
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string())),
        name_setting: name_setting_re()
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string())),
        version_setting: version_setting_re()
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string())),
        scala_version: scala_version_re()
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string())),
    };

    let mut deps: Vec<DeclaredSbtDep> = Vec::new();

    // Single-add form: libraryDependencies += "g" %% "a" % "v" [% Config]
    for caps in library_dependency_single_re().captures_iter(&text) {
        if let Some(dep) = build_dep_from_captures(&caps) {
            deps.push(dep);
        }
    }

    // Multi-add Seq form: libraryDependencies ++= Seq(...)
    for seq_body in extract_seq_bodies(&text) {
        for caps in sbt_dep_tuple_re().captures_iter(&seq_body) {
            if let Some(dep) = build_dep_from_captures(&caps) {
                deps.push(dep);
            }
        }
    }

    Ok((main, deps))
}

fn build_dep_from_captures(caps: &regex::Captures<'_>) -> Option<DeclaredSbtDep> {
    let (group_idx, percent_idx, artifact_idx, version_idx, config_idx) =
        if caps.len() == 6 { (1, 2, 3, 4, 5) } else { return None };
    let group = caps.get(group_idx)?.as_str().to_string();
    let percent = caps.get(percent_idx)?.as_str();
    let artifact = caps.get(artifact_idx)?.as_str().to_string();
    let version = caps.get(version_idx)?.as_str().to_string();
    let configuration = caps.get(config_idx).map(|m| m.as_str().to_string());
    let declaration_kind = match percent.len() {
        1 => DeclKind::SinglePercent,
        2 => DeclKind::DoublePercent,
        3 => DeclKind::TriplePercent,
        _ => return None,
    };
    if declaration_kind == DeclKind::TriplePercent {
        tracing::debug!(
            group = %group,
            artifact = %artifact,
            "scala: skipping %%% triple-percent declaration (Scala.js/Native is Out-of-Scope for v1)",
        );
        return None;
    }
    Some(DeclaredSbtDep {
        group,
        artifact,
        declaration_kind,
        version,
        configuration,
        subproject: None,
    })
}

/// Extract bodies of `libraryDependencies ++= Seq(...)` blocks via
/// paren-counted tokenization. Mirrors elixir.rs::tokenize_mix_lock +
/// erlang.rs::tokenize_rebar_lock shape per research §R4; factor to a
/// shared `package_db/brace_tokenizer.rs` when a 4th DSL-extracted
/// ecosystem reader needs the helper.
fn extract_seq_bodies(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let pattern = b"libraryDependencies";
    let mut idx = 0usize;
    while idx + pattern.len() < bytes.len() {
        if &bytes[idx..idx + pattern.len()] == pattern {
            // Skip past whitespace + `++=` / `+=` operator.
            let mut i = idx + pattern.len();
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            // Match ++=
            let has_plus_plus_eq = i + 2 < bytes.len()
                && bytes[i] == b'+'
                && bytes[i + 1] == b'+'
                && bytes[i + 2] == b'=';
            if !has_plus_plus_eq {
                idx += 1;
                continue;
            }
            i += 3;
            // Skip whitespace + optional `Seq(`
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n') {
                i += 1;
            }
            let has_seq = i + 4 < bytes.len()
                && &bytes[i..i + 4] == b"Seq(";
            if !has_seq {
                idx += 1;
                continue;
            }
            i += 4; // past `Seq(`
            let body_start = i;
            let mut depth = 1i32;
            let mut in_str = false;
            let mut escape = false;
            while i < bytes.len() && depth > 0 {
                let c = bytes[i];
                if escape {
                    escape = false;
                } else if in_str {
                    if c == b'\\' {
                        escape = true;
                    } else if c == b'"' {
                        in_str = false;
                    }
                } else {
                    match c {
                        b'"' => in_str = true,
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                }
                if depth == 0 {
                    break;
                }
                i += 1;
            }
            if depth == 0 {
                let body = String::from_utf8_lossy(&bytes[body_start..i]).into_owned();
                out.push(body);
            }
            idx = i;
        } else {
            idx += 1;
        }
    }
    out
}

// -----------------------------------------------------------------------
// Dependencies.scala parser (T017 + research §R7)
// -----------------------------------------------------------------------

fn parse_dependencies_scala(path: &Path) -> Vec<DeclaredSbtDep> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "scala: failed to read Dependencies.scala; skipping",
            );
            return Vec::new();
        }
    };
    let mut out: Vec<DeclaredSbtDep> = Vec::new();
    for caps in dependencies_val_re().captures_iter(&text) {
        // Captures: 1 = ident (ignored), 2 = group, 3 = % count,
        // 4 = artifact, 5 = version, 6 = optional configuration.
        let group = match caps.get(2) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let percent = match caps.get(3) {
            Some(m) => m.as_str(),
            None => continue,
        };
        let artifact = match caps.get(4) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let version = match caps.get(5) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let configuration = caps.get(6).map(|m| m.as_str().to_string());
        let declaration_kind = match percent.len() {
            1 => DeclKind::SinglePercent,
            2 => DeclKind::DoublePercent,
            3 => DeclKind::TriplePercent,
            _ => continue,
        };
        if declaration_kind == DeclKind::TriplePercent {
            continue;
        }
        out.push(DeclaredSbtDep {
            group,
            artifact,
            declaration_kind,
            version,
            configuration,
            subproject: None,
        });
    }
    out
}

// -----------------------------------------------------------------------
// Q1 inference cascade (T006 + research §R3)
// -----------------------------------------------------------------------

fn derive_scala_version(
    main: &SbtMainModule,
    build_properties_text: Option<&str>,
) -> (Option<String>, ScalaVersionSource) {
    // Rung 1: explicit scalaVersion := "..." in build.sbt.
    if let Some(v) = &main.scala_version {
        return (Some(v.clone()), ScalaVersionSource::BuildSbtExplicit);
    }
    // Rung 2: project/build.properties embedded scala.version=...
    if let Some(text) = build_properties_text {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r#"(?m)^\s*scala\.version\s*=\s*(\S+)\s*$"#)
                .expect("static scala.version regex")
        });
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                return (
                    Some(m.as_str().to_string()),
                    ScalaVersionSource::BuildPropertiesEmbedded,
                );
            }
        }
    }
    // Rung 3: default fallback per Q1.
    (Some("2.13".to_string()), ScalaVersionSource::DefaultFallback)
}

// -----------------------------------------------------------------------
// apply_scala_suffix (T004 + data-model §4)
// -----------------------------------------------------------------------

fn apply_scala_suffix(
    kind: DeclKind,
    bare_artifact: &str,
    scala_version: Option<&str>,
) -> String {
    match kind {
        DeclKind::SinglePercent => bare_artifact.to_string(),
        DeclKind::TriplePercent => bare_artifact.to_string(), // upstream warns-and-skips
        DeclKind::DoublePercent => {
            let suffix = match scala_version {
                Some(v) if v.starts_with("3.") || v == "3" => "_3".to_string(),
                Some(v) => {
                    let mut iter = v.split('.');
                    let major = iter.next().unwrap_or("2");
                    let minor = iter.next().unwrap_or("13");
                    format!("_{major}.{minor}")
                }
                None => "_2.13".to_string(), // Q1 default fallback
            };
            format!("{bare_artifact}{suffix}")
        }
    }
}

// -----------------------------------------------------------------------
// Multi-project Q2 union discovery (T023 + research §R5)
// -----------------------------------------------------------------------

fn discover_subprojects(
    rootfs: &Path,
    build_sbt_paths: &[PathBuf],
    build_sbt_data: &HashMap<PathBuf, (SbtMainModule, Vec<DeclaredSbtDep>)>,
) -> Vec<SbtSubproject> {
    // Identify the "root" build.sbt — the shallowest one under rootfs.
    let canonical_root = std::fs::canonicalize(rootfs).unwrap_or_else(|_| rootfs.to_path_buf());
    let root_build_sbt = build_sbt_paths
        .iter()
        .min_by_key(|p| p.components().count())
        .cloned();

    let mut by_dir: HashMap<PathBuf, SbtSubproject> = HashMap::new();

    // Phase A — parse root build.sbt for `lazy val <name> = project.in(file("<path>"))`.
    if let Some(root_path) = &root_build_sbt {
        if let Ok(text) = std::fs::read_to_string(root_path) {
            let root_dir = root_path.parent().unwrap_or(&canonical_root);
            for caps in lazy_val_project_re().captures_iter(&text) {
                let name = match caps.get(1) {
                    Some(m) => m.as_str().to_string(),
                    None => continue,
                };
                let rel_path = match caps.get(2) {
                    Some(m) => m.as_str().to_string(),
                    None => continue,
                };
                let subproj_dir = root_dir.join(&rel_path);
                let canonical = std::fs::canonicalize(&subproj_dir)
                    .unwrap_or_else(|_| subproj_dir.clone());
                let sub_build_sbt = canonical.join("build.sbt");
                let has_subproject_build_sbt = build_sbt_data
                    .keys()
                    .any(|p| std::fs::canonicalize(p)
                        .map(|c| c == sub_build_sbt)
                        .unwrap_or(false));
                by_dir.insert(
                    canonical.clone(),
                    SbtSubproject {
                        name,
                        project_dir: canonical,
                        build_sbt_path: if has_subproject_build_sbt {
                            Some(sub_build_sbt)
                        } else {
                            None
                        },
                        declared_in_root: true,
                    },
                );
            }
        }
    }

    // Phase B — walk subdirs for <subdir>/build.sbt; emit any not already
    // covered by Phase A.
    for build_sbt_path in build_sbt_paths {
        let dir = match build_sbt_path.parent() {
            Some(d) => d,
            None => continue,
        };
        let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if canonical == canonical_root {
            continue; // root project handled in Phase C
        }
        if by_dir.contains_key(&canonical) {
            continue; // already covered by Phase A
        }
        let name = canonical
            .file_name()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_else(|| "unknown".to_string());
        by_dir.insert(
            canonical.clone(),
            SbtSubproject {
                name,
                project_dir: canonical,
                build_sbt_path: Some(build_sbt_path.clone()),
                declared_in_root: false,
            },
        );
    }

    // Phase C — emit the implicit root project (always).
    let root_name = canonical_root
        .file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .unwrap_or_else(|| "root".to_string());
    let root_subproj = SbtSubproject {
        name: root_name,
        project_dir: canonical_root.clone(),
        build_sbt_path: root_build_sbt,
        declared_in_root: false,
    };
    by_dir.insert(canonical_root, root_subproj);

    let mut out: Vec<SbtSubproject> = by_dir.into_values().collect();
    out.sort_by(|a, b| a.project_dir.cmp(&b.project_dir));
    out
}

// -----------------------------------------------------------------------
// Component builders (T011 + T016 + T021)
// -----------------------------------------------------------------------

fn build_lockfile_component(entry: &SbtLockEntry) -> PackageDbEntry {
    // Per FR-002: entry.name ALREADY includes any Scala-version suffix.
    let purl_str = format!(
        "pkg:maven/{org}/{name}@{version}",
        org = entry.org,
        name = entry.name,
        version = entry.version,
    );
    let purl = Purl::new(&purl_str).unwrap_or_else(|_| {
        tracing::warn!(
            purl = %purl_str,
            "scala: malformed PURL from lockfile entry; falling back to bare",
        );
        // Last-resort fallback — this should never happen in practice.
        Purl::new("pkg:maven/unknown/unknown@unknown").expect("fallback PURL")
    });

    let mut hashes = Vec::new();
    if let Some(sha) = &entry.sha256 {
        if let Ok(h) = ContentHash::with_algorithm(HashAlgorithm::Sha256, sha) {
            hashes.push(h);
        }
    }

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("scala-sbt-lock".to_string()),
    );

    // FR-008: lifecycle-scope = Development when any configuration equals "test"
    // (case-insensitive). Conservative interpretation per F4 finding.
    let lifecycle_scope = if entry
        .configurations
        .iter()
        .any(|c| c.eq_ignore_ascii_case("test"))
    {
        LifecycleScope::Development
    } else {
        LifecycleScope::Runtime
    };

    PackageDbEntry {
        purl,
        name: entry.name.clone(),
        version: entry.version.clone(),
        arch: None,
        source_path: String::new(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(lifecycle_scope),
        requirement_ranges: Vec::new(),
        source_type: Some("scala-sbt-lock".to_string()),
        buildinfo_status: None,
        sbom_tier: Some("source".to_string()),
        evidence_kind: Some("sbt-lock".to_string()),
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
        hashes,
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    }
}

fn build_main_module_component(
    subproj: &SbtSubproject,
    main: &SbtMainModule,
    build_properties_text: Option<&str>,
    doc_has_lockfile: bool,
) -> Option<PackageDbEntry> {
    let organization = main
        .organization
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let name = main
        .name_setting
        .clone()
        .unwrap_or_else(|| subproj.name.clone());
    // Milestone 197 US3 (#567): emit versionless canonical PURL per
    // purl-spec when the sbt build has no `version := "..."` — matches
    // m191 fix pattern.
    let raw_version = main.version_setting.clone();
    let version = raw_version
        .clone()
        .unwrap_or_else(|| "0.0.0-unknown".to_string());

    if name.is_empty() {
        return None;
    }

    // Q1 cascade → derive Scala-version + source for suffix application + F6 annotation.
    let (scala_version, source) = derive_scala_version(main, build_properties_text);
    // Main-modules use %% semantics (Scala-published artifacts are suffixed).
    let artifactid = apply_scala_suffix(DeclKind::DoublePercent, &name, scala_version.as_deref());

    let purl_str = if raw_version.as_deref().unwrap_or("").is_empty() {
        format!("pkg:maven/{organization}/{artifactid}")
    } else {
        format!("pkg:maven/{organization}/{artifactid}@{version}")
    };
    let purl = match Purl::new(&purl_str) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                purl = %purl_str,
                error = ?err,
                "scala: skipping main-module with non-PURL-safe form",
            );
            return None;
        }
    };

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("scala-main-module".to_string()),
    );
    // Per F6 remediation: surface the Scala-version-source on the main-module
    // (matches the design-tier %% deps' transparency convention).
    extra_annotations.insert(
        "mikebom:scala-version-source".to_string(),
        serde_json::Value::String(source.to_annotation_value().to_string()),
    );

    let sbom_tier = if doc_has_lockfile { "source" } else { "design" };

    Some(PackageDbEntry {
        purl,
        name,
        version,
        arch: None,
        source_path: subproj
            .build_sbt_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| subproj.project_dir.to_string_lossy().into_owned()),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("scala-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some("sbt-build".to_string()),
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
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

fn build_design_tier_component(
    dep: &DeclaredSbtDep,
    subproject_scala_version: Option<&str>,
    scala_version_source: ScalaVersionSource,
    source_path_hint: &Path,
) -> Option<PackageDbEntry> {
    if dep.declaration_kind == DeclKind::TriplePercent {
        // Out-of-Scope warn-and-skip per spec.
        return None;
    }
    let artifactid = apply_scala_suffix(
        dep.declaration_kind,
        &dep.artifact,
        subproject_scala_version,
    );
    let sanitized_version = sanitize_purl_version(&dep.version);
    let purl_str = format!(
        "pkg:maven/{group}/{artifact}@{version}",
        group = dep.group,
        artifact = artifactid,
        version = sanitized_version,
    );
    let purl = match Purl::new(&purl_str) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                name = %dep.artifact,
                purl = %purl_str,
                error = ?err,
                "scala: skipping design-tier entry with non-PURL-safe form",
            );
            return None;
        }
    };

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("scala-sbt-design".to_string()),
    );
    // F6 + Q1: emit mikebom:scala-version-source ONLY on %% deps (not on % pure-Java).
    if dep.declaration_kind == DeclKind::DoublePercent {
        extra_annotations.insert(
            "mikebom:scala-version-source".to_string(),
            serde_json::Value::String(scala_version_source.to_annotation_value().to_string()),
        );
    }

    let lifecycle_scope = match dep.configuration.as_deref() {
        Some("Test") => LifecycleScope::Development,
        _ => LifecycleScope::Runtime,
    };

    Some(PackageDbEntry {
        purl,
        name: dep.artifact.clone(),
        version: sanitized_version,
        arch: None,
        source_path: source_path_hint.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(lifecycle_scope),
        requirement_ranges: vec![dep.version.clone()],
        source_type: Some("scala-sbt-design".to_string()),
        buildinfo_status: None,
        sbom_tier: Some("design".to_string()),
        evidence_kind: Some("sbt-build".to_string()),
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
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

// -----------------------------------------------------------------------
// PURL string utilities (matches elixir.rs + erlang.rs conventions)
// -----------------------------------------------------------------------

fn sanitize_purl_version(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '?' | '#' | ' ' => '_',
            other => other,
        })
        .collect()
}

// -----------------------------------------------------------------------
// Unit tests
// -----------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn apply_scala_suffix_single_percent_no_suffix() {
        let out = apply_scala_suffix(DeclKind::SinglePercent, "postgresql", Some("2.13.12"));
        assert_eq!(out, "postgresql");
    }

    #[test]
    fn apply_scala_suffix_double_percent_scala_2_13() {
        let out = apply_scala_suffix(DeclKind::DoublePercent, "cats-core", Some("2.13.12"));
        assert_eq!(out, "cats-core_2.13");
    }

    #[test]
    fn apply_scala_suffix_double_percent_scala_3_drops_patch() {
        let out = apply_scala_suffix(DeclKind::DoublePercent, "cats-core", Some("3.3.1"));
        assert_eq!(out, "cats-core_3");
    }

    #[test]
    fn apply_scala_suffix_double_percent_default_fallback() {
        let out = apply_scala_suffix(DeclKind::DoublePercent, "cats-core", None);
        assert_eq!(out, "cats-core_2.13");
    }

    #[test]
    fn validate_sbt_lock_shape_accepts_valid() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"lockVersion":1,"modules":[]}"#).unwrap();
        assert!(validate_sbt_lock_shape(&json));
    }

    #[test]
    fn validate_sbt_lock_shape_rejects_missing_lockversion() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"modules":[]}"#).unwrap();
        assert!(!validate_sbt_lock_shape(&json));
    }

    #[test]
    fn validate_sbt_lock_shape_rejects_missing_modules() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"lockVersion":1}"#).unwrap();
        assert!(!validate_sbt_lock_shape(&json));
    }

    #[test]
    fn validate_sbt_lock_shape_rejects_non_array_modules() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"lockVersion":1,"modules":"not-an-array"}"#).unwrap();
        assert!(!validate_sbt_lock_shape(&json));
    }

    #[test]
    fn parse_sbt_lock_schema_v1() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("build.sbt.lock");
        std::fs::write(
            &lock,
            r#"{"lockVersion":1,"modules":[
              {"org":"org.typelevel","name":"cats-core_2.13","version":"2.10.0","configurations":["compile"]}
            ]}"#,
        ).unwrap();
        let entries = parse_sbt_lock(&lock).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].org, "org.typelevel");
        assert_eq!(entries[0].name, "cats-core_2.13");
        assert_eq!(entries[0].version, "2.10.0");
        assert_eq!(entries[0].sha256, None);
    }

    #[test]
    fn parse_sbt_lock_schema_v2_with_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("build.sbt.lock");
        std::fs::write(
            &lock,
            r#"{"lockVersion":2,"modules":[
              {"org":"org.typelevel","name":"cats-core_2.13","version":"2.10.0","configurations":["compile"],
               "checksums":[{"name":"x.jar","type":"SHA-256","checksum":"abc123"}]}
            ]}"#,
        )
        .unwrap();
        let entries = parse_sbt_lock(&lock).unwrap();
        assert_eq!(entries[0].sha256, Some("abc123".to_string()));
    }

    #[test]
    fn parse_sbt_lock_rejects_invalid_shape() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("build.sbt.lock");
        std::fs::write(&lock, r#"{"unrelated":"json"}"#).unwrap();
        let result = parse_sbt_lock(&lock);
        assert!(result.is_err());
    }

    #[test]
    fn derive_scala_version_explicit_wins() {
        let main = SbtMainModule {
            scala_version: Some("2.13.12".to_string()),
            ..Default::default()
        };
        let (v, src) = derive_scala_version(&main, None);
        assert_eq!(v, Some("2.13.12".to_string()));
        assert_eq!(src, ScalaVersionSource::BuildSbtExplicit);
    }

    #[test]
    fn derive_scala_version_build_properties_rung_2() {
        let main = SbtMainModule::default();
        let (v, src) = derive_scala_version(&main, Some("sbt.version=1.10.0\nscala.version=2.13.10"));
        assert_eq!(v, Some("2.13.10".to_string()));
        assert_eq!(src, ScalaVersionSource::BuildPropertiesEmbedded);
    }

    #[test]
    fn derive_scala_version_default_fallback() {
        let main = SbtMainModule::default();
        let (v, src) = derive_scala_version(&main, None);
        assert_eq!(v, Some("2.13".to_string()));
        assert_eq!(src, ScalaVersionSource::DefaultFallback);
    }

    #[test]
    fn parse_build_sbt_settings_and_single_deps() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("build.sbt");
        std::fs::write(
            &p,
            r#"
name := "my-app"
version := "1.2.3"
organization := "com.example"
scalaVersion := "2.13.12"

libraryDependencies += "org.typelevel" %% "cats-core" % "2.10.0"
libraryDependencies += "org.postgresql" % "postgresql" % "42.7.0"
"#,
        )
        .unwrap();
        let (main, deps) = parse_build_sbt(&p).unwrap();
        assert_eq!(main.name_setting.as_deref(), Some("my-app"));
        assert_eq!(main.version_setting.as_deref(), Some("1.2.3"));
        assert_eq!(main.organization.as_deref(), Some("com.example"));
        assert_eq!(main.scala_version.as_deref(), Some("2.13.12"));
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|d| d.artifact == "cats-core"
            && d.declaration_kind == DeclKind::DoublePercent));
        assert!(deps.iter().any(|d| d.artifact == "postgresql"
            && d.declaration_kind == DeclKind::SinglePercent));
    }

    #[test]
    fn parse_build_sbt_seq_form_extracts_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("build.sbt");
        std::fs::write(
            &p,
            r#"
libraryDependencies ++= Seq(
  "org.typelevel" %% "cats-core" % "2.10.0",
  "com.typesafe.akka" %% "akka-actor" % "2.6.20",
  "org.scalatest" %% "scalatest" % "3.2.18" % Test
)
"#,
        )
        .unwrap();
        let (_main, deps) = parse_build_sbt(&p).unwrap();
        assert_eq!(deps.len(), 3);
        let scalatest = deps.iter().find(|d| d.artifact == "scalatest").unwrap();
        assert_eq!(scalatest.configuration.as_deref(), Some("Test"));
    }

    #[test]
    fn parse_build_sbt_triple_percent_warns_and_skips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("build.sbt");
        std::fs::write(
            &p,
            r#"libraryDependencies += "org.scala-js" %%% "scalajs-dom" % "2.4.0""#,
        )
        .unwrap();
        let (_main, deps) = parse_build_sbt(&p).unwrap();
        assert!(deps.is_empty(), "triple-percent should be skipped");
    }

    #[test]
    fn parse_build_sbt_lazy_val_subprojects_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("build.sbt");
        std::fs::write(
            &p,
            r#"
lazy val core = project.in(file("core"))
lazy val server = project.in(file("server")).settings(name := "server")
"#,
        )
        .unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        let captures: Vec<(String, String)> = lazy_val_project_re()
            .captures_iter(&text)
            .filter_map(|c| {
                Some((c.get(1)?.as_str().to_string(), c.get(2)?.as_str().to_string()))
            })
            .collect();
        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0], ("core".to_string(), "core".to_string()));
        assert_eq!(captures[1], ("server".to_string(), "server".to_string()));
    }

    #[test]
    fn parse_dependencies_scala_extracts_val_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Dependencies.scala");
        std::fs::write(
            &p,
            r#"
object Dependencies {
  val cats = "org.typelevel" %% "cats-core" % "2.10.0"
  val postgres = "org.postgresql" % "postgresql" % "42.7.0"
}
"#,
        )
        .unwrap();
        let deps = parse_dependencies_scala(&p);
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|d| d.artifact == "cats-core"
            && d.declaration_kind == DeclKind::DoublePercent));
        assert!(deps.iter().any(|d| d.artifact == "postgresql"
            && d.declaration_kind == DeclKind::SinglePercent));
    }

    #[test]
    fn build_lockfile_component_purl_shape() {
        let entry = SbtLockEntry {
            org: "org.typelevel".to_string(),
            name: "cats-core_2.13".to_string(),
            version: "2.10.0".to_string(),
            configurations: vec!["compile".to_string()],
            sha256: None,
        };
        let component = build_lockfile_component(&entry);
        assert_eq!(component.purl.as_str(), "pkg:maven/org.typelevel/cats-core_2.13@2.10.0");
    }

    #[test]
    fn build_lockfile_component_test_config_maps_to_dev_scope() {
        let entry = SbtLockEntry {
            org: "org.scalatest".to_string(),
            name: "scalatest_2.13".to_string(),
            version: "3.2.18".to_string(),
            configurations: vec!["test".to_string()],
            sha256: None,
        };
        let component = build_lockfile_component(&entry);
        assert_eq!(component.lifecycle_scope, Some(LifecycleScope::Development));
    }

    #[test]
    fn cross_built_purls_are_distinct() {
        let cats_2_13 = SbtLockEntry {
            org: "org.typelevel".to_string(),
            name: "cats-core_2.13".to_string(),
            version: "2.10.0".to_string(),
            configurations: vec![],
            sha256: None,
        };
        let cats_3 = SbtLockEntry {
            org: "org.typelevel".to_string(),
            name: "cats-core_3".to_string(),
            version: "2.10.0".to_string(),
            configurations: vec![],
            sha256: None,
        };
        let c1 = build_lockfile_component(&cats_2_13);
        let c2 = build_lockfile_component(&cats_3);
        assert_ne!(c1.purl.as_str(), c2.purl.as_str());
    }
}
