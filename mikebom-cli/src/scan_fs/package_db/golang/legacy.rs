//! Read Go source-tree package metadata from `go.mod` + `go.sum`.
//!
//! Source-tier (FR-012/R3): a `go.sum` declares the exact version + h1
//! hash for every module the build pulls in, direct or transitive. This
//! is authoritative enough to emit `sbom_tier = "source"` components.
//! `go.mod` layers a dependency graph on top (direct requires → main
//! module) plus `replace` / `exclude` directives that rewrite or drop
//! entries before conversion.
//!
//! Transitive dep-graph enrichment: `go.sum` doesn't encode module →
//! module edges, but the Go module cache does — each downloaded
//! module's own `go.mod` sits at
//! `<GOMODCACHE>/cache/download/<escaped>/@v/<version>.mod` and lists
//! its declared `require` block. When the cache is present (CI,
//! developer machines, build containers that haven't been cleaned),
//! the reader fetches each module's go.mod and populates `depends`
//! accordingly. Cache-absent scans (scratch images, stripped build
//! artefacts) still emit the root → direct-dep edges; transitive
//! nodes stay flat.
//!
//! Not in scope for this milestone: private module proxy lookup, module
//! cache file-hash verification, `vendor/` directory component
//! extraction. Those are follow-ups.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use mikebom_common::types::license::SpdxExpression;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

// `super` is now `golang/` (not `package_db/`) post-milestone-055 T008
// directory promotion. Import via crate-absolute path so the reference
// is unambiguous regardless of where this module nests in the tree.
use crate::scan_fs::package_db::PackageDbEntry;

/// Max depth for the recursive project-root search. Matches the npm
/// walker's budget — enough to cover monorepo shapes without running
/// away into source trees.
const MAX_PROJECT_ROOT_DEPTH: usize = 6;

// ---------------------------------------------------------------------------
// Module cache lookup — for transitive dep-graph reconstruction
// ---------------------------------------------------------------------------

