// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// Output shaping (FR-014, FR-016, FR-017, contracts C-3/C-5/C-7/C-8).
//
// The renderer states quantities and the conditions they were obtained
// under. It never states that a tool is better. Interpretation is a human
// act performed afterwards, which is exactly where the errors that
// motivated this milestone occurred.

use std::error::Error;
use std::path::{Path, PathBuf};

use super::score::Scored;
use super::{Measurement, Verdict};

/// Words the harness must never emit about a tool (FR-017). Structurally
/// preventing the sentence is stronger than asking people not to write it.
pub const FORBIDDEN_COMPARATIVES: &[&str] = &[
    "faster", "slower", "better", "worse", "superior", "inferior",
    "outperform", "beats", "wins", "most accurate", "best",
];

/// Sole output location (FR-014). `target/` is gitignored, and nothing is
/// written outside it.
pub fn output_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("target/compare")
}

pub fn write_run(
    workspace_root: &Path,
    measurements: &[Measurement],
    verdict: &Verdict,
) -> Result<PathBuf, Box<dyn Error>> {
    let dir = output_dir(workspace_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("run-{stamp}.json"));
    let doc = serde_json::json!({
        "schema_version": 1,
        "verdict": match verdict {
            Verdict::Comparable => serde_json::json!({"comparable": true}),
            Verdict::Withheld { reasons } => serde_json::json!({
                "comparable": false,
                "reasons": reasons.iter().map(|r| r.describe()).collect::<Vec<_>>(),
            }),
        },
        "measurements": measurements.iter().map(measurement_json).collect::<Vec<_>>(),
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&doc)?)?;
    Ok(path)
}

fn measurement_json(m: &Measurement) -> serde_json::Value {
    serde_json::json!({
        "tool": m.tool_id,
        "target": m.target,
        "mode": if m.enriched { "enriched" } else { "offline" },
        "outcome": m.outcome.label(),
        // C-7 — absolutes are context, never a comparison.
        "timing_context_only": {
            "median_secs": m.median_secs,
            "all_secs": m.wall_secs,
            "spread": m.spread,
            "indicative": m.enriched,
        },
        "coverage": {
            "distinct_packages": m.distinct_packages,
            // C-3 / FR-006a — reported beside distinct, never instead of it.
            "raw_components": m.raw_components,
            "identityless_components": m.identityless,
        },
        "accuracy": match &m.scored {
            Scored::Yes(a) => serde_json::json!({
                "scored": true,
                "truth_method": a.method.as_str(),
                "truth_is_superset": a.truth_is_superset,
                "truth_size": a.truth_size,
                "found": a.found, "missed": a.missed, "extra": a.extra,
                "caveat": super::score::superset_caveat(a.method),
            }),
            // FR-009 / SC-008 — the reason is stated, never a zero.
            Scored::No(n) => serde_json::json!({"scored": false, "reason": n.reason()}),
        },
    })
}

