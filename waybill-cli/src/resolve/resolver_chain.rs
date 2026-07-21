//! Milestone 209 (#601): the `ResolverChain` orchestrator plus the
//! compile-time-checked `RESOLVER_REGISTRY` that fixes chain
//! composition.
//!
//! Public API contract locked in
//! `specs/209-resolver-trait-chain/contracts/resolver-trait.md` (C-3 + C-5).
//! Data-model at `specs/209-resolver-trait-chain/data-model.md` (E5 + E6).
//!
//! Design decisions:
//! - **Q2**: priority-uniqueness enforced at compile time via a
//!   `const fn` uniqueness check on `RESOLVER_REGISTRY`. Two
//!   resolvers declaring matching priorities fail `cargo build`
//!   with a panic message pointing at this file.
//! - **R4**: first-match-wins per-input. Chain iterates in priority-
//!   descending order; the first resolver returning `Ok(components)`
//!   with `!components.is_empty()` short-circuits the chain for that
//!   input.
//! - **R5**: `tokio::task::spawn` was the original plan but was
//!   deferred — panics inside `.await`-ed futures can be caught
//!   with `std::panic::catch_unwind` on the returned future's
//!   `poll` call, but our simpler + cheaper approach is: catch
//!   `Result::Err` returns cleanly, and rely on Principle IV's
//!   no-panic discipline for individual resolvers. If a resolver
//!   DOES panic, the process aborts (matching pre-refactor
//!   pipeline.rs behavior — that code doesn't catch panics either).
//!   FR-013's panic-catch guarantee is deferred to a follow-up
//!   milestone; a WARN log is emitted for every `Err` return.

use std::time::Duration;

use waybill_common::resolution::ResolvedComponent;

use super::resolver_trait::{ResolveContext, ResolveInput, Resolver};
use super::resolvers;

/// Canonical resolver registry — locks the chain composition and
/// priority ordering. Every entry MUST correspond to a live
/// implementation registered in `ResolverChain::new_default()`;
/// mismatches are caught at chain-construction time.
///
/// Priorities are u32; higher = runs earlier. Two entries with the
/// same priority fail `cargo build` via [`assert_registry_priorities_unique`]
/// per FR-017.
pub const RESOLVER_REGISTRY: &[(&str, u32)] = &[
    ("cargo", 100),
    ("pypi", 99),
    ("npm", 98),
    ("golang", 97),
    ("maven", 96),
    ("rubygems", 95),
    ("deb", 94),
    ("deps_dev_hash", 90),
    ("path", 70),
    ("hostname_fallback", 40),
];

/// Compile-time uniqueness check for resolver priorities. Two
/// entries with matching priorities fail `cargo build` at const-eval
/// time with the panic message below.
///
/// # Compile-fail verification (FR-017 / T045)
///
/// Verified during m209 Phase 1 by temporarily editing
/// RESOLVER_REGISTRY to duplicate the "npm" priority to 100
/// (matching "cargo"); `cargo build` failed with error E0080 and
/// the panic message identifying this file. Reverted; compile-time
/// check confirmed active. A doctest-style compile-fail assertion
/// isn't possible here because the mikebom-cli lib crate doesn't
/// expose `resolve::resolver_chain` (Constitution Principle VI —
/// resolvers stay crate-internal).
pub const fn assert_registry_priorities_unique(reg: &[(&str, u32)]) {
    let mut i = 0;
    while i < reg.len() {
        let mut j = i + 1;
        while j < reg.len() {
            if reg[i].1 == reg[j].1 {
                panic!(
                    "resolver priority collision — two resolvers declared \
                     matching priorities in RESOLVER_REGISTRY. Give each \
                     resolver a unique priority in \
                     mikebom-cli/src/resolve/resolver_chain.rs (see the \
                     RESOLVER_REGISTRY const definition)."
                );
            }
            j += 1;
        }
        i += 1;
    }
}

const _: () = assert_registry_priorities_unique(RESOLVER_REGISTRY);

/// The resolver chain — ordered list of `Box<dyn Resolver>` iterated
/// per input event.
pub struct ResolverChain {
    resolvers: Vec<Box<dyn Resolver>>,
}

impl ResolverChain {
    /// Construct the default chain from `RESOLVER_REGISTRY`.
    /// Instantiates every registered resolver + sorts priority-
    /// descending. Panics at construction if a registered name has
    /// no live implementation (indicates programmer error — the
    /// registry + `new_default` MUST stay in sync).
    ///
    /// `deps_dev_timeout` is threaded to the `DepsDevHashResolver`
    /// (matches pre-refactor `ResolutionPipeline::new`).
    pub fn new_default(deps_dev_timeout: Duration) -> Self {
        let mut resolvers: Vec<Box<dyn Resolver>> = vec![
            Box::new(resolvers::cargo::CargoResolver),
            Box::new(resolvers::pypi::PypiResolver),
            Box::new(resolvers::npm::NpmResolver),
            Box::new(resolvers::golang::GolangResolver),
            Box::new(resolvers::maven::MavenResolver),
            Box::new(resolvers::rubygems::RubyGemsResolver),
            Box::new(resolvers::deb::DebResolver),
            Box::new(resolvers::deps_dev_hash::DepsDevHashResolver::new(
                deps_dev_timeout,
            )),
            Box::new(resolvers::path::PathResolver),
            Box::new(resolvers::hostname_fallback::HostnameFallbackResolver),
        ];
        resolvers.sort_by_key(|r| std::cmp::Reverse(r.priority()));

        // Sanity: the sorted `.name()` sequence MUST match
        // RESOLVER_REGISTRY in order. Panics at construction if
        // drift is detected.
        debug_assert_eq!(
            resolvers.iter().map(|r| r.name()).collect::<Vec<_>>(),
            RESOLVER_REGISTRY.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            "ResolverChain::new_default order/names disagree with RESOLVER_REGISTRY",
        );

        Self { resolvers }
    }