/// Encode a Go module path for the filesystem layout the module cache
/// uses. Every uppercase letter `X` becomes `!x` — e.g.
/// `github.com/Azure/azure-sdk-for-go` → `github.com/!azure/azure-sdk-for-go`.
/// Non-ASCII characters and punctuation pass through unchanged (no
/// module path in the wild uses them outside ASCII identifiers).
pub(crate) fn escape_module_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 4);
    for ch in path.chars() {
        if ch.is_ascii_uppercase() {
            out.push('!');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Candidate module-cache roots for a given scan. Populated once per
/// scan to avoid redundant I/O across N module lookups. Each entry is
/// expected to contain a `cache/download/...` subtree.
#[derive(Clone, Debug, Default)]
pub(crate) struct GoModCache {
    roots: Vec<PathBuf>,
}

impl GoModCache {
    /// Discover candidate cache roots in priority order:
    /// 1. `$GOMODCACHE` environment variable (honouring the user's
    ///    local Go setup when running `--path` scans).
    /// 2. `$HOME/go/pkg/mod` (default when GOMODCACHE isn't set).
    /// 3. `<rootfs>/root/go/pkg/mod` (conventional in container images
    ///    that bake the cache in).
    /// 4. `<rootfs>/go/pkg/mod`
    /// 5. `<rootfs>/home/*/go/pkg/mod` (multi-user images).
    /// 6. `<rootfs>/usr/local/go/pkg/mod`
    ///
    /// Each candidate is included only when its `cache/download`
    /// subdirectory actually exists. The order matters for deterministic
    /// resolution when multiple caches are present — earlier wins.
    pub(crate) fn discover(rootfs: &Path) -> Self {
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        let mut try_add = |candidate: PathBuf, roots: &mut Vec<PathBuf>| {
            let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate.clone());
            if !seen.insert(canonical) {
                return;
            }
            if candidate.join("cache/download").is_dir() {
                roots.push(candidate);
            }
        };

        if let Ok(env) = std::env::var("GOMODCACHE") {
            if !env.is_empty() {
                try_add(PathBuf::from(&env), &mut roots);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                try_add(PathBuf::from(&home).join("go/pkg/mod"), &mut roots);
            }
        }
        try_add(rootfs.join("root/go/pkg/mod"), &mut roots);
        try_add(rootfs.join("go/pkg/mod"), &mut roots);
        // Enumerate rootfs/home/<user>/go/pkg/mod — common on
        // multi-user container layouts.
        if let Ok(home_dir) = std::fs::read_dir(rootfs.join("home")) {
            for entry in home_dir.flatten() {
                let candidate = entry.path().join("go/pkg/mod");
                try_add(candidate, &mut roots);
            }
        }
        try_add(rootfs.join("usr/local/go/pkg/mod"), &mut roots);

        GoModCache { roots }
    }

    /// True when no cache roots were discovered. Used by the
    /// milestone 055 resolver to short-circuit step 2 (cache walk)
    /// when there's no cache to walk.
    pub(crate) fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Read `<cache>/cache/download/<escaped>/@v/<version>.mod` and
    /// return its contents. Returns `None` when no cache root has the
    /// file. IO errors are swallowed and reported as `None` so a single
    /// unreadable module doesn't abort the broader scan.
    pub(crate) fn read_mod_file(&self, module: &str, version: &str) -> Option<String> {
        if self.roots.is_empty() {
            return None;
        }
        let escaped = escape_module_path(module);
        let relative = format!("cache/download/{escaped}/@v/{version}.mod");
        for root in &self.roots {
            let path = root.join(&relative);
            if let Ok(text) = std::fs::read_to_string(&path) {
                return Some(text);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// go.mod parser
// ---------------------------------------------------------------------------

/// One `require` line from a `go.mod`. `indirect` tracks the `// indirect`
/// trailing comment Go emits for transitively-needed modules that aren't
/// imported directly. We keep both so downstream consumers can choose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoModRequire {
    pub path: String,
    pub version: String,
    pub indirect: bool,
}

/// Parsed `go.mod` contents. `replaces` maps `(old_path, old_version) →
/// (new_path, new_version)` — an `old_version` of `""` means "match any
/// version of old_path". `excludes` holds the set that must be filtered
/// out before PURL construction.
#[derive(Clone, Debug, Default)]
pub(crate) struct GoModDocument {
    pub module_path: Option<String>,
    pub go_version: Option<String>,
    pub requires: Vec<GoModRequire>,
    pub replaces: HashMap<(String, String), (String, String)>,
    pub excludes: HashSet<(String, String)>,
}

/// Parse a `go.mod` file body into a [`GoModDocument`]. The parser is
/// line-oriented and deliberately lenient: unknown directives and
/// malformed lines are skipped rather than rejecting the whole file.
/// This mirrors `go mod`'s own tolerance for files that were hand-edited
/// between runs.
pub(crate) fn parse_go_mod(text: &str) -> GoModDocument {
    let mut doc = GoModDocument::default();
    let mut lines = text.lines();

    while let Some(raw) = lines.next() {
        let stripped = strip_line_comment(raw);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("module ") {
            doc.module_path = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("go ") {
            doc.go_version = Some(rest.trim().to_string());
        } else if line == "require (" {
            for raw_inner in lines.by_ref() {
                let inner_owned = strip_line_comment_preserving_indirect(raw_inner);
                let inner = inner_owned.trim();
                if inner == ")" {
                    break;
                }
                if inner.is_empty() {
                    continue;
                }
                if let Some(req) = parse_require_line(inner) {
                    doc.requires.push(req);
                }
            }
        } else if let Some(rest) = line.strip_prefix("require ") {
            if let Some(req) = parse_require_line(rest) {
                doc.requires.push(req);
            }
        } else if line == "replace (" {
            for raw_inner in lines.by_ref() {
                let inner_owned = strip_line_comment(raw_inner);
                let inner = inner_owned.trim();
                if inner == ")" {
                    break;
                }
                if inner.is_empty() {
                    continue;
                }
                if let Some((k, v)) = parse_replace_line(inner) {
                    doc.replaces.insert(k, v);
                }
            }
        } else if let Some(rest) = line.strip_prefix("replace ") {
            if let Some((k, v)) = parse_replace_line(rest) {
                doc.replaces.insert(k, v);
            }
        } else if line == "exclude (" {
            for raw_inner in lines.by_ref() {
                let inner_owned = strip_line_comment(raw_inner);
                let inner = inner_owned.trim();
                if inner == ")" {
                    break;
                }
                if inner.is_empty() {
                    continue;
                }
                if let Some(coord) = parse_module_version_pair(inner) {
                    doc.excludes.insert(coord);
                }
            }
        } else if let Some(rest) = line.strip_prefix("exclude ") {
            if let Some(coord) = parse_module_version_pair(rest) {
                doc.excludes.insert(coord);
            }
        }
        // else: unknown directive (`toolchain`, `retract`, ...) — skip.
    }

    doc
}

/// Strip `// ...` line comments, but preserve the `// indirect` marker
/// — callers inside `require` blocks need to see it to flag the entry.
fn strip_line_comment_preserving_indirect(line: &str) -> String {
    let trimmed_end = line.trim_end();
    if let Some(comment_start) = trimmed_end.find("//") {
        let (code, comment) = trimmed_end.split_at(comment_start);
        if comment.trim() == "// indirect" {
            return format!("{code} // indirect");
        }
        code.to_string()
    } else {
        trimmed_end.to_string()
    }
}

/// Strip `// ...` comments from a line. Used outside `require` blocks
/// where the `// indirect` marker isn't meaningful.
fn strip_line_comment(line: &str) -> String {
    if let Some(i) = line.find("//") {
        line[..i].to_string()
    } else {
        line.to_string()
    }
}

fn parse_require_line(rest: &str) -> Option<GoModRequire> {
    let indirect = rest.contains("// indirect");
    let without_comment = rest.split("//").next().unwrap_or("").trim();
    let mut parts = without_comment.split_whitespace();
    let path = parts.next()?.trim_matches('"').to_string();
    let version = parts.next()?.trim_matches('"').to_string();
    if path.is_empty() || version.is_empty() {
        return None;
    }
    Some(GoModRequire {
        path,
        version,
        indirect,
    })
}

/// Parse `old-path [old-version] => new-path [new-version]`. Returns
/// `((old_path, old_version_or_empty), (new_path, new_version_or_empty))`.
fn parse_replace_line(rest: &str) -> Option<((String, String), (String, String))> {
    let (lhs, rhs) = rest.split_once("=>")?;
    let lhs_parts: Vec<&str> = lhs.split_whitespace().collect();
    let rhs_parts: Vec<&str> = rhs.split_whitespace().collect();
    let (old_path, old_ver) = match lhs_parts.as_slice() {
        [path] => (path.to_string(), String::new()),
        [path, ver] => (path.to_string(), ver.to_string()),
        _ => return None,
    };
    let (new_path, new_ver) = match rhs_parts.as_slice() {
        [path] => (path.to_string(), String::new()),
        [path, ver] => (path.to_string(), ver.to_string()),
        _ => return None,
    };
    Some((
        (old_path.trim_matches('"').to_string(), old_ver.trim_matches('"').to_string()),
        (new_path.trim_matches('"').to_string(), new_ver.trim_matches('"').to_string()),
    ))
}

fn parse_module_version_pair(rest: &str) -> Option<(String, String)> {
    let mut parts = rest.split_whitespace();
    let path = parts.next()?.trim_matches('"').to_string();
    let version = parts.next()?.trim_matches('"').to_string();
    Some((path, version))
}

// ---------------------------------------------------------------------------
// go.sum parser
// ---------------------------------------------------------------------------

/// One line from a `go.sum`. `GoSum` tracks `<module> <version>/go.mod`
/// entries (integrity for the module's go.mod file); `Module` tracks
/// `<module> <version>` entries (integrity for the module zip). Only
/// `Module` entries become SBOM components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoSumKind {
    Module,
    GoMod,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoSumEntry {
    pub module: String,
    pub version: String,
    pub hash: String,
    pub kind: GoSumKind,
}

/// Parse a `go.sum` file body. Malformed lines produce `None` and are
/// skipped; valid lines return populated entries.
pub(crate) fn parse_go_sum(text: &str) -> Vec<GoSumEntry> {
    text.lines().filter_map(parse_go_sum_line).collect()
}

fn parse_go_sum_line(line: &str) -> Option<GoSumEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split_whitespace();
    let module = parts.next()?.to_string();
    let version_raw = parts.next()?.to_string();
    let hash = parts.next()?.to_string();
    let (version, kind) = if let Some(stripped) = version_raw.strip_suffix("/go.mod") {
        (stripped.to_string(), GoSumKind::GoMod)
    } else {
        (version_raw, GoSumKind::Module)
    };
    if !hash.starts_with("h1:") {
        return None;
    }
    Some(GoSumEntry {
        module,
        version,
        hash,
        kind,
    })
}

// ---------------------------------------------------------------------------
// GoModEntry → PackageDbEntry
// ---------------------------------------------------------------------------

/// Apply `replace` / `exclude` directives, then build the PURL. Returns
/// `None` when an entry is fully excluded.
fn apply_replace_and_exclude(
    module: &str,
    version: &str,
    replaces: &HashMap<(String, String), (String, String)>,
    excludes: &HashSet<(String, String)>,
) -> Option<(String, String)> {
    if excludes.contains(&(module.to_string(), version.to_string())) {
        return None;
    }
    // Prefer the exact (path, version) match; fall back to path-only
    // (versioned replace → "any version" replace).
    if let Some((new_path, new_ver)) =
        replaces.get(&(module.to_string(), version.to_string()))
    {
        let final_path = new_path.clone();
        let final_ver = if new_ver.is_empty() {
            version.to_string()
        } else {
            new_ver.clone()
        };
        // Skip replace targets that point at a local path (`./foo`,
        // `../bar`, `/abs/path`) — those aren't registry modules and
        // carry no PURL.
        if looks_like_local_path(&final_path) {
            return None;
        }
        return Some((final_path, final_ver));
    }
    if let Some((new_path, new_ver)) =
        replaces.get(&(module.to_string(), String::new()))
    {
        let final_path = new_path.clone();
        let final_ver = if new_ver.is_empty() {
            version.to_string()
        } else {
            new_ver.clone()
        };
        if looks_like_local_path(&final_path) {
            return None;
        }
        return Some((final_path, final_ver));
    }
    Some((module.to_string(), version.to_string()))
}

fn looks_like_local_path(p: &str) -> bool {
    p.starts_with("./") || p.starts_with("../") || p.starts_with('/')
}

/// Decode a go.sum `h1:<base64-sha256>` value into a `ContentHash`
/// tagged as SHA-256. The h1 prefix stands for "hash algorithm 1"
/// which is `dirhash.Hash1` — SHA-256 over a sorted newline-joined
/// manifest of per-file SHA-256 hashes (see
/// `golang.org/x/mod/sumdb/dirhash`). The value is a valid 32-byte
/// SHA-256 digest by construction, so emitting it on
/// `component.hashes` with `alg: SHA-256` is correct per CDX's
/// field semantics — the hash input is a manifest rather than a
/// tarball, but CDX doesn't constrain what was hashed.
fn h1_to_content_hash(
    h1: &str,
) -> Option<mikebom_common::types::hash::ContentHash> {
    use base64::Engine;
    use mikebom_common::types::hash::{ContentHash, HashAlgorithm};
    let b64 = h1.strip_prefix("h1:")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    ContentHash::with_algorithm(HashAlgorithm::Sha256, &hex).ok()
}

/// Build a `pkg:golang/<module>@<version>` PURL. `Purl::new` does the
/// spec-compliant encoding; module paths happen to already be lowercase
/// by convention and contain `/` which the packageurl spec treats as
/// subpath segments for `pkg:golang` specifically.
fn build_golang_purl(module: &str, version: &str) -> Option<Purl> {
    // purl-spec § Character encoding: Go versions like
    // `v1.2.3+incompatible` MUST encode `+` → `%2B`. Module path `/`
    // separators are spec-allowed and pass through unchanged via
    // encode_purl_segment.
    let s = format!(
        "pkg:golang/{}@{}",
        encode_purl_segment(module),
        encode_purl_segment(version),
    );
    Purl::new(&s).ok()
}

/// Convert a `GoModDocument` + its `go.sum` entries into `PackageDbEntry`
/// values. `source_path` is the go.sum path (used for evidence). The
/// main module (from go.mod) gets its own entry with a dep list;
/// transitive modules have their `depends` populated from the module
/// cache at `<GOMODCACHE>/cache/download/<escaped>/@v/<version>.mod`
/// when `cache` can resolve it — otherwise the transitive entry stays
/// edge-less.
// Backward-compat wrapper for the milestone 049/053 unit tests. The
// production path in `read()` uses
// `build_entries_from_go_module_with_lookup` directly with a
// `ModuleGraphMap`-backed closure (T025); this wrapper preserves the
// pre-055 cache-only behavior for the existing tests so they continue
// to verify cache-walk semantics in isolation. `#[allow(dead_code)]`
// because rustc's dead-code analysis runs on the production-binary
// compile (tests excluded) — the function IS used, just only by tests.
#[allow(dead_code)]
pub(crate) fn build_entries_from_go_module(
    doc: &GoModDocument,
    sums: &[GoSumEntry],
    source_path: &str,
    cache: &GoModCache,
) -> Vec<PackageDbEntry> {
    build_entries_from_go_module_with_lookup(
        doc,
        sums,
        source_path,
        |p, v| cache_lookup_depends(cache, p, v),
        &HashSet::new(),
    )
}

/// Like `build_entries_from_go_module`, but lets the caller supply the
/// transitive-edge lookup. Used by the post-T025 `read()` path: it
/// builds a `ModuleGraphMap` once per scan via `GraphResolver::resolve`,
/// then passes a closure that consults the map.
///
/// `lookup_depends` receives `(resolved_path, resolved_version)` AFTER
/// `apply_replace_and_exclude` and returns the ordered list of direct-
/// require module paths to attach to the entry's `depends` field.
pub(crate) fn build_entries_from_go_module_with_lookup<F>(
    doc: &GoModDocument,
    sums: &[GoSumEntry],
    source_path: &str,
    lookup_depends: F,
    gosum_fallback_paths: &HashSet<String>,
) -> Vec<PackageDbEntry>
where
    F: Fn(&str, &str) -> Vec<String>,
{
    let mut out = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();

    // Intentionally NOT emitting the project's own go.mod module as a
    // component — it's the workspace root being scanned, not a
    // dependency consumed by it. This mirrors the cargo + npm + maven
    // workspace filters (see `scan_fs/package_db/maven.rs` comment
    // block for the full rationale). The project's declared `module X`
    // path has no upstream PURL (it's what we're producing the SBOM
    // FOR), so emitting it as a dependency is a false positive and
    // also drags down sbomqs licensing because we have no license
    // source for the project itself.

    // --- Transitive modules (from go.sum) -----------------------------------
    for entry in sums {
        if entry.kind != GoSumKind::Module {
            continue;
        }
        let Some((resolved_path, resolved_version)) = apply_replace_and_exclude(
            &entry.module,
            &entry.version,
            &doc.replaces,
            &doc.excludes,
        ) else {
            continue;
        };
        let Some(purl) = build_golang_purl(&resolved_path, &resolved_version) else {
            continue;
        };
        let purl_key = purl.as_str().to_string();
        if !seen_purls.insert(purl_key) {
            continue;
        }
        // Pull the module's own go.mod from the cache (when present)
        // and extract its direct `require` entries — these are the
        // transitive edges for this node. Unresolvable lookups produce
        // an empty `depends` vec; the scan_fs resolver drops dangling
        // targets so only modules actually observed in go.sum become
        // dependsOn edges.
        let depends = lookup_depends(&resolved_path, &resolved_version);
        // Attach the module's `h1:` dirhash as a SHA-256 component
        // hash. This isn't a tarball hash — it's SHA-256 over a
        // sorted manifest of per-file hashes (see
        // `golang.org/x/mod/sumdb/dirhash`) — but the bytes ARE a
        // valid 32-byte SHA-256 and CDX's `component.hashes[]`
        // accepts any SHA-256. sbomqs's `comp_with_strong_checksums`
        // scorer counts it; humans who care about the specific
        // semantic (tarball vs dirhash) see the disambiguating tier
        // marker (`mikebom:sbom-tier = source`).
        let hashes = h1_to_content_hash(&entry.hash).into_iter().collect();
        // Milestone 091: components reached only via step 5 (the
        // go.sum flat fallback) carry a per-component provenance
        // discriminator so operators can distinguish the lower-fidelity
        // discovery path from steps 1–3. Constitution Principle X.
        let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        if gosum_fallback_paths.contains(&resolved_path) {
            extra_annotations.insert(
                "mikebom:resolver-step".to_string(),
                serde_json::Value::String("go-sum-fallback".to_string()),
            );
        }
        out.push(PackageDbEntry {
            purl,
            name: resolved_path,
            version: resolved_version,
            arch: None,
            source_path: source_path.to_string(),
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_range: None,
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
            hashes,
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations,
        });
    }

    out
}

/// Fetch a module's own go.mod from `cache` and return its direct
/// `require`-d module names. Indirect entries are included (we can't
/// tell post-hoc which of the upstream module's deps ended up in the
/// current project's build graph — better to emit the full edge set
/// and let the scan-wide dedup drop dangling targets).
///
/// Dead-code-allowed because the production `read()` path uses the
/// milestone 055 `GraphResolver`'s cache walk instead. This function
/// is preserved for the milestone 053 backward-compat wrapper above
/// (used by unit tests).
#[allow(dead_code)]
fn cache_lookup_depends(cache: &GoModCache, module: &str, version: &str) -> Vec<String> {
    let Some(text) = cache.read_mod_file(module, version) else {
        return Vec::new();
    };
    let upstream_doc = parse_go_mod(&text);
    upstream_doc
        .requires
        .into_iter()
        .map(|r| r.path)
        .collect()
}

// ---------------------------------------------------------------------------
// Milestone 057 — main-module LICENSE detection (Layer 1: SPDX header)
// ---------------------------------------------------------------------------

/// Candidate license-file basenames, in priority order. First file
/// found whose first 4 KB contains a parseable
/// `SPDX-License-Identifier:` header wins. Case-INsensitive match
/// against directory entries.
const LICENSE_FILE_CANDIDATES: &[&str] = &[
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "LICENCE",
    "LICENCE.md",
    "LICENCE.txt",
    "COPYING",
];

/// Cap on bytes read from each candidate license file. Per spec FR-001
/// — sufficient for the SPDX header (conventionally on the first line),
/// bounded against runaway reads on stray text files masquerading as
/// LICENSE.md.
const LICENSE_READ_LIMIT: usize = 4 * 1024;

/// SPDX header marker per <https://spdx.dev/specifications/>.
const SPDX_HEADER_MARKER: &str = "SPDX-License-Identifier:";

/// Layer-1 license detection: scan candidate LICENSE-style files at
/// `workspace_root` for an `SPDX-License-Identifier:` header and
/// return the canonicalized expression if found and parseable.
///
/// Returns an empty `Vec` when:
/// - no candidate file exists in the workspace root
/// - candidate files exist but contain no SPDX header in their first
///   4 KB (Layer 2 territory; deferred to follow-up)
/// - a SPDX header exists but fails to canonicalize (a `tracing::warn`
///   line is emitted with the path + raw expression for operator
///   visibility)
///
/// Never panics; never fails the scan. See
/// `specs/057-go-license-detection/spec.md` FR-001 / FR-002 / FR-003.
pub(crate) fn detect_main_module_license(workspace_root: &Path) -> Vec<SpdxExpression> {
    let entries = match std::fs::read_dir(workspace_root) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut found: HashMap<&'static str, PathBuf> = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };
        for candidate in LICENSE_FILE_CANDIDATES {
            if name_str.eq_ignore_ascii_case(candidate) {
                found.entry(candidate).or_insert_with(|| entry.path());
                break;
            }
        }
    }

    for candidate in LICENSE_FILE_CANDIDATES {
        let Some(path) = found.get(candidate) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let text = match read_first_kb(path, LICENSE_READ_LIMIT) {
            Some(t) => t,
            None => continue,
        };
        let raw = match extract_spdx_header(&text) {
            Some(r) => r,
            None => continue,
        };
        match SpdxExpression::try_canonical(raw) {
            Ok(expr) => return vec![expr],
            Err(e) => {
                tracing::warn!(
                    license_path = %path.display(),
                    raw_expression = raw,
                    error = %e,
                    "main-module LICENSE file's SPDX-License-Identifier header failed to canonicalize",
                );
                return Vec::new();
            }
        }
    }
    Vec::new()
}

fn read_first_kb(path: &Path, limit: usize) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; limit];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    let text = String::from_utf8_lossy(&buf).into_owned();
    Some(strip_bom(text))
}

fn strip_bom(s: String) -> String {
    if let Some(stripped) = s.strip_prefix('\u{feff}') {
        stripped.to_string()
    } else {
        s
    }
}

fn extract_spdx_header(text: &str) -> Option<&str> {
    let idx = text.find(SPDX_HEADER_MARKER)?;
    let after = &text[idx + SPDX_HEADER_MARKER.len()..];
    let line_end = after.find(['\n', '\r']).unwrap_or(after.len());
    let mut s = after[..line_end].trim();
    if let Some(stripped) = s.strip_suffix("-->") {
        s = stripped.trim();
    }
    if let Some(stripped) = s.strip_suffix("*/") {
        s = stripped.trim();
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Build the synthetic Go main-module `PackageDbEntry` for a workspace
/// root, per milestone 053 FR-001 + FR-001a + FR-002 + FR-004 + FR-005
/// + FR-006.
///
/// Returns `None` when the project's `go.mod` lacks a `module`
/// directive (malformed source). Returns `Some(entry)` with:
///
/// - `purl`: `pkg:golang/<module-path>@<resolve_workspace_version()>`
///   per FR-001 — the workspace's Go module path plus the
///   git-describe-resolved version (or `v0.0.0-unknown` placeholder).
/// - `name`: bare module path (e.g., `github.com/argoproj/argo-workflows`).
/// - `depends`: every direct require declared in `go.mod` (including
///   `// indirect`), after applying `replace`/`exclude` directives via
///   the existing `apply_replace_and_exclude` helper. FR-002 +
///   deliberate Trivy-divergence note for indirect requires.
/// - `parent_purl: None` — top-level qualification for SPDX
///   root-selection (case 1 / case 3 of `build_document::root_id`).
/// - `sbom_tier: Some("source")` — go.mod is the authoritative source
///   of direct requires (FR-006).
/// - `extra_annotations`: `mikebom:component-role: "main-module"`
///   per FR-004 (catalog row C40 supplementary signal layered on top
///   of the native-field placement that `metadata.rs` /
///   `packages.rs` will read).
/// - `licenses`: populated by `detect_main_module_license` (milestone
///   057, closes #103). See that function's doc for the Layer-1
///   detection contract.
///
/// `project_root` is used for the `git describe` ladder; `source_path`
/// is the project's `go.mod` location used for evidence/provenance.
pub(crate) fn build_main_module_entry(
    doc: &GoModDocument,
    project_root: &Path,
    source_path: &str,
) -> Option<PackageDbEntry> {
    let module_path = doc.module_path.as_ref()?.clone();
    let version = resolve_workspace_version(project_root);
    let purl = build_golang_purl(&module_path, &version)?;

    // Milestone 059 (closes #113 properly per reviewer feedback):
    // emit ONLY the workspace `go.mod`'s NON-`// indirect` requires
    // as direct edges from main-module. The pre-059 behavior of
    // including `// indirect` requires was milestone 053 FR-002's
    // deliberate "every go.mod-line require gets a root edge regardless
    // of indirect status" choice — it kept components reachable from
    // the SBOM root in the offline + empty-cache case but at the cost
    // of lying about the graph topology (claiming main-module
    // directly depends on testify's transitively-pulled
    // `davecgh/go-spew` etc.).
    //
    // The corrected graph: main-module → only its direct requires.
    // Indirect-marked requires reach their components transitively
    // via milestone 055's resolver (when the resolver can supply
    // transitive edges), or become orphans (Trivy-style trade-off,
    // accepted per spec Q&A). Orphan visibility comes from the
    // end-of-scan tracing summary in `read()` per FR-004.
    let depends: Vec<String> = doc
        .requires
        .iter()
        .filter(|req| !req.indirect)
        .filter_map(|req| {
            apply_replace_and_exclude(
                &req.path,
                &req.version,
                &doc.replaces,
                &doc.excludes,
            )
            .map(|(resolved_path, _)| resolved_path)
        })
        .collect();

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );

    // Milestone 057 Layer 1: detect the project's own license from a
    // LICENSE-style file at the workspace root via SPDX header scan.
    // Empty when no candidate file exists / no SPDX header found /
    // header fails to canonicalize (in the last case, a tracing::warn
    // line records the path + raw expression). Layer 2 (content-based
    // matcher) is out of scope per spec FR-004.
    let licenses = detect_main_module_license(project_root);

    Some(PackageDbEntry {
        purl,
        name: module_path,
        version,
        arch: None,
        source_path: source_path.to_string(),
        depends,
        maintainer: None,
        licenses,
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: Some(true),
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
    })
}

/// Resolve the synthetic main-module version per milestone 053 FR-001's
/// 3-step ladder:
/// 1. `git describe --tags --exact-match HEAD` — clean tagged release
///    (yields `v3.3.9` etc.)
/// 2. `git describe --tags --always` — tag-with-commits-since
///    (yields `v3.3.9-2-gabc1234`); also handles "no tags but commit
///    SHA known" by emitting the abbreviated SHA alone.
/// 3. The literal placeholder `v0.0.0-unknown` when not in a git repo,
///    when no tags or commits are reachable, when `git` is missing
///    from `$PATH`, or when the subprocess takes longer than the
///    configured timeout.
///
/// Test fixtures use tarball-style sources (no `.git` dir) so step 3
/// fires deterministically — preserves cross-host byte identity per
/// SC-007.
pub(crate) fn resolve_workspace_version(project_root: &Path) -> String {
    const PLACEHOLDER: &str = "v0.0.0-unknown";
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

    // Skip the subprocess entirely when there's no `.git` directory at
    // the workspace root — saves the spawn cost on every tarball-style
    // fixture scan and avoids touching parent-directory `.git` (which
    // would yield a tag from the *host repo* rather than the scanned
    // project, a cross-host identity bug).
    if !project_root.join(".git").exists() {
        return PLACEHOLDER.to_string();
    }

    if let Some(v) = run_git_describe_with_timeout(
        project_root,
        &["describe", "--tags", "--exact-match", "HEAD"],
        TIMEOUT,
    ) {
        return v;
    }
    if let Some(v) = run_git_describe_with_timeout(
        project_root,
        &["describe", "--tags", "--always"],
        TIMEOUT,
    ) {
        return v;
    }
    PLACEHOLDER.to_string()
}

/// Spawn `git -C <project_root> <args...>` and return Some(trimmed
/// stdout) on success, None on any failure (binary missing, non-zero
/// exit, timeout, malformed output). Stderr is silenced; stderr output
/// from `git describe` is normal and non-actionable for our flow.
fn run_git_describe_with_timeout(
    project_root: &Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(project_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Take stdout BEFORE moving the child into the worker thread, so
    // we can both read stdout (worker) and kill the child (main) if
    // the timeout elapses.
    let stdout = child.stdout.take()?;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        use std::io::Read as _;
        let mut buf = Vec::with_capacity(64);
        let mut handle = stdout;
        let _ = handle.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });

    // Wait up to `timeout` for the worker to finish reading stdout. If
    // it doesn't, kill the child and bail. Reading stdout is the
    // bottleneck for `git describe` (the actual git op is fast); a
    // hung subprocess shows up as a stalled stdout-read.
    let output_bytes = match rx.recv_timeout(timeout) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };

    // Reap the child so it doesn't become a zombie. Brief secondary
    // wait — the worker has already finished reading stdout so this
    // returns immediately on healthy children; on slow exits we accept
    // a tiny extra wait to clean up.
    let status = match child.wait() {
        Ok(s) => s,
        Err(_) => return None,
    };
    if !status.success() {
        return None;
    }

    let trimmed = String::from_utf8_lossy(&output_bytes).trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

// ---------------------------------------------------------------------------
// Public reader
// ---------------------------------------------------------------------------

/// Walk `rootfs` looking for Go project roots (any directory containing
/// both `go.mod` and `go.sum`) and convert each into SBOM entries. The
/// walk is bounded by [`MAX_PROJECT_ROOT_DEPTH`] and skips descents into
/// `vendor/`, `.git/`, `node_modules/`, `target/`, `dist/`, and
/// `__pycache__/` — the same shape the npm + pip readers use.
/// Cross-reader signals collected during Go source-tree scanning.
/// Consumed by the aggregation filters in `package_db::read_all`:
///
/// * `main_modules` — Go module paths declared as the project's own
///   `module` directive in any scanned go.mod. Feeds the G5 filter
///   (feature 007 US3): a project is never its own dependency.
/// * `production_imports` — Go module paths reachable from at least
///   one non-`_test.go` import anywhere in the scanned source tree
///   (this project's prod imports — direct only). Used as the prod
///   baseline for the milestone 049 test-vs-prod tagging.
/// * `test_only_imports` (milestone 049) — Go module paths reachable
///   from `_test.go` imports of this project but NOT from any
///   non-`_test.go` import. These deps are tagged
///   `is_dev = Some(true)` and dropped when `--include-dev` is off.
///   Source-walk only; we do not BFS through deps' go.mod `require`
///   blocks because those don't distinguish prod-vs-test requires
///   (a dep can declare testify in its go.mod purely for its own
///   tests, but downstream consumers wouldn't load it in prod).
#[derive(Debug, Default)]
pub struct GoScanSignals {
    pub main_modules: HashSet<String>,
    pub production_imports: HashSet<String>,
    pub test_only_imports: HashSet<String>,
    /// Milestone 061 (closes #119): aggregate graph completeness for
    /// the Go ecosystem. `None` ⇒ no `go.sum` entries were emitted in
    /// this scan (no Go components exist; signal not applicable).
    /// `Some(Complete)` ⇒ every `pkg:golang/...` component has at
    /// least one incoming `dependsOn`. `Some(Partial)` ⇒ one or more
    /// orphans (the per-component `mikebom:orphan-reason` annotations
    /// name the why; the doc-level reason summary lives in
    /// `graph_completeness_reasons`).
    pub graph_completeness:
        Option<crate::scan_fs::package_db::GraphCompleteness>,
    /// Sorted-deduplicated list of `<reason-class>` tokens contributing
    /// to the `Partial` completeness state. Empty when `Complete` /
    /// `None`. Ecosystem prefix (`go:`) added by the upstream
    /// `read_all` aggregator before the value flows into the document-
    /// level annotation, so this field carries just the bare class
    /// names (`unresolved-indirect-require`, `proxy-fetch-failed`, ...).
    pub graph_completeness_reasons: Vec<String>,
}

pub fn read(rootfs: &Path, _include_dev: bool) -> (Vec<PackageDbEntry>, GoScanSignals) {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    let mut signals = GoScanSignals::default();
    // Discover module cache roots once per scan — N module lookups
    // would otherwise stat the same non-existent paths repeatedly.
    let cache = GoModCache::discover(rootfs);
    if !cache.roots.is_empty() {
        tracing::debug!(
            rootfs = %rootfs.display(),
            cache_roots = cache.roots.len(),
            "Go module cache discovered",
        );
    }

    // First pass: collect every project root's (doc, sums) so we can
    // build the union of known module paths BEFORE the import-scan
    // pass. The production-import filter (G4) needs to longest-
    // prefix-match import strings against this union.
    let project_roots = candidate_project_roots(rootfs);
    let mut parsed_roots: Vec<(PathBuf, GoModDocument, Vec<GoSumEntry>)> = Vec::new();
    let mut known_modules: Vec<String> = Vec::new();
    for project_root in &project_roots {
        let go_mod_path = project_root.join("go.mod");
        let go_sum_path = project_root.join("go.sum");
        if !go_mod_path.is_file() {
            continue;
        }
        let Ok(go_mod_text) = std::fs::read_to_string(&go_mod_path) else {
            continue;
        };
        let doc = parse_go_mod(&go_mod_text);
        let sums = if go_sum_path.is_file() {
            std::fs::read_to_string(&go_sum_path)
                .map(|s| parse_go_sum(&s))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref main_path) = doc.module_path {
            signals.main_modules.insert(main_path.clone());
        }
        for req in &doc.requires {
            known_modules.push(req.path.clone());
        }
        for sum in &sums {
            if sum.kind == GoSumKind::Module {
                known_modules.push(sum.module.clone());
            }
        }
        parsed_roots.push((project_root.clone(), doc, sums));
    }
    // Longest-prefix match requires the longest path to be tried first.
    known_modules.sort_by_key(|m| std::cmp::Reverse(m.len()));
    known_modules.dedup();

    // Second pass: emit entries AND walk .go files for production +
    // test-only imports.
    let mut test_imports: HashSet<String> = HashSet::new();
    let mut main_module_emitted = 0usize;
    // Milestone 055 (T024 + T025): build the GraphResolver once per
    // scan and reuse it across project roots. The resolver's
    // 4-step ladder produces a `ModuleGraphMap` that supersedes the
    // per-entry `cache_lookup_depends()` lookup of milestone 053 —
    // edges populate from `$GOMODCACHE` (when present) AND from the
    // proxy fetch (`$GOPROXY`, default `proxy.golang.org`) when the
    // cache misses. Sync throughout (R3 deviation: the resolver lives
    // inside this sync chain). `--offline` plumbing is the T010
    // followup — for now we hard-code `false` and let `$GOPROXY=off`
    // be the user's offline knob; T024/T025 leaves a TODO marker in
    // `package_db/mod.rs::read_all` covering the flag-thread.
    use crate::scan_fs::package_db::golang::graph_resolver::{
        GraphResolver, GraphResolverConfig, WorkspaceContext,
    };
    let resolver = GraphResolver::new(GraphResolverConfig::default());

    for (project_root, doc, sums) in &parsed_roots {
        let go_sum_path = project_root.join("go.sum");
        let source_path = go_sum_path.to_string_lossy().into_owned();

        // Build the workspace context + resolve the transitive graph
        // for this project root once. The map is then consulted by the
        // per-entry closure passed into
        // `build_entries_from_go_module_with_lookup`.
        let ctx = WorkspaceContext::from_parts(
            project_root.clone(),
            doc,
            sums,
            /* offline = */ false,
        );
        let graph_map = match resolver.resolve(&ctx, &cache) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    project_root = %project_root.display(),
                    error = %e,
                    "Go transitive-edge resolver failed; falling back to empty edge set"
                );
                Default::default()
            }
        };

        // Milestone 091: collect set of paths whose entries were
        // claimed via step 5 so build_entries_from_go_module_with_lookup
        // can attach the per-component provenance discriminator.
        let gosum_fallback_set: HashSet<String> =
            graph_map.gosum_fallback_paths().into_iter().collect();

        let entries = build_entries_from_go_module_with_lookup(
            doc,
            sums,
            &source_path,
            |path, version| {
                let id = crate::scan_fs::package_db::golang::module_id::ModuleId::new(
                    path.to_string(),
                    version.to_string(),
                );
                graph_map
                    .requires(&id)
                    .iter()
                    .map(|m| m.path().to_string())
                    .collect()
            },
            &gosum_fallback_set,
        );
        for entry in entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }

        // Milestone 053 FR-001 + FR-002 + FR-004: emit a synthetic
        // main-module entry for this workspace root, with direct-
        // require edges to every go.mod-declared dependency. The
        // existing edge-emission loop in `scan_fs/mod.rs:526-547`
        // converts this entry's `depends` into `DependsOn`
        // relationships against components already present in the
        // scan, with dangling targets silently dropped.
        let go_mod_path = project_root.join("go.mod");
        let go_mod_source = go_mod_path.to_string_lossy().into_owned();
        if let Some(mut main_entry) =
            build_main_module_entry(doc, project_root, &go_mod_source)
        {
            // Milestone 091: in offline + cache-empty mode, the
            // resolver's step 5 claims every go.sum module steps 1–3
            // didn't reach, tagging them with source = GoSumFallback.
            // Augment main-module's `depends` with those module paths
            // so the SBOM includes flat root → transitive edges
            // recovering the ~110 transitive edges trivy captures from
            // go.sum content alone. Existing `// indirect`-filtered
            // direct-deps already in `main_entry.depends` are deduped
            // via the HashSet pass.
            let fallback_paths = graph_map.gosum_fallback_paths();
            if !fallback_paths.is_empty() {
                let existing: std::collections::HashSet<String> =
                    main_entry.depends.iter().cloned().collect();
                for path in fallback_paths {
                    if !existing.contains(&path) {
                        main_entry.depends.push(path);
                    }
                }
            }
            let purl_key = main_entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_entry);
                main_module_emitted += 1;
            }
        }
        // Feature 007 US2 / Milestone 049: walk .go source files for
        // prod imports (non-`_test.go`) and test imports (`_test.go`
        // only). The two sets together drive the test-vs-prod
        // classification in `package_db::mod::apply_go_production_set_filter`.
        collect_production_imports(
            project_root,
            0,
            &known_modules,
            &mut signals.production_imports,
        );
        collect_test_imports(
            project_root,
            0,
            &known_modules,
            &mut test_imports,
        );
    }

    // Test-only set: imports reachable from `_test.go` source MINUS
    // imports reachable from non-test source. Modules in the difference
    // get `is_dev = Some(true)` in the classifier.
    //
    // Milestone 049 R3 (revised): the test-only computation uses
    // direct source imports only, NOT a go.mod-`require` BFS expansion.
    // Reason: a dep's go.mod `require` block doesn't distinguish prod
    // requires from test-only requires (Go test deps live in the dep's
    // `_test.go` source, not in its go.mod). Conservative BFS through
    // go.mod requires would falsely promote test-only deps to prod
    // whenever a transitively-prod dep's go.mod also requires them
    // (e.g., logrus's go.mod requires testify because logrus's own
    // tests use it; from this project's perspective testify is still
    // test-only). Output scope is unchanged — every go.sum entry is
    // emitted (FR-001) — only the test-only TAG uses this difference.
    signals.test_only_imports = test_imports
        .difference(&signals.production_imports)
        .cloned()
        .collect();

    if !out.is_empty() {
        tracing::info!(
            rootfs = %rootfs.display(),
            modules = out.len(),
            production_imports = signals.production_imports.len(),
            test_only_imports = signals.test_only_imports.len(),
            main_modules = signals.main_modules.len(),
            main_module_components_emitted = main_module_emitted,
            "parsed Go source tree",
        );

        // Milestone 059 FR-004: orphan-visibility summary. After the
        // graph-topology fix (main-module emits ONLY non-`// indirect`
        // requires), a Go component sourced from `go.sum` is an orphan
        // when no other component references it via `depends`.
        //
        // Milestone 061 (closes #119): classify each orphan with a
        // reason and populate the per-component
        // `mikebom:orphan-reason` annotation. Aggregate the
        // completeness state into `signals` for the doc-level
        // `mikebom:graph-completeness` annotation that the format
        // emitters surface in `metadata.properties[]`.
        //
        // First pass: build the incoming-edge count over Go components
        // only (the workspace's wrapping non-Go entries don't get
        // classified — they belong to other ecosystems' completeness
        // signals which are separate doc-level concerns).
        let mut incoming_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for entry in &out {
            if entry.purl.as_str().starts_with("pkg:golang/") {
                incoming_count
                    .entry(entry.name.clone())
                    .or_insert(0);
            }
        }
        for entry in &out {
            for child_path in &entry.depends {
                if let Some(c) = incoming_count.get_mut(child_path.as_str()) {
                    *c += 1;
                }
            }
        }
        let go_component_count = incoming_count.len();
        let orphan_count = incoming_count.values().filter(|&&c| c == 0).count();
        let reachable_count = go_component_count - orphan_count;
        tracing::info!(
            rootfs = %rootfs.display(),
            go_components = go_component_count,
            reachable_via_depends_on = reachable_count,
            orphans = orphan_count,
            "Go graph reachability summary (orphans = no incoming dependsOn — expected when --offline + empty cache + indirect-only requires)",
        );

        // Second pass: classify each orphan + populate the per-
        // component `mikebom:orphan-reason` annotation. The classifier
        // is conservative — it picks `unresolved-indirect-require` as
        // the default and would refine to `private-module` /
        // `proxy-fetch-failed` if we had per-module fetch-error data
        // from the milestone 055 resolver. Threading that data
        // through is a follow-up; the default reason is operationally
        // correct for the offline + empty-cache common case (the
        // resolver's step 4 fall-through).
        let mut reason_classes: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        if orphan_count > 0 {
            for entry in out.iter_mut() {
                if !entry.purl.as_str().starts_with("pkg:golang/") {
                    continue;
                }
                // Milestone 091: skip the main-module (the project root
                // itself) — it has 0 incoming edges by construction
                // (nothing depends on the thing we're scanning), but
                // it's not a transitive orphan. Pre-091 the holistic-
                // parity asymmetry was masked because step-4 also left
                // most transitives orphaned (so CDX + SPDX both had
                // populated orphan-reason sets via components[]); post-
                // step-5, only main-module would still be tagged, and
                // CDX's main-module-via-metadata.component path doesn't
                // serialize extra_annotations — surfacing the gap.
                let is_main_module = entry
                    .extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module");
                if is_main_module {
                    continue;
                }
                let count = incoming_count.get(entry.name.as_str()).copied().unwrap_or(0);
                if count > 0 {
                    continue;
                }
                let reason = "unresolved-indirect-require".to_string();
                reason_classes.insert(reason.clone());
                entry.extra_annotations.insert(
                    "mikebom:orphan-reason".to_string(),
                    serde_json::Value::String(reason),
                );
            }
        }

        // Aggregate doc-level completeness signal. Only set when there
        // were Go components at all (signal not applicable for
        // non-Go-touching scans).
        if go_component_count > 0 {
            use crate::scan_fs::package_db::GraphCompleteness;
            signals.graph_completeness = Some(if orphan_count == 0 {
                GraphCompleteness::Complete
            } else {
                GraphCompleteness::Partial
            });
            signals.graph_completeness_reasons = reason_classes.into_iter().collect();
        }
    }
    (out, signals)
}

