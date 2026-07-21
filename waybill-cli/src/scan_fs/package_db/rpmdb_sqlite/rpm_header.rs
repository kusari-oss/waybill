//! Selective RPM header-blob tag reader.
#![allow(dead_code)] // wire-shape variants for the RPM header decoder.
//!
//! RPM headers ship as a flat binary record inside rpmdb.sqlite's
//! Packages.blob column. On-wire format:
//! - 16-byte intro: magic `\x8E\xAD\xE8\x01` + 4 reserved + u32be
//!   `nindex` + u32be `hsize`
//! - N index entries (16 bytes each): u32be tag + u32be type + u32be
//!   offset-into-store + u32be count
//! - data store: `hsize` bytes, accessed by offset
//!
//! We extract only the tags waybill needs (package metadata + file
//! lists). The upstream `rpm = 0.22` crate parses full `.rpm` files
//! (lead + signature + header), but its header-only `Header::parse`
//! is `pub(crate)` and unreachable, so this selective reader lives
//! here.
//!
//! Defensive budgets reject oversized headers (>64K entries, >16 MiB
//! store) up front. All offset/length reads bounds-check.

use std::path::PathBuf;

/// Header magic (offset 0..4). Every valid header starts with these
/// four bytes.
pub const HEADER_MAGIC: [u8; 4] = [0x8e, 0xad, 0xe8, 0x01];

/// Real RHEL packages have under ~8K entries; 64K is generous
/// headroom for the pathological case.
const MAX_INDEX_ENTRIES: u32 = 65_535;

/// Real RHEL data stores top out around ~5 MiB (packages with huge
/// file lists). 16 MiB is defense-in-depth.
const MAX_DATA_STORE_SIZE: u32 = 16 * 1024 * 1024;

const INTRO_SIZE: usize = 16;
const INDEX_ENTRY_SIZE: usize = 16;

// --- RPM tag constants (see /usr/include/rpm/rpmtag.h) ---

pub const TAG_NAME: u32 = 1000;
pub const TAG_VERSION: u32 = 1001;
pub const TAG_RELEASE: u32 = 1002;
pub const TAG_EPOCH: u32 = 1003;
/// Organization responsible for the rpm build (e.g. `Fedora Project`,
/// `CentOS`, `Rocky Enterprise Software Foundation`). Maps to
/// `component.supplier.name` when present. See `rpm_file.rs` for the
/// matching .rpm-file-level extraction.
pub const TAG_VENDOR: u32 = 1006;
/// Individual or team who built this specific rpm. Populated on
/// user-rebuilt rpms and some distro builds; the vendor→packager
/// fallback chain lives in `rpm.rs::build_entry_from_header`.
pub const TAG_PACKAGER: u32 = 1007;
pub const TAG_LICENSE: u32 = 1014;
pub const TAG_ARCH: u32 = 1022;
pub const TAG_REQUIRENAME: u32 = 1049;
pub const TAG_DIRINDEXES: u32 = 1116;
pub const TAG_BASENAMES: u32 = 1117;
pub const TAG_DIRNAMES: u32 = 1118;
/// Per-file content digests, parallel-indexed with `BASENAMES`. Each
/// entry is a hex-encoded digest string. The algorithm is named by
/// [`TAG_FILEDIGESTALGO`]; when that tag is absent, MD5 is the
/// rpm-spec-defined default. Populated by `rpmbuild` at package
/// creation; waybill uses it for the milestone-041 cross-ref on
/// per-file evidence.
pub const TAG_FILEDIGESTS: u32 = 1035;
/// IANA hash-algorithm code for [`TAG_FILEDIGESTS`]. Common values:
/// `1`=MD5, `2`=SHA-1, `8`=SHA-256, `9`=SHA-384, `10`=SHA-512. Absent
/// or `0` defaults to MD5 per the rpm spec (legacy behavior).
pub const TAG_FILEDIGESTALGO: u32 = 5011;

// --- RPM type codes (subset we handle) ---

const TYPE_INT32: u32 = 4;
const TYPE_STRING: u32 = 6;
const TYPE_STRING_ARRAY: u32 = 8;
const TYPE_I18NSTRING: u32 = 9;

/// Errors the header reader can surface. Callers typically turn these
/// into a single WARN log + zero entries; a malformed blob never
/// aborts a scan.
#[derive(Debug, thiserror::Error)]
pub enum HeaderError {
    #[error("blob too short ({len} bytes) to contain an RPM header")]
    TooShort { len: usize },