    /// Dispatch an input through the chain, respecting first-match-
    /// wins semantics per research R4 + preserving pre-refactor
    /// pipeline behavior for SC-001 byte-identity.
    ///
    /// For each resolver in priority-descending order:
    /// 1. Skip if `handles(input, ctx)` returns false.
    /// 2. Call `resolve(input, ctx).await`.
    /// 3. On `Ok(vec)` with `!vec.is_empty()` → return vec (short-
    ///    circuit).
    /// 4. On `Ok(vec![])` → continue to next resolver (clean
    ///    no-match).
    /// 5. On `Err(e)` → log WARN naming the resolver + continue
    ///    (per FR-013 partial semantics; panic-catch deferred).
    ///
    /// Returns `Vec::new()` when no resolver produces components.
    pub async fn run<'a>(
        &self,
        input: ResolveInput<'a>,
        ctx: &ResolveContext<'a>,
    ) -> Vec<ResolvedComponent> {
        for resolver in &self.resolvers {
            if !resolver.handles(&input, ctx) {
                continue;
            }
            match resolver.resolve(&input, ctx).await {
                Ok(components) if !components.is_empty() => return components,
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(
                        resolver = resolver.name(),
                        kind = "error",
                        "{e}"
                    );
                    continue;
                }
            }
        }
        Vec::new()
    }

    /// Introspection accessor for tests — exposes the number of
    /// registered resolvers.
    #[cfg(test)]
    pub fn registered_count(&self) -> usize {
        self.resolvers.len()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn registry_priorities_sorted_descending() {
        for pair in RESOLVER_REGISTRY.windows(2) {
            assert!(
                pair[0].1 > pair[1].1,
                "RESOLVER_REGISTRY MUST be sorted priority-descending: \
                 {} (priority {}) came before {} (priority {})",
                pair[0].0, pair[0].1, pair[1].0, pair[1].1,
            );
        }
    }

    #[test]
    fn registry_names_unique() {
        let mut names: Vec<&str> = RESOLVER_REGISTRY.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        for pair in names.windows(2) {
            assert_ne!(
                pair[0], pair[1],
                "RESOLVER_REGISTRY has duplicate name `{}`",
                pair[0]
            );
        }
    }

    #[test]
    fn registry_contains_all_expected_resolvers() {
        let expected: &[&str] = &[
            "cargo",
            "pypi",
            "npm",
            "golang",
            "maven",
            "rubygems",
            "deb",
            "deps_dev_hash",
            "path",
            "hostname_fallback",
        ];
        let actual: Vec<&str> = RESOLVER_REGISTRY.iter().map(|(n, _)| *n).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn new_default_wires_all_ten_resolvers() {
        let chain = ResolverChain::new_default(Duration::from_secs(10));
        assert_eq!(chain.registered_count(), 10);
    }

    /// T044 (Phase 5 / SC-005): every registered resolver's
    /// `technique()` MUST match the value locked in contract C-4.
    /// Fails if a resolver's technique drifts without a paired
    /// contract update.
    #[test]
    fn technique_mapping_matches_contract_c4() {
        use waybill_common::resolution::ResolutionTechnique;

        let chain = ResolverChain::new_default(Duration::from_secs(1));

        // Build a name → resolver lookup by iterating the chain's
        // registered resolvers. resolvers is private; use registered_count
        // as a sanity + iterate via the sorted RESOLVER_REGISTRY.
        // We assert count first, then check each expected mapping.
        assert_eq!(chain.registered_count(), 10);

        let expected: &[(&str, ResolutionTechnique)] = &[
            ("cargo", ResolutionTechnique::UrlPattern),
            ("pypi", ResolutionTechnique::UrlPattern),
            ("npm", ResolutionTechnique::UrlPattern),
            ("golang", ResolutionTechnique::UrlPattern),
            ("maven", ResolutionTechnique::UrlPattern),
            ("rubygems", ResolutionTechnique::UrlPattern),
            ("deb", ResolutionTechnique::UrlPattern),
            ("deps_dev_hash", ResolutionTechnique::HashMatch),
            ("path", ResolutionTechnique::FilePathPattern),
            ("hostname_fallback", ResolutionTechnique::HostnameHeuristic),
        ];

        for (name, expected_technique) in expected {
            let found = chain.resolvers.iter().find(|r| r.name() == *name);
            let resolver = found.unwrap_or_else(|| {
                panic!("resolver `{name}` not registered in ResolverChain::new_default")
            });
            assert_eq!(
                &resolver.technique(),
                expected_technique,
                "SC-005/C-4 violation: resolver `{name}` technique() = {:?}, contract says {:?}",
                resolver.technique(),
                expected_technique,
            );
        }
    }
}
