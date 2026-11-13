//! Spec 1491 (#1491) — the curated CPE table as data, and the schema an
//! operator's `--cpe-map` file shares with it.
//!
//! The built-in table is `cpe_table.toml`, embedded at build time: a scan
//! never reaches the network for it (FR-015). An operator file is the same
//! TOML, read from disk, with the evidence fields optional. Contract:
//! `specs/1491-operator-cpe-table/data-model.md`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use waybill_common::types::purl::Purl;

/// The committed table.
const BUILT_IN: &str = include_str!("cpe_table.toml");

/// One table: built-in or an operator's.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CpeTable {
    #[serde(default)]
    pub(crate) row: Vec<CpeRow>,
}

/// A versionless PURL and the NVD `(vendor, product)` pairs it maps to. The
/// first pair is the primary CPE.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CpeRow {
    pub(crate) key: String,
    pub(crate) pair: Vec<CpePair>,
}

/// One `(vendor, product)` and the dictionary evidence for it. The evidence
/// fields are required in the built-in table and optional for an operator.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CpePair {
    pub(crate) vendor: String,
    pub(crate) product: String,
    pub(crate) dictionary_entries: Option<u32>,
    pub(crate) observed: Option<String>,
    pub(crate) dictionary_evidence: Option<String>,
    pub(crate) title_evidence: Option<String>,
}

/// Where a table came from, for error messages and for the rules that
/// differ between the two.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Origin<'a> {
    BuiltIn,
    /// Constructed by `--cpe-map` (spec 1491 PR B); exercised by the tests
    /// below until then.
    #[cfg_attr(not(test), allow(dead_code))]
    Operator(&'a Path),
}

impl std::fmt::Display for Origin<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn => f.write_str("the built-in CPE table"),
            Self::Operator(path) => write!(f, "--cpe-map {}", path.display()),
        }
    }
}

/// Why a table was rejected. Each names the table and, for a row, its
/// position and key, so the operator can find it (contracts/cli.md).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum CpeTableError {
    #[error("{origin}: not a valid CPE table: {message}")]
    Syntax { origin: String, message: String },
    #[error("{origin}: row {index} ({key}): {reason}")]
    Row { origin: String, index: usize, key: String, reason: String },
}

/// Parse and validate a table (data-model.md § Validation failures).
pub(crate) fn parse(text: &str, origin: Origin<'_>) -> Result<CpeTable, CpeTableError> {
    let table: CpeTable = toml::from_str(text).map_err(|e| CpeTableError::Syntax {
        origin: origin.to_string(),
        message: e.message().to_string(),
    })?;
    if table.row.is_empty() {
        return Err(CpeTableError::Syntax {
            origin: origin.to_string(),
            message: "the table has no rows".to_string(),
        });
    }
    let mut seen = HashSet::new();
    for (i, row) in table.row.iter().enumerate() {
        let fail = |reason: String| CpeTableError::Row {
            origin: origin.to_string(),
            index: i + 1,
            key: row.key.clone(),
            reason,
        };
        let key = parse_key(&row.key).map_err(fail)?;
        if matches!(origin, Origin::BuiltIn)
            && (key.ecosystem() != "generic" || key.namespace().is_some())
        {
            return Err(fail("a built-in key is `pkg:generic/<slug>`".to_string()));
        }
        if !seen.insert(row.key.as_str()) {
            return Err(fail("the key appears more than once".to_string()));
        }
        if row.pair.is_empty() {
            return Err(fail("the row has no vendor/product pair".to_string()));
        }
        for pair in &row.pair {
            validate_pair(pair, origin).map_err(fail)?;
        }
    }
    Ok(table)
}

/// A key is a versionless PURL: no version, qualifiers or subpath.
fn parse_key(raw: &str) -> Result<Purl, String> {
    if raw.contains('?') || raw.contains('#') {
        return Err("the key must be a PURL without qualifiers or subpath".to_string());
    }
    let purl = Purl::new(raw).map_err(|e| format!("the key is not a PURL: {e}"))?;
    if purl.version().is_some() {
        return Err(
            "the key must be a versionless PURL; the CPE takes the component's version"
                .to_string(),
        );
    }
    Ok(purl)
}

