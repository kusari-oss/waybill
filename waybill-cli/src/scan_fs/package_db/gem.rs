//! Read Ruby gem package metadata from `Gemfile.lock`.
//!
//! Gemfile.lock format (bundler ≥ 2.x):
//!
//! ```text
//! GEM
//!   remote: https://rubygems.org/
//!   specs:
//!     activesupport (7.1.3)
//!       base64
//!       concurrent-ruby (~> 1.0, >= 1.0.2)
//!     base64 (0.2.0)
//!     concurrent-ruby (1.2.3)
//!
//! GIT
//!   remote: https://github.com/rails/rails.git
//!   revision: abc123...
//!   specs:
//!     rails (7.2.0.alpha.internal)
//!
//! PATH
//!   remote: ../vendor/my-gem
//!   specs:
//!     my-gem (0.1.0)
//!
//! PLATFORMS
//!   ruby
//!
//! DEPENDENCIES
//!   activesupport
//!   rails!
//!   my-gem!
//!
//! BUNDLED WITH
//!    2.5.3
//! ```
//!
//! Section headers at column 0, section body at indent 2, gem specs at
//! indent 4 (`gem-name (version)`), transitive deps at indent 6. Legacy
//! bundler 1.x format is largely the same but has no `BUNDLED WITH`
//! trailer and may use two-space vs four-space indents inconsistently;
//! we handle both via indent counting (≥2 for section body, ≥4 for
//! specs) rather than fixed counts.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

// Ruby gem projects are typically a flat or shallow Gemfile +
// Gemfile.lock at root + lib/ + spec/; 6 covers any realistic
// layout. Defense-in-depth backstop for the canonicalize-keyed
// visited-set primary mechanism. Per milestone-054 FR-003.
const MAX_PROJECT_ROOT_DEPTH: usize = 6;

// --------------------------------------------------------------------
// Milestone 162 (T001) — Ruby built-in gems allowlist
// --------------------------------------------------------------------

/// Ruby built-in gems allowlist per milestone 162 (issue #496).
///
/// Union across Ruby 3.2, 3.3, and 3.4 stable-release `Gem::default_gems`
/// outputs — any gem present in the default_gems set of ANY of these
/// releases is treated as built-in per Q2 clarification. This is the
/// authoritative list for the FR-002 synthetic-emission gate: gem names
/// in this list that appear as dep-targets in `Gemfile.lock` but NOT in
/// the GEM/specs section trigger synthetic component emission per
/// data-model.md E4.
///
/// Review cadence: annual (aligned with Ruby's stable release cycle).
/// When Ruby N+1 stable ships:
///
///   1. Add any newly-introduced default_gems to this array.
///   2. Drop any gem that has NOT been default in the last 3 stable
///      Ruby releases (rolling window).
///
/// See FR-006 in specs/162-ruby-built-in-gems/spec.md.
pub(crate) const RUBY_BUILT_IN_GEMS: &[&str] = &[
    "base64",
    "bigdecimal",
    "bundler",
    "cgi",
    "csv",
    "date",
    "delegate",
    "did_you_mean",
    "digest",
    "drb",
    "english",
    "erb",
    "etc",
    "fcntl",
    "fiddle",
    "fileutils",
    "find",
    "forwardable",
    "getoptlong",
    "io-console",
    "io-nonblock",
    "io-wait",
    "ipaddr",
    "irb",
    "json",
    "logger",
    "mutex_m",
    "net-http",
    "net-protocol",
    "nkf",
    "observer",
    "open-uri",
    "open3",
    "openssl",
    "optparse",
    "ostruct",
    "pathname",
    "pp",
    "prettyprint",
    "prime",
    "pstore",
    "psych",
    "racc",
    "rdoc",
    "readline",
    "reline",
    "resolv",
    "resolv-replace",
    "rinda",
    "rss",
    "ruby2_keywords",
    "securerandom",
    "set",
    "shellwords",
    "singleton",
    "stringio",
    "strscan",
    "syslog",
    "tempfile",
    "time",
    "timeout",
    "tmpdir",
    "tsort",
    "un",
    "uri",
    "weakref",
    "yaml",
    "zlib",
];

/// Milestone 162 (T002): True iff `name` is a Ruby toolchain-provided
/// built-in gem per the FR-001 allowlist. O(N) linear scan — N=67 as of
/// 2026-07-04.
pub(crate) fn is_ruby_built_in_gem(name: &str) -> bool {
    RUBY_BUILT_IN_GEMS.contains(&name)
}

// --------------------------------------------------------------------
// Milestone 162 (T005) — SyntheticGemKind for C113 annotation value
// --------------------------------------------------------------------

/// Language runtime whose toolchain provides this gem as built-in.
/// Serialized to the `waybill:synthetic-built-in` (C113) annotation value.
///
/// Closed 1-variant vocab in scope for milestone 162; extensible in
/// future milestones if similar patterns are discovered in other
/// ecosystems.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyntheticGemKind {
    RubyBuiltIn,
}

impl SyntheticGemKind {
    pub(crate) fn as_wire_str(&self) -> &'static str {
        match self {
            Self::RubyBuiltIn => "ruby",
        }
    }
}

/// One spec line in GEM / GIT / PATH. `depends` holds the transitive
/// dependency names parsed from the indent-6 block under this spec —
/// the bit of Gemfile.lock that actually encodes the per-gem dep graph.
/// Per milestone 162 (T003), `depends` carries both the dep-name AND
/// the raw version-constraint clause (empty when the source declaration
/// had no clause). The constraint is load-bearing for FR-005 (C114
/// `waybill:built-in-requirement` annotation on synthetic components).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GemSpec {
    pub name: String,
    pub version: String,
    pub kind: GemSection,
    pub depends: Vec<GemDep>,
}

/// Milestone 162 (T003): one dep-line under a `GemSpec`.
///
/// - `name` — the bare gem name (e.g., `"bundler"`).
/// - `requirement` — the original parenthesized version-constraint
///   from Gemfile.lock's indent-6 dep line, WITHOUT the parentheses.
///   For example: `">= 1.2.0"`, `"~> 1.0, >= 1.0.2"`, or `""` when the
///   source declared no constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GemDep {
    pub name: String,
    pub requirement: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GemSection {
    Gem,
    Git,
    Path,
}

/// A parsed `Gemfile.lock`. `dependencies` holds the gem names declared
/// in the `DEPENDENCIES` block (top-level / direct deps).
#[derive(Clone, Debug, Default)]
pub(crate) struct GemfileLockDocument {
    pub specs: Vec<GemSpec>,
    pub dependencies: Vec<String>,
}

pub(crate) fn parse_gemfile_lock(text: &str) -> GemfileLockDocument {
    let mut doc = GemfileLockDocument::default();
    let mut current_section: Option<GemSection> = None;
    let mut in_specs = false;
    let mut in_dependencies = false;

    for raw_line in text.lines() {
        let indent = raw_line.chars().take_while(|c| *c == ' ').count();
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            in_specs = false;
            in_dependencies = false;
            continue;
        }
        // Section headers live at column 0.
        if indent == 0 {
            match trimmed {
                "GEM" => {
                    current_section = Some(GemSection::Gem);
                    in_specs = false;
                    in_dependencies = false;
                }
                "GIT" => {
                    current_section = Some(GemSection::Git);
                    in_specs = false;
                    in_dependencies = false;
                }
                "PATH" => {
                    current_section = Some(GemSection::Path);
                    in_specs = false;
                    in_dependencies = false;
                }
                "DEPENDENCIES" => {
                    current_section = None;
                    in_specs = false;
                    in_dependencies = true;
                }
                "PLATFORMS" | "BUNDLED WITH" | "CHECKSUMS" | "RUBY VERSION" => {
                    current_section = None;
                    in_specs = false;
                    in_dependencies = false;
                }
                _ => {
                    current_section = None;
                    in_specs = false;
                    in_dependencies = false;
                }
            }
            continue;
        }
        if in_dependencies {
            // DEPENDENCIES block: one gem name per line, optionally
            // with `!` suffix (pinned to GIT/PATH source) or
            // version-spec parens that we ignore here.
            let name = trimmed
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('!')
                .to_string();
            if !name.is_empty() {
                doc.dependencies.push(name);
            }
            continue;
        }
        if current_section.is_none() {
            continue;
        }
        if trimmed == "specs:" {
            in_specs = true;
            continue;
        }
        if !in_specs {
            // Section metadata line (`remote:`, `revision:`, etc.) —
            // ignored; the source_type is captured via the section.
            continue;
        }
        // A gem spec line looks like `gem-name (version)`. Transitive
        // deps (indent 6+) also have this shape; we dedup by name
        // within this lockfile so the transitive line doesn't overwrite
        // the primary spec.
        if indent < 4 {
            continue;
        }
        if indent == 4 {
            // New spec — `gem-name (version)`.
            if let Some((name, version)) = parse_spec_line(trimmed) {
                if let Some(section) = current_section {
                    doc.specs.push(GemSpec {
                        name: name.to_string(),
                        version: version.to_string(),
                        kind: section,
                        depends: Vec::new(),
                    });
                }
            }
        } else if indent >= 6 {
            // Transitive dep line under the most-recently-opened spec.
            // Format is `name` or `name (constraint[, constraint])`.
            // Milestone 162 (T004): preserve the version-constraint
            // clause (needed for the FR-005 C114
            // `waybill:built-in-requirement` annotation on synthetic
            // components). Also strip the `!` source pin per the
            // DEPENDENCIES block's convention.
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let bare = parts
                .next()
                .unwrap_or("")
                .trim_end_matches('!');
            let raw_req = parts.next().unwrap_or("").trim();
            // Strip surrounding parens: `(>= 1.2.0)` → `>= 1.2.0`.
            let requirement = raw_req
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(raw_req)
                .to_string();
            if !bare.is_empty() {
                if let Some(last) = doc.specs.last_mut() {
                    // Ignore duplicate edges if a lockfile lists the
                    // same transitive dep twice (unusual but harmless).
                    if !last.depends.iter().any(|d| d.name == bare) {
                        last.depends.push(GemDep {
                            name: bare.to_string(),
                            requirement,
                        });
                    }
                }
            }
        }
    }

    doc
}

fn parse_spec_line(line: &str) -> Option<(&str, &str)> {
    // Expect `name (version[, versionspec])`. We ignore version
    // constraints of the form `(~> 1.0, >= 1.0.2)` — those only appear
    // on transitive dep lines at deeper indent, which we filter out
    // already.
    let open = line.find('(')?;
    let close = line.find(')')?;
    if close <= open {
        return None;
    }
    let name = line[..open].trim();
    let inner = &line[open + 1..close];
    if name.is_empty() || inner.is_empty() {
        return None;
    }
    // If the inner starts with a comparator, this is a constraint line.
    if inner
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '~' | '>' | '<' | '='))
    {
        return None;
    }
    let version = inner.split(',').next().unwrap_or(inner).trim();
    Some((name, version))
}

fn build_gem_purl(name: &str, version: &str) -> Option<Purl> {
    // purl-spec § Character encoding: `+` and other non-allowed
    // chars must be percent-encoded in both name and version.
    //
    // Milestone 191 (#558): when version is empty (design-tier
    // Gemfile declaration with no resolved Gemfile.lock entry), emit
    // a versionless PURL per purl-spec canonical form — no trailing `@`.
    let purl_str = if version.is_empty() {
        format!("pkg:gem/{}", encode_purl_segment(name))
    } else {
        format!(
            "pkg:gem/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version),
        )
    };
    Purl::new(&purl_str).ok()
}