    #[error("blob does not start with RPM header magic")]
    BadMagic,

    #[error("header declares too many index entries ({count} > cap {cap})")]
    TooManyEntries { count: u32, cap: u32 },

    #[error("header declares oversized data store ({size} bytes > cap {cap})")]
    DataStoreTooLarge { size: u32, cap: u32 },

    #[error("blob truncated — needs {declared} bytes but only {actual} available")]
    Truncated { declared: usize, actual: usize },
}

#[derive(Debug, Clone, Copy)]
struct IndexEntry {
    tag: u32,
    tag_type: u32,
    offset: u32,
    count: u32,
}

/// A parsed RPM header. Clones the caller's store bytes so accessors
/// can return `&str` slices without a lifetime dependency on the input.
#[derive(Debug, Clone)]
pub struct RpmHeader {
    store: Vec<u8>,
    entries: Vec<IndexEntry>,
}

impl RpmHeader {
    /// Parse a header blob.
    ///
    /// Accepts two on-wire shapes:
    /// 1. **Full** — 16-byte intro: `magic(4) + reserved(4) + nindex(4) + hsize(4)`.
    ///    This is what `.rpm` files on disk carry and what `rpm
    ///    --querytag` prints.
    /// 2. **Stripped / immutable-region** — 8-byte intro:
    ///    `nindex(4) + hsize(4)`. This is how rpmdb.sqlite's
    ///    `Packages.blob` column stores headers (the magic+reserved
    ///    is stripped on insert and re-added on query by
    ///    `headerImport` / `headerExport`).
    ///
    /// Auto-detects which form is present by checking for the magic
    /// signature. Validates declared sizes against defensive budgets
    /// before any allocation.
    pub fn parse(blob: &[u8]) -> Result<Self, HeaderError> {
        if blob.len() < 8 {
            return Err(HeaderError::TooShort { len: blob.len() });
        }
        // Detect format: full (magic-prefixed) vs stripped.
        let (nindex, hsize, intro_end) = if blob.len() >= INTRO_SIZE
            && blob[..4] == HEADER_MAGIC
        {
            (
                u32::from_be_bytes([blob[8], blob[9], blob[10], blob[11]]),
                u32::from_be_bytes([blob[12], blob[13], blob[14], blob[15]]),
                INTRO_SIZE,
            )
        } else {
            // Stripped form: no magic, intro is just nindex(4) + hsize(4).
            (
                u32::from_be_bytes([blob[0], blob[1], blob[2], blob[3]]),
                u32::from_be_bytes([blob[4], blob[5], blob[6], blob[7]]),
                8,
            )
        };
        if nindex > MAX_INDEX_ENTRIES {
            return Err(HeaderError::TooManyEntries {
                count: nindex,
                cap: MAX_INDEX_ENTRIES,
            });
        }
        if hsize > MAX_DATA_STORE_SIZE {
            return Err(HeaderError::DataStoreTooLarge {
                size: hsize,
                cap: MAX_DATA_STORE_SIZE,
            });
        }

        let index_bytes = (nindex as usize)
            .checked_mul(INDEX_ENTRY_SIZE)
            .ok_or(HeaderError::TooManyEntries {
                count: nindex,
                cap: MAX_INDEX_ENTRIES,
            })?;
        let index_end = intro_end
            .checked_add(index_bytes)
            .ok_or(HeaderError::TooManyEntries {
                count: nindex,
                cap: MAX_INDEX_ENTRIES,
            })?;
        let store_end =
            index_end
                .checked_add(hsize as usize)
                .ok_or(HeaderError::DataStoreTooLarge {
                    size: hsize,
                    cap: MAX_DATA_STORE_SIZE,
                })?;
        if blob.len() < store_end {
            return Err(HeaderError::Truncated {
                declared: store_end,
                actual: blob.len(),
            });
        }

        let mut entries = Vec::with_capacity(nindex as usize);
        let mut cursor = intro_end;
        for _ in 0..nindex {
            let tag = u32::from_be_bytes([
                blob[cursor],
                blob[cursor + 1],
                blob[cursor + 2],
                blob[cursor + 3],
            ]);
            let tag_type = u32::from_be_bytes([
                blob[cursor + 4],
                blob[cursor + 5],
                blob[cursor + 6],
                blob[cursor + 7],
            ]);
            let offset = u32::from_be_bytes([
                blob[cursor + 8],
                blob[cursor + 9],
                blob[cursor + 10],
                blob[cursor + 11],
            ]);
            let count = u32::from_be_bytes([
                blob[cursor + 12],
                blob[cursor + 13],
                blob[cursor + 14],
                blob[cursor + 15],
            ]);
            entries.push(IndexEntry {
                tag,
                tag_type,
                offset,
                count,
            });
            cursor += INDEX_ENTRY_SIZE;
        }

        let store = blob[index_end..store_end].to_vec();
        Ok(RpmHeader { store, entries })
    }

