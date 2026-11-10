//! Milestone 1423 (#1423), US2 / FR-005 — the reachability walk, used when no
//! store can answer for the evaluated output's runtime closure.
//!
//! From the evaluated derivation, follow only the fields that name what goes
//! *into* the artifact ([`super::classify::ARTIFACT_FIELDS`]), transitively,
//! resolving each listed store path to the derivation that produces it. Every
//! member the walk does not reach is marked not-in-artifact.
//!
//! This is the rule `specs/1423-nix-runtime-scope/measurements/reach.py`
//! scored against the runtime closure of five targets: 331/346 (95.66%)
//! precision and 331/350 (94.57%) recall at the finding level (SC-003). Two
//! properties of that rule
//! are deliberate and must not drift:
//!
//! * **Root-relative, not whole-graph.** The milestone-1277 role is a flat
//!   scan crediting whatever *any* derivation names, which is why curl's
//!   closure presented python3, libxml2 and pcre2 as runtime: a build tool
//!   links them. Here only what the evaluated package reaches counts.
//! * **Build-machine tooling is never followed.** A member reached only
//!   through `nativeBuildInputs` and its siblings is a build tool, and so is
//!   everything it links.
//!
//! It is an approximation, and the document says so (C216 `method:
//! reachability-walk`, C203 `reachability-walk` on every member it decides).
//! Scored as implemented (`measurements/us2-walk-score/`), it agrees with
//! `reach.py` on all 1,301 findings. Its errors are that rule's: on curl it
//! keeps libxml2, c-ares and libxslt that the output does not reference; on
//! httpie it misses libxml2, pcre2 and coreutils that the output does
//! reference, along with darwin SDK derivations that carry no findings.
//!
//! A `justStaticExecutables` root (shellcheck's shape) reaches its static
//! build only through `inputs.drvs` and an `installPhase` path, never a
//! dependency field, so the walk reaches nothing from it. None of the scored
//! targets has that shape. Such a walk is not used: when the walk reaches no
//! member but the root and the root consumed a Haskell build
//! ([`consumes_haskell_build`]), [`super::runtime::evidence`] leaves the
//! closure to the role rule, and C216 says `walk-outcome: reached-only-root`.

use std::collections::{BTreeSet, VecDeque};

use super::classify::ARTIFACT_FIELDS;
use super::derivation::{store_basename, RawClosure};

/// The `.drv` keys reachable from `root` over [`ARTIFACT_FIELDS`], `root`
/// included.
///
/// A listed path that no closure output produces (a source tarball, a path
/// outside the closure) is not followed: there is no derivation to continue
/// from. `root` missing from the closure yields the empty set.
pub(crate) fn reachable(raw: &RawClosure, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    if !raw.derivations.contains_key(root) {
        return seen;
    }
    let index = raw.output_index();
    seen.insert(root.to_string());
    let mut queue = VecDeque::from([root.to_string()]);
    while let Some(key) = queue.pop_front() {
        let Some(drv) = raw.derivations.get(&key) else { continue };
        for field in ARTIFACT_FIELDS {
            for path in drv.paths_in(field) {
                let Some(next) = index.get(store_basename(path)) else { continue };
                if seen.insert((*next).to_string()) {
                    queue.push_back((*next).to_string());
                }
            }
        }
    }
    seen
}

