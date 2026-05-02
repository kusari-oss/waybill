//! Integration tests for the Go ecosystem (milestone 003 US1).
//!
//! Covers the four spec-declared scenarios:
//!
//! 1. Source-tree scan (`go.mod` + `go.sum`) emits canonical PURLs for
//!    every go.sum `Module`-kind line plus the main module from go.mod.
//! 2. Binary scan (`runtime/debug.BuildInfo`) emits analyzed-tier
//!    components for the embedded module list.
//! 3. A binary with no readable BuildInfo (synthesized in-test with a
//!    truncated payload) emits a file-level diagnostic entry.
//! 4. A scratch "image-shaped" rootfs (bare binary, no go.mod) still
//!    produces the full module list — that's the distroless win.
//!
//! All four shell out to the `mikebom` CLI (same pattern as
//! `scan_python.rs` / `scan_npm.rs`).

use std::path::PathBuf;
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tests/fixtures/go")
        .join(sub)
}

fn scan_path(path: &std::path::Path) -> serde_json::Value {
    scan_path_args(path, &[])
}

fn scan_path_args(path: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn golang_purls(sbom: &serde_json::Value) -> Vec<String> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter_map(|c| {
            let p = c["purl"].as_str()?;
            if p.starts_with("pkg:golang/") {
                Some(p.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn go_sum_module_count(fixture_sub: &str) -> usize {
    let go_sum = fixture(fixture_sub).join("go.sum");
    let text = std::fs::read_to_string(&go_sum)
        .unwrap_or_else(|_| panic!("fixture {fixture_sub}/go.sum must exist"));
    text.lines()
        .filter(|l| {
            let mut parts = l.split_whitespace();
            let _m = parts.next();
            let v = parts.next();
            matches!(v, Some(v) if !v.ends_with("/go.mod"))
        })
        .count()
}

// --- T029: source-tree scan --------------------------------------------

#[test]
fn scan_go_source_tree_emits_canonical_purls() {
    let sbom = scan_path(&fixture("simple-module"));
    let purls = golang_purls(&sbom);
    let gosum_modules = go_sum_module_count("simple-module");
    // SC-001 tolerance: we expect at least gosum_modules components
    // (plus the main module from go.mod). The scanner might drop one
    // for a replace-to-local target; the tolerance is `gosum_modules - 1`.
    assert!(
        purls.len() >= gosum_modules.saturating_sub(1),
        "expected ≥{} golang components, got {}: {purls:?}",
        gosum_modules.saturating_sub(1),
        purls.len(),
    );
    // The main module (workspace root) is intentionally NOT emitted
    // — same semantics as cargo/npm/maven workspace filters. Only
    // transitive deps from go.sum surface as components.
    assert!(
        purls.iter().all(|p| !p.contains("example.com/simple")),
        "workspace root PURL should not be emitted: {purls:?}",
    );
    // Canonical PURLs always have `pkg:golang/` + a `/`-separated module path.
    for p in &purls {
        assert!(
            p.starts_with("pkg:golang/"),
            "non-canonical Go PURL: {p}"
        );
    }
}

// --- T030: binary BuildInfo scan --------------------------------------

#[test]
fn scan_go_binary_emits_buildinfo_modules() {
    let sbom = scan_path(&fixture("binaries"));
    let purls = golang_purls(&sbom);
    // ≥3: at least main + cobra + logrus — the simple-module binary
    // pulls in nine transitive deps by construction.
    assert!(
        purls.len() >= 3,
        "expected ≥3 golang components from binary, got {}: {purls:?}",
        purls.len(),
    );
    // Specific modules we know should be present.
    let must_have = ["github.com/spf13/cobra", "github.com/sirupsen/logrus"];
    for needle in must_have {
        assert!(
            purls.iter().any(|p| p.contains(needle)),
            "expected PURL containing {needle}, got {purls:?}",
        );
    }
    // aggregate=complete for the golang ecosystem.
    let compositions = sbom["compositions"].as_array();
    assert!(
        compositions.is_some_and(|c| c.iter().any(|comp| {
            comp["aggregate"].as_str() == Some("complete")
                && comp["assemblies"]
                    .as_array()
                    .map(|asm| {
                        asm.iter()
                            .any(|s| s.as_str().unwrap_or("").starts_with("pkg:golang/"))
                    })
                    .unwrap_or(false)
        })),
        "golang aggregate=complete composition expected",
    );
}

// --- T031: stripped / unreadable binary emits diagnostic --------------

#[test]
fn scan_go_stripped_binary_emits_diagnostic_property() {
    // Build a synthetic rootfs with a single Go-magic-bearing file that
    // is deliberately malformed (truncated after the header). This
    // simulates a stripped binary where the BuildInfo section is gone
    // but the magic bytes happen to be elsewhere in the binary.
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_path = dir.path().join("corrupted");
    let mut bytes = vec![0u8; 4096];
    // Append the magic + a non-inline flags byte to trigger the
    // "unsupported" path.
    let magic = b"\xff Go buildinf:";
    let mut header: Vec<u8> = Vec::new();
    header.extend_from_slice(magic);
    header.push(8); // ptr size
    header.push(0x0); // no inline flag → unsupported
    header.extend_from_slice(&[0u8; 16]);
    bytes.extend_from_slice(&header);
    std::fs::write(&bin_path, &bytes).expect("write bin");

    let sbom = scan_path(dir.path());
    // Exit 0 is implicit (scan_path asserts success). We expect one
    // file-level diagnostic component — it carries a generic PURL
    // with the filename, and the `mikebom:buildinfo-status` property.
    let diagnostics: Vec<_> = sbom["components"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|c| {
            c["properties"]
                .as_array()
                .map(|props| {
                    props.iter().any(|p| {
                        p["name"].as_str() == Some("mikebom:buildinfo-status")
                    })
                })
                .unwrap_or(false)
        })
        .collect();
    assert!(
        !diagnostics.is_empty(),
        "expected ≥1 component with mikebom:buildinfo-status property; got components: {}",
        serde_json::to_string_pretty(&sbom["components"]).unwrap_or_default(),
    );
    let status = diagnostics[0]["properties"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["name"].as_str() == Some("mikebom:buildinfo-status")))
        .and_then(|p| p["value"].as_str())
        .unwrap_or("");
    assert!(
        status == "unsupported" || status == "missing",
        "unexpected buildinfo-status value: {status}",
    );
}

// --- Transitive dep-graph via module cache ---------------------------

#[test]
fn scan_go_source_tree_emits_transitive_edges_when_cache_present() {
    // The Go module cache discovery honours $GOMODCACHE / $HOME/go.
    // If neither points at a populated cache on the test runner, the
    // graph is expected to be empty beyond the root — skip rather
    // than fail, since this test is observational of a real cache.
    let gomodcache = std::env::var("GOMODCACHE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|h| std::path::PathBuf::from(h).join("go/pkg/mod"))
        });
    let Some(cache_root) = gomodcache else {
        eprintln!("skipping: no GOMODCACHE or HOME/go/pkg/mod");
        return;
    };
    let cached_cobra_mod = cache_root
        .join("cache/download/github.com/spf13/cobra/@v/v1.10.2.mod");
    if !cached_cobra_mod.is_file() {
        eprintln!(
            "skipping: no cached cobra/@v/v1.10.2.mod at {}",
            cached_cobra_mod.display()
        );
        return;
    }

    let sbom = scan_path(&fixture("simple-module"));
    let deps = sbom["dependencies"]
        .as_array()
        .expect("dependencies array");
    let go_deps: Vec<_> = deps
        .iter()
        .filter(|d| {
            d["ref"]
                .as_str()
                .is_some_and(|s| s.starts_with("pkg:golang/"))
        })
        .collect();
    let with_edges: Vec<_> = go_deps
        .iter()
        .filter(|d| {
            d.get("dependsOn")
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty())
        })
        .collect();
    // Root + ≥2 transitive nodes with outbound edges (logrus + cobra
    // both declare their own requires in their cached .mod files).
    assert!(
        with_edges.len() >= 3,
        "expected ≥3 golang records with dependsOn edges, got {}",
        with_edges.len(),
    );
    // logrus → x/sys specifically.
    let logrus = go_deps
        .iter()
        .find(|d| {
            d["ref"]
                .as_str()
                .is_some_and(|s| s.starts_with("pkg:golang/github.com/sirupsen/logrus@"))
        })
        .expect("logrus dependency record");
    let logrus_targets: Vec<String> = logrus["dependsOn"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        logrus_targets
            .iter()
            .any(|t| t.contains("pkg:golang/golang.org/x/sys@")),
        "logrus → x/sys edge missing (cached go.mod resolution failed): {logrus_targets:?}",
    );
}

// --- T032: scratch / distroless image emits binary-sourced modules ----

#[test]
fn scan_go_scratch_rootfs_via_path_flag() {
    // Simulate a scratch image by copying just the binary into a bare
    // directory — no go.mod, no /etc, no other signals. This is the
    // "distroless win" spec scenario.
    let dir = tempfile::tempdir().expect("tempdir");
    let src = fixture("binaries").join("hello-linux-amd64");
    let dst = dir.path().join("app");
    std::fs::copy(&src, &dst).expect("copy binary into scratch rootfs");

    let sbom = scan_path(dir.path());
    let purls = golang_purls(&sbom);
    assert!(
        purls.len() >= 3,
        "scratch scan produced too few golang components: {purls:?}",
    );
    // Dedup test: these modules match the source-tree fixture set, so
    // the same PURL shape is expected.
    assert!(
        purls.iter().any(|p| p.contains("github.com/spf13/cobra")),
        "cobra missing from scratch scan: {purls:?}",
    );
}

// --- G1: dual-identity — file-level + module-level for Go binaries ----

#[test]
fn scan_go_binary_emits_both_generic_file_and_golang_module_components() {
    // The ground truth for a compiled Go binary counts both:
    //   - `pkg:generic/<basename>?file-sha256=...` — the binary file
    //     identity (same shape the binary walker emits for every
    //     non-Go ELF/Mach-O/PE).
    //   - `pkg:golang/<module>@<version>` — the Go module identity
    //     from the embedded BuildInfo (emitted by
    //     `package_db::go_binary`).
    //
    // Pre-G1, mikebom only emitted the golang one — file-level was
    // suppressed for Go binaries on Linux. Post-G1, both emit with
    // `mikebom:detected-go = true` on the file-level entry as a
    // cross-link marker.
    let dir = tempfile::tempdir().expect("tempdir");
    let src = fixture("binaries").join("hello-linux-amd64");
    let dst = dir.path().join("goapp");
    std::fs::copy(&src, &dst).expect("copy binary");
    // G1's `detected_go` cross-link wiring fires only when the
    // binary walker's `go_in_linux` predicate matches, which
    // requires the rootfs to be detected as Linux. Plant a minimal
    // `/etc/os-release` so `detect_rootfs_kind` returns Linux.
    let etc = dir.path().join("etc");
    std::fs::create_dir_all(&etc).unwrap();
    std::fs::write(etc.join("os-release"), "ID=debian\nVERSION_ID=\"12\"\n").unwrap();

    let sbom = scan_path(dir.path());
    let components = sbom["components"]
        .as_array()
        .expect("components array");

    // File-level `pkg:generic/goapp?file-sha256=...` must be present.
    let file_level = components.iter().find(|c| {
        c["purl"]
            .as_str()
            .is_some_and(|p| p.starts_with("pkg:generic/goapp"))
    });
    assert!(
        file_level.is_some(),
        "pkg:generic/goapp file-level component missing; \
         got purls = {:?}",
        components
            .iter()
            .filter_map(|c| c["purl"].as_str())
            .collect::<Vec<_>>(),
    );
    // `mikebom:detected-go = true` marks the file-level as a Go binary.
    let props = file_level.unwrap()["properties"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let detected_go = props
        .iter()
        .find(|p| p["name"].as_str() == Some("mikebom:detected-go"))
        .and_then(|p| p["value"].as_str().map(|s| s.to_string()));
    assert_eq!(
        detected_go.as_deref(),
        Some("true"),
        "file-level Go binary must carry mikebom:detected-go=true; \
         props = {props:?}",
    );

    // Module-level `pkg:golang/...` entries still emit alongside.
    let golang = golang_purls(&sbom);
    assert!(
        !golang.is_empty(),
        "golang module components must still emit alongside file-level",
    );
}

// --- G3: filter go.sum against Go binary BuildInfo ------------------

#[test]
fn scan_go_source_plus_binary_filters_go_sum_to_linked_subset() {
    // G3 regression: when a scan carries both a go.sum and a Go
    // binary's BuildInfo, go.sum entries that don't appear in the
    // binary's linked-module set get dropped. Polyglot-builder-
    // image emits 22 go.sum entries, only 7 of which are actually
    // linked — the other 15 are test/tool transitives go.sum
    // carries for lockfile completeness but that don't ship in
    // the binary.
    //
    // Reproduction: the `hello-linux-amd64` fixture contains
    // BuildInfo with a known module set. We add an invented
    // `github.com/never-linked/fake v9.9.9` to go.sum that's NOT
    // in BuildInfo. Post-G3, it must be dropped.
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("opt/goapp");
    std::fs::create_dir_all(&app).unwrap();
    // Drop the binary in place.
    let bin_src = fixture("binaries").join("hello-linux-amd64");
    std::fs::copy(&bin_src, app.join("appbin")).expect("copy binary");
    // Write go.mod + go.sum. go.sum lists the `never-linked/fake`
    // module plus a handful of the binary's real BuildInfo
    // entries, so the filter has BOTH a keep case and a drop
    // case to exercise.
    std::fs::write(
        app.join("go.mod"),
        "module example.com/app\ngo 1.22\nrequire github.com/never-linked/fake v9.9.9\n",
    )
    .unwrap();
    std::fs::write(
        app.join("go.sum"),
        concat!(
            "github.com/never-linked/fake v9.9.9 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            "github.com/sirupsen/logrus v1.9.4 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            "github.com/davecgh/go-spew v1.1.1 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
        ),
    )
    .unwrap();

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);

    // Milestone 050: the never-linked module IS retained (no longer
    // dropped) and carries a `mikebom:not-linked = true` property
    // identifying it as in-go.sum-but-not-in-BuildInfo. Consumers
    // wanting the strict "what shipped" view filter on this property.
    assert!(
        golang.iter().any(|p| p.contains("never-linked/fake")),
        "never-linked/fake must be RETAINED (milestone 050 — \
         tagged not dropped): {golang:?}",
    );
    let fake_props = sbom["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.contains("never-linked/fake"))
        })
        .and_then(|c| c["properties"].as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        fake_props.iter().any(|p| {
            p["name"].as_str() == Some("mikebom:not-linked")
                && (p["value"].as_str() == Some("true")
                    || p["value"].as_bool() == Some(true))
        }),
        "never-linked/fake must carry mikebom:not-linked = true: \
         props={fake_props:?}",
    );

    // The binary's BuildInfo modules (logrus, go-spew, etc.) DO
    // emit — as analyzed-tier entries from go_binary.rs and/or
    // via dedup'd source+analyzed collapse.
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus")),
        "logrus from BuildInfo must emit: {golang:?}",
    );
    assert!(
        golang.iter().any(|p| p.contains("davecgh/go-spew")),
        "go-spew from BuildInfo must emit: {golang:?}",
    );
}

