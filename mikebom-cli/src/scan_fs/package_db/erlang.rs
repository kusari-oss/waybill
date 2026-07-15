//! Milestone 141 — Erlang/OTP rebar3 ecosystem reader.
//!
//! Discovers Erlang/OTP rebar3 projects under the scan root via three
//! input artifacts:
//!
//! - `rebar.lock` (Erlang-term-syntax tuple-list literal) — source-tier
//!   per FR-002. Regex-tokenized with brace-counted multi-line tuple
//!   handling. Three shape variants dispatched at parse time per
//!   research §R2:
//!   - Modern Hex (rebar3 3.7+):
//!     `{<<"name">>, {pkg, <<"name">>, <<"version">>[, <<"sha256">>]}, depth}`
//!   - Modern Hex with private-org map-form (rebar3 3.13+):
//!     `{<<"name">>, {pkg, ..., #{repo => <<"hexpm:org">>}}, depth}`
//!   - Legacy Hex (pre-rebar3-3.7):
//!     `{<<"name">>, <<"version">>, depth}`
//!   - Git:
//!     `{<<"name">>, {git, "url", {ref|tag|branch, "value"}}, depth}`
//!
//! - `rebar.config` (Erlang source) — design-tier fallback per FR-005 +
//!   profile-scope source per FR-008. Regex-extracted `{deps, [...]}`
//!   block + `{profiles, [{<env>, [{deps, [...]}]}]}` blocks. No Erlang
//!   runtime evaluation.
//!
//! - `*.app.src` (OTP application descriptor) — main-module emission
//!   source per FR-012 + OTP-runtime-libs source per FR-003 otp-runtime
//!   branch. Three keyword lists extracted per Q3:
//!   - `{applications, [<atoms>]}` (required runtime deps)
//!   - `{included_applications, [<atoms>]}` (embedded sub-apps; OTP R6+)
//!   - `{optional_applications, [<atoms>]}` (soft deps; OTP 26+)
//!
//! Four source discriminators with per-source PURL shapes (research §R1):
//!
//! - **hex** (default `"hexpm"` repo): `pkg:hex/<lc-name>@<version>`.
//! - **hex (private org `"hexpm:<org>"`)**: `pkg:hex/<org>/<lc-name>@<version>?repository_url=https://repo.hex.pm`.
//! - **git**: `pkg:generic/<name>@<resolved-ref>?vcs_url=git+<url>` —
//!   purl-spec does NOT bless `vcs_url=` for `hex` type, so git-source
//!   uses `pkg:generic/` per the milestone-140 precedent.
//! - **otp-runtime**: `pkg:generic/<lib-name>@unspecified` placeholder
//!   per Q1 — emitted for ALL atoms in `applications:`/`included_applications:`/
//!   `optional_applications:` that don't appear in the lockfile.
//!   Allowlisted atoms additionally carry `mikebom:otp-stdlib = "true"`
//!   per the informational-allowlist Q1 contract.
//!
//! Main-module emission per FR-012: one per `*.app.src`, PURL
//! `pkg:hex/<app>@<vsn>`. Main-module `depends` set is the UNION of
//! atoms from `applications:` + `included_applications:` +
//! `optional_applications:` + the nearest-ancestor `rebar.config`'s
//! `{deps, [...]}` block per Q2+Q3. Each edge-target component carries
//! `mikebom:erlang-app-dep-kind = "required" | "included" | "optional"`
//! annotation (precedence required > included > optional when an atom
//! appears in multiple keyword families); pure-build-time deps from
//! `rebar.config` alone carry no annotation per data-model §3.5.
//!
//! `mikebom:source-type` value-set per the milestone-122/137-140
//! precedent: `erlang-hex`, `erlang-git`, `erlang-otp-runtime`,
//! `erlang-main-module`. Distinguishes Erlang-derived components from
//! milestone-140's `hex-*` Elixir-derived values even when both readers
//! emit `pkg:hex/<name>@<version>` for hex deps.
//!
//! Zero new Cargo dependencies — reuses workspace `regex`, `serde_json`,
//! `tracing`, `anyhow`. The brace-counted tokenizer mirrors
//! `elixir.rs::tokenize_mix_lock` shape per research §R4; factor to a
//! shared `package_db/brace_tokenizer.rs` when a third BEAM-ecosystem
//! reader (e.g., Gleam) materializes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::hash::{ContentHash, HashAlgorithm};
use mikebom_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

const MAX_ERLANG_WALK_DEPTH: usize = 12;

/// Hardcoded set of Ericsson-distributed OTP runtime apps. Per Q1
/// clarification, the allowlist is INFORMATIONAL ONLY — apps in
/// `applications:`/`included_applications:`/`optional_applications:`
/// lists NOT in any parsed `rebar.lock` emit as `pkg:generic/<lib>@unspecified`
/// regardless of allowlist membership. Allowlisted atoms additionally
/// carry `mikebom:otp-stdlib = "true"` so operators can filter "Ericsson
/// stdlib only" via the standard property filter.
const OTP_STDLIB_ALLOWLIST: &[&str] = &[
    "kernel",
    "stdlib",
    "crypto",
    "ssl",
    "inets",
    "mnesia",
    "runtime_tools",
    "sasl",
    "os_mon",
    "tools",
    "compiler",
    "syntax_tools",
    "xmerl",
    "public_key",
    "asn1",
    "ftp",
    "tftp",
    "eldap",
    "observer",
    "wx",
    "debugger",
    "diameter",
    "edoc",
    "et",
    "eunit",
    "ssh",
    "snmp",
    "common_test",
    "dialyzer",
    "erts",
];

fn should_skip_descent(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".svn"
            | ".hg"
            | "_build"
            | "deps"
            | "node_modules"
            | "ebin"
            | "priv"
            | "logs"
    )
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum LockEntry {
    Hex {
        name: String,
        version: String,
        inner_sha256: Option<String>,
        repo: Option<String>,
    },
    Git {
        name: String,
        url: String,
        resolved_ref: String,
        declared_ref_form: String,
    },
}

