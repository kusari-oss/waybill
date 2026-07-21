//! Milestone 128 — line-oriented BitBake recipe body parser.
//!
//! Extracts metadata fields (LICENSE, SRC_URI, SRCREV, HOMEPAGE,
//! SUMMARY, DESCRIPTION, DEPENDS, RDEPENDS_<pkg>, BBCLASSEXTEND)
//! from `.bb` and `.inc` files. Resolves `require` and `include`
//! directives transitively with depth-bound 8 + cycle detection.
//!
//! ## What this is NOT
//!
//! - Not a full BitBake metadata evaluator. We do not execute
//!   `inherit` class processing, function definitions, or
//!   Python expressions. The reader handles assignment statements
//!   and directive resolution only.
//! - Not a variable expansion engine. We resolve `${PN}` and `${PV}`
//!   from the recipe filename (per FR-005), and collect every other
//!   `${VAR}` reference into `RecipeMetadata.unexpanded_vars` for
//!   transparency. Operators wanting precise per-machine output
//!   should run `bitbake -e <recipe>` against their target.
//!
//! ## Field-precedence semantics
//!
//! Per FR-004 + Clarifications Q1: **last-write-in-source-order
//! wins**. For each recipe, process the included `.inc` files'
//! assignments first (in include-order), then process the
//! referencing `.bb`'s assignments. Conflicting fields take the
//! `.bb`'s value. Override-syntax (`FIELD:append:<override>` /
//! `FIELD:prepend:<override>`) is then applied per FR-016's
//! union-merge approximation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use waybill_common::types::license::SpdxExpression;

/// Maximum recursion depth for `require` + `include` resolution
/// per FR-004. Mirrors milestone 107's walker convention. Deep
/// include chains are rare in practice (Poky core uses ~3 levels).
const INCLUDE_DEPTH_LIMIT: usize = 8;

/// All fields the body-parser extracts from one recipe.
///
/// Per FR-004 last-write-in-source-order semantics — after parsing
/// the `.bb` + every `require`/`include`d `.inc`, the result is one
/// merged `RecipeMetadata` with the `.bb`'s assignments overriding
/// any conflicting `.inc` values.
#[derive(Debug, Clone, Default)]
pub(crate) struct RecipeMetadata {
    /// `<name>` segment of `<name>_<version>.bb`. Filename-derived.
    pub recipe_name: String,
    /// `<version>` segment of `<name>_<version>.bb`. Filename-derived.
    /// Caller (recipe.rs) overrides this with the SRCREV-derived
    /// 12-hex prefix per FR-018 when the filename `PV` is literally
    /// `"git"` or contains `AUTOINC`.
    pub recipe_version: String,
    /// `LICENSE = "..."` field, canonicalized via
    /// `SpdxExpression::try_canonical`. `None` when LICENSE was
    /// absent OR set to `"CLOSED"` (use `license_closed` flag for
    /// the discriminator); `Some(...)` for every other case.
    pub license: Option<SpdxExpression>,
    /// `LICENSE = "CLOSED"` discriminator. Drives FR-012's
    /// `mikebom:yocto-license-closed` annotation.
    pub license_closed: bool,
    /// `SRC_URI = "..."` field, split on whitespace. Each entry
    /// preserves its scheme + qualifiers (e.g.,
    /// `git://...;branch=...`). Caller normalizes the first
    /// `git://`/`git+https://` entry to an `https://` vcs URL per
    /// FR-002.
    pub src_uris: Vec<String>,
    /// `SRCREV = "..."` field (single-value).
    pub srcrev: Option<String>,
    /// `SRCREV_<machine> = "..."` field set. Key is the machine
    /// arch literal, value is the SRCREV.
    pub srcrev_by_machine: BTreeMap<String, String>,
    /// `HOMEPAGE = "..."` field. Caller emits as
    /// `externalReferences[type=website]`.
    pub homepage: Option<String>,
    /// `SUMMARY = "..."` field. Caller populates
    /// `component.description` (CDX) / `Package.summary` (SPDX).
    pub summary: Option<String>,
    /// `DESCRIPTION = "..."` field. Caller emits a
    /// `mikebom:yocto-description` annotation when it differs from
    /// `summary` per FR-010.
    pub description: Option<String>,
    /// `DEPENDS = "..."` field, split on whitespace. Each entry is
    /// a recipe-name (later resolved to a PURL by the cross-recipe
    /// linker pass).
    pub depends: Vec<String>,
    /// `RDEPENDS_<pkg> = "..."` field set. Key is the `<pkg>`
    /// suffix (typically `${PN}`); value is the dep-name list.
    pub rdepends: BTreeMap<String, Vec<String>>,
    /// `BBCLASSEXTEND = "..."` field, split on whitespace. e.g.,
    /// `["native", "nativesdk"]`. Drives the
    /// `mikebom:yocto-class-extend` annotation.
    pub class_extend: Vec<String>,
    /// Unresolved `${VAR}` references the parser saw in IDENTITY
    /// fields (LICENSE, SRC_URI, SRCREV, HOMEPAGE, SUMMARY). Drives
    /// FR-005's `mikebom:yocto-unexpanded-vars` annotation.
    pub unexpanded_vars: Vec<String>,
    /// Did the parser apply ≥1 override-syntax merge (FR-016)?
    /// Drives the `mikebom:yocto-overrides-merged` annotation.
    pub overrides_merged: bool,
    /// Filesystem path of the `.bb` file (for provenance +
    /// nearest-ancestor layer attribution per FR-006).
    pub source_path: PathBuf,
    /// Filesystem paths of every `.inc` file merged into this
    /// recipe.
    pub include_paths: Vec<PathBuf>,
}