/// Milestone 162 (T006): build a **versionless** PURL for a synthetic
/// Ruby built-in gem component per FR-003 + Q1. The PURL spec permits
/// omitting the `@version` segment when the version is unknown —
/// consumer vulnerability scanners doing exact-version CVE lookups
/// won't false-positive on `pkg:gem/bundler` (no version to match).
fn build_gem_purl_versionless(name: &str) -> Option<Purl> {
    Purl::new(&format!("pkg:gem/{}", encode_purl_segment(name))).ok()
}

/// Milestone 162 (T006): append synthetic components for Ruby built-in
/// gems referenced as dep targets but not present in GEM/specs.
///
/// Enforces FR-002 (allowlist gate), FR-003 (versionless PURL), FR-004
/// (real-gem-precedence via `emitted_names` check), FR-005 (requirement
/// annotation), and R4 (multi-source union → JSON array).
///
/// - `out` — the already-populated `Vec<PackageDbEntry>` from the
///   per-spec emission loop. Mutated in-place to append synthetic entries.
/// - `emitted_names` — set of gem names already in `out`. Used to
///   enforce FR-004 real-gem-precedence: a real GEM/specs entry with
///   the same name as an allowlist gem takes precedence and NO
///   synthetic entry is emitted for that name.
/// - `source_path` — the Gemfile.lock path (evidence provenance).
/// - `specs` — the parsed `GemfileLockDocument.specs`.
///
/// Returns the list of built-in gem names emitted as synthetic (used
/// for the FR-011 tracing log). Empty when no synthetic components
/// were appended this call.
fn append_synthetic_built_in_gems(
    out: &mut Vec<PackageDbEntry>,
    emitted_names: &HashSet<String>,
    source_path: &str,
    specs: &[GemSpec],
) -> Vec<String> {
    // Collect built-in gem name → set-of-requirement-strings. BTreeMap
    // + BTreeSet for deterministic (sorted) output across runs.
    let mut built_in_refs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for spec in specs {
        for dep in &spec.depends {
            if !is_ruby_built_in_gem(&dep.name) {
                continue;
            }
            if emitted_names.contains(&dep.name) {
                // FR-004: real gem takes precedence over synthetic.
                continue;
            }
            let requirements = built_in_refs.entry(dep.name.clone()).or_default();
            if !dep.requirement.is_empty() {
                requirements.insert(dep.requirement.clone());
            }
        }
    }

    let mut emitted_built_in_names: Vec<String> = Vec::new();

    for (name, requirements) in built_in_refs {
        // FR-003: versionless PURL.
        let Some(purl) = build_gem_purl_versionless(&name) else {
            continue;
        };

        let mut extra_annotations: std::collections::BTreeMap<
            String,
            serde_json::Value,
        > = std::collections::BTreeMap::new();
        // C113: waybill:synthetic-built-in = "ruby"
        extra_annotations.insert(
            "waybill:synthetic-built-in".to_string(),
            serde_json::Value::String(
                SyntheticGemKind::RubyBuiltIn.as_wire_str().to_string(),
            ),
        );
        // C114: waybill:built-in-requirement — string OR JSON array
        // per R4 (multi-source union).
        if !requirements.is_empty() {
            let value = if requirements.len() == 1 {
                // Single source: bare string. `into_iter().next()` on a
                // non-empty BTreeSet is safe.
                match requirements.into_iter().next() {
                    Some(v) => serde_json::Value::String(v),
                    None => serde_json::Value::Null,
                }
            } else {
                serde_json::Value::Array(
                    requirements
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                )
            };
            extra_annotations.insert("waybill:built-in-requirement".to_string(), value);
        }

        let entry = PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.clone(),
            version: String::new(), // versionless per FR-003
            arch: None,
            source_path: source_path.to_string(),
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
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        };
        emitted_built_in_names.push(name);
        out.push(entry);
    }

    emitted_built_in_names
}

fn spec_to_entry(
    spec: &GemSpec,
    source_path: &str,
    _direct_deps: &HashSet<String>,
) -> Option<PackageDbEntry> {
    let purl = build_gem_purl(&spec.name, &spec.version)?;
    let source_type = match spec.kind {
        GemSection::Gem => None,
        GemSection::Git => Some("git".to_string()),
        GemSection::Path => Some("path".to_string()),
    };
    // Gemfile.lock encodes per-gem transitive edges via the indent-6
    // lines under each spec; the parser collected them into
    // `spec.depends`. Scan_fs's relationship resolver will drop any
    // dangling targets (e.g. bundler-provided gems that aren't listed
    // as specs in this lockfile).
    //
    // Milestone 162 (T004): `spec.depends` is now `Vec<GemDep>` (each
    // dep carries both the name and the raw version-constraint); the
    // `PackageDbEntry.depends` shape is `Vec<String>` of bare names.
    // Extract `.name` for edge construction.
    let depends: Vec<String> = spec.depends.iter().map(|d| d.name.clone()).collect();
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: spec.name.clone(),
        version: spec.version.clone(),
        arch: None,
        source_path: source_path.to_string(),
        depends,
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type,
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
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations: Default::default(),
        binary_role: None,
    })
}

/// Convert a disk-observed gemspec into a PackageDbEntry. Gemspec
/// files carry no transitive dep graph (that lives in Gemfile.lock), so
/// `depends` is always empty. Tagged `source_type = "installed-gemspec"`
/// to distinguish from Gemfile.lock-tier entries.
fn gemspec_to_entry(
    name: &str,
    version: &str,
    authors: Option<&str>,
    source_path: &str,
) -> Option<PackageDbEntry> {
    let purl = build_gem_purl(name, version)?;
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: authors.map(|s| s.to_string()),
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("installed-gemspec".to_string()),
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
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        extra_annotations: Default::default(),
        binary_role: None,
    })
}

// ---------------------------------------------------------------------------
// Milestone 051 — Gem dev/test group classification (T022-T026)
// ---------------------------------------------------------------------------

/// Parse a `Gemfile` for `group :name [, :name2] do ... end` blocks
/// and inline `gem "name", group(s): ...` syntax. Returns a map from
/// gem name to the set of groups it appears in. The default group
/// (no enclosing block, no inline keyword) maps to an empty set —
/// production semantic.
///
/// Best-effort line scanner per plan R2: warn-and-skip lines that
/// don't fit the canonical idioms (interpolation, conditional
/// loading, eval_gemfile). Bundler accepts a wider Ruby DSL surface
/// than this matches; consumers wanting full coverage rely on the
/// gemspec source as a fallback.
pub(crate) fn parse_gemfile(path: &Path) -> HashMap<String, BTreeSet<String>> {
    let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    let mut block_stack: Vec<BTreeSet<String>> = Vec::new();
    for raw_line in text.lines() {
        let line = strip_ruby_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        // `end` closes the most recent block (group or otherwise).
        if line == "end" {
            block_stack.pop();
            continue;
        }
        // Block opener: `group :foo[, :bar] do`. Other `do` blocks
        // (source, platforms, ruby) push an empty set so `end`
        // unwinds correctly.
        if line.ends_with(" do") || line.ends_with("do") {
            if let Some(rest) = line.strip_prefix("group ") {
                let groups = parse_group_idents(rest);
                block_stack.push(groups);
                continue;
            }
            if line.starts_with("source ")
                || line.starts_with("platforms ")
                || line.starts_with("group ")
                || line == "do"
            {
                block_stack.push(BTreeSet::new());
                continue;
            }
            // Unknown block — push empty so the matching end pops.
            block_stack.push(BTreeSet::new());
            continue;
        }
        // `gem "name"[, options...]`
        if let Some(rest) = line.strip_prefix("gem ") {
            let Some((name, inline_groups)) = parse_gem_call(rest) else {
                continue;
            };
            let mut groups = BTreeSet::new();
            for g in block_stack.iter().flatten() {
                groups.insert(g.clone());
            }
            for g in inline_groups {
                groups.insert(g);
            }
            merge_groups(&mut out, name, groups);
        }
    }
    out
}

fn strip_ruby_comment(line: &str) -> &str {
    // Naive: `#` starts a comment unless inside a string. The
    // canonical Gemfile rarely uses `#` inside strings, so this
    // suffices for line-oriented scanning.
    line.find('#').map(|i| &line[..i]).unwrap_or(line)
}

fn parse_group_idents(rest: &str) -> BTreeSet<String> {
    // Input shape: `:test do` or `:development, :test do`.
    let mut out = BTreeSet::new();
    let payload = rest.trim_end_matches("do").trim().trim_end_matches(',');
    for piece in payload.split(',') {
        let s = piece.trim();
        if let Some(name) = s.strip_prefix(':') {
            let trimmed = name.trim().trim_end_matches('"').trim_end_matches('\'');
            if !trimmed.is_empty() {
                out.insert(trimmed.to_string());
            }
        }
    }
    out
}

fn parse_gem_call(rest: &str) -> Option<(String, BTreeSet<String>)> {
    // `"rspec"` or `"rspec", "~> 3.0"` or
    // `"pry", group: :development` or `"foo", groups: [:dev, :test]`.
    let s = rest.trim();
    let (name_quote, name) = if let Some(s) = s.strip_prefix('"') {
        ('"', s)
    } else {
        let s = s.strip_prefix('\'')?;
        ('\'', s)
    };
    let close = name.find(name_quote)?;
    let gem_name = name[..close].to_string();
    let after = &name[close + 1..];
    let inline_groups = extract_inline_groups(after);
    Some((gem_name, inline_groups))
}

