//! Milestone 213 (issue #616) — kernel-side trace-noise filter classifier.
//!
//! Pure-Rust `no_std`-compatible classifier that matches file-open path
//! bytes against four filter categories (System, UserCache, Ephemeral,
//! CargoFingerprint). Called from `mikebom-ebpf/src/programs/file_ops.rs`
//! kprobes to drop noise events BEFORE they enter the FILE_EVENTS ring
//! buffer, freeing capacity for the actual rustc + linker events the
//! operator cares about.
//!
//! The classifier is shared between the kernel-side eBPF programs and
//! userspace test code by living in `mikebom-common` — same crate that
//! defines `FileEvent`. This lets us test the substring / prefix
//! matching logic exhaustively in `cargo test -p mikebom-common` without
//! needing a kernel + eBPF loader. The eBPF side calls
//! `path_matches_filter_category(&path, widen_system)` from
//! `try_do_filp_open` + `try_openat2` (m213 T012 + T013).
//!
//! ## Verifier safety (v2 — post-CI-88657233643-fix)
//!
//! The v1 implementation used byte-wise bounded loops (128 offsets × 16-
//! byte inner check per pattern). On kernel 5.15 this blew past the
//! verifier's 1M instruction budget (Processed 1000001 insns). v2 uses
//! the m211 whitelist recipe verbatim: patterns are stored as u64
//! words + mask, and the inner check is a single u64 XOR-and-mask.
//! Also caps Contains-scan at 64 offsets (was 128). Combined state cost
//! drops by ~16×.
//!
//! Per-open verifier cost estimate:
//! - Prefix compare: 6 patterns × 1 u64 XOR-and-mask = 6 ops (~40 insns)
//! - Contains scan:  5 patterns × 64 offsets × 1 u64 XOR-and-mask = 320
//!   ops (~2000 insns)
//! - Total: ~2050 verifier insns per open — comfortably under 1M budget.
//!
//! See specs/213-kernel-noise-filter/contracts/ebpf-verifier-notes.md
//! for the full recipe (Rules 1-5 all followed).

use crate::events::FilterCategoryTag;

/// The four filter categories (mirrors `FilterCategoryTag` variants).
/// Kept as internal u8 to match `FilterCategoryTag` discriminants
/// verbatim per contracts/filter-category-tag.md.
const CAT_SYSTEM: u8 = 0;
const CAT_USER_CACHE: u8 = 1;
const CAT_EPHEMERAL: u8 = 2;
const CAT_CARGO_FINGERPRINT: u8 = 3;

/// Maximum offset (exclusive) at which a `Contains` pattern is scanned
/// for. Paths longer than this within the 256-byte scratch buffer are
/// effectively skipped (FR-016 fail-open-on-truncated). Chosen to bound
/// the verifier's loop-state cost: 64 offsets × 8-byte u64 compare per
/// pattern is verifier-friendly on kernels 5.15+. v1's 128 blew the
/// budget.
const CONTAINS_SCAN_MAX_OFFSET: usize = 96;

/// One filter pattern. Compact `#[repr(C)]` layout — fits in 24 bytes.
/// The `word` field is the pattern's first 8 bytes packed as a little-
/// endian u64; longer patterns are truncated to their first 8 bytes.
/// The `mask` masks off unused byte positions in the compare (for
/// patterns shorter than 8 bytes; bytes past `len` become 0x00 in mask,
/// so XOR at those positions is ignored).
///
/// Precomputed at compile time via `pattern_from(bytes, len, ...)`
/// const fn — zero runtime cost to build the catalog.
#[derive(Copy, Clone)]
#[repr(C)]
struct FilterPattern {
    /// First 8 bytes of the pattern packed as little-endian u64. NUL-
    /// padded when pattern is < 8 bytes; truncated when > 8 bytes (only
    /// case in current catalog: `/.local/share/` at 14 → `/.local/`).
    word: u64,
    /// XOR mask. Set bits identify positions to include in the compare
    /// (`(path_word ^ pattern.word) & pattern.mask == 0` means match).
    /// For 8-byte patterns → `u64::MAX`. For 5-byte patterns like
    /// `/etc/` → `0x000000FFFFFFFFFF` (5 low bytes = 0xFF).
    mask: u64,
    /// `FilterCategoryTag` discriminant (0-3).
    category: u8,
    /// `true` = only match at path offset 0 (prefix). `false` = scan
    /// the first `CONTAINS_SCAN_MAX_OFFSET` bytes of the path.
    is_prefix: bool,
    _pad: [u8; 6],
}

