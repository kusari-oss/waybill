//! #1455, second rule — a runtime-closure reference that exists only because
//! of a darwin `lib/links` symlink does not ship its target, unless a Mach-O
//! file in a shipping output loads the dylib that symlink names.
//!
//! nixpkgs' Haskell builder on darwin gives every library output a
//! `lib/links` folder: one symlink per dylib in the build environment, test
//! dependencies included, with the output's dylibs carrying `lib/links` as
//! their first `LC_RPATH`. The symlinks are store references, so the runtime
//! closure holds every test framework any library in it was tested with.
//! On moat (aarch64-darwin, `measurements/lib-links/`) 1,179 of 3,351
//! references from the 124 outputs with a farm exist only through it, and
//! moat's own dylib loads 3 of the 33 Haskell libraries its farm names.
//!
//! # The rule
//!
//! For a runtime output `O` with a `lib/links` directory and a reference
//! `O → T`:
//!
//! * if `T`'s store hash appears anywhere in `O` outside `lib/links` (file
//!   contents, or a symlink target), the reference stands: that is how nix
//!   found it, and `nix-support/propagated-build-inputs` or a Mach-O's own
//!   path names it;
//! * else, if symlinks in `O/lib/links` point into `T`, it stands only when
//!   one of their names is the last component of an install name some
//!   Mach-O file in a **shipping** output loads. dyld resolves `@rpath/`
//!   names against the rpaths of every image in the load chain, so a name a
//!   library loads can be satisfied by its loader's farm; the loaded names
//!   are therefore pooled over every shipping output, to a fixpoint;
//! * otherwise (the hash is found nowhere, or the reference is not a store
//!   hash), it stands.
//!
//! # Where it is not assessed
//!
//! The files are read from the local store. A runtime closure a substituter
//! answered has none, an output that cannot be read could load anything,
//! and a Mach-O file whose load commands cannot be parsed could too. In
//! each case nothing is dropped and C216 says why
//! ([`NotAssessed`]). A closure in which no runtime output has a
//! `lib/links` directory (every Linux closure) is assessed without reading
//! a file.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::scan_fs::binary::macho;

/// The C216 `rule` value.
pub(crate) const RULE: &str = "unloaded-lib-links";

/// Why the rule was not assessed (C216 `not-assessed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotAssessed {
    /// A substituter answered: its files are not here to read.
    RemoteStore,
    /// A shipping output, or one with a `lib/links` folder, could not be read.
    UnreadableOutput,
    /// A Mach-O file in a shipping output has load commands that could not
    /// be parsed, so what it loads is unknown.
    UnparsedMachO,
}

impl NotAssessed {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::RemoteStore => "remote-store",
            Self::UnreadableOutput => "unreadable-output",
            Self::UnparsedMachO => "unparsed-mach-o",
        }
    }
}

/// The nix base-32 alphabet store hashes are written in.
const NIX_BASE32: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// The 32-character hash of a store basename, when it has one.
pub(crate) fn store_hash(basename: &str) -> Option<&str> {
    let h = basename.get(..32)?;
    (basename.as_bytes().get(32) == Some(&b'-') && h.bytes().all(|b| NIX_BASE32.contains(&b)))
        .then_some(h)
}

/// Every `wanted` hash that occurs in `bytes`. A store hash is 32 base-32
/// characters, so only runs that long are looked at, the way nix scans.
fn hashes_in(bytes: &[u8], wanted: &HashSet<&str>, found: &mut HashSet<String>) {
    let mut run = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if NIX_BASE32.contains(b) {
            run += 1;
            if run >= 32 {
                if let Ok(w) = std::str::from_utf8(&bytes[i + 1 - 32..=i]) {
                    if wanted.contains(w) {
                        found.insert(w.to_string());
                    }
                }
            }
        } else {
            run = 0;
        }
    }
}

/// What one output's files say.
#[derive(Debug, Default)]
struct OutputScan {
    /// Target store hash → the `lib/links` entry names pointing into it.
    farm: HashMap<String, BTreeSet<String>>,
    /// The output's reference hashes found outside `lib/links`.
    elsewhere: HashSet<String>,
    /// The last component of every install name its Mach-O files load.
    loads: BTreeSet<String>,
}

/// Read every file under `root` without following a symlink, so the walk
/// cannot leave the output or loop. Directory entries are visited in sorted
/// order for determinism.
fn scan_tree(root: &Path, visit: &mut dyn FnMut(&Path, &std::fs::Metadata) -> Result<(), NotAssessed>) -> Result<(), NotAssessed> {
    let meta = std::fs::symlink_metadata(root).map_err(|_| NotAssessed::UnreadableOutput)?;
    visit(root, &meta)?;
    if !meta.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .map_err(|_| NotAssessed::UnreadableOutput)?
        .map(|e| e.map(|e| e.path()).map_err(|_| NotAssessed::UnreadableOutput))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for e in entries {
        scan_tree(&e, visit)?;
    }
    Ok(())
}