fn extract_inline_groups(after: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    // Look for `group: :foo` or `groups: [:foo, :bar]`.
    if let Some(idx) = after.find("group:") {
        let payload = &after[idx + "group:".len()..];
        let payload = payload.trim_start();
        if let Some(rest) = payload.strip_prefix(':') {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
    }
    if let Some(idx) = after.find("groups:") {
        let payload = &after[idx + "groups:".len()..];
        // Match `[:foo, :bar]`.
        if let Some(open) = payload.find('[') {
            if let Some(close) = payload[open..].find(']') {
                let inner = &payload[open + 1..open + close];
                for piece in inner.split(',') {
                    let s = piece.trim();
                    if let Some(name) = s.strip_prefix(':') {
                        let trimmed: String = name
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if !trimmed.is_empty() {
                            out.insert(trimmed);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Parse a `*.gemspec` file for `s.add_dependency` (prod),
/// `s.add_runtime_dependency` (prod), and
/// `s.add_development_dependency` (dev) calls.
/// Returns gem name → groups map (empty set for prod, single
/// `"development"` for dev).
pub(crate) fn parse_gemspec_groups(path: &Path) -> HashMap<String, BTreeSet<String>> {
    let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for raw_line in text.lines() {
        let line = strip_ruby_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let (rest, groups): (&str, BTreeSet<String>) = if let Some(r) =
            line.strip_prefix(".add_development_dependency")
                .or_else(|| line.split('.').nth(1).and_then(|after_dot| {
                    after_dot.strip_prefix("add_development_dependency")
                }))
        {
            let mut g = BTreeSet::new();
            g.insert("development".to_string());
            (r, g)
        } else if let Some(r) = line.strip_prefix(".add_dependency").or_else(|| {
            line.split('.').nth(1).and_then(|after_dot| {
                after_dot
                    .strip_prefix("add_dependency")
                    .or_else(|| after_dot.strip_prefix("add_runtime_dependency"))
            })
        }) {
            (r, BTreeSet::new())
        } else {
            continue;
        };
        // Now `rest` looks like ` "rspec", "~> 3.0"`. Pull the first
        // string literal.
        let s = rest.trim_start_matches(|c: char| c == '(' || c.is_whitespace());
        let Some((quote_char, body)) = s
            .strip_prefix('"')
            .map(|b| ('"', b))
            .or_else(|| s.strip_prefix('\'').map(|b| ('\'', b)))
        else {
            continue;
        };
        let Some(close) = body.find(quote_char) else {
            continue;
        };
        let name = body[..close].to_string();
        if name.is_empty() {
            continue;
        }
        out.entry(name).or_default().extend(groups);
    }
    out
}

/// Compute the prod-reachable gem name set by BFS-walking the lock's
/// `specs:` indent-6 transitive edges starting from `direct_prod`.
pub(crate) fn compute_gem_prod_set(
    direct_prod: &HashSet<String>,
    lock: &GemfileLockDocument,
) -> HashSet<String> {
    let mut by_name: HashMap<&str, &GemSpec> = HashMap::new();
    for spec in &lock.specs {
        by_name.insert(spec.name.as_str(), spec);
    }
    let mut visited: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = direct_prod.iter().cloned().collect();
    while let Some(name) = frontier.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(spec) = by_name.get(name.as_str()) {
            for dep in &spec.depends {
                if !visited.contains(&dep.name) {
                    frontier.push(dep.name.clone());
                }
            }
        }
    }
    visited
}

/// Find sibling Gemfile + `*.gemspec` files alongside a Gemfile.lock.
fn find_grouping_sources(lock_path: &Path) -> (Option<PathBuf>, Vec<PathBuf>) {
    let Some(project_root) = lock_path.parent() else {
        return (None, Vec::new());
    };
    let gemfile = project_root.join("Gemfile");
    let gemfile = if gemfile.is_file() { Some(gemfile) } else { None };
    let mut gemspecs = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(project_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) == Some("gemspec") {
                gemspecs.push(path);
            }
        }
    }
    (gemfile, gemspecs)
}

/// Public entry point — walks `rootfs` for `Gemfile.lock` files AND
/// for `specifications/*.gemspec` files (Ruby's stdlib/default gems +
/// system-installed gems not pinned by a Gemfile). Dedupes on PURL so
/// Gemfile.lock entries win if both sources see the same gem.
///
/// Milestone 051: per-Gemfile.lock, parse co-located Gemfile +
/// `*.gemspec` files; build a union grouping map (per FR-006:
/// production wins when sources disagree); compute prod-reachable
/// closure; tag entries OUTSIDE the prod set with `is_dev = Some(true)`;
/// drop tagged entries when `!include_dev`.
pub fn read(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    let mut tagged_dev = 0usize;
    let mut dropped = 0usize;
    for lock_path in find_gemfile_locks(rootfs, exclude_set) {
        let Ok(text) = std::fs::read_to_string(&lock_path) else {
            continue;
        };
        let doc = parse_gemfile_lock(&text);
        let direct: HashSet<String> = doc.dependencies.iter().cloned().collect();
        let source_path = lock_path.to_string_lossy().into_owned();

        // Build the merged grouping signal from Gemfile + gemspec
        // sources. Production-wins union (FR-006): a gem with empty
        // group set in ANY source counts as prod.
        let (gemfile_path, gemspec_paths) = find_grouping_sources(&lock_path);
        let mut grouping: HashMap<String, BTreeSet<String>> = HashMap::new();
        if let Some(p) = gemfile_path {
            for (name, groups) in parse_gemfile(&p) {
                merge_groups(&mut grouping, name, groups);
            }
        }
        for p in &gemspec_paths {
            for (name, groups) in parse_gemspec_groups(p) {
                merge_groups(&mut grouping, name, groups);
            }
        }

        // Direct prod-roots = direct deps that EITHER aren't in
        // grouping OR carry an empty group set in grouping (default
        // group = production).
        let direct_prod: HashSet<String> = direct
            .iter()
            .filter(|name| {
                grouping
                    .get(name.as_str())
                    .is_none_or(|groups| groups.is_empty())
            })
            .cloned()
            .collect();
        let prod_set = compute_gem_prod_set(&direct_prod, &doc);

        for spec in &doc.specs {
            let Some(mut entry) = spec_to_entry(spec, &source_path, &direct) else {
                continue;
            };
            // Milestone 052/part-2: 4-way classifier per FR-006.
            // Gems in `:test` group → Test; other non-default groups
            // (`:development`, `:doc`, custom) → Development; default
            // group → Runtime. Multi-group gems with `:test` plus
            // anything else fall to Test only when test is the
            // narrowest classification — but production-wins via the
            // empty-group classification at the direct_prod filter
            // above already handles default+anything-else cases.
            use waybill_common::resolution::LifecycleScope;
            if !prod_set.contains(&spec.name) {
                let scope = match grouping.get(&spec.name) {
                    Some(groups) if groups.contains("test") && groups.len() == 1 => {
                        LifecycleScope::Test
                    }
                    Some(_) => LifecycleScope::Development,
                    // Transitive gems reachable only from non-default
                    // direct deps inherit Development (the most
                    // common classification — Bundler `:development`
                    // and `:test` groups dominate).
                    None => LifecycleScope::Development,
                };
                entry.lifecycle_scope = Some(scope);
                tagged_dev += 1;
                if !include_dev {
                    dropped += 1;
                    continue;
                }
            } else {
                entry.lifecycle_scope = Some(LifecycleScope::Runtime);
            }
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }

        // Milestone 162 (T012): after the per-spec emission loop, walk
        // every entry's dep list and emit synthetic components for
        // allowlist gem names that were referenced as deps but NOT
        // already in `out`. FR-004 real-gem-precedence enforced via
        // `emitted_names` HashSet.
        let emitted_names: HashSet<String> = out
            .iter()
            .filter(|e| e.purl.as_str().starts_with("pkg:gem/"))
            .map(|e| e.name.clone())
            .collect();
        let synthetic_names = append_synthetic_built_in_gems(
            &mut out,
            &emitted_names,
            &source_path,
            &doc.specs,
        );
        // Update `seen_purls` to include the synthetic PURLs (versionless)
        // so a later Gemfile.lock in the same scan doesn't emit a
        // duplicate.
        for name in &synthetic_names {
            if let Some(purl) = build_gem_purl_versionless(name) {
                seen_purls.insert(purl.as_str().to_string());
            }
        }
        // FR-011 tracing log: emit ONLY when at least one synthetic
        // component was appended.
        if !synthetic_names.is_empty() {
            tracing::info!(
                lockfile_path = %source_path,
                count = synthetic_names.len(),
                built_in_names = %synthetic_names.join(","),
                "gem built-in synthetic components emitted"
            );
        }
    }
    // Gemspec walk (conformance bug 3): Ruby stdlib and default gems
    // ship as `<ruby>/lib/ruby/gems/<VERSION>/specifications/default/*.gemspec`
    // and are invisible to Gemfile.lock scanning. Also catches any
    // system-wide `gem install` outputs living in the standard
    // specifications dirs.
    for spec_path in find_gemspecs(rootfs, exclude_set) {
        let Ok(text) = std::fs::read_to_string(&spec_path) else {
            continue;
        };
        let Some(spec) = parse_gemspec_full(&text) else {
            continue;
        };
        let source_path = spec_path.to_string_lossy().into_owned();
        let Some(entry) = gemspec_to_entry(
            &spec.name,
            &spec.version,
            spec.authors.as_deref(),
            &source_path,
        ) else {
            continue;
        };
        let purl_key = entry.purl.as_str().to_string();
        if seen_purls.insert(purl_key) {
            out.push(entry);
        }
    }

    // Milestone 069 — Phase A: emit one main-module per top-level
    // `*.gemspec` (FR-001). Augment-existing-or-emit-new pattern
    // mirrors cargo (064) / npm (066) / pip (068). When a same-PURL
    // Gemfile.lock-derived entry exists, layer C40 + parent_purl: None
    // on top while preserving the existing entry's (richer) `depends`
    // list. When no same-PURL match exists, emit net-new.
    let mut main_modules_emitted = 0usize;
    for gemspec_path in find_top_level_gemspecs(rootfs) {
        let Some(synthesized) = build_gem_main_module_entry(&gemspec_path) else {
            continue;
        };
        let purl_key = synthesized.purl.as_str().to_string();
        if let Some(existing) = out.iter_mut().find(|e| e.purl.as_str() == purl_key) {
            for (k, v) in synthesized.extra_annotations.iter() {
                existing
                    .extra_annotations
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
            existing.parent_purl = None;
            // Merge synthesized depends, dedup against existing.
            let existing_deps: HashSet<String> =
                existing.depends.iter().cloned().collect();
            for d in &synthesized.depends {
                if !existing_deps.contains(d) {
                    existing.depends.push(d.clone());
                }
            }
            if existing.sbom_tier.is_none() {
                existing.sbom_tier = synthesized.sbom_tier.clone();
            }
            main_modules_emitted += 1;
        } else if seen_purls.insert(purl_key) {
            out.push(synthesized);
            main_modules_emitted += 1;
        }
    }

    // Milestone 216 — Phase B: emit application main-module for every
    // top-level `Gemfile` in a directory that does NOT carry a
    // `*.gemspec` (bundler-managed application, not a published gem).
    // FR-007 gemspec-wins is enforced inside `find_top_level_gemfiles`
    // (the walker's directory-has-gemspec guard), so there's no PURL
    // overlap with the m069 gemspec-loop above by construction —
    // append-only, no augment-existing pattern needed.
    for gemfile_path in find_top_level_gemfiles(rootfs) {
        let Some(app_entry) = build_gem_application_main_module_entry(
            &gemfile_path,
            rootfs,
        ) else {
            continue;
        };
        out.push(app_entry);
        main_modules_emitted += 1;
    }

    // Milestone 069 same-PURL dedup (rare given install-state path
    // exclusion in `find_top_level_gemspecs`, but defensive).
    let dedup_drops = dedup_gem_main_modules_by_purl(&mut out);
    if !dedup_drops.is_empty() {
        let dropped_paths: Vec<String> = dedup_drops
            .iter()
            .map(|d| d.dropped_path.clone())
            .collect();
        let kept_path = dedup_drops
            .first()
            .map(|d| d.kept_path.clone())
            .unwrap_or_default();
        let example_purl = dedup_drops
            .first()
            .map(|d| d.purl.clone())
            .unwrap_or_default();
        tracing::warn!(
            count = dedup_drops.len(),
            example_purl = %example_purl,
            kept = %kept_path,
            dropped = ?dropped_paths,
            "gem: deduped same-PURL *.gemspec files",
        );
    }

    if !out.is_empty() {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            main_modules_emitted,
            same_purl_duplicates_dropped = dedup_drops.len(),
            tagged_dev,
            dropped_when_no_include_dev = dropped,
            include_dev,
            "parsed Gemfile.lock + gemspec entries",
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Milestone 069 — gem source-tree main-module component
// ---------------------------------------------------------------------------

/// Record describing a duplicate main-module dropped during dedup,
/// returned in batch from `dedup_gem_main_modules_by_purl` for
/// caller-side `tracing::warn!` emission. Mirrors cargo (064) /
/// npm (066) / pip (068).
#[derive(Debug, Clone)]
pub(crate) struct GemDroppedDuplicate {
    pub purl: String,
    pub kept_path: String,
    pub dropped_path: String,
}

/// Walk `rootfs` for top-level project `*.gemspec` files. Excludes
/// install-state paths (`vendor/`, `gems/`, `specifications/`,
/// `.bundle/`) per FR-003. Distinct from `find_gemspecs`, which
/// targets `specifications/` directories for the dep-emission path.
///
/// Output is in deterministic walk order (alphabetical via
/// `read_dir`-then-sort) so the dedup-by-PURL pass is host-agnostic.
/// Milestone 114: delegates to `scan_fs::walk::safe_walk`. Output is
/// sorted lex by path to preserve pre-114 cross-platform deterministic
/// discovery order (the pre-114 walker explicitly sorted children
/// before iterating).
fn find_top_level_gemspecs(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // top_level_gemspecs uses a richer skip set than the other gem
    // walkers (also skips `vendor`, `gems`, `specifications`, `.bundle`
    // per FR-003). This walker does NOT use the milestone-113 user
    // exclusion set (it's used internally for main-module discovery,
    // not at the user-visible scan boundary).
    let empty = super::exclude_path::ExclusionSet::default();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_GEMSPEC_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
                || matches!(
                    name,
                    "vendor" | "gems" | "specifications" | ".bundle"
                )
        },
        exclude_set: &empty,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("gemspec"))
                .unwrap_or(false)
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

/// Build the gem main-module entry for a single top-level `*.gemspec`.
///
/// Lenient parse: extracts `s.name` (or `spec.name`) and `s.version`
/// (or `spec.version`) literal-string assignments. Returns `None`
/// when `name` is unparseable (no fallback identity). When `name`
/// is present but `version` is non-literal (constant ref, expression),
/// emits with the literal `0.0.0-unknown` placeholder per FR-001
/// step 2 + Assumption A1.
///
/// Waybill does NOT execute the gemspec as Ruby code (A9). Only
/// literal-string assignments are recognized; everything else falls
/// through to the placeholder.
fn build_gem_main_module_entry(gemspec_path: &Path) -> Option<PackageDbEntry> {
    let text = std::fs::read_to_string(gemspec_path).ok()?;
    // Lenient extraction — same predicates as `parse_gemspec_full` at
    // gem.rs:947 but version is optional (placeholder fallback).
    let mut name: Option<String> = None;
    let mut version_literal: Option<String> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(v) = strip_assignment(line, "name") {
            if let Some(literal) = extract_string_literal(v) {
                if !literal.is_empty() {
                    name = Some(literal);
                }
            }
        } else if let Some(v) = strip_assignment(line, "version") {
            if let Some(literal) = extract_string_literal(v) {
                if !literal.is_empty() {
                    version_literal = Some(literal);
                }
            }
        }
    }
    let name = name?;
    let version = version_literal.unwrap_or_else(|| "0.0.0-unknown".to_string());
    let purl = build_gem_purl(&name, &version)?;
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "waybill:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );

    // Milestone 116 — produces-binaries extraction per FR-008 (gem).
    // The gemspec's `s.executables = [...]` declaration enumerates
    // produced binary names. We support both the array-literal form
    // (`s.executables = ["a", "b"]`) and the %w-syntax shorthand
    // (`s.executables = %w[a b]`).
    {
        let binary_candidates = extract_gemspec_executables(&text);
        crate::scan_fs::produces_binaries::stamp_into_annotations(
            &mut extra_annotations,
            binary_candidates,
        );
    }

    let manifest_dir = gemspec_path.parent()?;
    let source_path = format!("path+file://{}", manifest_dir.display());
    // Direct-dep names from `s.add_dependency` / `s.add_runtime_dependency`
    // / `s.add_development_dependency` per FR-007. Reuses
    // `parse_gemspec_groups`, which returns name → groups; flatten the
    // keys into a single Vec.
    let groups = parse_gemspec_groups(gemspec_path);
    let depends: Vec<String> = groups.into_keys().collect();
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: None,
        source_path,
        depends,
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
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

/// Dedup main-module entries by PURL, preserving the first occurrence.
/// Mirrors cargo's `dedup_main_modules_by_purl` from milestone 064 T010.
/// Predicate is C40-tag-driven; non-main-module gem entries are
/// untouched even if their PURLs would collide.
pub(crate) fn dedup_gem_main_modules_by_purl(
    entries: &mut Vec<PackageDbEntry>,
) -> Vec<GemDroppedDuplicate> {
    let mut dropped: Vec<GemDroppedDuplicate> = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut keep: Vec<PackageDbEntry> = Vec::with_capacity(entries.len());
    for entry in std::mem::take(entries) {
        let is_main = entry
            .extra_annotations
            .get("waybill:component-role")
            .and_then(|v| v.as_str())
            == Some("main-module");
        if !is_main {
            keep.push(entry);
            continue;
        }
        let purl = entry.purl.as_str().to_string();
        if let Some(kept_path) = seen.get(&purl) {
            dropped.push(GemDroppedDuplicate {
                purl: purl.clone(),
                kept_path: kept_path.clone(),
                dropped_path: entry.source_path.clone(),
            });
        } else {
            seen.insert(purl, entry.source_path.clone());
            keep.push(entry);
        }
    }
    *entries = keep;
    dropped
}

// ---------------------------------------------------------------------------
// Milestone 216 — Gemfile-only Ruby application main-module
// ---------------------------------------------------------------------------
//
// Emits a `waybill:component-role = "main-module"` component for every
// directory that carries a `Gemfile` AND does NOT carry a `*.gemspec`
// (bundler-managed application, not a published gem). Uses the
// purl-spec-blessed `pkg:generic/<name>@<version>` PURL type (per m216
// FR-002) with a companion `waybill:package-shape = "application"`
// annotation (per FR-008) so downstream consumers can distinguish
// Gemfile-derived main-modules from published-gem main-modules.
//
// See specs/216-gemfile-main-module/ for the full spec + design.

const M216_PACKAGE_SHAPE_KEY: &str = "waybill:package-shape";
const M216_APPLICATION_VALUE: &str = "application";
const M216_VERSION_PLACEHOLDER: &str = "0.0.0-unknown";

/// Version-resolution ladder for a Gemfile-only Ruby application
/// (m216 FR-004 + research R3). Two-step ladder over the m053
/// `run_git_describe_with_timeout` primitive:
///   1. `git describe --tags --always` in the application dir
///   2. `git describe --tags --always` at the scan root
///   3. Literal `"0.0.0-unknown"` placeholder
///
/// Deliberately bare `0.0.0-unknown` (NOT `v0.0.0-unknown` as Go
/// uses) — Ruby versioning convention doesn't prefix `v`. This is
/// the ONLY reason gem.rs can't call `resolve_workspace_version`
/// verbatim.
fn resolve_gem_application_version(
    application_dir: &Path,
    scan_root: &Path,
) -> String {
    use crate::scan_fs::package_db::golang::legacy::run_git_describe_with_timeout;
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
    // Step 1 — describe from application dir. Skip if no .git (see
    // the m053 legacy comment on parent-repo cross-host leakage).
    if application_dir.join(".git").exists() {
        if let Some(v) = run_git_describe_with_timeout(
            application_dir,
            &["describe", "--tags", "--always"],
            TIMEOUT,
        ) {
            return v;
        }
    }
    // Step 2 — describe from scan root (covers the "single-tag applies
    // to the whole tree" case for monorepos).
    if scan_root.join(".git").exists() {
        if let Some(v) = run_git_describe_with_timeout(
            scan_root,
            &["describe", "--tags", "--always"],
            TIMEOUT,
        ) {
            return v;
        }
    }
    M216_VERSION_PLACEHOLDER.to_string()
}

/// Walk `rootfs` for top-level `Gemfile` files. Excludes install-state
/// paths (`vendor/`, `gems/`, `specifications/`, `.bundle/`) per m216
/// R1. Applies the FR-007 gemspec-wins guard: skips any directory
/// whose contents include ANY `*.gemspec` file (the pre-existing m069
/// gemspec path already emits the identity for those directories).
///
/// Result is sorted lex by path so downstream emit order is
/// host-agnostic.
fn find_top_level_gemfiles(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let empty = super::exclude_path::ExclusionSet::default();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_GEMSPEC_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
                || matches!(
                    name,
                    "vendor" | "gems" | "specifications" | ".bundle"
                )
        },
        exclude_set: &empty,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if !path.is_file() {
            return;
        }
        // Only exact-name `Gemfile` (case-sensitive) qualifies.
        let name = path.file_name().and_then(|s| s.to_str());
        if name != Some("Gemfile") {
            return;
        }
        // FR-007 gemspec-wins: skip if any *.gemspec sibling exists.
        if let Some(dir) = path.parent() {
            if directory_has_gemspec(dir) {
                return;
            }
        }
        out.push(path.to_path_buf());
    });
    out.sort();
    out
}

/// Returns true iff `dir` contains any file with a `.gemspec`
/// extension (case-insensitive to match `find_top_level_gemspecs`
/// behavior). Used by [`find_top_level_gemfiles`] to enforce the
/// FR-007 gemspec-wins precedence.
fn directory_has_gemspec(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("gemspec"))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

/// Build the application main-module entry for a `Gemfile`-only
/// directory (m216 FR-001 + FR-002 + FR-008 + data-model E1).
///
/// - PURL: `pkg:generic/<slug>@<version>` where slug comes from the
///   m215 `subject_slug` rules applied to the directory basename,
///   and version comes from the [`resolve_gem_application_version`]
///   ladder (git-describe → placeholder).
/// - Annotations: `waybill:component-role = "main-module"` (split-axis
///   + m127 root-selector signal) AND `waybill:package-shape =
///   "application"` (m216 parity-bridging distinguisher).
/// - `sbom_tier = Some("source")` — matches the gemspec-derived
///   builder convention (line 1281).
///
/// Returns `None` when the directory basename sanitizes to empty
/// (pathological case per R2 skip pattern) or when PURL construction
/// fails.
fn build_gem_application_main_module_entry(
    gemfile_path: &Path,
    scan_root: &Path,
) -> Option<PackageDbEntry> {
    let application_dir = gemfile_path.parent()?;
    let raw_name = application_dir
        .file_name()
        .and_then(|n| n.to_str())?
        .to_string();
    // Reuse m215's subject_slug rules for lowercase + unsafe-char
    // stripping + non-ASCII stripping + 100-char truncation. Build a
    // pseudo-PURL just to route through the same helper; we only
    // consume the returned slug string.
    let pseudo_purl_string = format!(
        "pkg:generic/{}@0.0.0",
        encode_purl_segment(&raw_name)
    );
    let pseudo_purl = Purl::new(&pseudo_purl_string).ok()?;
    let slug = crate::generate::split::subject_slug(&pseudo_purl);
    if slug.is_empty() {
        return None;
    }
    let version = resolve_gem_application_version(application_dir, scan_root);
    let purl_string = format!(
        "pkg:generic/{}@{}",
        encode_purl_segment(&slug),
        encode_purl_segment(&version),
    );
    let purl = Purl::new(&purl_string).ok()?;

    let mut extra_annotations: BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "waybill:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        M216_PACKAGE_SHAPE_KEY.to_string(),
        serde_json::Value::String(M216_APPLICATION_VALUE.to_string()),
    );

    let source_path = format!("path+file://{}", application_dir.display());

    // Populate `depends` from the Gemfile.lock's DEPENDENCIES block
    // when the sibling lock exists. Without this the application
    // main-module lands in the graph with zero outgoing edges, and
    // issue-#236's `synthesize_root` fallback no longer fires (that
    // fallback was gated on "no main-module exists" — m216 changed
    // that condition). Result would be an SBOM where the operator-
    // facing root has no edges to its own declared deps.
    let depends: Vec<String> = application_dir
        .join("Gemfile.lock")
        .canonicalize()
        .ok()
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .map(|text| parse_gemfile_lock(&text).dependencies)
        .unwrap_or_default();

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: slug,
        version,
        arch: None,
        source_path,
        depends,
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
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

// ---------------------------------------------------------------------------
// end of m216 additions
// ---------------------------------------------------------------------------

/// Production-wins union: a gem with empty group set in ANY source
/// counts as production (mirrors FR-006). When the existing entry
/// already represents prod (empty groups), keep it; otherwise the
/// union of groups is the new value.
fn merge_groups(
    out: &mut HashMap<String, BTreeSet<String>>,
    name: String,
    new_groups: BTreeSet<String>,
) {
    match out.get_mut(&name) {
        Some(existing) if existing.is_empty() => {
            // Already classified as prod by another source — keep.
        }
        Some(existing) if new_groups.is_empty() => {
            existing.clear();
        }
        Some(existing) => {
            existing.extend(new_groups);
        }
        None => {
            out.insert(name, new_groups);
        }
    }
}

/// Milestone 114: delegates to `scan_fs::walk::safe_walk`.
fn find_gemfile_locks(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_PROJECT_ROOT_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(should_skip_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_dir() {
            let lock = path.join("Gemfile.lock");
            if lock.is_file() {
                out.push(lock);
            }
        }
    });
    out
}

fn should_skip_descent(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name,
        "target" | "vendor" | "node_modules" | "dist" | "__pycache__"
    )
}

/// Find `.gemspec` files under `rootfs` that live in a
/// `specifications/` directory (including `specifications/default/`).
/// This is the canonical location for installed gems — Ruby's
/// `Gem::Specification.dirs` resolves to paths like:
///
/// - `/usr/lib/ruby/gems/3.3.0/specifications/`
/// - `/usr/lib/ruby/gems/3.3.0/specifications/default/`   (stdlib gems)
/// - `$HOME/.gem/ruby/3.3.0/specifications/`
/// - `/opt/*/gems/specifications/`
///
/// Rather than hard-code those paths, we walk the filesystem looking
/// for any directory named `specifications` containing `.gemspec`
/// files. Cheap, covers all Ruby install layouts (distro packages,
/// rbenv, rvm, asdf, ruby-install), and doesn't depend on environment
/// variables.
// Gemspec scans walk install-tree paths like `/usr/lib/ruby/gems/
// <ruby-ver>/specifications/`; 10 levels covers depth from any
// realistic rootfs. Defense-in-depth backstop for the canonicalize-
// keyed visited-set primary mechanism. Per milestone-054 FR-003.
const MAX_GEMSPEC_WALK_DEPTH: usize = 10;

/// Milestone 114: delegates to `scan_fs::walk::safe_walk`.
///
/// The pre-114 walker had a special semantic: when descent hit a
/// `specifications` directory, it harvested gemspecs from it AND did
/// NOT recurse further. Post-114 the visit callback detects when we
/// *visit* a `specifications` dir and invokes `harvest_gemspecs_in_dir`
/// on its contents — which handles the `default/` one-level
/// recursion internally. We let `safe_walk` descend naturally past
/// the specifications dir (a minor perf cost on systems with deep
/// per-gem source trees, but functionally byte-identical since
/// neither psych.gemspec NOR default/json.gemspec ever names a
/// nested `specifications` dir; only the harvest call pushes into
/// `out`, and the natural-descent visits ignore files).
fn find_gemspecs(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_GEMSPEC_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(should_skip_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_dir()
            && path
                .file_name()
                .and_then(|s| s.to_str())
                == Some("specifications")
        {
            harvest_gemspecs_in_dir(path, &mut out);
        }
    });
    out
}

fn harvest_gemspecs_in_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("gemspec"))
                .unwrap_or(false)
            {
                out.push(path);
            }
        } else if path.is_dir() {
            // `specifications/default/` contains Ruby-shipped stdlib
            // gems. One level of recursion is enough — Ruby doesn't
            // nest deeper.
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name == "default" {
                    harvest_gemspecs_in_dir(&path, out);
                }
            }
        }
    }
}

