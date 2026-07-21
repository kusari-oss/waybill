//! Unit-level sanity tests for the transitive-parity audit harness.
//!
//! Cargo's integration-test sharing pattern: `tests/transitive_parity_common/mod.rs`
//! is the shared module; each integration-test target that wants its
//! types declares `mod transitive_parity_common;`. This file runs the
//! parsing + normalization + diff sanity checks without touching real
//! fixtures or external tools.

mod transitive_parity_common;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::transitive_parity_common::*;
    use serde_json::json;

    #[test]
    fn extract_edges_finds_depends_on() {
        let doc = json!({
            "packages": [
                {
                    "SPDXID": "SPDXRef-A",
                    "externalRefs": [{
                        "referenceType": "purl",
                        "referenceLocator": "pkg:cargo/serde@1.0.0"
                    }]
                },
                {
                    "SPDXID": "SPDXRef-B",
                    "externalRefs": [{
                        "referenceType": "purl",
                        "referenceLocator": "pkg:cargo/serde_derive@1.0.0"
                    }]
                }
            ],
            "relationships": [
                {
                    "spdxElementId": "SPDXRef-A",
                    "relatedSpdxElement": "SPDXRef-B",
                    "relationshipType": "DEPENDS_ON"
                }
            ]
        });
        let edges = extract_edges_spdx_2_3(&doc);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, "pkg:cargo/serde@1.0.0");
        assert_eq!(edges[0].to, "pkg:cargo/serde_derive@1.0.0");
    }

    #[test]
    fn extract_edges_skips_non_depends_on() {
        let doc = json!({
            "packages": [{
                "SPDXID": "SPDXRef-A",
                "externalRefs": [{
                    "referenceType": "purl",
                    "referenceLocator": "pkg:cargo/foo@1.0"
                }]
            }],
            "relationships": [
                {
                    "spdxElementId": "SPDXRef-DOCUMENT",
                    "relatedSpdxElement": "SPDXRef-A",
                    "relationshipType": "DESCRIBES"
                }
            ]
        });
        assert_eq!(extract_edges_spdx_2_3(&doc).len(), 0);
    }

    #[test]
    fn normalize_purl_lowercases_type_and_strips_qualifiers() {
        assert_eq!(
            normalize_purl("pkg:Cargo/serde@1.0.0"),
            "pkg:cargo/serde@1.0.0"
        );
        assert_eq!(
            normalize_purl("pkg:maven/com.google.guava/guava@32.1.3-jre?repository_url=https://repo.maven.apache.org"),
            "pkg:maven/com.google.guava/guava@32.1.3-jre"
        );
        assert_eq!(
            normalize_purl("pkg:cargo/foo@1.0"),
            "pkg:cargo/foo@1.0"
        );
    }

    #[test]
    fn diff_unanimous_when_all_three_match() {
        let edges = vec![Edge::new("a", "b"), Edge::new("c", "d")];
        let diff = compute_edge_diff(&edges, &edges, &edges);
        assert!(diff.is_unanimous());
        assert!(!diff.requires_tiebreaker());
        assert_eq!(diff.agreement.len(), 2);
    }

    #[test]
    fn diff_detects_mikebom_only() {
        let mikebom = vec![Edge::new("a", "b"), Edge::new("c", "d")];
        let trivy = vec![Edge::new("a", "b")];
        let syft = vec![Edge::new("a", "b")];
        let diff = compute_edge_diff(&mikebom, &trivy, &syft);
        assert!(!diff.is_unanimous());
        assert_eq!(diff.mikebom_only.len(), 1);
        assert!(diff.mikebom_only.contains(&Edge::new("c", "d")));
    }

    #[test]
    fn maybe_skip_returns_none_when_no_tools_required() {
        let result = maybe_skip(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn maybe_skip_returns_some_when_tool_missing_and_strict_unset() {
        // Ensure strict mode is off for this test (other tests in
        // the same process might have set it).
        std::env::remove_var("MIKEBOM_REQUIRE_TRANSITIVE_PARITY");
        let result = maybe_skip(&["this-tool-definitely-does-not-exist"]);
        assert!(result.is_some());
        assert!(result.unwrap().contains("this-tool-definitely-does-not-exist"));
    }

    #[test]
    fn skip_on_macos_for_os_package_skips_dpkg() {
        if cfg!(target_os = "macos") {
            assert!(skip_on_macos_for_os_package("dpkg").is_some());
            assert!(skip_on_macos_for_os_package("rpm").is_some());
            assert!(skip_on_macos_for_os_package("apk").is_some());
            assert!(skip_on_macos_for_os_package("cargo").is_none());
        } else {
            // On Linux these all return None — fixture isn't skipped.
            assert!(skip_on_macos_for_os_package("dpkg").is_none());
        }
    }
}