#[test]
fn scan_go_source_only_preserves_full_go_sum() {
    // Source-tree-only scan: go.mod + go.sum present, no compiled
    // binary. G3 filter must no-op because the analyzed-tier set
    // is empty. Every go.sum entry emits as source-tier, including
    // coords that would be dropped in a source+binary scan.
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("go.mod"),
        "module example.com/sourceonly\ngo 1.22\nrequire github.com/never-linked/fake v9.9.9\n",
    )
    .unwrap();
    std::fs::write(
        app.join("go.sum"),
        concat!(
            "github.com/never-linked/fake v9.9.9 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            "github.com/also-never-linked/other v1.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
        ),
    )
    .unwrap();

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);

    // Both "never-linked" modules should survive — there's no
    // BuildInfo to filter against, so go.sum is authoritative.
    assert!(
        golang.iter().any(|p| p.contains("never-linked/fake")),
        "never-linked/fake must survive in source-only scan: {golang:?}",
    );
    assert!(
        golang.iter().any(|p| p.contains("also-never-linked/other")),
        "also-never-linked/other must survive in source-only scan: {golang:?}",
    );
}

// --- 007 US2: Go test-scope intersection filter (G4) ------------------

#[test]
fn scan_go_source_test_only_import_is_tagged_and_droppable() {
    // Milestone 052/part-3: when a module is imported only from a
    // `_test.go` file, default mode emits it tagged with native CDX
    // scope: "excluded" + mikebom:lifecycle-scope: "test".
    // --exclude-scope dev,build,test restores the strict pre-052
    // "drop test-only" view (G4 filter behavior).
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("go.mod"),
        "module example.com/us2\n\
         go 1.22\n\
         require github.com/sirupsen/logrus v1.9.4\n\
         require github.com/stretchr/testify v1.11.1\n",
    )
    .unwrap();
    std::fs::write(
        app.join("go.sum"),
        concat!(
            "github.com/sirupsen/logrus v1.9.4 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            "github.com/stretchr/testify v1.11.1 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
        ),
    )
    .unwrap();
    // main.go imports logrus from production code.
    std::fs::write(
        app.join("main.go"),
        "package main\n\
         import \"github.com/sirupsen/logrus\"\n\
         func main() { logrus.Info(\"hi\") }\n",
    )
    .unwrap();
    // main_test.go imports testify — test-scope only.
    std::fs::write(
        app.join("main_test.go"),
        "package main\n\
         import (\n\
             \"testing\"\n\
             \"github.com/stretchr/testify/assert\"\n\
         )\n\
         func TestMain(t *testing.T) { assert.Equal(t, 1, 1) }\n",
    )
    .unwrap();

    // Default mode (post-052): testify present + tagged.
    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus")),
        "logrus (production import) must be retained: {golang:?}",
    );
    assert!(
        golang.iter().any(|p| p.contains("stretchr/testify")),
        "testify (test-only import) must emit in default mode (post-052): {golang:?}",
    );

    // --exclude-scope dev,build,test: testify dropped.
    let sbom_strict = scan_path_args(dir.path(), &["--exclude-scope", "dev,build,test"]);
    let golang_strict = golang_purls(&sbom_strict);
    assert!(
        golang_strict.iter().any(|p| p.contains("sirupsen/logrus")),
        "logrus must survive --exclude-scope: {golang_strict:?}",
    );
    assert!(
        !golang_strict.iter().any(|p| p.contains("stretchr/testify")),
        "testify (test-only) must be dropped under --exclude-scope dev,build,test: {golang_strict:?}",
    );
}