/// Parse a `.gemspec` file and extract `(name, version)`.
///
/// Gemspec files are Ruby source; waybill doesn't execute Ruby.
/// Fortunately the name+version assignments follow a rigid idiom
/// across all installed gemspecs:
///
/// ```ruby
/// Gem::Specification.new do |s|
///   s.name = "json"
///   s.version = "2.7.2"                 # most common
///   # or:
///   s.version = Gem::Version.new "2.7.2"
///   ...
/// end
/// ```
///
/// We only need to recognise the `s.name`/`s.version` (or `spec.`/
/// `specification.`) assignment lines and strip the quoted literal.
/// Any non-trivial Ruby expression (interpolation, conditionals) for
/// name or version returns `None` and the caller skips the gem.
///
/// Production code calls `parse_gemspec_full` directly (richer return).
/// This name+version-only wrapper stays for unit-test convenience.
#[allow(dead_code)]
pub(crate) fn parse_gemspec(text: &str) -> Option<(String, String)> {
    parse_gemspec_full(text).map(|g| (g.name, g.version))
}

/// Parsed `.gemspec` fields. `authors` is the raw array content
/// joined with `", "` when multiple; single-author form (`s.author =
/// "..."`) is also accepted and returned as a one-element string.
pub(crate) struct GemspecFields {
    pub name: String,
    pub version: String,
    pub authors: Option<String>,
}

