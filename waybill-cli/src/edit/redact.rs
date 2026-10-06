//! Redaction for `waybill sbom edit` (milestone 1071, research R5).
//!
//! Values are collected per field class, then replaced **everywhere** they
//! occur. A value appears in more places than the field it was collected
//! from:
//! - a package name is also in PURLs and `bom-ref`s, percent-encoded;
//! - it's in CPEs, backslash-escaped;
//! - it's in annotation payloads, JSON-escaped a second time;
//! - its bare last segment is in the component's own distribution URLs.
//!
//! Every form is replaced with the same token. Afterwards an independent
//! plain-substring search ([`Redactor::check_no_leaks`]) looks for every
//! form again. If any remains, the edit fails and nothing is written: a
//! redaction that leaks is worse than none.
//!
//! What redaction does not hide (hashes, versions, graph shape, the
//! original's hash in the derivation record) is documented in
//! docs/user-guide/sbom-edit.md.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, bail, Result};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;

use super::select::{pattern, PatternMatcher};
use super::SbomAdapter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RedactClass {
    Paths,
    Hosts,
    Names,
}

impl RedactClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paths => "paths",
            Self::Hosts => "hosts",
            Self::Names => "names",
        }
    }

    pub fn category(self) -> &'static str {
        match self {
            Self::Paths => "redact-paths",
            Self::Hosts => "redact-hosts",
            Self::Names => "redact-names",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "paths" => Some(Self::Paths),
            "hosts" => Some(Self::Hosts),
            "names" => Some(Self::Names),
            _ => None,
        }
    }

    /// Default mode per class (contracts/cli.md): paths are usually noise
    /// and are removed; hosts and names often need to stay distinguishable.
    pub fn default_mode(self) -> RedactMode {
        match self {
            Self::Paths => RedactMode::Remove,
            Self::Hosts | Self::Names => RedactMode::Pseudonymise,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RedactMode {
    /// Replace with an opaque per-document marker that carries no
    /// information (`redacted-<n>`); distinct values stay distinct.
    Remove,
    /// Replace with a keyed pseudonym, stable across documents for one key.
    Pseudonymise,
}

impl RedactMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "remove" => Some(Self::Remove),
            "pseudonymise" | "pseudonymize" => Some(Self::Pseudonymise),
            _ => None,
        }
    }
}

/// `redacted-` + lowercase base32 of the first 10 bytes of
/// HMAC-SHA256(key, class ‖ 0x00 ‖ value). Stable for one key, unlinkable
/// without it, and a valid PURL name segment and CPE component.
pub fn pseudonym(key: &[u8], class: RedactClass, value: &str) -> String {
    // HMAC accepts keys of any length; `new_from_slice` can't fail for it.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap_or_else(|_| unreachable!());
    mac.update(class.as_str().as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    let digest = mac.finalize().into_bytes();
    format!("redacted-{}", BASE32_NOPAD.encode(&digest[..10]).to_lowercase())
}

/// Where a replacement applies.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reach {
    Everywhere,
    /// Only within one component's own subtrees (its bare name segment, or
    /// a file component's file name), never in identifier fields. The id
    /// reads as it did after the first `.1` everywhere passes.
    Component(String, usize),
}

#[derive(Clone, Debug)]
struct Replacement {
    needle: String,
    token: String,
    reach: Reach,
    class: RedactClass,
}

pub struct Redactor {
    key: Option<Vec<u8>>,
    ordinals: BTreeMap<RedactClass, usize>,
    tokens: BTreeMap<(RedactClass, String), String>,
    applied: Vec<Replacement>,
    /// The everywhere replacements, one pass per operation, in order: they
    /// can rewrite identifiers, so a component is found through them.
    passes: Vec<Multi>,
}

impl Redactor {
    pub fn new(key: Option<Vec<u8>>) -> Self {
        Self {
            key,
            ordinals: BTreeMap::new(),
            tokens: BTreeMap::new(),
            applied: Vec::new(),
            passes: Vec::new(),
        }
    }