#[test]
fn scan_go_source_production_and_test_import_dominates() {
    // FR-008: when a module is imported from BOTH production and
    // test files, production wins and the module is retained.
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("go.mod"),
        "module example.com/us2b\n\
         go 1.22\n\
         require github.com/sirupsen/logrus v1.9.4\n",
    )
    .unwrap();
    std::fs::write(
        app.join("go.sum"),
        "github.com/sirupsen/logrus v1.9.4 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
    )
    .unwrap();
    std::fs::write(
        app.join("main.go"),
        "package main\n\
         import \"github.com/sirupsen/logrus\"\n\
         func main() { logrus.Info(\"hi\") }\n",
    )
    .unwrap();
    std::fs::write(
        app.join("main_test.go"),
        "package main\n\
         import (\n\
             \"testing\"\n\
             \"github.com/sirupsen/logrus\"\n\
         )\n\
         func TestMain(t *testing.T) { logrus.Info(\"test\") }\n",
    )
    .unwrap();

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus")),
        "logrus imported from both main.go and main_test.go must be retained: {golang:?}",
    );
}

// --- 007 US3: Go main-module exclusion (G5) ---------------------------

#[test]
fn scan_go_main_module_from_gomod_is_suppressed() {
    // FR-010 / FR-012: go.mod's `module` directive names the project
    // itself. Even when go.sum somehow lists it, mikebom must drop
    // the self-reference.
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("go.mod"),
        "module example.com/polyglot-fixture\n\
         go 1.22\n\
         require github.com/sirupsen/logrus v1.9.4\n",
    )
    .unwrap();
    std::fs::write(
        app.join("go.sum"),
        concat!(
            "github.com/sirupsen/logrus v1.9.4 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            // Simulated self-reference in go.sum — unusual but has
            // happened in the wild (and it's the polyglot case):
            "example.com/polyglot-fixture v0.0.0-000 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
        ),
    )
    .unwrap();
    std::fs::write(
        app.join("main.go"),
        "package main\n\
         import \"github.com/sirupsen/logrus\"\n\
         func main() { logrus.Info(\"hi\") }\n",
    )
    .unwrap();

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);
    assert!(
        !golang
            .iter()
            .any(|p| p.contains("example.com/polyglot-fixture")),
        "main module self-reference must be dropped: {golang:?}",
    );
    // Real dep still there.
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus")),
        "real dep must not be affected by main-module filter: {golang:?}",
    );
}

