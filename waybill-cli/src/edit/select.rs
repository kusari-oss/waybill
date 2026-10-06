//! Component selectors for `waybill sbom edit` (milestone 1071, research R3).
//!
//! Text form: `;`-separated `key=value[,value...]` terms. Terms combine
//! with AND; values within a term combine with OR. Keys:
//! `purl` (glob), `ecosystem` (PURL type), `scope`
//! (`runtime|development|build|test|optional`), `tier`, `role`, `name` (glob, or a
//! regex prefixed `re:`).

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A dependency's lifecycle scope, as the selector and bridging see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    Runtime,
    Development,
    Build,
    Test,
    /// An optional dependency (waybill's `LifecycleScope::Optional`).
    Optional,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "runtime" | "required" => Some(Self::Runtime),
            "development" | "dev" => Some(Self::Development),
            "build" => Some(Self::Build),
            "test" => Some(Self::Test),
            "optional" => Some(Self::Optional),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Development => "development",
            Self::Build => "build",
            Self::Test => "test",
            Self::Optional => "optional",
        }
    }

    /// The scope of an edge bridged across a dropped component (research
    /// R4): equal scopes stay; a non-runtime scope wins over runtime, since
    /// a dependency reached through a dev-only link is dev-only; between two
    /// different non-runtime scopes, the edge into the dropped component
    /// wins, because it is what made the dependency reachable.
    pub fn bridge(into_dropped: Scope, out_of_dropped: Scope) -> Scope {
        match (into_dropped, out_of_dropped) {
            (a, b) if a == b => a,
            (Scope::Runtime, b) => b,
            (a, _) => a,
        }
    }
}

/// The facts a selector matches on, read per format by the adapters.
#[derive(Clone, Debug, Default)]
pub struct ComponentView {
    pub id: String,
    pub purl: Option<String>,
    pub name: String,
    pub scopes: BTreeSet<Scope>,
    pub tier: Option<String>,
    pub roles: BTreeSet<String>,
}

impl ComponentView {
    fn ecosystem(&self) -> Option<&str> {
        let p = self.purl.as_deref()?;
        let rest = p.strip_prefix("pkg:")?;
        Some(rest.split('/').next().unwrap_or(rest))
    }

    /// A component with no scope recorded is a runtime dependency.
    fn effective_scopes(&self) -> BTreeSet<Scope> {
        if self.scopes.is_empty() {
            BTreeSet::from([Scope::Runtime])
        } else {
            self.scopes.clone()
        }
    }
}

#[derive(Clone, Debug)]
enum NameMatch {
    Glob(GlobMatcher),
    Regex(Regex),
}

impl NameMatch {
    fn is_match(&self, s: &str) -> bool {
        match self {
            Self::Glob(g) => g.is_match(s),
            Self::Regex(r) => r.is_match(s),
        }
    }
}

/// Build a glob or `re:` regex matcher. Globs treat `/` as an ordinary
/// character, so `@acme/*` matches `@acme/internal-utils`.
pub(crate) fn pattern(text: &str) -> Result<PatternMatcher, String> {
    if let Some(re) = text.strip_prefix("re:") {
        return Regex::new(re)
            .map(|r| PatternMatcher(NameMatch::Regex(r)))
            .map_err(|e| format!("invalid regex `{re}`: {e}"));
    }
    globset::GlobBuilder::new(text)
        .literal_separator(false)
        .build()
        .map(|g: Glob| PatternMatcher(NameMatch::Glob(g.compile_matcher())))
        .map_err(|e| format!("invalid glob `{text}`: {e}"))
}

#[derive(Clone, Debug)]
pub struct PatternMatcher(NameMatch);

