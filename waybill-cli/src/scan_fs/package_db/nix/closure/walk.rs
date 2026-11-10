//! Milestone 1423 (#1423), US2 / FR-005 — the reachability walk, used when no
//! store can answer for the evaluated output's runtime closure.
//!
//! From the evaluated derivation, follow only the fields that name what goes
//! *into* the artifact ([`super::classify::ARTIFACT_FIELDS`]), transitively,
//! resolving each listed store path to the derivation that produces it. Every
//! member the walk does not reach is marked not-in-artifact.
//!
//! This is the rule `specs/1423-nix-runtime-scope/measurements/reach.py`
//! scored against the runtime closure of five targets: 95.7% precision and
//! 94.6% recall at the finding level (SC-003). Two properties of that rule
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
//! targets has that shape.

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
}