#[test]
fn scan_go_main_module_from_binary_buildinfo_is_suppressed() {
    // FR-010 via BuildInfo: the hello-linux-amd64 fixture's BuildInfo
    // declares `mod example.com/simple (devel)`. That should be
    // dropped as a main-module self-reference.
    let dir = tempfile::tempdir().expect("tempdir");
    let src = fixture("binaries").join("hello-linux-amd64");
    let dst = dir.path().join("app");
    std::fs::copy(&src, &dst).expect("copy binary");

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);
    assert!(
        !golang.iter().any(|p| p.contains("example.com/simple")),
        "binary's main module (example.com/simple) must be dropped: {golang:?}",
    );
    // Dependencies from BuildInfo must still be present.
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus")),
        "BuildInfo dep logrus must be retained: {golang:?}",
    );
}

// --- 008 US2: G6 cache-ZIP filter ---------------------------------------

#[test]
fn scan_go_cache_zip_not_in_buildinfo_is_dropped() {
    // Feature 008 US2 polyglot scenario: rootfs contains a Go binary
    // whose BuildInfo lists a set of linked modules, AND a
    // `/go/pkg/mod/cache/download/` tree with EXTRA test-scope
    // modules that the artifact walker would otherwise emit as
    // analyzed-tier golang components. G6 must drop the cache-ZIP-
    // only entries whose coord isn't also in BuildInfo.
    let dir = tempfile::tempdir().expect("tempdir");
    // Place the Go binary so its BuildInfo provides the linked set:
    // example.com/simple links logrus + ~8 other modules (see
    // `go version -m tests/fixtures/go/binaries/hello-linux-amd64`).
    let src = fixture("binaries").join("hello-linux-amd64");
    std::fs::copy(&src, dir.path().join("app")).expect("copy binary");
    // Synthesize cache-ZIP entries for modules NOT in the binary's
    // BuildInfo. Both must be dropped by G6. (Every module the
    // hello-linux-amd64 fixture actually links is uninteresting
    // here — it's the non-linked extras that should get dropped.)
    let cache_dir = dir.path().join("root/go/pkg/mod/cache/download");
    let orphan_a = cache_dir.join("github.com/never-linked/fake/@v");
    let orphan_b = cache_dir.join("gopkg.in/bogus-test-helper.v2/@v");
    std::fs::create_dir_all(&orphan_a).unwrap();
    std::fs::create_dir_all(&orphan_b).unwrap();
    // The path_resolver only needs the filename to match the
    // `<version>.zip` pattern; file contents aren't inspected.
    std::fs::write(orphan_a.join("v9.9.9.zip"), b"\x50\x4b\x03\x04fake zip").unwrap();
    std::fs::write(orphan_b.join("v2.0.0.zip"), b"\x50\x4b\x03\x04fake zip").unwrap();

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);

    // Both orphan modules are in cache but NOT in BuildInfo → G6
    // drops them.
    assert!(
        !golang.iter().any(|p| p.contains("never-linked/fake")),
        "never-linked/fake from cache/download/ must be dropped by \
         G6 since it isn't in the binary's BuildInfo: {golang:?}",
    );
    assert!(
        !golang
            .iter()
            .any(|p| p.contains("bogus-test-helper")),
        "bogus-test-helper from cache/download/ must be dropped: {golang:?}",
    );
    // BuildInfo deps must still be emitted (regression guard — G6
    // must not over-suppress).
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus")),
        "logrus from BuildInfo must be retained: {golang:?}",
    );
}