/// The last components of the install names a Mach-O file loads, `None`
/// when the file is not Mach-O, or an error when it is and its load commands
/// cannot be parsed.
fn macho_loads(bytes: &[u8]) -> Result<Option<Vec<String>>, NotAssessed> {
    let slices: Vec<&[u8]> = if macho::is_thin_macho(bytes) {
        vec![bytes]
    } else if let Some(fat) = macho::fat_slices(bytes) {
        fat.iter()
            .map(|&(off, size)| {
                let (off, size) = (usize::try_from(off).ok()?, usize::try_from(size).ok()?);
                bytes.get(off..off.checked_add(size)?)
            })
            .collect::<Option<_>>()
            .ok_or(NotAssessed::UnparsedMachO)?
    } else {
        return Ok(None);
    };
    let mut names = Vec::new();
    // A universal static archive's slices are `ar` archives, not images
    // (apple-sdk's universal `.a` files, in moat's runtime closure): they
    // load nothing at run time. A slice that is a Mach-O image must parse.
    for s in slices.into_iter().filter(|s| macho::is_thin_macho(s)) {
        let loads = macho::parse_load_dylibs(s).ok_or(NotAssessed::UnparsedMachO)?;
        names.extend(loads.into_iter().map(|n| n.rsplit('/').next().unwrap_or(&n).to_string()));
    }
    Ok(Some(names))
}

/// Read one output. `refs` are its references (basenames); only their hashes
/// are looked for, and only when the output has a `lib/links` folder.
fn scan_output(store: &Path, base: &str, refs: &BTreeSet<String>) -> Result<OutputScan, NotAssessed> {
    let root = store.join(base);
    let links = root.join("lib").join("links");
    let has_farm = std::fs::symlink_metadata(&links).is_ok_and(|m| m.is_dir());
    let wanted: HashSet<&str> = if has_farm {
        refs.iter().filter(|r| *r != base).filter_map(|r| store_hash(r)).collect()
    } else {
        HashSet::new()
    };
    let store_prefix = format!("{}/", store.display());
    let mut scan = OutputScan::default();
    scan_tree(&root, &mut |path, meta| {
        let in_farm = path.parent() == Some(links.as_path());
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(path).map_err(|_| NotAssessed::UnreadableOutput)?;
            let target = target.to_string_lossy();
            let farm_target = target
                .strip_prefix(&store_prefix)
                .and_then(|rest| store_hash(rest.split('/').next().unwrap_or("")));
            match (in_farm, farm_target) {
                (true, Some(h)) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                    scan.farm.entry(h.to_string()).or_default().extend(name);
                }
                _ => hashes_in(target.as_bytes(), &wanted, &mut scan.elsewhere),
            }
            return Ok(());
        }
        if !meta.is_file() {
            return Ok(());
        }
        let mut f = std::fs::File::open(path).map_err(|_| NotAssessed::UnreadableOutput)?;
        let mut head = [0u8; 4];
        let n = f.read(&mut head).map_err(|_| NotAssessed::UnreadableOutput)?;
        let maybe_macho = n == 4
            && (macho::is_thin_macho(&head) || matches!(head, [0xca, 0xfe, 0xba, 0xbe | 0xbf]));
        if !has_farm && !maybe_macho {
            return Ok(());
        }
        let bytes = std::fs::read(path).map_err(|_| NotAssessed::UnreadableOutput)?;
        if has_farm {
            hashes_in(&bytes, &wanted, &mut scan.elsewhere);
        }
        if maybe_macho {
            if let Some(loads) = macho_loads(&bytes)? {
                scan.loads.extend(loads);
            }
        }
        Ok(())
    })?;
    Ok(scan)
}

/// The runtime paths reachable from `roots` through `references`, never
/// entering a path `stop` names, and following an edge only when `keep`
/// allows it.
pub(crate) fn reachable<'a>(
    roots: &'a [String],
    references: &'a std::collections::BTreeMap<String, BTreeSet<String>>,
    stop: &dyn Fn(&str) -> bool,
    keep: &mut dyn FnMut(&str, &str) -> Result<bool, NotAssessed>,
) -> Result<HashSet<&'a str>, NotAssessed> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = roots.iter().map(String::as_str).collect();
    while let Some(p) = stack.pop() {
        if stop(p) || !seen.insert(p) {
            continue;
        }
        if let Some(rs) = references.get(p) {
            for t in rs {
                if keep(p, t)? {
                    stack.push(t);
                }
            }
        }
    }
    Ok(seen)
}

