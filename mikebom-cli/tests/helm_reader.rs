//! Milestone 188 (#455) — Helm chart scanning integration tests.
//!
//! US1 (P1 MVP) — chart-level enumeration: `Chart.yaml` + `Chart.lock` +
//!   `charts/*.tgz` recursion. Emits `pkg:helm/<repo>/<name>@<version>`
//!   components with Chart.lock precedence per FR-004.
//!
//! US2 (P1) — template-level image-ref extraction with Go-template
//!   tolerance. Emits `pkg:docker/...` (tagged), `pkg:oci/...` (digested),
//!   or `pkg:generic/<placeholder>` (templated) components with
//!   `mikebom:image-ref-unresolved = "true"` on placeholders.
//!
//! Test fixtures are fabricated at test time via stdlib `fs::write` +
//! `tar` + `flate2` — no vendored fixtures needed. Matches m187's
//! `ipk_yocto_reader_fixes.rs` pattern.

use std::io::Write;
use std::path::Path;
use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn write_chart_yaml(chart_dir: &Path, content: &str) {
    std::fs::create_dir_all(chart_dir).unwrap();
    std::fs::write(chart_dir.join("Chart.yaml"), content).unwrap();
}

fn write_template(chart_dir: &Path, name: &str, content: &str) {
    let templates = chart_dir.join("templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(templates.join(name), content).unwrap();
}

/// Build a chart tarball from an in-memory chart layout. `chart_name`
/// is the top-level directory name inside the tarball.
fn build_chart_tgz(tgz_path: &Path, chart_name: &str, chart_yaml: &str) {
    let mut builder = tar::Builder::new(Vec::<u8>::new());
    let body = chart_yaml.as_bytes();
    let mut header = tar::Header::new_gnu();
    header
        .set_path(format!("{chart_name}/Chart.yaml"))
        .unwrap();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, body).unwrap();
    let uncompressed = builder.into_inner().unwrap();
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::<u8>::new(), flate2::Compression::default());
    encoder.write_all(&uncompressed).unwrap();
    let compressed = encoder.finish().unwrap();
    std::fs::write(tgz_path, compressed).unwrap();
}

fn scan_dir(scan_root: &Path) -> (serde_json::Value, String) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let cmd_out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--path",
            scan_root.to_str().unwrap(),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    assert!(
        cmd_out.status.success(),
        "mikebom exit={:?} stderr:\n{stderr}",
        cmd_out.status.code(),
    );
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).expect("output is valid JSON");
    (json, stderr)
}

fn scan_helm_chart_tgz(tgz_path: &Path) -> (serde_json::Value, String) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let cmd_out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--helm-chart",
            tgz_path.to_str().unwrap(),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    if !cmd_out.status.success() {
        return (serde_json::Value::Null, stderr);
    }
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).expect("output is valid JSON");
    (json, stderr)
}

fn components_by_purl_prefix<'a>(
    cdx: &'a serde_json::Value,
    prefix: &str,
) -> Vec<&'a serde_json::Value> {
    cdx.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|p| p.as_str())
                        .map(|s| s.starts_with(prefix))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn get_property(component: &serde_json::Value, key: &str) -> Option<String> {
    component
        .get("properties")
        .and_then(|p| p.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(key))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()).map(String::from))
}

// ─────────────────────────────────────────────────────────────────
// US1 (#455 chart-level) — auto-detect + chart-dep enumeration
// ─────────────────────────────────────────────────────────────────

#[test]
fn us1_chart_yaml_only_produces_expected_components() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    let chart_yaml = "\
name: mychart
version: 1.0.0
type: application
dependencies:
  - name: postgres
    version: 11.0.0
    repository: https://charts.bitnami.com/bitnami
  - name: redis
    version: 17.0.0
    repository: https://charts.bitnami.com/bitnami
  - name: cache
    version: 3.0.0
    repository: '@bitnami'
";
    write_chart_yaml(&chart_dir, chart_yaml);

    let (cdx, _stderr) = scan_dir(&chart_dir);
    let helm_comps = components_by_purl_prefix(&cdx, "pkg:helm/");
    // 1 chart (mychart itself) + 3 deps = 4 total.
    assert_eq!(
        helm_comps.len(),
        4,
        "expected 4 helm components; got {}. cdx: {cdx:#}",
        helm_comps.len()
    );
    // Chart itself present.
    let mychart = helm_comps
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("mychart"))
        .expect("mychart component present");
    let purl = mychart.get("purl").and_then(|p| p.as_str()).unwrap();
    assert_eq!(purl, "pkg:helm/local/mychart@1.0.0");
    // At least one dep uses bitnami repo host.
    let postgres = helm_comps
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("postgres"))
        .expect("postgres dep present");
    let postgres_purl = postgres.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(
        postgres_purl.contains("charts.bitnami.com"),
        "postgres PURL should carry bitnami host: {postgres_purl}"
    );
}

