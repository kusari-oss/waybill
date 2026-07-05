//! Milestone 159 — pnpm/yarn npm-alias syntax detection (issue #493).
//!
//! Two grammar contracts:
//!
//! - **Pnpm v9 alias-VALUE syntax** — per contracts/pnpm-alias-grammar.md.
//!   A dep VALUE string of shape `<aliased-name>@<version>[(peer-suffix)]`
//!   where `<aliased-name>` differs from the enclosing dep's local-name.
//!   Examples:
//!     - Quoted: `react-helmet-async: '@slorber/react-helmet-async@1.3.0(peers)'`
//!     - Unquoted: `string-width-cjs: string-width@4.2.3`
//!
//! - **Yarn v1 key-side alias syntax** — per contracts/yarn-alias-grammar.md.
//!   A top-level entry key of shape
//!   `"local-name@range", "local-name@npm:aliased-name":` where any spec's
//!   version-part starts with `npm:`. Example:
//!     - `"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":`
//!
//! Both parsers produce the same [`AliasResolution`] output which is then
//! consumed by [`rewrite_dep_names`] for edge rewriting per FR-005.
//!
//! Under Q1 (2026-07-04): the annotation value is a raw string equal to
//! the local-name only. No envelope JSON, no peer-suffix payload.

use std::collections::HashMap;

/// Result of parsing a lockfile alias entry.
///
/// Emitted for both pnpm value-side aliases and yarn v1 key-side
/// aliases. Consumed by parse_pnpm_lock / parse_yarn_lock to:
///   1. Emit the component under the aliased canonical PURL
///      (FR-003).
///   2. Rewrite edges from local-name refs to aliased identity
///      (FR-005).
///   3. Populate the `mikebom:pnpm-alias` / `mikebom:yarn-alias`
///      annotation on the aliased component (FR-006).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasResolution {
    /// The dep name as it appears in the depender's lockfile section.
    /// Preserved byte-identically from the lockfile per FR-007.
    pub(crate) local_name: String,
    /// The actual npm-registry package name that the local name
    /// resolves to. Used as the emitted component's `name` field.
    pub(crate) aliased_name: String,
    /// The resolved version of the aliased package.
    pub(crate) aliased_version: String,
    /// The pnpm peer-suffix if present in the source lockfile value.
    /// Discarded per Q1 but retained in the type for future debugging.
    pub(crate) pnpm_peer_suffix: Option<String>,
    /// Which lockfile ecosystem the alias originated from.
    pub(crate) ecosystem: AliasEcosystem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasEcosystem {
    Pnpm,
    YarnV1,
}

#[allow(dead_code)]
impl AliasEcosystem {
    /// Wire annotation name per FR-006. Reserved for future emission-
    /// layer callers that need to dispatch by ecosystem; today the
    /// pnpm_lock.rs + yarn_lock.rs sites use the literal string.
    pub(crate) fn annotation_name(&self) -> &'static str {
        match self {
            Self::Pnpm => "mikebom:pnpm-alias",
            Self::YarnV1 => "mikebom:yarn-alias",
        }
    }

    /// Wire tracing-log ecosystem name per FR-011. Similarly reserved
    /// for future callers.
    pub(crate) fn log_name(&self) -> &'static str {
        match self {
            Self::Pnpm => "pnpm",
            Self::YarnV1 => "yarn",
        }
    }
}

/// Per-lockfile alias-resolution table used during the second-pass
/// edge rewrite.
pub(crate) type AliasMap = HashMap<String, AliasedIdentity>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasedIdentity {
    pub(crate) aliased_name: String,
    pub(crate) aliased_version: String,
}