#[test]
fn scan_go_cache_zip_alone_is_retained_when_no_binary() {
    // Scratch/distroless scenario: ONLY cache-ZIP entries, no Go
    // binary on the rootfs. G6 must no-op and preserve cache-ZIP
    // entries as the authoritative signal (the "distroless win"
    // preserved per path_resolver::resolve_go_path design).
    let dir = tempfile::tempdir().expect("tempdir");
    let cache_dir = dir.path().join("root/go/pkg/mod/cache/download");
    let logrus_dir = cache_dir.join("github.com/sirupsen/logrus/@v");
    std::fs::create_dir_all(&logrus_dir).unwrap();
    std::fs::write(logrus_dir.join("v1.9.4.zip"), b"\x50\x4b\x03\x04fake zip").unwrap();

    let sbom = scan_path(dir.path());
    let golang = golang_purls(&sbom);
    assert!(
        golang.iter().any(|p| p.contains("sirupsen/logrus@v1.9.4")),
        "scratch scan must retain cache-ZIP entry when no \
         BuildInfo contradicts: {golang:?}",
    );
}

// --- Milestone 050: BuildInfo-vs-go.sum scope hint --------------------

/// Helper that mirrors `scan_path` but returns stderr alongside the
/// SBOM. Needed for the milestone-050 hint test, which asserts on
/// the `tracing::info` line rather than SBOM content.
fn scan_path_with_stderr(path: &std::path::Path) -> (serde_json::Value, String) {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    let sbom = serde_json::from_str(&raw).expect("valid JSON");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (sbom, stderr)
}