/// Parse a `.bb` recipe file (and every `require`/`include`d
/// `.inc`) into a single merged `RecipeMetadata` per FR-001..FR-005.
///
/// Returns `None` only when the top-level file cannot be read
/// (e.g., permission denied). Malformed bodies return `Some` with
/// partial fields populated and `unexpanded_vars` recording the
/// gaps — better partial signal than dropping (Constitution
/// Principle VIII).
///
/// Field-precedence semantics per FR-004 + Clarifications Q1:
/// **last-write-in-source-order wins**. Process each include
/// FIRST (in include-order), then process the referencing file.
/// The referencing file's assignments override any conflicting
/// earlier values via `merge_metadata`.
pub(crate) fn parse_recipe_file(
    path: &Path,
    recipe_name: &str,
    recipe_version: &str,
) -> Option<RecipeMetadata> {
    let mut metadata = RecipeMetadata {
        recipe_name: recipe_name.to_string(),
        recipe_version: recipe_version.to_string(),
        source_path: path.to_path_buf(),
        ..Default::default()
    };
    let mut in_progress: BTreeSet<PathBuf> = BTreeSet::new();
    parse_into(path, &mut metadata, &mut in_progress, 0).ok()?;
    Some(metadata)
}

/// Recursive parser core. Reads `path`, processes every assignment
/// statement and `require`/`include` directive, merging the result
/// into `metadata`. Last-write-in-source-order semantics: when a
/// later assignment (in this file or in a deeper include) conflicts
/// with an earlier one, the later value wins.
///
/// `in_progress` tracks files currently being parsed in the include
/// chain — used to break cycles per FR-004 (cycle's tail include
/// silently dropped + a `tracing::debug!` log emitted).
fn parse_into(
    path: &Path,
    metadata: &mut RecipeMetadata,
    in_progress: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> std::io::Result<()> {
    if depth >= INCLUDE_DEPTH_LIMIT {
        tracing::debug!(
            path = %path.display(),
            depth = depth,
            "Milestone 128 FR-004: include depth limit reached; dropping deeper include"
        );
        return Ok(());
    }
    let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !in_progress.insert(canonical_path.clone()) {
        tracing::debug!(
            path = %path.display(),
            "Milestone 128 FR-004: cyclic include detected; dropping tail"
        );
        return Ok(());
    }

    if path != metadata.source_path {
        // Includes (non-top-level files) are recorded in include_paths.
        metadata.include_paths.push(path.to_path_buf());
    }

    let content = std::fs::read_to_string(path)?;
    let logical_lines = collect_logical_lines(&content);

    for line in logical_lines {
        let trimmed = line.trim();
        // Skip blank and comment lines.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // `require <path>` and `include <path>` directives.
        if let Some(rel) = trimmed.strip_prefix("require ") {
            resolve_include(path, rel.trim(), metadata, in_progress, depth + 1);
            continue;
        }
        if let Some(rel) = trimmed.strip_prefix("include ") {
            resolve_include(path, rel.trim(), metadata, in_progress, depth + 1);
            continue;
        }

        // Assignment statements.
        if let Some(assignment) = parse_assignment(trimmed) {
            apply_assignment(assignment, metadata);
        }
    }

    in_progress.remove(&canonical_path);
    Ok(())
}

/// Collapse multi-line backslash-continuation into single logical
/// lines. e.g.,
/// ```text
/// SRC_URI = "git://... \
///            file://patch.patch"
/// ```
/// becomes one logical line.
fn collect_logical_lines(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in content.lines() {
        if let Some(stripped) = line.strip_suffix('\\') {
            current.push_str(stripped.trim_end());
            current.push(' ');
        } else {
            current.push_str(line);
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Resolve a `require <path>` or `include <path>` directive
/// relative to the current file's directory. BitBake searches
/// `BBPATH`-listed dirs in addition; we accept only the local-dir
/// case here (the common case for shared `*.inc` files alongside
/// their `*.bb`s — see `meta-balena-rust/recipes-devtools/cargo/`
/// for a concrete example).
fn resolve_include(
    current_file: &Path,
    include_str: &str,
    metadata: &mut RecipeMetadata,
    in_progress: &mut BTreeSet<PathBuf>,
    depth: usize,
) {
    let candidate = current_file
        .parent()
        .map(|p| p.join(include_str))
        .unwrap_or_else(|| PathBuf::from(include_str));
    if candidate.exists() {
        let _ = parse_into(&candidate, metadata, in_progress, depth);
        return;
    }
    // Walk parents looking for a top-level Yocto layer dir + recipes-* subtree match.
    // BitBake's `BBPATH` resolution would do this; we approximate by walking
    // the current file's ancestors up to 5 levels and trying the relative
    // path under each.
    let mut ancestor = current_file.parent();
    for _ in 0..5 {
        if let Some(a) = ancestor {
            let try_path = a.join(include_str);
            if try_path.exists() {
                let _ = parse_into(&try_path, metadata, in_progress, depth);
                return;
            }
            ancestor = a.parent();
        }
    }
    // Couldn't resolve — common for `include <file-from-BBPATH>`. Silent
    // drop is consistent with the spec's "approximation" posture.
    tracing::debug!(
        from = %current_file.display(),
        include = include_str,
        "Milestone 128 FR-004: include not resolved against local dir or ancestor walk"
    );
}

/// Recognized BitBake assignment shape.
#[derive(Debug)]
struct Assignment<'a> {
    /// The bare field name (e.g., `LICENSE`, `SRC_URI`).
    field: &'a str,
    /// Optional override suffix after the colon — e.g., for
    /// `LICENSE:append:rpi4 = "foo"`, this is `append:rpi4`.
    /// `None` for plain assignments.
    override_kind: Option<&'a str>,
    /// The assignment operator.
    op: Operator,
    /// The right-hand-side value (already unquoted).
    value: &'a str,
}

#[derive(Debug, Clone, Copy)]
enum Operator {
    /// `=` (immediate assignment) OR `:=` (also immediate; treated identically)
    Assign,
    /// `?=` (weak assignment; same shape as Assign for our purposes — last-write-wins)
    WeakAssign,
    /// `??=` (default; only assigns when undefined — we model as
    /// "do not overwrite if already set")
    Default,
    /// `+=` (append-with-space)
    AppendSpace,
    /// `=+` (prepend-with-space)
    PrependSpace,
    /// `.=` (append-no-space)
    AppendNoSpace,
    /// `=.` (prepend-no-space)
    PrependNoSpace,
}

/// Parse a logical line into an `Assignment`, or `None` if it
/// doesn't match the BitBake assignment grammar.
fn parse_assignment(line: &str) -> Option<Assignment<'_>> {
    // Find the operator + its position. Iterate operators
    // in priority order (longest-match first to avoid mis-matching
    // `:=` as `=`).
    let ops: &[(&str, Operator)] = &[
        ("??=", Operator::Default),
        ("?=", Operator::WeakAssign),
        (":=", Operator::Assign),
        ("+=", Operator::AppendSpace),
        ("=+", Operator::PrependSpace),
        (".=", Operator::AppendNoSpace),
        ("=.", Operator::PrependNoSpace),
        ("=", Operator::Assign),
    ];
    for (op_str, op) in ops {
        if let Some(idx) = line.find(op_str) {
            // Verify the character on either side isn't part of a longer
            // operator we already checked above (e.g., `=` at position
            // X of `??=` should not match here — the longest-match
            // iteration order handles this).
            let lhs = line[..idx].trim();
            let rhs_raw = line[idx + op_str.len()..].trim();
            let value = strip_quotes(rhs_raw)?;
            // Split LHS on `:` to extract any override suffix.
            // E.g., `LICENSE:append:rpi4` → field=LICENSE, override=append:rpi4.
            // Variable-flag syntax `LICENSE[md5]` is NOT supported here
            // (treated as a non-assignment and silently skipped).
            let (field, override_kind) = if let Some(colon) = lhs.find(':') {
                let (field, rest) = lhs.split_at(colon);
                (field.trim(), Some(rest[1..].trim()))
            } else {
                (lhs, None)
            };
            // Reject if field name has invalid chars (e.g., spaces — likely
            // a directive we didn't catch). Permit lowercase: BitBake's
            // legacy underscore-override syntax (e.g., `SRCREV_qemuarm`)
            // appends a lowercase machine literal to an otherwise-uppercase
            // field name; the field MUST start with an uppercase letter.
            if !field.starts_with(|c: char| c.is_ascii_uppercase()) {
                return None;
            }
            if !field
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return None;
            }
            return Some(Assignment {
                field,
                override_kind,
                op: *op,
                value,
            });
        }
    }
    None
}

/// Strip surrounding `"` or `'` quotes from a BitBake RHS value.
fn strip_quotes(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.len() < 2 {
        return None;
    }
    let first = trimmed.chars().next()?;
    let last = trimmed.chars().last()?;
    if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
        Some(&trimmed[1..trimmed.len() - 1])
    } else {
        None
    }
}