    fn find_entry(&self, tag: u32) -> Option<&IndexEntry> {
        self.entries.iter().find(|e| e.tag == tag)
    }

    /// Decode a STRING (type 6) tag, or the first element of an
    /// I18NSTRING (type 9). Returns `None` if the tag is missing, the
    /// type doesn't match, or the payload is malformed.
    pub fn string(&self, tag: u32) -> Option<&str> {
        let e = self.find_entry(tag)?;
        if e.tag_type != TYPE_STRING && e.tag_type != TYPE_I18NSTRING {
            return None;
        }
        let start = e.offset as usize;
        if start >= self.store.len() {
            return None;
        }
        let rel_nul = self.store[start..].iter().position(|&b| b == 0)?;
        std::str::from_utf8(&self.store[start..start + rel_nul]).ok()
    }

    /// Decode a STRING_ARRAY (type 8) or I18NSTRING (type 9) tag as
    /// `count` NUL-terminated elements.
    pub fn string_array(&self, tag: u32) -> Option<Vec<&str>> {
        let e = self.find_entry(tag)?;
        if e.tag_type != TYPE_STRING_ARRAY && e.tag_type != TYPE_I18NSTRING {
            return None;
        }
        let mut out = Vec::with_capacity(e.count as usize);
        let mut pos = e.offset as usize;
        for _ in 0..e.count {
            if pos >= self.store.len() {
                return None;
            }
            let rel_nul = self.store[pos..].iter().position(|&b| b == 0)?;
            let s = std::str::from_utf8(&self.store[pos..pos + rel_nul]).ok()?;
            out.push(s);
            pos += rel_nul + 1;
        }
        Some(out)
    }

    /// Decode an INT32 array (type 4) as `count` big-endian u32s.
    pub fn int32_array(&self, tag: u32) -> Option<Vec<u32>> {
        let e = self.find_entry(tag)?;
        if e.tag_type != TYPE_INT32 {
            return None;
        }
        let len_bytes = (e.count as usize).checked_mul(4)?;
        let start = e.offset as usize;
        let end = start.checked_add(len_bytes)?;
        if end > self.store.len() {
            return None;
        }
        let mut out = Vec::with_capacity(e.count as usize);
        for i in 0..e.count as usize {
            let off = start + i * 4;
            out.push(u32::from_be_bytes([
                self.store[off],
                self.store[off + 1],
                self.store[off + 2],
                self.store[off + 3],
            ]));
        }
        Some(out)
    }

