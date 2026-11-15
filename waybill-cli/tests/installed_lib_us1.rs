//! Spec 1531 (#1531), User Story 1: every library an installed prefix
//! declares gets its exact version. Synthetic prefixes in a tempdir;
//! nothing is fetched.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use common::installed_lib::{
    component, coverage, dpkg_owning, prop, purls_with_prefix, row_all_formats, scan, skipped, source_files,
    write, Docs,
};
use serde_json::Value;

const CMAKE_VERSION_FILE: &str =
    include_str!("../src/scan_fs/package_db/installed_lib/testdata/BasicConfigVersion-SameMajor.cmake.txt");

/// The US1 tree (tasks T014): a prefix with three `.pc` files (one
/// unresolved), two CMake version files (one computed), a library stated by
/// both kinds, a source-tree area, a dpkg-owned `.pc`, and a wheel in a
/// project virtual environment.
fn us1_tree(root: &Path, with_dpkg: bool) {
    write(root, "opt/sdk/lib/pkgconfig/waybill-fixture-a.pc", "version=1.2.3\nVersion: ${version}\n");
    write(root, "opt/sdk/lib/pkgconfig/waybill-fixture-c.pc", "Version: 2.0\n");
    write(root, "opt/sdk/lib/pkgconfig/waybill-fixture-t.pc", "Version: ${undefined}\n");
    write(root, "opt/sdk/lib/cmake/WaybillFixtureB/WaybillFixtureBConfigVersion.cmake", CMAKE_VERSION_FILE);
    write(
        root,
        "opt/sdk/lib/cmake/WaybillFixtureX/WaybillFixtureXConfigVersion.cmake",
        "set(PACKAGE_VERSION ${WAYBILL_FIXTURE_X_VERSION})\n",
    );
    // US1 scenario 5: one library, both kinds, same version.
    write(root, "opt/sdk/lib/pkgconfig/waybill-fixture-m.pc", "Version: 7.0\n");
    write(
        root,
        "opt/sdk/lib/cmake/waybill-fixture-m/waybill-fixture-m-config-version.cmake",
        "set(PACKAGE_VERSION \"7.0\")\n",
    );
    // SC-005: templates and stray files in a source tree.
    write(root, "src/waybill-fixture-in.pc.in", "Version: @VERSION@\n");
    write(root, "src/WaybillFixtureInConfigVersion.cmake.in", "set(PACKAGE_VERSION \"@V@\")\n");
    write(root, "waybill-fixture-root.pc", "Version: 8.0\n");
    write(root, "tests/waybill-fixture-tst.pc", "Version: 9.0\n");
    // FR-015: a dpkg-owned `.pc`.
    let owned = "usr/lib/x86_64-linux-gnu/pkgconfig/waybill-fixture-z.pc";
    write(root, owned, "Version: 6.0\n");
    if with_dpkg {
        dpkg_owning(root, "waybill-fixture-z-dev", &[owned]);
    }
    // Analysis C1: a wheel in `.venv`. pip's RECORD lists the `.pc`; that is a
    // language claim, not an OS one, so it is still read.
    let site = ".venv/lib/python3.12/site-packages";
    let in_wheel = "waybill_fixture_sdk/lib/pkgconfig/waybill-fixture-v.pc";
    write(root, &format!("{site}/{in_wheel}"), "Version: 5.0\n");
    write(root, &format!("{site}/waybill_fixture_sdk-1.0.dist-info/RECORD"), &format!("{in_wheel},,\n"));
    write(
        root,
        &format!("{site}/waybill_fixture_sdk-1.0.dist-info/METADATA"),
        "Metadata-Version: 2.1\nName: waybill-fixture-sdk\nVersion: 1.0\n",
    );
}

fn libs(docs: &Docs) -> Vec<String> {
    let mut v = purls_with_prefix(&docs.cdx, "pkg:generic/waybill");
    v.extend(purls_with_prefix(&docs.cdx, "pkg:generic/waybillfixture"));
    v.sort();
    v.dedup();
    v
}

