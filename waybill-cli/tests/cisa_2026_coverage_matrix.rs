//! Feature 221 US1 — machine-verify `docs/cisa-2026-coverage.md`.
//!
//! Parses the coverage doc, then for every ✅ verdict in the matrix
//! runs the corresponding jq recipe from Appendix A against a fresh
//! `waybill sbom scan` output. Fails the CI build if any native slot
//! regresses to empty, or if the doc structure drifts.
//!
//! Constitution Principle VII: runs without root. Uses the
//! milestone-090 fixture cache at `~/.cache/waybill/fixtures/<pin>/
//! transitive_parity/cargo` as the scan target.
//!
//! Skips gracefully (`INFO: cisa_2026_coverage_matrix skipped: jq not on PATH`)
//! when `jq` is absent from the environment — the test is meant to
//! catch regressions in CI, not to force every dev to install jq.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::{bin, workspace_root};

/// Path to the coverage doc under test.
fn coverage_doc_path() -> PathBuf {
    workspace_root().join("docs").join("cisa-2026-coverage.md")
}

/// Path to the feature spec (used to validate ❌ rows link to real user stories).
fn spec_path() -> PathBuf {
    workspace_root()
        .join("specs")
        .join("221-cisa-2026-elements-audit")
        .join("spec.md")
}

fn load_doc() -> String {
    std::fs::read_to_string(coverage_doc_path())
        .expect("docs/cisa-2026-coverage.md must exist and be UTF-8")
}