    /// Read the per-file FILEDIGESTS values + their algorithm.
    /// Returns `None` when FILEDIGESTS is absent, when the
    /// FILEDIGESTALGO is set to a code waybill doesn't recognize
    /// (defensive — defer to a follow-on rather than emit a
    /// mis-prefixed cross-ref), or when the digest array is empty.
    ///
    /// Per the rpm spec, FILEDIGESTALGO absent or `0` means MD5
    /// (legacy default); we honor that. Algorithm codes other than
    /// the standard `{1, 2, 8, 9, 10}` set return `None` with a
    /// debug-log breadcrumb.
    pub fn file_digests(&self) -> Option<RpmFileDigests<'_>> {
        let values = self.string_array(TAG_FILEDIGESTS)?;
        if values.is_empty() {
            return None;
        }
        let algo_code: u32 = self
            .int32_array(TAG_FILEDIGESTALGO)
            .and_then(|v| v.first().copied())
            .unwrap_or(0);
        let algo = match algo_code {
            0 | 1 => RpmDigestAlgo::Md5,
            2 => RpmDigestAlgo::Sha1,
            8 => RpmDigestAlgo::Sha256,
            9 => RpmDigestAlgo::Sha384,
            10 => RpmDigestAlgo::Sha512,
            other => {
                tracing::debug!(
                    code = other,
                    "rpm FILEDIGESTALGO is an unknown IANA hash code; \
                     omitting rpm_filedigest cross-ref for this package"
                );
                return None;
            }
        };
        Some(RpmFileDigests { algo, values })
    }

    /// Reconstruct the owned file paths from the
    /// BASENAMES/DIRNAMES/DIRINDEXES triple. Returns an empty vec for
    /// metapackages (no files) or if any tag is missing; individual
    /// out-of-range dirindexes are skipped rather than failing the
    /// whole package.
    pub fn file_paths(&self) -> Vec<PathBuf> {
        let Some(basenames) = self.string_array(TAG_BASENAMES) else {
            return Vec::new();
        };
        let Some(dirnames) = self.string_array(TAG_DIRNAMES) else {
            return Vec::new();
        };
        let Some(dirindexes) = self.int32_array(TAG_DIRINDEXES) else {
            return Vec::new();
        };
        let n = basenames.len().min(dirindexes.len());
        let mut paths = Vec::with_capacity(n);
        for i in 0..n {
            let idx = dirindexes[i] as usize;
            if idx >= dirnames.len() {
                continue;
            }
            let mut p = String::with_capacity(dirnames[idx].len() + basenames[i].len());
            p.push_str(dirnames[idx]);
            p.push_str(basenames[i]);
            paths.push(PathBuf::from(p));
        }
        paths
    }
}

/// Per-file digests + algorithm extracted from a HeaderBlob's
/// FILEDIGESTS / FILEDIGESTALGO tags. Borrows the digest strings
/// from the underlying `RpmHeader` to avoid cloning during the
/// per-package walk; consumers that need owned data clone at
/// emission time.
pub struct RpmFileDigests<'a> {
    /// Hash algorithm rpm used to compute the digests. Encoded as
    /// the IANA hash-algorithm-name slug for emission (e.g.
    /// `"sha256"`, `"md5"`).
    pub algo: RpmDigestAlgo,
    /// Hex-encoded per-file digest values, parallel-indexed with
    /// `BASENAMES`. Each entry's length matches `algo.hex_len()`
    /// when populated; empty entries (non-regular files) and
    /// shorter-than-expected ones are filtered at emission time.
    pub values: Vec<&'a str>,
}

/// Hash-algorithm name for the rpm FILEDIGESTS tag, decoded from
/// the IANA hash-algorithm registry code carried in
/// FILEDIGESTALGO. Set is restricted to the 5 algorithms rpm
/// actually uses today; unknown codes fall through to `None` at
/// the call site rather than getting a `RpmDigestAlgo::Other`
/// variant (defensive against mis-prefixing the cross-ref).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpmDigestAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl RpmDigestAlgo {
    /// IANA-spec lowercase name. Used as the algorithm prefix in
    /// the wire form `<algo>:<hex>`.
    pub fn name(&self) -> &'static str {
        match self {
            RpmDigestAlgo::Md5 => "md5",
            RpmDigestAlgo::Sha1 => "sha1",
            RpmDigestAlgo::Sha256 => "sha256",
            RpmDigestAlgo::Sha384 => "sha384",
            RpmDigestAlgo::Sha512 => "sha512",
        }
    }

    /// Expected hex-string length for this algorithm. Used to
    /// filter out empty or truncated FILEDIGESTS entries (rpm
    /// records empty strings for non-regular files like devices /
    /// fifos).
    pub fn hex_len(&self) -> usize {
        match self {
            RpmDigestAlgo::Md5 => 32,
            RpmDigestAlgo::Sha1 => 40,
            RpmDigestAlgo::Sha256 => 64,
            RpmDigestAlgo::Sha384 => 96,
            RpmDigestAlgo::Sha512 => 128,
        }
    }
}

/// Convenience wrapper matching the module's public API.
pub fn parse_header_blob(blob: &[u8]) -> Result<RpmHeader, HeaderError> {
    RpmHeader::parse(blob)
}

/// Re-exported test helper so `rpm.rs` can craft production-shaped
/// blob rows in its own tests without duplicating the builder.
#[cfg(test)]
pub(crate) use tests::build_test_header;

