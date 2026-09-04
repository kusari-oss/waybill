// milestone 770 - see specs/770-sbom-quality-corpus/plan.md
//
// T005: newtypes with validating constructors (Constitution IV — an
//       inverted range must be unrepresentable at runtime, not merely
//       rejected at the call site).
// T006: serde structs for the committed TOML corpus.
// T007: CorpusConfig::load performing every validation in contract
//       corpus-config.md § C-4, collecting ALL errors rather than the
//       first (FR-021).

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A corpus target's stable identity. Used as the cache directory name
/// and the report key, so it is deliberately distinct from the URL —
/// re-pointing a URL must not orphan a target's history.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TargetName(String);

impl TargetName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TargetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // f.pad, not f.write_str: a custom Display that writes directly
        // silently ignores width/alignment specifiers, which breaks the
        // report's column alignment.
        f.pad(&self.0)
    }
}

/// Inclusive integer range (FR-017). Construct via [`Range::new`] so
/// `min > max` cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct Range {
    pub min: u64,
    pub max: u64,
}

impl Range {
    pub fn new(min: u64, max: u64) -> Result<Self, String> {
        if min > max {
            return Err(format!("range min ({min}) exceeds max ({max})"));
        }
        Ok(Self { min, max })
    }

    /// Inclusive at both ends per FR-017.
    pub fn contains(&self, v: u64) -> bool {
        v >= self.min && v <= self.max
    }
}

/// Inclusive float range, for the 0.0–10.0 sbomqs score.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct RangeF {
    pub min: f64,
    pub max: f64,
}

impl RangeF {
    pub fn new(min: f64, max: f64) -> Result<Self, String> {
        if min > max {
            return Err(format!("range min ({min}) exceeds max ({max})"));
        }
        Ok(Self { min, max })
    }

    pub fn contains(&self, v: f64) -> bool {
        v >= self.min && v <= self.max
    }
}

/// How a target's revision is identified.
///
/// `Ref` exists from the outset purely so switching a target to a moving
/// branch later is a configuration change plus one match arm, never a
/// schema migration (FR-003). It is NOT implemented this milestone and is
/// rejected at parse time with an explicit message rather than silently
/// ignored (contract C-2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    Sha { hex: String },
    Ref { name: String },
}

impl Pin {
    /// First 12 characters of a SHA, or the ref name. Used as the
    /// `--root-version` passed to waybill and in the report path.
    pub fn short(&self) -> String {
        match self {
            Pin::Sha { hex } => hex.chars().take(12).collect(),
            Pin::Ref { name } => name.clone(),
        }
    }

    pub fn as_fetch_spec(&self) -> &str {
        match self {
            Pin::Sha { hex } => hex,
            Pin::Ref { name } => name,
        }
    }
}

/// Hand-authored bounds. Every field optional: absent means "observe,
/// never fail" (FR-020), which is what makes the corpus landable before
/// any range is written.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub wall_ms: Option<Range>,
    pub sbomqs: Option<RangeF>,
    pub pkgs: Option<Range>,
    pub files: Option<Range>,
    pub edges: Option<Range>,
    pub max_depth: Option<Range>,
    /// `true` asserts the graph is expected flat (legitimate for
    /// lockfile-less upstreams); `false` asserts it should have depth.
    pub flat: Option<bool>,
}

/// Raw TOML shape for one target. Converted to [`Target`] by `load`
/// after validation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTarget {
    name: String,
    url: String,
    sha: Option<String>,
    #[serde(rename = "ref")]
    ref_: Option<String>,
    ecosystem: Option<String>,
    timeout_secs: Option<u64>,
    expect: Option<Expectations>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: TargetName,
    pub url: String,
    pub pin: Pin,
    /// Documentation only — never gates. Lets a reader see coverage at a
    /// glance.
    pub ecosystem: String,
    pub timeout_secs: Option<u64>,
    pub expect: Option<Expectations>,
}