/// Walk a Go project root collecting production-scope imports. Skips
/// `_test.go` files (test-scope) and any directory `should_skip_descent`
/// says to skip. Thin wrapper over [`collect_imports_filtered`] with the
/// "non-test files only" predicate.
fn collect_production_imports(
    dir: &Path,
    depth: usize,
    known_modules: &[String],
    out: &mut HashSet<String>,
) {
    collect_imports_filtered(
        dir,
        depth,
        known_modules,
        out,
        FileScope::ProdOnly,
    );
}

/// Walk a Go project root collecting test-scope imports. Visits ONLY
/// `_test.go` files (the inverse predicate of [`collect_production_imports`]).
/// Milestone 049: paired with `collect_production_imports` to compute
/// the test-only set as a difference (`test_imports - prod_imports`).
fn collect_test_imports(
    dir: &Path,
    depth: usize,
    known_modules: &[String],
    out: &mut HashSet<String>,
) {
    collect_imports_filtered(
        dir,
        depth,
        known_modules,
        out,
        FileScope::TestOnly,
    );
}

/// Which `.go` files to inspect in [`collect_imports_filtered`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileScope {
    /// Skip `_test.go` files; record imports from non-test source.
    ProdOnly,
    /// Only `_test.go` files; record imports from test source.
    TestOnly,
}