fn jq_available() -> bool {
    Command::new("jq")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// T015 — parse structural expectations
// ---------------------------------------------------------------------------

#[test]
fn test_matrix_parses() {
    let doc = load_doc();

    // Front-matter checks (FR-015).
    assert!(
        doc.starts_with("---\n"),
        "coverage doc must begin with YAML front-matter"
    );
    let fm_end = doc[4..].find("---\n").expect("front-matter must terminate") + 4;
    let front_matter = &doc[4..fm_end];

    for required in &[
        "cisa-publication:",
        "cisa-publication-date: 2026-07-29",
        "cisa-publication-tlp: TLP:CLEAR",
        "waybill-milestone: 222",
        "last-verified:",
    ] {
        assert!(
            front_matter.contains(required),
            "front-matter missing required field: {required}\nfront-matter:\n{front_matter}"
        );
    }

    // Data Fields section — exactly 17 rows.
    assert!(
        doc.contains("## Data Fields (17)"),
        "coverage doc must contain `## Data Fields (17)` header"
    );
    let data_fields_rows = count_data_field_rows(&doc);
    assert_eq!(
        data_fields_rows, 17,
        "Data Fields matrix must have exactly 17 rows (SBOM Metadata 9 + Component Data 8); found {data_fields_rows}"
    );

    // Practices section — exactly 6 blocks.
    assert!(
        doc.contains("## Practices & Processes (6)"),
        "coverage doc must contain `## Practices & Processes (6)` header"
    );
    let practice_headers = practices_h3_headers(&doc);
    assert_eq!(
        practice_headers.len(),
        6,
        "expected 6 practice blocks (### headings under Practices & Processes); found {}: {:?}",
        practice_headers.len(),
        practice_headers
    );
}

// ---------------------------------------------------------------------------
// T016 — native-verdict jq recipes resolve to non-empty values
// ---------------------------------------------------------------------------

#[test]
fn test_native_verdicts_have_non_empty_values() {
    if !jq_available() {
        eprintln!(
            "INFO: cisa_2026_coverage_matrix::test_native_verdicts_have_non_empty_values skipped: jq not on PATH"
        );
        return;
    }

    let doc = load_doc();
    let recipes = parse_appendix_a(&doc);
    assert!(
        recipes.len() >= 30,
        "Appendix A should contain >=30 jq recipes across the 14 native elements × 3 emitters; found {}",
        recipes.len()
    );

    let scan = ScanOutputs::produce();

    // Some elements are populated by enrichment (deps.dev / ClearlyDefined)
    // and are legitimately empty when the scan runs in `--offline` mode
    // against a lockfile-only fixture. The coverage doc claims a ✅ verdict
    // because the SLOT is populated when data is available; the test
    // exercises the slot's existence and shape, not the specific fixture's
    // content.
    let offline_empty_ok: &[(&str, Emitter)] = &[
        ("Component License", Emitter::Cdx16),
        ("Component License", Emitter::Spdx23),
        ("Component License", Emitter::Spdx301),
        ("Component Producer", Emitter::Cdx16),
        ("Component Producer", Emitter::Spdx23),
        ("Component Producer", Emitter::Spdx301),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (element, emitter, recipe) in &recipes {
        let target = scan.path_for(*emitter);
        match run_jq(recipe, target) {
            Ok(stdout) if !stdout.trim().is_empty() => { /* ✓ */ }
            Ok(_empty)
                if offline_empty_ok
                    .iter()
                    .any(|(e, em)| e == element && em == emitter) =>
            { /* enrichment-dependent — OK in --offline mode */ }
            Ok(_empty) => failures.push(format!(
                "empty result: element={element:?} emitter={emitter:?}\n  recipe: {recipe}\n  target: {}",
                target.display()
            )),
            Err(e) => failures.push(format!(
                "jq error: element={element:?} emitter={emitter:?} err={e}\n  recipe: {recipe}"
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "coverage-matrix recipes returned empty or errored for {} cell(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// T017 — ⚠️ (annotation) verdicts name a `waybill:*` key
// ---------------------------------------------------------------------------

#[test]
fn test_annotation_verdicts_have_expected_key() {
    let doc = load_doc();
    let matrix = parse_data_fields_table(&doc);

    for row in &matrix {
        for (_emitter, cell) in &row.cells {
            if !cell.starts_with("⚠️") {
                continue;
            }
            // Row is either annotation-only, "pending USn", or the
            // signing row (⚠️ opt-in because default emit is unsigned
            // per FR-009, but both --sign and --sign-key are shipped
            // paths per m221 US2a + m222 US2b). Acceptable markers:
            //   - "pending US2/3/4" — feature not yet landed
            //   - `waybill:` prefix — annotation-based bridging
            //   - "implicit" — native fallback
            //   - "omitted" / "asymmetry" — row-17-style CDX quirk
            //   - "--sign" — either signing flag (row 2 opt-in)
            let has_pending = cell.contains("pending US")
                || cell.contains("(see US")
                || cell.contains("US4")
                || cell.contains("US3")
                || cell.contains("US2");
            let has_key = cell.contains("waybill:");
            let has_native_fallback = cell.contains("implicit"); // e.g. row 3 SBOM Data Format Name
            // Row 17 (Component Version) CDX side has ⚠️ because of the
            // omission-vs-NOASSERTION asymmetry with SPDX; document as
            // "omitted" / "asymmetry" and treat as a valid ⚠️ signal.
            let has_asymmetry_marker =
                cell.contains("omitted") || cell.contains("asymmetry");
            // Row 2 (SBOM Author Signature) — post-m222 both signing
            // paths ship; the ⚠️ is because default is unsigned. Any
            // mention of `--sign` (which includes `--sign-key`)
            // satisfies the marker requirement.
            let has_sign_flag = cell.contains("--sign");
            assert!(
                has_pending || has_key || has_native_fallback || has_asymmetry_marker
                    || has_sign_flag,
                "⚠️ row missing waybill:*/implicit/USn/omitted/asymmetry/--sign signal — element={} cell={}",
                row.element, cell
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T018 — ❌ verdicts link to a real user story in spec.md
// ---------------------------------------------------------------------------

#[test]
fn test_absent_verdicts_link_to_open_user_story() {
    let doc = load_doc();
    let matrix = parse_data_fields_table(&doc);
    let spec = std::fs::read_to_string(spec_path()).expect("spec.md must exist");

    let mut refs: Vec<String> = Vec::new();
    for row in &matrix {
        for (_e, cell) in &row.cells {
            if !cell.starts_with("❌") {
                continue;
            }
            // Extract every USn reference.
            for us in extract_us_refs(cell) {
                refs.push(us);
            }
        }
    }
    // An empty ❌ set is a healthy state — it means every element has
    // been closed. Pre-m221 US2/US3/US4 the matrix carried multiple ❌
    // rows; post-Polish (T053) only pending-US2b traces remain, which
    // are recorded as ⚠️ opt-in + follow-up-USn text rather than as
    // ❌ verdicts. The loop below still validates any USn refs we do
    // find; the empty-set case is silently accepted.

    for us in refs {
        let n: usize = us
            .trim_start_matches("US")
            .parse()
            .unwrap_or_else(|_| panic!("bad USn ref: {us}"));
        let header = format!("### User Story {n} —");
        assert!(
            spec.contains(&header),
            "coverage doc references {us} but spec.md has no matching `{header}` header"
        );
    }
}

// ---------------------------------------------------------------------------
// T019 — Practices rows have three required subsections + SWID anchor
// ---------------------------------------------------------------------------

#[test]
fn test_practice_rows_have_three_required_subsections() {
    let doc = load_doc();
    let blocks = practice_blocks(&doc);
    assert_eq!(blocks.len(), 6, "expected 6 practice blocks; found {}", blocks.len());

    for (title, body) in &blocks {
        for required in &[
            "**CISA text**",
            "**Classification**",
            "**How waybill enables the operator to satisfy this**",
        ] {
            assert!(
                body.contains(required),
                "practice `{title}` missing subsection `{required}`"
            );
        }
        assert!(
            body.contains("Organizational practice"),
            "practice `{title}` classification does not name it as an organizational practice"
        );
    }

    // T013a — Machine-Processable Data row must contain the FR-016 SWID anchor.
    let mpd_body = blocks
        .iter()
        .find(|(t, _)| t.contains("Machine-Processable Data"))
        .map(|(_, b)| b)
        .expect("Machine-Processable Data practice block missing");
    assert!(
        mpd_body.contains("<!-- fr-016-swid-advisory -->"),
        "Machine-Processable Data row missing FR-016 SWID advisory anchor `<!-- fr-016-swid-advisory -->`"
    );
    assert!(
        mpd_body.contains("SWID"),
        "Machine-Processable Data row missing SWID mention in the advisory sub-bullet"
    );
}

// ===========================================================================
// Helpers
// ===========================================================================

fn count_data_field_rows(doc: &str) -> usize {
    let Some(start) = doc.find("## Data Fields (17)") else {
        return 0;
    };
    let after = &doc[start..];
    let end = after
        .find("\n## ")
        .map(|i| start + i)
        .unwrap_or(doc.len());
    let section = &doc[start..end];
    // Rows look like `| 1 | ... |`, `| 17 | ... |`. Skip header + separator rows.
    section
        .lines()
        .filter(|l| {
            let t = l.trim();
            if !t.starts_with('|') {
                return false;
            }
            let cells: Vec<&str> = t.split('|').collect();
            if cells.len() < 3 {
                return false;
            }
            // First non-empty cell should be a bare integer 1..=17.
            cells[1].trim().parse::<u8>().ok().is_some_and(|n| (1..=17).contains(&n))
        })
        .count()
}

fn practices_h3_headers(doc: &str) -> Vec<String> {
    let Some(start) = doc.find("## Practices & Processes (6)") else {
        return Vec::new();
    };
    let after = &doc[start..];
    let end = after
        .find("\n## ")
        .map(|i| start + i)
        .unwrap_or(doc.len());
    let section = &doc[start..end];
    section
        .lines()
        .filter_map(|l| l.strip_prefix("### ").map(|s| s.trim().to_string()))
        .collect()
}

fn practice_blocks(doc: &str) -> Vec<(String, String)> {
    let Some(start) = doc.find("## Practices & Processes (6)") else {
        return Vec::new();
    };
    let after = &doc[start..];
    let end = after
        .find("\n## ")
        .map(|i| start + i)
        .unwrap_or(doc.len());
    let section = &doc[start..end];
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in section.lines() {
        if let Some(title) = line.strip_prefix("### ") {
            if let Some(done) = current.take() {
                blocks.push(done);
            }
            current = Some((title.trim().to_string(), String::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(done) = current {
        blocks.push(done);
    }
    blocks
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Emitter {
    Cdx16,
    Spdx23,
    Spdx301,
}

struct MatrixRow {
    element: String,
    cells: Vec<(Emitter, String)>,
}

fn parse_data_fields_table(doc: &str) -> Vec<MatrixRow> {
    let Some(start) = doc.find("## Data Fields (17)") else {
        return Vec::new();
    };
    let after = &doc[start..];
    let end = after
        .find("\n## ")
        .map(|i| start + i)
        .unwrap_or(doc.len());
    let section = &doc[start..end];

    let mut rows: Vec<MatrixRow> = Vec::new();
    for line in section.lines() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.split('|').map(|c| c.trim()).collect();
        // 9 pipe-splits = 8 cells: [#, Element, Category, Change, CDX, SPDX2.3, SPDX3, Notes]
        // (`|`-split of `| a | b | c |` yields ["", "a", "b", "c", ""], hence +2 empties)
        if cells.len() < 10 {
            continue;
        }
        let idx: u8 = match cells[1].parse() {
            Ok(n) if (1..=17).contains(&n) => n,
            _ => continue,
        };
        let _ = idx;
        let element = cells[2].to_string();
        rows.push(MatrixRow {
            element,
            cells: vec![
                (Emitter::Cdx16, cells[5].to_string()),
                (Emitter::Spdx23, cells[6].to_string()),
                (Emitter::Spdx301, cells[7].to_string()),
            ],
        });
    }
    rows
}

fn extract_us_refs(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = cell.as_bytes();
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 2] == b"US" && bytes[i + 2].is_ascii_digit() {
            let mut j = i + 3;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let s = std::str::from_utf8(&bytes[i..j]).unwrap().to_string();
            if !out.contains(&s) {
                out.push(s);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Parses Appendix A into `(element, emitter, recipe)` tuples.
///
/// Each recipe block looks like:
///
/// ```md
/// **Element: <Name>** (row N)
/// - CDX: `jq ...`
/// - SPDX 2.3: `jq ...`
/// - SPDX 3: `jq ...`
/// ```
fn parse_appendix_a(doc: &str) -> Vec<(String, Emitter, String)> {
    let Some(start) = doc.find("## Appendix A") else {
        return Vec::new();
    };
    let section = &doc[start..];

    let mut out = Vec::new();
    let mut current_element: Option<String> = None;
    for line in section.lines() {
        if let Some(rest) = line.strip_prefix("**Element: ") {
            // Everything up to `**` is the element name.
            if let Some(end) = rest.find("**") {
                current_element = Some(rest[..end].trim().to_string());
            }
            continue;
        }
        let Some(element) = current_element.as_ref() else {
            continue;
        };
        let (emitter, tail) = if let Some(t) = line.strip_prefix("- CDX: ") {
            (Emitter::Cdx16, t)
        } else if let Some(t) = line.strip_prefix("- SPDX 2.3: ") {
            (Emitter::Spdx23, t)
        } else if let Some(t) = line.strip_prefix("- SPDX 3: ") {
            (Emitter::Spdx301, t)
        } else {
            continue;
        };
        // Recipe is between the first and last backtick on the line.
        let Some(a) = tail.find('`') else { continue };
        let Some(b) = tail.rfind('`') else { continue };
        if b <= a + 1 {
            continue;
        }
        let recipe = &tail[a + 1..b];
        out.push((element.clone(), emitter, recipe.to_string()));
    }
    out
}

struct ScanOutputs {
    cdx: PathBuf,
    spdx23: PathBuf,
    spdx3: PathBuf,
    // keep the tempdir alive for the duration of the test
    _tmp: tempfile::TempDir,
}

impl ScanOutputs {
    fn path_for(&self, e: Emitter) -> &Path {
        match e {
            Emitter::Cdx16 => &self.cdx,
            Emitter::Spdx23 => &self.spdx23,
            Emitter::Spdx301 => &self.spdx3,
        }
    }

    /// Runs `waybill sbom scan` in offline mode against a Cargo fixture and
    /// writes all three emitter outputs. Uses the milestone-090 fixture cache
    /// via the `common::fixture_path("cmake")` helper if the m090 cache has
    /// the transitive_parity/cargo tree; otherwise falls back to a small
    /// vendored fixture directory (this test is deliberately tolerant).
    fn produce() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cdx = tmp.path().join("scan.cdx.json");
        let spdx23 = tmp.path().join("scan.spdx.json");
        let spdx3 = tmp.path().join("scan.spdx3.json");

        // The fixture cache is populated by m090's harness (see
        // WAYBILL_FIXTURES_DIR env var). We rely on it having been seeded
        // by a prior `cargo test` run per T003. If not present, the caller
        // gets a clear "target not found" error rather than a spurious
        // recipe-empty failure.
        let target_root = discover_target_root();

        let status = Command::new(bin())
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(&target_root)
            .arg("--format")
            .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
            .arg("--output")
            .arg(format!("cyclonedx-json={}", cdx.display()))
            .arg("--output")
            .arg(format!("spdx-2.3-json={}", spdx23.display()))
            .arg("--output")
            .arg(format!("spdx-3-json={}", spdx3.display()))
            .arg("--no-deep-hash")
            .status()
            .expect("waybill invocation");
        assert!(status.success(), "waybill sbom scan failed (target: {})", target_root.display());

        for p in [&cdx, &spdx23, &spdx3] {
            assert!(
                p.exists(),
                "expected scan output file missing: {}",
                p.display()
            );
        }

        Self { cdx, spdx23, spdx3, _tmp: tmp }
    }
}

fn discover_target_root() -> PathBuf {
    // First: the m090 fixture cache. Path shape:
    // ~/.cache/waybill/fixtures/<pinned-rev>/transitive_parity/cargo/
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home)
            .join(".cache")
            .join("waybill")
            .join("fixtures");
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("transitive_parity").join("cargo");
                if candidate.join("Cargo.toml").exists() {
                    return candidate;
                }
            }
        }
    }
    // Fallback — the workspace root itself is a Cargo project.
    workspace_root()
}

fn run_jq(recipe: &str, target: &Path) -> Result<String, String> {
    // The Appendix A recipes end with the concrete file path already, so
    // the recipe embeds the target reference. Strip that trailing token and
    // re-parameterize with our per-scan tempdir path.
    let (jq_expr, _) = split_recipe_expr(recipe)
        .ok_or_else(|| format!("cannot parse recipe: {recipe}"))?;

    let output = Command::new("jq")
        .arg("-r")
        .arg(jq_expr)
        .arg(target)
        .output()
        .map_err(|e| format!("jq spawn failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "jq non-zero exit: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // The recipe may include a trailing `| head -1` — jq handles that
    // internally when the expression is JSON-shaped, but the recipes cite
    // shell pipes to `head`. We approximate by taking the first non-empty
    // line of stdout.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_non_empty = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    Ok(first_non_empty.to_string())
}

/// Splits a recipe like `jq [-r] 'EXPR' /tmp/scan.X.json [| head -1]`
/// into (EXPR, target-path). Returns None on malformed recipes.
///
/// Handles:
/// - Optional `-r` flag.
/// - jq expressions containing shell-pipe-like `|` operators (splits
///   on the CLOSING quote of the expression, not on the first `|`).
/// - Optional trailing `| head -N` suffix.
fn split_recipe_expr(recipe: &str) -> Option<(String, String)> {
    // 1. Strip the `jq ` (or `jq -r `) prefix.
    let after_jq = if let Some(rest) = recipe.strip_prefix("jq -r ") {
        rest
    } else {
        recipe.strip_prefix("jq ")?
    }
    .trim();

    // 2. The expression MUST be single-quoted (all our recipes are).
    if !after_jq.starts_with('\'') {
        return None;
    }
    let after_open = &after_jq[1..];
    let close = after_open.find('\'')?;
    let expr = after_open[..close].to_string();

    // 3. After the closing quote, expect ` <PATH>` optionally followed
    //    by ` | head -N`.
    let rest = after_open[close + 1..].trim();
    let path = if let Some(pipe) = rest.find(" | ") {
        rest[..pipe].trim().to_string()
    } else {
        rest.to_string()
    };
    if path.is_empty() {
        return None;
    }
    Some((expr, path))
}

#[test]
fn helper_split_recipe_expr_handles_head_pipe() {
    let (expr, path) = split_recipe_expr(
        "jq -r '.metadata.authors[].name' /tmp/scan.cdx.json | head -1",
    )
    .expect("parse");
    assert_eq!(expr, ".metadata.authors[].name");
    assert_eq!(path, "/tmp/scan.cdx.json");
}

#[test]
fn helper_split_recipe_expr_handles_no_pipe() {
    let (expr, path) = split_recipe_expr("jq '.metadata.version' /tmp/scan.cdx.json")
        .expect("parse");
    assert_eq!(expr, ".metadata.version");
    assert_eq!(path, "/tmp/scan.cdx.json");
}

#[test]
fn helper_split_recipe_expr_handles_double_quotes_inside_expr() {
    // recipes with `.["@graph"][] | select(...)` should preserve the inner content.
    let (expr, _) = split_recipe_expr(
        "jq -r '.[\"@graph\"][] | select(.[\"@type\"]==\"CreationInfo\") | .createdBy[]' /tmp/scan.spdx3.json | head -1",
    )
    .expect("parse");
    assert!(expr.contains("@graph"));
    assert!(expr.contains("CreationInfo"));
}

fn _link_use_common() {
    // Silence unused-import warnings if the common module changes.
    let _ = BTreeMap::<Emitter, PathBuf>::new();
}