impl Target {
    /// Per-target budget, falling back to the corpus default.
    pub fn effective_timeout(&self, default_secs: u64) -> u64 {
        self.timeout_secs.unwrap_or(default_secs)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCorpus {
    sbomqs_version: String,
    #[serde(default = "default_timeout")]
    default_timeout_secs: u64,
    #[serde(default)]
    targets: Vec<RawTarget>,
}

fn default_timeout() -> u64 {
    600
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorpusConfig {
    pub sbomqs_version: String,
    pub default_timeout_secs: u64,
    pub targets: Vec<Target>,
}

/// Every configuration error found, reported together (FR-021). A
/// configuration error is a distinct failure class from a measurement
/// violation and exits 2, not 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigErrors(pub Vec<String>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "corpus configuration is invalid ({} problem(s)):", self.0.len())?;
        for e in &self.0 {
            writeln!(f, "  - {e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigErrors {}

impl CorpusConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigErrors> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            ConfigErrors(vec![format!("cannot read {}: {e}", path.display())])
        })?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, ConfigErrors> {
        let raw: RawCorpus = toml::from_str(text)
            .map_err(|e| ConfigErrors(vec![format!("TOML parse failed: {e}")]))?;

        let mut errs: Vec<String> = Vec::new();

        if raw.sbomqs_version.trim().is_empty() {
            errs.push("sbomqs_version must not be empty".into());
        }
        if raw.targets.is_empty() {
            errs.push("corpus contains no targets".into());
        }

        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut targets: Vec<Target> = Vec::new();

        for rt in &raw.targets {
            let who = &rt.name;
            if !seen.insert(rt.name.clone()) {
                errs.push(format!("duplicate target name: {who}"));
            }

            let pin = match (&rt.sha, &rt.ref_) {
                (Some(_), Some(_)) => {
                    errs.push(format!("{who}: specify exactly one of `sha` or `ref`, not both"));
                    continue;
                }
                (None, None) => {
                    errs.push(format!("{who}: missing `sha`"));
                    continue;
                }
                (None, Some(name)) => {
                    // FR-003 forward compatibility: the variant exists, the
                    // behaviour does not. Reject loudly (C-2.2).
                    errs.push(format!(
                        "{who}: moving-reference pins (`ref = \"{name}\"`) are not yet \
                         supported; use a 40-character `sha`"
                    ));
                    continue;
                }
                (Some(hex), None) => {
                    if !is_sha40(hex) {
                        errs.push(format!(
                            "{who}: `sha` must be 40 lowercase hex characters, got {hex:?}"
                        ));
                        continue;
                    }
                    Pin::Sha { hex: hex.clone() }
                }
            };

            if let Some(exp) = &rt.expect {
                validate_expectations(who, exp, &mut errs);
            }

            targets.push(Target {
                name: TargetName(rt.name.clone()),
                url: rt.url.clone(),
                pin,
                ecosystem: rt.ecosystem.clone().unwrap_or_default(),
                timeout_secs: rt.timeout_secs,
                expect: rt.expect.clone(),
            });
        }

        if errs.is_empty() {
            Ok(CorpusConfig {
                sbomqs_version: raw.sbomqs_version,
                default_timeout_secs: raw.default_timeout_secs,
                targets,
            })
        } else {
            Err(ConfigErrors(errs))
        }
    }
}

fn is_sha40(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn validate_expectations(who: &str, exp: &Expectations, errs: &mut Vec<String>) {
    let int_ranges: [(&str, Option<Range>); 5] = [
        ("wall_ms", exp.wall_ms),
        ("pkgs", exp.pkgs),
        ("files", exp.files),
        ("edges", exp.edges),
        ("max_depth", exp.max_depth),
    ];
    for (field, r) in int_ranges {
        if let Some(r) = r {
            if let Err(e) = Range::new(r.min, r.max) {
                errs.push(format!("{who}.expect.{field}: {e}"));
            }
        }
    }
    if let Some(r) = exp.sbomqs {
        if let Err(e) = RangeF::new(r.min, r.max) {
            errs.push(format!("{who}.expect.sbomqs: {e}"));
        }
        if r.min < 0.0 || r.max > 10.0 {
            errs.push(format!(
                "{who}.expect.sbomqs: bounds must lie within 0.0..=10.0, got {}..{}",
                r.min, r.max
            ));
        }
    }
}

// ────────────────────────────────────────────────────────────────
// T008 — config parsing + validation unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const SHA: &str = "a655097faf7d54f78933a815984b9919d51a05d2";

    fn one_target(extra: &str) -> String {
        format!(
            r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "go-cobra"
url = "https://github.com/spf13/cobra"
sha = "{SHA}"
ecosystem = "go"
{extra}
"#
        )
    }

    #[test]
    fn parses_a_minimal_corpus() {
        let c = CorpusConfig::parse(&one_target("")).unwrap();
        assert_eq!(c.sbomqs_version, "v2.0.6");
        assert_eq!(c.default_timeout_secs, 600);
        assert_eq!(c.targets.len(), 1);
        assert_eq!(c.targets[0].name.as_str(), "go-cobra");
        assert_eq!(c.targets[0].pin, Pin::Sha { hex: SHA.into() });
        // No expect block ⟹ observe-only (FR-020).
        assert!(c.targets[0].expect.is_none());
    }

    #[test]
    fn rejects_duplicate_names() {
        let t = format!(
            r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "dup"
url = "https://example.com/a"
sha = "{SHA}"
[[targets]]
name = "dup"
url = "https://example.com/b"
sha = "{SHA}"
"#
        );
        let e = CorpusConfig::parse(&t).unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("duplicate target name: dup")), "{e:?}");
    }