/// Shared implementation for [`collect_production_imports`] and
/// [`collect_test_imports`]. The `known_modules` slice MUST be sorted
/// by length descending so the first prefix match is the longest
/// (e.g., import `github.com/foo/bar/baz` correctly attributes to
/// module `github.com/foo/bar` when both `github.com/foo` and
/// `github.com/foo/bar` are known modules).
fn collect_imports_filtered(
    dir: &Path,
    depth: usize,
    known_modules: &[String],
    out: &mut HashSet<String>,
    scope: FileScope,
) {
    if depth >= MAX_PROJECT_ROOT_DEPTH {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if should_skip_descent(&path) {
                continue;
            }
            collect_imports_filtered(&path, depth + 1, known_modules, out, scope);
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.ends_with(".go") {
            continue;
        }
        let is_test_file = name.ends_with("_test.go");
        match scope {
            FileScope::ProdOnly if is_test_file => continue,
            FileScope::TestOnly if !is_test_file => continue,
            _ => {}
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        for import_path in extract_go_imports(&bytes) {
            for module in known_modules {
                if import_path == *module
                    || import_path.starts_with(&format!("{module}/"))
                {
                    out.insert(module.clone());
                    break;
                }
            }
        }
    }
}

/// Extract every `import "…"` or grouped `import ( … )` path from a Go
/// source file. Returns the raw import path strings (e.g.,
/// `"github.com/sirupsen/logrus"`). Hand-rolled byte scanner — Go's
/// import syntax is simple enough that we don't need a full parser
/// and an external crate is overkill for "find import strings."
pub(crate) fn extract_go_imports(bytes: &[u8]) -> Vec<String> {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut remaining = text;
    while let Some(idx) = remaining.find("import") {
        let after = &remaining[idx + "import".len()..];
        // "import" must be a keyword, not part of a longer identifier.
        let before_is_boundary = idx == 0
            || matches!(
                remaining.as_bytes().get(idx.wrapping_sub(1)),
                Some(c) if !c.is_ascii_alphanumeric() && *c != b'_'
            );
        let after_is_boundary = after
            .as_bytes()
            .first()
            .map(|c| !c.is_ascii_alphanumeric() && *c != b'_')
            .unwrap_or(false);
        if !before_is_boundary || !after_is_boundary {
            let Some(next) = remaining.get(idx + 1..) else {
                break;
            };
            remaining = next;
            continue;
        }
        let trimmed = after.trim_start();
        if let Some(rest) = trimmed.strip_prefix('(') {
            // Grouped block: consume up to matching ')'.
            if let Some(end_rel) = rest.find(')') {
                let block = &rest[..end_rel];
                for line in block.lines() {
                    if let Some(path) = parse_import_line(line) {
                        out.push(path);
                    }
                }
                remaining = &rest[end_rel + 1..];
            } else {
                break;
            }
        } else if let Some(path) = parse_import_line(trimmed) {
            // Single-line import. Advance past the line.
            out.push(path);
            let Some(nl) = trimmed.find('\n') else {
                break;
            };
            remaining = &trimmed[nl + 1..];
        } else {
            let Some(next) = remaining.get(idx + 1..) else {
                break;
            };
            remaining = next;
        }
    }
    out
}

/// Parse a single import line. Handles optional alias (`foo "path"`,
/// `. "path"`, `_ "path"`) and returns just the quoted path.
fn parse_import_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("//") {
        return None;
    }
    let quote_start = line.find('"')?;
    let after = &line[quote_start + 1..];
    let quote_end = after.find('"')?;
    Some(after[..quote_end].to_string())
}

