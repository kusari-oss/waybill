"""Apply one named mutation for spec 1536 (T017). Usage: mutate.py <repo> <id>."""
import sys
repo, mid = sys.argv[1], sys.argv[2]
M = {
 "M1": ("waybill-cli/src/scan_fs/package_db/mod.rs",
        "    Ok(DbScanResult {\n        entries: out,\n        claimed_paths: claimed,",
        "    claimed.retain(|p| record_owners.owner_of(p).is_none());\n    Ok(DbScanResult {\n        entries: out,\n        claimed_paths: claimed,"),
 "M2": ("waybill-cli/src/scan_fs/binary/owned.rs",
        "            file.parent_purl = Some(owner.clone());", "            let _ = owner;"),
 "M3": ("waybill-cli/src/scan_fs/binary/mod.rs",
        "                    e.parent_purl = Some(file_level_purl.as_str().to_string());\n                    owned_children.insert",
        "                    owned_children.insert"),
 "M4": ("waybill-cli/src/scan_fs/binary/owned.rs",
        "            for d in depends {\n                if !file.depends.contains(&d) {\n                    file.depends.push(d);\n                }\n            }",
        "            let _ = depends;"),
 "M5": ("waybill-cli/src/scan_fs/binary/mod.rs",
        "                            owned_internal.insert(base.to_string());\n                            continue;",
        "                            owned_internal.insert(base.to_string());"),
 "M6": ("waybill-cli/src/scan_fs/binary/mod.rs",
        "                entry::rename_file_level(&mut file_level, &name);", "                let _ = name;"),
 "M7": ("waybill-cli/src/scan_fs/binary/entry.rs",
        "    if !dir.ends_with(\".libs\") || dir == \".libs\" {\n        return None;\n    }", "    let _ = dir;"),
 "M8": ("waybill-cli/src/scan_fs/package_db/pip/dist_info.rs",
        "    for site_packages in candidate_site_packages_roots(rootfs) {\n        let Ok(read_dir) = std::fs::read_dir(&site_packages) else {\n            continue;\n        };\n        for entry in read_dir.flatten() {\n            let p = entry.path();\n            if p.extension().and_then(|s| s.to_str()) == Some(\"dist-info\") {",
        "    for site_packages in candidate_site_packages_roots(rootfs).into_iter().filter(|s| s.as_path() == rootfs) {\n        let Ok(read_dir) = std::fs::read_dir(&site_packages) else {\n            continue;\n        };\n        for entry in read_dir.flatten() {\n            let p = entry.path();\n            if p.extension().and_then(|s| s.to_str()) == Some(\"dist-info\") {"),
 "M9": ("waybill-cli/src/generate/cyclonedx/builder.rs",
        "        for (i, nests) in entry_nests.iter().enumerate() {\n            if !nests {",
        "        for (i, nests) in entry_nests.iter().enumerate() {\n            if !nests || i < usize::MAX {"),
 "M10": ("waybill-cli/src/scan_fs/binary/owned.rs",
        "        if !children.is_empty() {\n            coverage.identified += 1;",
        "        if !children.is_empty() || !b.links.is_empty() {\n            coverage.identified += 1;"),
 "M11": ("waybill-cli/src/scan_fs/package_db/pip/dist_info.rs",
        "        self.by_path.entry(abs_path).or_insert_with(|| owner.to_path_buf());",
        "        self.by_path.insert(abs_path, owner.to_path_buf());"),
 "M12": ("waybill-cli/src/generate/spdx/packages.rs",
        "        if !seen.insert(c.purl.as_str()) {\n            continue;\n        }",
        "        let _ = seen.insert(c.purl.as_str());"),
 "M13": ("waybill-cli/src/generate/cyclonedx/builder.rs",
        "self.cross_ecosystem_edges_report.as_ref(), Some(&bom_refs));",
        "self.cross_ecosystem_edges_report.as_ref(), None::<&BomRefIndex>.or(None));"),
}
path, old, new = M[mid]
p = f"{repo}/{path}"
s = open(p).read()
n = s.count(old)
if n != 1:
    sys.exit(f"{mid}: anchor found {n} times in {path}")
open(p, "w").write(s.replace(old, new))
print(path)