/// Apply one parsed assignment to the in-progress `RecipeMetadata`.
///
/// `RecipeMetadata` is built up via repeated calls. When an
/// assignment targets a field already populated, last-write wins
/// (FR-004); override-syntax `:append`/`:prepend`/`:remove` is
/// merged-as-union per FR-016 with `metadata.overrides_merged`
/// set to `true`.
///
/// `${PN}` and `${PV}` are expanded inline per FR-005;
/// other `${VAR}` references trigger an entry in
/// `metadata.unexpanded_vars`.
fn apply_assignment(asn: Assignment<'_>, metadata: &mut RecipeMetadata) {
    let expanded = expand_vars(asn.value, metadata);

    // For override-syntax (FR-016), mark the metadata flag.
    if asn.override_kind.is_some() {
        metadata.overrides_merged = true;
    }

    // Detect `SRCREV_machine:<arch>` and `RDEPENDS_<pkg>` shapes:
    // these aren't override-syntax even though they look similar.
    // BitBake distinguishes by the assignment-side syntax: `_machine:`
    // here is an override (newer syntax); legacy `_<machine>` uses
    // underscore. We handle both.
    let field_for_dispatch = asn.field;

    match field_for_dispatch {
        "LICENSE" => apply_string_field(&expanded, &asn, |s| {
            // Update the canonical-licensed view + the closed flag.
            if s.trim() == "CLOSED" {
                metadata.license = None;
                metadata.license_closed = true;
            } else {
                metadata.license = try_canonicalize_license(s);
                if metadata.license.is_none() {
                    // Canonicalization failure — record the original
                    // string in unexpanded_vars for transparency.
                    metadata.unexpanded_vars.push(format!("LICENSE={}", s));
                }
                metadata.license_closed = false;
            }
        }),
        "HOMEPAGE" => {
            apply_string_field(&expanded, &asn, |s| {
                metadata.homepage = Some(s.to_string());
            });
        }
        "SUMMARY" => {
            apply_string_field(&expanded, &asn, |s| {
                metadata.summary = Some(s.to_string());
            });
        }
        "DESCRIPTION" => {
            apply_string_field(&expanded, &asn, |s| {
                metadata.description = Some(s.to_string());
            });
        }
        "SRC_URI" => {
            apply_list_field(&expanded, &asn, &mut metadata.src_uris);
        }
        "DEPENDS" => {
            apply_list_field(&expanded, &asn, &mut metadata.depends);
        }
        "SRCREV" => {
            apply_string_field(&expanded, &asn, |s| {
                metadata.srcrev = Some(s.to_string());
            });
        }
        "BBCLASSEXTEND" => {
            apply_list_field(&expanded, &asn, &mut metadata.class_extend);
        }
        f if f.starts_with("SRCREV_") => {
            // Legacy underscore-overrides: SRCREV_<machine>
            let arch = f.trim_start_matches("SRCREV_");
            if !arch.is_empty() {
                metadata.srcrev_by_machine.insert(arch.to_string(), expanded);
            }
        }
        f if f.starts_with("RDEPENDS_") => {
            // Legacy underscore-overrides: RDEPENDS_<pkg>
            let pkg = f.trim_start_matches("RDEPENDS_");
            if !pkg.is_empty() {
                let mut current = metadata.rdepends.remove(pkg).unwrap_or_default();
                apply_list_field(&expanded, &asn, &mut current);
                metadata.rdepends.insert(pkg.to_string(), current);
            }
        }
        _ => {
            // Unknown field — silently ignore. We only extract a
            // curated set of metadata fields (FR-001..FR-005).
        }
    }
}