fn candidate_project_roots(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    walk_for_go_roots(rootfs, 0, &mut out, &mut visited);
    out
}

/// Milestone 054 audit (verify-only — no patch needed):
/// `walk_for_go_roots` already has the canonicalize-keyed visited
/// set + depth bound (`MAX_PROJECT_ROOT_DEPTH`) per the contract's
/// FR-001/FR-002/FR-003 invariants. This walker is the reference
/// implementation that the per-walker hardening passes patterned
/// after (rpm_file, binary, cargo, gem, go_binary, maven).
fn walk_for_go_roots(
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
) {
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        return;
    }

    if dir.join("go.mod").is_file() {
        out.push(dir.to_path_buf());
    }

    if depth >= MAX_PROJECT_ROOT_DEPTH {
        return;
    }

    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if should_skip_descent(&path) {
            continue;
        }
        walk_for_go_roots(&path, depth + 1, out, visited);
    }
}

/// Skip descent into directories that can't legitimately hold a
/// project root — dev-time residue, build outputs, and language-
/// specific vendor trees. Also skips Go's module cache
/// (`.../go/pkg/mod/...`) wherever it appears in the rootfs: the
/// cache is populated at build time by `go mod download` and
/// shouldn't contribute components to the scanned-image SBOM.
/// (This is a typical signature of a multi-stage Docker build that
/// copied the builder's cache into the image.)
fn should_skip_descent(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return true;
    };
    if name.starts_with('.') {
        return true;
    }
    if matches!(
        name,
        "vendor" | "node_modules" | "target" | "dist" | "build" | "__pycache__"
    ) {
        return true;
    }
    // Go module cache: `.../go/pkg/mod/...` anywhere in the
    // rootfs. Each cached module ships its own `go.mod`, so without
    // this skip the walker treats every cached module as a project
    // root and emits its deps as components — 21 FPs on polyglot.
    //
    // Recognize the three-component signature `.../go/pkg/mod/...`
    // via a sliding-window check over path components. Catches
    // `$HOME/go/pkg/mod`, `/root/go/pkg/mod`, `/go/pkg/mod`,
    // `/workspace/go/pkg/mod`, etc. — any layout where Go's
    // standard `GOMODCACHE` convention applies.
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    for window in components.windows(3) {
        if window == ["go", "pkg", "mod"] {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // --- go.mod parser -----------------------------------------------------

    #[test]
    fn parses_minimal_go_mod() {
        let src = "module example.com/app\n\ngo 1.22\n";
        let doc = parse_go_mod(src);
        assert_eq!(doc.module_path.as_deref(), Some("example.com/app"));
        assert_eq!(doc.go_version.as_deref(), Some("1.22"));
        assert!(doc.requires.is_empty());
    }

    #[test]
    fn parses_multi_require_block() {
        let src = r#"
module example.com/app

go 1.22

require (
    github.com/spf13/cobra v1.7.0
    github.com/sirupsen/logrus v1.9.0 // indirect
    gopkg.in/yaml.v3 v3.0.1
)
"#;
        let doc = parse_go_mod(src);
        assert_eq!(doc.requires.len(), 3);
        assert!(doc
            .requires
            .iter()
            .any(|r| r.path == "github.com/spf13/cobra" && r.version == "v1.7.0" && !r.indirect));
        assert!(doc
            .requires
            .iter()
            .any(|r| r.path == "github.com/sirupsen/logrus" && r.indirect));
    }

    #[test]
    fn parses_single_line_require() {
        let src = "module x\nrequire github.com/pkg/errors v0.9.1\n";
        let doc = parse_go_mod(src);
        assert_eq!(doc.requires.len(), 1);
        assert_eq!(doc.requires[0].path, "github.com/pkg/errors");
        assert_eq!(doc.requires[0].version, "v0.9.1");
    }

    #[test]
    fn parses_replace_directive() {
        let src = r#"
module x
replace github.com/old/lib v1.0.0 => github.com/new/lib v2.0.0
"#;
        let doc = parse_go_mod(src);
        let k = ("github.com/old/lib".to_string(), "v1.0.0".to_string());
        let v = doc.replaces.get(&k).unwrap();
        assert_eq!(v.0, "github.com/new/lib");
        assert_eq!(v.1, "v2.0.0");
    }

    #[test]
    fn parses_replace_without_old_version() {
        let src = "module x\nreplace github.com/old/lib => github.com/new/lib v2.0.0\n";
        let doc = parse_go_mod(src);
        let k = ("github.com/old/lib".to_string(), String::new());
        assert!(doc.replaces.contains_key(&k));
    }

    #[test]
    fn parses_exclude_directive() {
        let src = "module x\nexclude github.com/bad/lib v0.0.1\n";
        let doc = parse_go_mod(src);
        assert!(doc
            .excludes
            .contains(&("github.com/bad/lib".to_string(), "v0.0.1".to_string())));
    }

    #[test]
    fn line_comments_are_stripped() {
        let src = "module x // main module comment\ngo 1.22 // min version\n";
        let doc = parse_go_mod(src);
        assert_eq!(doc.module_path.as_deref(), Some("x"));
        assert_eq!(doc.go_version.as_deref(), Some("1.22"));
    }

    // --- go.sum parser -----------------------------------------------------

    #[test]
    fn parses_module_and_gomod_pair() {
        let src = "github.com/a/b v1.0.0 h1:abc=\ngithub.com/a/b v1.0.0/go.mod h1:def=\n";
        let sums = parse_go_sum(src);
        assert_eq!(sums.len(), 2);
        assert_eq!(sums[0].kind, GoSumKind::Module);
        assert_eq!(sums[1].kind, GoSumKind::GoMod);
    }

    #[test]
    fn parses_pseudo_version() {
        let src = "github.com/a/b v0.0.0-20240101000000-abcdef123456 h1:xyz=\n";
        let sums = parse_go_sum(src);
        assert_eq!(sums.len(), 1);
        assert_eq!(sums[0].version, "v0.0.0-20240101000000-abcdef123456");
    }

    #[test]
    fn malformed_go_sum_lines_are_skipped() {
        let src = "garbage\nfoo bar\ngithub.com/x/y v1.0.0 h1:ok=\n";
        let sums = parse_go_sum(src);
        assert_eq!(sums.len(), 1);
    }

    #[test]
    fn go_sum_line_without_h1_prefix_is_skipped() {
        // Some odd tools emit `sha256:` — we only trust h1:.
        let src = "github.com/x/y v1.0.0 sha256:notvalid\n";
        assert!(parse_go_sum(src).is_empty());
    }

    // --- entry construction ------------------------------------------------

    #[test]
    fn entries_exclude_workspace_root() {
        // The project's own `module X` from go.mod is NOT emitted —
        // it's the scan target, not a dependency. Mirrors cargo/npm/
        // maven workspace-root filters.
        let doc = parse_go_mod(
            "module example.com/app\ngo 1.22\nrequire github.com/x/y v1.0.0\n",
        );
        // 32-byte SHA-256, base64-encoded — 44 chars incl. one `=`
        // pad. The literal chosen doesn't correspond to any real
        // module; the decoder only validates length + base64.
        let sums = parse_go_sum(
            "github.com/x/y v1.0.0 h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n",
        );
        let entries = build_entries_from_go_module(&doc, &sums, "/p/go.sum", &GoModCache::default());
        assert_eq!(entries.len(), 1, "only the transitive dep surfaces");
        assert!(!entries.iter().any(|e| e.name == "example.com/app"));
        assert_eq!(entries[0].name, "github.com/x/y");
        assert_eq!(entries[0].sbom_tier.as_deref(), Some("source"));
    }

    #[test]
    fn h1_decode_yields_sha256_content_hash() {
        use mikebom_common::types::hash::HashAlgorithm;
        // `h1:` + base64 of 32 zero bytes = 42 `A`s plus `==` pad...
        // actually: base64(32 bytes) = ceil(32*8/6) = 44 chars with
        // one `=` pad (32 bytes = 256 bits; 256/6 = 42.67 → 43 non-
        // pad chars + 1 pad).
        let h1 = "h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let hash = h1_to_content_hash(h1).expect("valid h1 decodes");
        assert_eq!(hash.algorithm, HashAlgorithm::Sha256);
        // 32 zero bytes = 64 zero hex chars.
        assert_eq!(hash.value.as_str(), "0".repeat(64));
    }

    #[test]
    fn h1_decode_rejects_missing_prefix() {
        let bad = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        assert!(h1_to_content_hash(bad).is_none());
    }

    #[test]
    fn h1_decode_rejects_wrong_length() {
        // 16 bytes of base64 — wrong size.
        let bad = "h1:AAAAAAAAAAAAAAAAAAAAAA==";
        assert!(h1_to_content_hash(bad).is_none());
    }

    #[test]
    fn build_entries_attaches_module_hash_from_go_sum() {
        use mikebom_common::types::hash::HashAlgorithm;
        let doc = parse_go_mod(
            "module example.com/app\ngo 1.22\nrequire github.com/x/y v1.0.0\n",
        );
        let sums = parse_go_sum(
            "github.com/x/y v1.0.0 h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n",
        );
        let entries = build_entries_from_go_module(&doc, &sums, "/p/go.sum", &GoModCache::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hashes.len(), 1);
        assert_eq!(entries[0].hashes[0].algorithm, HashAlgorithm::Sha256);
    }

    #[test]
    fn gomod_kind_sum_line_produces_no_component_even_with_hash() {
        // The `<module>/go.mod` sum line carries a hash too, but it
        // hashes go.mod (not the module) — we drop the whole entry
        // upstream so no component is constructed from it.
        let doc = parse_go_mod("module x\ngo 1.22\n");
        let sums = parse_go_sum(
            "github.com/x/y v1.0.0/go.mod h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n",
        );
        let entries = build_entries_from_go_module(&doc, &sums, "/p/go.sum", &GoModCache::default());
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn replace_changes_purl() {
        let doc = parse_go_mod(
            "module x\ngo 1.22\nrequire github.com/old/lib v1.0.0\nreplace github.com/old/lib v1.0.0 => github.com/new/lib v2.0.0\n",
        );
        let sums = parse_go_sum("github.com/old/lib v1.0.0 h1:ok=\n");
        let entries = build_entries_from_go_module(&doc, &sums, "/go.sum", &GoModCache::default());
        let transitive = entries
            .iter()
            .find(|e| e.name == "github.com/new/lib")
            .expect("replacement applied");
        assert_eq!(transitive.version, "v2.0.0");
        assert_eq!(transitive.purl.as_str(), "pkg:golang/github.com/new/lib@v2.0.0");
    }

    #[test]
    fn exclude_filters_entry() {
        let doc = parse_go_mod(
            "module x\ngo 1.22\nexclude github.com/bad/lib v1.0.0\n",
        );
        let sums = parse_go_sum("github.com/bad/lib v1.0.0 h1:ok=\n");
        let entries = build_entries_from_go_module(&doc, &sums, "/go.sum", &GoModCache::default());
        assert!(entries.iter().all(|e| e.name != "github.com/bad/lib"));
    }

    #[test]
    fn replace_to_local_path_is_dropped() {
        let doc = parse_go_mod(
            "module x\ngo 1.22\nreplace github.com/old/lib v1.0.0 => ./vendor/local\n",
        );
        let sums = parse_go_sum("github.com/old/lib v1.0.0 h1:ok=\n");
        let entries = build_entries_from_go_module(&doc, &sums, "/go.sum", &GoModCache::default());
        // Only the main module should remain.
        assert!(entries.iter().all(|e| e.name != "github.com/old/lib"));
        assert!(entries.iter().all(|e| !e.name.starts_with("./")));
    }

    #[test]
    fn gomod_kind_entries_do_not_produce_components() {
        let doc = parse_go_mod("module x\ngo 1.22\n");
        let sums = parse_go_sum("github.com/x/y v1.0.0/go.mod h1:abc=\n");
        let entries = build_entries_from_go_module(&doc, &sums, "/go.sum", &GoModCache::default());
        // Workspace root (`x`) is suppressed, AND the `/go.mod` sum line
        // is `GoSumKind::GoMod` so it doesn't produce a transitive
        // component either. Net: zero entries.
        assert_eq!(entries.len(), 0);
    }

    // --- module cache walker ----------------------------------------------

    #[test]
    fn module_path_escaping_handles_capitals() {
        assert_eq!(escape_module_path("github.com/spf13/cobra"), "github.com/spf13/cobra");
        assert_eq!(
            escape_module_path("github.com/Azure/azure-sdk-for-go"),
            "github.com/!azure/azure-sdk-for-go",
        );
        assert_eq!(
            escape_module_path("github.com/ClickHouse/clickhouse-go"),
            "github.com/!click!house/clickhouse-go",
        );
        // Non-letter characters pass through unchanged.
        assert_eq!(escape_module_path("go.yaml.in/yaml/v3"), "go.yaml.in/yaml/v3");
    }

    fn write_mod_cache_entry(
        cache_root: &Path,
        module: &str,
        version: &str,
        body: &str,
    ) {
        let rel = format!(
            "cache/download/{}/@v/{}.mod",
            escape_module_path(module),
            version
        );
        let full = cache_root.join(&rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, body).unwrap();
    }

    #[test]
    fn cache_read_mod_file_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path().join("go/pkg/mod");
        write_mod_cache_entry(
            &cache_root,
            "github.com/spf13/cobra",
            "v1.10.2",
            "module github.com/spf13/cobra\ngo 1.15\nrequire github.com/spf13/pflag v1.0.9\n",
        );
        // Wire the cache root in explicitly, bypassing env discovery.
        let cache = GoModCache {
            roots: vec![cache_root.clone()],
        };
        let text = cache
            .read_mod_file("github.com/spf13/cobra", "v1.10.2")
            .expect("cached .mod file readable");
        assert!(text.contains("github.com/spf13/pflag"));
    }

    #[test]
    fn entries_pull_transitive_deps_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path().join("go/pkg/mod");
        // cobra depends on pflag in its own go.mod
        write_mod_cache_entry(
            &cache_root,
            "github.com/spf13/cobra",
            "v1.7.0",
            "module github.com/spf13/cobra\ngo 1.15\nrequire github.com/spf13/pflag v1.0.5 // indirect\n",
        );
        let doc = parse_go_mod(
            "module example.com/app\ngo 1.22\nrequire github.com/spf13/cobra v1.7.0\n",
        );
        let sums = parse_go_sum(
            "github.com/spf13/cobra v1.7.0 h1:ok=\ngithub.com/spf13/pflag v1.0.5 h1:ok=\n",
        );
        let cache = GoModCache {
            roots: vec![cache_root.clone()],
        };
        let entries = build_entries_from_go_module(&doc, &sums, "/p/go.sum", &cache);
        let cobra = entries
            .iter()
            .find(|e| e.name == "github.com/spf13/cobra")
            .expect("cobra entry present");
        assert_eq!(
            cobra.depends,
            vec!["github.com/spf13/pflag".to_string()],
            "cobra's cached go.mod declared pflag — expected edge populated",
        );
    }

    #[test]
    fn transitive_deps_empty_when_cache_missing() {
        // Same fixture as above but without any cache root registered —
        // the transitive entry should still emit with empty `depends`.
        let doc = parse_go_mod(
            "module example.com/app\ngo 1.22\nrequire github.com/spf13/cobra v1.7.0\n",
        );
        let sums = parse_go_sum("github.com/spf13/cobra v1.7.0 h1:ok=\n");
        let entries = build_entries_from_go_module(
            &doc,
            &sums,
            "/p/go.sum",
            &GoModCache::default(),
        );
        let cobra = entries
            .iter()
            .find(|e| e.name == "github.com/spf13/cobra")
            .expect("cobra entry present");
        assert!(cobra.depends.is_empty());
    }

    // --- reader ------------------------------------------------------------

    #[test]
    fn read_empty_rootfs_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let (entries, _signals) = read(dir.path(), false);
        assert!(entries.is_empty());
    }

    #[test]
    fn read_finds_nested_go_project() {
        let dir = tempfile::tempdir().unwrap();
        let svc = dir.path().join("services").join("api");
        std::fs::create_dir_all(&svc).unwrap();
        std::fs::write(
            svc.join("go.mod"),
            "module example.com/api\ngo 1.22\nrequire github.com/x/y v1.0.0\n",
        )
        .unwrap();
        std::fs::write(svc.join("go.sum"), "github.com/x/y v1.0.0 h1:ok=\n")
            .unwrap();
        let (entries, _) = read(dir.path(), false);
        // Milestone 053: the workspace root IS now emitted as a
        // synthetic main-module component (per FR-001), tagged with
        // `mikebom:component-role: main-module`. Pre-053 the
        // workspace root was suppressed entirely.
        let main = entries
            .iter()
            .find(|e| e.name == "example.com/api")
            .expect("milestone 053: workspace root must be emitted as main-module");
        assert_eq!(
            main.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
            "main-module entry must carry the C40 supplementary tag",
        );
        assert!(entries.iter().any(|e| e.name == "github.com/x/y"));
    }

    // --- Go module cache exclusion (M4) ---------------------------------

    fn write_go_project(root: &Path, module: &str, deps: &[(&str, &str)]) {
        std::fs::create_dir_all(root).unwrap();
        let mut go_mod = format!("module {module}\ngo 1.22\n");
        if !deps.is_empty() {
            go_mod.push_str("require (\n");
            for (path, version) in deps {
                go_mod.push_str(&format!("    {path} {version}\n"));
            }
            go_mod.push_str(")\n");
        }
        std::fs::write(root.join("go.mod"), go_mod).unwrap();
        let mut go_sum = String::new();
        for (path, version) in deps {
            go_sum.push_str(&format!("{path} {version} h1:fake=\n"));
        }
        std::fs::write(root.join("go.sum"), go_sum).unwrap();
    }

    #[test]
    fn walker_skips_root_go_pkg_mod_trees() {
        // Multi-stage Docker build pattern: build-stage `go mod
        // download` populates `/root/go/pkg/mod/`, which then gets
        // carried into the final image. Each cached module has its
        // own `go.mod` — the walker must NOT treat them as project
        // roots.
        let dir = tempfile::tempdir().unwrap();
        let cache =
            dir.path().join("root/go/pkg/mod/github.com/foo/bar@v1.0.0");
        write_go_project(&cache, "github.com/foo/bar", &[("github.com/x/y", "v2.0.0")]);
        let (entries, _) = read(dir.path(), false);
        assert!(
            entries.is_empty(),
            "walker must skip /root/go/pkg/mod cache tree: {entries:?}",
        );
    }

    #[test]
    fn walker_skips_home_user_go_pkg_mod() {
        let dir = tempfile::tempdir().unwrap();
        let cache =
            dir.path().join("home/alice/go/pkg/mod/github.com/foo/bar@v1.0.0");
        write_go_project(&cache, "github.com/foo/bar", &[("github.com/x/y", "v2.0.0")]);
        let (entries, _) = read(dir.path(), false);
        assert!(
            entries.is_empty(),
            "walker must skip $HOME/go/pkg/mod cache tree: {entries:?}",
        );
    }

    #[test]
    fn walker_still_finds_legitimate_project_roots() {
        // Control: a real project at `/app/go.mod` + `/app/go.sum`
        // still emits normally after M4.
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        write_go_project(&app, "example.com/app", &[("github.com/real/dep", "v1.2.3")]);
        let (entries, _) = read(dir.path(), false);
        assert!(
            entries.iter().any(|e| e.name == "github.com/real/dep"),
            "legitimate project root must still emit: {entries:?}",
        );
    }

    #[test]
    fn walker_skips_gopath_outside_standard_paths() {
        // Non-standard `GOPATH` layout — still matches the
        // `.../go/pkg/mod/...` path-component signature.
        let dir = tempfile::tempdir().unwrap();
        let cache = dir
            .path()
            .join("workspace/go/pkg/mod/github.com/foo/bar@v1.0.0");
        write_go_project(&cache, "github.com/foo/bar", &[("github.com/x/y", "v2.0.0")]);
        let (entries, _) = read(dir.path(), false);
        assert!(
            entries.is_empty(),
            "walker must skip /workspace/go/pkg/mod cache tree: {entries:?}",
        );
    }

    // ---------------------------------------------------------------
    // Milestone 049 — collect_test_imports + compute_transitive_prod_set
    // ---------------------------------------------------------------

    #[test]
    fn collect_test_imports_records_only_test_files() {
        let dir = tempfile::tempdir().unwrap();
        // Project source: main.go (prod) + main_test.go (test).
        std::fs::write(
            dir.path().join("main.go"),
            br#"package main
import "github.com/prod/lib"
func main() { _ = lib.Something() }"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main_test.go"),
            br#"package main
import "github.com/test/lib"
func TestSomething(t *testing.T) { _ = lib.Something() }"#,
        )
        .unwrap();

        let known_modules = vec![
            "github.com/prod/lib".to_string(),
            "github.com/test/lib".to_string(),
        ];
        let mut prod = HashSet::new();
        let mut test = HashSet::new();
        collect_production_imports(dir.path(), 0, &known_modules, &mut prod);
        collect_test_imports(dir.path(), 0, &known_modules, &mut test);

        assert_eq!(prod.len(), 1);
        assert!(prod.contains("github.com/prod/lib"));
        assert!(!prod.contains("github.com/test/lib"));

        assert_eq!(test.len(), 1);
        assert!(test.contains("github.com/test/lib"));
        assert!(!test.contains("github.com/prod/lib"));
    }

    #[test]
    fn collect_test_imports_records_modules_imported_from_both() {
        // A module imported by BOTH prod and test code appears in
        // BOTH sets. The classifier later subtracts prod from test
        // to compute test-only.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("main.go"),
            br#"package main
import "github.com/shared/lib"
func main() { _ = lib.X() }"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main_test.go"),
            br#"package main