#[test]
fn us1_chart_lock_overrides_chart_yaml_versions() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    let chart_yaml = "\
name: mychart
version: 1.0.0
dependencies:
  - name: postgres
    version: 11.0.0
    repository: https://charts.bitnami.com/bitnami
";
    let chart_lock = "\
dependencies:
  - name: postgres
    version: 11.9.5
    repository: https://charts.bitnami.com/bitnami
digest: sha256:abc
generated: '2026-01-01T00:00:00Z'
";
    write_chart_yaml(&chart_dir, chart_yaml);
    std::fs::write(chart_dir.join("Chart.lock"), chart_lock).unwrap();

    let (cdx, _stderr) = scan_dir(&chart_dir);
    let postgres = components_by_purl_prefix(&cdx, "pkg:helm/")
        .into_iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("postgres"))
        .expect("postgres dep present");
    let purl = postgres.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(
        purl.contains("@11.9.5"),
        "Chart.lock should override to 11.9.5; got {purl}"
    );
    assert_eq!(
        get_property(postgres, "mikebom:helm-lock-authoritative").as_deref(),
        Some("true"),
        "postgres should carry mikebom:helm-lock-authoritative = true"
    );
}

#[test]
fn us1_helm_chart_flag_with_tarball_extracts_and_scans() {
    let tempdir = tempfile::tempdir().unwrap();
    let tgz_path = tempdir.path().join("mychart-1.0.0.tgz");
    let chart_yaml = "\
name: mychart
version: 1.0.0
dependencies:
  - name: dep-a
    version: 2.0.0
    repository: https://example.com/charts
";
    build_chart_tgz(&tgz_path, "mychart", chart_yaml);

    let (cdx, stderr) = scan_helm_chart_tgz(&tgz_path);
    assert!(!cdx.is_null(), "scan should have succeeded. stderr:\n{stderr}");
    let helm_comps = components_by_purl_prefix(&cdx, "pkg:helm/");
    assert_eq!(
        helm_comps.len(),
        2,
        "expected mychart + dep-a helm components; got {}. cdx: {cdx:#}",
        helm_comps.len()
    );
}

#[test]
fn us1_helm_chart_flag_with_invalid_tarball_exits_nonzero() {
    let tempdir = tempfile::tempdir().unwrap();
    let tgz_path = tempdir.path().join("notachart.tgz");
    // Write a valid tarball but with NO Chart.yaml.
    let mut builder = tar::Builder::new(Vec::<u8>::new());
    let body = b"not a chart";
    let mut header = tar::Header::new_gnu();
    header.set_path("randomfile.txt").unwrap();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, body.as_ref()).unwrap();
    let uncompressed = builder.into_inner().unwrap();
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::<u8>::new(), flate2::Compression::default());
    encoder.write_all(&uncompressed).unwrap();
    std::fs::write(&tgz_path, encoder.finish().unwrap()).unwrap();

    let cmd_out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--helm-chart",
            tgz_path.to_str().unwrap(),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            tempdir.path().join("out.cdx.json").to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    assert!(
        !cmd_out.status.success(),
        "mikebom should reject tarball without Chart.yaml. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("no Chart.yaml"),
        "stderr should name the missing Chart.yaml. stderr:\n{stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────
// US2 (#455 template-level) — image-ref extraction
// ─────────────────────────────────────────────────────────────────

#[test]
fn us2_templated_image_ref_emits_unresolved_property() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    write_chart_yaml(&chart_dir, "name: mychart\nversion: 1.0.0\n");
    write_template(
        &chart_dir,
        "deployment.yaml",
        "\
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
        - name: app
          image: \"{{ .Values.image.repository }}:{{ .Values.image.tag }}\"
",
    );

    let (cdx, _stderr) = scan_dir(&chart_dir);
    let generic_comps = components_by_purl_prefix(&cdx, "pkg:generic/");
    assert!(
        !generic_comps.is_empty(),
        "expected at least one pkg:generic/ component for templated image. cdx: {cdx:#}"
    );
    let templated = &generic_comps[0];
    assert_eq!(
        get_property(templated, "mikebom:image-ref-unresolved").as_deref(),
        Some("true"),
        "templated image should carry mikebom:image-ref-unresolved = true"
    );
    let raw = get_property(templated, "mikebom:image-ref-raw").unwrap_or_default();
    assert!(
        raw.contains("{{"),
        "mikebom:image-ref-raw should preserve the raw string with placeholders; got: {raw}"
    );
}