/// Pack a byte-string prefix (up to 8 bytes) as a little-endian u64,
/// NUL-padded, then compute the compare mask for its effective length.
/// `bytes` MUST be ≥ 8 bytes long (zero-padded past the pattern's
/// effective length so the u64 read is well-defined).
const fn pack_pattern(bytes: &[u8; 8], len: u8, category: u8, is_prefix: bool) -> FilterPattern {
    // Pack little-endian.
    let word = (bytes[0] as u64)
        | ((bytes[1] as u64) << 8)
        | ((bytes[2] as u64) << 16)
        | ((bytes[3] as u64) << 24)
        | ((bytes[4] as u64) << 32)
        | ((bytes[5] as u64) << 40)
        | ((bytes[6] as u64) << 48)
        | ((bytes[7] as u64) << 56);
    // Mask: `len` low bytes are 0xFF, rest are 0x00.
    let mask = if len >= 8 {
        u64::MAX
    } else {
        // 1u64 << (len * 8) - 1 gives 2^(len*8) - 1 = a mask with `len*8` low bits set.
        (1u64 << (len as u32 * 8)) - 1
    };
    FilterPattern {
        word,
        mask,
        category,
        is_prefix,
        _pad: [0; 6],
    }
}

/// Pattern catalog. 19 entries: 10 prefix (System×8 + Ephemeral×2) +
/// 9 contains (UserCache×2 + CargoFingerprint×7).
///
/// Patterns longer than 8 bytes are TRUNCATED to their first 8 bytes
/// per verifier-cost constraint (see module docs). The truncated form
/// is still unique enough to avoid false positives for the target
/// filesystem structures — `/fingerp` is not a substring of any
/// non-cargo path we've observed, and `/.local/` catches
/// `/.local/share/` correctly.
///
/// Semantics per specs/213-kernel-noise-filter/spec.md FR-001..FR-004
/// **plus post-Colima-verification expansion** based on empirical
/// noise measurement on aarch64 6.8 (Colima) where rustc's library-
/// search fanout dominated over cargo fingerprint noise. Top observed
/// noise patterns from the SC-001 fixture run: `/usr/local/rustup/
/// toolchains/...` (474×), `/lib/aarch64-linux-gnu/lib*.so.*` (474×
/// each), `/bin/sh` (238×). Widening System to include `/lib/`,
/// `/usr/`, `/bin/` catches these — all three are OS-managed dirs
/// no build "produces" outputs into, so filtering them kernel-side
/// costs no signal.
///
/// - System (prefix): `/etc/`, `/proc/`, `/sys/`, `/dev/`, `/lib/`,
///   `/usr/`, `/bin/`
/// - UserCache (contains): `/.cache/`, `/.local/` (truncated from
///   `/.local/share/` — still specific enough)
/// - Ephemeral (prefix): `/tmp/`, `/var/tmp` (7 chars — falls through
///   to `/var/tmp/foo` matching)
/// - CargoFingerprint (contains): `/fingerp` (truncated from
///   `/fingerprint/`), `/deps/`, `/increme` (truncated from
///   `/incremental/`)
const PATTERNS: [FilterPattern; 19] = [
    // System — 8 prefix patterns (4 original + 3 post-Colima + 1 host-
    // noise addition for docker/systemd runtime dirs)
    pack_pattern(b"/etc/\0\0\0", 5, CAT_SYSTEM, true),
    pack_pattern(b"/proc/\0\0", 6, CAT_SYSTEM, true),
    pack_pattern(b"/sys/\0\0\0", 5, CAT_SYSTEM, true),
    pack_pattern(b"/dev/\0\0\0", 5, CAT_SYSTEM, true),
    pack_pattern(b"/lib/\0\0\0", 5, CAT_SYSTEM, true),
    pack_pattern(b"/usr/\0\0\0", 5, CAT_SYSTEM, true),
    pack_pattern(b"/bin/\0\0\0", 5, CAT_SYSTEM, true),
    // `/var/` (5 chars) — catches /var/lib/, /var/log/, /var/cache/,
    // /var/run/. Container-side docker log churn dominates traces on
    // Colima aarch64 6.8 (/var/lib/docker/containers/*/json.log seen
    // 4096× per test run). This prefix overlaps with `/var/tmp` below
    // but both would trigger a filter drop; iteration order picks
    // System first, so `/var/tmp/foo` will classify as System rather
    // than Ephemeral. Semantically equivalent (both filter).
    pack_pattern(b"/var/\0\0\0", 5, CAT_SYSTEM, true),
    // UserCache — 2 contains patterns (truncated to 8 bytes)
    pack_pattern(b"/.cache/", 8, CAT_USER_CACHE, false),
    pack_pattern(b"/.local/", 8, CAT_USER_CACHE, false),
    // Ephemeral — 2 prefix patterns.
    // `/var/tmp` kept for semantic completeness even though `/var/`
    // above will match first — if someone later removes `/var/` from
    // System, `/var/tmp` still catches the sub-case.
    pack_pattern(b"/tmp/\0\0\0", 5, CAT_EPHEMERAL, true),
    pack_pattern(b"/var/tmp", 8, CAT_EPHEMERAL, true),
    // CargoFingerprint — 6 contains patterns (truncated to 8 bytes each).
    // Original 3 (fingerprint / deps / incremental) + 3 post-Colima
    // additions to cover cargo's workspace-crawl bookkeeping observed
    // dominating Colima aarch64 6.8 traces (Cargo.toml/rust-toolchain/
    // .cargo/config probed 400+× per package):
    pack_pattern(b"/fingerp", 8, CAT_CARGO_FINGERPRINT, false),
    pack_pattern(b"/deps/\0\0", 6, CAT_CARGO_FINGERPRINT, false),
    pack_pattern(b"/increme", 8, CAT_CARGO_FINGERPRINT, false),
    // `Cargo.to` matches `Cargo.toml` (10 chars → truncate to 8) as
    // path substring. Cargo probes Cargo.toml at every workspace level
    // when resolving manifests.
    pack_pattern(b"Cargo.to", 8, CAT_CARGO_FINGERPRINT, false),
    // `-toolcha` matches `rust-toolchain` (14 chars → 8-byte tail
    // suffix). Distinctive enough to avoid false positives (no other
    // common tooling embeds `-toolcha`).
    pack_pattern(b"-toolcha", 8, CAT_CARGO_FINGERPRINT, false),
    // `.cargo/c` matches `.cargo/config` (13 chars → 8-byte prefix
    // starting at the leading `.`). Distinctive to cargo config lookup.
    pack_pattern(b".cargo/c", 8, CAT_CARGO_FINGERPRINT, false),
    // `Cargo.lo` matches `Cargo.lock` (10 chars → 8-byte prefix). Cargo
    // probes Cargo.lock ~1000× per SC-001 fixture run. Second-most
    // dominant cargo-metadata noise after Cargo.toml.
    pack_pattern(b"Cargo.lo", 8, CAT_CARGO_FINGERPRINT, false),
];

