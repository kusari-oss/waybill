//! Spec 1538 (#1519): why a CMake fetch declaration might not be taken.
//!
//! The shapes and their scope are the ones CMake 4.4.5 was measured to
//! honour (`specs/1538-cmake-nested-fetches/measurements/cmake-semantics/`):
//! `FIND_PACKAGE_ARGS` is skipped when the package is found, unless
//! `FETCHCONTENT_TRY_FIND_PACKAGE_MODE NEVER` is in force (P2a, P4); `ALWAYS`
//! makes every `FetchContent_Declare` try `find_package` first (P3); a
//! dependency provider never sees `ExternalProject_Add` (P5). The value of
//! each shape is C122 `waybill:optional-derivation`.

use std::sync::OnceLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchKind {
    FetchContent,
    ExternalProject,
}

/// In precedence order: the first that applies is the reason given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FallbackShape {
    FindPackageArgs,
    NotTarget,
    FindPackageFallback,
    TryFindPackageAlways,
    DependencyProvider,
    FunctionBody,
    Conditional,
}

impl FallbackShape {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FallbackShape::FindPackageArgs => "cmake-find-package-args",
            FallbackShape::NotTarget => "cmake-not-target",
            FallbackShape::FindPackageFallback => "cmake-find-package-fallback",
            FallbackShape::TryFindPackageAlways => "cmake-try-find-package-always",
            FallbackShape::DependencyProvider => "cmake-dependency-provider",
            FallbackShape::FunctionBody => "cmake-function-body",
            FallbackShape::Conditional => "cmake-conditional",
        }
    }
}

/// `FETCHCONTENT_TRY_FIND_PACKAGE_MODE`. `OPT_IN` is CMake's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TryMode {
    #[default]
    OptIn,
    Always,
    Never,
}

/// What is in force for one declaration.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Context {
    pub(crate) mode: TryMode,
    /// The tree defines a provider whose `SUPPORTED_METHODS` include
    /// `FETCHCONTENT_MAKEAVAILABLE_SERIAL`.
    pub(crate) fetchcontent_provider: bool,
}

