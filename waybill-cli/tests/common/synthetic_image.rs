//! Spec 1657 (#1712): the synthetic container image `dual_format_perf` and
//! `holistic_parity` both scan. It lived in `dual_format_perf.rs`, which
//! `holistic_parity` declared as a submodule, so `dual_format_perf`'s two
//! tests ran in both binaries.

use std::io::Write;
use std::path::PathBuf;

/// One file inside the synthetic image's inner layer tar.
///
/// `tests/holistic_parity.rs` reuses `build_benchmark_fixture` for its synthetic-container-
/// image test case, which in turn needs this struct visible
/// across test files in the same integration-test crate.
pub(crate) struct ImageFile {
    pub(crate) path: &'static str,
    pub(crate) content: Vec<u8>,
}

/// Build a docker-save-format tarball with `files` placed in the
/// rootfs at their declared paths. Returns the path to the
/// persistent tarball (kept alive by the returned `TempDir`).
/// Mirrors the pattern in `tests/scan_image.rs`.
///
/// `pub(crate)` since milestone 013 — see `ImageFile` doc above.
pub(crate) fn build_synthetic_image(files: &[ImageFile]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut layer_bytes = Vec::new();
    {
        let mut layer_tar = tar::Builder::new(&mut layer_bytes);
        for f in files {
            let mut header = tar::Header::new_ustar();
            header.set_path(f.path).expect("set_path");
            header.set_size(f.content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            layer_tar
                .append(&header, f.content.as_slice())
                .expect("tar append");
        }
        layer_tar.finish().expect("layer finish");
    }
    let manifest = r#"[{"Config":"config.json","RepoTags":["waybill-perf:latest"],"Layers":["layer0/layer.tar"]}]"#;
    let tar_path = dir.path().join("image.tar");
    let file = std::fs::File::create(&tar_path).expect("create image.tar");
    {
        let mut outer = tar::Builder::new(file);
        let mut mh = tar::Header::new_ustar();
        mh.set_path("manifest.json").unwrap();
        mh.set_size(manifest.len() as u64);
        mh.set_mode(0o644);
        mh.set_cksum();
        outer.append(&mh, manifest.as_bytes()).expect("outer append manifest");
        let mut lh = tar::Header::new_ustar();
        lh.set_path("layer0/layer.tar").unwrap();
        lh.set_size(layer_bytes.len() as u64);
        lh.set_mode(0o644);
        lh.set_cksum();
        outer
            .append(&lh, layer_bytes.as_slice())
            .expect("outer append layer");
        outer.into_inner().expect("outer finish").flush().expect("flush");
    }
    (dir, tar_path)
}

/// Build a synthetic image where actual scan work **dominates**
/// CLI startup + docker-extract + serialization overhead by a
/// comfortable margin.
///
/// SC-009's ≥ 30 % reduction only holds when per-scan wall-clock
/// is big enough that the fixed overhead per `waybill` invocation
/// (CLI init + docker-tarball extract + enrichment no-op) is a
/// small fraction of the total. On a 100 ms-scale per-scan
/// timing, ~50 ms of that is fixed overhead — leaving only ~50 ms
/// scan work per invocation. The theoretical best-case dual-
/// format reduction is then ~25 %, below the threshold. The
/// first CI run of this test hit exactly that shape (28.3 %).
///
/// Fix: inflate the synthetic fixture until per-scan wall-clock
/// is ~500 ms+ on representative runners. Startup overhead
/// becomes a small fraction of total time and the ~50 % dual-
/// format ceiling is reachable with comfortable noise margin.
///
/// Composition: 1500 npm packages (exercises npm walker +
/// deep-hash — each package.json is ~4 KB so there's real
/// hashing per component), 500 deb stanzas in dpkg/status
/// (dpkg reader's per-stanza parse loop). At ~6 MB of
/// package.json content alone, this is a plausible lower bound
/// for a real container image's npm + deb footprint.
///
/// `pub(crate)` since milestone 013 — `tests/holistic_parity.rs`
/// reuses this fixture for its synthetic-container-image parity
/// test case (research.md §R2: scale-realism coverage without
/// checking a 60 MB image tarball into git).
pub(crate) fn build_benchmark_fixture() -> (tempfile::TempDir, PathBuf) {
    let mut files: Vec<ImageFile> = Vec::new();

    files.push(ImageFile {
        path: "etc/os-release",
        content: b"ID=debian\nVERSION_ID=12\nVERSION_CODENAME=bookworm\n".to_vec(),
    });

    // 500 deb packages — one big dpkg/status blob.
    let mut dpkg = String::new();
    for i in 0..500 {
        use std::fmt::Write as _;
        write!(
            dpkg,
            "Package: pkg-{i:04}\n\
             Status: install ok installed\n\
             Version: 1.{i}.0\n\
             Architecture: amd64\n\
             Maintainer: Debian <debian@example.org>\n\n",
        )
        .unwrap();
    }
    files.push(ImageFile {
        path: "var/lib/dpkg/status",
        content: dpkg.into_bytes(),
    });

    // 1500 npm packages. 4 KB per package.json so the deep-hash
    // pass actually has work to do per component.
    for i in 0..1500 {
        let content = format!(
            r#"{{"name":"pkg-{i:04}","version":"2.{i}.0","license":"MIT","description":"{repeat}"}}"#,
            repeat = "x".repeat(4096)
        );
        let path: &'static str = Box::leak(
            format!("usr/lib/node_modules/pkg-{i:04}/package.json").into_boxed_str(),
        );
        files.push(ImageFile {
            path,
            content: content.into_bytes(),
        });
    }

    build_synthetic_image(&files)
}