#[test]
fn us1_every_declared_library_gets_its_exact_version() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, true);
    let docs = scan(&root);

    assert_eq!(
        libs(&docs),
        vec![
            "pkg:generic/waybill-fixture-a@1.2.3",
            "pkg:generic/waybill-fixture-c@2.0",
            "pkg:generic/waybill-fixture-m@7.0",
            "pkg:generic/waybill-fixture-v@5.0",
            "pkg:generic/waybillfixtureb@4.5.6",
        ],
        "SC-001: exactly the resolvable, unowned, recognised files"
    );
    let a = component(&docs.cdx, "pkg:generic/waybill-fixture-a@1.2.3").unwrap();
    assert_eq!(prop(a, "waybill:source-mechanism"), Some("pkg-config"));
    assert_eq!(prop(a, "waybill:sbom-tier"), Some("deployed"));
    assert_eq!(source_files(a), vec!["opt/sdk/lib/pkgconfig/waybill-fixture-a.pc"]);
    let b = component(&docs.cdx, "pkg:generic/waybillfixtureb@4.5.6").unwrap();
    assert_eq!(prop(b, "waybill:source-mechanism"), Some("cmake-package-version"));
    assert_eq!(prop(b, "waybill:cmake-find-package-name"), Some("WaybillFixtureB"));

    // US1 scenario 5 / SC-003: one component, the `.pc` mechanism, both files.
    let m = component(&docs.cdx, "pkg:generic/waybill-fixture-m@7.0").unwrap();
    assert_eq!(prop(m, "waybill:source-mechanism"), Some("pkg-config"));
    assert_eq!(
        source_files(m),
        vec![
            "opt/sdk/lib/cmake/waybill-fixture-m/waybill-fixture-m-config-version.cmake",
            "opt/sdk/lib/pkgconfig/waybill-fixture-m.pc",
        ]
    );

    // C55 agrees across the three formats.
    let c55 = row_all_formats(&docs, "C55");
    assert!(c55.contains("\"pkg-config\"") && c55.contains("\"cmake-package-version\""), "{c55:?}");

    // C225 (T032): exact, in all three formats.
    let cov = coverage(&docs).expect("C225 present");
    assert_eq!(cov["pkg-config"], serde_json::json!({"emitted": 4, "read": 8}), "{cov}");
    assert_eq!(cov["cmake-version"], serde_json::json!({"emitted": 2, "read": 3}), "{cov}");
    assert_eq!(skipped(&cov, "unresolved-version"), 1, "t; {cov}");
    assert_eq!(skipped(&cov, "computed-version"), 1, "X; {cov}");
    assert_eq!(skipped(&cov, "unrecognised-location"), 2, "root and tests/; {cov}");
    assert_eq!(skipped(&cov, "owned-by-os-package"), 1, "z; {cov}");
    assert_eq!(skipped(&cov, "malformed"), 0, "{cov}");
    assert_eq!(skipped(&cov, "no-version"), 0, "{cov}");
    assert_eq!(cov["version-disagreements"], 0);
}

/// SC-009 control: without the dpkg database the same file is emitted.
#[test]
fn sc009_an_unowned_copy_of_the_dpkg_file_is_emitted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, false);
    let docs = scan(&root);
    assert!(component(&docs.cdx, "pkg:generic/waybill-fixture-z@6.0").is_some());
    assert_eq!(skipped(&coverage(&docs).unwrap(), "owned-by-os-package"), 0);
}