/// Milestone 116 — extract produced binary names from a gemspec's
/// `s.executables = [...]` declaration. Supports both shapes:
///
///   - Array-literal: `s.executables = ["foo", "bar"]`
///   - %w shorthand:  `s.executables = %w[foo bar]`
///
/// Returns the names in source order (the shared normalizer dedup-sorts
/// at stamp time). Empty when the declaration is absent.
fn extract_gemspec_executables(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        let Some(rhs) = strip_assignment(line, "executables") else {
            continue;
        };
        let rhs = rhs.trim();
        // %w[a b] / %w(a b) / %w{a b}
        if let Some(rest) = rhs.strip_prefix("%w") {
            let body = rest
                .trim()
                .trim_start_matches(['[', '(', '{'])
                .trim_end_matches([']', ')', '}']);
            for token in body.split_whitespace() {
                if !token.is_empty() {
                    out.push(token.to_string());
                }
            }
            continue;
        }
        // ["a", "b"] / ['a', 'b']
        if let Some(inner) = rhs
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
        {
            for piece in inner.split(',') {
                if let Some(literal) = extract_string_literal(piece.trim()) {
                    if !literal.is_empty() {
                        out.push(literal);
                    }
                }
            }
        }
    }
    out
}

pub(crate) fn parse_gemspec_full(text: &str) -> Option<GemspecFields> {
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut authors: Option<String> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(v) = strip_assignment(line, "name") {
            if let Some(literal) = extract_string_literal(v) {
                name = Some(literal);
            }
        } else if let Some(v) = strip_assignment(line, "version") {
            if let Some(literal) = extract_string_literal(v) {
                version = Some(literal);
            }
        } else if let Some(v) = strip_assignment(line, "authors") {
            if let Some(joined) = extract_string_array(v) {
                authors = Some(joined);
            }
        } else if let Some(v) = strip_assignment(line, "author") {
            // Some gemspecs use the singular form.
            if let Some(literal) = extract_string_literal(v) {
                authors = Some(literal);
            }
        }
    }
    match (name, version) {
        (Some(n), Some(v)) if !n.is_empty() && !v.is_empty() => Some(GemspecFields {
            name: n,
            version: v,
            authors,
        }),
        _ => None,
    }
}

/// Extract a bracketed array of string literals — `["Alice", "Bob"]`
/// or `['Alice']` — and return `"Alice, Bob"`. Ignores surrounding
/// trailing tokens like `.freeze`. Returns `None` on malformed input.
fn extract_string_array(rhs: &str) -> Option<String> {
    let trimmed = rhs.trim();
    let inside = trimmed
        .strip_prefix('[')
        .and_then(|s| s.rsplit_once(']'))
        .map(|(before, _after)| before.trim())?;
    let mut out: Vec<String> = Vec::new();
    for piece in inside.split(',') {
        let p = piece.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(literal) = extract_string_literal(p) {
            if !literal.is_empty() {
                out.push(literal);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join(", "))
    }
}

/// Match a line like `s.name = "foo"` / `spec.version = "1.0"` /
/// `specification.name = "foo"` and return the RHS trimmed. Returns
/// `None` when the line doesn't match any accepted receiver + attribute
/// combo, or when the attribute doesn't match `attr`.
fn strip_assignment<'a>(line: &'a str, attr: &str) -> Option<&'a str> {
    // Receivers Ruby gemspec generators emit in practice.
    const RECEIVERS: &[&str] = &["s", "spec", "specification", "gem"];
    for receiver in RECEIVERS {
        let prefix = format!("{receiver}.{attr}");
        if let Some(rest) = line.strip_prefix(&prefix) {
            let rest = rest.trim_start();
            if let Some(rhs) = rest.strip_prefix('=') {
                return Some(rhs.trim());
            }
        }
    }
    None
}