    #[test]
    fn rejects_inverted_range() {
        let e = CorpusConfig::parse(&one_target("[targets.expect]\npkgs = { min = 90, max = 10 }"))
            .unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("pkgs") && m.contains("exceeds max")), "{e:?}");
    }

    #[test]
    fn rejects_non_hex_sha() {
        let t = r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "bad"
url = "https://example.com/a"
sha = "not-a-sha"
"#;
        let e = CorpusConfig::parse(t).unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("40 lowercase hex")), "{e:?}");
    }

    #[test]
    fn rejects_uppercase_sha() {
        let t = format!(
            r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "bad"
url = "https://example.com/a"
sha = "{}"
"#,
            SHA.to_uppercase()
        );
        let e = CorpusConfig::parse(&t).unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("40 lowercase hex")), "{e:?}");
    }

    #[test]
    fn rejects_moving_ref_as_not_yet_supported() {
        let t = r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "floating"
url = "https://example.com/a"
ref = "main"
"#;
        let e = CorpusConfig::parse(t).unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("not yet supported")), "{e:?}");
    }

    #[test]
    fn rejects_empty_target_list() {
        let e = CorpusConfig::parse("sbomqs_version = \"v2.0.6\"\n").unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("no targets")), "{e:?}");
    }

    #[test]
    fn rejects_empty_sbomqs_version() {
        let t = format!(
            r#"
sbomqs_version = ""
[[targets]]
name = "x"
url = "https://example.com/a"
sha = "{SHA}"
"#
        );
        let e = CorpusConfig::parse(&t).unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("sbomqs_version")), "{e:?}");
    }

    #[test]
    fn rejects_sbomqs_bound_outside_zero_to_ten() {
        let e = CorpusConfig::parse(&one_target(
            "[targets.expect]\nsbomqs = { min = 1.0, max = 11.0 }",
        ))
        .unwrap_err();
        assert!(e.0.iter().any(|m| m.contains("0.0..=10.0")), "{e:?}");
    }

    /// FR-021: all problems reported together, not just the first.
    #[test]
    fn reports_every_error_not_only_the_first() {
        let t = r#"
sbomqs_version = ""
[[targets]]
name = "dup"
url = "https://example.com/a"
sha = "zzz"
[[targets]]
name = "dup"
url = "https://example.com/b"
ref = "main"
"#;
        let e = CorpusConfig::parse(t).unwrap_err();
        assert!(e.0.len() >= 4, "expected >=4 errors, got {:?}", e.0);
    }

    #[test]
    fn range_contains_is_inclusive_at_both_ends() {
        let r = Range::new(10, 20).unwrap();
        assert!(r.contains(10));
        assert!(r.contains(20));
        assert!(!r.contains(9));
        assert!(!r.contains(21));
    }

    #[test]
    fn range_new_rejects_inverted() {
        assert!(Range::new(20, 10).is_err());
        assert!(RangeF::new(2.0, 1.0).is_err());
    }

    #[test]
    fn pin_short_truncates_sha_to_twelve() {
        let p = Pin::Sha { hex: SHA.into() };
        assert_eq!(p.short(), "a655097faf7d");
        assert_eq!(p.short().len(), 12);
    }

    #[test]
    fn effective_timeout_prefers_per_target_override() {
        let c = CorpusConfig::parse(&one_target("timeout_secs = 42")).unwrap();
        assert_eq!(c.targets[0].effective_timeout(600), 42);
        let c2 = CorpusConfig::parse(&one_target("")).unwrap();
        assert_eq!(c2.targets[0].effective_timeout(600), 600);
    }

    #[test]
    fn unknown_key_is_rejected_rather_than_ignored() {
        // deny_unknown_fields: a typo'd bound must not silently disable gating.
        let t = format!(
            r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "x"
url = "https://example.com/a"
sha = "{SHA}"
[targets.expect]
pkgz = {{ min = 1, max = 2 }}
"#
        );
        assert!(CorpusConfig::parse(&t).is_err());
    }
}
