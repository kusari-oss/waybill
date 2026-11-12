//! Milestone 966 (FR-004, SC-005) — the boot-library table agrees with every
//! GHC package database available to the test.
//!
//! The table is assembled from published sources (GHC's release notes,
//! Stack's `global-hints.yaml`, GHC's release tags). The package database a
//! GHC actually installs, `lib/ghc-X.Y.Z/lib/package.conf.d/*.conf`, is the
//! independent check: for each database found, every library it holds must
//! match the table, every `all` row for that release must be present, and no
//! `windows` row may be (no store GHC is a Windows GHC).
//!
//! Databases are found by listing `/nix/store` and from
//! `$WAYBILL_GHC_PACKAGE_DB_PATHS` (colon-separated `package.conf.d`
//! directories). Nothing is evaluated or built. With none found the test says
//! so and passes, unless `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1`, which the public
//! corpus workflow sets on Linux where the closure targets realise their GHCs
//! (the `WAYBILL_REQUIRE_SPDX3_VALIDATOR` pattern).
//!
//! The table is parsed here with the same rules as `boot_table.rs`, not
//! through it: a check that shares its parser with its subject would accept
//! whatever the parser got wrong.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// release -> library -> (version, platform)
type Table = BTreeMap<String, BTreeMap<String, (String, String)>>;

fn table() -> Table {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv");
    let mut t: Table = BTreeMap::new();
    for line in std::fs::read_to_string(p).unwrap().lines() {
        if line.starts_with('#') || line.starts_with("ghc\t") || line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 5, "malformed table line {line:?}");
        t.entry(f[0].to_string())
            .or_default()
            .insert(f[1].to_string(), (f[2].to_string(), f[4].to_string()));
    }
    t
}