/// T020: a `.pc` inside a Homebrew keg, reached through the prefix's
/// `lib/pkgconfig` symlink, is OS-owned. Control: without the receipt it
/// is not a keg, and it is emitted.
#[cfg(unix)]
#[test]
fn a_homebrew_keg_owns_its_pkgconfig_files() {
    for with_receipt in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let keg = "opt/homebrew/Cellar/waybill-fixture-k/1.0";
        write(&root, &format!("{keg}/lib/pkgconfig/waybill-fixture-k.pc"), "Version: 1.0\n");
        if with_receipt {
            write(&root, &format!("{keg}/INSTALL_RECEIPT.json"), "{}\n");
        }
        std::fs::create_dir_all(root.join("opt/homebrew/lib/pkgconfig")).unwrap();
        std::os::unix::fs::symlink(
            format!("/{keg}/lib/pkgconfig/waybill-fixture-k.pc"),
            root.join("opt/homebrew/lib/pkgconfig/waybill-fixture-k.pc"),
        )
        .unwrap();
        let docs = scan(&root);
        let present = component(&docs.cdx, "pkg:generic/waybill-fixture-k@1.0").is_some();
        assert_eq!(present, !with_receipt, "receipt={with_receipt}");
        let cov = coverage(&docs).unwrap();
        if with_receipt {
            assert!(skipped(&cov, "owned-by-os-package") >= 1, "{cov}");
        }
    }
}

/// T042 (spec FR-012, analysis G1): a `.pc` symlink is resolved against the
/// scan root, never the host.
/// - A relative link climbing out of the root is not read.
/// - An absolute link is read as the scanned system would read it: its
///   target under the root. The host file at the same absolute path holds a
///   different version, which must not appear. That host file existing is
///   what makes the shared walker dispatch the link at all, so this is the
///   case where reading through the link naively would read the host.
#[cfg(unix)]
#[test]
fn a_symlink_resolves_inside_the_root_never_on_the_host() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    write(tmp.path(), "outside/waybill-fixture-x.pc", "Version: 9.9.9\n");
    std::fs::create_dir_all(root.join("usr/lib/pkgconfig")).unwrap();
    std::os::unix::fs::symlink(
        "../../../../outside/waybill-fixture-x.pc",
        root.join("usr/lib/pkgconfig/waybill-fixture-x.pc"),
    )
    .unwrap();

    let host = tmp.path().join("hostside/waybill-fixture-w.pc");
    write(tmp.path(), "hostside/waybill-fixture-w.pc", "Version: 1.1.1\n");
    let host_rel = host.strip_prefix("/").unwrap().to_str().unwrap().to_string();
    write(&root, &host_rel, "Version: 3.3.3\n");
    std::os::unix::fs::symlink(&host, root.join("usr/lib/pkgconfig/waybill-fixture-w.pc")).unwrap();

    let docs = scan(&root);
    let versions: Vec<String> = common::installed_lib::components(&docs.cdx)
        .iter()
        .filter_map(|c| c["version"].as_str().map(str::to_string))
        .collect();
    assert!(!versions.iter().any(|v| v == "9.9.9"), "a file outside the root was read: {versions:?}");
    assert!(!versions.iter().any(|v| v == "1.1.1"), "the host's file was read through an absolute link: {versions:?}");
    assert!(
        component(&docs.cdx, "pkg:generic/waybill-fixture-w@3.3.3").is_some(),
        "the absolute link resolves to its target under the root: {versions:?}"
    );
}

/// T033 / SC-004: ten scans are identical in all three formats (CycloneDX's
/// per-document `serialNumber` UUID masked); a tree with no matching file
/// carries no coverage record.
#[test]
fn repeated_scans_are_identical_and_absence_carries_no_record() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, true);
    let mask = |d: &Docs| -> (Value, Value, Value) {
        let mut c = d.cdx.clone();
        c["serialNumber"] = Value::Null;
        (c, d.spdx23.clone(), d.spdx3.clone())
    };
    let first = mask(&scan(&root));
    for i in 1..10 {
        assert!(mask(&scan(&root)) == first, "run {i} differs");
    }

    let empty = tmp.path().join("empty");
    write(&empty, "README", "nothing here\n");
    let docs = scan(&empty);
    assert!(coverage(&docs).is_none(), "no matching file, no C225");
}