    fn token(&mut self, class: RedactClass, mode: RedactMode, canonical: &str) -> String {
        if let Some(t) = self.tokens.get(&(class, canonical.to_string())) {
            return t.clone();
        }
        let base = match (mode, &self.key) {
            (RedactMode::Pseudonymise, Some(key)) => pseudonym(key, class, canonical),
            _ => {
                let n = self.ordinals.entry(class).or_insert(0);
                *n += 1;
                match class {
                    RedactClass::Names => format!("redacted-{n}"),
                    RedactClass::Paths => format!("redacted-path-{n}"),
                    RedactClass::Hosts => format!("redacted-host-{n}"),
                }
            }
        };
        let token = if class == RedactClass::Hosts { format!("{base}.invalid") } else { base };
        self.tokens.insert((class, canonical.to_string()), token.clone());
        token
    }

    /// Every original form this edit replaced, for checking the original's
    /// signature material before embedding it (research R7, analysis H2).
    pub fn redacted_forms(&self) -> BTreeSet<String> {
        self.applied.iter().map(|r| r.needle.clone()).collect()
    }

    /// Apply one redaction operation. Returns (distinct values matched,
    /// distinct values replaced).
    pub fn apply(
        &mut self,
        adapter: &mut dyn SbomAdapter,
        class: RedactClass,
        mode: RedactMode,
        pattern_text: Option<&str>,
    ) -> Result<(usize, usize)> {
        let matcher: Option<PatternMatcher> = match pattern_text {
            Some(p) => Some(pattern(p).map_err(anyhow::Error::msg)?),
            None => None,
        };
        // canonical value → replacements for its forms
        let mut plan: BTreeMap<String, Vec<Replacement>> = BTreeMap::new();
        match class {
            RedactClass::Paths => {
                // A file component is named by its file name; a package
                // merely containing a file is not, so the basename is
                // replaced only for the former.
                let files: BTreeMap<String, String> = adapter
                    .components()
                    .into_iter()
                    .filter(|c| c.tier.as_deref() == Some("file") || c.roles.contains("file"))
                    .map(|c| (c.id, c.name))
                    .collect();
                for (id, paths) in adapter.path_values() {
                    for p in paths {
                        // `.` and the like name nothing.
                        if !p.chars().any(char::is_alphanumeric) {
                            continue;
                        }
                        if matcher.as_ref().is_some_and(|m| !m.is_match(&p)) {
                            continue;
                        }
                        let token = self.token(class, mode, &p);
                        let entry = plan.entry(p.clone()).or_default();
                        entry.push(Replacement { needle: p.clone(), token: token.clone(), reach: Reach::Everywhere, class });
                        if let Some(base) = p.rsplit('/').next().filter(|b| *b != p && !b.is_empty()) {
                            if files.get(&id).is_some_and(|n| n == base) {
                                entry.push(Replacement { needle: base.to_string(), token, reach: Reach::Component(id.clone(), 0), class });
                            }
                        }
                    }
                }
            }
            RedactClass::Hosts => {
                let Some(m) = matcher.as_ref() else {
                    bail!("--redact hosts needs a pattern, e.g. hosts=*.corp.example");
                };
                let mut hosts = BTreeSet::new();
                walk_strings(adapter.doc(), &mut |s| {
                    for h in host_like().find_iter(s) {
                        let h = h.as_str().to_ascii_lowercase();
                        if m.is_match(&h) {
                            hosts.insert(h);
                        }
                    }
                });
                for h in hosts {
                    let token = self.token(class, mode, &h);
                    plan.entry(h.clone()).or_default().push(Replacement { needle: h, token, reach: Reach::Everywhere, class });
                }
            }
            RedactClass::Names => {
                let Some(m) = matcher.as_ref() else {
                    bail!("--redact names needs a pattern, e.g. names=@acme/*");
                };
                for c in adapter.components() {
                    if !m.is_match(&c.name) {
                        continue;
                    }
                    let token = self.token(class, mode, &c.name);
                    let mut forms: BTreeSet<String> = BTreeSet::from([c.name.clone(), cpe_escape(&c.name)]);
                    let mut bare: BTreeSet<String> = BTreeSet::new();
                    if let Some(path) = adapter.purl_of(&c.id) {
                        forms.insert(path.clone());
                        if let Some(seg) = path.rsplit('/').next() {
                            bare.insert(percent_decode(seg));
                        }
                    }
                    if let Some(seg) = c.name.rsplit('/').next() {
                        bare.insert(seg.to_string());
                    }
                    let entry = plan.entry(c.name.clone()).or_default();
                    for f in forms {
                        entry.push(Replacement { needle: f, token: token.clone(), reach: Reach::Everywhere, class });
                    }
                    for b in bare.into_iter().filter(|b| *b != c.name && !b.is_empty()) {
                        entry.push(Replacement { needle: b, token: token.clone(), reach: Reach::Component(c.id.clone(), 0), class });
                    }
                }
            }
        }

        // JSON-escaped forms, for values nested inside annotation payloads.
        for reps in plan.values_mut() {
            let extra: Vec<Replacement> = reps
                .iter()
                .filter_map(|r| {
                    let esc = json_escape(&r.needle);
                    (esc != r.needle).then(|| Replacement { needle: esc, ..r.clone() })
                })
                .collect();
            reps.extend(extra);
        }

        // Everywhere forms first, in one pass, so identifiers and the
        // references to them change together. Then component-scoped forms,
        // within each component found under its identifier as rewritten (a
        // `bom-ref` is often the PURL, which a name redaction rewrites).
        let matched = plan.len();
        let mut hit_values: BTreeSet<String> = BTreeSet::new();
        let everywhere: Vec<(String, String, String)> = plan
            .iter()
            .flat_map(|(c, reps)| reps.iter().filter(|r| r.reach == Reach::Everywhere).map(move |r| (r.needle.clone(), r.token.clone(), c.clone())))
            .collect();
        if let Some(pass) = Multi::new(&everywhere)? {
            walk_strings_mut(adapter.doc_mut(), &mut |s| {
                if let Some((n, hits)) = pass.replace(s) {
                    *s = n;
                    hit_values.extend(hits);
                }
            });
            self.passes.push(pass);
        }
        self.applied.extend(plan.values().flatten().filter(|r| r.reach == Reach::Everywhere).cloned());

        let mut scoped: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
        for (c, reps) in &plan {
            for r in reps {
                if let Reach::Component(id, _) = &r.reach {
                    scoped.entry(id.clone()).or_default().push((r.needle.clone(), r.token.clone(), c.clone()));
                }
            }
        }
        let after = self.passes.len();
        for (id, entries) in scoped {
            let id = self.current_id(&id, 0);
            let Some(pass) = Multi::new(&entries)? else { continue };
            let subs = adapter.component_subtrees_mut(&id);
            if subs.is_empty() {
                bail!("cannot find component `{id}` to redact within; nothing written");
            }
            for sub in subs {
                walk_strings_mut_except_ids(sub, &mut |s| {
                    if let Some((n, hits)) = pass.replace(s) {
                        *s = n;
                        hit_values.extend(hits);
                    }
                });
            }
            for (needle, token, _) in entries {
                self.applied.push(Replacement { needle, token, reach: Reach::Component(id.clone(), after), class });
            }
        }
        Ok((matched, hit_values.len()))
    }