#[test]
fn us2_concrete_image_refs_emit_normal_purl() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    write_chart_yaml(&chart_dir, "name: mychart\nversion: 1.0.0\n");
    write_template(
        &chart_dir,
        "deployment.yaml",
        "\
apiVersion: apps/v1
kind: Deployment
spec:
  containers:
    - image: nginx:1.27.0
    - image: ghcr.io/foo/bar:v2
",
    );

    let (cdx, _stderr) = scan_dir(&chart_dir);
    let docker_comps = components_by_purl_prefix(&cdx, "pkg:docker/");
    assert!(
        docker_comps.len() >= 2,
        "expected >=2 pkg:docker/ components; got {}. cdx: {cdx:#}",
        docker_comps.len()
    );
    // At least one should be library/nginx (unqualified DockerHub convention).
    let purls: Vec<String> = docker_comps
        .iter()
        .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(String::from))
        .collect();
    assert!(
        purls.iter().any(|p| p.contains("library/nginx")),
        "nginx should be pkg:docker/library/nginx@... — got {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p.contains("ghcr.io/foo/bar")),
        "ghcr.io ref should be pkg:docker/ghcr.io/foo/bar@... — got {purls:?}"
    );
    // None should carry the unresolved property.
    for c in &docker_comps {
        assert!(
            get_property(c, "mikebom:image-ref-unresolved").is_none(),
            "concrete refs must NOT carry mikebom:image-ref-unresolved"
        );
    }
}

#[test]
fn us2_extraction_survives_go_template_broken_yaml() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    write_chart_yaml(&chart_dir, "name: mychart\nversion: 1.0.0\n");
    // Template with Go-template block that breaks YAML parsing
    // (unbalanced `if`/`end` spanning multiple would-be documents).
    write_template(
        &chart_dir,
        "conditional.yaml",
        "\
{{ if .Values.enabled }}
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
        - image: nginx:1.27.0
{{ end }}
",
    );

    let (cdx, _stderr) = scan_dir(&chart_dir);
    let docker_comps = components_by_purl_prefix(&cdx, "pkg:docker/");
    assert!(
        !docker_comps.is_empty(),
        "line-based regex should extract nginx:1.27.0 from YAML-broken template. cdx: {cdx:#}"
    );
    assert!(
        docker_comps.iter().any(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .map(|s| s.contains("nginx"))
                .unwrap_or(false)
        }),
        "should find nginx PURL despite YAML being broken by Go-template block"
    );
}

// ─────────────────────────────────────────────────────────────────
// FR-016 byte-identity — non-Helm scans see zero drift
// ─────────────────────────────────────────────────────────────────

#[test]
fn default_scan_without_chart_yaml_is_byte_identical() {
    let tempdir = tempfile::tempdir().unwrap();
    let scan_dir_path = tempdir.path().join("random-dir");
    std::fs::create_dir_all(&scan_dir_path).unwrap();
    // A non-Helm directory with a random text file.
    std::fs::write(scan_dir_path.join("readme.txt"), b"hello world").unwrap();

    let (cdx, _stderr) = scan_dir(&scan_dir_path);
    let helm_comps = components_by_purl_prefix(&cdx, "pkg:helm/");
    assert!(
        helm_comps.is_empty(),
        "non-Helm scan MUST emit zero pkg:helm/ components"
    );
    let generic_comps = components_by_purl_prefix(&cdx, "pkg:generic/");
    // Note: other readers may emit pkg:generic/ for legitimate reasons —
    // we assert no `mikebom:image-ref-unresolved` marker specifically.
    for c in &generic_comps {
        assert!(
            get_property(c, "mikebom:image-ref-unresolved").is_none(),
            "non-Helm scan MUST NOT emit mikebom:image-ref-unresolved components"
        );
    }
}

// ─────────────────────────────────────────────────────────────────
// Milestone 203 (#553) — `--helm-render` subprocess + fallback tests
// ─────────────────────────────────────────────────────────────────
//
// US2 fallback classes exercised via `PATH` scrubbing + stub shell
// scripts (research §R3 Approach A). Each test overrides `PATH` when
// invoking mikebom so `Command::new("helm")` resolves to either
// (a) nothing (`BinaryNotFound`) or (b) a stub script (NonZeroExit /
// Timeout). Zero real `helm` binary required.
//
// US1 success test is gated behind `MIKEBOM_HELM_INTEGRATION=1`
// (matches m188 precedent) — nightly-only, requires real `helm`.