impl PatternMatcher {
    pub fn is_match(&self, s: &str) -> bool {
        self.0.is_match(s)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Selector {
    purl: Vec<PatternMatcher>,
    ecosystem: BTreeSet<String>,
    scope: BTreeSet<Scope>,
    tier: BTreeSet<String>,
    role: BTreeSet<String>,
    name: Vec<PatternMatcher>,
    text: String,
}

impl Selector {
    pub fn matches(&self, c: &ComponentView) -> bool {
        let any = |pats: &[PatternMatcher], s: Option<&str>| match s {
            Some(s) => pats.iter().any(|p| p.is_match(s)),
            None => false,
        };
        (self.purl.is_empty() || any(&self.purl, c.purl.as_deref()))
            && (self.ecosystem.is_empty()
                || c.ecosystem().is_some_and(|e| self.ecosystem.contains(e)))
            && (self.scope.is_empty()
                || !self.scope.is_disjoint(&c.effective_scopes()))
            && (self.tier.is_empty() || c.tier.as_ref().is_some_and(|t| self.tier.contains(t)))
            && (self.role.is_empty() || !self.role.is_disjoint(&c.roles))
            && (self.name.is_empty() || any(&self.name, Some(&c.name)))
    }
}

impl FromStr for Selector {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut sel = Selector {
            text: text.to_string(),
            ..Selector::default()
        };
        let mut any_term = false;
        for term in text.split(';').map(str::trim).filter(|t| !t.is_empty()) {
            let (key, values) = term
                .split_once('=')
                .ok_or_else(|| format!("selector term `{term}` is not key=value"))?;
            let values: Vec<&str> = values.split(',').map(str::trim).filter(|v| !v.is_empty()).collect();
            if values.is_empty() {
                return Err(format!("selector term `{term}` has no value"));
            }
            any_term = true;
            match key.trim() {
                "purl" => {
                    for v in values {
                        sel.purl.push(pattern(v)?);
                    }
                }
                "name" => {
                    for v in values {
                        sel.name.push(pattern(v)?);
                    }
                }
                "ecosystem" => sel.ecosystem.extend(values.iter().map(|v| v.to_string())),
                "tier" => sel.tier.extend(values.iter().map(|v| v.to_string())),
                "role" => sel.role.extend(values.iter().map(|v| v.to_string())),
                "scope" => {
                    for v in values {
                        sel.scope.insert(
                            Scope::parse(v).ok_or_else(|| {
                                format!("unknown scope `{v}` (runtime, development, build, test)")
                            })?,
                        );
                    }
                }
                other => {
                    return Err(format!(
                        "unknown selector key `{other}` (purl, ecosystem, scope, tier, role, name)"
                    ))
                }
            }
        }
        if !any_term {
            return Err("an empty selector would select everything; name at least one term".to_string());
        }
        Ok(sel)
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl PartialEq for Selector {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

// The policy-file form (FR-014) is the same text as the command line.
impl Serialize for Selector {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn view(name: &str, purl: &str, scopes: &[Scope]) -> ComponentView {
        ComponentView {
            id: purl.to_string(),
            purl: Some(purl.to_string()),
            name: name.to_string(),
            scopes: scopes.iter().copied().collect(),
            tier: Some("source".to_string()),
            roles: BTreeSet::from(["library".to_string()]),
        }
    }

    #[test]
    fn parse_errors_are_named() {
        assert!("".parse::<Selector>().unwrap_err().contains("empty selector"));
        assert!("nonsense".parse::<Selector>().unwrap_err().contains("key=value"));
        assert!("colour=red".parse::<Selector>().unwrap_err().contains("unknown selector key"));
        assert!("scope=prod".parse::<Selector>().unwrap_err().contains("unknown scope"));
        assert!("name=re:(".parse::<Selector>().unwrap_err().contains("invalid regex"));
        assert!("scope=".parse::<Selector>().unwrap_err().contains("no value"));
    }

    #[test]
    fn each_key_matches() {
        let c = view("@acme/internal-utils", "pkg:npm/%40acme/internal-utils@2.0.1", &[]);
        assert!("purl=pkg:npm/*".parse::<Selector>().unwrap().matches(&c));
        assert!("ecosystem=npm".parse::<Selector>().unwrap().matches(&c));
        assert!(!"ecosystem=cargo".parse::<Selector>().unwrap().matches(&c));
        assert!("scope=runtime".parse::<Selector>().unwrap().matches(&c));
        assert!("tier=source".parse::<Selector>().unwrap().matches(&c));
        assert!("role=library".parse::<Selector>().unwrap().matches(&c));
        assert!("name=@acme/*".parse::<Selector>().unwrap().matches(&c));
        assert!("name=re:^@acme/internal-".parse::<Selector>().unwrap().matches(&c));
    }

    #[test]
    fn terms_and_values_or() {
        let dev = view("jest-lite", "pkg:npm/jest-lite@1.0.0", &[Scope::Development]);
        let run = view("express", "pkg:npm/express@4.18.2", &[]);
        let s: Selector = "scope=development,test".parse().unwrap();
        assert!(s.matches(&dev) && !s.matches(&run));
        let s: Selector = "ecosystem=npm;scope=development".parse().unwrap();
        assert!(s.matches(&dev) && !s.matches(&run));
    }

    #[test]
    fn bridged_scope_rule() {
        use Scope::*;
        assert_eq!(Scope::bridge(Runtime, Runtime), Runtime);
        assert_eq!(Scope::bridge(Runtime, Development), Development);
        assert_eq!(Scope::bridge(Test, Runtime), Test);
        assert_eq!(Scope::bridge(Build, Test), Build);
    }

    #[test]
    fn selector_round_trips_through_serde() {
        let s: Selector = "scope=development,test;ecosystem=npm".parse().unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let back: Selector = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
