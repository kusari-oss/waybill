//! Milestone 224: coursier coordinate-string parser.
//!
//! Coursier's `dependencies[]` / `directDependencies[]` fields hold
//! opaque strings shaped like:
//!
//! ```text
//! coord_string = coord_triple ("," metadata_kv ("," metadata_kv)*)?
//! coord_triple = <group> ":" <artifact> ":" <version>
//! metadata_kv  = <key> "=" <value>
//! ```
//!
//! Examples:
//! - `"com.google.guava:guava:31.0.1-jre"`
//! - `"org.scala-lang:scala-library:2.13.10,url=https://...,jar=..."`
//!
//! The metadata segment is ignored — waybill needs only the triple.
//! Per research.md §R2, we split on the FIRST `,` to separate the
//! triple from the metadata, then `splitn(3, ':')` the triple. This
//! survives future metadata-key additions without any parser change.

/// Parsed coord triple. Metadata k/v pairs after the first `,` are
/// discarded — waybill's dep-graph edges need only the triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coordinate {
    pub(crate) group: String,
    pub(crate) artifact: String,
    pub(crate) version: String,
}

/// Parse a coursier coord-string into its triple. Returns `None` on
/// any of:
/// - empty input
/// - fewer than three colon-separated segments before the first `,`
/// - any segment (group / artifact / version) is empty after `.trim()`
///
/// Extra colons beyond the third are folded into the version segment
/// via `splitn(3, ':')` — coursier occasionally emits classifier or
/// packaging suffixes as `"g:a:v:classifier"`; we absorb them into
/// version so the triple parses (the emitter later strips at a
/// higher layer via [`super::lockfile::EntryCoord`] fields).
pub(crate) fn parse_coord_string(s: &str) -> Option<Coordinate> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let triple_part = match s.split_once(',') {
        Some((left, _rest)) => left,
        None => s,
    };
    let mut it = triple_part.splitn(3, ':');
    let group = it.next()?.trim();
    let artifact = it.next()?.trim();
    let version = it.next()?.trim();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    Some(Coordinate {
        group: group.to_string(),
        artifact: artifact.to_string(),
        version: version.to_string(),
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_triple() {
        let c = parse_coord_string("g:a:v").unwrap();
        assert_eq!(c.group, "g");
        assert_eq!(c.artifact, "a");
        assert_eq!(c.version, "v");
    }

    #[test]
    fn parses_triple_with_url_metadata() {
        let c = parse_coord_string("g:a:v,url=https://example.test/x.jar").unwrap();
        assert_eq!(c.group, "g");
        assert_eq!(c.artifact, "a");
        assert_eq!(c.version, "v");
    }

    #[test]
    fn parses_triple_with_url_and_jar_metadata() {
        let c = parse_coord_string("g:a:v,url=X,jar=Y").unwrap();
        assert_eq!(c.group, "g");
        assert_eq!(c.artifact, "a");
        assert_eq!(c.version, "v");
    }

    #[test]
    fn missing_version_returns_none() {
        assert!(parse_coord_string("g:a").is_none());
    }

    #[test]
    fn extra_colon_folded_into_version() {
        // "g:a:v:extra" — splitn(3) captures group="g", artifact="a",
        // version="v:extra" (the fourth segment sticks to version).
        let c = parse_coord_string("g:a:v:extra").unwrap();
        assert_eq!(c.group, "g");
        assert_eq!(c.artifact, "a");
        assert_eq!(c.version, "v:extra");
    }

    #[test]
    fn empty_string_returns_none() {
        assert!(parse_coord_string("").is_none());
    }

    #[test]
    fn empty_group_returns_none() {
        assert!(parse_coord_string(":a:v").is_none());
    }

    #[test]
    fn empty_artifact_returns_none() {
        assert!(parse_coord_string("g::v").is_none());
    }

    #[test]
    fn empty_version_returns_none() {
        assert!(parse_coord_string("g:a:").is_none());
    }

    #[test]
    fn trailing_comma_still_parses_triple() {
        let c = parse_coord_string("g:a:v,").unwrap();
        assert_eq!(c.group, "g");
        assert_eq!(c.artifact, "a");
        assert_eq!(c.version, "v");
    }
}