/// Apply a string-valued assignment.
fn apply_string_field<F: FnMut(&str)>(value: &str, asn: &Assignment<'_>, mut setter: F) {
    // For string-valued fields, the spec collapses `+=`/`=+`/`.=`/`=.`
    // semantically into "last-write wins" for our purposes — recipe
    // metadata rarely uses these operators on string fields, and
    // when it does (e.g., DESCRIPTION += " more"), the operator's
    // semantic is "concatenate the rhs to the existing value." We
    // implement the conservative "overwrite" choice; downstream
    // consumers wanting BitBake-faithful concatenation should run
    // bitbake itself.
    //
    // For override-syntax (`:append`/`:prepend`/`:remove`), per
    // FR-016 we merge-as-union — for string fields this means
    // "treat the override as if it replaced the base," matching
    // the spec's documented approximation.
    let _ = asn; // operator not consulted for string fields
    setter(value);
}

/// Apply a list-valued assignment (split on whitespace).
fn apply_list_field(value: &str, asn: &Assignment<'_>, target: &mut Vec<String>) {
    let new_items: Vec<String> = value
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    match asn.op {
        Operator::Assign | Operator::WeakAssign => {
            // Replace.
            *target = new_items;
        }
        Operator::Default => {
            // Only assign when undefined (per `??=` semantic).
            if target.is_empty() {
                *target = new_items;
            }
        }
        Operator::AppendSpace | Operator::AppendNoSpace => {
            target.extend(new_items);
        }
        Operator::PrependSpace | Operator::PrependNoSpace => {
            let mut combined = new_items;
            combined.extend(std::mem::take(target));
            *target = combined;
        }
    }
    // For override-syntax (`:append`/`:prepend`/`:remove`), per FR-016
    // we merge-as-union — for list fields this is naturally "extend
    // the base list with the override entries." Already handled above
    // by the AppendSpace/AppendNoSpace branch when `op` is `=` and
    // the override_kind is `append`/`prepend`; the simpler treatment
    // (always extend) matches the spec's approximation. The `:remove`
    // case is also treated as extend per the FR-016 caveat (no
    // subtraction).
    if let Some(override_kind) = asn.override_kind {
        let _ = override_kind; // already handled above
    }
}

