// milestone 669 - see specs/669-bench-harness/plan.md
// bench-docs: Markdown emission from baseline.json.
// Fleshed out in T035-T037 (US3 docs generation).

use clap::Args;

/// Args for `xtask bench-docs`. Flags land in T036 (US3).
#[derive(Args, Debug)]
pub struct BenchDocsArgs {}

/// Runs the docs-generation subcommand. Fleshed out in T037 (US3).
pub fn run(_args: BenchDocsArgs) -> Result<(), Box<dyn std::error::Error>> {
    todo!("m669 T037 US3 docs generator — not yet implemented");
}