#[test]
fn scan_go_source_only_emits_buildinfo_scope_hint() {
    // Milestone 050 SC-001: when `mikebom sbom scan --path` finds a
    // go.mod but no built Go binary in the rootfs, emit a hint
    // explaining the SBOM scope and how to tighten it via
    // `go build` + the existing G3 BuildInfo intersection.
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("go.mod"),
        "module example.com/m050\n\
         go 1.22\n\
         require github.com/sirupsen/logrus v1.9.4\n",
    )
    .unwrap();
    std::fs::write(
        app.join("go.sum"),
        "github.com/sirupsen/logrus v1.9.4 h1:fake/sha==\n",
    )
    .unwrap();
    std::fs::write(
        app.join("main.go"),
        "package main\n\
         import \"github.com/sirupsen/logrus\"\n\
         func main() { logrus.Info(\"hi\") }\n",
    )
    .unwrap();

    let (_sbom, stderr) = scan_path_with_stderr(dir.path());
    assert!(
        stderr.contains("no Go binary found alongside go.mod")
            && stderr.contains("mikebom:not-linked"),
        "SC-001: hint must fire when go.mod parsed but no binary \
         present, naming the not-linked annotation. stderr was: {stderr}",
    );
}

#[test]
fn scan_go_non_go_project_does_not_emit_buildinfo_hint() {
    // Milestone 050 FR-004: hint MUST NOT fire when no go.mod is
    // parsed (i.e., not a Go project). The scan-mode condition is
    // gated on `go_signals.main_modules` being non-empty.
    let dir = tempfile::tempdir().expect("tempdir");
    // Empty rootfs — nothing for the Go reader to find.
    let (_sbom, stderr) = scan_path_with_stderr(dir.path());
    assert!(
        !stderr.contains("no Go binary found alongside go.mod"),
        "FR-004: hint must NOT fire on non-Go scans. stderr was: \
         {stderr}",
    );
}

// ===========================================================================
// Milestone 053 — Go main-module synthetic component (closes #102)
// ===========================================================================

/// Two-format scan helper used by the milestone-053 tests. Forces an
/// isolated `HOME` + empty `GOMODCACHE` so the resulting SBOM has the
/// "fresh-clone, no module cache" shape that issue #102 reproduces,
/// regardless of the developer's local Go cache state. Returns
/// `(spdx_2_3, cdx_1_6)` parsed JSON values.
fn scan_isolated_two_format(
    fixture_sub: &str,
    extra: &[&str],
) -> (serde_json::Value, serde_json::Value) {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let spdx_path = tmp.path().join("out.spdx.json");
    let cdx_path = tmp.path().join("out.cdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let empty_cache = tempfile::tempdir().expect("empty-cache tempdir");
    let mut cmd = Command::new(bin);
    cmd.env("HOME", fake_home.path())
        .env("GOMODCACHE", empty_cache.path().join("empty"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture(fixture_sub))
        .arg("--format")
        .arg("spdx-2.3-json,cyclonedx-json")
        .arg("--output")
        .arg(format!(
            "spdx-2.3-json={}",
            spdx_path.to_string_lossy()
        ))
        .arg("--output")
        .arg(format!(
            "cyclonedx-json={}",
            cdx_path.to_string_lossy()
        ))
        .arg("--no-deep-hash");
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let spdx_raw = std::fs::read_to_string(&spdx_path).expect("read spdx");
    let cdx_raw = std::fs::read_to_string(&cdx_path).expect("read cdx");
    (
        serde_json::from_str(&spdx_raw).expect("valid spdx JSON"),
        serde_json::from_str(&cdx_raw).expect("valid cdx JSON"),
    )
}

/// Find the SPDX 2.3 main-module package — the one carrying
/// `primaryPackagePurpose: "APPLICATION"`. Panics if zero or more
/// than one is present (milestone 053 invariant: exactly one
/// main-module per Go-only scan).
fn spdx_main_module_package(spdx: &serde_json::Value) -> &serde_json::Value {
    let candidates: Vec<&serde_json::Value> = spdx["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .filter(|p| {
            p.get("primaryPackagePurpose")
                .and_then(|v| v.as_str())
                == Some("APPLICATION")
        })
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "expected exactly one APPLICATION-purpose package; found {}",
        candidates.len()
    );
    candidates[0]
}