/// `X.Y.Z` from a GHC store basename, `<hash>-ghc-X.Y.Z` or
/// `<hash>-ghc-binary-X.Y.Z`; `None` for `ghc-paths-0.1.0.12` and every other
/// package whose name merely starts with `ghc`.
fn release_of_store_name(base: &str) -> Option<String> {
    let (_, name) = base.split_once('-')?;
    let v = name.strip_prefix("ghc-binary-").or_else(|| name.strip_prefix("ghc-"))?;
    let parts: Vec<&str> = v.split('.').collect();
    (parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
        .then(|| v.to_string())
}

/// Every `(release, package.conf.d)` available.
fn databases() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/nix/store") {
        for e in entries.flatten() {
            let base = e.file_name().to_string_lossy().into_owned();
            let Some(release) = release_of_store_name(&base) else { continue };
            for rel in [
                format!("lib/ghc-{release}/lib/package.conf.d"),
                format!("lib/ghc-{release}/package.conf.d"),
            ] {
                let db = e.path().join(rel);
                if db.is_dir() {
                    out.push((release.clone(), db));
                }
            }
        }
    }
    if let Ok(paths) = std::env::var("WAYBILL_GHC_PACKAGE_DB_PATHS") {
        for p in paths.split(':').filter(|p| !p.is_empty()) {
            let db = PathBuf::from(p);
            // `…/lib/ghc-X.Y.Z/lib/package.conf.d`: the release is the
            // `ghc-X.Y.Z` path component.
            let release = db
                .components()
                .filter_map(|c| c.as_os_str().to_str())
                .filter_map(|c| c.strip_prefix("ghc-"))
                .find(|v| release_of_store_name(&format!("h-ghc-{v}")).is_some())
                .map(str::to_string);
            match release {
                Some(r) if db.is_dir() => out.push((r, db)),
                _ => panic!("WAYBILL_GHC_PACKAGE_DB_PATHS entry {p} is not a GHC package.conf.d"),
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `name -> versions` held by one database.
fn read_db(db: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in std::fs::read_dir(db).unwrap().flatten() {
        if e.path().extension().and_then(|x| x.to_str()) != Some("conf") {
            continue;
        }
        let text = std::fs::read_to_string(e.path()).unwrap();
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(key).map(|v| v.trim().to_string()))
                .unwrap_or_else(|| panic!("{} has no {key}", e.path().display()))
        };
        out.entry(field("name:")).or_default().insert(field("version:"));
    }
    out
}

/// Every disagreement between one database and the table's rows for its
/// release; empty when they agree.
fn disagreements(
    release: &str,
    db: &BTreeMap<String, BTreeSet<String>>,
    rows: &BTreeMap<String, (String, String)>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (name, versions) in db {
        match rows.get(name) {
            None => out.push(format!("{release}: the database holds {name} {versions:?}; the table has no row")),
            Some((v, _)) if versions.len() != 1 || !versions.contains(v) => {
                out.push(format!("{release}: {name} is {versions:?} in the database, {v} in the table"))
            }
            Some((_, platform)) if platform != "all" => {
                out.push(format!("{release}: {name} is a {platform} row, present on a non-Windows GHC"))
            }
            Some(_) => {}
        }
    }
    for (name, (v, platform)) in rows {
        if platform == "all" && !db.contains_key(name) {
            out.push(format!("{release}: the table lists {name} {v}; the database lacks it"));
        }
    }
    out
}

#[test]
fn ghc_boot_libraries_match_every_available_package_db() {
    let table = table();
    // CONTROL: the table parsed to something.
    assert!(table.len() >= 30, "only {} releases in the table", table.len());
    let dbs = databases();
    if dbs.is_empty() {
        println!("no GHC package database found");
        assert!(
            std::env::var("WAYBILL_REQUIRE_GHC_PACKAGE_DB").as_deref() != Ok("1"),
            "WAYBILL_REQUIRE_GHC_PACKAGE_DB=1 and no GHC package database was found"
        );
        return;
    }
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (release, path) in &dbs {
        let db = read_db(path);
        let Some(rows) = table.get(release) else {
            println!("{}: ghc-{release} is absent from the table; not checked", path.display());
            continue;
        };
        let found = disagreements(release, &db, rows);
        let all_rows = rows.values().filter(|(_, p)| p == "all").count();
        println!(
            "{}: ghc-{release}, {} libraries in the database, {all_rows} `all` rows in the table, {} disagreements",
            path.display(),
            db.len(),
            found.len()
        );
        checked += 1;
        failures.extend(found);
    }
    assert!(failures.is_empty(), "the table disagrees with a GHC package database:\n{}", failures.join("\n"));
    if checked == 0 {
        println!("no GHC package database for a release the table has");
        assert!(
            std::env::var("WAYBILL_REQUIRE_GHC_PACKAGE_DB").as_deref() != Ok("1"),
            "WAYBILL_REQUIRE_GHC_PACKAGE_DB=1 and no database matched a table release"
        );
    }
}

/// The check has teeth: one altered version, one missing library and one
/// Windows-only row are each reported. Without this, a `disagreements` that
/// returned nothing would pass against every database.
#[test]
fn a_disagreement_is_reported() {
    // Synthetic names: no committed file pairs a real boot library with a
    // version.
    let rows: BTreeMap<String, (String, String)> = [
        ("waybill-fixture-a", ("1.0", "all")),
        ("waybill-fixture-b", ("2.0", "all")),
        ("waybill-fixture-win", ("3.0", "windows")),
    ]
    .into_iter()
    .map(|(n, (v, p))| (n.to_string(), (v.to_string(), p.to_string())))
    .collect();
    let db = |pairs: &[(&str, &str)]| -> BTreeMap<String, BTreeSet<String>> {
        pairs.iter().map(|(n, v)| (n.to_string(), BTreeSet::from([v.to_string()]))).collect()
    };
    let (a, b, win) = ("waybill-fixture-a", "waybill-fixture-b", "waybill-fixture-win");
    assert!(disagreements("0.0.1", &db(&[(a, "1.0"), (b, "2.0")]), &rows).is_empty());
    assert_eq!(disagreements("0.0.1", &db(&[(a, "1.1"), (b, "2.0")]), &rows).len(), 1);
    assert_eq!(disagreements("0.0.1", &db(&[(a, "1.0")]), &rows).len(), 1);
    assert_eq!(disagreements("0.0.1", &db(&[(a, "1.0"), (b, "2.0"), (win, "3.0")]), &rows).len(), 1);
}

#[test]
fn only_a_ghc_store_path_names_a_release() {
    assert_eq!(release_of_store_name("abc-ghc-9.10.3").as_deref(), Some("9.10.3"));
    assert_eq!(release_of_store_name("abc-ghc-binary-9.8.4").as_deref(), Some("9.8.4"));
    for other in ["abc-ghc-paths-0.1.0.12", "abc-ghc-9.10.3-doc", "abc-ghc-exactprint-1.10.0.0", "abc-ghc-9.10"] {
        assert_eq!(release_of_store_name(other), None, "{other}");
    }
}