/// Extract the first string literal from `rhs`, handling:
///   `"foo"` / `'foo'`
///   `Gem::Version.new("foo")` / `Gem::Version.new "foo"`
///   `"foo".freeze`
/// Returns the content between quotes; `None` if no literal found.
fn extract_string_literal(rhs: &str) -> Option<String> {
    let bytes = rhs.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' || b == b'\'' {
            let quote = b;
            // Find the matching closing quote; gemspec strings don't
            // contain escapes in practice (Ruby string literals with
            // `\"` do exist but gem names/versions never use them).
            let start = i + 1;
            for j in start..bytes.len() {
                if bytes[j] == quote {
                    let literal = &rhs[start..j];
                    if literal.is_empty() {
                        return None;
                    }
                    return Some(literal.to_string());
                }
            }
            return None;
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_gem_section() {
        let text = r#"
GEM
  remote: https://rubygems.org/
  specs:
    activesupport (7.1.3)
      base64
      concurrent-ruby (~> 1.0, >= 1.0.2)
    base64 (0.2.0)
    concurrent-ruby (1.2.3)

PLATFORMS
  ruby

DEPENDENCIES
  activesupport

BUNDLED WITH
   2.5.3
"#;
        let doc = parse_gemfile_lock(text);
        assert_eq!(doc.specs.len(), 3);
        let active = doc
            .specs
            .iter()
            .find(|s| s.name == "activesupport")
            .expect("activesupport spec");
        assert_eq!(active.version, "7.1.3");
        // Transitive deps captured from indent-6 lines. Milestone 162
        // (T003): `depends` is now `Vec<GemDep>` — compare names only
        // for this test.
        let dep_names: Vec<String> =
            active.depends.iter().map(|d| d.name.clone()).collect();
        assert_eq!(
            dep_names,
            vec!["base64".to_string(), "concurrent-ruby".to_string()],
        );
        // Leaf specs carry empty depends.
        let base64 = doc.specs.iter().find(|s| s.name == "base64").unwrap();
        assert!(base64.depends.is_empty());
        assert_eq!(doc.dependencies, vec!["activesupport".to_string()]);
    }

    #[test]
    fn captures_per_spec_transitive_deps_with_constraints_stripped() {
        let text = r#"
GEM
  specs:
    foo (1.0.0)
      activesupport (~> 7.0, >= 7.0.1)
      base64
      concurrent-ruby (>= 1.0.2, < 2.0)
    activesupport (7.1.3)
    base64 (0.2.0)
    concurrent-ruby (1.2.3)
"#;
        let doc = parse_gemfile_lock(text);
        let foo = doc.specs.iter().find(|s| s.name == "foo").unwrap();
        let dep_names: Vec<String> = foo.depends.iter().map(|d| d.name.clone()).collect();
        assert_eq!(
            dep_names,
            vec![
                "activesupport".to_string(),
                "base64".to_string(),
                "concurrent-ruby".to_string(),
            ],
        );
    }

    #[test]
    fn transitive_deps_deduplicate_within_a_spec() {
        // A lockfile that declared the same dep twice under one gem —
        // make sure we don't emit two edges. Unusual in practice but
        // cheap defensive check.
        let text = r#"
GEM
  specs:
    foo (1.0.0)
      bar
      bar
    bar (0.1.0)
"#;
        let doc = parse_gemfile_lock(text);
        let foo = doc.specs.iter().find(|s| s.name == "foo").unwrap();
        let dep_names: Vec<String> = foo.depends.iter().map(|d| d.name.clone()).collect();
        assert_eq!(dep_names, vec!["bar".to_string()]);
    }

    #[test]
    fn parses_git_section() {
        let text = r#"
GIT
  remote: https://github.com/rails/rails.git
  revision: abc123
  specs:
    rails (7.2.0.alpha)

DEPENDENCIES
  rails!
"#;
        let doc = parse_gemfile_lock(text);
        assert_eq!(doc.specs.len(), 1);
        assert_eq!(doc.specs[0].kind, GemSection::Git);
    }

    #[test]
    fn parses_path_section() {
        let text = r#"
PATH
  remote: ../vendor/my-gem
  specs:
    my-gem (0.1.0)

DEPENDENCIES
  my-gem!
"#;
        let doc = parse_gemfile_lock(text);
        assert_eq!(doc.specs.len(), 1);
        assert_eq!(doc.specs[0].kind, GemSection::Path);
    }

    #[test]
    fn ignores_constraint_lines() {
        // Lines like `activesupport (~> 7.0)` should NOT appear as specs.
        let text = r#"
GEM
  specs:
    foo (1.0.0)
      activesupport (~> 7.0, >= 7.0.1)
      base64 (>= 0.1.0)
    activesupport (7.1.3)
"#;
        let doc = parse_gemfile_lock(text);
        let names: Vec<_> = doc.specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"activesupport"));
        assert!(!names.contains(&"base64")); // base64 never listed at indent 4
    }

    #[test]
    fn dependencies_block_strips_pin_suffix() {
        let text = r#"
DEPENDENCIES
  rails!
  activesupport
  rspec (~> 3.13)
"#;
        let doc = parse_gemfile_lock(text);
        assert_eq!(
            doc.dependencies,
            vec!["rails".to_string(), "activesupport".to_string(), "rspec".to_string()],
        );
    }

    #[test]
    fn read_empty_rootfs_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path(), false, &Default::default()).is_empty());
    }

    #[test]
    fn read_finds_gemfile_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    activesupport (7.1.3)\n\nDEPENDENCIES\n  activesupport\n",
        )
        .unwrap();
        let entries = read(dir.path(), false, &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "activesupport");
        assert_eq!(entries[0].purl.as_str(), "pkg:gem/activesupport@7.1.3");
    }

    #[test]
    fn git_spec_carries_source_type() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Gemfile.lock"),
            "GIT\n  remote: https://x/y\n  revision: abc\n  specs:\n    y (0.1.0)\n\nDEPENDENCIES\n  y!\n",
        )
        .unwrap();
        let entries = read(dir.path(), false, &Default::default());
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));
    }

    // --- gemspec walker (conformance bug 3) ----------------------------

    #[test]
    fn parse_gemspec_simple_name_version() {
        // Canonical shape from `gem build` output.
        let text = r#"# -*- encoding: utf-8 -*-
Gem::Specification.new do |s|
  s.name = "json"
  s.version = "2.7.2"
  s.authors = ["foo"]
end
"#;
        let (name, version) = parse_gemspec(text).unwrap();
        assert_eq!(name, "json");
        assert_eq!(version, "2.7.2");
    }

    #[test]
    fn parse_gemspec_gem_version_new_form() {
        // Common alternative — Ruby stdlib default gems emit this.
        let text = r#"Gem::Specification.new do |s|
  s.name = "bundler"
  s.version = Gem::Version.new "4.0.10"
end
"#;
        let (name, version) = parse_gemspec(text).unwrap();
        assert_eq!(name, "bundler");
        assert_eq!(version, "4.0.10");
    }

    #[test]
    fn parse_gemspec_spec_receiver_and_freeze() {
        // `spec.` receiver (vs `s.`) and `.freeze` suffix both occur.
        let text = r#"Gem::Specification.new do |spec|
  spec.name = "psych".freeze
  spec.version = "5.1.2".freeze
end
"#;
        let (name, version) = parse_gemspec(text).unwrap();
        assert_eq!(name, "psych");
        assert_eq!(version, "5.1.2");
    }

    #[test]
    fn parse_gemspec_single_quoted() {
        let text = r#"Gem::Specification.new do |s|
  s.name = 'rdoc'
  s.version = '6.6.3.1'
end
"#;
        let (name, version) = parse_gemspec(text).unwrap();
        assert_eq!(name, "rdoc");
        assert_eq!(version, "6.6.3.1");
    }

    #[test]
    fn parse_gemspec_full_extracts_authors_array() {
        let text = r#"Gem::Specification.new do |s|
  s.name = "rake"
  s.version = "13.0.6"
  s.authors = ["Hiroshi SHIBATA", "Eric Hodel", "Jim Weirich"]
end
"#;
        let spec = parse_gemspec_full(text).unwrap();
        assert_eq!(spec.name, "rake");
        assert_eq!(
            spec.authors.as_deref(),
            Some("Hiroshi SHIBATA, Eric Hodel, Jim Weirich"),
        );
    }

    #[test]
    fn parse_gemspec_full_extracts_singular_author() {
        let text = r#"Gem::Specification.new do |s|
  s.name = "solo"
  s.version = "1.0.0"
  s.author = "Solo Dev"
end
"#;
        let spec = parse_gemspec_full(text).unwrap();
        assert_eq!(spec.authors.as_deref(), Some("Solo Dev"));
    }

    #[test]
    fn parse_gemspec_full_no_authors_field_is_none() {
        let text = r#"Gem::Specification.new do |s|
  s.name = "noauth"
  s.version = "1.0"
end
"#;
        let spec = parse_gemspec_full(text).unwrap();
        assert!(spec.authors.is_none());
    }

    #[test]
    fn gemspec_to_entry_populates_maintainer_from_authors() {
        let entry = gemspec_to_entry(
            "rake",
            "13.0.6",
            Some("Hiroshi SHIBATA, Eric Hodel"),
            "/test.gemspec",
        )
        .unwrap();
        assert_eq!(
            entry.maintainer.as_deref(),
            Some("Hiroshi SHIBATA, Eric Hodel"),
        );
    }

    #[test]
    fn parse_gemspec_rejects_when_name_missing() {
        let text = r#"Gem::Specification.new do |s|
  s.version = "1.0"
end
"#;
        assert!(parse_gemspec(text).is_none());
    }

    #[test]
    fn parse_gemspec_handles_interpolated_version() {
        // Ruby `#{}` interpolation means we can't resolve without
        // executing the gemspec. The string-literal extractor still
        // captures the raw `#{VAR}` contents — downstream PURL
        // construction may fail on non-alphanumerics, in which case
        // the caller skips. This test documents current behavior.
        let text = "Gem::Specification.new do |s|\n  s.name = \"foo\"\n  s.version = \"#{FOO_VERSION}\"\nend\n";
        let result = parse_gemspec(text);
        if let Some((_, v)) = result {
            assert!(v.contains('#') || v.contains('{'));
        }
    }

    #[test]
    fn find_gemspecs_walks_default_specs_dir() {
        // Simulate a Ruby install tree:
        //   usr/lib/ruby/gems/3.3.0/specifications/default/json-2.7.2.gemspec
        //   usr/lib/ruby/gems/3.3.0/specifications/psych-5.1.2.gemspec
        let dir = tempfile::tempdir().unwrap();
        let specs = dir.path().join("usr/lib/ruby/gems/3.3.0/specifications");
        std::fs::create_dir_all(specs.join("default")).unwrap();
        std::fs::write(
            specs.join("default/json-2.7.2.gemspec"),
            "Gem::Specification.new do |s|\n  s.name = \"json\"\n  s.version = \"2.7.2\"\nend\n",
        )
        .unwrap();
        std::fs::write(
            specs.join("psych-5.1.2.gemspec"),
            "Gem::Specification.new do |s|\n  s.name = \"psych\"\n  s.version = \"5.1.2\"\nend\n",
        )
        .unwrap();
        let found =
            find_gemspecs(dir.path(), &super::super::exclude_path::ExclusionSet::new_empty());
        assert_eq!(found.len(), 2, "expected two gemspecs, got {found:?}");
    }

    #[test]
    fn read_returns_installed_gems_without_gemfile_lock() {
        let dir = tempfile::tempdir().unwrap();
        let specs = dir.path().join("usr/lib/ruby/gems/3.3.0/specifications/default");
        std::fs::create_dir_all(&specs).unwrap();
        std::fs::write(
            specs.join("bigdecimal-3.1.5.gemspec"),
            "Gem::Specification.new do |s|\n  s.name = \"bigdecimal\"\n  s.version = \"3.1.5\"\nend\n",
        )
        .unwrap();
        let entries = read(dir.path(), false, &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "bigdecimal");
        assert_eq!(entries[0].version, "3.1.5");
        assert_eq!(entries[0].source_type.as_deref(), Some("installed-gemspec"));
        assert_eq!(entries[0].purl.as_str(), "pkg:gem/bigdecimal@3.1.5");
    }

    #[test]
    fn gemfile_lock_wins_over_gemspec_for_same_gem() {
        // Dedup: if a gem appears in both Gemfile.lock and a
        // specifications/*.gemspec, the Gemfile.lock version wins
        // (Gemfile.lock processed first and seen_purls blocks the
        // gemspec from being re-added).
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Gemfile.lock"),
            "GEM\n  remote: https://rubygems.org/\n  specs:\n    json (2.7.1)\n\nDEPENDENCIES\n  json\n",
        )
        .unwrap();
        let specs = dir.path().join("usr/lib/ruby/gems/3.3.0/specifications/default");
        std::fs::create_dir_all(&specs).unwrap();
        std::fs::write(
            specs.join("json-2.7.2.gemspec"),
            "Gem::Specification.new do |s|\n  s.name = \"json\"\n  s.version = \"2.7.2\"\nend\n",
        )
        .unwrap();
        let entries = read(dir.path(), false, &Default::default());
        // Two distinct PURLs — different versions so they're distinct
        // packages, both emitted. This is correct: two different
        // versions of json are installed.
        let json_entries: Vec<_> =
            entries.iter().filter(|e| e.name == "json").collect();
        assert_eq!(json_entries.len(), 2);
    }

    // ---- Milestone 051 — gem dev/test group classification ----

    #[test]
    fn parse_gemfile_extracts_group_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Gemfile");
        std::fs::write(
            &path,
            r#"
source "https://rubygems.org"

gem "rack"

group :test do
  gem "rspec"
  gem "factory_bot"
end

group :development, :test do
  gem "pry"
end
"#,
        )
        .unwrap();
        let groups = parse_gemfile(&path);
        assert!(groups.get("rack").map(|g| g.is_empty()).unwrap_or(false));
        assert!(groups.get("rspec").unwrap().contains("test"));
        assert!(groups.get("factory_bot").unwrap().contains("test"));
        let pry_groups = groups.get("pry").unwrap();
        assert!(pry_groups.contains("development"));
        assert!(pry_groups.contains("test"));
    }

    #[test]
    fn parse_gemfile_extracts_inline_group_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Gemfile");
        std::fs::write(
            &path,
            r#"
gem "rack"
gem "byebug", group: :development
gem "minitest", groups: [:test, :ci]
"#,
        )
        .unwrap();
        let groups = parse_gemfile(&path);
        assert!(groups.get("byebug").unwrap().contains("development"));
        let minitest = groups.get("minitest").unwrap();
        assert!(minitest.contains("test"));
        assert!(minitest.contains("ci"));
    }

    #[test]
    fn parse_gemspec_groups_extracts_dev_deps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo.gemspec");
        std::fs::write(
            &path,
            r#"
Gem::Specification.new do |s|
  s.name = "foo"
  s.version = "0.1.0"
  s.add_dependency "activesupport", "~> 7.0"
  s.add_runtime_dependency "json"
  s.add_development_dependency "rspec", "~> 3.0"
  s.add_development_dependency("factory_bot")
end
"#,
        )
        .unwrap();
        let groups = parse_gemspec_groups(&path);
        assert!(groups.get("activesupport").map(|g| g.is_empty()).unwrap_or(false));
        assert!(groups.get("json").map(|g| g.is_empty()).unwrap_or(false));
        assert!(groups.get("rspec").unwrap().contains("development"));
        assert!(groups.get("factory_bot").unwrap().contains("development"));
    }

    #[test]
    fn parse_gemfile_returns_empty_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let groups = parse_gemfile(&dir.path().join("NoSuchFile"));
        assert!(groups.is_empty());
    }

    #[test]
    fn parse_gemspec_groups_returns_empty_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let groups = parse_gemspec_groups(&dir.path().join("nope.gemspec"));
        assert!(groups.is_empty());
    }

    #[test]
    fn compute_gem_prod_set_walks_three_level_chain() {
        let lock = GemfileLockDocument {
            specs: vec![
                GemSpec {
                    name: "a".to_string(),
                    version: "1".to_string(),
                    kind: GemSection::Gem,
                    depends: vec![GemDep { name: "b".to_string(), requirement: String::new() }],
                },
                GemSpec {
                    name: "b".to_string(),
                    version: "1".to_string(),
                    kind: GemSection::Gem,
                    depends: vec![GemDep { name: "c".to_string(), requirement: String::new() }],
                },
                GemSpec {
                    name: "c".to_string(),
                    version: "1".to_string(),
                    kind: GemSection::Gem,
                    depends: vec![],
                },
            ],
            dependencies: vec!["a".to_string()],
        };
        let mut direct = HashSet::new();
        direct.insert("a".to_string());
        let prod = compute_gem_prod_set(&direct, &lock);
        assert!(prod.contains("a"));
        assert!(prod.contains("b"));
        assert!(prod.contains("c"));
    }

    /// Milestone 054 SC-002 + FR-009: walker terminates promptly on
    /// a synthesized minimal symlink-loop fixture instead of hanging.
    /// Covers both gem walkers (find_gemfile_locks + find_gemspecs).
    ///
    /// Milestone 100: `#[cfg(unix)]` — POSIX-only symlink API.
    #[cfg(unix)]
    #[test]
    fn walks_symlink_loop_without_hanging() {
        let tmp = tempfile::tempdir().unwrap();
        let loop_dir = tmp.path().join("loop");
        std::fs::create_dir_all(&loop_dir).unwrap();
        std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();
        let empty = super::super::exclude_path::ExclusionSet::new_empty();
        let locks = find_gemfile_locks(tmp.path(), &empty);
        let specs = find_gemspecs(tmp.path(), &empty);
        assert!(locks.is_empty());
        assert!(specs.is_empty());
    }

    // -------------------------------------------------------------------
    // Milestone 069 — main-module emission helpers (T007)
    // -------------------------------------------------------------------

    fn write_gemspec(dir: &std::path::Path, filename: &str, contents: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(filename);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn build_gem_main_module_literal_name_and_version() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_gemspec(
            tmp.path(),
            "foo.gemspec",
            r#"
Gem::Specification.new do |s|
  s.name    = "foo"
  s.version = "1.2.3"
end
"#,
        );
        let entry = build_gem_main_module_entry(&path).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:gem/foo@1.2.3");
        assert_eq!(entry.name, "foo");
        assert_eq!(entry.version, "1.2.3");
        assert_eq!(entry.parent_purl, None);
        assert_eq!(entry.sbom_tier.as_deref(), Some("source"));
        assert_eq!(
            entry
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );
    }

    #[test]
    fn build_gem_main_module_non_literal_version_falls_back_to_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_gemspec(
            tmp.path(),
            "bar.gemspec",
            r#"
Gem::Specification.new do |s|
  s.name    = "bar"
  s.version = Bar::VERSION
end
"#,
        );
        let entry = build_gem_main_module_entry(&path).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:gem/bar@0.0.0-unknown");
        assert_eq!(entry.version, "0.0.0-unknown");
    }

    #[test]
    fn build_gem_main_module_freeze_chained_literal_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_gemspec(
            tmp.path(),
            "freezy.gemspec",
            r#"
Gem::Specification.new do |s|
  s.name    = "freezy".freeze
  s.version = "2.0.0".freeze
end
"#,
        );
        let entry = build_gem_main_module_entry(&path).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:gem/freezy@2.0.0");
        assert_eq!(entry.name, "freezy");
        assert_eq!(entry.version, "2.0.0");
    }

    #[test]
    fn build_gem_main_module_unparseable_name_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_gemspec(
            tmp.path(),
            "noname.gemspec",
            r#"
Gem::Specification.new do |s|
  s.version = "1.0.0"
  # name is set dynamically — no fallback identity available
  s.name = compute_name()
end
"#,
        );
        // No literal `s.name = "..."` → return None per FR-001 step 3.
        assert!(build_gem_main_module_entry(&path).is_none());
    }

    #[test]
    fn find_top_level_gemspecs_excludes_install_state_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Top-level gemspec — must be discovered.
        write_gemspec(root, "myproj.gemspec", r#"
Gem::Specification.new do |s|
  s.name = "myproj"
  s.version = "1.0.0"
end
"#);
        // Install-state paths — must be skipped per FR-003.
        for skip_parent in ["vendor", "gems", "specifications", ".bundle"] {
            write_gemspec(
                &root.join(skip_parent),
                "shadow.gemspec",
                r#"
Gem::Specification.new do |s|
  s.name = "shadow"
  s.version = "9.9.9"
end
"#,
            );
        }
        let found = find_top_level_gemspecs(root);
        let names: Vec<&str> = found
            .iter()
            .filter_map(|p| p.file_name().and_then(|s| s.to_str()))
            .collect();
        assert!(names.contains(&"myproj.gemspec"), "expected myproj.gemspec; got {names:?}");
        assert!(
            !names.contains(&"shadow.gemspec"),
            "shadow.gemspec inside install-state path must be excluded; got {names:?}"
        );
    }

    fn make_gem_main_module_entry(name: &str, version: &str, source_path: &str) -> PackageDbEntry {
        let purl = build_gem_purl(name, version).unwrap();
        let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        extra.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.to_string(),
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
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations: extra,
            binary_role: None,
        }
    }

    #[test]
    fn dedup_gem_main_modules_no_collision_returns_empty() {
        let mut entries = vec![
            make_gem_main_module_entry("a", "1.0.0", "/tmp/a.gemspec"),
            make_gem_main_module_entry("b", "1.0.0", "/tmp/b.gemspec"),
        ];
        let drops = dedup_gem_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 2);
        assert!(drops.is_empty());
    }

    #[test]
    fn dedup_gem_main_modules_two_same_purl_keeps_first() {
        let mut entries = vec![
            make_gem_main_module_entry("foo", "1.2.3", "/tmp/proj/foo.gemspec"),
            make_gem_main_module_entry("foo", "1.2.3", "/tmp/proj/dup/foo.gemspec"),
        ];
        let drops = dedup_gem_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_path, "/tmp/proj/foo.gemspec");
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].dropped_path, "/tmp/proj/dup/foo.gemspec");
    }

    // ====================================================================
    // Milestone 162 (T013–T023, T025–T027): Ruby built-in gem allowlist
    // + synthetic emission + SC-004 dual invariant tests
    // ====================================================================

    fn synth_gem_spec(name: &str, version: &str, deps: &[(&str, &str)]) -> GemSpec {
        GemSpec {
            name: name.to_string(),
            version: version.to_string(),
            kind: GemSection::Gem,
            depends: deps
                .iter()
                .map(|(n, r)| GemDep {
                    name: n.to_string(),
                    requirement: r.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn t013_allowlist_contains_bundler() {
        // SC-009 (a): bundler is Ruby-toolchain-provided; MUST be in
        // the FR-001 allowlist.
        assert!(is_ruby_built_in_gem("bundler"));
    }

    #[test]
    fn t014_allowlist_excludes_thor() {
        // SC-009 (b): thor is a real gem (not Ruby-toolchain-provided);
        // MUST NOT be in the allowlist.
        assert!(!is_ruby_built_in_gem("thor"));
    }

    #[test]
    fn t015_allowlist_contains_csv() {
        // Verifies coverage of csv — a gem promoted to Ruby 3.4
        // default_gems that older projects still reference. Union
        // strategy (per Q2) MUST include it.
        assert!(is_ruby_built_in_gem("csv"));
    }

    #[test]
    fn t016_parser_preserves_version_constraint() {
        // SC-009 (f): the parser preserves the (>= 1.2.0) clause
        // (parens stripped) on GemSpec.depends[i].requirement.
        let text = "\
GEM
  specs:
    bundler-audit (0.9.3)
      bundler (>= 1.2.0)
      thor (~> 1.0)

DEPENDENCIES
  bundler-audit
";
        let doc = parse_gemfile_lock(text);
        let spec = doc
            .specs
            .iter()
            .find(|s| s.name == "bundler-audit")
            .expect("bundler-audit spec");
        let bundler_dep = spec
            .depends
            .iter()
            .find(|d| d.name == "bundler")
            .expect("bundler dep");
        assert_eq!(bundler_dep.requirement, ">= 1.2.0");
        let thor_dep = spec
            .depends
            .iter()
            .find(|d| d.name == "thor")
            .expect("thor dep");
        assert_eq!(thor_dep.requirement, "~> 1.0");
    }

    #[test]
    fn t017_synthetic_component_versionless_purl() {
        // SC-009 (d) + FR-003: emitted synthetic component's PURL is
        // exactly `pkg:gem/bundler` — no `@` symbol.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![synth_gem_spec(
            "bundler-audit",
            "0.9.3",
            &[("bundler", ">= 1.2.0")],
        )];
        let names = append_synthetic_built_in_gems(
            &mut out,
            &HashSet::new(),
            "/tmp/Gemfile.lock",
            &specs,
        );
        assert_eq!(names, vec!["bundler".to_string()]);
        assert_eq!(out.len(), 1);
        let entry = &out[0];
        assert_eq!(entry.purl.as_str(), "pkg:gem/bundler");
        assert!(!entry.purl.as_str().contains('@'));
        assert_eq!(entry.version, "");
    }

    #[test]
    fn t018_synthetic_component_carries_c113() {
        // SC-009 (e): C113 waybill:synthetic-built-in = "ruby".
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![synth_gem_spec(
            "bundler-audit",
            "0.9.3",
            &[("bundler", ">= 1.2.0")],
        )];
        append_synthetic_built_in_gems(&mut out, &HashSet::new(), "/tmp", &specs);
        assert_eq!(out.len(), 1);
        let entry = &out[0];
        assert_eq!(
            entry.extra_annotations.get("waybill:synthetic-built-in"),
            Some(&serde_json::Value::String("ruby".to_string()))
        );
    }

    #[test]
    fn t019_synthetic_component_carries_c114_single_source() {
        // SC-009 (f): C114 waybill:built-in-requirement carries the
        // Gemfile.lock version-constraint (single source → bare string).
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![synth_gem_spec(
            "bundler-audit",
            "0.9.3",
            &[("bundler", ">= 1.2.0")],
        )];
        append_synthetic_built_in_gems(&mut out, &HashSet::new(), "/tmp", &specs);
        let entry = &out[0];
        assert_eq!(
            entry.extra_annotations.get("waybill:built-in-requirement"),
            Some(&serde_json::Value::String(">= 1.2.0".to_string()))
        );
    }

    #[test]
    fn t020_real_gem_takes_precedence() {
        // SC-009 (g) + FR-004: if `bundler` is already in `emitted_names`
        // (as a real GEM/specs entry), NO synthetic component is
        // appended for that name.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let mut emitted_names: HashSet<String> = HashSet::new();
        emitted_names.insert("bundler".to_string());
        let specs = vec![synth_gem_spec(
            "bundler-audit",
            "0.9.3",
            &[("bundler", ">= 1.2.0")],
        )];
        let names = append_synthetic_built_in_gems(
            &mut out,
            &emitted_names,
            "/tmp",
            &specs,
        );
        assert!(names.is_empty());
        assert!(out.is_empty());
    }

    #[test]
    fn t021_multi_source_dedup_single_component() {
        // SC-009 (h): 3 specs each declare `bundler` → exactly ONE
        // synthetic entry appended.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![
            synth_gem_spec("gem-a", "1.0.0", &[("bundler", ">= 1.0")]),
            synth_gem_spec("gem-b", "1.0.0", &[("bundler", ">= 2.0")]),
            synth_gem_spec("gem-c", "1.0.0", &[("bundler", ">= 1.0")]),
        ];
        let names = append_synthetic_built_in_gems(
            &mut out,
            &HashSet::new(),
            "/tmp",
            &specs,
        );
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "bundler");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn t022_multi_source_requirement_union_as_json_array() {
        // R4: multi-source with distinct requirements → JSON array of
        // sorted, deduplicated constraints.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![
            synth_gem_spec("gem-a", "1.0.0", &[("bundler", ">= 1.2.0")]),
            synth_gem_spec("gem-b", "1.0.0", &[("bundler", ">= 2.0.0")]),
        ];
        append_synthetic_built_in_gems(&mut out, &HashSet::new(), "/tmp", &specs);
        let entry = &out[0];
        let req = entry
            .extra_annotations
            .get("waybill:built-in-requirement")
            .expect("C114 present");
        // BTreeSet sorts alphabetically: `">= 1.2.0"` < `">= 2.0.0"`.
        let expected = serde_json::Value::Array(vec![
            serde_json::Value::String(">= 1.2.0".to_string()),
            serde_json::Value::String(">= 2.0.0".to_string()),
        ]);
        assert_eq!(req, &expected);
    }

    #[test]
    fn t023_non_allowlist_dangling_target_dropped() {
        // SC-009 (j) + FR-008: unknown dep-name (not in allowlist)
        // does NOT trigger synthetic emission.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![synth_gem_spec(
            "gem-a",
            "1.0.0",
            &[("some-unknown-gem", ">= 1.0")],
        )];
        let names = append_synthetic_built_in_gems(
            &mut out,
            &HashSet::new(),
            "/tmp",
            &specs,
        );
        assert!(names.is_empty());
        assert!(out.is_empty());
    }

    // --- Milestone 162 US2 (T025–T027): SC-004 dual invariant ---------

    #[test]
    fn t025_synthetic_component_has_no_at_symbol() {
        // SC-004: synthetic component's PURL has NO `@` — versionless.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        let specs = vec![synth_gem_spec("gem-a", "1.0.0", &[("bundler", "")])];
        append_synthetic_built_in_gems(&mut out, &HashSet::new(), "/tmp", &specs);
        assert!(!out[0].purl.as_str().contains('@'));
    }

    #[test]
    fn t026_real_gem_carries_at_version_and_no_c113() {
        // SC-004 complement: a real GEM/specs entry emitted via
        // spec_to_entry has `@version` in PURL AND does NOT carry the
        // waybill:synthetic-built-in annotation.
        let spec = synth_gem_spec("bundler-audit", "0.9.3", &[]);
        let entry = spec_to_entry(&spec, "/tmp", &HashSet::new())
            .expect("real entry constructed");
        assert!(entry.purl.as_str().contains("@0.9.3"));
        assert!(!entry
            .extra_annotations
            .contains_key("waybill:synthetic-built-in"));
    }

    #[test]
    fn t027_dual_invariant_holds_for_mixed_output() {
        // SC-004: iterate mixed real + synthetic components; assert the
        // dual invariant `has @` XOR `has C113 annotation`.
        let mut out: Vec<PackageDbEntry> = Vec::new();
        // Real components
        let real_specs = [
            synth_gem_spec("bundler-audit", "0.9.3", &[("bundler", ">= 1.2.0")]),
            synth_gem_spec("thor", "1.4.0", &[]),
        ];
        for spec in &real_specs {
            let entry = spec_to_entry(spec, "/tmp", &HashSet::new()).unwrap();
            out.push(entry);
        }
        // Synthetic emission for `bundler` (referenced by bundler-audit
        // but not in `out`).
        let emitted_names: HashSet<String> = out
            .iter()
            .filter(|e| e.purl.as_str().starts_with("pkg:gem/"))
            .map(|e| e.name.clone())
            .collect();
        append_synthetic_built_in_gems(&mut out, &emitted_names, "/tmp", &real_specs);
        // Now iterate all pkg:gem/* components and verify the dual
        // invariant.
        for entry in &out {
            let has_at = entry.purl.as_str().contains('@');
            let has_c113 = entry
                .extra_annotations
                .contains_key("waybill:synthetic-built-in");
            assert_ne!(
                has_at, has_c113,
                "SC-004 dual invariant violated for {}: has_at={has_at}, has_c113={has_c113}",
                entry.purl.as_str()
            );
        }
        // Verify we actually have 3 entries: 2 real + 1 synthetic.
        assert_eq!(out.len(), 3);
    }

    // ── Milestone 191 (#558) — build_gem_purl versionless shape ──

    #[test]
    fn build_gem_purl_empty_version_emits_versionless_shape() {
        let p = build_gem_purl("rails", "").expect("empty-version permitted");
        assert_eq!(p.as_str(), "pkg:gem/rails");
    }

    #[test]
    fn build_gem_purl_nonempty_version_byte_identical_to_pre_m191() {
        let p = build_gem_purl("rails", "7.1.3").expect("non-empty");
        assert_eq!(p.as_str(), "pkg:gem/rails@7.1.3");
    }

    // -------- m216 walker + builder unit tests (T009) --------

    #[test]
    fn find_top_level_gemfiles_walks_gemfile_only_dirs() {
        let root = tempfile::tempdir().unwrap();
        // Gemfile-only dir (no gemspec) — SHOULD be walked.
        let app = root.path().join("my-app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();
        std::fs::write(app.join("Gemfile.lock"), b"# empty\n").unwrap();
        let found = find_top_level_gemfiles(root.path());
        assert_eq!(found.len(), 1, "expected 1 Gemfile, got {found:?}");
        assert_eq!(found[0], app.join("Gemfile"));
    }

    #[test]
    fn find_top_level_gemfiles_skips_gemspec_carrying_dirs() {
        let root = tempfile::tempdir().unwrap();
        let pub_gem = root.path().join("published-gem");
        std::fs::create_dir_all(&pub_gem).unwrap();
        std::fs::write(pub_gem.join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();
        std::fs::write(
            pub_gem.join("published-gem.gemspec"),
            b"Gem::Specification.new {|s| s.name = 'x'; s.version = '1.0.0'}\n",
        )
        .unwrap();
        let found = find_top_level_gemfiles(root.path());
        assert!(
            found.is_empty(),
            "FR-007: gemspec-carrying dirs must NOT emit application main-module; got {found:?}"
        );
    }

    #[test]
    fn find_top_level_gemfiles_skips_vendor_gems_bundle() {
        for excluded in ["vendor", "gems", ".bundle"] {
            let root = tempfile::tempdir().unwrap();
            let nested = root.path().join(excluded).join("nested-app");
            std::fs::create_dir_all(&nested).unwrap();
            std::fs::write(nested.join("Gemfile"), b"source 'x'\n").unwrap();
            let found = find_top_level_gemfiles(root.path());
            assert!(
                found.is_empty(),
                "Gemfile under {excluded}/ must be skipped; got {found:?}"
            );
        }
    }

    #[test]
    fn build_gem_application_main_module_purl_is_pkg_generic() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("my-service");
        std::fs::create_dir_all(&app).unwrap();
        let gemfile = app.join("Gemfile");
        std::fs::write(&gemfile, b"source 'x'\n").unwrap();
        let entry =
            build_gem_application_main_module_entry(&gemfile, root.path())
                .expect("build succeeds");
        assert!(
            entry.purl.as_str().starts_with("pkg:generic/my-service@"),
            "expected pkg:generic/my-service@..., got {}",
            entry.purl.as_str()
        );
    }

    #[test]
    fn build_gem_application_main_module_has_package_shape_annotation() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("my-service");
        std::fs::create_dir_all(&app).unwrap();
        let gemfile = app.join("Gemfile");
        std::fs::write(&gemfile, b"source 'x'\n").unwrap();
        let entry =
            build_gem_application_main_module_entry(&gemfile, root.path())
                .expect("build succeeds");
        assert_eq!(
            entry
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module")
        );
        assert_eq!(
            entry
                .extra_annotations
                .get("waybill:package-shape")
                .and_then(|v| v.as_str()),
            Some("application")
        );
    }

    #[test]
    fn build_gem_application_main_module_falls_back_to_unknown_version() {
        // Non-git tempdir → git-describe ladder returns None at both
        // steps → fallback to "0.0.0-unknown".
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("my-app");
        std::fs::create_dir_all(&app).unwrap();
        let gemfile = app.join("Gemfile");
        std::fs::write(&gemfile, b"source 'x'\n").unwrap();
        let entry =
            build_gem_application_main_module_entry(&gemfile, root.path())
                .expect("build succeeds");
        assert_eq!(entry.version, "0.0.0-unknown");
        assert!(
            entry.purl.as_str().ends_with("@0.0.0-unknown"),
            "PURL version must reflect fallback: got {}",
            entry.purl.as_str()
        );
    }

    #[test]
    fn build_gem_application_main_module_applies_m215_slug_rules() {
        // Dir name with uppercase + spaces + non-ASCII — slug rules
        // should lowercase, strip whitespace, and strip non-ASCII.
        let root = tempfile::tempdir().unwrap();
        // Note: OS filesystems accept mixed-case names on most platforms
        // (macOS is HFS+ case-insensitive but preserving; Linux is
        // case-sensitive). We stick with a name that survives on both.
        let app = root.path().join("MyService_v2");
        std::fs::create_dir_all(&app).unwrap();
        let gemfile = app.join("Gemfile");
        std::fs::write(&gemfile, b"source 'x'\n").unwrap();
        let entry =
            build_gem_application_main_module_entry(&gemfile, root.path())
                .expect("build succeeds");
        // subject_slug lowercases; underscore is preserved (not in the
        // strip set).
        assert!(
            entry.name.starts_with("myservice_v2"),
            "expected lowercased name, got {}",
            entry.name
        );
    }
}