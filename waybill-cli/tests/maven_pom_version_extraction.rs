//! Integration tests for milestone 092 — Maven pom.xml version-extraction
//! bug fix. Covers the FR-001 / FR-002 / FR-003 trio (project-version
//! precedence, parent-version fallback, both-missing skip) plus the
//! property-substitution preservation cases (US2 / Contracts 3 + 4).
//!
//! These tests build synthetic pom.xml fixtures in tempdirs so they
//! exercise the parser + main-module emission paths without requiring
//! the milestone-090 fixture-cache repo. The transitive_parity_maven
//! test (waybill-cli/tests/transitive_parity_maven.rs) provides the
//! complementary audit-fixture coverage.

use std::path::Path;
use std::process::Command;

fn scan_to_cdx(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

/// Milestone 092 / FR-001 / SC-001: when a pom.xml omits project-level
/// `<groupId>` (inherits from `<parent>`) but declares its own
/// project-level `<version>`, the emitted main-module PURL MUST use
/// the project's own version, NOT the parent's. This is the exact bug
/// triggered by the milestone-083 commons-lang3 audit fixture.
///
/// Pre-092 emits `pkg:maven/org.apache.commons/commons-lang3@64`
/// (parent's version); post-092 emits
/// `pkg:maven/org.apache.commons/commons-lang3@3.14.0`.
#[test]
fn main_module_emits_project_version_when_groupid_inherited_from_parent() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <parent>
    <groupId>org.apache.commons</groupId>
    <artifactId>commons-parent</artifactId>
    <version>64</version>
  </parent>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>commons-lang3</artifactId>
  <version>3.14.0</version>
</project>"#,
    )
    .unwrap();
    let cdx = scan_to_cdx(dir.path());
    let purl = cdx["metadata"]["component"]["purl"]
        .as_str()
        .expect("metadata.component.purl");
    assert_eq!(
        purl, "pkg:maven/org.apache.commons/commons-lang3@3.14.0",
        "main-module PURL must use project's own version (3.14.0), \
         not parent's (64); got {purl}"
    );
}

/// Milestone 092 / FR-002: when a pom.xml omits project-level
/// `<version>` (intentional inheritance — child relies on parent's
/// version), the emitted main-module PURL MUST fall back to the
/// parent's version. This case worked correctly pre-092 by accident
/// (the buggy "first-version-wins" path matched the parent slot);
/// post-092 the fallback chain explicitly handles it.
#[test]
fn main_module_falls_back_to_parent_version_when_project_version_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>1.0</version>
  </parent>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>child</artifactId>
</project>"#,
    )
    .unwrap();
    let cdx = scan_to_cdx(dir.path());
    let purl = cdx["metadata"]["component"]["purl"]
        .as_str()
        .expect("metadata.component.purl");
    assert_eq!(
        purl, "pkg:maven/com.example/child@1.0",
        "main-module PURL must inherit parent's version when project-level \
         <version> is absent; got {purl}"
    );
}

/// Milestone 092 / FR-003: when BOTH project-level `<version>` and
/// `<parent>/<version>` are absent, waybill MUST NOT emit a main-module
/// component (the existing `is_empty()` guard at maven.rs:3436 catches
/// it). No malformed `pkg:maven/.../<artifact>@` PURL appears.
#[test]
fn main_module_emits_nothing_when_both_versions_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
  </parent>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>orphan</artifactId>
</project>"#,
    )
    .unwrap();
    let cdx = scan_to_cdx(dir.path());
    // Either no metadata.component.purl is emitted, or the emitted
    // root is the synthetic workspace placeholder (NOT a maven PURL).
    // Verify no maven PURL with the @-version-empty shape appears
    // anywhere in the output.
    let raw = serde_json::to_string(&cdx).expect("re-serialize");
    assert!(
        !raw.contains("pkg:maven/com.example/orphan@\""),
        "must NOT emit maven PURL with empty version; raw contains: {raw}"
    );
    assert!(
        !raw.contains("pkg:maven/com.example/orphan@,"),
        "must NOT emit maven PURL with empty version (comma-terminated)"
    );
    // Also assert the metadata.component.purl, if present, is not a
    // malformed maven PURL.
    if let Some(purl) = cdx["metadata"]["component"]["purl"].as_str() {
        assert!(
            !purl.starts_with("pkg:maven/com.example/orphan"),
            "metadata.component.purl must not be a malformed maven PURL; \
             got {purl}"
        );
    }
}

/// Milestone 092 / US2 / Contract 3: existing property-substitution path
/// continues to work post-fix when the pom uses `${revision}`-style
/// version AND omits project-level `<groupId>`. Ensures the
/// milestone-092 fix doesn't break the `<groupId>`-inherited +
/// property-substituted-version combination.
#[test]
fn main_module_resolves_revision_property_when_groupid_inherited() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>10.0</version>
  </parent>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>app</artifactId>
  <version>${revision}</version>
  <properties>
    <revision>1.2.3</revision>
  </properties>
</project>"#,
    )
    .unwrap();
    let cdx = scan_to_cdx(dir.path());
    let purl = cdx["metadata"]["component"]["purl"]
        .as_str()
        .expect("metadata.component.purl");
    assert_eq!(
        purl, "pkg:maven/com.example/app@1.2.3",
        "property substitution must resolve ${{revision}} to 1.2.3 (not parent's 10.0); \
         got {purl}"
    );
}

/// Milestone 092 / US2 / Contracts 3 + 4: a dep with
/// `<version>${{project.version}}</version>` inside a pom that omits
/// project-level `<groupId>` MUST resolve to the project's own version,
/// NOT the parent's. Exercises both `resolve_pom_property_value`'s
/// "project.version" arm AND `resolve_maven_property`'s.
#[test]
fn dep_version_uses_project_version_property_when_groupid_inherited() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>10.0</version>
  </parent>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>app</artifactId>
  <version>3.14.0</version>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>sibling</artifactId>
      <version>${project.version}</version>
    </dependency>
  </dependencies>
</project>"#,
    )
    .unwrap();
    let cdx = scan_to_cdx(dir.path());
    // Main-module PURL is the project's own 3.14.0.
    let main_purl = cdx["metadata"]["component"]["purl"]
        .as_str()
        .expect("metadata.component.purl");
    assert_eq!(main_purl, "pkg:maven/com.example/app@3.14.0");
    // The dep should resolve via property substitution to the
    // project's version (3.14.0), NOT the parent's (10.0). We assert
    // by serializing the whole document and checking the absence of
    // the wrong version paired with sibling.
    let raw = serde_json::to_string(&cdx).expect("re-serialize");
    assert!(
        !raw.contains("pkg:maven/com.example/sibling@10.0"),
        "${{project.version}} on sibling dep must NOT resolve to parent's \
         version (10.0); raw contains: {raw}"
    );
    // Note: we don't assert on the *presence* of `sibling@3.14.0`
    // because the dep edge target is only emitted as a component if a
    // matching cached jar exists — but the version *string* in any
    // edge or dep entry must be 3.14.0 (not 10.0). The negative
    // assertion above is sufficient to gate the regression.
}
