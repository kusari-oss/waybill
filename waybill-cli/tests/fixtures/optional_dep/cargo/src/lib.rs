// Milestone 179 US3 fixture — trivial lib.rs so Cargo.toml resolves
// as a valid Rust crate under `waybill sbom scan`. The actual content
// doesn't matter; waybill's cargo reader operates on Cargo.toml +
// Cargo.lock, not on Rust source.

pub fn hello() -> &'static str {
    "m179 optional-dep fixture"
}