    /// A component identifier as it reads after every everywhere pass from
    /// `from` on.
    fn current_id(&self, id: &str, from: usize) -> String {
        self.passes[from.min(self.passes.len())..]
            .iter()
            .fold(id.to_string(), |id, p| p.replace(&id).map(|(n, _)| n).unwrap_or(id))
    }

    /// Fail closed if any replaced form still appears: an independent plain
    /// substring search, with no boundary rules, over every string (and
    /// what any base64 string decodes to).
    pub fn check_no_leaks(&self, adapter: &mut dyn SbomAdapter) -> Result<()> {
        let leak = |class: RedactClass| {
            anyhow!(
                "a redacted {} value is still present after redaction; nothing written \
                 (please report: the redactor missed a form this document uses)",
                class.as_str()
            )
        };
        let everywhere: Vec<&Replacement> = self.applied.iter().filter(|r| r.reach == Reach::Everywhere).collect();
        if let Some(re) = any_of(everywhere.iter().map(|r| r.needle.as_str()))? {
            let mut found = false;
            walk_strings(adapter.doc(), &mut |s| {
                if !found {
                    found = readable_views(s).iter().any(|v| re.is_match(v));
                }
            });
            if found {
                let class = everywhere
                    .iter()
                    .find(|r| {
                        let mut f = false;
                        walk_strings(adapter.doc(), &mut |s| f |= readable_views(s).iter().any(|v| v.contains(&r.needle)));
                        f
                    })
                    .map(|r| r.class)
                    .unwrap_or(RedactClass::Paths);
                return Err(leak(class));
            }
        }
        let mut scoped: BTreeMap<(String, usize), Vec<&Replacement>> = BTreeMap::new();
        for r in &self.applied {
            if let Reach::Component(id, after) = &r.reach {
                scoped.entry((id.clone(), *after)).or_default().push(r);
            }
        }
        for ((id, after), reps) in scoped {
            let id = self.current_id(&id, after);
            let Some(re) = any_of(reps.iter().map(|r| r.needle.as_str()))? else { continue };
            let subs = adapter.component_subtrees_mut(&id);
            if subs.is_empty() {
                bail!("a component redacted by this edit can no longer be found to check; nothing written");
            }
            let mut found = false;
            for sub in subs {
                walk_strings(sub, &mut |s| found |= re.is_match(s));
            }
            if found {
                return Err(leak(reps[0].class));
            }
        }
        Ok(())
    }
}