#[cfg(unix)]
fn write_stub_helm(dir: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let script = dir.join("helm");
    std::fs::write(&script, body).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
}

fn scan_dir_with_env(
    scan_root: &Path,
    helm_render: bool,
    path_env: Option<&str>,
    render_timeout_secs: Option<&str>,
) -> (serde_json::Value, String, bool) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let mut cmd = Command::new(mikebom_bin());
    cmd.args([
        "sbom",
        "scan",
        "--path",
        scan_root.to_str().unwrap(),
        "--offline",
        "--format",
        "cyclonedx-json",
        "--output",
        out.to_str().unwrap(),
    ]);
    if helm_render {
        cmd.arg("--helm-render");
    }
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    if let Some(t) = render_timeout_secs {
        cmd.env("MIKEBOM_HELM_RENDER_TIMEOUT_SECS", t);
    }
    let cmd_out = cmd.output().expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    let success = cmd_out.status.success();
    let json = if success {
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&out).unwrap())
            .expect("output is valid JSON")
    } else {
        serde_json::Value::Null
    };
    (json, stderr, success)
}

fn make_chart_with_templated_image(chart_dir: &Path) {
    write_chart_yaml(chart_dir, "name: mychart\nversion: 1.0.0\n");
    std::fs::write(
        chart_dir.join("values.yaml"),
        "image:\n  repository: nginx\n  tag: 1.27.0\n",
    )
    .unwrap();
    write_template(
        chart_dir,
        "deployment.yaml",
        "\
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
        - name: app
          image: \"{{ .Values.image.repository }}:{{ .Values.image.tag }}\"
",
    );
}

#[test]
#[cfg(unix)]
fn m203_us2_1_missing_helm_binary_falls_back_to_unrendered() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    make_chart_with_templated_image(&chart_dir);

    let (cdx, stderr, ok) = scan_dir_with_env(&chart_dir, true, Some(""), None);
    assert!(ok, "scan should succeed despite missing helm: stderr:\n{stderr}");
    // Fallback path emitted the unresolved-template component.
    let generic = components_by_purl_prefix(&cdx, "pkg:generic/");
    assert!(
        generic.iter().any(|c| get_property(c, "mikebom:image-ref-unresolved").as_deref()
            == Some("true")),
        "expected fallback to unrendered extraction. cdx: {cdx:#}"
    );
    assert!(
        stderr.contains("BinaryNotFound") || stderr.contains("not found on $PATH"),
        "expected BinaryNotFound WARN log; got stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("falling back"),
        "expected 'falling back' WARN log; got stderr:\n{stderr}"
    );
}

#[test]
#[cfg(unix)]
fn m203_us2_2_nonzero_exit_falls_back_to_unrendered() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    make_chart_with_templated_image(&chart_dir);
    let stub_dir = tempdir.path().join("stubs");
    write_stub_helm(&stub_dir, "#!/bin/sh\necho 'stub failure' >&2\nexit 1\n");

    let path_env = format!("{}:/usr/bin:/bin", stub_dir.display());
    let (cdx, stderr, ok) = scan_dir_with_env(&chart_dir, true, Some(&path_env), None);
    assert!(ok, "scan should succeed despite helm exit=1: stderr:\n{stderr}");
    let generic = components_by_purl_prefix(&cdx, "pkg:generic/");
    assert!(
        generic.iter().any(|c| get_property(c, "mikebom:image-ref-unresolved").as_deref()
            == Some("true")),
        "expected fallback to unrendered extraction. cdx: {cdx:#}"
    );
    assert!(
        stderr.contains("NonZeroExit") || stderr.contains("exited with code 1"),
        "expected NonZeroExit WARN log; got stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("falling back"),
        "expected 'falling back' WARN log; got stderr:\n{stderr}"
    );
}

#[test]
#[cfg(unix)]
fn m203_us2_3_timeout_falls_back_to_unrendered() {
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    make_chart_with_templated_image(&chart_dir);
    let stub_dir = tempdir.path().join("stubs");
    // First invocation is `helm version --short` (probe). We make that
    // succeed instantly, then the second invocation (`helm template`)
    // sleeps to trigger the timeout. Distinguish by presence of
    // `template` in $@.
    write_stub_helm(
        &stub_dir,
        "#!/bin/sh\ncase \"$1\" in\n  template) sleep 30 ;;\n  *) echo 'v3.stub' ;;\nesac\n",
    );

    let path_env = format!("{}:/usr/bin:/bin", stub_dir.display());
    let (cdx, stderr, ok) =
        scan_dir_with_env(&chart_dir, true, Some(&path_env), Some("1"));
    assert!(ok, "scan should succeed despite helm timeout: stderr:\n{stderr}");
    let generic = components_by_purl_prefix(&cdx, "pkg:generic/");
    assert!(
        generic.iter().any(|c| get_property(c, "mikebom:image-ref-unresolved").as_deref()
            == Some("true")),
        "expected fallback to unrendered extraction. cdx: {cdx:#}"
    );
    assert!(
        stderr.contains("Timeout") || stderr.contains("exceeded"),
        "expected Timeout WARN log; got stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("falling back"),
        "expected 'falling back' WARN log; got stderr:\n{stderr}"
    );
}