impl LockEntry {
    fn name(&self) -> &str {
        match self {
            LockEntry::Hex { name, .. } => name,
            LockEntry::Git { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum DeclaredDepSource {
    Hex,
    Git {
        url: String,
        declared_ref: Option<String>,
    },
    Path {
        path: String,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DeclaredDep {
    name: String,
    constraint: Option<String>,
    source_kind: DeclaredDepSource,
    profile: Option<String>, // None = default profile; Some("test"/"dev"/"doc") = scoped
}

#[derive(Debug, Clone, Default)]
struct AppSrcManifest {
    app_name: String,
    version: String,
    required_apps: Vec<String>,
    included_apps: Vec<String>,
    optional_apps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppDepKind {
    Required,
    Included,
    Optional,
    BuildOnly,
}

impl AppDepKind {
    fn annotation_value(&self) -> Option<&'static str> {
        match self {
            AppDepKind::Required => Some("required"),
            AppDepKind::Included => Some("included"),
            AppDepKind::Optional => Some("optional"),
            AppDepKind::BuildOnly => None, // build-only deps don't carry the OTP-keyword annotation
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            AppDepKind::Required => 3,
            AppDepKind::Included => 2,
            AppDepKind::Optional => 1,
            AppDepKind::BuildOnly => 0,
        }
    }
}

pub fn read(
    rootfs: &Path,
    _include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();

    let lockfile_paths = discover_rebar_locks(rootfs, exclude_set);
    let config_paths = discover_rebar_configs(rootfs, exclude_set);
    let app_src_paths = discover_app_src_files(rootfs, exclude_set);

    // FR-006: clean no-op when no Erlang artifacts present.
    if lockfile_paths.is_empty() && config_paths.is_empty() && app_src_paths.is_empty() {
        return out;
    }

    // Parse all lockfiles upfront. Map directory → entries so per-app.src
    // resolution can pick the nearest one.
    let mut lock_data: HashMap<PathBuf, Vec<LockEntry>> = HashMap::new();
    for path in &lockfile_paths {
        match parse_rebar_lock(path) {
            Ok(entries) => {
                if let Some(dir) = path.parent() {
                    lock_data.insert(dir.to_path_buf(), entries);
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "erlang: failed to parse rebar.lock; falling back to sibling rebar.config design-tier if present (FR-007)",
                );
            }
        }
    }

    // Parse all rebar.config files upfront.
    let mut config_data: HashMap<PathBuf, Vec<DeclaredDep>> = HashMap::new();
    for path in &config_paths {
        match parse_rebar_config(path) {
            Ok(deps) => {
                if let Some(dir) = path.parent() {
                    config_data.insert(dir.to_path_buf(), deps);
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "erlang: failed to parse rebar.config; skipping (FR-007)",
                );
            }
        }
    }

    // Parse all *.app.src files upfront.
    let mut app_src_data: Vec<(PathBuf, AppSrcManifest)> = Vec::new();
    for path in &app_src_paths {
        match parse_app_src(path) {
            Ok(manifest) => app_src_data.push((path.clone(), manifest)),
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "erlang: failed to parse *.app.src; skipping (FR-007)",
                );
            }
        }
    }

    // Build a global set of names known to the lockfile(s) so OTP
    // runtime placeholders can filter them out.
    let lockfile_names: HashSet<String> = lock_data
        .values()
        .flat_map(|entries| entries.iter().map(|e| e.name().to_string()))
        .collect();

    // Per main-module, compute the AppDepKind for each atom referenced
    // across the union (Q2 + Q3). Map atom → highest-binding kind across
    // all observed declarations (precedence required > included >
    // optional > build-only).
    let mut atom_kind: HashMap<String, AppDepKind> = HashMap::new();
    for (_, manifest) in &app_src_data {
        for atom in &manifest.required_apps {
            upgrade_kind(&mut atom_kind, atom, AppDepKind::Required);
        }
        for atom in &manifest.included_apps {
            upgrade_kind(&mut atom_kind, atom, AppDepKind::Included);
        }
        for atom in &manifest.optional_apps {
            upgrade_kind(&mut atom_kind, atom, AppDepKind::Optional);
        }
    }
    // BuildOnly tagging — any declared rebar.config dep absent from the
    // three runtime keyword sets gets BuildOnly. This does NOT emit an
    // annotation but DOES preserve the union semantics from Q2.
    for declared in config_data.values().flatten() {
        atom_kind
            .entry(declared.name.clone())
            .or_insert(AppDepKind::BuildOnly);
    }

    // Pass 1: emit lockfile-derived components.
    for entries in lock_data.values() {
        for entry in entries {
            let purl = match build_lock_entry_purl(entry) {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!(
                        name = %entry.name(),
                        error = %err,
                        "erlang: skipping malformed lockfile entry",
                    );
                    continue;
                }
            };
            let purl_key = purl.as_str().to_string();
            if !seen_purls.insert(purl_key) {
                continue;
            }
            out.push(build_lock_entry_component(entry, purl, &atom_kind));
        }
    }

    // Pass 2: emit main-modules per *.app.src + OTP-runtime placeholders.
    for (app_src_path, manifest) in &app_src_data {
        // Find nearest rebar.config (walk up from app_src_path's dir).
        let nearby_config = find_nearest_config(app_src_path, &config_data);

        // Build the union of dep atoms per Q2+Q3.
        let depends_union: Vec<String> = compute_main_module_depends(manifest, nearby_config);

        // Emit main-module component.
        if let Some(main_module) = build_main_module_component(
            app_src_path,
            manifest,
            &depends_union,
            &lock_data,
            !lock_data.is_empty(),
        ) {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        // Emit OTP-runtime placeholders for atoms not in any lockfile.
        let mut union_atoms: Vec<&String> = Vec::new();
        union_atoms.extend(manifest.required_apps.iter());
        union_atoms.extend(manifest.included_apps.iter());
        union_atoms.extend(manifest.optional_apps.iter());
        for atom in union_atoms {
            if lockfile_names.contains(atom) {
                continue;
            }
            // Skip the main-module's own name (shouldn't appear in its
            // own applications: list but Erlang doesn't prevent it).
            if atom == &manifest.app_name {
                continue;
            }
            let placeholder_purl = format!("pkg:generic/{atom}@unspecified");
            if !seen_purls.insert(placeholder_purl.clone()) {
                continue;
            }
            if let Some(component) =
                build_otp_runtime_placeholder(atom, &placeholder_purl, &atom_kind, app_src_path)
            {
                out.push(component);
            }
        }
    }

    // Pass 3: design-tier emission per FR-005 — only when a rebar.config
    // exists WITHOUT a sibling rebar.lock in the same directory.
    //
    // Per F1 finding from /speckit-analyze: profile-scoped deps that
    // don't appear in rebar.lock are NOT separately surfaced when a
    // lockfile is present in the same dir. This matches rebar3's
    // real-world behavior where test/dev deps are NOT written to the
    // default rebar.lock.
    for (config_dir, declared_deps) in &config_data {
        if lock_data.contains_key(config_dir) {
            continue;
        }
        for decl in declared_deps {
            let component = match build_design_tier_component(decl, config_dir) {
                Some(c) => c,
                None => continue,
            };
            let purl_key = component.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(component);
            }
        }
    }

    out
}

// -----------------------------------------------------------------------
// Helpers — atom_kind promotion (T008 precedence rule)
// -----------------------------------------------------------------------

fn upgrade_kind(map: &mut HashMap<String, AppDepKind>, atom: &str, new_kind: AppDepKind) {
    let entry = map.entry(atom.to_string()).or_insert(AppDepKind::BuildOnly);
    if new_kind.precedence() > entry.precedence() {
        *entry = new_kind;
    }
}

// -----------------------------------------------------------------------
// Helpers — discovery (T009)
// -----------------------------------------------------------------------

fn discover_rebar_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_ERLANG_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("rebar.lock") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn discover_rebar_configs(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_ERLANG_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("rebar.config") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn discover_app_src_files(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_ERLANG_WALK_DEPTH,
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
            if name.ends_with(".app.src") {
                out.push(path.to_path_buf());
            }
        }
    });
    out.sort();
    out
}

// -----------------------------------------------------------------------
// Helpers — rebar.lock parsing (T006 + T010 + T015)
// -----------------------------------------------------------------------

/// Brace-counted tokenizer for rebar.lock. Mirrors elixir.rs::tokenize_mix_lock
/// shape per research §R4. Factor to a shared `package_db/brace_tokenizer.rs`
/// when a third BEAM-ecosystem reader (e.g., Gleam) needs the helper.
///
/// rebar.lock shape: `{"<lock-version>", [<pinned-deps>]}.` where
/// `<pinned-deps>` is a comma-separated list of tuples each terminated
/// by `}` at depth 0 of the inner list. Returns one raw entry-string
/// per pinned-dep (the full `{...}` tuple as text).
fn tokenize_rebar_lock(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();

    // Find the outer `{` that opens the top-level 2-element tuple.
    while i < len && bytes[i] != b'{' {
        i += 1;
    }
    if i >= len {
        return out;
    }
    i += 1; // past outer `{`

    // Skip the lock-version string `"1.2.0"` and following comma.
    // The lock-version is enclosed in `"..."`.
    while i < len && bytes[i] != b'"' {
        if bytes[i] == b'[' {
            // Some lockfiles may have an empty list `{[]}` — accept it.
            break;
        }
        i += 1;
    }
    if i < len && bytes[i] == b'"' {
        i += 1; // past opening quote
        while i < len && bytes[i] != b'"' {
            i += 1;
        }
        if i < len {
            i += 1;
        } // past closing quote
    }
    // Skip whitespace + comma after the lock-version.
    while i < len
        && (bytes[i] == b','
            || bytes[i] == b' '
            || bytes[i] == b'\t'
            || bytes[i] == b'\n'
            || bytes[i] == b'\r')
    {
        i += 1;
    }
    // Expect `[` opening the inner pinned-deps list.
    if i >= len || bytes[i] != b'[' {
        return out;
    }
    i += 1; // past `[`

    loop {
        // Skip whitespace + commas between pinned-dep tuples.
        while i < len
            && (bytes[i] == b' '
                || bytes[i] == b'\n'
                || bytes[i] == b'\r'
                || bytes[i] == b'\t'
                || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= len || bytes[i] == b']' {
            break;
        }
        // Each pinned-dep starts with `{`.
        if bytes[i] != b'{' {
            // Defensive: skip unexpected byte.
            i += 1;
            continue;
        }
        let entry_start = i;
        let mut depth_brace = 0i32;
        let mut depth_bracket = 0i32;
        let mut in_str = false;
        let mut escape = false;
        while i < len {
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
                    b'{' => depth_brace += 1,
                    b'[' => depth_bracket += 1,
                    b']' => depth_bracket -= 1,
                    b'}' => {
                        depth_brace -= 1;
                        if depth_brace == 0 && depth_bracket == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        if depth_brace != 0 || depth_bracket != 0 {
            // Unterminated entry — bail.
            break;
        }
        let entry_end = i;
        let entry = String::from_utf8_lossy(&bytes[entry_start..entry_end]).into_owned();
        out.push(entry);
    }
    out
}

fn parse_rebar_lock(path: &Path) -> anyhow::Result<Vec<LockEntry>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let tokens = tokenize_rebar_lock(&text);
    if tokens.is_empty() && !text.trim().is_empty() {
        anyhow::bail!("zero entries from non-empty rebar.lock — malformed term syntax");
    }
    let mut entries: Vec<LockEntry> = Vec::new();
    for entry_text in &tokens {
        if let Some(e) = parse_lock_entry(entry_text) {
            entries.push(e);
        } else {
            tracing::debug!(
                path = %path.display(),
                entry = %entry_text,
                "erlang: skipping unparseable lockfile entry",
            );
        }
    }
    Ok(entries)
}

fn parse_lock_entry(entry_text: &str) -> Option<LockEntry> {
    let trimmed = entry_text.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }

    // Extract the name (first element). Erlang term syntax encodes
    // names as binary-strings `<<"name">>` in rebar.lock — but bare
    // atoms `name` are also accepted as a defensive fallback.
    let name = extract_first_name(trimmed)?;

    // Dispatch on the SECOND element's shape:
    // - `{pkg, ...}` → Hex (modern or map-form)
    // - `{git, ...}` → Git
    // - `<<"version">>` directly (no nested tuple) → Hex legacy
    if let Some(hex) = try_parse_hex_modern(&name, trimmed) {
        return Some(hex);
    }
    if let Some(git) = try_parse_git(&name, trimmed) {
        return Some(git);
    }
    if let Some(hex_legacy) = try_parse_hex_legacy(&name, trimmed) {
        return Some(hex_legacy);
    }
    None
}

/// Extract the package name from the FIRST element of a pinned-dep tuple.
/// Handles both `<<"name">>` (binary-string atom) and `name` (bare atom)
/// shapes per spec Edge Case "rebar.lock binary-string atom encoding".
fn extract_first_name(entry_text: &str) -> Option<String> {
    static BINARY_NAME_RE: OnceLock<Regex> = OnceLock::new();
    static BARE_NAME_RE: OnceLock<Regex> = OnceLock::new();
    let binary_re = BINARY_NAME_RE.get_or_init(|| {
        Regex::new(r#"^\{\s*<<"([^"]+)">>\s*,"#).expect("static binary-name regex")
    });
    let bare_re = BARE_NAME_RE
        .get_or_init(|| Regex::new(r"^\{\s*([a-z][a-zA-Z0-9_]*)\s*,").expect("static bare-name regex"));

    if let Some(caps) = binary_re.captures(entry_text) {
        return caps.get(1).map(|m| m.as_str().to_lowercase());
    }
    if let Some(caps) = bare_re.captures(entry_text) {
        return caps.get(1).map(|m| m.as_str().to_lowercase());
    }
    None
}

/// Try parsing as modern-Hex shape:
/// `{<<"name">>, {pkg, <<"name">>, <<"version">>[, <<"sha">>][, #{repo => <<"hexpm:org">>}]}, depth}`
fn try_parse_hex_modern(name: &str, entry_text: &str) -> Option<LockEntry> {
    static HEX_REGEX: OnceLock<Regex> = OnceLock::new();
    static REPO_REGEX: OnceLock<Regex> = OnceLock::new();
    let hex_re = HEX_REGEX.get_or_init(|| {
        // Capture: 1 = inner-pkg-name (ignored), 2 = version,
        // 3 = optional sha256.
        Regex::new(
            r#"\{\s*pkg\s*,\s*<<"([^"]+)">>\s*,\s*<<"([^"]+)">>(?:\s*,\s*<<"([^"]*)">>)?"#,
        )
        .expect("static hex regex")
    });
    let repo_re = REPO_REGEX.get_or_init(|| {
        Regex::new(r#"#\{\s*repo\s*=>\s*<<"([^"]+)">>\s*\}"#).expect("static repo regex")
    });

    let caps = hex_re.captures(entry_text)?;
    let version = caps.get(2)?.as_str().to_string();
    let inner_sha256 = caps
        .get(3)
        .map(|m| m.as_str().to_string())
        .filter(|s| !s.is_empty());
    let repo = repo_re
        .captures(entry_text)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));

    Some(LockEntry::Hex {
        name: name.to_string(),
        version,
        inner_sha256,
        repo,
    })
}

/// Try parsing as Git shape:
/// `{<<"name">>, {git, "url", {ref|tag|branch, "value"}}, depth}`
fn try_parse_git(name: &str, entry_text: &str) -> Option<LockEntry> {
    static GIT_REGEX: OnceLock<Regex> = OnceLock::new();
    let git_re = GIT_REGEX.get_or_init(|| {
        // Capture: 1 = url, 2 = ref-form (ref/tag/branch), 3 = value.
        Regex::new(r#"\{\s*git\s*,\s*"([^"]+)"\s*,\s*\{\s*(ref|tag|branch)\s*,\s*"([^"]+)"\s*\}"#)
            .expect("static git regex")
    });
    let caps = git_re.captures(entry_text)?;
    let url = caps.get(1)?.as_str().to_string();
    let declared_ref_form = caps.get(2)?.as_str().to_string();
    let resolved_ref = caps.get(3)?.as_str().to_string();
    Some(LockEntry::Git {
        name: name.to_string(),
        url,
        resolved_ref,
        declared_ref_form,
    })
}

/// Try parsing as legacy-Hex shape: `{<<"name">>, <<"version">>, depth}`
/// (pre-rebar3-3.7). Distinguished from modern-Hex by ABSENCE of a
/// `{pkg, ...}` inner tuple.
fn try_parse_hex_legacy(name: &str, entry_text: &str) -> Option<LockEntry> {
    // First reject if the entry contains a `{pkg, ` or `{git, ` marker —
    // those are handled by the other parsers and any "version" match
    // would be a false positive.
    if entry_text.contains("{pkg,") || entry_text.contains("{git,") {
        return None;
    }
    static LEGACY_REGEX: OnceLock<Regex> = OnceLock::new();
    let legacy_re = LEGACY_REGEX.get_or_init(|| {
        // After the name's `<<"name">>,` capture, the next element is
        // `<<"version">>` directly (no nested tuple).
        Regex::new(r#"^\{\s*<<"[^"]+">>\s*,\s*<<"([^"]+)">>\s*,\s*\d+\s*\}$"#)
            .expect("static legacy regex")
    });
    let caps = legacy_re.captures(entry_text.trim())?;
    let version = caps.get(1)?.as_str().to_string();
    Some(LockEntry::Hex {
        name: name.to_string(),
        version,
        inner_sha256: None,
        repo: None,
    })
}

// -----------------------------------------------------------------------
// Helpers — rebar.config parsing (T021)
// -----------------------------------------------------------------------

fn parse_rebar_config(path: &Path) -> anyhow::Result<Vec<DeclaredDep>> {
    let text =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let stripped = strip_erlang_comments(&text);
    let mut out: Vec<DeclaredDep> = Vec::new();

    // Top-level {deps, [...]} block.
    if let Some(deps_body) = extract_keyword_list_body(&stripped, "deps") {
        out.extend(parse_dep_tuples(&deps_body, None));
    }

    // {profiles, [{<env>, [{deps, [...]}]}]} — extract each profile's
    // env name + its inner {deps, [...]} body.
    if let Some(profiles_body) = extract_keyword_list_body(&stripped, "profiles") {
        for (env, env_body) in iter_profile_blocks(&profiles_body) {
            if let Some(inner_deps_body) = extract_keyword_list_body(&env_body, "deps") {
                out.extend(parse_dep_tuples(&inner_deps_body, Some(env)));
            }
        }
    }

    Ok(out)
}

/// Strip Erlang line comments (`% ...` to end-of-line). Preserves
/// strings — `%` inside `"..."` is left alone.
fn strip_erlang_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_str = false;
    let mut escape = false;
    let mut skipping_comment = false;
    for c in text.chars() {
        if skipping_comment {
            if c == '\n' {
                skipping_comment = false;
                out.push(c);
            }
            continue;
        }
        if escape {
            out.push(c);
            escape = false;
            continue;
        }
        if in_str {
            match c {
                '\\' => {
                    out.push(c);
                    escape = true;
                }
                '"' => {
                    in_str = false;
                    out.push(c);
                }
                _ => out.push(c),
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '%' => {
                skipping_comment = true;
            }
            _ => out.push(c),
        }
    }
    out
}

/// Find a `{<keyword>, [...]}` top-level block and return the body inside
/// the matching `[ ... ]`. Uses brace + bracket counting so nested
/// structures don't terminate the body prematurely.
fn extract_keyword_list_body(text: &str, keyword: &str) -> Option<String> {
    // Search for `{<keyword>` (whitespace-tolerant).
    let mut start = 0usize;
    while let Some(idx) = text[start..].find(&format!("{{{keyword}")) {
        let abs = start + idx;
        // Verify the byte right after the keyword is whitespace or
        // comma (to avoid matching `{deps_alt, ...}`).
        let after_kw = abs + 1 + keyword.len();
        if after_kw < text.len() {
            let next = text.as_bytes()[after_kw];
            if next != b' '
                && next != b'\t'
                && next != b'\n'
                && next != b'\r'
                && next != b','
            {
                start = abs + 1;
                continue;
            }
        }
        // Find the opening `[` for the value list. Skip whitespace +
        // comma after the keyword.
        let mut i = after_kw;
        while i < text.len() {
            let c = text.as_bytes()[i];
            if c == b','
                || c == b' '
                || c == b'\t'
                || c == b'\n'
                || c == b'\r'
            {
                i += 1;
            } else {
                break;
            }
        }
        if i >= text.len() || text.as_bytes()[i] != b'[' {
            start = abs + 1;
            continue;
        }
        let body_start = i + 1;
        // Bracket-count to find the matching `]`.
        let mut bracket = 1i32;
        let mut brace = 0i32;
        let mut in_str = false;
        let mut escape = false;
        let mut j = body_start;
        while j < text.len() {
            let c = text.as_bytes()[j];
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
                    b'{' => brace += 1,
                    b'}' => brace -= 1,
                    b'[' => bracket += 1,
                    b']' => {
                        bracket -= 1;
                        if bracket == 0 && brace == 0 {
                            return Some(text[body_start..j].to_string());
                        }
                    }
                    _ => {}
                }
            }
            j += 1;
        }
        start = abs + 1;
    }
    None
}

/// Iterate `{<env>, [<inner>]}` blocks inside a profiles-list body.
/// Returns `(env_name, env_body_with_outer_braces)` per block so the
/// caller can extract `{deps, [...]}` from inside it.
fn iter_profile_blocks(profiles_body: &str) -> Vec<(String, String)> {
    // Hoisted outside the loop per `clippy::regex_creation_in_loops`.
    static ENV_RE: OnceLock<Regex> = OnceLock::new();
    let env_re = ENV_RE.get_or_init(|| {
        Regex::new(r"^\{\s*([a-z][a-zA-Z0-9_]*)\s*,").expect("static env regex")
    });
    let mut out: Vec<(String, String)> = Vec::new();
    let bytes = profiles_body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip whitespace + commas.
        while i < bytes.len()
            && (bytes[i] == b' '
                || bytes[i] == b'\t'
                || bytes[i] == b'\n'
                || bytes[i] == b'\r'
                || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        let block_start = i;
        let mut depth = 0i32;
        let mut in_str = false;
        let mut escape = false;
        while i < bytes.len() {
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
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        if depth != 0 {
            break;
        }
        let block_text = &profiles_body[block_start..i];
        // Extract env name: first atom after `{`.
        if let Some(caps) = env_re.captures(block_text) {
            if let Some(env_match) = caps.get(1) {
                out.push((env_match.as_str().to_string(), block_text.to_string()));
            }
        }
    }
    out
}

/// Parse `{<atom>, ...}` dep tuples from a deps-list body. Handles:
/// - `{name, "version-constraint"}` → Hex
/// - `{name, {pkg, name, "version-constraint"}}` → Hex (explicit-pkg form)
/// - `{name, {git, "url", {ref|tag|branch, "value"}}}` → Git
/// - `{name, {path, "path"}}` → Path
fn parse_dep_tuples(body: &str, profile: Option<String>) -> Vec<DeclaredDep> {
    let mut out: Vec<DeclaredDep> = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip whitespace + commas.
        while i < bytes.len()
            && (bytes[i] == b' '
                || bytes[i] == b'\t'
                || bytes[i] == b'\n'
                || bytes[i] == b'\r'
                || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Bare-atom shorthand: `{name}` (uncommon but valid) or `name`
        // (no constraint, no source spec). We handle the brace form here.
        if bytes[i] != b'{' {
            // Bare-atom shorthand: just consume the atom and emit a
            // bare-Hex DeclaredDep.
            if bytes[i].is_ascii_lowercase() {
                let atom_start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                {
                    i += 1;
                }
                let name = String::from_utf8_lossy(&bytes[atom_start..i]).into_owned();
                out.push(DeclaredDep {
                    name,
                    constraint: None,
                    source_kind: DeclaredDepSource::Hex,
                    profile: profile.clone(),
                });
                continue;
            }
            i += 1;
            continue;
        }
        // Brace-counted tuple extraction.
        let tuple_start = i;
        let mut depth = 0i32;
        let mut in_str = false;
        let mut escape = false;
        while i < bytes.len() {
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
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        if depth != 0 {
            break;
        }
        let tuple_text = &body[tuple_start..i];
        if let Some(dep) = parse_dep_tuple_text(tuple_text, profile.clone()) {
            out.push(dep);
        }
    }
    out
}

fn parse_dep_tuple_text(tuple_text: &str, profile: Option<String>) -> Option<DeclaredDep> {
    static NAME_RE: OnceLock<Regex> = OnceLock::new();
    let name_re = NAME_RE
        .get_or_init(|| Regex::new(r"^\{\s*([a-z][a-zA-Z0-9_]*)\s*,?").expect("static name regex"));
    let caps = name_re.captures(tuple_text)?;
    let name = caps.get(1)?.as_str().to_string();

    // Git source detection.
    if let Some(rest) = tuple_text.find("{git,") {
        static GIT_RE: OnceLock<Regex> = OnceLock::new();
        let git_re = GIT_RE.get_or_init(|| {
            Regex::new(
                r#"\{\s*git\s*,\s*"([^"]+)"(?:\s*,\s*\{\s*(ref|tag|branch)\s*,\s*"([^"]+)"\s*\})?"#,
            )
            .expect("static git decl regex")
        });
        if let Some(git_caps) = git_re.captures(&tuple_text[rest..]) {
            let url = git_caps.get(1)?.as_str().to_string();
            let declared_ref = match (git_caps.get(2), git_caps.get(3)) {
                (Some(form), Some(value)) => {
                    Some(format!("{}: {}", form.as_str(), value.as_str()))
                }
                _ => None,
            };
            return Some(DeclaredDep {
                name,
                constraint: None,
                source_kind: DeclaredDepSource::Git { url, declared_ref },
                profile,
            });
        }
    }

    // Path source detection.
    if let Some(_rest) = tuple_text.find("{path,") {
        static PATH_RE: OnceLock<Regex> = OnceLock::new();
        let path_re = PATH_RE.get_or_init(|| {
            Regex::new(r#"\{\s*path\s*,\s*"([^"]+)"\s*\}"#).expect("static path decl regex")
        });
        if let Some(path_caps) = path_re.captures(tuple_text) {
            let path = path_caps.get(1)?.as_str().to_string();
            return Some(DeclaredDep {
                name,
                constraint: None,
                source_kind: DeclaredDepSource::Path { path },
                profile,
            });
        }
    }

    // Hex source — constraint from first quoted string OR from
    // `{pkg, name, "constraint"}` inner tuple.
    static CONSTRAINT_RE: OnceLock<Regex> = OnceLock::new();
    let constraint_re = CONSTRAINT_RE.get_or_init(|| {
        Regex::new(r#""([^"]+)""#).expect("static constraint regex")
    });
    let constraint = constraint_re
        .captures(tuple_text)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));

    Some(DeclaredDep {
        name,
        constraint,
        source_kind: DeclaredDepSource::Hex,
        profile,
    })
}

// -----------------------------------------------------------------------
// Helpers — *.app.src parsing (T017 + T024)
// -----------------------------------------------------------------------

fn parse_app_src(path: &Path) -> anyhow::Result<AppSrcManifest> {
    let text =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let stripped = strip_erlang_comments(&text);

    // App-name from `{application, <atom>, [...]}` outer tuple. Fallback
    // to parent-directory basename per FR-012 cascade.
    static APP_NAME_RE: OnceLock<Regex> = OnceLock::new();
    let app_name_re = APP_NAME_RE.get_or_init(|| {
        Regex::new(r"\{\s*application\s*,\s*([a-z][a-zA-Z0-9_]*)\s*,\s*\[")
            .expect("static app-name regex")
    });
    let app_name = app_name_re
        .captures(&stripped)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| {
            // Fallback per FR-012 — derive from parent-dir basename
            // (an *.app.src lives at .../<app>/src/<app>.app.src, so
            // grandparent is the app dir).
            path.parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(String::from)
                .unwrap_or_else(|| "unknown".to_string())
        });

    // Version from `{vsn, "..."}` keyword. Fallback to "0.0.0-unknown"
    // per FR-012.
    static VSN_RE: OnceLock<Regex> = OnceLock::new();
    let vsn_re = VSN_RE
        .get_or_init(|| Regex::new(r#"\{\s*vsn\s*,\s*"([^"]+)"\s*\}"#).expect("static vsn regex"));
    let version = vsn_re
        .captures(&stripped)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| "0.0.0-unknown".to_string());

    let required_apps = extract_atom_list_for_keyword(&stripped, "applications");
    let included_apps = extract_atom_list_for_keyword(&stripped, "included_applications");
    let optional_apps = extract_atom_list_for_keyword(&stripped, "optional_applications");

    Ok(AppSrcManifest {
        app_name,
        version,
        required_apps,
        included_apps,
        optional_apps,
    })
}

/// Extract atoms from `{<keyword>, [<atoms>]}` keyword tuple inside an
/// *.app.src body. Atoms are bare-Erlang-atoms (no quotes); returns
/// lowercased names.
fn extract_atom_list_for_keyword(text: &str, keyword: &str) -> Vec<String> {
    let Some(list_body) = extract_keyword_list_body(text, keyword) else {
        return Vec::new();
    };
    static ATOM_RE: OnceLock<Regex> = OnceLock::new();
    let atom_re = ATOM_RE
        .get_or_init(|| Regex::new(r"\b([a-z][a-zA-Z0-9_]*)\b").expect("static atom regex"));
    atom_re
        .captures_iter(&list_body)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

// -----------------------------------------------------------------------
// Helpers — main-module emission (T018 + T024 + T025)
// -----------------------------------------------------------------------

fn compute_main_module_depends(
    manifest: &AppSrcManifest,
    nearby_config: Option<&Vec<DeclaredDep>>,
) -> Vec<String> {
    let mut union: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // Required (highest precedence).
    for atom in &manifest.required_apps {
        if atom != &manifest.app_name && seen.insert(atom.clone()) {
            union.push(atom.clone());
        }
    }
    for atom in &manifest.included_apps {
        if atom != &manifest.app_name && seen.insert(atom.clone()) {
            union.push(atom.clone());
        }
    }
    for atom in &manifest.optional_apps {
        if atom != &manifest.app_name && seen.insert(atom.clone()) {
            union.push(atom.clone());
        }
    }
    if let Some(deps) = nearby_config {
        for d in deps {
            if d.name != manifest.app_name && seen.insert(d.name.clone()) {
                union.push(d.name.clone());
            }
        }
    }
    union
}

fn find_nearest_config<'a>(
    app_src_path: &Path,
    config_data: &'a HashMap<PathBuf, Vec<DeclaredDep>>,
) -> Option<&'a Vec<DeclaredDep>> {
    let mut dir = app_src_path.parent();
    while let Some(d) = dir {
        if let Some(deps) = config_data.get(d) {
            return Some(deps);
        }
        dir = d.parent();
    }
    None
}

fn build_main_module_component(
    app_src_path: &Path,
    manifest: &AppSrcManifest,
    depends: &[String],
    _lock_data: &HashMap<PathBuf, Vec<LockEntry>>,
    doc_has_lockfile: bool,
) -> Option<PackageDbEntry> {
    if manifest.app_name.is_empty() {
        return None;
    }
    let lc_name = manifest.app_name.to_lowercase();
    // Milestone 197 US3 (#567): emit versionless canonical PURL per
    // purl-spec when the .app.src has no `{vsn, "..."}` — matches m191
    // fix pattern. The parse_app_src path at line 1210 sets
    // manifest.version to `"0.0.0-unknown"` as a display placeholder;
    // detect that here to decide between versionless / versioned PURL.
    let purl_str = if manifest.version == "0.0.0-unknown" || manifest.version.is_empty() {
        format!("pkg:hex/{lc_name}")
    } else {
        format!("pkg:hex/{lc_name}@{version}", version = manifest.version)
    };
    let purl = Purl::new(&purl_str).ok()?;

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("erlang-main-module".to_string()),
    );

    let sbom_tier = if doc_has_lockfile { "source" } else { "design" };

    Some(PackageDbEntry {
        purl,
        name: manifest.app_name.clone(),
        version: manifest.version.clone(),
        arch: None,
        source_path: app_src_path.to_string_lossy().into_owned(),
        depends: depends.to_vec(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
        source_type: Some("erlang-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some("app-src".to_string()),
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
// Helpers — lockfile component emission (T011 + T016 + T024 annotation)
// -----------------------------------------------------------------------

fn build_lock_entry_purl(entry: &LockEntry) -> Result<Purl, String> {
    let purl_str = match entry {
        LockEntry::Hex {
            name,
            version,
            repo,
            ..
        } => {
            let lc_name = name.to_lowercase();
            if let Some(r) = repo {
                // `r` is either "hexpm:<org>" or "<org>" (bare); both
                // are private-org per rebar3 documented flexibility.
                let org = r.strip_prefix("hexpm:").unwrap_or(r);
                if org == "hexpm" {
                    // Default Hex.pm — emit without namespace.
                    format!("pkg:hex/{lc_name}@{version}")
                } else {
                    let lc_org = org.to_lowercase();
                    format!(
                        "pkg:hex/{lc_org}/{lc_name}@{version}?repository_url=https://repo.hex.pm"
                    )
                }
            } else {
                format!("pkg:hex/{lc_name}@{version}")
            }
        }
        LockEntry::Git {
            name,
            url,
            resolved_ref,
            ..
        } => {
            format!(
                "pkg:generic/{name}@{resolved_ref}?vcs_url=git+{}",
                minimal_qualifier_encode(url)
            )
        }
    };
    Purl::new(&purl_str).map_err(|e| format!("PURL construction failed for {purl_str}: {e:?}"))
}

fn build_lock_entry_component(
    entry: &LockEntry,
    purl: Purl,
    atom_kind: &HashMap<String, AppDepKind>,
) -> PackageDbEntry {
    let (source_type, hashes, declared_ref) = match entry {
        LockEntry::Hex { inner_sha256, .. } => {
            let mut h = Vec::new();
            if let Some(sha) = inner_sha256 {
                if let Ok(ch) = ContentHash::with_algorithm(HashAlgorithm::Sha256, sha) {
                    h.push(ch);
                }
            }
            ("erlang-hex", h, None)
        }
        LockEntry::Git {
            declared_ref_form, ..
        } => ("erlang-git", Vec::new(), Some(declared_ref_form.clone())),
    };

    let (name, version) = match entry {
        LockEntry::Hex { name, version, .. } => (name.clone(), version.clone()),
        LockEntry::Git {
            name, resolved_ref, ..
        } => (name.clone(), resolved_ref.clone()),
    };

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String(source_type.to_string()),
    );
    if let Some(r) = declared_ref {
        extra_annotations.insert(
            "mikebom:vcs-declared-ref".to_string(),
            serde_json::Value::String(r),
        );
    }
    // Q3: apply mikebom:erlang-app-dep-kind annotation per the highest-
    // binding kind across all *.app.src declarations.
    if let Some(kind) = atom_kind.get(&name) {
        if let Some(kind_str) = kind.annotation_value() {
            extra_annotations.insert(
                "mikebom:erlang-app-dep-kind".to_string(),
                serde_json::Value::String(kind_str.to_string()),
            );
        }
    }

    PackageDbEntry {
        purl,
        name,
        version,
        arch: None,
        source_path: String::new(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(LifecycleScope::Runtime),
        requirement_range: None,
        source_type: Some(source_type.to_string()),
        buildinfo_status: None,
        sbom_tier: Some("source".to_string()),
        evidence_kind: Some("rebar-lock".to_string()),
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

// -----------------------------------------------------------------------
// Helpers — OTP runtime placeholder emission (T019 + T024)
// -----------------------------------------------------------------------

fn build_otp_runtime_placeholder(
    atom: &str,
    purl_str: &str,
    atom_kind: &HashMap<String, AppDepKind>,
    app_src_path: &Path,
) -> Option<PackageDbEntry> {
    let purl = Purl::new(purl_str).ok()?;

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("erlang-otp-runtime".to_string()),
    );
    if OTP_STDLIB_ALLOWLIST.contains(&atom) {
        extra_annotations.insert(
            "mikebom:otp-stdlib".to_string(),
            serde_json::Value::String("true".to_string()),
        );
    }
    if let Some(kind) = atom_kind.get(atom) {
        if let Some(kind_str) = kind.annotation_value() {
            extra_annotations.insert(
                "mikebom:erlang-app-dep-kind".to_string(),
                serde_json::Value::String(kind_str.to_string()),
            );
        }
    }

    Some(PackageDbEntry {
        purl,
        name: atom.to_string(),
        version: "unspecified".to_string(),
        arch: None,
        source_path: app_src_path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(LifecycleScope::Runtime),
        requirement_range: None,
        source_type: Some("erlang-otp-runtime".to_string()),
        buildinfo_status: None,
        sbom_tier: Some("source".to_string()),
        evidence_kind: Some("app-src".to_string()),
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
// Helpers — design-tier component emission (T022)
// -----------------------------------------------------------------------

fn build_design_tier_component(decl: &DeclaredDep, config_dir: &Path) -> Option<PackageDbEntry> {
    let constraint = decl
        .constraint
        .clone()
        .unwrap_or_else(|| "unspecified".to_string());
    let sanitized = sanitize_purl_version(&constraint);

    let (purl_str, source_type, source_anns) = match &decl.source_kind {
        DeclaredDepSource::Hex => {
            let lc_name = decl.name.to_lowercase();
            (
                format!("pkg:hex/{lc_name}@{sanitized}"),
                "erlang-hex",
                BTreeMap::<String, serde_json::Value>::new(),
            )
        }
        DeclaredDepSource::Git { url, declared_ref } => {
            let purl_s = format!(
                "pkg:generic/{}@unspecified?vcs_url=git+{}",
                decl.name,
                minimal_qualifier_encode(url),
            );
            let mut anns: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            if let Some(r) = declared_ref {
                anns.insert(
                    "mikebom:vcs-declared-ref".to_string(),
                    serde_json::Value::String(r.clone()),
                );
            }
            (purl_s, "erlang-git", anns)
        }
        DeclaredDepSource::Path { path } => {
            let purl_s = format!("pkg:generic/{}@unspecified", decl.name);
            let mut anns: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            if !path.is_empty() {
                anns.insert(
                    "mikebom:path".to_string(),
                    serde_json::Value::String(path.clone()),
                );
            }
            (purl_s, "erlang-path", anns)
        }
    };

    let purl = match Purl::new(&purl_str) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                name = %decl.name,
                purl = %purl_str,
                error = ?err,
                "erlang: skipping design-tier entry with non-PURL-safe form",
            );
            return None;
        }
    };

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String(source_type.to_string()),
    );
    for (k, v) in source_anns {
        extra_annotations.insert(k, v);
    }

    // Profile-scoped → mikebom:lifecycle-scope per FR-008.
    let lifecycle_scope = match decl.profile.as_deref() {
        Some("dev") | Some("test") | Some("doc") => LifecycleScope::Development,
        _ => LifecycleScope::Runtime,
    };

    let requirement_range = decl.constraint.clone();
    let version_field = match &decl.source_kind {
        DeclaredDepSource::Hex => sanitized,
        _ => "unspecified".to_string(),
    };

    Some(PackageDbEntry {
        purl,
        name: decl.name.clone(),
        version: version_field,
        arch: None,
        source_path: config_dir.join("rebar.config").to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(lifecycle_scope),
        requirement_range,
        source_type: Some(source_type.to_string()),
        buildinfo_status: None,
        sbom_tier: Some("design".to_string()),
        evidence_kind: Some("rebar-config".to_string()),
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
// Helpers — PURL string utilities (matches elixir.rs conventions)
// -----------------------------------------------------------------------

fn sanitize_purl_version(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '?' | '#' | ' ' => '_',
            other => other,
        })
        .collect()
}

fn minimal_qualifier_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            '&' => out.push_str("%26"),
            other => out.push(other),
        }
    }
    out
}

// -----------------------------------------------------------------------
// Unit tests
// -----------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn tokenize_modern_hex_entry() {
        let body = r#"{"1.2.0",
[
  {<<"cowboy">>,{pkg,<<"cowboy">>,<<"2.10.0">>},0},
  {<<"jiffy">>,{pkg,<<"jiffy">>,<<"1.1.1">>},1}
]}."#;
        let tokens = tokenize_rebar_lock(body);
        assert_eq!(tokens.len(), 2);
        assert!(tokens[0].contains("cowboy"));
        assert!(tokens[1].contains("jiffy"));
    }

    #[test]
    fn parse_modern_hex_with_sha() {
        let entry =
            r#"{<<"cowboy">>,{pkg,<<"cowboy">>,<<"2.10.0">>,<<"3b53b9647e3fa42d3ddd91c1e3e7b8e8">>},0}"#;
        let parsed = parse_lock_entry(entry).expect("parse should succeed");
        match parsed {
            LockEntry::Hex {
                name,
                version,
                inner_sha256,
                repo,
            } => {
                assert_eq!(name, "cowboy");
                assert_eq!(version, "2.10.0");
                assert_eq!(
                    inner_sha256,
                    Some("3b53b9647e3fa42d3ddd91c1e3e7b8e8".to_string())
                );
                assert_eq!(repo, None);
            }
            _ => panic!("expected Hex variant"),
        }
    }

    #[test]
    fn parse_modern_hex_no_sha() {
        let entry = r#"{<<"jiffy">>,{pkg,<<"jiffy">>,<<"1.1.1">>},1}"#;
        let parsed = parse_lock_entry(entry).expect("parse should succeed");
        match parsed {
            LockEntry::Hex {
                name,
                version,
                inner_sha256,
                ..
            } => {
                assert_eq!(name, "jiffy");
                assert_eq!(version, "1.1.1");
                assert_eq!(inner_sha256, None);
            }
            _ => panic!("expected Hex variant"),
        }
    }

    #[test]
    fn parse_legacy_hex_shape() {
        let entry = r#"{<<"lager">>,<<"3.9.2">>,1}"#;
        let parsed = parse_lock_entry(entry).expect("parse should succeed");
        match parsed {
            LockEntry::Hex {
                name,
                version,
                inner_sha256,
                repo,
            } => {
                assert_eq!(name, "lager");
                assert_eq!(version, "3.9.2");
                assert_eq!(inner_sha256, None);
                assert_eq!(repo, None);
            }
            _ => panic!("expected Hex variant"),
        }
    }

    #[test]
    fn parse_private_org_map_form() {
        let entry = r#"{<<"internal_lib">>,{pkg,<<"internal_lib">>,<<"2.0.0">>,<<"abc123">>,#{repo => <<"hexpm:acme">>}},0}"#;
        let parsed = parse_lock_entry(entry).expect("parse should succeed");
        match parsed {
            LockEntry::Hex {
                name,
                version,
                inner_sha256,
                repo,
            } => {
                assert_eq!(name, "internal_lib");
                assert_eq!(version, "2.0.0");
                assert_eq!(inner_sha256, Some("abc123".to_string()));
                assert_eq!(repo, Some("hexpm:acme".to_string()));
            }
            _ => panic!("expected Hex variant"),
        }
    }

    #[test]
    fn parse_git_ref_form() {
        let entry = r#"{<<"my_fork">>,{git,"https://github.com/foo/my-fork.git",{ref,"eb39649a76b87e8451baf75d10ce82ca3a3d5601"}},0}"#;
        let parsed = parse_lock_entry(entry).expect("parse should succeed");
        match parsed {
            LockEntry::Git {
                name,
                url,
                resolved_ref,
                declared_ref_form,
            } => {
                assert_eq!(name, "my_fork");
                assert_eq!(url, "https://github.com/foo/my-fork.git");
                assert_eq!(resolved_ref, "eb39649a76b87e8451baf75d10ce82ca3a3d5601");
                assert_eq!(declared_ref_form, "ref");
            }
            _ => panic!("expected Git variant"),
        }
    }

    #[test]
    fn parse_git_tag_form() {
        let entry =
            r#"{<<"my_fork">>,{git,"https://github.com/foo/my-fork.git",{tag,"v1.2.3"}},0}"#;
        let parsed = parse_lock_entry(entry).expect("parse should succeed");
        match parsed {
            LockEntry::Git {
                resolved_ref,
                declared_ref_form,
                ..
            } => {
                assert_eq!(resolved_ref, "v1.2.3");
                assert_eq!(declared_ref_form, "tag");
            }
            _ => panic!("expected Git variant"),
        }
    }

    #[test]
    fn build_hex_purl_default_org() {
        let entry = LockEntry::Hex {
            name: "cowboy".to_string(),
            version: "2.10.0".to_string(),
            inner_sha256: None,
            repo: None,
        };
        let purl = build_lock_entry_purl(&entry).unwrap();
        assert_eq!(purl.as_str(), "pkg:hex/cowboy@2.10.0");
    }

    #[test]
    fn build_hex_purl_private_org() {
        let entry = LockEntry::Hex {
            name: "internal_lib".to_string(),
            version: "2.0.0".to_string(),
            inner_sha256: None,
            repo: Some("hexpm:acme".to_string()),
        };
        let purl = build_lock_entry_purl(&entry).unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:hex/acme/internal_lib@2.0.0?repository_url=https://repo.hex.pm"
        );
    }

    #[test]
    fn build_git_purl_with_sha_ref() {
        let entry = LockEntry::Git {
            name: "my_fork".to_string(),
            url: "https://github.com/foo/my-fork.git".to_string(),
            resolved_ref: "eb39649a76b87e8451baf75d10ce82ca3a3d5601".to_string(),
            declared_ref_form: "ref".to_string(),
        };
        let purl = build_lock_entry_purl(&entry).unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:generic/my_fork@eb39649a76b87e8451baf75d10ce82ca3a3d5601?vcs_url=git+https://github.com/foo/my-fork.git"
        );
    }