/// Read a u64 (little-endian, unaligned) from `path` at `offset`.
/// Caller MUST ensure `offset + 8 <= 256`.
#[inline(always)]
fn read_path_word(path: &[u8; 256], offset: usize) -> u64 {
    // SAFETY: caller-verified offset + 8 <= 256 (which holds when
    // offset < CONTAINS_SCAN_MAX_OFFSET (64) and for prefix at offset 0).
    // Matches m211's `read_unaligned` pattern at compiler_exec.rs
    // (proven working on kernels 5.15/6.1/6.6/6.8).
    unsafe { core::ptr::read_unaligned(path.as_ptr().add(offset) as *const u64) }
}

/// Classify a file-open path into one of the four filter categories,
/// or return `None` if no category matches.
///
/// When `widen_system` is `true`, the System category returns `None`
/// even on match — per m213 FR-010 the widening flag disables ONLY the
/// System category, leaving UserCache/Ephemeral/CargoFingerprint active.
///
/// v2 implementation (post-CI-verifier-limit-fix): all pattern compares
/// use word-wide u64 XOR-and-mask (~2050 verifier insns per open, down
/// from v1's byte-wise loops that blew the 1M budget on kernel 5.15).
pub fn path_matches_filter_category(path: &[u8; 256], widen_system: bool) -> Option<FilterCategoryTag> {
    // Post-Colima observation: rustup toolchain resolution via
    // `readdir_at` / `openat(dfd, relative_name)` produces file-open
    // events where the kernel-visible `name_ptr` in `struct filename`
    // is a RELATIVE dirent name (e.g., `1.88.0-aarch64-unknown-linux-gnu`
    // from a readdir over `/usr/local/rustup/toolchains/`). These
    // events flood the ring buffer (~12000× per SC-001 fixture run on
    // Colima aarch64 6.8) — dominating every non-relative event. Since
    // legitimate build inputs are always absolute paths (rustc reads
    // source files via absolute paths from cargo), any path whose
    // first byte is NOT `/` is by definition either (a) a relative
    // dirent name from readdir walks (System-namespace noise) or
    // (b) an anomalous edge case we don't care about. Classify as System.
    //
    // Widen-flag interaction: gated on `!widen_system` — operators
    // who opt into `--include-system-reads` are asking for MAXIMUM
    // visibility including these readdir dirents. Passing through
    // relative paths preserves the FR-010 semantic that the widen flag
    // fully disables the System category (including its dominant
    // sub-source, relative dirents).
    if !widen_system && path[0] != b'/' {
        return Some(FilterCategoryTag::System);
    }

    let mut i = 0usize;
    while i < PATTERNS.len() {
        let p = &PATTERNS[i];
        let matched = if p.is_prefix {
            // Prefix: read at offset 0 only.
            let path_word = read_path_word(path, 0);
            (path_word ^ p.word) & p.mask == 0
        } else {
            // Contains: bounded scan through first CONTAINS_SCAN_MAX_OFFSET
            // bytes. Read u64 at each offset; XOR against pattern word;
            // mask; compare to 0. Constant-bound loop keeps verifier happy.
            let mut offset = 0usize;
            let mut found = false;
            while offset < CONTAINS_SCAN_MAX_OFFSET {
                let path_word = read_path_word(path, offset);
                if (path_word ^ p.word) & p.mask == 0 {
                    found = true;
                    // Don't break early — the verifier prefers loops that
                    // run to completion. The `found` flag captures the
                    // hit; we exit the outer pattern loop after.
                }
                offset += 1;
            }
            found
        };
        if matched {
            // Widen-flag gate: System-category matches suppressed when
            // the operator opted into system-read visibility. Other 3
            // categories remain filtered per FR-010.
            if p.category == CAT_SYSTEM && widen_system {
                i += 1;
                continue;
            }
            return Some(match p.category {
                CAT_SYSTEM => FilterCategoryTag::System,
                CAT_USER_CACHE => FilterCategoryTag::UserCache,
                CAT_EPHEMERAL => FilterCategoryTag::Ephemeral,
                CAT_CARGO_FINGERPRINT => FilterCategoryTag::CargoFingerprint,
                // Unreachable per PATTERNS catalog; if this fires
                // someone added a new pattern without extending the map.
                _ => return None,
            });
        }
        i += 1;
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn to_path_buf(s: &str) -> [u8; 256] {
        let mut buf = [0u8; 256];
        let bytes = s.as_bytes();
        let n = core::cmp::min(bytes.len(), 256);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf
    }

    // --- T007: US1 unit tests --------------------------------------------

    #[test]
    fn t007_system_paths_classified() {
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/etc/hostname"), false),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/proc/self/status"), false),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/sys/kernel/debug/tracing"), false),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/dev/null"), false),
            Some(FilterCategoryTag::System)
        );
    }

    #[test]
    fn t007_user_cache_paths_classified() {
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/root/.cache/pip/wheels"), false),
            Some(FilterCategoryTag::UserCache)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/home/mike/.cache/npm/foo"), false),
            Some(FilterCategoryTag::UserCache)
        );
        // v2: `/.local/` (truncated from `/.local/share/`) still catches
        // the common home-dir XDG data paths.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/home/mike/.local/share/gem"), false),
            Some(FilterCategoryTag::UserCache)
        );
    }

    #[test]
    fn t007_ephemeral_paths_classified() {
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/tmp/rustc-xxx"), false),
            Some(FilterCategoryTag::Ephemeral)
        );
        // Post-Colima /var/ addition: /var/tmp/... now matches System
        // (which iterates first). Either category filters — semantically
        // equivalent. Test accepts either.
        let cat = path_matches_filter_category(&to_path_buf("/var/tmp/cache-abc"), false);
        assert!(matches!(cat, Some(FilterCategoryTag::System | FilterCategoryTag::Ephemeral)));
    }

    #[test]
    fn t007_cargo_fingerprint_paths_classified() {
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/root/mikebom/target/debug/build/foo-abc/fingerprint/dep-blah"),
                false
            ),
            Some(FilterCategoryTag::CargoFingerprint)
        );
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/home/dev/proj/target/release/deps/libfoo.rlib"),
                false
            ),
            Some(FilterCategoryTag::CargoFingerprint)
        );
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/home/dev/proj/target/debug/incremental/foo/abc"),
                false
            ),
            Some(FilterCategoryTag::CargoFingerprint)
        );
    }

    #[test]
    fn t007_non_matching_paths_return_none() {
        // FR-012: non-matching paths flow through unfiltered.
        // Note: post-Colima expansion, `/usr/`, `/lib/`, `/bin/` NOW
        // match System (they're read-only OS-managed dirs that no build
        // produces outputs into — filtering them costs no signal). The
        // non-matching examples below are project-source paths, /opt/
        // (custom install prefix outside System), and /home/ project
        // trees — all things a build actually reads AS INPUT.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/home/dev/proj/src/main.rs"), false),
            None
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/home/dev/proj/target/release/mikebom"), false),
            None
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/opt/myapp/config.toml"), false),
            None
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/mikebom/mikebom-cli/src/main.rs"), false),
            None
        );
    }

    #[test]
    fn t007_truncated_full_buffer_paths_return_none() {
        // FR-016: paths that fill the 256-byte buffer with no matching
        // pattern in the first 64 bytes are treated as unknown category
        // (fail-open). This test builds a 256-byte path of 'x' — no match.
        let mut buf = [b'x'; 256];
        buf[0] = b'/';
        assert_eq!(path_matches_filter_category(&buf, false), None);
    }

    #[test]
    fn t007_fingerprint_beyond_scan_window_missed() {
        // FR-016 corollary: pattern appearing past CONTAINS_SCAN_MAX_OFFSET
        // is NOT caught (fail-open on truncated).
        let padding = "a".repeat(CONTAINS_SCAN_MAX_OFFSET + 10);
        assert!(padding.len() >= CONTAINS_SCAN_MAX_OFFSET);
        let path = format!("/{}/fingerprint/dep-blah", padding);
        assert_eq!(path_matches_filter_category(&to_path_buf(&path), false), None);
    }

    // --- T026: US3 widen-flag unit tests --------------------------------

    #[test]
    fn t026_widen_flag_disables_system_only() {
        // FR-010: --include-system-reads disables System category ONLY.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/etc/hostname"), true),
            None
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/proc/self/status"), true),
            None
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/sys/kernel/debug/tracing"), true),
            None
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/dev/null"), true),
            None
        );

        // UserCache/Ephemeral/CargoFingerprint remain active.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/root/.cache/pip/wheels"), true),
            Some(FilterCategoryTag::UserCache)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/tmp/rustc-xxx"), true),
            Some(FilterCategoryTag::Ephemeral)
        );
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/root/mikebom/target/debug/build/foo/fingerprint/dep-x"),
                true
            ),
            Some(FilterCategoryTag::CargoFingerprint)
        );
    }

    // --- Pattern-catalog stability guards --------------------------------

    #[test]
    fn patterns_catalog_size_matches_declared_categories() {
        // Guards against future-you adding a category without adding
        // patterns for it. Post-Colima expansion: 8 System + 2 UserCache
        // + 2 Ephemeral + 6 CargoFingerprint = 18.
        assert_eq!(PATTERNS.len(), 19);
    }

    #[test]
    fn t007_relative_paths_classified_as_system() {
        // Post-Colima: readdir-produced relative dirent names (no
        // leading /) dominate the fixture's noise (~12000 events per
        // trace). Non-absolute paths ARE noise by definition — no
        // build reads source via relative dirent names.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("1.88.0-aarch64-unknown-linux-gnu"), false),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("README.md"), false),
            Some(FilterCategoryTag::System)
        );
        // FR-010: widen flag DOES unblock relative-path filtering —
        // operators wanting max visibility get to see readdir dirents too.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("1.88.0-aarch64-unknown-linux-gnu"), true),
            None
        );
    }

    #[test]
    fn t007_var_prefix_catches_docker_and_systemd_noise() {
        // Post-Colima: docker log churn (/var/lib/docker/containers/*/
        // json.log) was seen 4096× per trace. `/var/` prefix catches
        // this AND /var/log/, /var/cache/, /var/run/. Sub-`/var/tmp/`
        // is still Ephemeral but System iterates first.
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/var/lib/docker/containers/abc/abc-json.log"),
                false
            ),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/var/log/syslog"), false),
            Some(FilterCategoryTag::System)
        );
        // /var/tmp/foo — matches System (since /var/ is first) OR
        // Ephemeral (if System removed). Either way filter drops.
        let cat = path_matches_filter_category(&to_path_buf("/var/tmp/foo"), false);
        assert!(matches!(cat, Some(FilterCategoryTag::System | FilterCategoryTag::Ephemeral)));
    }

    #[test]
    fn t007_cargo_workspace_crawl_paths_classified() {
        // Post-Colima expansion — cargo's workspace-crawl behavior on
        // real builds probes these paths 400+× per package.
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/mikebom/mikebom-cli/tests/fixtures/foo/Cargo.toml"),
                false
            ),
            Some(FilterCategoryTag::CargoFingerprint)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/rust-toolchain"), false),
            Some(FilterCategoryTag::CargoFingerprint)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/rust-toolchain.toml"), false),
            Some(FilterCategoryTag::CargoFingerprint)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/mikebom/.cargo/config"), false),
            Some(FilterCategoryTag::CargoFingerprint)
        );
    }

    #[test]
    fn t007_expanded_system_paths_classified() {
        // Post-Colima expansion — top rustc noise on Linux hosts.
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/lib/aarch64-linux-gnu/libc.so.6"), false),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(
                &to_path_buf("/usr/local/rustup/toolchains/1.88.0/lib/librustc_driver.so"),
                false
            ),
            Some(FilterCategoryTag::System)
        );
        assert_eq!(
            path_matches_filter_category(&to_path_buf("/bin/sh"), false),
            Some(FilterCategoryTag::System)
        );
    }

    #[test]
    fn all_patterns_have_valid_category_discriminants() {
        for p in &PATTERNS {
            assert!(
                (p.category as usize) < FilterCategoryTag::ALL.len(),
                "pattern has out-of-range category discriminant: {}",
                p.category
            );
        }
    }

    #[test]
    fn pack_pattern_produces_correct_mask() {
        // Verifies the const-fn packing logic — critical because the
        // catalog is entirely compile-time-built and a mask bug would
        // silently produce wrong classifications.
        let p1 = pack_pattern(b"/etc/\0\0\0", 5, CAT_SYSTEM, true);
        assert_eq!(p1.mask, 0x000000FFFFFFFFFF);
        let p2 = pack_pattern(b"/.cache/", 8, CAT_USER_CACHE, false);
        assert_eq!(p2.mask, u64::MAX);
        let p3 = pack_pattern(b"/tmp/\0\0\0", 5, CAT_EPHEMERAL, true);
        assert_eq!(p3.mask, 0x000000FFFFFFFFFF);
    }

    #[test]
    fn pack_pattern_produces_correct_word() {
        // /etc/ + NULs → little-endian u64 = 0x00_00_00_2F_63_74_65_2F
        let p = pack_pattern(b"/etc/\0\0\0", 5, CAT_SYSTEM, true);
        let expected = u64::from_le_bytes(*b"/etc/\0\0\0");
        assert_eq!(p.word, expected);
    }
}