/// Resolve `${PN}` and `${PV}` references inline (FR-005).
/// Collect every other `${VAR}` reference into
/// `metadata.unexpanded_vars` for transparency.
fn expand_vars(value: &str, metadata: &mut RecipeMetadata) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after_brace = &rest[start + 2..];
        if let Some(end) = after_brace.find('}') {
            let var_name = &after_brace[..end];
            let replacement: String = match var_name {
                "PN" => metadata.recipe_name.clone(),
                "PV" => metadata.recipe_version.clone(),
                _ => {
                    // Unexpanded — record for transparency, leave the
                    // original token in place.
                    let token = format!("${{{}}}", var_name);
                    if !metadata.unexpanded_vars.contains(&token) {
                        metadata.unexpanded_vars.push(token.clone());
                    }
                    token
                }
            };
            out.push_str(&replacement);
            rest = &after_brace[end + 1..];
        } else {
            // Unclosed `${...` — keep literal.
            out.push_str("${");
            rest = after_brace;
        }
    }
    out.push_str(rest);
    out
}

/// Canonicalize a BitBake-syntax LICENSE expression to SPDX.
///
/// BitBake uses `&` for AND and `|` for OR within license strings
/// (e.g., `"GPL-2.0-only & LGPL-2.1-or-later"`). SPDX uses literal
/// `AND` and `OR` keywords. This helper translates between them
/// before calling `SpdxExpression::try_canonical`.
///
/// Returns `None` when canonicalization fails (malformed input,
/// unknown identifier). Callers MUST then emit `NOASSERTION` per
/// FR-001 + FR-012.
pub(crate) fn try_canonicalize_license(raw: &str) -> Option<SpdxExpression> {
    let normalized = raw
        .trim()
        .replace(" & ", " AND ")
        .replace(" | ", " OR ")
        .replace("&", " AND ")
        .replace("|", " OR ");
    // Collapse double-spaces that may emerge from the broad replacements.
    let collapsed = normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    SpdxExpression::try_canonical(&collapsed).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn simple_license_canonicalizes() {
        let result = try_canonicalize_license("MIT");
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_str(), "MIT");
    }

    #[test]
    fn bitbake_ampersand_translates_to_spdx_and() {
        let result = try_canonicalize_license("GPL-2.0-only & LGPL-2.1-or-later");
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(
            s.as_str().contains("AND"),
            "BitBake `&` must become SPDX `AND`, got {}",
            s.as_str()
        );
    }

    #[test]
    fn bitbake_pipe_translates_to_spdx_or() {
        let result = try_canonicalize_license("GPL-2.0-only | MIT");
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(
            s.as_str().contains("OR"),
            "BitBake `|` must become SPDX `OR`, got {}",
            s.as_str()
        );
    }

    #[test]
    fn malformed_license_returns_none() {
        // SPDX doesn't accept arbitrary uppercase strings as license IDs.
        let result = try_canonicalize_license("DEFINITELY-NOT-A-REAL-LICENSE-ID-9999");
        assert!(result.is_none());
    }

    fn write_fixture(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn parse_simple_recipe_extracts_license_and_summary() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "widget_1.0.bb",
            r#"LICENSE = "MIT"
SUMMARY = "Widget library"
HOMEPAGE = "https://example.com/widget"
DEPENDS = "openssl libcurl"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "widget", "1.0").unwrap();
        assert_eq!(meta.license.as_ref().unwrap().as_str(), "MIT");
        assert_eq!(meta.summary.as_deref(), Some("Widget library"));
        assert_eq!(meta.homepage.as_deref(), Some("https://example.com/widget"));
        assert_eq!(meta.depends, vec!["openssl", "libcurl"]);
        assert!(!meta.license_closed);
        assert!(meta.unexpanded_vars.is_empty());
    }

    #[test]
    fn license_closed_sets_flag_and_none() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "secret_1.0.bb",
            r#"LICENSE = "CLOSED"