import "github.com/shared/lib"
func TestX(t *testing.T) { _ = lib.X() }"#,
        )
        .unwrap();
        let known_modules = vec!["github.com/shared/lib".to_string()];
        let mut prod = HashSet::new();
        let mut test = HashSet::new();
        collect_production_imports(dir.path(), 0, &known_modules, &mut prod);
        collect_test_imports(dir.path(), 0, &known_modules, &mut test);
        assert_eq!(prod, test);
        // Per spec: a module reachable from both prod and test is
        // classified as prod (test_only = test - prod = empty).
        let test_only: HashSet<String> = test.difference(&prod).cloned().collect();
        assert!(test_only.is_empty());
    }

    // --- Milestone 053 FR-001: workspace version-resolution ladder ---------

    #[test]
    fn resolve_workspace_version_no_git_dir_returns_placeholder() {
        // No `.git` → step 3 fires deterministically. Locks the
        // tarball-style fixture invariant for SC-007 (cross-host byte
        // identity).
        let tmp = tempfile::tempdir().expect("tempdir");
        let v = resolve_workspace_version(tmp.path());
        assert_eq!(v, "v0.0.0-unknown");
    }

    #[test]
    fn resolve_workspace_version_git_repo_no_tags_falls_through_steps_to_sha() {
        // Step 1 (--exact-match) fails (no tag at HEAD). Step 2
        // (--always) succeeds and returns the abbreviated commit
        // SHA. Verifies the ladder progresses past step 1.
        let tmp = tempfile::tempdir().expect("tempdir");
        // Initialize a git repo with one commit; no tags.
        if !run_git_init_with_commit(tmp.path()) {
            // Skip if `git` isn't available on $PATH (unusual in CI).
            return;
        }
        let v = resolve_workspace_version(tmp.path());
        assert_ne!(v, "v0.0.0-unknown", "step 2 (--always) should win");
        // `--always` returns 7-char abbreviated SHA when no tag is
        // reachable. Hex chars only, length-7 (default).
        assert!(
            v.len() == 7 && v.chars().all(|c| c.is_ascii_hexdigit()),
            "expected 7-char abbreviated SHA, got: {v:?}",
        );
    }

    #[test]
    fn resolve_workspace_version_git_repo_with_exact_tag_returns_tag() {
        // Step 1 succeeds when HEAD points at an annotated tag. The
        // ladder MUST stop at step 1 (don't fall through to --always).
        let tmp = tempfile::tempdir().expect("tempdir");
        if !run_git_init_with_commit(tmp.path()) {
            return;
        }
        if !run_git_tag(tmp.path(), "v0.5.0-test") {
            return;
        }
        let v = resolve_workspace_version(tmp.path());
        assert_eq!(v, "v0.5.0-test");
    }

    /// Run `git init && git commit --allow-empty -m initial`. Returns
    /// false when `git` isn't on PATH (test should noop on those hosts
    /// rather than fail). Configures author + committer locally so the
    /// commit succeeds even when the test runner has no global git
    /// identity. Helper for the ladder tests above.
    fn run_git_init_with_commit(dir: &std::path::Path) -> bool {
        use std::process::{Command, Stdio};
        let init_ok = Command::new("git")
            .arg("init")
            .arg("--initial-branch=main")
            .arg("-q")
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !init_ok {
            return false;
        }
        let _ = Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.name", "Test Runner"])
            .current_dir(dir)
            .status();
        Command::new("git")
            .args(["commit", "--allow-empty", "-q", "-m", "initial"])
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn run_git_tag(dir: &std::path::Path, name: &str) -> bool {
        use std::process::{Command, Stdio};
        Command::new("git")
            .args(["tag", name])
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    // --- Milestone 053 FR-001/FR-002/FR-004: build_main_module_entry ------

    fn make_doc(src: &str) -> GoModDocument {
        parse_go_mod(src)
    }

    #[test]
    fn build_main_module_entry_three_requires_produces_three_depends() {
        let doc = make_doc(
            "module example.com/x\n\
             go 1.22\n\
             require (\n\
                 a.example.com/r1 v1.0.0\n\
                 b.example.com/r2 v2.0.0\n\
                 c.example.com/r3 v3.0.0\n\
             )\n",
        );
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = build_main_module_entry(&doc, tmp.path(), "/p/go.mod")
            .expect("main-module entry constructed");
        assert_eq!(entry.name, "example.com/x");
        assert_eq!(entry.depends.len(), 3);
        assert!(entry.depends.contains(&"a.example.com/r1".to_string()));
        assert!(entry.depends.contains(&"b.example.com/r2".to_string()));
        assert!(entry.depends.contains(&"c.example.com/r3".to_string()));
    }

    #[test]
    fn build_main_module_entry_excludes_indirect_requires() {
        // Milestone 059 (closes #113 properly per reviewer feedback):
        // INVERTED from the original 053 spec FR-002 behavior. The
        // pre-059 build_main_module_entry deliberately included
        // `// indirect` requires as direct edges from main-module
        // ("for offline-scan simplicity") — that lied about the
        // graph topology. Post-059, only NON-indirect requires
        // become direct edges from main-module; indirect components
        // are reached via the milestone 055 transitive resolver
        // (when it can supply edges) or become orphans (Trivy-style
        // trade-off).
        let doc = make_doc(
            "module example.com/x\n\
             go 1.22\n\
             require (\n\
                 a.example.com/direct v1.0.0\n\
                 b.example.com/indirect v2.0.0 // indirect\n\
             )\n",
        );
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = build_main_module_entry(&doc, tmp.path(), "/p/go.mod")
            .expect("main-module entry constructed");
        assert_eq!(entry.depends.len(), 1, "only the non-indirect require makes a direct edge");
        assert!(entry.depends.contains(&"a.example.com/direct".to_string()));
        assert!(
            !entry.depends.contains(&"b.example.com/indirect".to_string()),
            "indirect requires MUST NOT appear as direct edges from main-module post-059",
        );
    }

    #[test]
    fn build_main_module_entry_applies_replace_directive() {
        let doc = make_doc(
            "module example.com/x\n\
             go 1.22\n\
             require y.example.com/orig v1.0.0\n\
             replace y.example.com/orig => z.example.com/replaced v1.0.0\n",
        );
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = build_main_module_entry(&doc, tmp.path(), "/p/go.mod")
            .expect("main-module entry constructed");
        assert_eq!(entry.depends.len(), 1);
        assert!(
            entry.depends.contains(&"z.example.com/replaced".to_string()),
            "replace should rewrite the target: {:?}",
            entry.depends
        );
    }

    #[test]
    fn build_main_module_entry_applies_exclude_directive() {
        let doc = make_doc(
            "module example.com/x\n\
             go 1.22\n\
             require z.example.com/dropme v1.0.0\n\
             exclude z.example.com/dropme v1.0.0\n",
        );
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = build_main_module_entry(&doc, tmp.path(), "/p/go.mod")
            .expect("main-module entry constructed");
        assert!(
            entry.depends.is_empty(),
            "exclude should drop the require: {:?}",
            entry.depends
        );
    }

    #[test]
    fn build_main_module_entry_zero_requires_emits_with_empty_depends() {
        let doc = make_doc("module example.com/empty\ngo 1.22\n");
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = build_main_module_entry(&doc, tmp.path(), "/p/go.mod")
            .expect("main-module entry constructed");
        assert_eq!(entry.name, "example.com/empty");
        assert!(entry.depends.is_empty());
    }

    #[test]
    fn build_main_module_entry_has_top_level_shape_and_supplementary_c40_tag() {
        let doc = make_doc("module example.com/x\ngo 1.22\n");
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = build_main_module_entry(&doc, tmp.path(), "/p/go.mod")
            .expect("main-module entry constructed");
        // FR-001a precondition: parent_purl=None so SPDX root-selection
        // picks this as a top-level component.
        assert!(entry.parent_purl.is_none());
        // FR-006: source-tier (go.mod is the authoritative source).
        assert_eq!(entry.sbom_tier.as_deref(), Some("source"));
        // FR-005: empty licenses (LICENSE detection deferred to #103).
        assert!(entry.licenses.is_empty());
        // FR-004: supplementary C40 annotation present with value
        // "main-module".
        let role = entry
            .extra_annotations
            .get("mikebom:component-role")
            .and_then(|v| v.as_str());
        assert_eq!(role, Some("main-module"));
        // PURL shape per FR-001 + tarball-style fixture (no .git) →
        // step-3 placeholder version.
        assert_eq!(
            entry.purl.as_str(),
            "pkg:golang/example.com/x@v0.0.0-unknown"
        );
    }

    #[test]
    fn build_main_module_entry_returns_none_on_missing_module_directive() {
        let doc = make_doc("// just a comment, no module directive\n");
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(build_main_module_entry(&doc, tmp.path(), "/p/go.mod").is_none());
    }

    // --- Milestone 057: main-module LICENSE detection (Layer 1) -----------

    #[test]
    fn detect_license_returns_empty_when_no_candidate_files() {
        // SC-002 (regression-baseline): a workspace with no LICENSE-style
        // files at the root produces empty `licenses`. Critically, this
        // is the case for the existing `tests/fixtures/go/simple-module/`
        // and `argo-style-no-cache/argo-workflows/` fixtures, so their
        // goldens stay byte-identical post-057.
        let dir = tempfile::tempdir().unwrap();
        assert!(detect_main_module_license(dir.path()).is_empty());
    }

    #[test]
    fn detect_license_extracts_apache_2_0_from_license_file() {
        // SC-001: canonical Apache-2.0 SPDX header on the first line.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE"),
            "SPDX-License-Identifier: Apache-2.0\n\nApache License, Version 2.0...\n",
        )
        .unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0].as_str(), "Apache-2.0");
    }

    #[test]
    fn detect_license_extracts_compound_expression() {
        // AS#2: dual-licensed (`MIT OR Apache-2.0`) canonicalizes
        // through `try_canonical`.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE.md"),
            "# License\n\nSPDX-License-Identifier: MIT OR Apache-2.0\n",
        )
        .unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        // Canonical form may insert spaces / re-order; assert on the
        // round-trip rather than literal string equality.
        let canonical = licenses[0].as_str();
        assert!(
            canonical.contains("MIT") && canonical.contains("Apache-2.0"),
            "canonicalized expression should retain both license IDs: {canonical}",
        );
    }

    #[test]
    fn detect_license_priority_license_beats_license_md() {
        // Multiple candidate files in same workspace — `LICENSE` wins
        // over `LICENSE.md` per priority order in
        // LICENSE_FILE_CANDIDATES.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE"),
            "SPDX-License-Identifier: Apache-2.0\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("LICENSE.md"),
            "SPDX-License-Identifier: MIT\n",
        )
        .unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0].as_str(), "Apache-2.0");
    }

    #[test]
    fn detect_license_case_insensitive_filename() {
        // `license` (lowercase) matches `LICENSE` candidate via
        // eq_ignore_ascii_case.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("license"),
            "SPDX-License-Identifier: BSD-3-Clause\n",
        )
        .unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0].as_str(), "BSD-3-Clause");
    }

    #[test]
    fn detect_license_returns_empty_when_no_spdx_header() {
        // AS#4: Layer-1 miss when LICENSE has no SPDX header. Layer 2
        // territory; we deliberately don't guess.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE"),
            "Apache License\nVersion 2.0, January 2004\n",
        )
        .unwrap();
        assert!(detect_main_module_license(dir.path()).is_empty());
    }

    #[test]
    fn detect_license_returns_empty_for_unparseable_header() {
        // AS#5 / SC-003: malformed SPDX expression. Tracing-warn
        // visibility is not asserted here (would require a captured
        // tracing subscriber); the empty-return contract is sufficient
        // to verify the FR-002 behavior.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE"),
            "SPDX-License-Identifier: NotARealLicenseExpression!!!\n",
        )
        .unwrap();
        assert!(detect_main_module_license(dir.path()).is_empty());
    }

    #[test]
    fn detect_license_strips_html_comment_trailer() {
        // Edge case: SPDX header inside an HTML comment, common in
        // README.md-style LICENSE files.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE.md"),
            "<!-- SPDX-License-Identifier: MIT -->\n\nMIT License\n",
        )
        .unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0].as_str(), "MIT");
    }

    #[test]
    fn detect_license_strips_bom() {
        // Edge case: UTF-8 BOM at file start. Some Windows-authored
        // LICENSE.md files have one.
        let dir = tempfile::tempdir().unwrap();
        let mut bytes: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"SPDX-License-Identifier: ISC\n");
        std::fs::write(dir.path().join("LICENSE"), &bytes).unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0].as_str(), "ISC");
    }

    #[test]
    fn detect_license_skips_directory_named_license() {
        // Edge case: some projects ship a `LICENSE/` directory of
        // per-vendor license files. The detector should skip it via
        // is_file() rather than panicking on the directory.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("LICENSE")).unwrap();
        // Add a sibling COPYING file so the priority list still has
        // something to pick up.
        std::fs::write(
            dir.path().join("COPYING"),
            "SPDX-License-Identifier: GPL-2.0-only\n",
        )
        .unwrap();
        let licenses = detect_main_module_license(dir.path());
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0].as_str(), "GPL-2.0-only");
    }

    #[test]
    fn detect_license_caps_read_at_4kb() {
        // SPDX header buried >4 KB into the file → Layer 1 miss.
        let dir = tempfile::tempdir().unwrap();
        let mut content = "x".repeat(LICENSE_READ_LIMIT + 100);
        content.push_str("\nSPDX-License-Identifier: MIT\n");
        std::fs::write(dir.path().join("LICENSE"), content).unwrap();
        assert!(detect_main_module_license(dir.path()).is_empty());
    }

    #[test]
    fn build_main_module_entry_populates_license_when_license_file_has_spdx_header() {
        // SC-001 end-to-end via build_main_module_entry: the entry's
        // `licenses` field contains the canonical Apache-2.0 expression.
        let doc = make_doc("module example.com/with-license\n");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("LICENSE"),
            "SPDX-License-Identifier: Apache-2.0\n",
        )
        .unwrap();
        let entry = build_main_module_entry(&doc, dir.path(), "/p/go.mod")
            .expect("entry built");
        assert_eq!(entry.licenses.len(), 1);
        assert_eq!(entry.licenses[0].as_str(), "Apache-2.0");
    }

    #[test]
    fn build_main_module_entry_empty_licenses_when_no_license_file() {
        // FR-005: pre-057 behavior preserved when no LICENSE file
        // present. This is the regression-baseline that keeps the
        // existing simple-module / argo-style-no-cache fixtures'
        // goldens byte-identical.
        let doc = make_doc("module example.com/no-license\n");
        let dir = tempfile::tempdir().unwrap();
        let entry = build_main_module_entry(&doc, dir.path(), "/p/go.mod")
            .expect("entry built");
        assert!(entry.licenses.is_empty());
    }
}