/// C-7 — timing as a within-session ratio. Absolutes are labelled context
/// because on a non-reference host they have been observed to move by more
/// than 2x between runs of an identical command.
pub fn render_timing_ratios(measurements: &[Measurement]) -> String {
    let mut out = String::from("timing (within-session ratios, interleaved)\n");
    let usable: Vec<&Measurement> = measurements
        .iter()
        .filter(|m| m.outcome.succeeded() && m.median_secs.is_some())
        .collect();
    if usable.len() < 2 {
        out.push_str("  (fewer than two comparable measurements)\n");
        return out;
    }
    let base = usable
        .iter()
        .min_by(|a, b| {
            a.median_secs
                .unwrap_or(f64::MAX)
                .partial_cmp(&b.median_secs.unwrap_or(f64::MAX))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("non-empty");
    let base_secs = base.median_secs.unwrap_or(1.0);
    for m in &usable {
        let secs = m.median_secs.unwrap_or(0.0);
        out.push_str(&format!(
            "  {:<20} {:>6.2}x vs {}   (spread {:.2}){}\n",
            m.tool_id,
            if base_secs > 0.0 { secs / base_secs } else { f64::NAN },
            base.tool_id,
            m.spread,
            if m.enriched { "  [INDICATIVE — network-dependent]" } else { "" },
        ));
    }
    out.push_str("  absolute medians are recorded in the JSON as context only\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::config::TruthMethod;
    use crate::compare::score::{Accuracy, NotScored};
    use crate::compare::{Outcome, WithheldReason};

    fn m(id: &str, secs: f64, enriched: bool) -> Measurement {
        Measurement {
            tool_id: id.into(),
            target: "t".into(),
            enriched,
            outcome: Outcome::Ok,
            wall_secs: vec![secs, secs, secs],
            median_secs: Some(secs),
            spread: 1.0,
            distinct_packages: 10,
            raw_components: 12,
            identityless: 2,
            scored: Scored::No(NotScored::NoTruthDeclared { ecosystem: "cargo".into() }),
        }
    }

    /// FR-017 — the instrument must be structurally incapable of producing
    /// the sentence we do not want published.
    #[test]
    fn rendered_output_contains_no_comparative_adjective() {
        let ms = vec![m("a", 1.0, false), m("b", 4.0, false)];
        let text = render_timing_ratios(&ms).to_lowercase();
        for word in FORBIDDEN_COMPARATIVES {
            assert!(!text.contains(word), "renderer emitted {word:?}: {text}");
        }
    }

    #[test]
    fn enriched_timings_are_marked_indicative() {
        let ms = vec![m("a", 1.0, false), m("b", 2.0, true)];
        let text = render_timing_ratios(&ms);
        assert!(text.contains("INDICATIVE"), "{text}");
    }

    /// FR-010 / T037 — one tool in two modes is two rows, never merged.
    #[test]
    fn same_tool_in_two_modes_produces_two_rows() {
        let ms = vec![m("tool-a", 1.0, false), m("tool-a-enriched", 3.0, true)];
        let text = render_timing_ratios(&ms);
        assert!(text.contains("tool-a "), "{text}");
        assert!(text.contains("tool-a-enriched"), "{text}");
        assert_eq!(text.matches("vs ").count(), 2, "two rows: {text}");
    }

    #[test]
    fn failed_measurements_are_excluded_from_ratios() {
        let mut bad = m("b", 9.0, false);
        bad.outcome = Outcome::TimedOut;
        let ms = vec![m("a", 1.0, false), bad];
        let text = render_timing_ratios(&ms);
        assert!(text.contains("fewer than two"), "{text}");
    }

    #[test]
    fn json_reports_distinct_beside_raw_and_states_unscored_reason() {
        let v = measurement_json(&m("a", 1.0, false));
        assert_eq!(v["coverage"]["distinct_packages"], 10);
        assert_eq!(v["coverage"]["raw_components"], 12);
        assert_eq!(v["coverage"]["identityless_components"], 2);
        assert_eq!(v["accuracy"]["scored"], false);
        assert!(v["accuracy"]["reason"].as_str().expect("reason").contains("not scored"));
    }

    #[test]
    fn superset_scores_carry_their_caveat_in_json() {
        let mut one = m("a", 1.0, false);
        one.scored = Scored::Yes(Box::new(Accuracy {
            method: TruthMethod::GoSumUnion,
            truth_size: 479, found: 468, missed: 11, extra: 0,
            truth_is_superset: true,
        }));
        let v = measurement_json(&one);
        assert_eq!(v["accuracy"]["truth_is_superset"], true);
        assert!(v["accuracy"]["caveat"].as_str().expect("caveat").contains("SUPERSET"));
    }

    /// FR-014 — output lands only under target/, which is gitignored.
    #[test]
    fn output_dir_is_inside_target() {
        let d = output_dir(Path::new("/repo"));
        assert_eq!(d, Path::new("/repo/target/compare"));
    }

    /// FR-016 — a CI lane would publish results on a public repository.
    #[test]
    fn no_workflow_references_the_compare_subcommand() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("parent");
        let wf = root.join(".github/workflows");
        let Ok(entries) = std::fs::read_dir(&wf) else { return };
        for e in entries.flatten() {
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            assert!(
                !text.contains("xtask -- compare") && !text.contains("xtask compare"),
                "{:?} references the compare subcommand; artefacts on a public \
                 repository are world-readable, which defeats the purpose",
                e.path()
            );
        }
    }

    /// FR-015 / SC-006 — the committed example must stay tool-agnostic.
    ///
    /// Deliberately an ALLOWLIST of permitted ids rather than a denylist of
    /// competitor names. A denylist has to enumerate the very names the rule
    /// exists to keep out of this repository, and it silently misses any
    /// name whoever wrote it did not think of.
    #[test]
    fn committed_example_declares_only_permitted_tool_ids() {
        const PERMITTED: &[&str] = &["waybill", "tool-a", "tool-b", "tool-a-enriched", "tool-b-enriched"];
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(root.join("compare/tools.example.toml"))
            .expect("example config must exist");
        for line in text.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("id") else { continue };
            let Some(value) = rest.split('=').nth(1) else { continue };
            let id = value.trim().trim_matches('"');
            assert!(
                PERMITTED.contains(&id),
                "committed example declares tool id {id:?}, which is not a \
                 placeholder. The repository is public; the real comparison \
                 set belongs in the gitignored tools.local.toml"
            );
        }
    }

    #[test]
    fn verdict_reasons_render_without_comparatives() {
        let r = WithheldReason::TimingSpreadExceeded { tool: "a".into(), observed: 1.6, limit: 1.25 };
        let d = r.describe().to_lowercase();
        for word in FORBIDDEN_COMPARATIVES {
            assert!(!d.contains(word), "{d}");
        }
    }
}
