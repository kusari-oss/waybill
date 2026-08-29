// milestone 669 - see specs/669-bench-harness/plan.md
//
// Entry points for the `xtask bench` and `xtask bench-docs` subcommands.
// This is the T006 scaffold; each `todo!()`-shaped stub is fleshed out
// in subsequent Phase 3+ tasks per specs/669-bench-harness/tasks.md.

use clap::Args;

pub mod compare;
pub mod docs;
pub mod matrix;
pub mod measure;
pub mod run;
pub mod schema;

/// Args for `xtask bench`. Flags land in T024 (US1) / T031 (US2) / T041 (US4).
#[derive(Args, Debug)]
pub struct BenchArgs {}

/// Runs the benchmark suite. Fleshed out in T025 (US1) + T032 (US2) + T041 (US4).
pub fn run(_args: BenchArgs) -> Result<(), Box<dyn std::error::Error>> {
    todo!("m669 T025 US1 driver — not yet implemented");
}