    #[test]
    fn parse_app_src_basic() {
        let dir = tempfile::tempdir().unwrap();
        let app_dir = dir.path().join("my_app");
        let src_dir = app_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let app_src = src_dir.join("my_app.app.src");
        std::fs::write(
            &app_src,
            r#"{application, my_app, [
    {vsn, "1.2.3"},
    {applications, [kernel, stdlib, cowboy]},
    {description, "Test app"}
]}."#,
        )
        .unwrap();
        let parsed = parse_app_src(&app_src).unwrap();
        assert_eq!(parsed.app_name, "my_app");
        assert_eq!(parsed.version, "1.2.3");
        assert_eq!(parsed.required_apps, vec!["kernel", "stdlib", "cowboy"]);
        assert!(parsed.included_apps.is_empty());
        assert!(parsed.optional_apps.is_empty());
    }

    #[test]
    fn parse_app_src_q3_all_keyword_families() {
        let dir = tempfile::tempdir().unwrap();
        let app_dir = dir.path().join("my_app");
        let src_dir = app_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let app_src = src_dir.join("my_app.app.src");
        std::fs::write(
            &app_src,
            r#"{application, my_app, [
    {vsn, "1.0.0"},
    {applications, [kernel, stdlib, cowboy]},
    {included_applications, [config_app]},
    {optional_applications, [telemetry]},
    {description, "Q3 keyword family fixture"}
]}."#,
        )
        .unwrap();
        let parsed = parse_app_src(&app_src).unwrap();
        assert_eq!(parsed.required_apps, vec!["kernel", "stdlib", "cowboy"]);
        assert_eq!(parsed.included_apps, vec!["config_app"]);
        assert_eq!(parsed.optional_apps, vec!["telemetry"]);
    }

    #[test]
    fn parse_app_src_missing_vsn_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let app_dir = dir.path().join("my_app");
        let src_dir = app_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let app_src = src_dir.join("my_app.app.src");
        std::fs::write(
            &app_src,
            r#"{application, my_app, [
    {applications, [kernel]},
    {description, "No vsn"}
]}."#,
        )
        .unwrap();
        let parsed = parse_app_src(&app_src).unwrap();
        assert_eq!(parsed.app_name, "my_app");
        assert_eq!(parsed.version, "0.0.0-unknown");
    }

    #[test]
    fn parse_rebar_config_with_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("rebar.config");
        std::fs::write(
            &config,
            r#"{deps, [
    {cowboy, "~> 2.10"},
    {jiffy, {pkg, jiffy, "~> 1.1"}}
]}.

