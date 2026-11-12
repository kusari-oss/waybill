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
//! ([`consumed_haskell_builds`]), [`super::runtime::evidence`] leaves the
//! closure to the role rule, and C216 says `walk-outcome: reached-only-root`.
//! The consumed Haskell builds and the libraries linked into them are the
//! exception: their code ships inside the wrapper's binary, so they are kept
//! runtime as statically linked (T042). The runtime-closure path seeds static
//! linking from them too, because x86_64-linux's runtime closure of the
//! wrapper does not hold the static build (q10).

use std::collections::{BTreeSet, VecDeque};

use super::classify::{ARTIFACT_FIELDS, TOOLING_FIELDS};
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

/// #1467 — the C libraries the compilers of `members` link: for each member,
/// its `stdenv`'s C compiler wrapper (an entry of the stdenv's
/// `defaultNativeBuildInputs`), and that wrapper's `libc_lib`, as `.drv`
/// keys.
///
/// No dependency field names the C library. A member is linked against it
/// because its stdenv's compiler wrapper passes that libc's dynamic loader
/// and library directory to every link, and the derivation graph records
/// exactly that chain: on x86_64-linux at nixpkgs `a799d3e3`, a member's
/// `stdenv` → `stdenv-linux` → `gcc-wrapper-15.2.0` → `libc_lib` =
/// `glibc-2.42-61`; on aarch64-darwin the same chain ends at `libSystem-B`.
/// Measured on curl, openssh, httpie, gh and ripgrep, x86_64-linux: the
/// runtime closure holds glibc for all five, and the walk marked it in all
/// five (`specs/1423-nix-runtime-scope/measurements/c-runtime/`).
///
/// Only `libc_lib`: the wrapper's `cc` is the compiler, `libc_dev` its
/// headers, and what the libc itself references (glibc's `xgcc` libgcc and
/// `libidn2`, named only in `makeFlags` and `postPatch`) is not in a
/// dependency field either. Those are not assessed, and C216
/// `c-runtime-libraries` says so. A member built with a stdenv that has no C
/// compiler (`stdenvNoCC`) names none. A libc that another libc found here
/// was built from is the bootstrap libc that one replaced, and is dropped.
///
/// It errs toward runtime: a member with no dynamically linked output still
/// names its compiler's libc.
pub(crate) fn compiler_libcs(raw: &RawClosure, members: &BTreeSet<String>) -> BTreeSet<String> {
    let index = raw.output_index();
    let resolve = |p: &str| index.get(store_basename(p)).copied();
    let named = |k: &str, field: &str| -> Vec<&str> {
        raw.derivations
            .get(k)
            .map(|d| d.paths_in(field).filter_map(resolve).collect())
            .unwrap_or_default()
    };
    let stdenvs: BTreeSet<&str> = members.iter().flat_map(|m| named(m, "stdenv")).collect();
    let libcs: BTreeSet<&str> = stdenvs
        .iter()
        .flat_map(|s| named(s, "defaultNativeBuildInputs"))
        .flat_map(|w| named(w, "libc_lib"))
        .collect();
    // A libc another of them was built from is the bootstrap one it replaced.
    // nixpkgs reuses a few early-stage builds in the final package set
    // (curl's `libunistring` is built by `bootstrap-stage2-stdenv-linux`,
    // whose compiler names `bootstrap-stage0-glibc-minimal-bootstrap`), and
    // the final `glibc-2.42-61` is built from that same bootstrap libc. No
    // runtime closure of the five measured targets holds it.
    let replaced: BTreeSet<&str> = libcs
        .iter()
        .filter_map(|l| raw.derivations.get(*l))
        .flat_map(|d| d.input_drvs(&raw.derivations))
        .collect();
    libcs.into_iter().filter(|l| !replaced.contains(l)).map(str::to_string).collect()
}