fn re(cell: &'static OnceLock<Option<Regex>>, pattern: &str) -> Option<&'static Regex> {
    cell.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

fn not_target(c: &str) -> bool {
    static R: OnceLock<Option<Regex>> = OnceLock::new();
    re(&R, r"\bNOT\s+TARGET\b").is_some_and(|r| r.is_match(c))
}

fn not_found(c: &str) -> bool {
    static R: OnceLock<Option<Regex>> = OnceLock::new();
    re(&R, r"\bNOT\s+\S+_FOUND\b").is_some_and(|r| r.is_match(c))
}

/// The shape of one declaration, or `None` when it is unconditional.
/// An `else:` branch never takes a guard's specific reason: the `else` of
/// `if(NOT TARGET x)` runs when the target exists.
pub(crate) fn classify(
    kind: FetchKind,
    args: &[String],
    if_conditions: &[String],
    in_function: bool,
    ctx: Context,
) -> Option<FallbackShape> {
    let fc = kind == FetchKind::FetchContent;
    let guards: Vec<&str> = if_conditions.iter().map(String::as_str).filter(|c| !c.starts_with("else: ")).collect();
    if fc && ctx.mode != TryMode::Never && args.iter().any(|a| a == "FIND_PACKAGE_ARGS") {
        return Some(FallbackShape::FindPackageArgs);
    }
    if guards.iter().any(|c| not_target(c)) {
        return Some(FallbackShape::NotTarget);
    }
    if guards.iter().any(|c| not_found(c)) {
        return Some(FallbackShape::FindPackageFallback);
    }
    if fc && ctx.mode == TryMode::Always {
        return Some(FallbackShape::TryFindPackageAlways);
    }
    if fc && ctx.fetchcontent_provider {
        return Some(FallbackShape::DependencyProvider);
    }
    if in_function {
        return Some(FallbackShape::FunctionBody);
    }
    (!if_conditions.is_empty()).then_some(FallbackShape::Conditional)
}

/// `BUILD_TESTING` in an enclosing `if`, not in an `else:` branch and not
/// `OR`-combined (spec FR-008, research R7).
pub(crate) fn build_testing(if_conditions: &[String]) -> bool {
    static BT: OnceLock<Option<Regex>> = OnceLock::new();
    static OR: OnceLock<Option<Regex>> = OnceLock::new();
    if_conditions.iter().any(|c| {
        !c.starts_with("else: ")
            && re(&BT, r"\bBUILD_TESTING\b").is_some_and(|r| r.is_match(c))
            && !re(&OR, r"\bOR\b").is_some_and(|r| r.is_match(c))
    })
}

/// `set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE <mode> …)`.
pub(crate) fn mode_set(args: &[String]) -> Option<TryMode> {
    if args.first().map(String::as_str) != Some("FETCHCONTENT_TRY_FIND_PACKAGE_MODE") {
        return None;
    }
    match args.get(1).map(|s| s.to_ascii_uppercase()).as_deref() {
        Some("ALWAYS") => Some(TryMode::Always),
        Some("NEVER") => Some(TryMode::Never),
        Some("OPT_IN") => Some(TryMode::OptIn),
        _ => None,
    }
}

/// `cmake_language(SET_DEPENDENCY_PROVIDER <cmd> SUPPORTED_METHODS …)` that
/// lists `FETCHCONTENT_MAKEAVAILABLE_SERIAL`.
pub(crate) fn fetchcontent_provider(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("SET_DEPENDENCY_PROVIDER")
        && args.iter().skip_while(|a| *a != "SUPPORTED_METHODS").any(|a| a == "FETCHCONTENT_MAKEAVAILABLE_SERIAL")
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }
    const FC: FetchKind = FetchKind::FetchContent;
    const EP: FetchKind = FetchKind::ExternalProject;

    #[test]
    fn find_package_args_unless_never() {
        let args = v(&["x", "URL", "u", "FIND_PACKAGE_ARGS", "NAMES", "X"]);
        assert_eq!(classify(FC, &args, &[], false, Context::default()), Some(FallbackShape::FindPackageArgs));
        let never = Context { mode: TryMode::Never, ..Default::default() };
        assert_eq!(classify(FC, &args, &[], false, never), None, "NEVER cancels it (P4)");
    }

    #[test]
    fn guards_and_their_else_branches() {
        let c = v(&["NOT TARGET GTest::GTest AND NOT TARGET GTest::gtest"]);
        assert_eq!(classify(EP, &v(&["x"]), &c, false, Context::default()), Some(FallbackShape::NotTarget));
        let c = v(&["NOT GTest_FOUND"]);
        assert_eq!(classify(FC, &v(&["x"]), &c, false, Context::default()), Some(FallbackShape::FindPackageFallback));
        let c = v(&["else: NOT TARGET x"]);
        assert_eq!(classify(FC, &v(&["x"]), &c, false, Context::default()), Some(FallbackShape::Conditional));
        let c = v(&["WIN32"]);
        assert_eq!(classify(FC, &v(&["x"]), &c, false, Context::default()), Some(FallbackShape::Conditional));
    }

    #[test]
    fn always_and_provider_are_fetchcontent_only() {
        let always = Context { mode: TryMode::Always, ..Default::default() };
        assert_eq!(classify(FC, &v(&["x"]), &[], false, always), Some(FallbackShape::TryFindPackageAlways));
        assert_eq!(classify(EP, &v(&["x"]), &[], false, always), None, "P5: ExternalProject never calls find_package");
        let prov = Context { fetchcontent_provider: true, ..Default::default() };
        assert_eq!(classify(FC, &v(&["x"]), &[], false, prov), Some(FallbackShape::DependencyProvider));
        assert_eq!(classify(EP, &v(&["x"]), &[], false, prov), None);
    }

    #[test]
    fn function_body_and_unconditional() {
        assert_eq!(classify(FC, &v(&["x"]), &[], true, Context::default()), Some(FallbackShape::FunctionBody));
        assert_eq!(classify(FC, &v(&["x"]), &[], false, Context::default()), None);
    }

    #[test]
    fn build_testing_not_in_or_or_else() {
        assert!(build_testing(&v(&["BUILD_TESTING"])));
        assert!(build_testing(&v(&["BUILD_TESTING AND FOO"])));
        assert!(!build_testing(&v(&["BUILD_TESTING OR FOO"])));
        assert!(!build_testing(&v(&["else: BUILD_TESTING"])));
        assert!(!build_testing(&v(&["MY_BUILD_TESTING_X"])), "word boundary");
    }

    #[test]
    fn mode_and_provider_parsing() {
        assert_eq!(mode_set(&v(&["FETCHCONTENT_TRY_FIND_PACKAGE_MODE", "ALWAYS"])), Some(TryMode::Always));
        assert_eq!(mode_set(&v(&["FETCHCONTENT_TRY_FIND_PACKAGE_MODE", "never", "CACHE", "STRING", "x"])), Some(TryMode::Never));
        assert_eq!(mode_set(&v(&["OTHER", "ALWAYS"])), None);
        assert!(fetchcontent_provider(&v(&["SET_DEPENDENCY_PROVIDER", "p", "SUPPORTED_METHODS", "FIND_PACKAGE", "FETCHCONTENT_MAKEAVAILABLE_SERIAL"])));
        assert!(!fetchcontent_provider(&v(&["SET_DEPENDENCY_PROVIDER", "p", "SUPPORTED_METHODS", "FIND_PACKAGE"])), "TheRock's provider");
    }
}