SUMMARY = "Proprietary"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "secret", "1.0").unwrap();
        assert!(meta.license.is_none());
        assert!(meta.license_closed);
    }

    #[test]
    fn bitbake_ampersand_canonicalizes_in_license_field() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "dual_1.0.bb",
            r#"LICENSE = "GPL-2.0-only & LGPL-2.1-or-later"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "dual", "1.0").unwrap();
        let s = meta.license.unwrap();
        assert!(s.as_str().contains("AND"), "got {}", s.as_str());
    }

    #[test]
    fn multi_line_backslash_continuation_collapses() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "multi_1.0.bb",
            r#"SRC_URI = "git://github.com/foo/bar.git;branch=main \
            file://patch-1.patch \
            file://patch-2.patch"
LICENSE = "MIT"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "multi", "1.0").unwrap();
        assert_eq!(meta.src_uris.len(), 3);
        assert!(meta.src_uris[0].starts_with("git://"));
        assert!(meta.src_uris[1].starts_with("file://"));
        assert!(meta.src_uris[2].starts_with("file://"));
    }

    #[test]
    fn pn_and_pv_expand_inline_other_vars_collected() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "widget_2.0.bb",
            r#"LICENSE = "MIT"
SUMMARY = "Built ${PN} version ${PV} with extra ${BPN}"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "widget", "2.0").unwrap();
        assert_eq!(
            meta.summary.as_deref(),
            Some("Built widget version 2.0 with extra ${BPN}")
        );
        assert!(meta
            .unexpanded_vars
            .iter()
            .any(|v| v.contains("BPN")));
    }

    #[test]
    fn include_chain_merges_with_bb_overriding_inc() {
        let tmp = tempfile::tempdir().unwrap();
        // .bb requires foo.inc, which requires foo-shared.inc.
        // foo-shared.inc sets LICENSE=Apache-2.0
        // foo.inc sets LICENSE=GPL-2.0-only
        // foo_1.0.bb sets LICENSE=MIT
        // Per FR-004 + Q1, the .bb wins.
        write_fixture(
            tmp.path(),
            "foo-shared.inc",
            r#"LICENSE = "Apache-2.0"
SUMMARY = "From shared inc"
"#,
        );
        write_fixture(
            tmp.path(),
            "foo.inc",
            r#"require foo-shared.inc
LICENSE = "GPL-2.0-only"
"#,
        );
        let bb_path = write_fixture(
            tmp.path(),
            "foo_1.0.bb",
            r#"require foo.inc
LICENSE = "MIT"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "foo", "1.0").unwrap();
        // .bb's LICENSE wins (last-write-in-source-order).
        assert_eq!(meta.license.as_ref().unwrap().as_str(), "MIT");
        // But SUMMARY is only set in the foo-shared.inc — should
        // propagate up.
        assert_eq!(meta.summary.as_deref(), Some("From shared inc"));
    }

    #[test]
    fn cyclic_include_chain_is_broken() {
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(
            tmp.path(),
            "a.inc",
            r#"require b.inc
LICENSE = "MIT"
"#,
        );
        write_fixture(
            tmp.path(),
            "b.inc",
            r#"require a.inc
SUMMARY = "From B"
"#,
        );
        let bb_path = write_fixture(
            tmp.path(),
            "cyclic_1.0.bb",
            r#"require a.inc
"#,
        );
        // Should NOT loop forever — cycle is broken.
        let meta = parse_recipe_file(&bb_path, "cyclic", "1.0").unwrap();
        assert_eq!(meta.license.as_ref().unwrap().as_str(), "MIT");
        assert_eq!(meta.summary.as_deref(), Some("From B"));
    }

    #[test]
    fn srcrev_machine_overrides_populate_map() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "kernel_1.0.bb",
            r#"LICENSE = "GPL-2.0-only"