#[cfg(test)]
pub(crate) use tests::TagValue;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Test helper — build a header blob byte-for-byte. Tags are laid
    /// out in the order provided; offsets into the store are computed
    /// sequentially.
    pub(crate) enum TagValue<'a> {
        Str(&'a str),
        StrArray(&'a [&'a str]),
        I18nStr(&'a [&'a str]),
        Int32Array(&'a [u32]),
    }

    pub(crate) fn build_test_header(tags: &[(u32, TagValue<'_>)]) -> Vec<u8> {
        // First pass: serialise each tag's data payload and remember
        // its offset + count + type code.
        let mut store: Vec<u8> = Vec::new();
        let mut index: Vec<(u32, u32, u32, u32)> = Vec::new(); // tag, type, offset, count
        for (tag, value) in tags {
            let offset = store.len() as u32;
            match value {
                TagValue::Str(s) => {
                    store.extend_from_slice(s.as_bytes());
                    store.push(0);
                    index.push((*tag, TYPE_STRING, offset, 1));
                }
                TagValue::StrArray(items) => {
                    for it in *items {
                        store.extend_from_slice(it.as_bytes());
                        store.push(0);
                    }
                    index.push((*tag, TYPE_STRING_ARRAY, offset, items.len() as u32));
                }
                TagValue::I18nStr(items) => {
                    for it in *items {
                        store.extend_from_slice(it.as_bytes());
                        store.push(0);
                    }
                    index.push((*tag, TYPE_I18NSTRING, offset, items.len() as u32));
                }
                TagValue::Int32Array(values) => {
                    for v in *values {
                        store.extend_from_slice(&v.to_be_bytes());
                    }
                    index.push((*tag, TYPE_INT32, offset, values.len() as u32));
                }
            }
        }

        let nindex = index.len() as u32;
        let hsize = store.len() as u32;
        let mut out = Vec::with_capacity(INTRO_SIZE + index.len() * INDEX_ENTRY_SIZE + store.len());
        out.extend_from_slice(&HEADER_MAGIC);
        out.extend_from_slice(&[0u8; 4]); // reserved
        out.extend_from_slice(&nindex.to_be_bytes());
        out.extend_from_slice(&hsize.to_be_bytes());
        for (tag, tag_type, offset, count) in index {
            out.extend_from_slice(&tag.to_be_bytes());
            out.extend_from_slice(&tag_type.to_be_bytes());
            out.extend_from_slice(&offset.to_be_bytes());
            out.extend_from_slice(&count.to_be_bytes());
        }
        out.extend_from_slice(&store);
        out
    }

    #[test]
    fn parses_package_metadata() {
        let blob = build_test_header(&[
            (TAG_NAME, TagValue::Str("bash")),
            (TAG_VERSION, TagValue::Str("5.2.15")),
            (TAG_RELEASE, TagValue::Str("1.fc40")),
            (TAG_ARCH, TagValue::Str("x86_64")),
            (TAG_LICENSE, TagValue::I18nStr(&["GPL-3.0-or-later"])),
            (TAG_EPOCH, TagValue::Int32Array(&[0])),
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert_eq!(h.string(TAG_NAME), Some("bash"));
        assert_eq!(h.string(TAG_VERSION), Some("5.2.15"));
        assert_eq!(h.string(TAG_RELEASE), Some("1.fc40"));
        assert_eq!(h.string(TAG_ARCH), Some("x86_64"));
        assert_eq!(h.string(TAG_LICENSE), Some("GPL-3.0-or-later"));
        assert_eq!(h.int32_array(TAG_EPOCH), Some(vec![0]));
    }

    #[test]
    fn reconstructs_file_paths() {
        let blob = build_test_header(&[
            (
                TAG_DIRNAMES,
                TagValue::StrArray(&["/usr/bin/", "/usr/lib/"]),
            ),
            (
                TAG_BASENAMES,
                TagValue::StrArray(&["bash", "libfoo.so"]),
            ),
            (TAG_DIRINDEXES, TagValue::Int32Array(&[0, 1])),
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert_eq!(
            h.file_paths(),
            vec![
                PathBuf::from("/usr/bin/bash"),
                PathBuf::from("/usr/lib/libfoo.so"),
            ]
        );
    }

    #[test]
    fn dirindex_values_beyond_127_decode_correctly() {
        // Guard against big-endian regressions — this index value byte-
        // swaps incorrectly under little-endian reads.
        let blob = build_test_header(&[
            (TAG_DIRNAMES, {
                // Build an array big enough that index 200 is in-range,
                // then reference index 200 from dirindexes. We need 201
                // dirnames; make them distinct so we can assert the
                // right one was picked.
                // Static leak is impossible here — use the owned-vec path.
                let v: Vec<String> = (0..201).map(|i| format!("/dir{i}/")).collect();
                let v_leaked: &'static Vec<String> = Box::leak(Box::new(v));
                let refs: &'static Vec<&'static str> = Box::leak(Box::new(
                    v_leaked.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                ));
                TagValue::StrArray(refs.as_slice())
            }),
            (TAG_BASENAMES, TagValue::StrArray(&["target.bin"])),
            (TAG_DIRINDEXES, TagValue::Int32Array(&[200])),
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert_eq!(h.file_paths(), vec![PathBuf::from("/dir200/target.bin")]);
    }

    #[test]
    fn empty_file_list_is_not_an_error() {
        let blob = build_test_header(&[(TAG_NAME, TagValue::Str("metapackage"))]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert!(h.file_paths().is_empty());
    }

    #[test]
    fn accepts_stripped_immutable_region_form() {
        // rpmdb.sqlite stores headers without the magic+reserved
        // prefix — just `nindex(4) + hsize(4) + entries + store`.
        // Build a synthetic stripped blob: 1 entry (NAME="bash"), 5
        // store bytes.
        let mut blob = Vec::new();
        blob.extend_from_slice(&1u32.to_be_bytes()); // nindex
        blob.extend_from_slice(&5u32.to_be_bytes()); // hsize
        // One index entry: NAME, STRING, offset=0, count=1
        blob.extend_from_slice(&TAG_NAME.to_be_bytes());
        blob.extend_from_slice(&TYPE_STRING.to_be_bytes());
        blob.extend_from_slice(&0u32.to_be_bytes());
        blob.extend_from_slice(&1u32.to_be_bytes());
        // Store: "bash\0"
        blob.extend_from_slice(b"bash\0");
        let h = RpmHeader::parse(&blob).unwrap();
        assert_eq!(h.string(TAG_NAME), Some("bash"));
    }

    #[test]
    fn too_short_rejected() {
        // Minimum is now 8 bytes (stripped intro). 4 bytes is too short.
        let blob = [0x8e, 0xad, 0xe8, 0x01];
        assert!(matches!(
            RpmHeader::parse(&blob),
            Err(HeaderError::TooShort { .. })
        ));
    }

    #[test]
    fn oversized_entry_count_rejected() {
        let mut blob = Vec::with_capacity(16);
        blob.extend_from_slice(&HEADER_MAGIC);
        blob.extend_from_slice(&[0u8; 4]); // reserved
        blob.extend_from_slice(&100_000u32.to_be_bytes()); // nindex
        blob.extend_from_slice(&0u32.to_be_bytes()); // hsize
        assert!(matches!(
            RpmHeader::parse(&blob),
            Err(HeaderError::TooManyEntries { .. })
        ));
    }

    #[test]
    fn oversized_data_store_rejected() {
        let mut blob = Vec::with_capacity(16);
        blob.extend_from_slice(&HEADER_MAGIC);
        blob.extend_from_slice(&[0u8; 4]); // reserved
        blob.extend_from_slice(&0u32.to_be_bytes()); // nindex
        blob.extend_from_slice(&(50 * 1024 * 1024u32).to_be_bytes()); // 50 MiB hsize
        assert!(matches!(
            RpmHeader::parse(&blob),
            Err(HeaderError::DataStoreTooLarge { .. })
        ));
    }

    #[test]
    fn truncated_blob_rejected() {
        // Declare 1 index entry + 100 bytes of store, but provide no
        // payload bytes after the intro.
        let mut blob = Vec::with_capacity(16);
        blob.extend_from_slice(&HEADER_MAGIC);
        blob.extend_from_slice(&[0u8; 4]);
        blob.extend_from_slice(&1u32.to_be_bytes());
        blob.extend_from_slice(&100u32.to_be_bytes());
        assert!(matches!(
            RpmHeader::parse(&blob),
            Err(HeaderError::Truncated { .. })
        ));
    }

    #[test]
    fn missing_tag_returns_none() {
        let blob = build_test_header(&[(TAG_NAME, TagValue::Str("bash"))]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert_eq!(h.string(TAG_VERSION), None);
        assert_eq!(h.int32_array(TAG_EPOCH), None);
        assert_eq!(h.string_array(TAG_BASENAMES), None);
    }

    #[test]
    fn dirindex_out_of_range_skipped_per_file() {
        let blob = build_test_header(&[
            (TAG_DIRNAMES, TagValue::StrArray(&["/usr/bin/"])),
            (TAG_BASENAMES, TagValue::StrArray(&["good", "bad"])),
            (TAG_DIRINDEXES, TagValue::Int32Array(&[0, 99])),
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        // "good" resolves, "bad" is skipped.
        assert_eq!(h.file_paths(), vec![PathBuf::from("/usr/bin/good")]);
    }

    // ---- Milestone 041: FILEDIGESTS / FILEDIGESTALGO --------------------

    #[test]
    fn file_digests_decodes_sha256_payload() {
        let aaa = "a".repeat(64);
        let bbb = "b".repeat(64);
        let blob = build_test_header(&[
            (TAG_FILEDIGESTS, TagValue::StrArray(&[&aaa, &bbb])),
            (TAG_FILEDIGESTALGO, TagValue::Int32Array(&[8])),
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        let fd = h.file_digests().expect("FILEDIGESTS present");
        assert_eq!(fd.algo, RpmDigestAlgo::Sha256);
        assert_eq!(fd.values.len(), 2);
        assert_eq!(fd.values[0], aaa);
        assert_eq!(fd.values[1], bbb);
    }

    #[test]
    fn file_digests_defaults_to_md5_when_algo_absent() {
        let md5 = "0".repeat(32);
        let blob = build_test_header(&[
            (TAG_FILEDIGESTS, TagValue::StrArray(&[&md5])),
            // No FILEDIGESTALGO — must default to MD5.
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        let fd = h.file_digests().expect("FILEDIGESTS present");
        assert_eq!(fd.algo, RpmDigestAlgo::Md5);
    }

    #[test]
    fn file_digests_defaults_to_md5_when_algo_zero() {
        let md5 = "0".repeat(32);
        let blob = build_test_header(&[
            (TAG_FILEDIGESTS, TagValue::StrArray(&[&md5])),
            (TAG_FILEDIGESTALGO, TagValue::Int32Array(&[0])),
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        let fd = h.file_digests().expect("FILEDIGESTS present");
        assert_eq!(fd.algo, RpmDigestAlgo::Md5);
    }

    #[test]
    fn file_digests_returns_none_when_filedigests_absent() {
        let blob = build_test_header(&[
            (TAG_NAME, TagValue::Str("bash")),
            // No FILEDIGESTS.
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert!(h.file_digests().is_none());
    }

    #[test]
    fn file_digests_returns_none_for_unknown_algo() {
        let blob = build_test_header(&[
            (TAG_FILEDIGESTS, TagValue::StrArray(&["abc"])),
            (TAG_FILEDIGESTALGO, TagValue::Int32Array(&[99])), // unknown
        ]);
        let h = RpmHeader::parse(&blob).unwrap();
        assert!(
            h.file_digests().is_none(),
            "unknown FILEDIGESTALGO must reject (defensive against mis-prefixing)"
        );
    }

    #[test]
    fn rpm_digest_algo_names_match_iana_lowercase() {
        assert_eq!(RpmDigestAlgo::Md5.name(), "md5");
        assert_eq!(RpmDigestAlgo::Sha1.name(), "sha1");
        assert_eq!(RpmDigestAlgo::Sha256.name(), "sha256");
        assert_eq!(RpmDigestAlgo::Sha384.name(), "sha384");
        assert_eq!(RpmDigestAlgo::Sha512.name(), "sha512");
    }

    #[test]
    fn rpm_digest_algo_hex_lengths_are_correct() {
        assert_eq!(RpmDigestAlgo::Md5.hex_len(), 32);
        assert_eq!(RpmDigestAlgo::Sha1.hex_len(), 40);
        assert_eq!(RpmDigestAlgo::Sha256.hex_len(), 64);
        assert_eq!(RpmDigestAlgo::Sha384.hex_len(), 96);
        assert_eq!(RpmDigestAlgo::Sha512.hex_len(), 128);
    }
}