/// Whether `root` consumed a Haskell-builder derivation (`inputs.drvs`), the
/// sign of a wrapper around a static Haskell build.
///
/// A walk that reached only the root drops linked code only when there is
/// linked code it could not follow. shellcheck's wrapper names its static
/// build in `inputs.drvs` and an `installPhase` path; gh's root reaches only
/// itself too (`measurements/reach.json`: 1 of 788 derivations), but its code
/// is its own, and that walk is exact on gh's findings (55 true, 0 false,
/// `measurements/us2-walk-score/`).
pub(crate) fn consumes_haskell_build(raw: &RawClosure, root: &str) -> bool {
    raw.derivations.get(root).is_some_and(|d| {
        d.input_drvs(&raw.derivations)
            .any(|k| raw.derivations.get(k).is_some_and(|i| i.is_haskell_builder()))
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// `app` links `ssl` (which propagates `z`) and has a cross-platform
    /// `depsHostHost` input; it builds with `tool`, which itself links `xml`
    /// and `py`. A structuredAttrs member is reached through its typed carrier.
    const CLOSURE: &str = r#"{
      "derivations": {
        "d-app.drv": {"env": {"pname":"app","version":"1",
                              "buildInputs":"/nix/store/h-ssl /nix/store/h-src-tarball",
                              "depsHostHost":"/nix/store/h-hh",
                              "nativeBuildInputs":"/nix/store/h-tool"},
                      "outputs": {"out": {"path":"h-app"}}},
        "d-ssl.drv": {"env": {"pname":"ssl","version":"3","propagatedBuildInputs":"/nix/store/h-z-dev"},
                      "outputs": {"out": {"path":"h-ssl"}}},
        "d-z.drv":   {"env": {"pname":"z","version":"1"},
                      "outputs": {"out": {"path":"h-z"}, "dev": {"path":"h-z-dev"}}},
        "d-hh.drv":  {"structuredAttrs": {"pname":"hh","version":"1","buildInputs":["/nix/store/h-sa"]},
                      "outputs": {"out": {"path":"h-hh"}}},
        "d-sa.drv":  {"env": {"pname":"sa","version":"1"}, "outputs": {"out": {"path":"h-sa"}}},
        "d-tool.drv": {"env": {"pname":"tool","version":"1","buildInputs":"/nix/store/h-xml /nix/store/h-py"},
                       "outputs": {"out": {"path":"h-tool"}}},
        "d-xml.drv": {"env": {"pname":"xml","version":"2"}, "outputs": {"out": {"path":"h-xml"}}},
        "d-py.drv":  {"env": {"pname":"py","version":"3"}, "outputs": {"out": {"path":"h-py"}}}
      }
    }"#;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_walk_follows_artifact_fields_transitively_from_the_root() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(
            reachable(&raw, "d-app.drv"),
            set(&["d-app.drv", "d-hh.drv", "d-sa.drv", "d-ssl.drv", "d-z.drv"]),
            "propagation is followed through a non-default output, the typed \
             carrier is read, and nothing behind `nativeBuildInputs` is reached \
             -- not the tool, and not what the tool links"
        );
    }

    #[test]
    fn the_walk_is_root_relative() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(
            reachable(&raw, "d-tool.drv"),
            set(&["d-py.drv", "d-tool.drv", "d-xml.drv"]),
            "control: the same fields from another root reach what that root links"
        );
    }

    #[test]
    fn an_unknown_root_reaches_nothing() {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        assert!(reachable(&raw, "d-absent.drv").is_empty());
    }

    /// shellcheck's wrapper: the static build is consumed through
    /// `inputs.drvs` only. The control is the same root consuming a
    /// non-Haskell derivation the same way.
    #[test]
    fn a_consumed_haskell_build_is_detected_through_inputs_drvs() {
        let closure = |setup: &str| {
            format!(
                r#"{{"derivations": {{
                  "d-wrap.drv": {{"env": {{"pname":"wrap","version":"1"}},
                                 "inputs": {{"drvs": {{"d-hs.drv": {{"outputs":["out"]}}}}, "srcs": []}},
                                 "outputs": {{"out": {{"path":"h-wrap"}}}}}},
                  "d-hs.drv": {{"env": {{"pname":"hs","version":"1"{setup}}},
                               "outputs": {{"out": {{"path":"h-hs"}}}}}}
                }}}}"#
            )
        };
        let raw = RawClosure::parse(&closure(r#","setupCompilerEnvironmentPhase":"x""#)).unwrap();
        assert_eq!(reachable(&raw, "d-wrap.drv"), set(&["d-wrap.drv"]), "the walk cannot follow it");
        assert!(consumes_haskell_build(&raw, "d-wrap.drv"));
        let raw = RawClosure::parse(&closure("")).unwrap();
        assert!(!consumes_haskell_build(&raw, "d-wrap.drv"), "control: not a Haskell build");
        assert!(!consumes_haskell_build(&raw, "d-absent.drv"));
    }
}