#[test]
fn scan_go_emits_main_module_direct_edges_with_empty_cache() {
    // US1 AS#1 + AS#4 + SC-001: 14 direct requires in the fixture's
    // go.mod must produce ≥14 DEPENDS_ON edges originating from the
    // main-module's SPDXID, with empty GOMODCACHE.
    let (spdx, _cdx) = scan_isolated_two_format(
        "argo-style-no-cache/argo-workflows",
        &[],
    );
    let main = spdx_main_module_package(&spdx);
    let main_id = main["SPDXID"].as_str().expect("SPDXID");
    let depends_on_count = spdx["relationships"]
        .as_array()
        .expect("relationships array")
        .iter()
        .filter(|r| {
            r["relationshipType"].as_str() == Some("DEPENDS_ON")
                && r["spdxElementId"].as_str() == Some(main_id)
        })
        .count();
    assert!(
        depends_on_count >= 14,
        "SC-001 violated: expected ≥14 DEPENDS_ON from main-module \
         (one per direct require in fixture's go.mod). Got: {depends_on_count}",
    );
}

#[test]
fn scan_go_main_module_carries_primary_package_purpose_application() {
    // US2 AS#2 + FR-001a (SPDX 2.3 placement) +
    // FR-005 (empty licenses) + FR-006 (sbom-tier: source).
    // documentDescribes-targeting (US3 AS#1) is verified separately
    // by `scan_go_documentdescribes_targets_main_module`.
    let (spdx, _cdx) = scan_isolated_two_format(
        "argo-style-no-cache/argo-workflows",
        &[],
    );
    let main = spdx_main_module_package(&spdx);
    // FR-001 + FR-001a: main-module name = the project's go.mod
    // module path.
    assert_eq!(
        main["name"].as_str(),
        Some("github.com/argoproj/argo-workflows/v3"),
    );
    // FR-005: empty licenses (no LICENSE detection in milestone 053).
    assert_eq!(main["licenseDeclared"].as_str(), Some("NOASSERTION"));
    assert_eq!(main["licenseConcluded"].as_str(), Some("NOASSERTION"));
    // FR-006: sbom-tier annotation = "source" via C40-style envelope.
    let tier_value: Option<String> = main["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let comment = a.get("comment")?.as_str()?;
            let env: serde_json::Value = serde_json::from_str(comment).ok()?;
            if env.get("schema")?.as_str() != Some("mikebom-annotation/v1") {
                return None;
            }
            if env.get("field")?.as_str()? == "mikebom:sbom-tier" {
                env.get("value")?.as_str().map(String::from)
            } else {
                None
            }
        })
        .next();
    assert_eq!(
        tier_value.as_deref(),
        Some("source"),
        "FR-006: main-module must carry mikebom:sbom-tier = \"source\"",
    );
}

#[test]
fn scan_go_main_module_emits_c40_annotation_in_spdx() {
    // US2 AS#3: SPDX 2.3 main-module package must carry the
    // `mikebom-annotation/v1` envelope with `field:
    // "mikebom:component-role"` and `value: "main-module"`.
    let (spdx, _cdx) = scan_isolated_two_format(
        "argo-style-no-cache/argo-workflows",
        &[],
    );
    let main = spdx_main_module_package(&spdx);
    let role_value: Option<String> = main["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let comment = a.get("comment")?.as_str()?;
            let env: serde_json::Value = serde_json::from_str(comment).ok()?;
            if env.get("schema")?.as_str() != Some("mikebom-annotation/v1") {
                return None;
            }
            if env.get("field")?.as_str()? == "mikebom:component-role" {
                env.get("value")?.as_str().map(String::from)
            } else {
                None
            }
        })
        .next();
    assert_eq!(
        role_value.as_deref(),
        Some("main-module"),
        "US2 AS#3: SPDX main-module must carry mikebom:component-role = \"main-module\"",
    );
}

#[test]
fn scan_go_main_module_in_cdx_metadata_component() {
    // US2 AS#1 + FR-001a (CDX placement): metadata.component is the
    // Go main-module; it does NOT also appear in components[].
    // FR-005 (empty licenses) + FR-006 (sbom-tier: source) verified
    // via the supplementary CDX properties on metadata.component.
    let (_spdx, cdx) = scan_isolated_two_format(
        "argo-style-no-cache/argo-workflows",
        &[],
    );
    let mc = &cdx["metadata"]["component"];
    assert_eq!(mc["type"].as_str(), Some("application"));
    let purl = mc["purl"].as_str().expect("metadata.component.purl");
    assert!(
        purl.starts_with("pkg:golang/"),
        "FR-001a: metadata.component.purl must be the Go main-module PURL; got {purl}",
    );
    // Main-module MUST NOT also appear in components[] — Principle V
    // standards-native placement supersedes flat sibling emission.
    let main_in_components = cdx["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| c["purl"].as_str() == Some(purl))
        .count();
    assert_eq!(
        main_in_components, 0,
        "FR-001a CDX: main-module must NOT be duplicated in components[]",
    );
    // metadata.component.bom-ref MUST equal the PURL so existing
    // dependencies[].ref entries resolve to it.
    assert_eq!(
        mc["bom-ref"].as_str(),
        Some(purl),
        "metadata.component.bom-ref must equal its PURL for edge resolution",
    );
    // Supplementary properties on metadata.component carry the C40
    // role tag + sbom-tier (FR-004 + FR-006).
    let props: Vec<(String, String)> = mc["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let name = p["name"].as_str()?.to_string();
            let value = p["value"].as_str()?.to_string();
            Some((name, value))
        })
        .collect();
    assert!(
        props.iter().any(|(n, v)| n == "mikebom:component-role"
            && v == "main-module"),
        "US2 AS#1: metadata.component.properties must contain \
         mikebom:component-role = main-module. got: {props:?}",
    );
    assert!(
        props.iter().any(|(n, v)| n == "mikebom:sbom-tier" && v == "source"),
        "FR-006: metadata.component.properties must contain \
         mikebom:sbom-tier = source. got: {props:?}",
    );
    // FR-005: metadata.component MUST NOT carry a non-empty licenses
    // array (LICENSE-file detection deferred to issue #103).
    let licenses_len = mc.get("licenses").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        licenses_len, 0,
        "FR-005: main-module licenses must be absent or empty",
    );
}

