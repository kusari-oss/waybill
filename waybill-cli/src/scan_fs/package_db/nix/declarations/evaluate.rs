//! The second evaluation: asking the pinned package set what it declares.
//!
//! One expression for the whole closure, evaluated once. Measured: 376 names
//! across four package sets in 0.6–1.1 s. A per-member invocation would be
//! 376 process spawns and was never a serious option.

use std::collections::BTreeMap;
use std::time::Duration;

use super::{parse::Declaration, DeclarationSource};
use crate::scan_fs::package_db::nix::eval::invoke::{
    argv_is_safe, is_safe_attribute_name, is_valid_revision, run_bounded,
};
use crate::scan_fs::package_db::nix::eval::preflight::IFD_SETTING;
use crate::scan_fs::package_db::nix::eval::reason::DegradationReason;

/// What the package set said about one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) source: DeclarationSource,
    /// The candidate's own output path, for the check in `resolve`.
    pub(crate) out_path: String,
    pub(crate) declarations: Vec<Declaration>,
}

/// Build the expression. Separated from running it so the shape is testable
/// without a `nix` on the machine.
///
/// Two guards here are not defensive style — each one was a failed run:
///
/// * **`or null` on every lookup.** A missing attribute is not a `throw` and
///   escapes `tryEval`. Without it the whole evaluation dies on the first
///   name that is not in the set being probed, and on a Haskell project that
///   is immediate.
/// * **`deepSeq` inside `tryEval`.** `tryEval` returns a *lazy* value, so the
///   throw escapes at serialisation time — outside the guard — and the first
///   unfree or broken package kills the run.
pub(crate) fn build_expr(revision: &str, system: &str, names: &[String]) -> String {
    let list = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let sets = DeclarationSource::ORDER
        .iter()
        .map(|s| format!("{{ id = \"{}\"; set = {}; }}", s.wire(), s.selector()))
        .collect::<Vec<_>>()
        .join("\n    ");
    format!(
        r#"let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{revision}").legacyPackages."{system}";
  sets = [
    {sets}
  ];
  probeIn = s: n:
    let
      cand = s.set.${{n}} or null;
      raw = if cand == null then null else {{
        setId = s.id;
        out = cand.outPath;
        kv = cand.meta.knownVulnerabilities or [];
      }};
      r = builtins.tryEval (builtins.deepSeq raw raw);
    in if r.success then r.value else null;
  first = n: builtins.foldl' (a: s: if a != null then a else probeIn s n) null sets;
in builtins.listToAttrs (map (n: {{ name = n; value = first n; }}) [ {list} ])"#
    )
}

/// Ask the pinned package set about every name, in one invocation.
pub(crate) fn evaluate(
    revision: &str,
    system: &str,
    names: &[String],
    budget: Duration,
) -> Result<BTreeMap<String, Candidate>, DegradationReason> {
    // The revision is interpolated into a Nix string literal, so an
    // unvalidated value is an expression-injection vector. Reuses milestone
    // 1034's guard rather than restating the rule.
    if !is_valid_revision(revision) {
        return Err(DegradationReason::RevisionUnfetchable {
            revision: revision.to_string(),
            detail: "not a 40-character lowercase hex git object id".to_string(),
        });
    }
    // Names reach us from derivation `pname` fields and are interpolated into
    // a Nix expression, so anything not attribute-shaped is dropped rather
    // than quoted around. A name containing a quote would close the literal.
    let safe: Vec<String> = names
        .iter()
        .filter(|n| is_safe_attribute_name(n))
        .cloned()
        .collect();
    if safe.is_empty() {
        return Ok(BTreeMap::new());
    }

    let expr = build_expr(revision, system, &safe);
    // No `--impure`. An earlier draft reached the package set through
    // `getFlake "path:<project>"` and read `.inputs.nixpkgs`, which needs it
    // — and milestone 1034's argv guard rejected the call, correctly: that
    // flag restores access to the host environment and would have widened
    // this tier's safety posture without any visible change in output.
    //
    // Evaluating nixpkgs at the revision `flake.lock` pins reaches the same
    // package set through a *locked* flakeref, which is pure. Same answer,
    // same posture as milestone 1034, no new exposure.
    let argv = vec![
        "eval",
        "--json",
        "--option",
        IFD_SETTING,
        "false",
        "--expr",
        expr.as_str(),
    ];
    if !argv_is_safe(&argv) {
        return Err(DegradationReason::ToolUnusable(
            "declaration argv contains a flag that would weaken evaluation".to_string(),
        ));
    }

    let out = run_bounded(&argv, budget)?;
    if !out.status_success {
        return Err(DegradationReason::EvaluationFailed(
            out.stderr.lines().take(3).collect::<Vec<_>>().join(" "),
        ));
    }
    parse_output(&out.stdout)
}