/// Parse `<name>@<version>[(peers)]` into `(name, version, optional_peer_suffix)`.
///
/// Mirrors the semantics of pnpm_lock.rs::parse_pnpm_key — the LAST
/// `@` after any scope prefix is the version separator. Peer-suffix
/// is captured verbatim (with parentheses) if present.
fn split_pnpm_value(raw_value: &str) -> Option<(String, String, Option<String>)> {
    // Split off the peer-suffix at the first unmatched `(`.
    let (head, peer_suffix) = match raw_value.find('(') {
        Some(idx) => (&raw_value[..idx], Some(raw_value[idx..].to_string())),
        None => (raw_value, None),
    };
    // Find LAST `@` after any scope prefix in `head`.
    let search_start = if head.starts_with('@') { 1 } else { 0 };
    let at_idx = head[search_start..].rfind('@').map(|i| i + search_start)?;
    let name = head[..at_idx].to_string();
    let version = head[at_idx + 1..].to_string();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version, peer_suffix))
}

/// Detect a pnpm alias in a dep VALUE string per FR-001.
///
/// Returns `Some(AliasResolution)` when the value's canonical name
/// differs from `local_name`. Returns `None` when:
///   - The value fails to parse (grammar violation) — emits an FR-010
///     warn log for transparency.
///   - The value's canonical name matches `local_name` (no alias, or
///     self-referential value form like `foo: foo@1.0.0`).
///
/// The `source_path` param is only used for the FR-010 warn log.
pub(crate) fn detect_pnpm_alias(
    local_name: &str,
    raw_value: &str,
    source_path: &str,
) -> Option<AliasResolution> {
    // Guard against non-alias values that would spuriously trip
    // FR-010 warn logs:
    //   - Bare version strings (`4.2.3`, `1.0.0-beta`) — no `@`.
    //   - Peer-suffixed bare versions (`1.17.9(@algolia/client-search@5.42.0)`)
    //     — value starts with a digit + parens contain `@`s. Not an
    //     alias; the enclosing `(...)` is pnpm peer-dep context on the
    //     resolved version.
    // Only run alias detection when the value LOOKS like an alias
    // attempt — starts with a scope-prefix `@` OR the part BEFORE the
    // first `(` contains an `@` AND the head starts with a name-like
    // character (letter, `_`, or `-` — npm dep names can't start with
    // a digit per registry validation).
    if raw_value.is_empty() {
        return None;
    }
    let head_before_parens = raw_value
        .split('(')
        .next()
        .unwrap_or(raw_value);
    let has_at_in_head = head_before_parens
        .get(1..)
        .map(|s| s.contains('@'))
        .unwrap_or(false)
        || (head_before_parens.starts_with('@')
            && head_before_parens[1..].contains('/'));
    // Bare versions start with a digit. Scope prefixes start with `@`.
    // Names start with letter / underscore / hyphen.
    let starts_like_name = raw_value
        .chars()
        .next()
        .map(|c| c == '@' || c.is_ascii_alphabetic() || c == '_' || c == '-')
        .unwrap_or(false);
    if !has_at_in_head || !starts_like_name {
        return None;
    }
    let Some((aliased_name, aliased_version, pnpm_peer_suffix)) = split_pnpm_value(raw_value)
    else {
        // Genuinely malformed alias-shaped value → FR-010 warn.
        tracing::warn!(
            lockfile = %source_path,
            local_name = %local_name,
            raw_value = %raw_value,
            "npm-alias parse failed (skipping entry)"
        );
        return None;
    };
    if aliased_name == local_name {
        return None;
    }
    Some(AliasResolution {
        local_name: local_name.to_string(),
        aliased_name,
        aliased_version,
        pnpm_peer_suffix,
        ecosystem: AliasEcosystem::Pnpm,
    })
}

