//! Milestone 140 — Elixir/Mix ecosystem reader.
//!
//! Discovers Elixir 1.4+ Mix projects under the scan root via two
//! input artifacts:
//!
//! - `mix.lock` (Elixir-syntax tuple map literal) — source-tier per
//!   FR-002. Regex-tokenized with brace-counted multi-line tuple
//!   handling (the lockfile IS stable Elixir source, not standardized
//!   data — but every prominent SBOM tool regex-parses it).
//! - `mix.exs` (Elixir source) — design-tier fallback per FR-005 +
//!   main-module name/version source per FR-012 + dev-scope cross-
//!   reference source per FR-008. Regex-extracted, no Elixir runtime.
//!
//! Three source discriminators with per-source PURL shapes:
//!
//! - **hex** (`:hex` tuple, default `"hexpm"` repo): `pkg:hex/<lc-name>@<version>`.
//! - **hex (private org `"hexpm:<org>"`)**: `pkg:hex/<org>/<lc-name>@<version>?repository_url=https://repo.hex.pm`
//!   per Phase 0 research correction — purl-spec hex-definition
//!   blesses the namespace-as-org form + `repository_url=` qualifier.
//! - **git** (`:git` tuple): `pkg:generic/<name>@<resolved-sha>?vcs_url=git+<url>`
//!   per Phase 0 — purl-spec doesn't bless `vcs_url=` for hex, so
//!   git-source emits via `pkg:generic/` (honest about loss of Hex.pm
//!   provenance once git-swapped).
//! - **path** (`:path` tuple): `pkg:generic/<name>@unspecified`
//!   placeholder + `mikebom:source-type = "hex-path"` annotation.
//!
//! Inner SHA-256 (4th tuple element) always emits; outer SHA-256 (8th,
//! optional) emits only when present + non-empty per Q3 best-effort.
//!
//! Umbrella projects (root `mix.exs` with `apps_path:` key) emit one
//! main-module per `mix.exs` (root + each sub-app under `apps/`);
//! root's `depends` lists each sub-app's main-module NAME per Q2.
//!
//! Design-tier `deps/0` extraction is conditional-flattened per Q1 —
//! every dep tuple in the file emits regardless of `if Mix.env() ...`
//! / `unless ...` / multi-clause `def deps(env)` nesting; components
//! inside any conditional carry `mikebom:elixir-extraction-mode =
//! "conditional-flattened"` annotation as a precision-loss signal.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use waybill_common::resolution::LifecycleScope;
use waybill_common::types::hash::{ContentHash, HashAlgorithm};
use waybill_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

const MAX_ELIXIR_WALK_DEPTH: usize = 12;

fn should_skip_descent(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".svn"
            | ".hg"
            | "_build"
            | "deps"
            | "node_modules"
            | "priv"
            | "cover"
    )
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum LockEntry {
    Hex {
        name: String,
        version: String,
        inner_sha256: String,
        repo: String,
        outer_sha256: Option<String>,
    },
    Git {
        name: String,
        url: String,
        resolved_sha: String,
        declared_ref: Option<String>,
    },
    Path {
        name: String,
        path: String,
        in_umbrella: bool,
    },
}

