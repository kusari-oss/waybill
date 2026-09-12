use std::process::Command;

use clap::Parser;
use xtask::bench;
use xtask::compare;
use xtask::corpus_diff;
use xtask::linkage;
use xtask::quality;

#[derive(Parser)]
enum Cli {
    /// Build the eBPF programs
    Ebpf,
    /// Run the perf benchmark suite (milestone 669)
    Bench(bench::BenchArgs),
    /// Milestone 780 — private comparative benchmark against other SBOM
    /// tools. Results never leave `target/`.
    Compare(compare::CompareArgs),
    /// Regenerate docs/perf/numbers.md from the committed baseline (milestone 669)
    BenchDocs(bench::docs::BenchDocsArgs),
    /// Measure SBOM quality across the pinned public-repo corpus (milestone 770)
    Quality(quality::QualityArgs),
    /// Assert a released binary gains no dynamic dependency beyond its
    /// platform baseline (Constitution Principle I; issue #824)
    CheckLinkage(linkage::LinkageArgs),
    /// Normalise a public-corpus golden diff for human review
    /// (feature 840; issue #763). Review tool, never a gate.
    CorpusDiff(corpus_diff::CorpusDiffArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli {
        Cli::Ebpf => {
            build_ebpf();
            Ok(())
        }
        Cli::Bench(args) => bench::run(args),
        Cli::Compare(args) => compare::run(args),
        Cli::BenchDocs(args) => bench::docs::run(args),
        Cli::Quality(args) => quality::run(args),
        Cli::CheckLinkage(args) => linkage::run(args),
        Cli::CorpusDiff(args) => corpus_diff::run(args),
    };
    if let Err(err) = result {
        eprintln!("xtask error: {err}");
        std::process::exit(1);
    }
}

fn build_ebpf() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../waybill-ebpf");

    let status = Command::new("cargo")
        .current_dir(dir)
        .args([
            "+nightly",
            "build",
            "--target=bpfel-unknown-none",
            "-Z",
            "build-std=core",
            "--release",
        ])
        .status()
        .expect("failed to build eBPF programs");

    if !status.success() {
        eprintln!("eBPF build failed with status: {status}");
        std::process::exit(1);
    }

    println!("eBPF programs built successfully");
}