/// The Haskell-builder derivations `root` consumed through `inputs.drvs`: when
/// non-empty, the sign of a wrapper around a static Haskell build.
///
/// A walk that reached only the root drops linked code only when there is
/// linked code it could not follow. shellcheck's wrapper names its static
/// build in `inputs.drvs` and an `installPhase` path; gh's root reaches only
/// itself too (`measurements/reach.json`: 1 of 788 derivations), but its code
/// is its own, and that walk is exact on gh's findings (55 true, 0 false,
/// `measurements/us2-walk-score/`).
///
/// These are the static builds whose code ships inside the wrapper's binary,
/// so they and the libraries linked into them are kept runtime: when the walk
/// is set aside (`reached-only-root`, T042), and under the runtime closure,
/// which on x86_64-linux does not hold the static build at all
/// (`specs/1423-nix-runtime-scope/measurements/nix-behaviour/q10-*`).
///
/// **A build the root names only as build-machine tooling
/// ([`TOOLING_FIELDS`]) is not one of them.** nixpkgs' shellcheck wrapper
/// consumes `pandoc-cli`, itself a Haskell generic-builder derivation, through
/// `inputs.drvs` and `nativeBuildInputs`, to render its man page (q10). It
/// runs on the build machine; counted here, it and every library linked into
/// it would be kept runtime.
pub(crate) fn consumed_haskell_builds(raw: &RawClosure, root: &str) -> BTreeSet<String> {
    let Some(d) = raw.derivations.get(root) else {
        return BTreeSet::new();
    };
    let index = raw.output_index();
    let named_in = |fields: &[&str]| -> BTreeSet<&str> {
        fields
            .iter()
            .flat_map(|f| d.paths_in(f))
            .filter_map(|p| index.get(store_basename(p)).copied())
            .collect()
    };
    let artifact = named_in(ARTIFACT_FIELDS);
    let tooling = named_in(TOOLING_FIELDS);
    d.input_drvs(&raw.derivations)
        .filter(|k| raw.derivations.get(*k).is_some_and(|i| i.is_haskell_builder()))
        .filter(|k| artifact.contains(k) || !tooling.contains(k))
        .map(str::to_string)
        .collect()
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
        assert_eq!(consumed_haskell_builds(&raw, "d-wrap.drv"), set(&["d-hs.drv"]));
        let raw = RawClosure::parse(&closure("")).unwrap();
        assert!(
            consumed_haskell_builds(&raw, "d-wrap.drv").is_empty(),
            "control: not a Haskell build"
        );
        assert!(consumed_haskell_builds(&raw, "d-absent.drv").is_empty());
    }

    /// #1467, the x86_64-linux shape measured on nixpkgs `a799d3e3`: glibc
    /// reaches a member only through its `stdenv`, whose
    /// `defaultNativeBuildInputs` carry the C compiler wrapper, whose
    /// `libc_lib` is glibc's library output. No dependency field names it.
    /// Only `libc_lib` is followed: the compiler (`cc`), the headers
    /// (`libc_dev`) and what the libc itself lists stay out, and so does the
    /// libc of a stdenv only an unreached member was built with.
    #[test]
    fn the_libc_a_members_compiler_links_is_found_through_its_stdenv() {
        let raw = RawClosure::parse(
            r#"{"derivations": {
              "d-app.drv":  {"env": {"pname":"app","version":"1","stdenv":"/nix/store/h-std"},
                             "outputs": {"out": {"path":"h-app"}}},
              "d-std.drv":  {"env": {"name":"stdenv-linux",
                                     "defaultNativeBuildInputs":"/nix/store/h-patchelf /nix/store/h-cc"},
                             "outputs": {"out": {"path":"h-std"}}},
              "d-cc.drv":   {"env": {"pname":"cc-wrapper","version":"1","wrapperName":"CC_WRAPPER",
                                     "cc":"/nix/store/h-gcc","libc_lib":"/nix/store/h-libc",
                                     "libc_dev":"/nix/store/h-libc-dev"},
                             "outputs": {"out": {"path":"h-cc"}}},
              "d-libc.drv": {"env": {"pname":"libc","version":"2","buildInputs":"/nix/store/h-headers"},
                             "inputs": {"drvs": {"d-stage0libc.drv": {"outputs":["out"]}}, "srcs": []},
                             "outputs": {"out": {"path":"h-libc"}, "dev": {"path":"h-libc-dev"}}},
              "d-unistring.drv": {"env": {"pname":"unistring","version":"1","stdenv":"/nix/store/h-stage2std"},
                             "outputs": {"out": {"path":"h-unistring"}}},
              "d-stage2std.drv": {"env": {"name":"bootstrap-stage2-stdenv-linux",
                                          "defaultNativeBuildInputs":"/nix/store/h-stage2cc"},
                             "outputs": {"out": {"path":"h-stage2std"}}},
              "d-stage2cc.drv": {"env": {"pname":"stage2cc","version":"1","libc_lib":"/nix/store/h-stage0libc"},
                             "outputs": {"out": {"path":"h-stage2cc"}}},
              "d-stage0libc.drv": {"env": {"pname":"stage0libc","version":"1"},
                             "outputs": {"out": {"path":"h-stage0libc"}}},
              "d-headers.drv": {"env": {"pname":"headers","version":"6"}, "outputs": {"out": {"path":"h-headers"}}},
              "d-gcc.drv":  {"env": {"pname":"gcc","version":"15"}, "outputs": {"out": {"path":"h-gcc"}}},
              "d-patchelf.drv": {"env": {"pname":"patchelf","version":"1"}, "outputs": {"out": {"path":"h-patchelf"}}},
              "d-tool.drv": {"env": {"pname":"tool","version":"1","stdenv":"/nix/store/h-bootstd"},
                             "outputs": {"out": {"path":"h-tool"}}},
              "d-bootstd.drv": {"env": {"name":"bootstrap-stdenv-linux","defaultNativeBuildInputs":"/nix/store/h-bootcc"},
                             "outputs": {"out": {"path":"h-bootstd"}}},
              "d-bootcc.drv": {"env": {"pname":"bootcc","version":"1","libc_lib":"/nix/store/h-bootlibc"},
                             "outputs": {"out": {"path":"h-bootcc"}}},
              "d-bootlibc.drv": {"env": {"pname":"bootlibc","version":"1"}, "outputs": {"out": {"path":"h-bootlibc"}}},
              "d-nocc.drv": {"env": {"pname":"nocc","version":"1","stdenv":"/nix/store/h-stdnocc"},
                             "outputs": {"out": {"path":"h-nocc"}}},
              "d-stdnocc.drv": {"env": {"name":"stdenv-linux","defaultNativeBuildInputs":"/nix/store/h-patchelf"},
                             "outputs": {"out": {"path":"h-stdnocc"}}}
            }}"#,
        )
        .unwrap();
        assert_eq!(compiler_libcs(&raw, &set(&["d-app.drv"])), set(&["d-libc.drv"]));
        assert_eq!(
            compiler_libcs(&raw, &set(&["d-tool.drv"])),
            set(&["d-bootlibc.drv"]),
            "control: another member's compiler names another libc"
        );
        assert!(
            compiler_libcs(&raw, &set(&["d-nocc.drv"])).is_empty(),
            "control: a stdenv without a C compiler names no libc"
        );
        assert!(compiler_libcs(&raw, &set(&["d-absent.drv"])).is_empty());
        // curl's `libunistring`: an early bootstrap stage's build, whose
        // compiler links the bootstrap libc the final libc was built from.
        assert_eq!(
            compiler_libcs(&raw, &set(&["d-app.drv", "d-unistring.drv"])),
            set(&["d-libc.drv"]),
            "the bootstrap libc is replaced by the libc built from it"
        );
        assert_eq!(
            compiler_libcs(&raw, &set(&["d-unistring.drv"])),
            set(&["d-stage0libc.drv"]),
            "control: alone, nothing replaces it"
        );
    }

    /// shellcheck's wrapper also consumes `pandoc-cli`, a Haskell build, as a
    /// build tool (q10). Named in a tooling field only, it is not counted;
    /// named in an artifact field as well, it is.
    #[test]
    fn a_haskell_build_the_root_names_only_as_tooling_is_not_consumed() {
        let closure = |fields: &str| {
            format!(
                r#"{{"derivations": {{
                  "d-wrap.drv": {{"env": {{"pname":"wrap","version":"1"{fields}}},
                                 "inputs": {{"drvs": {{"d-hs.drv": {{"outputs":["out"]}},
                                                      "d-doc.drv": {{"outputs":["out"]}}}}, "srcs": []}},
                                 "outputs": {{"out": {{"path":"h-wrap"}}}}}},
                  "d-hs.drv": {{"env": {{"pname":"hs","version":"1","setupCompilerEnvironmentPhase":"x"}},
                               "outputs": {{"out": {{"path":"h-hs"}}}}}},
                  "d-doc.drv": {{"env": {{"pname":"doc","version":"1","setupCompilerEnvironmentPhase":"x"}},
                               "outputs": {{"out": {{"path":"h-doc"}}}}}}
                }}}}"#
            )
        };
        let raw = RawClosure::parse(&closure(r#","nativeBuildInputs":"/nix/store/h-doc""#)).unwrap();
        assert_eq!(consumed_haskell_builds(&raw, "d-wrap.drv"), set(&["d-hs.drv"]));
        let raw = RawClosure::parse(&closure(
            r#","nativeBuildInputs":"/nix/store/h-doc","buildInputs":"/nix/store/h-doc""#,
        ))
        .unwrap();
        assert_eq!(
            consumed_haskell_builds(&raw, "d-wrap.drv"),
            set(&["d-doc.drv", "d-hs.drv"]),
            "control: also an artifact input, so counted"
        );
    }
}