/// Decode what the evaluation returned. A `null` means no set had the name.
pub(crate) fn parse_output(stdout: &str) -> Result<BTreeMap<String, Candidate>, DegradationReason> {
    let raw: BTreeMap<String, Option<serde_json::Value>> = serde_json::from_str(stdout)
        .map_err(|e| DegradationReason::EvaluationFailed(format!("undecodable output: {e}")))?;
    let mut out = BTreeMap::new();
    for (name, v) in raw {
        let Some(v) = v else { continue };
        let (Some(set_id), Some(path)) = (
            v.get("setId").and_then(|x| x.as_str()),
            v.get("out").and_then(|x| x.as_str()),
        ) else {
            continue;
        };
        let Some(source) = DeclarationSource::from_wire(set_id) else {
            continue;
        };
        let declarations = v
            .get("kv")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str())
                    .map(Declaration::parse)
                    .collect()
            })
            .unwrap_or_default();
        out.insert(
            name,
            Candidate {
                source,
                out_path: path.to_string(),
                declarations,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn the_expression_carries_both_guards() {
        // Each of these was a failed measurement run, not a precaution.
        let e = build_expr("0123456789abcdef0123456789abcdef01234567", "aarch64-darwin", &["foo".into()]);
        assert!(
            e.contains("or null"),
            "a missing attribute is not a throw and escapes tryEval"
        );
        assert!(
            e.contains("builtins.tryEval (builtins.deepSeq raw raw)"),
            "deepSeq must be INSIDE tryEval; tryEval returns a lazy value and \
             the throw would otherwise escape at serialisation time"
        );
    }

    #[test]
    fn the_expression_probes_every_set_in_order() {
        let e = build_expr("0123456789abcdef0123456789abcdef01234567", "x86_64-linux", &["foo".into()]);
        let positions: Vec<usize> = DeclarationSource::ORDER
            .iter()
            .map(|s| e.find(s.wire()).unwrap_or_else(|| panic!("{s:?} absent")))
            .collect();
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        assert_eq!(positions, sorted, "sets must appear in probe order");
    }

    #[test]
    fn the_call_never_carries_a_flag_that_would_weaken_evaluation() {
        // Regression guard. An earlier draft used `--impure` to reach the
        // project's flake by path, and milestone 1034's argv guard rejected
        // it — which is the guard working, not an obstacle to route around.
        let expr = build_expr("0123456789abcdef0123456789abcdef01234567", "x86_64-linux", &["a".into()]);
        let argv = vec!["eval", "--json", "--option", IFD_SETTING, "false", "--expr", expr.as_str()];
        assert!(argv_is_safe(&argv), "argv must pass the m1034 guard");
        assert!(!argv.contains(&"--impure"));
    }

    #[test]
    fn a_revision_that_is_not_a_git_object_id_is_refused() {
        // The revision is interpolated into a Nix string literal; a value
        // containing a quote would close it and evaluate what follows.
        let e = evaluate(r#"abc"; x = "#, "x86_64-linux", &["a".into()], Duration::from_secs(1));
        assert!(
            matches!(e, Err(DegradationReason::RevisionUnfetchable { .. })),
            "{e:?}"
        );
    }

    #[test]
    fn the_system_is_explicit_rather_than_read_from_the_host() {
        // Impurity stays scoped to getFlake. `builtins.currentSystem` would
        // widen it and would also make cross-platform scans silently wrong.
        let e = build_expr("0123456789abcdef0123456789abcdef01234567", "x86_64-linux", &["foo".into()]);
        assert!(e.contains(r#"legacyPackages."x86_64-linux""#));
        assert!(!e.contains("currentSystem"));
    }

    #[test]
    fn a_name_that_is_not_attribute_shaped_never_reaches_the_expression() {
        // CONTROL: the safe name survives, so this is testing the filter and
        // not an empty input.
        let names = vec!["aeson".to_string(), "evil\"; x = ".to_string()];
        let safe: Vec<String> = names
            .iter()
            .filter(|n| is_safe_attribute_name(n))
            .cloned()
            .collect();
        assert_eq!(safe, vec!["aeson".to_string()]);
    }

    #[test]
    fn output_decodes_declarations_and_skips_names_no_set_knew() {
        let stdout = r#"{
          "known":   {"setId":"haskell","out":"/nix/store/aaa-known-1.0",
                      "kv":["CVE-2020-1: boom","vendors an EOL thing"]},
          "clean":   {"setId":"top-level","out":"/nix/store/bbb-clean-2.0","kv":[]},
          "missing": null
        }"#;
        let m = parse_output(stdout).unwrap();
        assert_eq!(m.len(), 2, "the null must be skipped: {:?}", m.keys());

        let k = &m["known"];
        assert_eq!(k.source, DeclarationSource::Haskell);
        assert_eq!(k.declarations.len(), 2);
        assert_eq!(k.declarations[0].cves, vec!["CVE-2020-1"]);
        assert!(k.declarations[1].cves.is_empty(), "the prose entry names none");

        // An empty list is a real answer -- "nixpkgs says nothing" -- and is
        // not the same as the name being absent.
        assert!(m["clean"].declarations.is_empty());
    }

    #[test]
    fn undecodable_output_degrades_rather_than_panicking() {
        assert!(parse_output("not json").is_err());
    }
}