{profiles, [
    {test, [{deps, [{meck, "~> 0.9"}]}]}
]}.
"#,
        )
        .unwrap();
        let deps = parse_rebar_config(&config).unwrap();
        let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"cowboy"));
        assert!(names.contains(&"jiffy"));
        assert!(names.contains(&"meck"));
        let meck = deps.iter().find(|d| d.name == "meck").unwrap();
        assert_eq!(meck.profile.as_deref(), Some("test"));
        let cowboy = deps.iter().find(|d| d.name == "cowboy").unwrap();
        assert_eq!(cowboy.profile, None);
        assert_eq!(cowboy.constraint.as_deref(), Some("~> 2.10"));
    }

    #[test]
    fn parse_rebar_config_git_dep() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("rebar.config");
        std::fs::write(
            &config,
            r#"{deps, [
    {my_fork, {git, "https://github.com/foo/my-fork.git", {tag, "v1.0"}}}
]}."#,
        )
        .unwrap();
        let deps = parse_rebar_config(&config).unwrap();
        assert_eq!(deps.len(), 1);
        match &deps[0].source_kind {
            DeclaredDepSource::Git { url, declared_ref } => {
                assert_eq!(url, "https://github.com/foo/my-fork.git");
                assert_eq!(declared_ref.as_deref(), Some("tag: v1.0"));
            }
            _ => panic!("expected Git source"),
        }
    }

    #[test]
    fn appdepkind_precedence() {
        assert!(AppDepKind::Required.precedence() > AppDepKind::Included.precedence());
        assert!(AppDepKind::Included.precedence() > AppDepKind::Optional.precedence());
        assert!(AppDepKind::Optional.precedence() > AppDepKind::BuildOnly.precedence());
    }

    #[test]
    fn upgrade_kind_keeps_highest() {
        let mut m: HashMap<String, AppDepKind> = HashMap::new();
        upgrade_kind(&mut m, "cowboy", AppDepKind::Optional);
        upgrade_kind(&mut m, "cowboy", AppDepKind::Required);
        assert_eq!(m.get("cowboy"), Some(&AppDepKind::Required));
        upgrade_kind(&mut m, "cowboy", AppDepKind::Included); // shouldn't downgrade
        assert_eq!(m.get("cowboy"), Some(&AppDepKind::Required));
    }

    #[test]
    fn strip_comments_preserves_strings() {
        let input = "{vsn, \"1.0%hash\"}. % this is a comment\n{deps, []}.";
        let out = strip_erlang_comments(input);
        assert!(out.contains("\"1.0%hash\""));
        assert!(!out.contains("this is a comment"));
        assert!(out.contains("{deps, []}"));
    }
}
