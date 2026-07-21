pub mod component_role;
pub mod deduplicator;
pub mod hash_resolver;
pub mod hostname_resolver;
pub mod path_resolver;
pub mod pipeline;
#[cfg(test)]
pub mod pipeline_legacy_reference;
pub mod reconciler;
pub mod resolver_chain;
pub mod resolver_trait;
pub mod resolvers;
// Milestone 209 — url_resolver.rs is retained ONLY as a dependency
// of the `#[cfg(test)]`-gated `pipeline_legacy_reference` SC-001
// oracle. It is not part of the shipped binary. Scheduled for
// deletion 2 releases post-m209 merge per research R6, when the
// oracle itself is removed.
#[cfg(test)]
pub mod url_resolver;