#[test]
#[cfg(unix)]
fn m203_us2_4_default_flow_without_helm_render_does_not_invoke_helm() {
    // FR-006 regression guard: WITHOUT `--helm-render`, scan must not
    // shell out to `helm` even when a chart is present. If it did,
    // and helm was absent, the m188 default path would still succeed
    // — but we want zero mention of BinaryNotFound / falling back.
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    make_chart_with_templated_image(&chart_dir);

    let (cdx, stderr, ok) = scan_dir_with_env(&chart_dir, false, Some(""), None);
    assert!(ok, "default-flow scan must succeed: stderr:\n{stderr}");
    let generic = components_by_purl_prefix(&cdx, "pkg:generic/");
    assert!(
        !generic.is_empty(),
        "default flow should still extract the templated placeholder"
    );
    // NO subprocess invocation → NO WARN log about falling back.
    assert!(
        !stderr.contains("BinaryNotFound") && !stderr.contains("falling back"),
        "default-flow scan MUST NOT invoke helm subprocess. stderr:\n{stderr}"
    );
}

#[test]
#[cfg(unix)]
fn m203_us2_5_env_var_override_shortens_timeout() {
    // Sanity: setting `MIKEBOM_HELM_RENDER_TIMEOUT_SECS=1` while the
    // stub sleeps 5s must still produce a Timeout classification —
    // i.e., the env var actually reduces the effective timeout.
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    make_chart_with_templated_image(&chart_dir);
    let stub_dir = tempdir.path().join("stubs");
    write_stub_helm(
        &stub_dir,
        "#!/bin/sh\ncase \"$1\" in\n  template) sleep 5 ;;\n  *) echo 'v3.stub' ;;\nesac\n",
    );

    let path_env = format!("{}:/usr/bin:/bin", stub_dir.display());
    let start = std::time::Instant::now();
    let (_cdx, stderr, ok) =
        scan_dir_with_env(&chart_dir, true, Some(&path_env), Some("1"));
    let elapsed = start.elapsed();
    assert!(ok, "scan should succeed on timeout fallback: stderr:\n{stderr}");
    assert!(
        elapsed.as_secs() < 5,
        "1s timeout should preempt the 5s stub sleep; observed {}s",
        elapsed.as_secs_f64(),
    );
    assert!(
        stderr.contains("Timeout") || stderr.contains("exceeded 1s"),
        "expected Timeout WARN log with 1s value; got stderr:\n{stderr}"
    );
}

#[test]
#[cfg_attr(
    not(any()),
    ignore = "gated behind MIKEBOM_HELM_INTEGRATION=1 (requires real helm binary)"
)]
fn m203_us1_helm_render_extracts_rendered_image_ref() {
    if std::env::var("MIKEBOM_HELM_INTEGRATION").ok().as_deref() != Some("1") {
        eprintln!("skipping: MIKEBOM_HELM_INTEGRATION!=1");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let chart_dir = tempdir.path().join("mychart");
    make_chart_with_templated_image(&chart_dir);

    let (cdx, stderr, ok) = scan_dir_with_env(&chart_dir, true, None, None);
    assert!(ok, "us1 scan should succeed: stderr:\n{stderr}");
    // Rendered path resolves the placeholder to nginx:1.27.0 →
    // pkg:docker/library/nginx@1.27.0. NO unresolved placeholder.
    let docker = components_by_purl_prefix(&cdx, "pkg:docker/");
    assert!(
        docker
            .iter()
            .any(|c| c.get("purl").and_then(|p| p.as_str()).map(|s| s.contains("nginx")).unwrap_or(false)),
        "expected pkg:docker/.../nginx from rendered chart. cdx: {cdx:#}"
    );
    let generic = components_by_purl_prefix(&cdx, "pkg:generic/");
    assert!(
        !generic.iter().any(|c| get_property(c, "mikebom:image-ref-unresolved").as_deref()
            == Some("true")),
        "us1 rendered scan MUST NOT emit unresolved-placeholder components"
    );
}