impl LockEntry {
    fn name(&self) -> &str {
        match self {
            LockEntry::Hex { name, .. } => name,
            LockEntry::Git { name, .. } => name,
            LockEntry::Path { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct MixExsInfo {
    app_name: Option<String>,
    version: Option<String>,
    is_umbrella: bool,
    deps: Vec<DeclaredDep>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DeclaredDep {
    name: String,
    constraint: Option<String>,
    dev_scope: bool,
    in_conditional: bool,
    source_kind: DeclaredDepSource,
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
        in_umbrella: bool,
    },
}

pub fn read(
    rootfs: &Path,
    _include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    let mut lockfile_dirs: HashSet<PathBuf> = HashSet::new();
    let mut warned = 0usize;
    let mut emitted_lockfile = 0usize;
    let mut emitted_design = 0usize;

    // Pass A — mix.lock walker.
    for lockfile_path in find_mix_locks(rootfs, exclude_set) {
        let Some(project_dir) = lockfile_path.parent() else {
            continue;
        };

        let text = match std::fs::read_to_string(&lockfile_path) {
            Ok(t) => t,
            Err(err) => {
                warned += 1;
                tracing::warn!(
                    path = %lockfile_path.display(),
                    error = %err,
                    "elixir: failed to read mix.lock; skipping",
                );
                continue;
            }
        };

        let tokens = tokenize_mix_lock(&text);
        if tokens.is_empty() && !text.trim().is_empty() && !text.trim().starts_with("%{}") {
            warned += 1;
            tracing::warn!(
                path = %lockfile_path.display(),
                "elixir: mix.lock parse yielded zero entries; falling back to sibling mix.exs design-tier if present",
            );
            // Fall through — pass B handles the design-tier fallback
            // for this project dir (project_dir NOT marked in
            // lockfile_dirs).
            continue;
        }

        let mut entries: Vec<LockEntry> = Vec::new();
        for (name, body) in &tokens {
            if let Some(e) = parse_lock_entry(name, body) {
                entries.push(e);
            } else {
                tracing::debug!(
                    path = %lockfile_path.display(),
                    name = %name,
                    "elixir: skipping unparseable lockfile entry",
                );
            }
        }

        lockfile_dirs.insert(project_dir.to_path_buf());

        let mix_exs_path = project_dir.join("mix.exs");
        let mix_exs_info: Option<MixExsInfo> = if mix_exs_path.is_file() {
            parse_mix_exs(&mix_exs_path).ok()
        } else {
            None
        };

        let declared_dep_names: Vec<String> = mix_exs_info
            .as_ref()
            .map(|i| i.deps.iter().map(|d| d.name.clone()).collect())
            .unwrap_or_default();

        if let Some(main_module) = emit_main_module(
            project_dir,
            mix_exs_path.is_file().then_some(mix_exs_path.as_path()),
            Some(&lockfile_path),
            mix_exs_info.as_ref(),
            true,
            &declared_dep_names,
        ) {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        let lock_entries =
            emit_lockfile_components(&lockfile_path, &entries, mix_exs_info.as_ref());
        emitted_lockfile += lock_entries.len();
        for entry in lock_entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    // Pass B — mix.exs design-tier walker (only for projects without
    // a sibling mix.lock that Pass A processed).
    let mut umbrella_roots: Vec<(PathBuf, String)> = Vec::new(); // (project_dir, root_app_name)
    for mix_exs_path in find_mix_exs(rootfs, exclude_set) {
        let Some(project_dir) = mix_exs_path.parent() else {
            continue;
        };
        let info = match parse_mix_exs(&mix_exs_path) {
            Ok(info) => info,
            Err(err) => {
                warned += 1;
                tracing::warn!(
                    path = %mix_exs_path.display(),
                    error = %err,
                    "elixir: failed to parse mix.exs; skipping",
                );
                continue;
            }
        };

        // Track umbrella roots for Pass C aggregation regardless of
        // tier — applies to lockfile-mode projects too if root has
        // apps_path.
        if info.is_umbrella {
            if let Some(app_name) = info.app_name.clone() {
                umbrella_roots.push((project_dir.to_path_buf(), app_name));
            }
        }

        // If this project dir was Pass A processed, skip design-tier
        // emission (lockfile already won) but still need to track
        // umbrella status (handled above).
        if lockfile_dirs.contains(project_dir) {
            continue;
        }

        let declared_dep_names: Vec<String> =
            info.deps.iter().map(|d| d.name.clone()).collect();
        if let Some(main_module) = emit_main_module(
            project_dir,
            Some(&mix_exs_path),
            None,
            Some(&info),
            false,
            &declared_dep_names,
        ) {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        let design_entries = emit_design_tier_components(&mix_exs_path, &info);
        emitted_design += design_entries.len();
        for entry in design_entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    // Pass C — umbrella root aggregation per Q2 + I1 remediation.
    // For each umbrella root, append each sub-app's main-module NAME
    // (not bom-ref — orchestrator handles name→bom-ref translation at
    // dep-edge wiring) to the root's `depends` list.
    for (root_dir, root_app_name) in &umbrella_roots {
        let sub_app_names: Vec<String> = collect_apps_subdirs(root_dir)
            .into_iter()
            .filter_map(|sub_mix_exs| {
                parse_mix_exs(&sub_mix_exs)
                    .ok()
                    .and_then(|info| info.app_name)
            })
            .collect();
        if sub_app_names.is_empty() {
            continue;
        }
        // Find the root main-module in `out` and append sub-app names.
        for entry in out.iter_mut() {
            if entry.name == *root_app_name
                && entry
                    .extra_annotations
                    .get("mikebom:umbrella-root")
                    .and_then(|v| v.as_str())
                    == Some("true")
            {
                for sub in &sub_app_names {
                    if !entry.depends.contains(sub) {
                        entry.depends.push(sub.clone());
                    }
                }
                break;
            }
        }
    }

    if !out.is_empty() || warned > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            emitted_lockfile,
            emitted_design,
            umbrella_roots = umbrella_roots.len(),
            warned,
            "parsed mix.lock + mix.exs entries",
        );
    }
    out
}

fn find_mix_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_ELIXIR_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("mix.lock") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn find_mix_exs(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_ELIXIR_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("mix.exs") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

/// Tokenize `mix.lock` into `(name, tuple_body)` pairs via brace
/// counting. Each entry shape: `"<name>": {...},` where the value's
/// `{...}` may contain nested `[...]` and quoted strings.
fn tokenize_mix_lock(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();

    // Skip past the opening `%{`.
    while i < len && bytes[i] != b'{' {
        i += 1;
    }
    if i >= len {
        return out;
    }
    i += 1; // past `{`

    loop {
        // Skip whitespace and commas.
        while i < len
            && (bytes[i] == b' '
                || bytes[i] == b'\n'
                || bytes[i] == b'\r'
                || bytes[i] == b'\t'
                || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= len || bytes[i] == b'}' {
            break;
        }
        // Expect `"<name>"`.
        if bytes[i] != b'"' {
            // Unexpected byte — skip
            i += 1;
            continue;
        }
        let name_start = i + 1;
        i += 1;
        while i < len && bytes[i] != b'"' {
            i += 1;
        }
        if i >= len {
            break;
        }
        let name_end = i;
        i += 1; // past closing `"`
        let name = String::from_utf8_lossy(&bytes[name_start..name_end]).into_owned();

        // Skip `:` + whitespace.
        while i < len && (bytes[i] == b':' || bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= len || bytes[i] != b'{' {
            // Unexpected — skip to next entry by scanning to next `"`
            // at indent.
            continue;
        }
        let body_start = i;
        // Brace-counted scan to find matching `}`. Quoted strings +
        // bracketed lists pass through transparently — we only need
        // to track brace depth to find the entry's terminator.
        let mut depth_brace = 0i32;
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
                    b'}' => {
                        depth_brace -= 1;
                        if depth_brace == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        if depth_brace > 0 {
            // Unterminated — bail.
            break;
        }
        let body_end = i;
        let body = String::from_utf8_lossy(&bytes[body_start..body_end]).into_owned();
        out.push((name, body));
    }
    out
}

fn parse_lock_entry(name: &str, tuple_body: &str) -> Option<LockEntry> {
    let trimmed = tuple_body.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let inner_trimmed = inner.trim_start();

    if let Some(rest) = inner_trimmed.strip_prefix(":hex") {
        return parse_hex_entry(name, rest);
    }
    if let Some(rest) = inner_trimmed.strip_prefix(":git") {
        return parse_git_entry(name, rest);
    }
    if let Some(rest) = inner_trimmed.strip_prefix(":path") {
        return parse_path_entry(name, rest);
    }
    None
}

fn parse_hex_entry(name: &str, rest: &str) -> Option<LockEntry> {
    // After `:hex`, the comma-separated parts are:
    //   :<atom_name>, "<version>", "<inner_sha256>", [<managers>],
    //   [<deps>], "<repo>", "<outer_sha256>"?
    // We use a forgiving extraction: pull the first 3 quoted strings
    // (version, inner_sha, then later repo + outer_sha), with skip
    // logic for the bracketed lists.
    let s = rest.trim_start_matches(',').trim_start();
    // Skip the atom name.
    if !s.starts_with(':') {
        return None;
    }
    let atom_end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_' && *c != ':')
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let after_atom = &s[atom_end..];

    // Extract quoted strings sequentially, skipping bracketed lists
    // (managers + deps).
    let strings = extract_quoted_strings_skipping_brackets(after_atom);
    // Position 0: version, Position 1: inner_sha256, Position 2: repo,
    // Position 3 (optional): outer_sha256.
    let version = strings.first()?.clone();
    let inner_sha256 = strings.get(1)?.clone();
    if inner_sha256.len() != 64 || !inner_sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let repo = strings.get(2).cloned().unwrap_or_else(|| "hexpm".to_string());
    let outer_sha256 = strings.get(3).cloned().filter(|s| !s.is_empty()
        && s.len() == 64
        && s.chars().all(|c| c.is_ascii_hexdigit()));
    Some(LockEntry::Hex {
        name: name.to_string(),
        version,
        inner_sha256,
        repo,
        outer_sha256,
    })
}

fn parse_git_entry(name: &str, rest: &str) -> Option<LockEntry> {
    let s = rest.trim_start_matches(',').trim_start();
    let strings = extract_quoted_strings_skipping_brackets(s);
    let url = strings.first()?.clone();
    let resolved_sha = strings.get(1)?.clone();
    if resolved_sha.len() != 40 || !resolved_sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // Find opts bracket — last `[...]` in the body.
    let declared_ref = extract_first_kw_from_brackets(s);
    Some(LockEntry::Git {
        name: name.to_string(),
        url,
        resolved_sha,
        declared_ref,
    })
}

fn parse_path_entry(name: &str, rest: &str) -> Option<LockEntry> {
    let s = rest.trim_start_matches(',').trim_start();
    let strings = extract_quoted_strings_skipping_brackets(s);
    let path = strings.first()?.clone();
    let in_umbrella = s.contains("in_umbrella: true") || s.contains("in_umbrella: :true");
    Some(LockEntry::Path {
        name: name.to_string(),
        path,
        in_umbrella,
    })
}

/// Extract top-level quoted strings, skipping content inside `[...]`
/// brackets (which contain managers / deps lists with their own
/// quoted strings).
fn extract_quoted_strings_skipping_brackets(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut in_str = false;
    let mut escape = false;
    let mut bracket_depth = 0i32;
    let mut current = String::new();
    while i < bytes.len() {
        let c = bytes[i];
        if escape {
            current.push(c as char);
            escape = false;
        } else if in_str {
            if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_str = false;
                if bracket_depth == 0 {
                    out.push(std::mem::take(&mut current));
                }
            } else if bracket_depth == 0 {
                current.push(c as char);
            }
        } else {
            match c {
                b'[' => bracket_depth += 1,
                b']' => bracket_depth -= 1,
                b'"' if bracket_depth == 0 => in_str = true,
                b'"' => in_str = true, // inside brackets — skip but track string
                _ => {}
            }
        }
        i += 1;
    }
    out
}

/// Extract the first keyword from the LAST `[...]` block in the body
/// (the opts list for `:git` / `:path` entries). Returns e.g.
/// `"ref: main"` for `[ref: "main"]`.
fn extract_first_kw_from_brackets(s: &str) -> Option<String> {
    // Find the last `[` and matching `]`.
    let open = s.rfind('[')?;
    let close = s[open..].find(']').map(|p| open + p)?;
    let inner = &s[open + 1..close].trim();
    if inner.is_empty() {
        return None;
    }
    // Match `<kw>: "<value>"` or `<kw>: :<atom>` or `<kw>: <atom>`.
    static KW_RE: OnceLock<Regex> = OnceLock::new();
    let re = KW_RE.get_or_init(|| {
        Regex::new(r#"^\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*(?:"([^"]+)"|:?([a-zA-Z_][a-zA-Z0-9_.]*))"#)
            .expect("static kw regex")
    });
    let captures = re.captures(inner)?;
    let key = captures.get(1)?.as_str();
    let value = captures
        .get(2)
        .or_else(|| captures.get(3))?
        .as_str();
    Some(format!("{key}: {value}"))
}

fn parse_mix_exs(path: &Path) -> anyhow::Result<MixExsInfo> {
    static APP_RE: OnceLock<Regex> = OnceLock::new();
    static VERSION_RE: OnceLock<Regex> = OnceLock::new();
    static APPS_PATH_RE: OnceLock<Regex> = OnceLock::new();
    static DEP_RE: OnceLock<Regex> = OnceLock::new();
    // Regexes match either start-of-line (multi-line `do ... end` form)
    // OR after `[` / `,` (single-line `do: [app: :x, version: "...", ...]`
    // shorthand). The dep_re false-positive filter (T008) drops `app` /
    // `version` / `deps` / `elixir` as dep names so a stray match in a
    // dep tuple's opts doesn't pollute the `deps` list.
    let app_re = APP_RE.get_or_init(|| {
        Regex::new(r"(?m)(?:^\s*|[\[,]\s*)app:\s*:([a-zA-Z_][a-zA-Z0-9_]*)\b")
            .expect("static app regex")
    });
    let version_re = VERSION_RE.get_or_init(|| {
        Regex::new(r#"(?m)(?:^\s*|[\[,]\s*)version:\s*"([^"]+)""#)
            .expect("static version regex")
    });
    let apps_path_re = APPS_PATH_RE.get_or_init(|| {
        Regex::new(r"(?m)(?:^\s*|[\[,]\s*)apps_path:\s*").expect("static apps_path regex")
    });
    let dep_re = DEP_RE.get_or_init(|| {
        // Capture: 1 = name, 2 = optional first-string constraint,
        // 3 = optional opts blob (everything between possibly a comma
        // and the closing `}`).
        Regex::new(r#"\{\s*:([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:,\s*"([^"]+)")?(?:,\s*([^}]*))?\s*\}"#)
            .expect("static dep regex")
    });

    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let mut info = MixExsInfo {
        app_name: app_re
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string())),
        version: version_re
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string())),
        is_umbrella: apps_path_re.is_match(&text),
        deps: Vec::new(),
    };

    // Per-line tracking + dep extraction.
    //
    // `block_stack` distinguishes opener types: `true` for
    // conditional openers (`if`/`unless`/`case`/`cond ... do`); `false`
    // for everything else (`def`/`defp`/`defmodule`/`with`/`for` etc).
    // A dep is `in_conditional` iff any `true` exists in the stack.
    // We pop on every `end` (assumes balanced); imbalanced files emit
    // best-effort results.
    let mut block_stack: Vec<bool> = Vec::new();
    let mut after_first_do = false;
    for raw_line in text.lines() {
        let line = strip_comment(raw_line);
        let trimmed = line.trim();

        // Detect `do`-opening line. We classify by first keyword.
        let opens_do = trimmed.ends_with(" do") || trimmed == "do" || trimmed.contains(" do ");
        if opens_do {
            let is_conditional = trimmed.starts_with("if ")
                || trimmed.starts_with("if(")
                || trimmed.starts_with("unless ")
                || trimmed.starts_with("unless(")
                || trimmed.starts_with("case ")
                || trimmed.starts_with("case(")
                || trimmed.starts_with("cond ")
                || trimmed == "cond do"
                || trimmed.contains(" if ")
                || trimmed.contains(" case ")
                || trimmed.contains(" unless ");
            block_stack.push(is_conditional);
            after_first_do = true;
        }
        // `end` line — pop. Bare `end` or trailing ` end`.
        if (trimmed == "end" || trimmed.ends_with(" end")) && !block_stack.is_empty() {
            block_stack.pop();
        }

        // Skip dep extraction until we've entered at least one `do`
        // block (avoids capturing tuples in module-level constants
        // before any `defmodule` opener).
        if !after_first_do {
            continue;
        }
        // Dep tuple extraction — match each tuple in the line.
        let in_conditional = block_stack.iter().any(|b| *b);
        for caps in dep_re.captures_iter(line) {
            let name = match caps.get(1) {
                Some(m) => m.as_str().to_string(),
                None => continue,
            };
            // Skip common false positives — keyword tuples that aren't
            // actually dep declarations.
            if name == "app" || name == "version" || name == "deps" || name == "elixir" {
                continue;
            }
            let constraint = caps.get(2).map(|m| m.as_str().to_string());
            let opts_blob = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let dev_scope = detect_dev_scope(opts_blob);
            let source_kind = detect_source_kind(opts_blob);
            info.deps.push(DeclaredDep {
                name,
                constraint,
                dev_scope,
                in_conditional,
                source_kind,
            });
        }
    }
    Ok(info)
}

fn strip_comment(line: &str) -> &str {
    match line.split_once('#') {
        Some((s, _)) => s,
        None => line,
    }
    .trim_end()
}

fn detect_dev_scope(opts_blob: &str) -> bool {
    if opts_blob.contains("runtime: false") {
        return true;
    }
    // Match `only: :dev` / `only: :test` / `only: [:dev, :test]`.
    if let Some(idx) = opts_blob.find("only:") {
        let rest = &opts_blob[idx + 5..];
        if rest.contains(":dev") || rest.contains(":test") || rest.contains(":doc") {
            return true;
        }
    }
    false
}

fn detect_source_kind(opts_blob: &str) -> DeclaredDepSource {
    // Match `path: "..."`.
    if let Some(p) = extract_kw_string(opts_blob, "path") {
        let in_umbrella = opts_blob.contains("in_umbrella: true");
        return DeclaredDepSource::Path { path: p, in_umbrella };
    }
    // Match `git: "..."`.
    if let Some(u) = extract_kw_string(opts_blob, "git") {
        return DeclaredDepSource::Git {
            url: u,
            declared_ref: extract_first_git_ref(opts_blob),
        };
    }
    // Match `github: "owner/repo"` shortcut.
    if let Some(slug) = extract_kw_string(opts_blob, "github") {
        let url = format!("https://github.com/{slug}.git");
        return DeclaredDepSource::Git {
            url,
            declared_ref: extract_first_git_ref(opts_blob),
        };
    }
    if opts_blob.contains("in_umbrella: true") {
        return DeclaredDepSource::Path {
            path: String::new(),
            in_umbrella: true,
        };
    }
    DeclaredDepSource::Hex
}

fn extract_kw_string(opts: &str, key: &str) -> Option<String> {
    let pat = format!(r#"\b{key}\s*:\s*"([^"]+)""#);
    let re = Regex::new(&pat).ok()?;
    re.captures(opts).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn extract_first_git_ref(opts: &str) -> Option<String> {
    for key in &["ref", "tag", "branch", "commit"] {
        if let Some(v) = extract_kw_string(opts, key) {
            return Some(format!("{key}: {v}"));
        }
    }
    None
}

fn build_purl_for_lock_entry(entry: &LockEntry) -> Result<Purl, String> {
    let purl_str = match entry {
        LockEntry::Hex {
            name,
            version,
            repo,
            ..
        } => {
            let lc_name = name.to_lowercase();
            if let Some(org) = repo.strip_prefix("hexpm:") {
                let lc_org = org.to_lowercase();
                format!(
                    "pkg:hex/{lc_org}/{lc_name}@{version}?repository_url=https://repo.hex.pm"
                )
            } else {
                format!("pkg:hex/{lc_name}@{version}")
            }
        }
        LockEntry::Git {
            name,
            url,
            resolved_sha,
            ..
        } => {
            format!(
                "pkg:generic/{name}@{resolved_sha}?vcs_url=git+{url}",
                url = minimal_qualifier_encode(url),
            )
        }
        LockEntry::Path { name, .. } => {
            format!("pkg:generic/{name}@unspecified")
        }
    };
    Purl::new(&purl_str).map_err(|e| format!("PURL construction failed for {purl_str}: {e:?}"))
}

fn classify_source_type(entry: &LockEntry) -> &'static str {
    match entry {
        LockEntry::Hex { .. } => "hex-hex",
        LockEntry::Git { .. } => "hex-git",
        LockEntry::Path { .. } => "hex-path",
    }
}

fn build_extra_annotations(
    entry: &LockEntry,
    source_type_value: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    out.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String(source_type_value.to_string()),
    );
    match entry {
        LockEntry::Git {
            declared_ref: Some(r),
            ..
        } => {
            out.insert(
                "mikebom:vcs-declared-ref".to_string(),
                serde_json::Value::String(r.clone()),
            );
        }
        LockEntry::Path {
            path, in_umbrella, ..
        } => {
            out.insert(
                "mikebom:path".to_string(),
                serde_json::Value::String(path.clone()),
            );
            if *in_umbrella {
                out.insert(
                    "mikebom:in-umbrella".to_string(),
                    serde_json::Value::String("true".to_string()),
                );
            }
        }
        _ => {}
    }
    out
}

fn emit_main_module(
    project_dir: &Path,
    mix_exs_path: Option<&Path>,
    lockfile_path: Option<&Path>,
    info: Option<&MixExsInfo>,
    doc_has_lockfile: bool,
    declared_dep_names_for_depends: &[String],
) -> Option<PackageDbEntry> {
    let app_name = info
        .and_then(|i| i.app_name.clone())
        .or_else(|| {
            project_dir
                .file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
        })?;
    if app_name.is_empty() {
        return None;
    }
    let version = info
        .and_then(|i| i.version.clone())
        .unwrap_or_else(|| "0.0.0-unknown".to_string());
    let lc_app_name = app_name.to_lowercase();
    let purl_str = format!("pkg:hex/{lc_app_name}@{version}");
    let purl = Purl::new(&purl_str).ok()?;

    let source_path = mix_exs_path
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| lockfile_path.map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| project_dir.to_string_lossy().into_owned());

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("hex-main-module".to_string()),
    );
    if info.is_some_and(|i| i.is_umbrella) {
        extra_annotations.insert(
            "mikebom:umbrella-root".to_string(),
            serde_json::Value::String("true".to_string()),
        );
    }

    let sbom_tier = if doc_has_lockfile { "source" } else { "design" };

    Some(PackageDbEntry {
        purl,
        name: app_name,
        version,
        arch: None,
        source_path,
        depends: declared_dep_names_for_depends.to_vec(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("hex-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some("mix-exs".to_string()),
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

fn emit_lockfile_components(
    lockfile_path: &Path,
    entries: &[LockEntry],
    mix_exs_info: Option<&MixExsInfo>,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = lockfile_path.to_string_lossy().into_owned();
    for entry in entries {
        let purl = match build_purl_for_lock_entry(entry) {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(
                    name = %entry.name(),
                    path = %lockfile_path.display(),
                    error = %err,
                    "elixir: skipping malformed lockfile entry",
                );
                continue;
            }
        };
        let source_type_value = classify_source_type(entry);
        let extra_annotations = build_extra_annotations(entry, source_type_value);

        // Hashes per FR-011 + Q3 — only for hex entries.
        let hashes: Vec<ContentHash> = match entry {
            LockEntry::Hex {
                inner_sha256,
                outer_sha256,
                ..
            } => {
                let mut hs = Vec::new();
                if let Ok(h) =
                    ContentHash::with_algorithm(HashAlgorithm::Sha256, inner_sha256)
                {
                    hs.push(h);
                }
                if let Some(outer) = outer_sha256 {
                    if let Ok(h) = ContentHash::with_algorithm(HashAlgorithm::Sha256, outer) {
                        hs.push(h);
                    }
                }
                hs
            }
            _ => Vec::new(),
        };

        // Scope cross-reference per FR-008.
        let lifecycle_scope = mix_exs_info
            .and_then(|info| info.deps.iter().find(|d| d.name == entry.name()))
            .map(|d| {
                if d.dev_scope {
                    LifecycleScope::Development
                } else {
                    LifecycleScope::Runtime
                }
            })
            .unwrap_or(LifecycleScope::Runtime);

        let (name, version) = match entry {
            LockEntry::Hex { name, version, .. } => (name.clone(), version.clone()),
            LockEntry::Git {
                name, resolved_sha, ..
            } => (name.clone(), resolved_sha.clone()),
            LockEntry::Path { name, .. } => (name.clone(), "unspecified".to_string()),
        };

        out.push(PackageDbEntry {
            purl,
            name,
            version,
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: Some(lifecycle_scope),
            requirement_ranges: Vec::new(),
            source_type: Some(source_type_value.to_string()),
            buildinfo_status: None,
            sbom_tier: Some("source".to_string()),
            evidence_kind: Some("mix-lock".to_string()),
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
        });
    }
    out
}

fn emit_design_tier_components(
    mix_exs_path: &Path,
    info: &MixExsInfo,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = mix_exs_path.to_string_lossy().into_owned();
    let main_module_name = info.app_name.as_deref().unwrap_or("");

    for decl in &info.deps {
        if decl.name == main_module_name {
            continue;
        }
        let constraint = decl
            .constraint
            .clone()
            .unwrap_or_else(|| "unspecified".to_string());
        let sanitized = sanitize_purl_version(&constraint);

        // Per C2 remediation: dispatch on source_kind to construct
        // correct PURL shape per FR-003.
        let (purl_str, source_type_value, source_specific_anns) = match &decl.source_kind {
            DeclaredDepSource::Hex => {
                let lc_name = decl.name.to_lowercase();
                let purl = format!("pkg:hex/{lc_name}@{sanitized}");
                (purl, "hex-hex", BTreeMap::<String, serde_json::Value>::new())
            }
            DeclaredDepSource::Git { url, declared_ref } => {
                let purl = format!(
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
                (purl, "hex-git", anns)
            }
            DeclaredDepSource::Path { path, in_umbrella } => {
                let purl = format!("pkg:generic/{}@unspecified", decl.name);
                let mut anns: BTreeMap<String, serde_json::Value> = BTreeMap::new();
                if !path.is_empty() {
                    anns.insert(
                        "mikebom:path".to_string(),
                        serde_json::Value::String(path.clone()),
                    );
                }
                if *in_umbrella {
                    anns.insert(
                        "mikebom:in-umbrella".to_string(),
                        serde_json::Value::String("true".to_string()),
                    );
                }
                (purl, "hex-path", anns)
            }
        };

        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::warn!(
                name = %decl.name,
                purl = %purl_str,
                path = %mix_exs_path.display(),
                "elixir: skipping design-tier entry with non-PURL-safe form",
            );
            continue;
        };

        let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        extra_annotations.insert(
            "mikebom:source-type".to_string(),
            serde_json::Value::String(source_type_value.to_string()),
        );
        for (k, v) in source_specific_anns {
            extra_annotations.insert(k, v);
        }
        if decl.in_conditional {
            extra_annotations.insert(
                "mikebom:elixir-extraction-mode".to_string(),
                serde_json::Value::String("conditional-flattened".to_string()),
            );
        }

        let (version_field, requirement_ranges) = match &decl.source_kind {
            DeclaredDepSource::Hex => (
                sanitized.clone(),
                vec![decl.constraint.clone().unwrap_or_default()],
            ),
            _ => ("unspecified".to_string(), Vec::new()),
        };

        let lifecycle_scope = if decl.dev_scope {
            LifecycleScope::Development
        } else {
            LifecycleScope::Runtime
        };

        out.push(PackageDbEntry {
            purl,
            name: decl.name.clone(),
            version: version_field,
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: Some(lifecycle_scope),
            requirement_ranges,
            source_type: Some(source_type_value.to_string()),
            buildinfo_status: None,
            sbom_tier: Some("design".to_string()),
            evidence_kind: Some("mix-exs".to_string()),
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
        });
    }
    out
}

fn collect_apps_subdirs(umbrella_root: &Path) -> Vec<PathBuf> {
    let apps_dir = umbrella_root.join("apps");
    if !apps_dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&apps_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let mix_exs = path.join("mix.exs");
            if mix_exs.is_file() {
                out.push(mix_exs);
            }
        }
    }
    out.sort();
    out
}

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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_lockfile() {
        let text = r#"%{
  "phoenix": {:hex, :phoenix, "1.7.10", "abc", [:mix], [], "hexpm", "def"},
  "plug": {:hex, :plug, "1.15.2", "xyz", [:mix], [], "hexpm"}
}
"#;
        let tokens = tokenize_mix_lock(text);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].0, "phoenix");
        assert!(tokens[0].1.starts_with("{:hex"));
        assert_eq!(tokens[1].0, "plug");
    }

    #[test]
    fn parse_hex_entry_with_outer_sha() {
        let body = r#"{:hex, :phoenix, "1.7.10", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", [:mix], [], "hexpm", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#;
        let e = parse_lock_entry("phoenix", body).unwrap();
        match e {
            LockEntry::Hex {
                name,
                version,
                inner_sha256,
                repo,
                outer_sha256,
            } => {
                assert_eq!(name, "phoenix");
                assert_eq!(version, "1.7.10");
                assert_eq!(inner_sha256.len(), 64);
                assert_eq!(repo, "hexpm");
                assert!(outer_sha256.is_some());
            }
            _ => panic!("expected Hex variant"),
        }
    }

    #[test]
    fn parse_hex_entry_without_outer_sha() {
        let body = r#"{:hex, :plug, "1.15.2", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", [:mix], [], "hexpm"}"#;
        let e = parse_lock_entry("plug", body).unwrap();
        match e {
            LockEntry::Hex { outer_sha256, .. } => {
                assert!(outer_sha256.is_none(), "pre-Hex-2.0 entry must have None outer SHA");
            }
            _ => panic!("expected Hex"),
        }
    }

    #[test]
    fn parse_hex_entry_private_org() {
        let body = r#"{:hex, :my_lib, "2.0.0", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", [:mix], [], "hexpm:acme", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#;
        let e = parse_lock_entry("my_lib", body).unwrap();
        match e {
            LockEntry::Hex { repo, .. } => assert_eq!(repo, "hexpm:acme"),
            _ => panic!("expected Hex"),
        }
    }

    #[test]
    fn parse_git_entry_with_ref() {
        let body = r#"{:git, "https://github.com/foo/my-fork.git", "eb39649a76b87e8451baf75d10ce82ca3a3d5601", [ref: "main"]}"#;
        let e = parse_lock_entry("my_fork", body).unwrap();
        match e {
            LockEntry::Git {
                name,
                url,
                resolved_sha,
                declared_ref,
            } => {
                assert_eq!(name, "my_fork");
                assert_eq!(url, "https://github.com/foo/my-fork.git");
                assert_eq!(resolved_sha.len(), 40);
                assert_eq!(declared_ref, Some("ref: main".to_string()));
            }
            _ => panic!("expected Git"),
        }
    }

    #[test]
    fn parse_path_entry() {
        let body = r#"{:path, "apps/shared_lib", []}"#;
        let e = parse_lock_entry("shared_lib", body).unwrap();
        match e {
            LockEntry::Path {
                name,
                path,
                in_umbrella,
            } => {
                assert_eq!(name, "shared_lib");
                assert_eq!(path, "apps/shared_lib");
                assert!(!in_umbrella);
            }
            _ => panic!("expected Path"),
        }
    }

    #[test]
    fn parse_path_entry_in_umbrella() {
        let body = r#"{:path, "apps/core", [in_umbrella: true]}"#;
        let e = parse_lock_entry("core", body).unwrap();
        match e {
            LockEntry::Path { in_umbrella, .. } => assert!(in_umbrella),
            _ => panic!("expected Path"),
        }
    }

    #[test]
    fn build_purl_hex_default() {
        let e = LockEntry::Hex {
            name: "phoenix".into(),
            version: "1.7.10".into(),
            inner_sha256: "a".repeat(64),
            repo: "hexpm".into(),
            outer_sha256: None,
        };
        let p = build_purl_for_lock_entry(&e).unwrap();
        assert_eq!(p.as_str(), "pkg:hex/phoenix@1.7.10");
    }

    #[test]
    fn build_purl_hex_private_org_emits_namespace_and_repo_url() {
        let e = LockEntry::Hex {
            name: "internal_lib".into(),
            version: "2.0.0".into(),
            inner_sha256: "a".repeat(64),
            repo: "hexpm:acme".into(),
            outer_sha256: None,
        };
        let p = build_purl_for_lock_entry(&e).unwrap();
        assert_eq!(
            p.as_str(),
            "pkg:hex/acme/internal_lib@2.0.0?repository_url=https://repo.hex.pm"
        );
    }

    #[test]
    fn build_purl_git_uses_pkg_generic() {
        let e = LockEntry::Git {
            name: "my_fork".into(),
            url: "https://github.com/foo/my-fork.git".into(),
            resolved_sha: "eb39649a76b87e8451baf75d10ce82ca3a3d5601".into(),
            declared_ref: Some("ref: main".into()),
        };
        let p = build_purl_for_lock_entry(&e).unwrap();
        assert_eq!(
            p.as_str(),
            "pkg:generic/my_fork@eb39649a76b87e8451baf75d10ce82ca3a3d5601?vcs_url=git+https://github.com/foo/my-fork.git"
        );
    }

    #[test]
    fn build_purl_path_uses_pkg_generic() {
        let e = LockEntry::Path {
            name: "shared_lib".into(),
            path: "apps/shared_lib".into(),
            in_umbrella: false,
        };
        let p = build_purl_for_lock_entry(&e).unwrap();
        assert_eq!(p.as_str(), "pkg:generic/shared_lib@unspecified");
    }

    #[test]
    fn build_purl_hex_name_lowercased() {
        let e = LockEntry::Hex {
            name: "Phoenix".into(),
            version: "1.7.10".into(),
            inner_sha256: "a".repeat(64),
            repo: "hexpm".into(),
            outer_sha256: None,
        };
        let p = build_purl_for_lock_entry(&e).unwrap();
        assert_eq!(p.as_str(), "pkg:hex/phoenix@1.7.10");
    }

    #[test]
    fn classify_source_type_dispatch() {
        let h = LockEntry::Hex {
            name: "x".into(),
            version: "1".into(),
            inner_sha256: "a".repeat(64),
            repo: "hexpm".into(),
            outer_sha256: None,
        };
        assert_eq!(classify_source_type(&h), "hex-hex");
        let g = LockEntry::Git {
            name: "x".into(),
            url: "u".into(),
            resolved_sha: "a".repeat(40),
            declared_ref: None,
        };
        assert_eq!(classify_source_type(&g), "hex-git");
        let p = LockEntry::Path {
            name: "x".into(),
            path: "p".into(),
            in_umbrella: false,
        };
        assert_eq!(classify_source_type(&p), "hex-path");
    }

    #[test]
    fn extra_annotations_git_carries_declared_ref() {
        let e = LockEntry::Git {
            name: "x".into(),
            url: "u".into(),
            resolved_sha: "a".repeat(40),
            declared_ref: Some("ref: main".into()),
        };
        let ann = build_extra_annotations(&e, "hex-git");
        assert_eq!(
            ann.get("mikebom:vcs-declared-ref").and_then(|v| v.as_str()),
            Some("ref: main")
        );
    }

    #[test]
    fn extra_annotations_path_carries_path() {
        let e = LockEntry::Path {
            name: "x".into(),
            path: "../shared".into(),
            in_umbrella: true,
        };
        let ann = build_extra_annotations(&e, "hex-path");
        assert_eq!(ann.get("mikebom:path").and_then(|v| v.as_str()), Some("../shared"));
        assert_eq!(
            ann.get("mikebom:in-umbrella").and_then(|v| v.as_str()),
            Some("true")
        );
    }

    #[test]
    fn parse_mix_exs_extracts_app_version_deps() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mix.exs");
        std::fs::write(
            &path,
            r#"defmodule MyApp.MixProject do
  use Mix.Project

  def project do
    [
      app: :my_app,
      version: "0.5.2",
      elixir: "~> 1.16",
      deps: deps()
    ]
  end

  defp deps do
    [
      {:phoenix, "~> 1.7"},
      {:plug, "~> 1.15", optional: false},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false}
    ]
  end
end
"#,
        )
        .unwrap();
        let info = parse_mix_exs(&path).unwrap();
        assert_eq!(info.app_name.as_deref(), Some("my_app"));
        assert_eq!(info.version.as_deref(), Some("0.5.2"));
        assert!(!info.is_umbrella);
        assert_eq!(info.deps.len(), 3);
        assert_eq!(info.deps[0].name, "phoenix");
        assert_eq!(info.deps[0].constraint.as_deref(), Some("~> 1.7"));
        assert!(!info.deps[0].dev_scope);
        assert!(matches!(info.deps[0].source_kind, DeclaredDepSource::Hex));
        assert!(info.deps[2].dev_scope, "credo must have dev_scope (only/runtime: false)");
    }

    #[test]
    fn parse_mix_exs_detects_umbrella_shorthand_form() {
        // Regression for the apps_path KEY-presence regex needing to
        // match mid-line `, apps_path:` (single-line `do:` shorthand).
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mix.exs");
        std::fs::write(
            &path,
            r#"defmodule MyUmbrella.MixProject do
  def project, do: [app: :my_umbrella, version: "0.1.0", apps_path: "modules"]
end
"#,
        )
        .unwrap();
        let info = parse_mix_exs(&path).unwrap();
        assert!(
            info.is_umbrella,
            "shorthand form `, apps_path:` mid-line must trigger umbrella detection",
        );
        assert_eq!(info.app_name.as_deref(), Some("my_umbrella"));
        assert_eq!(info.version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn parse_mix_exs_detects_umbrella() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mix.exs");
        std::fs::write(
            &path,
            r#"defmodule Umbrella.MixProject do
  def project do
    [
      apps_path: "apps",
      version: "0.1.0",
      app: :my_umbrella,
      deps: deps()
    ]
  end

  defp deps do
    [{:dialyxir, "~> 1.4", only: [:dev], runtime: false}]
  end
end
"#,
        )
        .unwrap();
        let info = parse_mix_exs(&path).unwrap();
        assert!(info.is_umbrella);
    }

    #[test]
    fn parse_mix_exs_detects_git_and_path_source_kinds() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mix.exs");
        std::fs::write(
            &path,
            r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps do
    [
      {:phx, "~> 1.7"},
      {:my_fork, git: "https://github.com/foo/my-fork.git", branch: "main"},
      {:shared, path: "../shared"},
      {:gh_shortcut, github: "owner/repo"}
    ]
  end
end
"#,
        )
        .unwrap();
        let info = parse_mix_exs(&path).unwrap();
        let by_name = |n: &str| info.deps.iter().find(|d| d.name == n).unwrap();

        assert!(matches!(by_name("phx").source_kind, DeclaredDepSource::Hex));

        match &by_name("my_fork").source_kind {
            DeclaredDepSource::Git { url, declared_ref } => {
                assert_eq!(url, "https://github.com/foo/my-fork.git");
                assert_eq!(declared_ref.as_deref(), Some("branch: main"));
            }
            other => panic!("expected Git, got {other:?}"),
        }

        match &by_name("shared").source_kind {
            DeclaredDepSource::Path { path, .. } => assert_eq!(path, "../shared"),
            other => panic!("expected Path, got {other:?}"),
        }

        match &by_name("gh_shortcut").source_kind {
            DeclaredDepSource::Git { url, .. } => {
                assert_eq!(url, "https://github.com/owner/repo.git");
            }
            other => panic!("expected Git from :github shortcut, got {other:?}"),
        }
    }

    #[test]
    fn parse_mix_exs_marks_conditional_deps() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mix.exs");
        std::fs::write(
            &path,
            r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps do
    [
      {:phx, "~> 1.7"},
      if Mix.env() == :test do
        {:meck, "~> 0.9"}
      end
    ]
  end
end
"#,
        )
        .unwrap();
        let info = parse_mix_exs(&path).unwrap();
        let phx = info.deps.iter().find(|d| d.name == "phx").unwrap();
        let meck = info.deps.iter().find(|d| d.name == "meck").unwrap();
        assert!(!phx.in_conditional, "top-level dep is not conditional");
        assert!(meck.in_conditional, "dep inside `if Mix.env()` IS conditional");
    }

    #[test]
    fn sanitize_purl_version_neutralizes_unsafe_chars() {
        assert_eq!(sanitize_purl_version("~> 1.7"), "~>_1.7");
        assert_eq!(sanitize_purl_version(">= 1.0 and < 2.0"), ">=_1.0_and_<_2.0");
    }

    #[test]
    fn extract_first_git_ref_picks_first_present() {
        assert_eq!(
            extract_first_git_ref(r#"git: "u", ref: "main""#),
            Some("ref: main".to_string()),
        );
        assert_eq!(
            extract_first_git_ref(r#"git: "u", branch: "develop""#),
            Some("branch: develop".to_string()),
        );
        assert_eq!(extract_first_git_ref(r#"git: "u""#), None);
    }
}