/// The paths that ship under both rules: reached from `roots` without
/// entering a documentation output (`is_doc`) and without following a
/// reference that exists only through an unloaded `lib/links` entry.
///
/// `local` is false when a substituter answered the runtime query.
pub(crate) fn shipping<'a>(
    store: &Path,
    local: bool,
    roots: &'a [String],
    references: &'a std::collections::BTreeMap<String, BTreeSet<String>>,
    is_doc: &dyn Fn(&str) -> bool,
) -> Result<HashSet<&'a str>, NotAssessed> {
    if !local {
        return Err(NotAssessed::RemoteStore);
    }
    // No farm anywhere (every Linux closure): nothing to read.
    let any_farm = references.keys().any(|p| {
        std::fs::symlink_metadata(store.join(p).join("lib").join("links")).is_ok_and(|m| m.is_dir())
    });
    if !any_farm {
        return reachable(roots, references, is_doc, &mut |_, _| Ok(true));
    }
    let mut scans: HashMap<String, OutputScan> = HashMap::new();
    let mut names: BTreeSet<String> = BTreeSet::new();
    loop {
        let reached = reachable(roots, references, is_doc, &mut |o, t| {
            let scan = scanned(&mut scans, store, references, o)?;
            let Some(h) = store_hash(t) else { return Ok(true) };
            Ok(match scan.farm.get(h) {
                Some(entries) if !scan.elsewhere.contains(h) => {
                    entries.iter().any(|e| names.contains(e))
                }
                _ => true,
            })
        })?;
        let mut grown = names.clone();
        for p in &reached {
            grown.extend(scanned(&mut scans, store, references, p)?.loads.iter().cloned());
        }
        if grown == names {
            return Ok(reached);
        }
        names = grown;
    }
}

/// [`scan_output`], once per output.
fn scanned<'s>(
    scans: &'s mut HashMap<String, OutputScan>,
    store: &Path,
    references: &std::collections::BTreeMap<String, BTreeSet<String>>,
    p: &str,
) -> Result<&'s OutputScan, NotAssessed> {
    if !scans.contains_key(p) {
        let empty = BTreeSet::new();
        let scan = scan_output(store, p, references.get(p).unwrap_or(&empty))?;
        scans.insert(p.to_string(), scan);
    }
    scans.get(p).ok_or(NotAssessed::UnreadableOutput)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn a_store_hash_is_32_base32_characters_and_a_dash() {
        assert_eq!(
            store_hash("n0mppb0j2an4ljh0x9cd35g0y9ggqak8-hspec-2.11.17"),
            Some("n0mppb0j2an4ljh0x9cd35g0y9ggqak8")
        );
        assert_eq!(store_hash("h-app-bin"), None);
        // `e`, `o`, `t` and `u` are not in nix's alphabet.
        assert_eq!(store_hash("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-x"), None);
        assert_eq!(store_hash("n0mppb0j2an4ljh0x9cd35g0y9ggqak8"), None);
    }

    #[test]
    fn hashes_are_found_only_as_whole_32_character_windows() {
        let want: HashSet<&str> = ["n0mppb0j2an4ljh0x9cd35g0y9ggqak8"].into_iter().collect();
        let mut found = HashSet::new();
        hashes_in(b"/nix/store/n0mppb0j2an4ljh0x9cd35g0y9ggqak8-hspec/lib", &want, &mut found);
        assert_eq!(found.len(), 1);
        let mut none = HashSet::new();
        hashes_in(b"n0mppb0j2an4ljh0x9cd35g0y9ggqa", &want, &mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn a_truncated_macho_is_an_error_not_an_empty_load_list() {
        let mut bytes = vec![0xcf, 0xfa, 0xed, 0xfe];
        bytes.extend_from_slice(&[0u8; 12]);
        // ncmds 1, sizeofcmds 64, but nothing follows the header.
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&64u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        assert_eq!(macho_loads(&bytes), Err(NotAssessed::UnparsedMachO));
        assert_eq!(macho_loads(b"#!/bin/sh\n"), Ok(None));
    }

    /// A universal file: an image slice is read, an `ar` slice (a universal
    /// static archive, as apple-sdk ships) is not an image and loads nothing.
    #[test]
    fn a_universal_file_reads_its_image_slices_and_skips_archive_slices() {
        let mut image = Vec::new();
        let name = b"@rpath/libHSx.dylib\0\0\0\0\0";
        for v in [0xfeed_facf_u32, 0x0100_000c, 0, 6, 1, (24 + name.len()) as u32, 0, 0] {
            image.extend_from_slice(&v.to_le_bytes());
        }
        for v in [0x0c_u32, (24 + name.len()) as u32, 24, 2, 0, 0] {
            image.extend_from_slice(&v.to_le_bytes());
        }
        image.extend_from_slice(name);
        let archive = b"!<arch>\n".to_vec();
        let mut fat = Vec::new();
        fat.extend_from_slice(&0xcafe_babe_u32.to_be_bytes());
        fat.extend_from_slice(&2u32.to_be_bytes());
        let first = 8 + 2 * 20;
        for (off, len) in [(first, archive.len()), (first + archive.len(), image.len())] {
            for v in [0x0100_000c_u32, 0, off as u32, len as u32, 0] {
                fat.extend_from_slice(&v.to_be_bytes());
            }
        }
        fat.extend_from_slice(&archive);
        fat.extend_from_slice(&image);
        assert_eq!(macho_loads(&fat), Ok(Some(vec!["libHSx.dylib".to_string()])));
        // The same universal file with its image slice cut short is an error.
        let cut = &fat[..fat.len() - 10];
        assert_eq!(macho_loads(cut), Err(NotAssessed::UnparsedMachO));
    }
}
