//! `waybill-ebpf` is its own Cargo workspace with its own `Cargo.lock`, and it
//! depends on `waybill-common` by path. Nothing updated that lockfile when the
//! workspace version moved, so it said `waybill-common 0.1.0-alpha.3` from
//! the alpha series until 0.10.1, and any eBPF build rewrote it as a stray
//! diff. `scripts/release-bump.sh` now updates it with every bump, and
//! `scripts/check-version-bump.py` checks the diff. This test catches the
//! drift if a bump is ever made another way.

#[test]
fn ebpf_lockfile_records_the_workspace_version_of_waybill_common() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("waybill-cli sits inside the workspace root");
    let lock = std::fs::read_to_string(root.join("waybill-ebpf/Cargo.lock"))
        .expect("waybill-ebpf/Cargo.lock is committed");
    let recorded = lock
        .split("[[package]]")
        .find(|block| block.contains("name = \"waybill-common\""))
        .and_then(|block| block.lines().find_map(|l| l.strip_prefix("version = ")))
        .map(|v| v.trim_matches('"'))
        .expect("waybill-ebpf/Cargo.lock has a waybill-common entry");
    assert_eq!(
        recorded,
        env!("CARGO_PKG_VERSION"),
        "waybill-ebpf/Cargo.lock records waybill-common {recorded}, but the workspace is {}. \
         Run: cargo +$(sed -n 's/^channel *= *\"\\(.*\\)\"/\\1/p' rust-toolchain.toml) \
         update --workspace --manifest-path waybill-ebpf/Cargo.toml",
        env!("CARGO_PKG_VERSION"),
    );
}