fn validate_pair(pair: &CpePair, origin: Origin<'_>) -> Result<(), String> {
    let who = format!("{}:{}", pair.vendor, pair.product);
    for (field, value) in [("vendor", &pair.vendor), ("product", &pair.product)] {
        if value.is_empty() {
            return Err(format!("{who}: the {field} is empty"));
        }
        if value.contains('*') || value.contains('?') {
            return Err(format!("{who}: the {field} is a wildcard; name the {field}"));
        }
    }
    if crate::generate::cpe::format_cpe(&pair.vendor, &pair.product, "1.0").is_none() {
        return Err(format!("{who}: does not form a valid CPE 2.3 name"));
    }
    if pair.dictionary_entries == Some(0) {
        return Err(format!("{who}: dictionary_entries is 0, so NVD does not have the pair"));
    }
    if let Some(date) = &pair.observed {
        if !is_iso_date(date) {
            return Err(format!("{who}: observed {date:?} is not YYYY-MM-DD"));
        }
    }
    if matches!(origin, Origin::BuiltIn) {
        for (field, present) in [
            ("dictionary_entries", pair.dictionary_entries.is_some()),
            ("observed", pair.observed.is_some()),
            ("dictionary_evidence", pair.dictionary_evidence.is_some()),
            ("title_evidence", pair.title_evidence.is_some()),
        ] {
            if !present {
                return Err(format!("{who}: a built-in pair needs {field}"));
            }
        }
    }
    Ok(())
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The built-in table, parsed once, with a lookup by slug.
pub(crate) struct BuiltIn {
    /// The parsed rows, evidence included. Read by the enforcement tests;
    /// a scan needs only `by_slug`.
    #[cfg_attr(not(test), allow(dead_code))]
    table: CpeTable,
    by_slug: HashMap<String, Vec<(String, String)>>,
}

impl BuiltIn {
    /// The `(vendor, product)` pairs for a `pkg:generic` slug, primary first.
    pub(crate) fn pairs(&self, slug: &str) -> Option<&[(String, String)]> {
        self.by_slug.get(slug).map(Vec::as_slice)
    }

    #[cfg(test)]
    pub(crate) fn table(&self) -> &CpeTable {
        &self.table
    }
}

/// The built-in table. A parse failure is a defect the tests catch
/// (`the_built_in_table_parses`); at run time it is logged and the table is
/// treated as empty. No curated CPE is the closed failure: a guessed one
/// would assert software that may not be present (research R3).
pub(crate) fn built_in() -> &'static BuiltIn {
    static TABLE: OnceLock<BuiltIn> = OnceLock::new();
    TABLE.get_or_init(|| {
        let table = parse(BUILT_IN, Origin::BuiltIn).unwrap_or_else(|e| {
            tracing::error!("{e}; no curated CPEs will be emitted");
            CpeTable::default()
        });
        let by_slug = table
            .row
            .iter()
            .filter_map(|row| {
                let slug = row.key.strip_prefix("pkg:generic/")?;
                let pairs = row
                    .pair
                    .iter()
                    .map(|p| (p.vendor.clone(), p.product.clone()))
                    .collect();
                Some((slug.to_string(), pairs))
            })
            .collect();
        BuiltIn { table, by_slug }
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const EVIDENCE: &str = r#"dictionary_entries = 1, observed = "2026-11-13", dictionary_evidence = "d", title_evidence = "t""#;

    fn built_in_row(key: &str, pair: &str) -> String {
        format!("[[row]]\nkey = \"{key}\"\npair = [ {{ {pair}, {EVIDENCE} }} ]\n")
    }

    fn operator(text: &str) -> Result<CpeTable, CpeTableError> {
        parse(text, Origin::Operator(Path::new("map.toml")))
    }

    fn reason(r: Result<CpeTable, CpeTableError>) -> String {
        match r {
            Err(e) => e.to_string(),
            Ok(t) => panic!("expected a rejection, parsed {} rows", t.row.len()),
        }
    }

    /// T007 — the embedded file parses as the built-in table, with every row.
    #[test]
    fn the_built_in_table_parses() {
        let table = parse(BUILT_IN, Origin::BuiltIn).unwrap();
        assert_eq!(table.row.len(), 82);
        assert_eq!(table.row.iter().map(|r| r.pair.len()).sum::<usize>(), 96);
        // The lookup holds every row, so none was dropped by a key shape.
        assert_eq!(built_in().by_slug.len(), 82);
        assert_eq!(
            built_in().pairs("curl").unwrap(),
            &[("haxx".to_string(), "curl".to_string()), ("curl".to_string(), "curl".to_string())]
        );
    }

    /// T003 — valid tables parse; an operator row needs no evidence.
    #[test]
    fn valid_tables_parse() {
        let text = built_in_row("pkg:generic/zlib", r#"vendor = "zlib", product = "zlib""#);
        assert_eq!(parse(&text, Origin::BuiltIn).unwrap().row.len(), 1);

        let op = operator(
            "[[row]]\nkey = \"pkg:deb/debian/waybill-fixture-lib\"\n\
             pair = [ { vendor = \"waybill-fixture-vendor\", product = \"waybill-fixture-lib\" } ]\n",
        )
        .unwrap();
        assert_eq!(op.row[0].pair[0].dictionary_entries, None);
    }

    /// T003 — each rejection in data-model § Validation failures names the
    /// row and says why.
    #[test]
    fn each_malformed_table_is_rejected_with_the_row_named() {
        let pair = r#"{ vendor = "v", product = "p" }"#;
        let row = |key: &str| format!("[[row]]\nkey = \"{key}\"\npair = [ {pair} ]\n");

        assert!(reason(operator("not toml [")).contains("not a valid CPE table"));
        assert!(reason(operator("")).contains("no rows"));
        assert!(reason(operator(&format!(
            "[[row]]\nkey = \"pkg:generic/x\"\nextra = 1\npair = [ {pair} ]\n"
        )))
        .contains("not a valid CPE table"));
        assert!(reason(operator(&format!("{}{}", row("pkg:generic/x"), row("pkg:generic/x"))))
            .contains("row 2 (pkg:generic/x): the key appears more than once"));
        assert!(reason(operator(&row("pkg:generic/x@1.0"))).contains("versionless"));
        assert!(reason(operator(&row("pkg:generic/x?arch=amd64"))).contains("qualifiers"));
        assert!(reason(operator(&row("not-a-purl"))).contains("not a PURL"));
        assert!(reason(operator(
            "[[row]]\nkey = \"pkg:generic/x\"\npair = [ { vendor = \"*\", product = \"p\" } ]\n"
        ))
        .contains("wildcard"));
        assert!(reason(operator(
            "[[row]]\nkey = \"pkg:generic/x\"\npair = [ { vendor = \"v\", product = \"\" } ]\n"
        ))
        .contains("product is empty"));
        assert!(reason(operator("[[row]]\nkey = \"pkg:generic/x\"\npair = []\n"))
            .contains("no vendor/product pair"));
        assert!(reason(operator(
            "[[row]]\nkey = \"pkg:generic/x\"\npair = [ { vendor = \"v\", product = \"p\", dictionary_entries = 0 } ]\n"
        ))
        .contains("dictionary_entries is 0"));
        assert!(reason(operator(
            "[[row]]\nkey = \"pkg:generic/x\"\npair = [ { vendor = \"v\", product = \"p\", observed = \"13 Nov\" } ]\n"
        ))
        .contains("not YYYY-MM-DD"));
    }

    /// T003 — the built-in table's extra rules: evidence present, and only
    /// `pkg:generic/<slug>` keys, because it is looked up by slug.
    #[test]
    fn the_built_in_rules_are_enforced() {
        let no_evidence =
            "[[row]]\nkey = \"pkg:generic/x\"\npair = [ { vendor = \"v\", product = \"p\" } ]\n";
        assert!(reason(parse(no_evidence, Origin::BuiltIn)).contains("needs dictionary_entries"));

        let deb = built_in_row("pkg:deb/debian/x", r#"vendor = "v", product = "p""#);
        assert!(reason(parse(&deb, Origin::BuiltIn)).contains("pkg:generic/<slug>"));
        // CONTROL: the same row is fine for an operator.
        assert!(operator(&deb).is_ok());
    }
}
