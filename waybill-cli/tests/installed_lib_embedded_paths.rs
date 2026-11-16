//! Spec 1534 (#1532): installed binaries attributed to their package through
//! the source paths embedded in them, and bundled third-party trees
//! reported. Synthetic prefixes and ELF files; nothing is fetched.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use common::installed_lib::{
    component, components, doc_field_all_formats, dpkg_owning, elf_shared_lib, prop, row_all_formats, scan, source_files,
    write, Docs,
};
use serde_json::Value;

const C227: &str = "waybill:embedded-path-binaries";

/// An ELF shared library whose `.rodata` holds `paths`, NUL-separated.
fn binary(root: &Path, rel: &str, paths: &[&str]) {
    let mut rodata = format!("\0{rel}\0").into_bytes();
    for p in paths {
        rodata.extend_from_slice(p.as_bytes());
        rodata.push(0);
    }
    let file = root.join(rel);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, elf_shared_lib(&rodata)).unwrap();
}

/// The US1 prefix. `with_paths: false` is the control: the same binaries,
/// without embedded paths.
fn us1_tree(root: &Path, with_paths: bool) {
    let s = "opt/sdk";
    write(root, &format!("{s}/lib/pkgconfig/waybill-fixture-p.pc"), "Version: 1.2.3\n");
    write(root, &format!("{s}/lib/pkgconfig/waybill-fixture-q.pc"), "Version: 2.0\n");
    let p = |v: &'static [&'static str]| if with_paths { v } else { &[][..] };
    binary(root, &format!("{s}/lib/libone.so"), p(&["/w/projects/waybill-fixture-p/lib/one.cc"]));
    binary(root, &format!("{s}/lib/libshared.so"), p(&["/w/projects/waybill-fixture-p/a.cpp", "/w/projects/waybill-fixture-q/b.cpp"]));
    binary(root, &format!("{s}/lib/libhdr.so"), p(&["/w/projects/waybill-fixture-p/include/x.hpp", "/w/app/main.cc"]));
    binary(root, &format!("{s}/lib/libtool.so"), p(&["/builddir/build/BUILD/gcc-13/x.cc", "/usr/include/c++/13/v.h"]));
    binary(root, &format!("{s}/lib/libnone.so"), p(&["/w/projects/waybill-fixture-z/a.cc"]));
    // Already attributed to q by its own name (#1531); its paths name p.
    binary(root, &format!("{s}/lib/libwaybill-fixture-q.so.2"), p(&["/w/projects/waybill-fixture-p/c.cc"]));
    // Test-scoped by #1564 (a test directory in an installed tree).
    binary(root, &format!("{s}/share/waybill-fixture-p/tests/t-exe"), p(&["/w/projects/waybill-fixture-p/test/t.cc"]));
}

fn c227(c: &Value) -> Value {
    prop(c, C227).map(|v| serde_json::from_str(v).unwrap()).unwrap_or(Value::Null)
}

fn embedded_cov(docs: &Docs) -> Value {
    doc_field_all_formats(docs, "waybill:installed-library-coverage").expect("C225")["embedded-paths"].clone()
}

/// US1 scenarios 1-6 and C225 `embedded-paths`, in all three formats.
#[test]
fn us1_binaries_are_attributed_through_their_compiled_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, true);
    let docs = scan(&root);

    let p = component(&docs.cdx, "pkg:generic/waybill-fixture-p@1.2.3").expect("p");
    let q = component(&docs.cdx, "pkg:generic/waybill-fixture-q@2.0").expect("q");
    let pf = source_files(p);
    let qf = source_files(q);
    let has = |files: &[String], name: &str| files.iter().any(|f| f.ends_with(name));

    // 1: one package. 2: shared. 6: a test-scoped binary is attributed.
    assert!(has(&pf, "/libone.so") && has(&pf, "/libshared.so") && has(&pf, "/t-exe"), "{pf:?}");
    assert!(has(&qf, "/libshared.so"), "{qf:?}");
    // 3: header-only, 4: toolchain-only, unmatched: nowhere.
    for f in ["/libhdr.so", "/libtool.so", "/libnone.so"] {
        assert!(!has(&pf, f) && !has(&qf, f), "{f}");
    }
    // 5: already attributed to q by name; not re-attributed to p.
    assert!(has(&qf, "/libwaybill-fixture-q.so.2"));
    assert!(!has(&pf, "/libwaybill-fixture-q.so.2"));

    assert_eq!(
        c227(p),
        serde_json::json!([
            {"path": "opt/sdk/lib/libone.so"},
            {"path": "opt/sdk/lib/libshared.so", "shared-with": ["waybill-fixture-q"]},
            {"path": "opt/sdk/share/waybill-fixture-p/tests/t-exe"}
        ])
    );
    assert_eq!(c227(q), serde_json::json!([{"path": "opt/sdk/lib/libshared.so", "shared-with": ["waybill-fixture-p"]}]));
    // C227 agrees across the three formats (the extractor yields one value
    // per array entry: two for p, and q's one).
    let rows = row_all_formats(&docs, "C227");
    assert_eq!(rows.len(), 4, "{rows:?}");

    // 6: the test scope is unchanged by attribution.
    let t = components(&docs.cdx).into_iter().find(|c| c["name"] == "t-exe").unwrap();
    assert_eq!(t["scope"], "excluded");

    assert_eq!(
        embedded_cov(&docs),
        serde_json::json!({"attributed": 2, "header-only": 1, "no-versioned-match": 1, "read": 6, "shared": 1, "toolchain-only": 1, "with-paths": 6})
    );
}

/// The control: no embedded paths, nothing attributed, C225 as before.
#[test]
fn us1_control_without_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, false);
    let docs = scan(&root);
    let p = component(&docs.cdx, "pkg:generic/waybill-fixture-p@1.2.3").unwrap();
    assert_eq!(c227(p), Value::Null);
    assert!(!source_files(p).iter().any(|f| f.ends_with(".so") || f.ends_with("t-exe")));
    let cov = doc_field_all_formats(&docs, "waybill:installed-library-coverage").unwrap();
    assert!(cov.get("embedded-paths").is_none(), "{cov}");
}

/// FR-005: a binary an OS package owns is neither read nor attributed.
#[test]
fn an_os_owned_binary_is_not_attributed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("rootfs");
    write(&root, "etc/os-release", "ID=debian\nVERSION_ID=\"12\"\n");
    write(&root, "opt/sdk/lib/pkgconfig/waybill-fixture-p.pc", "Version: 1.2.3\n");
    binary(&root, "opt/sdk/lib/libowned.so", &["/w/projects/waybill-fixture-p/o.cc"]);
    dpkg_owning(&root, "waybill-fixture-o", &["opt/sdk/lib/libowned.so"]);
    let docs = scan(&root);
    let p = component(&docs.cdx, "pkg:generic/waybill-fixture-p@1.2.3").unwrap();
    assert!(!source_files(p).iter().any(|f| f.ends_with("libowned.so")));
    assert_eq!(c227(p), Value::Null);
}