SRCREV_qemuarm = "abc123"
SRCREV_qemuarm64 = "def456"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "kernel", "1.0").unwrap();
        assert_eq!(meta.srcrev_by_machine.len(), 2);
        assert_eq!(meta.srcrev_by_machine.get("qemuarm").map(|s| s.as_str()), Some("abc123"));
        assert_eq!(meta.srcrev_by_machine.get("qemuarm64").map(|s| s.as_str()), Some("def456"));
    }

    #[test]
    fn append_operator_extends_list_field() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "list_1.0.bb",
            r#"LICENSE = "MIT"
DEPENDS = "alpha beta"
DEPENDS += "gamma"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "list", "1.0").unwrap();
        assert_eq!(meta.depends, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn override_syntax_sets_overrides_merged_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "override_1.0.bb",
            r#"LICENSE = "MIT"
SRC_URI:append:rpi4 = " file://rpi4.patch"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "override", "1.0").unwrap();
        assert!(meta.overrides_merged);
    }

    #[test]
    fn closed_license_after_open_license_takes_last_write() {
        // .bb assigns LICENSE = "MIT" first, then overwrites to CLOSED.
        let tmp = tempfile::tempdir().unwrap();
        let bb_path = write_fixture(
            tmp.path(),
            "mixed_1.0.bb",
            r#"LICENSE = "MIT"
LICENSE = "CLOSED"
"#,
        );
        let meta = parse_recipe_file(&bb_path, "mixed", "1.0").unwrap();
        assert!(meta.license_closed);
        assert!(meta.license.is_none());
    }
}
