#![allow(clippy::unwrap_used)]
mod common;
use common::installed_lib::{elf_shared_lib, scan};

fn lib(paths: &[&str], tag: &str) -> Vec<u8> {
    let mut rodata = format!("\0{tag}\0").into_bytes();
    for p in paths {
        rodata.extend_from_slice(p.as_bytes());
        rodata.push(0);
    }
    elf_shared_lib(&rodata)
}

#[test]
#[ignore]
fn probe() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // The SDK's shape: two symlink chains ending in different bytes.
    let a = lib(&["/w/third_party/waybill-fixture-gotcha/a.c"], "three");
    let b = lib(&["/w/third_party/waybill-fixture-timemory/b.c"], "two");
    std::fs::create_dir_all(root.join("lib/sdk")).unwrap();
    std::fs::create_dir_all(root.join("lib/systems")).unwrap();
    std::fs::write(root.join("lib/sdk/libwaybill-fixture-g.so.3.0.0"), &a).unwrap();
    std::os::unix::fs::symlink("libwaybill-fixture-g.so.3.0.0", root.join("lib/sdk/libwaybill-fixture-g.so.3")).unwrap();
    std::os::unix::fs::symlink("libwaybill-fixture-g.so.3", root.join("lib/sdk/libwaybill-fixture-g.so")).unwrap();
    std::fs::write(root.join("lib/systems/libwaybill-fixture-g.so.2.4.2"), &b).unwrap();
    std::os::unix::fs::symlink("libwaybill-fixture-g.so.2.4.2", root.join("lib/systems/libwaybill-fixture-g.so.2")).unwrap();
    std::os::unix::fs::symlink("libwaybill-fixture-g.so.2", root.join("lib/systems/libwaybill-fixture-g.so")).unwrap();
    for i in 0..12 {
        let docs = scan(root);
        let mut out = Vec::new();
        fn walk(v: &serde_json::Value, parent: &str, out: &mut Vec<String>) {
            for c in v["components"].as_array().into_iter().flatten() {
                let p = c["purl"].as_str().unwrap_or("");
                if p.contains("gotcha") || p.contains("timemory") || p.contains("libwaybill-fixture-g") {
                    out.push(format!("{}<-{}", c["bom-ref"].as_str().unwrap_or(""), parent));
                }
                walk(c, c["purl"].as_str().unwrap_or(""), out);
            }
        }
        walk(&docs.cdx, "TOP", &mut out);
        out.sort();
        let gc = docs.cdx["metadata"]["properties"].as_array().unwrap().iter()
            .find(|p| p["name"] == "waybill:graph-completeness-reason").and_then(|p| p["value"].as_str()).unwrap_or("-").to_string();
        let mut coded = Vec::new();
        fn coded_walk(v: &serde_json::Value, out: &mut Vec<String>) {
            for c in v["components"].as_array().into_iter().flatten() {
                if let Some(r) = c["properties"].as_array().into_iter().flatten().find(|p| p["name"] == "waybill:orphan-reason") {
                    out.push(format!("{}={}", c["bom-ref"].as_str().unwrap_or(""), r["value"].as_str().unwrap_or("")));
                }
                coded_walk(c, out);
            }
        }
        coded_walk(&docs.cdx, &mut coded);
        println!("SCAN {i} gc={gc} coded={coded:?}");
        for o in out { println!("  {o}"); }
    }
}
