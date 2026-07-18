//! Milestone 209 (#601): per-ecosystem + per-technique resolver
//! modules. Each file in this directory holds one `Resolver`-trait
//! implementation.
//!
//! Registration model (per contracts/resolver-trait.md C-3):
//! adding a new resolver means creating one new file in this
//! directory + adding one `pub(crate) mod <name>;` declaration
//! below + one entry in `RESOLVER_REGISTRY` at
//! `mikebom-cli/src/resolve/resolver_chain.rs`. No other file
//! needs editing (FR-010).
//!
//! Priority ordering (per contracts/resolver-trait.md C-5):
//! URL-family resolvers 100–94, deps.dev-hash 90, path 70,
//! hostname-fallback 40. Two resolvers declaring matching
//! priorities MUST fail `cargo build` per FR-017 (compile-time
//! check in `resolver_chain.rs`).
//!
//! Modules populated during US1 (Phase 3 of tasks.md).
