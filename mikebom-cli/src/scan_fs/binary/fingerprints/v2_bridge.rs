//! Bridge between the existing `BinaryScan` extractor outputs and the
//! v2 matcher's `BinaryArtifact` input shape (milestone 110, Phase 4
//! Slice B-2).
//!
//! `BinaryScan` is the existing per-binary aggregate carrying everything
//! the milestone-001..109 extractors produced. The matcher's
//! `BinaryArtifact` is a narrower shape — only the fields the indicator
//! matchers consume. This bridge maps one to the other + supplies the
//! one piece of derived data the existing struct doesn't carry:
//! `rodata_strings`, extracted from `BinaryScan.string_region` via a
//! `strings(1)`-style scan.

use super::matcher::BinaryArtifact;

/// Extract printable ASCII / UTF-8 runs of length >= `min_len` from a
/// raw `.rodata`-like byte region. Mirrors the classic `strings(1)`
/// behaviour: a non-printable byte (including NUL) terminates the
/// current run; runs shorter than `min_len` are discarded.
///
/// Bounded by the existing 16 MB string-region cap upstream in
/// `scan.rs::collect_string_region`; this function does no additional
/// allocation beyond the output `Vec`.
#[allow(dead_code)] // Slice B-2 wires this in production scan path.
pub(crate) fn extract_printable_strings(region: &[u8], min_len: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut run_start: Option<usize> = None;

    for (i, &byte) in region.iter().enumerate() {
        // Printable subset: ASCII 0x20..=0x7E (space through tilde) +
        // tab (0x09). Non-ASCII bytes intentionally break runs — full
        // Unicode rodata extraction is out of scope for the matcher's
        // substring-pattern use case, which targets curated literal
        // strings like "OpenSSL 3.1.4" that are pure ASCII anyway.
        let printable = byte == 0x09 || (0x20..=0x7E).contains(&byte);
        match (printable, run_start) {
            (true, None) => run_start = Some(i),
            (true, Some(_)) => { /* continuation */ }
            (false, Some(start)) => {
                if i - start >= min_len {
                    if let Ok(s) = std::str::from_utf8(&region[start..i]) {
                        out.push(s.to_string());
                    }
                }
                run_start = None;
            }
            (false, None) => { /* skip */ }
        }
    }
    // Tail flush.
    if let Some(start) = run_start {
        if region.len() - start >= min_len {
            if let Ok(s) = std::str::from_utf8(&region[start..]) {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// Construct a matcher-ready `BinaryArtifact` from the existing
/// `BinaryScan` extractor outputs.
///
/// `string_region` is parsed for printable strings via
/// `extract_printable_strings(..., 4)` (matches the `strings(1)`
/// default minimum). The build-id / Mach-O LC_UUID / PE PDB GUID
/// fields are copied through directly.
#[allow(dead_code)] // Slice B-2 wires this in production scan path.
pub(crate) fn binary_artifact_from_scan(
    symbol_names: &[String],
    string_region: &[u8],
    build_id: Option<&str>,
    macho_uuid: Option<&str>,
    pe_pdb: Option<&str>,
) -> BinaryArtifact {
    BinaryArtifact {
        exported_symbols: symbol_names.to_vec(),
        rodata_strings: extract_printable_strings(string_region, 4),
        build_id: build_id.map(str::to_string),
        macho_uuid: macho_uuid.map(str::to_string),
        pe_pdb: pe_pdb.map(str::to_string),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn empty_region_yields_no_strings() {
        let out = extract_printable_strings(b"", 4);
        assert!(out.is_empty());
    }

    #[test]
    fn extracts_simple_null_separated_strings() {
        let region = b"hello\0world\0";
        let out = extract_printable_strings(region, 4);
        assert_eq!(out, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn discards_runs_shorter_than_min_len() {
        let region = b"abc\0longerstring\0xy\0";
        let out = extract_printable_strings(region, 4);
        assert_eq!(out, vec!["longerstring".to_string()]);
    }

    #[test]
    fn extracts_openssl_version_literal_from_realistic_region() {
        // Simulates a `.rodata` section containing the OpenSSL version
        // banner amid filler bytes (NULs + a non-printable byte that
        // terminates a run).
        let mut region: Vec<u8> = Vec::new();
        region.extend_from_slice(b"junk\0");
        region.extend_from_slice(b"OpenSSL 3.1.4 19 Oct 2023");
        region.push(0xFF); // non-printable terminator
        region.extend_from_slice(b"another\0");
        let out = extract_printable_strings(&region, 4);
        assert!(out.iter().any(|s| s.contains("OpenSSL 3.1.4")));
        assert!(out.iter().any(|s| s == "junk"));
        assert!(out.iter().any(|s| s == "another"));
    }

    #[test]
    fn tail_run_is_flushed_when_no_terminator() {
        let region = b"start\0unterminatedtail";
        let out = extract_printable_strings(region, 4);
        assert_eq!(out, vec!["start".to_string(), "unterminatedtail".to_string()]);
    }

    #[test]
    fn tab_byte_does_not_terminate_run() {
        // Tab (0x09) is treated as printable so multi-word strings with
        // tabs between fields survive intact.
        let region = b"col1\tcol2\tcol3\0";
        let out = extract_printable_strings(region, 4);
        assert_eq!(out, vec!["col1\tcol2\tcol3".to_string()]);
    }

    #[test]
    fn high_bit_byte_terminates_run() {
        // Non-ASCII byte (0x80+) breaks runs. The matcher's substring-
        // pattern use case targets ASCII literals.
        let region = b"asciipart\xC3\xA9more";
        let out = extract_printable_strings(region, 4);
        assert_eq!(out, vec!["asciipart".to_string(), "more".to_string()]);
    }

    #[test]
    fn binary_artifact_from_scan_populates_all_fields() {
        let symbol_names = vec!["sym1".to_string(), "sym2".to_string()];
        let region = b"OpenSSL 3.1.4\0";
        let artifact = binary_artifact_from_scan(
            &symbol_names,
            region,
            Some("abc123"),
            None,
            Some("deadbeef:1"),
        );
        assert_eq!(artifact.exported_symbols, symbol_names);
        assert!(artifact.rodata_strings.iter().any(|s| s.contains("OpenSSL 3.1.4")));
        assert_eq!(artifact.build_id.as_deref(), Some("abc123"));
        assert!(artifact.macho_uuid.is_none());
        assert_eq!(artifact.pe_pdb.as_deref(), Some("deadbeef:1"));
    }

    #[test]
    fn binary_artifact_from_scan_handles_empty_inputs() {
        let artifact = binary_artifact_from_scan(&[], b"", None, None, None);
        assert!(artifact.exported_symbols.is_empty());
        assert!(artifact.rodata_strings.is_empty());
        assert!(artifact.build_id.is_none());
        assert!(artifact.macho_uuid.is_none());
        assert!(artifact.pe_pdb.is_none());
    }
}