/// Several needles replaced in one pass, longest first at each position,
/// replacing a needle only where it isn't glued to a longer word on either
/// side: `internal-utils` is replaced in `internal-utils-2.0.1.tgz`, not in
/// `internal-utilsx`.
struct Multi {
    re: Regex,
    table: BTreeMap<String, (String, String)>,
}

impl Multi {
    /// `(needle, token, canonical value)`; `None` if there are none.
    fn new(entries: &[(String, String, String)]) -> Result<Option<Self>> {
        let mut table = BTreeMap::new();
        for (needle, token, canonical) in entries {
            if !needle.is_empty() {
                table.entry(needle.clone()).or_insert((token.clone(), canonical.clone()));
            }
        }
        let Some(re) = any_of(table.keys().map(String::as_str))? else { return Ok(None) };
        Ok(Some(Self { re, table }))
    }

    /// The rewritten string and the canonical values replaced, if any.
    fn replace(&self, s: &str) -> Option<(String, Vec<String>)> {
        let mut out = String::with_capacity(s.len());
        let mut hits = Vec::new();
        let mut last = 0;
        for m in self.re.find_iter(s) {
            let (needle, start, end) = (m.as_str(), m.start(), m.end());
            let lead = needle.chars().next().is_some_and(is_word);
            let trail = needle.chars().next_back().is_some_and(is_word);
            let glued = (lead && s[..start].chars().next_back().is_some_and(is_word))
                || (trail && s[end..].chars().next().is_some_and(is_word));
            if glued {
                continue;
            }
            let Some((token, canonical)) = self.table.get(needle) else { continue };
            out.push_str(&s[last..start]);
            out.push_str(token);
            hits.push(canonical.clone());
            last = end;
        }
        if hits.is_empty() {
            return None;
        }
        out.push_str(&s[last..]);
        Some((out, hits))
    }
}

/// One regex matching any of the needles, longest first (the leftmost
/// alternative wins at a position).
fn any_of<'a>(needles: impl Iterator<Item = &'a str>) -> Result<Option<Regex>> {
    let mut ns: Vec<&str> = needles.filter(|n| !n.is_empty()).collect();
    if ns.is_empty() {
        return Ok(None);
    }
    ns.sort_by_key(|n| std::cmp::Reverse(n.len()));
    ns.dedup();
    let pattern = ns.iter().map(|n| regex::escape(n)).collect::<Vec<_>>().join("|");
    let re = regex::RegexBuilder::new(&pattern)
        .size_limit(1 << 30)
        .dfa_size_limit(1 << 30)
        .build()
        .map_err(|e| anyhow!("building the redaction matcher: {e}"))?;
    Ok(Some(re))
}

/// Keys whose values identify an element or point at one. Everywhere forms
/// rewrite these consistently with the references to them; a
/// component-scoped form must not, or the element would stop matching its
/// references.
const ID_KEYS: &[&str] = &[
    "bom-ref", "ref", "dependsOn", "purl", "SPDXID", "spdxElementId", "relatedSpdxElement",
    "spdxId", "subject", "from", "to", "software_packageUrl", "documentDescribes", "rootElement",
];