/// Strip a leading/trailing double-quote pair from a spec.
fn unquote(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

/// Split a single yarn v1 spec `"local-name@version-spec"` into its
/// `(local_name, version_spec)` parts.
///
/// Semantics per yarn v1 grammar:
///   - Unscoped: `local@version-spec` — split at the FIRST `@`.
///   - Scoped: `@scope/name@version-spec` — split at the FIRST `@` that
///     appears AFTER the scope's `/`. The scope's leading `@` and any
///     `@`s embedded in the version-spec (e.g. `npm:@cosmos.gl/graph`)
///     are NOT separators.
fn split_yarn_v1_spec(spec: &str) -> Option<(String, String)> {
    let raw = unquote(spec);
    if raw.is_empty() {
        return None;
    }
    let at_idx = if let Some(rest_after_scope_at) = raw.strip_prefix('@') {
        // Scoped: find the `/` in the scope, then the FIRST `@` after it.
        let slash_pos = rest_after_scope_at.find('/')?;
        let search_offset = 1 /* leading @ */ + slash_pos + 1;
        raw[search_offset..].find('@').map(|i| i + search_offset)?
    } else {
        // Unscoped: FIRST `@`.
        raw.find('@')?
    };
    let local = raw[..at_idx].to_string();
    let ver = raw[at_idx + 1..].to_string();
    if local.is_empty() || ver.is_empty() {
        return None;
    }
    Some((local, ver))
}

/// Detect a yarn v1 key-side alias per FR-002.
///
/// Scans the comma-joined `key_line` for any spec whose version-part
/// starts with `npm:`. When found, the substring after `npm:` is the
/// aliased-name; combined with `resolved_version` (from the entry's
/// `version:` body line), this produces the aliased-canonical
/// identity.
///
/// Returns `None` when no `npm:` marker is found or when the key
/// fails to parse — malformed keys emit an FR-010 warn log.
pub(crate) fn detect_yarn_v1_alias(
    key_line: &str,
    resolved_version: &str,
    source_path: &str,
) -> Option<AliasResolution> {
    // Strip a trailing `:` (the yarn v1 entry-header colon).
    let key = key_line.trim().trim_end_matches(':');
    if key.is_empty() {
        return None;
    }
    // Split on `,` to get each spec.
    let mut local_name: Option<String> = None;
    let mut aliased_name: Option<String> = None;
    for raw_spec in key.split(',') {
        let Some((local, ver)) = split_yarn_v1_spec(raw_spec) else {
            continue;
        };
        // Preserve the first-seen local-name so we can produce an
        // AliasResolution.
        if local_name.is_none() {
            local_name = Some(local.clone());
        }
        if let Some(rest) = ver.strip_prefix("npm:") {
            if rest.is_empty() {
                tracing::warn!(
                    lockfile = %source_path,
                    local_name = %local,
                    raw_value = %key_line,
                    "npm-alias parse failed (skipping entry)"
                );
                return None;
            }
            // The `npm:` suffix may carry a bare aliased-name
            // (`npm:@cosmos.gl/graph`) OR aliased-name + version-range
            // (`npm:string-width@^4.2.0`). Split at the LAST `@` after
            // any scope prefix to peel off the range — the resolved
            // version comes from `resolved_version` below anyway.
            let scope_start = if rest.starts_with('@') { 1 } else { 0 };
            let aliased_only = match rest.get(scope_start..) {
                Some(s) => match s.rfind('@') {
                    Some(idx) => rest[..idx + scope_start].to_string(),
                    None => rest.to_string(),
                },
                None => rest.to_string(),
            };
            aliased_name = Some(aliased_only);
            // Preserve local from THIS spec (not first-seen) to match
            // the spec that declared the alias.
            local_name = Some(local);
            break;
        }
    }
    let local_name = local_name?;
    let aliased_name = aliased_name?;
    if resolved_version.is_empty() {
        tracing::warn!(
            lockfile = %source_path,
            local_name = %local_name,
            raw_value = %key_line,
            "npm-alias parse failed (skipping entry)"
        );
        return None;
    }
    if aliased_name == local_name {
        // Not really an alias — self-reference.
        return None;
    }
    Some(AliasResolution {
        local_name,
        aliased_name,
        aliased_version: resolved_version.to_string(),
        pnpm_peer_suffix: None,
        ecosystem: AliasEcosystem::YarnV1,
    })
}

/// Rewrite a dep-name list to use aliased canonical names per FR-005.
///
/// For each dep-name in `dep_names`:
///   - Split on the FIRST space to separate `<name>` from an optional
///     `<version>` suffix (per milestone 164's version-qualified
///     disambiguation form emitted by `collect_pnpm_dep_names` when
///     `emit_versioned=true`).
///   - Look up `<name>` in `alias_map`. If present:
///       * With version → emit `format!("{aliased_name} {version}")`
///         (version preserved through alias substitution — m164 FR-003
///         alias-composition contract).
///       * Without version → emit bare `aliased_name` (pre-164
///         behavior preserved for the no-version case).
///   - Otherwise preserve byte-identically (both name and any version).
///
/// Preserves the input ordering.
pub(crate) fn rewrite_dep_names(dep_names: &[String], alias_map: &AliasMap) -> Vec<String> {
    dep_names
        .iter()
        .map(|dep| {
            let (name, version_opt) = match dep.find(' ') {
                Some(idx) => (&dep[..idx], Some(&dep[idx + 1..])),
                None => (dep.as_str(), None),
            };
            match alias_map.get(name) {
                Some(ident) => match version_opt {
                    Some(v) => format!("{} {}", ident.aliased_name, v),
                    None => ident.aliased_name.clone(),
                },
                None => dep.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // ─── T1 (SC-007 a/c/d): pnpm quoted-form alias, scoped aliased-name ───
    #[test]
    fn t1_pnpm_quoted_scoped_alias() {
        let r = detect_pnpm_alias(
            "react-helmet-async",
            "@slorber/react-helmet-async@1.3.0(react-dom@18.3.1(react@18.2.0))(react@18.2.0)",
            "/fake.yaml",
        )
        .expect("alias detected");
        assert_eq!(r.local_name, "react-helmet-async");
        assert_eq!(r.aliased_name, "@slorber/react-helmet-async");
        assert_eq!(r.aliased_version, "1.3.0");
        assert_eq!(
            r.pnpm_peer_suffix.as_deref(),
            Some("(react-dom@18.3.1(react@18.2.0))(react@18.2.0)")
        );
        assert_eq!(r.ecosystem, AliasEcosystem::Pnpm);
    }

    // ─── T2 (SC-007 b): pnpm unquoted-form alias ───
    #[test]
    fn t2_pnpm_unquoted_alias() {
        let r = detect_pnpm_alias("string-width-cjs", "string-width@4.2.3", "/fake.yaml")
            .expect("alias detected");
        assert_eq!(r.local_name, "string-width-cjs");
        assert_eq!(r.aliased_name, "string-width");
        assert_eq!(r.aliased_version, "4.2.3");
        assert!(r.pnpm_peer_suffix.is_none());
    }

    // ─── T3 (SC-007 c): pnpm alias with scoped LOCAL name ───
    #[test]
    fn t3_pnpm_alias_scoped_local_name() {
        let r = detect_pnpm_alias("@some-scope/name", "@other-scope/other@2.0.0", "/fake.yaml")
            .expect("alias detected");
        assert_eq!(r.local_name, "@some-scope/name");
        assert_eq!(r.aliased_name, "@other-scope/other");
        assert_eq!(r.aliased_version, "2.0.0");
    }

    // ─── T4: pnpm no-alias case ───
    #[test]
    fn t4_pnpm_no_alias_returns_none() {
        // Value's canonical name matches local — no alias.
        assert!(detect_pnpm_alias("foo", "foo@1.0.0", "/fake.yaml").is_none());
    }

    // ─── T5 (SC-007 h): malformed pnpm alias — warn + skip ───
    #[test]
    fn t5_pnpm_malformed_value_returns_none() {
        // No `@` at all in value → grammar violation.
        assert!(detect_pnpm_alias("foo", "not-a-purl", "/fake.yaml").is_none());
        // Empty value.
        assert!(detect_pnpm_alias("foo", "", "/fake.yaml").is_none());
        // Value is only `@` → both name and version end up empty.
        assert!(detect_pnpm_alias("foo", "@", "/fake.yaml").is_none());
    }

    // ─── T6 (SC-007 e): yarn v1 key with npm: alias, both scoped ───
    #[test]
    fn t6_yarn_v1_both_scoped_alias() {
        let key = r#""@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":"#;
        let r = detect_yarn_v1_alias(key, "2.6.4", "/fake.lock").expect("alias detected");
        assert_eq!(r.local_name, "@cosmograph/cosmos");
        assert_eq!(r.aliased_name, "@cosmos.gl/graph");
        assert_eq!(r.aliased_version, "2.6.4");
        assert_eq!(r.ecosystem, AliasEcosystem::YarnV1);
        assert!(r.pnpm_peer_suffix.is_none());
    }

    // ─── T7 (SC-007 f): unscoped local, scoped aliased ───
    #[test]
    fn t7_yarn_v1_unscoped_local_scoped_aliased() {
        let key = r#""string-width@^4.0.0", "string-width@npm:@shim/string-width":"#;
        let r = detect_yarn_v1_alias(key, "4.2.3", "/fake.lock").expect("alias detected");
        assert_eq!(r.local_name, "string-width");
        assert_eq!(r.aliased_name, "@shim/string-width");
        assert_eq!(r.aliased_version, "4.2.3");
    }

    // ─── T8 (SC-007 g): scoped local, unscoped aliased ───
    #[test]
    fn t8_yarn_v1_scoped_local_unscoped_aliased() {
        let key = r#""@scope/name@^1.0.0", "@scope/name@npm:real-name":"#;
        let r = detect_yarn_v1_alias(key, "1.0.5", "/fake.lock").expect("alias detected");
        assert_eq!(r.local_name, "@scope/name");
        assert_eq!(r.aliased_name, "real-name");
        assert_eq!(r.aliased_version, "1.0.5");
    }

    // ─── T9: yarn v1 no-alias case ───
    #[test]
    fn t9_yarn_v1_no_alias_returns_none() {
        assert!(detect_yarn_v1_alias(r#""lodash@^4.17.21":"#, "4.17.21", "/fake.lock").is_none());
    }

    // ─── T10 (SC-007 i): malformed yarn v1 key — warn + skip ───
    #[test]
    fn t10_yarn_v1_malformed_key_returns_none() {
        // `npm:` with no aliased-name following.
        assert!(detect_yarn_v1_alias(r#""foo@npm:":"#, "1.0.0", "/fake.lock").is_none());
        // Empty key line.
        assert!(detect_yarn_v1_alias("", "1.0.0", "/fake.lock").is_none());
        // Empty resolved version.
        assert!(detect_yarn_v1_alias(r#""foo@npm:bar":"#, "", "/fake.lock").is_none());
    }

    // ─── FR-007 byte-identity guard (SC-007 j, analyze finding A1) ───
    #[test]
    fn fr007_byte_identity_local_name_pnpm() {
        for local in [
            "string-width-cjs",             // unscoped-with-hyphens
            "@scope/name-with-hyphens",     // scoped-with-hyphens
            "some_alias_name",              // underscores (npm-legal)
            "ReactHelmetAsync",             // mixed-case (no lowercase norm)
        ] {
            let raw = "real-target-name@1.0.0";
            let r = detect_pnpm_alias(local, raw, "/fake.yaml")
                .expect("alias detected for byte-identity test");
            assert_eq!(r.local_name, local, "local_name byte-identity violated for {local}");
        }
    }

    #[test]
    fn fr007_byte_identity_local_name_yarn() {
        // Yarn variant — the local-name is embedded in the key.
        for local in [
            "string-width-cjs",
            "@scope/name-with-hyphens",
            "some_alias_name",
            "ReactHelmetAsync",
        ] {
            let key = format!(r#""{local}@^1.0.0", "{local}@npm:aliased-target":"#);
            let r = detect_yarn_v1_alias(&key, "1.0.0", "/fake.lock")
                .expect("alias detected for byte-identity test");
            assert_eq!(r.local_name, local, "local_name byte-identity violated for {local}");
        }
    }

    // ─── rewrite_dep_names tests (T009) ───

    fn make_alias_map(entries: &[(&str, &str, &str)]) -> AliasMap {
        let mut m = HashMap::new();
        for (local, aliased_name, aliased_version) in entries {
            m.insert(
                (*local).to_string(),
                AliasedIdentity {
                    aliased_name: (*aliased_name).to_string(),
                    aliased_version: (*aliased_version).to_string(),
                },
            );
        }
        m
    }

    #[test]
    fn rewrite_empty_dep_names() {
        let alias_map = make_alias_map(&[]);
        let out = rewrite_dep_names(&[], &alias_map);
        assert!(out.is_empty());
    }

    #[test]
    fn rewrite_no_matches_passthrough() {
        let alias_map = make_alias_map(&[("string-width-cjs", "string-width", "4.2.3")]);
        let deps: Vec<String> = vec!["foo".into(), "bar".into(), "baz".into()];
        let out = rewrite_dep_names(&deps, &alias_map);
        assert_eq!(out, deps); // byte-identical passthrough
    }

    #[test]
    fn rewrite_partial_matches_preserve_order() {
        let alias_map = make_alias_map(&[
            ("string-width-cjs", "string-width", "4.2.3"),
            ("strip-ansi-cjs", "strip-ansi", "6.0.1"),
        ]);
        let deps: Vec<String> = vec![
            "foo".into(),
            "string-width-cjs".into(),
            "bar".into(),
            "strip-ansi-cjs".into(),
        ];
        let out = rewrite_dep_names(&deps, &alias_map);
        assert_eq!(
            out,
            vec![
                "foo".to_string(),
                "string-width".to_string(),
                "bar".to_string(),
                "strip-ansi".to_string(),
            ]
        );
    }

    #[test]
    fn rewrite_multi_alias_both_rewritten() {
        let alias_map = make_alias_map(&[
            ("helmet-shim", "@slorber/react-helmet-async", "1.3.0"),
            ("react-helmet-async", "@slorber/react-helmet-async", "1.3.0"),
        ]);
        let deps: Vec<String> = vec!["helmet-shim".into(), "react-helmet-async".into()];
        let out = rewrite_dep_names(&deps, &alias_map);
        assert_eq!(
            out,
            vec![
                "@slorber/react-helmet-async".to_string(),
                "@slorber/react-helmet-async".to_string(),
            ]
        );
    }

    // ─── Milestone 164 (T007a): rewrite_dep_names must preserve the
    // version segment through alias substitution when milestone-164's
    // `emit_versioned=true` path puts `"<name> <version>"` into
    // `depends`. Closes the FR-003 alias-composition test gap.
    #[test]
    fn t007a_rewrite_dep_names_preserves_version() {
        let alias_map = make_alias_map(&[("foo", "@real/foo", "1.2.3")]);

        // Case (a): bare name + alias hit → aliased bare (no version to
        // preserve — pre-164 behavior).
        let out_bare = rewrite_dep_names(&["foo".to_string()], &alias_map);
        assert_eq!(out_bare, vec!["@real/foo".to_string()]);

        // Case (b): versioned form + alias hit → version preserved
        // through substitution (m164 FR-003).
        let out_versioned = rewrite_dep_names(&["foo 1.2.3".to_string()], &alias_map);
        assert_eq!(out_versioned, vec!["@real/foo 1.2.3".to_string()]);

        // Case (c): versioned form + no alias hit → passthrough
        // unchanged (byte-identical).
        let out_passthrough = rewrite_dep_names(&["baz 4.5.6".to_string()], &alias_map);
        assert_eq!(out_passthrough, vec!["baz 4.5.6".to_string()]);
    }

    // ─── AliasEcosystem helper tests ───
    #[test]
    fn alias_ecosystem_annotation_name() {
        assert_eq!(AliasEcosystem::Pnpm.annotation_name(), "mikebom:pnpm-alias");
        assert_eq!(AliasEcosystem::YarnV1.annotation_name(), "mikebom:yarn-alias");
    }

    #[test]
    fn alias_ecosystem_log_name() {
        assert_eq!(AliasEcosystem::Pnpm.log_name(), "pnpm");
        assert_eq!(AliasEcosystem::YarnV1.log_name(), "yarn");
    }
}