#[test]
fn scan_go_main_module_in_cdx_dependencies_emits_direct_edges() {
    // US1 AS#1 + AS#4 + FR-002 (CDX side): the dependencies[] block
    // must contain an entry whose `ref` equals
    // metadata.component.bom-ref AND whose dependsOn lists every
    // direct require from the fixture's go.mod (≥14).
    let (_spdx, cdx) = scan_isolated_two_format(
        "argo-style-no-cache/argo-workflows",
        &[],
    );
    let main_ref = cdx["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("bom-ref");
    let depends_count: usize = cdx["dependencies"]
        .as_array()
        .expect("dependencies array")
        .iter()
        .filter(|d| d["ref"].as_str() == Some(main_ref))
        .filter_map(|d| d["dependsOn"].as_array().map(|a| a.len()))
        .sum();
    assert!(
        depends_count >= 14,
        "FR-002 CDX: expected ≥14 dependsOn edges from main-module \
         metadata.component (one per direct require). got: {depends_count}",
    );
}

#[test]
fn scan_go_documentdescribes_targets_main_module() {
    // US3 AS#1 + SC-005: Go-only scan's SPDX `documentDescribes[]`
    // contains exactly the main-module's SPDXID (not a synthetic
    // `SPDXRef-DocumentRoot-*` placeholder), and the
    // `SPDXRef-DOCUMENT DESCRIBES <main-module-spdxid>` relationship
    // is present in `relationships[]`.
    let (spdx, _cdx) = scan_isolated_two_format(
        "argo-style-no-cache/argo-workflows",
        &[],
    );
    let main = spdx_main_module_package(&spdx);
    let main_id = main["SPDXID"].as_str().expect("SPDXID");
    let document_describes: Vec<&str> = spdx["documentDescribes"]
        .as_array()
        .expect("documentDescribes array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        document_describes,
        vec![main_id],
        "US3 AS#1 / SC-005: documentDescribes must be exactly [main-module-spdxid]; \
         got {document_describes:?}",
    );
    // Also verify the corresponding relationship.
    let describes_rel_present = spdx["relationships"]
        .as_array()
        .expect("relationships array")
        .iter()
        .any(|r| {
            r["spdxElementId"].as_str() == Some("SPDXRef-DOCUMENT")
                && r["relatedSpdxElement"].as_str() == Some(main_id)
                && r["relationshipType"].as_str() == Some("DESCRIBES")
        });
    assert!(
        describes_rel_present,
        "US3 AS#1: SPDXRef-DOCUMENT DESCRIBES <main-module> relationship must be present",
    );
}

#[test]
fn scan_go_zero_requires_emits_main_module_no_edges() {
    // US1 AS#3: a Go fixture with `module x\ngo 1.22\n` and no
    // requires emits the main-module component but no DEPENDS_ON
    // edges. Verifies the zero-require case is handled cleanly.
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::write(
        project.join("go.mod"),
        "module example.com/empty\ngo 1.22\n",
    )
    .expect("write go.mod");

    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tmp.path().join("out.spdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let empty_cache = tempfile::tempdir().expect("empty-cache tempdir");
    let output = Command::new(bin)
        .env("HOME", fake_home.path())
        .env("GOMODCACHE", empty_cache.path().join("empty"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&project)
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let spdx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read"))
            .expect("valid JSON");
    let main = spdx_main_module_package(&spdx);
    assert_eq!(main["name"].as_str(), Some("example.com/empty"));
    let main_id = main["SPDXID"].as_str().expect("SPDXID");
    let depends_on_count = spdx["relationships"]
        .as_array()
        .expect("relationships array")
        .iter()
        .filter(|r| {
            r["relationshipType"].as_str() == Some("DEPENDS_ON")
                && r["spdxElementId"].as_str() == Some(main_id)
        })
        .count();
    assert_eq!(
        depends_on_count, 0,
        "US1 AS#3: zero-require go.mod must produce zero DEPENDS_ON \
         edges from main-module. got: {depends_on_count}",
    );
}