fn walk_strings_mut_except_ids(v: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter_mut().for_each(|x| walk_strings_mut_except_ids(x, f)),
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if !ID_KEYS.contains(&k.as_str()) {
                    walk_strings_mut_except_ids(x, f);
                }
            }
        }
        _ => {}
    }
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The string, plus what it decodes to if it is base64 (either alphabet).
/// Signature material carries certificates and payloads as base64, and a
/// certificate can name an internal host (analysis H2), so a substring
/// search over the raw text alone would miss it.
pub(crate) fn readable_views(s: &str) -> Vec<String> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    use base64::Engine;
    let mut views = vec![s.to_string()];
    if s.len() >= 16 && !s.contains(char::is_whitespace) {
        for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
            if let Ok(bytes) = engine.decode(s) {
                views.push(String::from_utf8_lossy(&bytes).into_owned());
                break;
            }
        }
    }
    views
}

/// Whether any string in `v`, raw or base64-decoded, contains any form.
pub(crate) fn contains_any(v: &Value, forms: &BTreeSet<String>) -> bool {
    let mut found = false;
    walk_strings(v, &mut |s| {
        if !found {
            found = readable_views(s).iter().any(|view| forms.iter().any(|f| view.contains(f.as_str())));
        }
    });
    found
}

pub(crate) fn walk_strings(v: &Value, f: &mut dyn FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter().for_each(|x| walk_strings(x, f)),
        Value::Object(m) => m.values().for_each(|x| walk_strings(x, f)),
        _ => {}
    }
}

pub(crate) fn walk_strings_mut(v: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter_mut().for_each(|x| walk_strings_mut(x, f)),
        Value::Object(m) => m.values_mut().for_each(|x| walk_strings_mut(x, f)),
        _ => {}
    }
}

fn host_like() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+")
            .unwrap_or_else(|e| unreachable!("static regex: {e}"))
    })
}

/// CPE 2.3 formatted-string escaping, as waybill writes it.
fn cpe_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn json_escape(s: &str) -> String {
    let quoted = serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""));
    quoted[1..quoted.len() - 1].to_string()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn bounded(s: &str, needle: &str, token: &str) -> Option<String> {
        let entry = (needle.to_string(), token.to_string(), needle.to_string());
        Multi::new(&[entry]).unwrap().unwrap().replace(s).map(|(n, _)| n)
    }

    #[test]
    fn several_needles_replace_longest_first_in_one_pass() {
        let m = Multi::new(&[
            ("src/a.rs".into(), "P1".into(), "src/a.rs".into()),
            ("a.rs".into(), "P2".into(), "a.rs".into()),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(m.replace("x src/a.rs and a.rs").map(|(n, _)| n).as_deref(), Some("x P1 and P2"));
    }

    #[test]
    fn pseudonym_is_deterministic_keyed_and_class_scoped() {
        let a = pseudonym(b"k1", RedactClass::Names, "@acme/internal-utils");
        assert_eq!(a, pseudonym(b"k1", RedactClass::Names, "@acme/internal-utils"));
        assert_ne!(a, pseudonym(b"k2", RedactClass::Names, "@acme/internal-utils"));
        assert_ne!(a, pseudonym(b"k1", RedactClass::Hosts, "@acme/internal-utils"));
        assert_ne!(a, pseudonym(b"k1", RedactClass::Names, "@acme/other"));
        assert!(a.starts_with("redacted-"));
        assert!(a.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert_eq!(a.len(), "redacted-".len() + 16);
    }

    #[test]
    fn bounded_replace_respects_word_edges() {
        assert_eq!(
            bounded("internal-utils-2.0.1.tgz", "internal-utils", "X").as_deref(),
            Some("X-2.0.1.tgz")
        );
        assert_eq!(bounded("internal-utilsx", "internal-utils", "X"), None);
        assert_eq!(bounded("ab/src/a.rs", "src/a.rs", "P").as_deref(), Some("ab/P"));
        assert_eq!(bounded("nothing here", "zzz", "X"), None);
    }

    #[test]
    fn escapes_match_how_waybill_writes_them() {
        assert_eq!(cpe_escape("@acme/internal-utils"), "\\@acme\\/internal-utils");
        assert_eq!(json_escape("\\@acme\\/x"), "\\\\@acme\\\\/x");
        assert_eq!(percent_decode("%40acme"), "@acme");
        assert_eq!(percent_decode("plain"), "plain");
    }
}
