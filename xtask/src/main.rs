use std::process::Command;

use clap::Parser;
use xtask::bench;
use xtask::quality;

#[derive(Parser)]
enum Cli {
    /// Build the eBPF programs
    Ebpf,
    /// Run the perf benchmark suite (milestone 669)
    Bench(bench::BenchArgs),
    /// Regenerate docs/perf/numbers.md from the committed baseline (milestone 669)
    BenchDocs(bench::docs::BenchDocsArgs),
    /// Measure SBOM quality across the pinned public-repo corpus (milestone 770)
    Quality(quality::QualityArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli {
        Cli::Ebpf => {
            build_ebpf();
            Ok(())
        }
        Cli::Bench(args) => bench::run(args),
        Cli::BenchDocs(args) => bench::docs::run(args),
        Cli::Quality(args) => quality::run(args),
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
