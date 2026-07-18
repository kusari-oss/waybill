//! Milestone 209 (#601): the `ResolverChain` orchestrator plus the
//! compile-time-checked `RESOLVER_REGISTRY` that fixes chain
//! composition.
//!
//! Public API contract locked in
//! `specs/209-resolver-trait-chain/contracts/resolver-trait.md` (C-3
//! + C-5). Data-model at
//! `specs/209-resolver-trait-chain/data-model.md` (E5 + E6).
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
//! - **R5**: `tokio::task::spawn` + `JoinError::is_panic()` for
//!   panic-catch (no `futures-util` dep needed).

use mikebom_common::resolution::ResolvedComponent;

use super::resolver_trait::{ResolveContext, ResolveInput, Resolver};

/// Canonical resolver registry — locks the chain composition and
/// priority ordering. Every entry MUST correspond to a live
/// implementation registered in `ResolverChain::new_default()`;
/// mismatches are caught at chain-construction time.
///
/// Priorities are u32; higher = runs earlier. Two entries with the
/// same priority fail `cargo build` via [`assert_registry_priorities_unique`]
/// per FR-017.
///
/// See contract C-3 (registration point) + C-5 (dispatch order).
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

/// Compile-time uniqueness check for resolver priorities. Called
/// via `const _: () = assert_registry_priorities_unique(RESOLVER_REGISTRY);`
/// below — a collision fails `cargo build` at const-eval time with
/// a panic message pointing at this file.
///
/// Const-fn limitations: the panic message can't dynamically embed
/// the two colliding resolver names (const-eval doesn't allow
/// runtime string formatting). Mitigation: the sorted `RESOLVER_REGISTRY`
/// array + the file-pointer in the message make manual diagnosis
/// trivial (~5-10 seconds to eyeball the array for duplicates).
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

// Compile-time uniqueness enforcement — see FR-017.
const _: () = assert_registry_priorities_unique(RESOLVER_REGISTRY);

/// The resolver chain — ordered list of `Box<dyn Resolver>` iterated
/// per input event.
///
/// Constructed from the compile-in `RESOLVER_REGISTRY` via
/// [`ResolverChain::new_default`]. Iteration is priority-descending;
/// per-input first-match-wins semantics preserve pre-refactor
/// behavior (research R4) and satisfy SC-001.
pub struct ResolverChain {
    resolvers: Vec<Box<dyn Resolver>>,
}

impl ResolverChain {
    /// Construct the default chain. Currently a placeholder —
    /// populated during Phase 3 (US1) as each resolver's file lands.
    /// Once complete, will instantiate one `Box::new(<T>)` per
    /// resolver, sort by `.priority()` descending, and assert
    /// (`debug_assert!`) that the sorted `.name()` sequence matches
    /// `RESOLVER_REGISTRY` in order.
    pub fn new_default() -> Self {
        Self { resolvers: Vec::new() }
    }

    /// Dispatch an input through the chain, respecting first-match-
    /// wins semantics per research R4. Returns the first non-empty
    /// `Vec<ResolvedComponent>` returned by a resolver whose
    /// `handles()` returned true, OR `Vec::new()` if every resolver
    /// returned an empty vec.
    ///
    /// Per FR-013, wraps every resolver.resolve invocation in a
    /// panic-catch (via `tokio::task::spawn` per R5). Both Err and
    /// panic emit a WARN naming the resolver + kind, then the chain
    /// continues.
    ///
    /// Currently a placeholder — implemented during Phase 3 (T027)
    /// once resolvers are wired.
    pub async fn run(
        &self,
        _input: ResolveInput<'_>,
        _ctx: &ResolveContext<'_>,
    ) -> Vec<ResolvedComponent> {
        Vec::new()
    }

    /// Introspection accessor for tests — exposes the number of
    /// registered resolvers. Not a public API; test-only use.
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
        // Not required by the compile-time check, but a nice
        // stylistic invariant that makes eyeballing the array for
        // typos + collisions easier.
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
        // Complements the compile-time priority-uniqueness check
        // with a runtime name-uniqueness check. Not required for
        // correctness (duplicate names would break
        // ResolverChain::new_default's name→impl lookup), but
        // catches the bug earlier + with a better error message.
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
        // Sanity check: the registered names match what contract C-5
        // + data-model E6 declare.
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
    fn new_default_placeholder_returns_empty() {
        // Placeholder shape until Phase 3 wires the real resolvers.
        let chain = ResolverChain::new_default();
        assert_eq!(chain.registered_count(), 0);
    }
}
