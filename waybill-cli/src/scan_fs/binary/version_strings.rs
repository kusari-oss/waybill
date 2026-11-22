//! Curated embedded-version-string scanner. Per research R6 / FR-025.
//! Patterns for well-known self-identifying libraries:
//!
//! | Library   | Signature                                                    | Boundary |
//! |-----------|--------------------------------------------------------------|----------|
//! | OpenSSL   | `OpenSSL X.Y.Z[letter] [DD MMM YYYY]`                        | NUL-only |
//! | BoringSSL | `BoringSSL <40-char-sha>`                                    | NUL-only |
//! | zlib      | ` deflate X.Y.Z[letter] Copyright ...` (one leading space)  | anchored |
//! | SQLite    | `SQLite version X.Y.Z[.W]`                                   | NUL-only |
//! | curl      | `libcurl/X.Y.Z`                                              | NUL-only |
//! | PCRE      | `PCRE X.Y YYYY-MM-DD`                                        | NUL-only |
//! | PCRE2     | `PCRE2 X.Y YYYY-MM-DD`                                       | NUL-only |
//! | GnuTLS    | `GnuTLS X.Y.Z`                                               | NUL-only |
//! | LibreSSL  | `LibreSSL X.Y.Z`                                             | NUL-only |
//! | LLVM      | `LLVM version X.Y.Z`                                         | NUL-only |
//! | OpenJDK   | `OpenJDK X.Y.Z[+B]` (modern) or `OpenJDK 8uX[-bY]` (legacy)  | NUL-only |
//! | PHP       | `X-Powered-By: PHP/X.Y.Z` (#1258)                            | NUL-only |
//! | Node.js   | `node.js/vX.Y.Z` (the compiled-in default User-Agent)        | NUL-only |
//! | expat     | `expat_X.Y.Z` (#1526)                                        | anchored |
//! | ncurses   | `ncurses X.Y.YYYYMMDD`, emitted as `X.Y` (#1526)             | anchored |
//!
//! **Boundary** (spec 1643, #1568, #1641). A NUL-only pattern must start a
//! string: the byte before it is NUL, or it starts the region. An anchored
//! pattern is fixed at both ends (its name starts it; a fixed suffix or a
//! required NUL ends it), so any preceding byte but `[A-Za-z0-9_]` is
//! accepted. The read-only sections mix strings with other constant data,
//! so a real banner can follow any byte. For zlib the test applies to the
//! byte before its leading space; its form without the space keeps the NUL
//! rule. See [`CuratedLibrary::boundary`].
//!
//! Scanning runs ONLY against format-appropriate read-only string sections (ELF `.rodata` + `.data.rel.ro`, Mach-O `__TEXT,__cstring` + `__TEXT,__const`, PE `.rdata`) — never against the full binary image (Q4 resolution / FR-025). This bounds the false-positive surface. Control-set validation per SC-005.
//
// TODO(milestone-026.x): three additional libraries deferred from
// milestone 026 because they don't have clean self-identifying
// strings in the read-only string region:
//   - glibc:  `GLIBC_X.Y` lives in `.gnu.version_r` (symbol-version
//             table), not `.rodata`. Needs a separate ELF-section
//             reader rather than the curated string scanner.
//   - musl:   typically doesn't self-identify in compiled output.
//             Research milestone needed to find a reliable signature
//             (or conclude there isn't one and document the gap).
//   - V8:     version strings buried in stack-trace formatting code;
//             tend to be non-deterministic across builds. May
//             require an inline-data scan rather than a string scan.
//             NOTE: the `Node` pattern below reads the Node.js runtime's
//             OWN version, not the V8 it embeds. A node binary matching
//             `node` says nothing about its V8, which stays deferred.
// Tracking: `docs/architecture/licenses.md` sbomqs deferred list.

/// One match from the curated scanner. Converted to a
/// `PackageDbEntry` with `pkg:generic/<library>@<version>` and
/// `waybill:evidence-kind = "embedded-version-string"` +
/// `waybill:confidence = "heuristic"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedVersionMatch {
    pub library: CuratedLibrary,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// `EnumIter` is test-only (#1246): the whole-set checks over this enum
// must not be able to miss a variant added after they were written, and
// a hand-written list of variants always can — that is the defect
// #1246 was filed against. `strum::iter()` is derived from the enum
// itself, so there is no list to forget.
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum CuratedLibrary {
    OpenSsl,
    BoringSsl,
    Zlib,
    Sqlite,
    Curl,
    Pcre,
    Pcre2,
    // Milestone 026 — easy-4 cohort.
    GnuTls,
    LibreSsl,
    Llvm,
    OpenJdk,
    // #1258 — the PHP interpreter. Unlike every entry above it this is a
    // runtime rather than a linked library, and it is here for the same
    // reason: the official `php:*` images compile it into `/usr/local`,
    // so no OS package reader claims it and the binary walker had nothing
    // to version it by.
    Php,
    // Issue #1244 — the Node.js runtime, which official `node:*`
    // images install outside any package manager.
    Node,
    // #1526 — libraries commonly bundled as private copies.
    Expat,
    Ncurses,
    // Spec 1644 (#1641) — libraries whose version is a bare string with no
    // name beside it. Each is versioned only through a second anchor in the
    // same binary ([`Boundary::WholeString`], [`anchored_verdicts`]).
    Bzip2,
    Gmp,
    Liblzma,
}

impl CuratedLibrary {
    pub fn slug(self) -> &'static str {
        match self {
            CuratedLibrary::OpenSsl => "openssl",
            CuratedLibrary::BoringSsl => "boringssl",
            CuratedLibrary::Zlib => "zlib",
            CuratedLibrary::Sqlite => "sqlite",
            CuratedLibrary::Curl => "curl",
            CuratedLibrary::Pcre => "pcre",
            CuratedLibrary::Pcre2 => "pcre2",
            CuratedLibrary::GnuTls => "gnutls",
            CuratedLibrary::LibreSsl => "libressl",
            CuratedLibrary::Llvm => "llvm",
            CuratedLibrary::OpenJdk => "openjdk",
            CuratedLibrary::Php => "php",
            CuratedLibrary::Node => "node",
            CuratedLibrary::Expat => "expat",
            CuratedLibrary::Ncurses => "ncurses",
            CuratedLibrary::Bzip2 => "bzip2",
            CuratedLibrary::Gmp => "gmp",
            CuratedLibrary::Liblzma => "liblzma",
        }
    }
}

/// Scan a read-only string region for curated version signatures.
/// Dedupes matches by (library, version) within a single binary.
pub fn scan(region: &[u8]) -> Vec<EmbeddedVersionMatch> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<(CuratedLibrary, String)> =
        std::collections::HashSet::new();

    // Iterate every position; the per-prefix check handles its own
    // length requirements. Using `windows` would drop positions near
    // the end of the region where fewer than 16 bytes remain.
    for i in 0..region.len() {
        if let Some(m) = match_prefix(region, i) {
            if seen.insert((m.library, m.version.clone())) {
                out.push(m);
            }
        }
    }
    out
}

/// The curated match at `pos`, if the boundary rule accepts it.
///
/// The byte before `pos` alone decides which patterns could be accepted, so
/// most positions are rejected before any prefix is compared: after NUL (or
/// at the region's start) every pattern is tried; after an identifier byte
/// none can be; after any other byte only the anchored ones, plus zlib when
/// the byte is the space its literal begins with. Trying every prefix at
/// every byte made `scan` 18 times slower (spec 1643 SC-006,
/// `measurements/therock.md`). The result equals `pattern_at` followed by
/// `new_rule_accepts`, which the census uses and a test compares.
fn match_prefix(region: &[u8], pos: usize) -> Option<EmbeddedVersionMatch> {
    let m = match pos.checked_sub(1).map(|i| region[i]) {
        None | Some(0) => pattern_at(region, pos, |_| true)?,
        Some(prev) if is_identifier(prev) => return None,
        Some(prev) => pattern_at(region, pos, |lib| {
            lib.boundary() == Boundary::Anchored || (prev == b' ' && lib == CuratedLibrary::Zlib)
        })?,
    };
    new_rule_accepts(m.library, region, pos).then_some(m)
}

/// Which bytes may precede a curated pattern's first character (spec 1643
/// FR-001). The byte test exists to keep a library name inside unrelated
/// text from matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// Both ends of the match are fixed: the library's name starts it and
    /// a fixed suffix or a required NUL ends it. Any preceding byte but an
    /// identifier character is accepted (FR-002).
    Anchored,
    /// The end of the match is not fixed, so only a NUL (or the start of
    /// the region) may precede it (FR-003).
    NulOnly,
    /// Spec 1644: the version is a whole NUL-bounded string with no name in
    /// it, so it identifies nothing alone. Such a library never matches
    /// through the prefix scan; [`anchored_verdicts`] versions it only when
    /// the binary also holds the library's anchor.
    WholeString,
}

impl Boundary {
    /// Whether a match starting at `pos` may be preceded by what precedes it.
    pub fn accepts(self, region: &[u8], pos: usize) -> bool {
        let Some(&prev) = pos.checked_sub(1).and_then(|i| region.get(i)) else {
            return true;
        };
        match self {
            Boundary::Anchored => !is_identifier(prev),
            Boundary::NulOnly | Boundary::WholeString => prev == 0,
        }
    }
}

/// `[A-Za-z0-9_]`: a byte that would make the match part of a longer name.
fn is_identifier(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

impl CuratedLibrary {
    /// The pattern's boundary class (spec 1643 FR-001). Exhaustive with no
    /// wildcard arm, so a new variant cannot compile until it is classified
    /// (FR-004).
    pub fn boundary(self) -> Boundary {
        match self {
            CuratedLibrary::Zlib | CuratedLibrary::Expat | CuratedLibrary::Ncurses => Boundary::Anchored,
            CuratedLibrary::OpenSsl
            | CuratedLibrary::BoringSsl
            | CuratedLibrary::Sqlite
            | CuratedLibrary::Curl
            | CuratedLibrary::Pcre
            | CuratedLibrary::Pcre2
            | CuratedLibrary::GnuTls
            | CuratedLibrary::LibreSsl
            | CuratedLibrary::Llvm
            | CuratedLibrary::OpenJdk
            | CuratedLibrary::Php
            | CuratedLibrary::Node => Boundary::NulOnly,
            CuratedLibrary::Bzip2 | CuratedLibrary::Gmp | CuratedLibrary::Liblzma => Boundary::WholeString,
        }
    }
}

/// The rule before spec 1643, kept frozen for the census (research R5):
/// every pattern needs a NUL (or the region's start) before it; zlib may
/// instead have one leading space with a NUL before that (#1525).
fn main_rule_accepts(library: CuratedLibrary, region: &[u8], pos: usize) -> bool {
    let at_boundary = Boundary::NulOnly.accepts(region, pos);
    match library {
        CuratedLibrary::Zlib => {
            at_boundary || (pos >= 1 && region[pos - 1] == b' ' && Boundary::NulOnly.accepts(region, pos - 1))
        }
        _ => at_boundary,
    }
}

/// Spec 1643's rule (FR-002, FR-003). zlib's literal is `" deflate …"`, so
/// for it the class applies to the byte before the leading space; its form
/// without the space keeps the NUL rule.
fn new_rule_accepts(library: CuratedLibrary, region: &[u8], pos: usize) -> bool {
    let class = library.boundary();
    match library {
        CuratedLibrary::Zlib => {
            Boundary::NulOnly.accepts(region, pos)
                || (pos >= 1 && region[pos - 1] == b' ' && class.accepts(region, pos - 1))
        }
        _ => class.accepts(region, pos),
    }
}

/// The curated pattern among those `want` admits whose prefix starts at
/// `pos`, with its parsed version, whatever precedes it. For zlib, `pos` is
/// at `deflate `. No two prefixes can both match at one position (`PCRE2 `
/// is tried before `PCRE `, which it does not start with).
fn pattern_at(
    region: &[u8],
    pos: usize,
    want: impl Fn(CuratedLibrary) -> bool,
) -> Option<EmbeddedVersionMatch> {
    let window = &region[pos..];

    // zlib — "deflate ". Its literal is " deflate X.Y.Z Copyright ..." (#1525).
    if want(CuratedLibrary::Zlib) && window.starts_with(b"deflate ") {
        let tail = &region[pos + 8..];
        if let Some(v) = parse_zlib_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Zlib,
                version: v,
            });
        }
    }

    // expat — "expat_X.Y.Z", a whole string (#1526).
    if let Some(tail) = window.strip_prefix(b"expat_").filter(|_| want(CuratedLibrary::Expat)) {
        if let Some(v) = parse_semver_triple(tail).filter(|v| tail.get(v.len()) == Some(&0)) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Expat,
                version: v,
            });
        }
    }
    // ncurses — "ncurses X.Y.YYYYMMDD", a whole string (#1526). The
    // version is the release, X.Y; the patch date is dropped.
    if let Some(tail) = window.strip_prefix(b"ncurses ").filter(|_| want(CuratedLibrary::Ncurses)) {
        if let Some(v) = parse_ncurses_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Ncurses,
                version: v,
            });
        }
    }

    // OpenSSL — "OpenSSL "
    if want(CuratedLibrary::OpenSsl) && window.starts_with(b"OpenSSL ") {
        let tail = &region[pos + 8..];
        if let Some(v) = parse_openssl_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::OpenSsl,
                version: v,
            });
        }
    }
    // BoringSSL — "BoringSSL "
    if want(CuratedLibrary::BoringSsl) && window.starts_with(b"BoringSSL ") {
        let tail = &region[pos + 10..];
        if let Some(sha) = parse_git_sha(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::BoringSsl,
                version: sha,
            });
        }
    }
    // SQLite — "SQLite version "
    if want(CuratedLibrary::Sqlite) && region.len() >= pos + 15 && &region[pos..pos + 15] == b"SQLite version " {
        let tail = &region[pos + 15..];
        if let Some(v) = parse_sqlite_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Sqlite,
                version: v,
            });
        }
    }
    // curl — "libcurl/"
    if want(CuratedLibrary::Curl) && window.starts_with(b"libcurl/") {
        let tail = &region[pos + 8..];
        if let Some(v) = parse_semver_triple(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Curl,
                version: v,
            });
        }
    }
    // PCRE2 (check first — PCRE prefix is a subset)
    if want(CuratedLibrary::Pcre2) && window.starts_with(b"PCRE2 ") {
        let tail = &region[pos + 6..];
        if let Some(v) = parse_pcre_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Pcre2,
                version: v,
            });
        }
    }
    // PCRE
    if want(CuratedLibrary::Pcre) && window.starts_with(b"PCRE ") {
        let tail = &region[pos + 5..];
        if let Some(v) = parse_pcre_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Pcre,
                version: v,
            });
        }
    }
    // GnuTLS — "GnuTLS " (milestone 026).
    if want(CuratedLibrary::GnuTls) && window.starts_with(b"GnuTLS ") {
        let tail = &region[pos + 7..];
        if let Some(v) = parse_semver_triple(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::GnuTls,
                version: v,
            });
        }
    }
    // LibreSSL — "LibreSSL " (milestone 026).
    if want(CuratedLibrary::LibreSsl) && window.starts_with(b"LibreSSL ") {
        let tail = &region[pos + 9..];
        if let Some(v) = parse_semver_triple(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::LibreSsl,
                version: v,
            });
        }
    }
    // LLVM — "LLVM version " (milestone 026). Strict prefix; bare
    // "LLVM " is too noisy (matches "LLVM ERROR:", "LLVM IR ...").
    if want(CuratedLibrary::Llvm) && window.starts_with(b"LLVM version ") {
        let tail = &region[pos + 13..];
        if let Some(v) = parse_semver_triple(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Llvm,
                version: v,
            });
        }
    }
    // OpenJDK — "OpenJDK " (milestone 026). Two-scheme parser handles
    // both modern JEP 322 (X.Y.Z+B) and legacy Java-8 (8uXXX-bXX).
    if want(CuratedLibrary::OpenJdk) && window.starts_with(b"OpenJDK ") {
        let tail = &region[pos + 8..];
        if let Some(v) = parse_openjdk_version(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::OpenJdk,
                version: v,
            });
        }
    }
    // PHP — `X-Powered-By: PHP/` (#1258). PHP's compiled-in response
    // header, and the whole prefix is the anchor rather than just `PHP/`:
    // a bare `8.3.35` occurs twice more in the same binary, and `PHP ` on
    // its own appears in diagnostics and INI comments. Measured on
    // `php:8.3.35-fpm` and `php:8.3.35-fpm-alpine`, glibc and musl alike,
    // this exact string occurs once per binary in `/usr/local/bin/php`
    // and `/usr/local/sbin/php-fpm`, NUL-delimited in `.rodata`.
    if want(CuratedLibrary::Php) && window.starts_with(b"X-Powered-By: PHP/") {
        let tail = &region[pos + 18..];
        if let Some(v) = parse_semver_triple(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Php,
                version: v,
            });
        }
    }
    // Node.js — "node.js/v" (issue #1244). This is Node's compiled-in
    // default HTTP User-Agent, which is why it carries the `node.js/`
    // context that a bare `v24.14.1` lacks; the module's contract is
    // that a version matches only behind a self-identifying prefix.
    //
    // Measured in node 24.14.1 (official linux-x64 ELF, `.rodata`) and
    // node 22.23.1 (Mach-O arm64): exactly one distinct `node.js/v<ver>`
    // per binary, every occurrence NUL-preceded, so the NUL rule holds.
    // One of the two occurrences is NOT NUL-terminated (it abuts the
    // next literal), which `parse_semver_triple` handles by stopping at
    // the first byte that is neither a digit nor a dot.
    //
    // A nightly build's `node.js/v25.0.0-nightly…` yields the triple
    // `25.0.0` and drops the prerelease tag — the same lossy-but-correct
    // behaviour the other semver-triple patterns have.
    if want(CuratedLibrary::Node) && window.starts_with(b"node.js/v") {
        let tail = &region[pos + 9..];
        if let Some(v) = parse_semver_triple(tail) {
            return Some(EmbeddedVersionMatch {
                library: CuratedLibrary::Node,
                version: v,
            });
        }
    }

    None
}

/// Spec 1644 FR-003 (research R3): the symbols that anchor liblzma. Both
/// are in liblzma's `XZ_5.0` node, and both return its version, so a binary
/// that *defines* one is liblzma. A binary that imports one only links it.
pub const ANCHOR_SYMBOLS: &[&str] = &["lzma_version_string", "lzma_version_number"];

/// Evidence naming a [`Boundary::WholeString`] library without its version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnchorKind {
    /// Text anywhere in the read-only region, as a substring (FR-002).
    Text(&'static str),
    /// A defined symbol from [`ANCHOR_SYMBOLS`] (FR-003).
    Symbol,
}

impl CuratedLibrary {
    /// The library's anchor; `None` for every self-naming pattern.
    /// Exhaustive with no wildcard arm (FR-009).
    fn anchor(self) -> Option<AnchorKind> {
        match self {
            CuratedLibrary::Bzip2 => Some(AnchorKind::Text("bzip2/libbzip2")),
            CuratedLibrary::Gmp => Some(AnchorKind::Text("GNU MP:")),
            CuratedLibrary::Liblzma => Some(AnchorKind::Symbol),
            CuratedLibrary::OpenSsl
            | CuratedLibrary::BoringSsl
            | CuratedLibrary::Zlib
            | CuratedLibrary::Sqlite
            | CuratedLibrary::Curl
            | CuratedLibrary::Pcre
            | CuratedLibrary::Pcre2
            | CuratedLibrary::GnuTls
            | CuratedLibrary::LibreSsl
            | CuratedLibrary::Llvm
            | CuratedLibrary::OpenJdk
            | CuratedLibrary::Php
            | CuratedLibrary::Node
            | CuratedLibrary::Expat
            | CuratedLibrary::Ncurses => None,
        }
    }

    /// The version `s` states, when `s` is a whole string of this library's
    /// candidate shape (FR-004); `None` for every self-naming pattern.
    fn candidate_version(self, s: &[u8]) -> Option<String> {
        match self {
            CuratedLibrary::Bzip2 => parse_bzip2_candidate(s),
            CuratedLibrary::Gmp => parse_dotted_triple(s).filter(|t| t.len() == s.len()).map(str::to_string),
            CuratedLibrary::Liblzma => {
                let t = parse_dotted_triple(s)?;
                matches!(&s[t.len()..], b"" | b"alpha" | b"beta").then(|| String::from_utf8_lossy(s).into_owned())
            }
            CuratedLibrary::OpenSsl
            | CuratedLibrary::BoringSsl
            | CuratedLibrary::Zlib
            | CuratedLibrary::Sqlite
            | CuratedLibrary::Curl
            | CuratedLibrary::Pcre
            | CuratedLibrary::Pcre2
            | CuratedLibrary::GnuTls
            | CuratedLibrary::LibreSsl
            | CuratedLibrary::Llvm
            | CuratedLibrary::OpenJdk
            | CuratedLibrary::Php
            | CuratedLibrary::Node
            | CuratedLibrary::Expat
            | CuratedLibrary::Ncurses => None,
        }
    }
}

/// The libraries [`CuratedLibrary::anchor`] names. A test checks this list
/// against every variant whose class is [`Boundary::WholeString`].
const WHOLE_STRING_LIBRARIES: [CuratedLibrary; 3] =
    [CuratedLibrary::Bzip2, CuratedLibrary::Gmp, CuratedLibrary::Liblzma];

/// `X.Y.Z` at the start of `s`, each part 1-4 ASCII digits, returned as the
/// matched prefix. What follows it is the caller's to judge.
fn parse_dotted_triple(s: &[u8]) -> Option<&str> {
    let mut end = 0;
    for part in 0..3 {
        if part > 0 {
            if s.get(end) != Some(&b'.') {
                return None;
            }
            end += 1;
        }
        let digits = s[end..].iter().take_while(|b| b.is_ascii_digit()).count();
        if !(1..=4).contains(&digits) {
            return None;
        }
        end += digits;
    }
    std::str::from_utf8(&s[..end]).ok()
}

/// bzip2's `BZ_VERSION`, `X.Y.Z, DD-Mon-YYYY` (research R2), as the whole
/// string; the version is `X.Y.Z`.
fn parse_bzip2_candidate(s: &[u8]) -> Option<String> {
    const MONTHS: [&[u8]; 12] =
        [b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec"];
    let version = parse_dotted_triple(s)?;
    let rest = s[version.len()..].strip_prefix(b", ")?;
    let day = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if !(1..=2).contains(&day) {
        return None;
    }
    let rest = rest[day..].strip_prefix(b"-")?;
    let (month, rest) = (rest.get(..3)?, rest.get(3..)?);
    if !MONTHS.contains(&month) {
        return None;
    }
    let year = rest.strip_prefix(b"-")?;
    (year.len() == 4 && year.iter().all(u8::is_ascii_digit)).then(|| version.to_string())
}

/// What anchored a library in one binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    Text { value: &'static str, offset: usize },
    Symbol { value: String },
}

/// A whole string of a library's candidate shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub string: String,
    pub version: String,
    pub offset: usize,
    /// What precedes it: `start`, `nul` or `binary` (a byte that is not
    /// printable text; spec 1644 FR-004 as amended by the census).
    pub preceding: &'static str,
}

/// Printable text: `0x20`-`0x7E`, tab, LF and CR.
fn is_text(b: u8) -> bool {
    (0x20..=0x7e).contains(&b) || matches!(b, b'\t' | b'\n' | b'\r')
}

/// Every whole string a candidate may be (FR-004, amended): the printable
/// tail of each NUL-terminated segment, with its offset and what precedes
/// it. The tail is the whole segment when NUL (or the region's start)
/// precedes it; otherwise it starts just after the segment's last byte that
/// is not text. GMP's `gmp_version` sits after a binary table in `.rodata`
/// (`0xff`, then `6.3.0\0`; `measurements/census.md`), so NUL alone is too
/// strict. A version inside text (`requires 6.3.0 or later`) is still never
/// a whole string: its tail is the sentence. An unterminated tail at the
/// region's end is not a string.
fn whole_strings(region: &[u8]) -> impl Iterator<Item = (usize, &[u8], &'static str)> {
    let mut start = 0;
    memchr::memchr_iter(0, region).filter_map(move |nul| {
        let seg_start = start;
        start = nul + 1;
        let seg = &region[seg_start..nul];
        let k = seg.iter().rposition(|b| !is_text(*b)).map_or(0, |i| i + 1);
        let preceding = if k > 0 {
            "binary"
        } else if seg_start == 0 {
            "start"
        } else {
            "nul"
        };
        (k < seg.len()).then(|| (seg_start + k, &seg[k..], preceding))
    })
}

/// Spec 1644's verdict for one anchored library in one binary (data-model).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Versioned(String),
    Ambiguous,
    AmbiguousShared,
    NoCandidate,
}

impl Verdict {
    fn label(&self) -> &'static str {
        match self {
            Verdict::Versioned(_) => "versioned",
            Verdict::Ambiguous => "ambiguous",
            Verdict::AmbiguousShared => "ambiguous-shared",
            Verdict::NoCandidate => "no-candidate",
        }
    }
}

/// One anchored library's evidence and verdict in one binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchoredVerdict {
    pub library: CuratedLibrary,
    pub anchors: Vec<Anchor>,
    /// Candidates left after FR-005a's removal.
    pub candidates: Vec<Candidate>,
    /// Candidates removed because another library anchored in the same
    /// binary also accepts them (FR-005a), with that library.
    pub shared: Vec<(Candidate, CuratedLibrary)>,
    pub verdict: Verdict,
}

impl AnchoredVerdict {
    /// The `waybill::version_census` `anchored` event (contracts/wire.md).
    pub fn to_json(&self) -> String {
        let cand = |c: &Candidate| serde_json::json!({"value": c.string, "offset": c.offset, "preceding": c.preceding});
        let mut j = serde_json::json!({
            "library": self.library.slug(),
            "anchors": self.anchors.iter().map(|a| match a {
                Anchor::Text { value, offset } => serde_json::json!({"kind": "text", "value": value, "offset": offset}),
                Anchor::Symbol { value } => serde_json::json!({"kind": "symbol", "value": value}),
            }).collect::<Vec<_>>(),
            "candidates": self.candidates.iter().map(cand).collect::<Vec<_>>(),
            "shared": self.shared.iter().map(|(c, with)| {
                let mut v = cand(c);
                v["with"] = serde_json::json!(with.slug());
                v
            }).collect::<Vec<_>>(),
            "verdict": self.verdict.label(),
        });
        if let Verdict::Versioned(v) = &self.verdict {
            j["version"] = serde_json::json!(v);
        }
        j.to_string()
    }
}

/// Spec 1644: every [`Boundary::WholeString`] library whose anchor is in
/// this binary, with its candidates and verdict. `anchor_symbols` holds the
/// binary's *defined* names from [`ANCHOR_SYMBOLS`].
///
/// A binary with no anchor costs one substring search per text anchor and
/// returns nothing. Only an anchored binary has its region split into
/// whole strings. A candidate is a NUL-terminated string that starts the
/// region or follows a NUL (FR-004); one another anchored library also
/// accepts versions neither (FR-005a); a library left with two or more
/// versions gets none (FR-005).
pub fn anchored_verdicts(region: &[u8], anchor_symbols: &[String]) -> Vec<AnchoredVerdict> {
    let mut found: Vec<(CuratedLibrary, Vec<Anchor>)> = Vec::new();
    for lib in WHOLE_STRING_LIBRARIES {
        let anchors: Vec<Anchor> = match lib.anchor() {
            Some(AnchorKind::Text(needle)) => memchr::memmem::find(region, needle.as_bytes())
                .map(|offset| Anchor::Text { value: needle, offset })
                .into_iter()
                .collect(),
            Some(AnchorKind::Symbol) => ANCHOR_SYMBOLS
                .iter()
                .filter(|s| anchor_symbols.iter().any(|d| d == *s))
                .map(|s| Anchor::Symbol { value: (*s).to_string() })
                .collect(),
            None => Vec::new(),
        };
        if !anchors.is_empty() {
            found.push((lib, anchors));
        }
    }
    if found.is_empty() {
        return Vec::new();
    }

    // Every candidate of every anchored library, deduplicated by string.
    let mut cands: Vec<Vec<Candidate>> = vec![Vec::new(); found.len()];
    for (offset, s, preceding) in whole_strings(region) {
        for (i, (lib, _)) in found.iter().enumerate() {
            if let Some(version) = lib.candidate_version(s) {
                let string = String::from_utf8_lossy(s).into_owned();
                if !cands[i].iter().any(|c| c.string == string) {
                    cands[i].push(Candidate { string, version, offset, preceding });
                }
            }
        }
    }

    let mut out = Vec::new();
    for (i, (lib, anchors)) in found.iter().enumerate() {
        let mut candidates = Vec::new();
        let mut shared = Vec::new();
        for c in &cands[i] {
            let other = found
                .iter()
                .enumerate()
                .find(|(j, _)| *j != i && cands[*j].iter().any(|o| o.string == c.string))
                .map(|(_, (l, _))| *l);
            match other {
                Some(with) => shared.push((c.clone(), with)),
                None => candidates.push(c.clone()),
            }
        }
        let mut versions: Vec<&str> = candidates.iter().map(|c| c.version.as_str()).collect();
        versions.sort_unstable();
        versions.dedup();
        let verdict = match versions.as_slice() {
            [v] => Verdict::Versioned((*v).to_string()),
            [] if !shared.is_empty() => Verdict::AmbiguousShared,
            [] => Verdict::NoCandidate,
            _ => Verdict::Ambiguous,
        };
        out.push(AnchoredVerdict { library: *lib, anchors: anchors.clone(), candidates, shared, verdict });
    }
    out
}

/// Spec 1644 census only (FR-001, FR-006): bzip2 date-form strings in a
/// binary that lacks bzip2's anchor. They are never emitted; the census
/// counts them to test whether the form could stand alone. GMP's and xz's
/// bare `X.Y.Z` is not reported this way: it occurs in unrelated binaries
/// too often to say anything.
pub fn bzip2_form_without_anchor(region: &[u8]) -> Vec<Candidate> {
    let Some(AnchorKind::Text(anchor)) = CuratedLibrary::Bzip2.anchor() else { return Vec::new() };
    if memchr::memmem::find(region, anchor.as_bytes()).is_some() {
        return Vec::new();
    }
    let mut out: Vec<Candidate> = Vec::new();
    for (offset, s, preceding) in whole_strings(region) {
        if let Some(version) = CuratedLibrary::Bzip2.candidate_version(s) {
            let string = String::from_utf8_lossy(s).into_owned();
            if !out.iter().any(|c| c.string == string) {
                out.push(Candidate { string, version, offset, preceding });
            }
        }
    }
    out
}

/// One occurrence of a curated pattern, found with no boundary test
/// (spec 1643 research R5). Emitted as a `waybill::version_census` debug
/// event; it never reaches the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CensusHit {
    pub library: CuratedLibrary,
    pub version: String,
    /// Offset of the literal's first byte in the region (zlib's leading
    /// space, when it has one).
    pub offset: usize,
    /// `start`, `nul`, `whitespace`, `other_non_identifier` or `identifier`.
    pub preceding: &'static str,
    pub accepted_main: bool,
    pub accepted_new: bool,
    /// Up to 24 bytes before the literal, and up to 64 from its start.
    pub context_before: Vec<u8>,
    pub context_after: Vec<u8>,
}

impl CensusHit {
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "library": self.library.slug(),
            "version": self.version,
            "offset": self.offset,
            "preceding": self.preceding,
            "accepted_main": self.accepted_main,
            "accepted_new": self.accepted_new,
            "context_before": data_encoding::HEXLOWER.encode(&self.context_before),
            "context_after": data_encoding::HEXLOWER.encode(&self.context_after),
        })
        .to_string()
    }
}

/// Every curated pattern occurrence in `region`, whatever precedes it, with
/// the preceding byte's class and both rules' verdicts.
pub fn census(region: &[u8]) -> Vec<CensusHit> {
    let mut out = Vec::new();
    for pos in 0..region.len() {
        let Some(m) = pattern_at(region, pos, |_| true) else { continue };
        let start = if m.library == CuratedLibrary::Zlib && pos >= 1 && region[pos - 1] == b' ' {
            pos - 1
        } else {
            pos
        };
        let preceding = match start.checked_sub(1).map(|i| region[i]) {
            None => "start",
            Some(0) => "nul",
            Some(b' ' | b'\t' | b'\r' | b'\n') => "whitespace",
            Some(b) if is_identifier(b) => "identifier",
            Some(_) => "other_non_identifier",
        };
        out.push(CensusHit {
            accepted_main: main_rule_accepts(m.library, region, pos),
            accepted_new: new_rule_accepts(m.library, region, pos),
            library: m.library,
            version: m.version,
            offset: start,
            preceding,
            context_before: region[start.saturating_sub(24)..start].to_vec(),
            context_after: region[start..(start + 64).min(region.len())].to_vec(),
        });
    }
    out
}

/// OpenSSL version: N.N.N with optional lowercase letter suffix
/// (e.g. `3.0.11`, `1.1.1w`). Must be followed by space or NUL.
fn parse_openssl_version(tail: &[u8]) -> Option<String> {
    let mut end = 0;
    let mut dots = 0;
    while end < tail.len() {
        let b = tail[end];
        if b.is_ascii_digit() {
            end += 1;
        } else if b == b'.' {
            dots += 1;
            if dots > 2 {
                break;
            }
            end += 1;
        } else {
            break;
        }
    }
    // Optional single lowercase letter suffix.
    if end < tail.len() && tail[end].is_ascii_lowercase() && tail[end] != b'r' {
        // Avoid matching `OpenSSL r` from strings like "OpenSSL reasons"
        end += 1;
    }
    if dots != 2 || end == 0 {
        return None;
    }
    // Require a non-version-char immediately after (space, NUL, or similar).
    let terminator_ok = match tail.get(end) {
        Some(&c) => !c.is_ascii_alphanumeric() && c != b'.',
        None => true,
    };
    if !terminator_ok {
        return None;
    }
    std::str::from_utf8(&tail[..end]).ok().map(str::to_string)
}

/// zlib signature: `deflate X.Y.Z[letter] Copyright`. Must be
/// followed by ` Copyright` to qualify — avoids matching `deflate
/// X.Y.Z` that appears in a panic-message help string.
fn parse_zlib_version(tail: &[u8]) -> Option<String> {
    let v = parse_openssl_version(tail)?; // same shape
    // Confirm " Copyright" follows.
    let after = &tail[v.len()..];
    if after.starts_with(b" Copyright") {
        Some(v)
    } else {
        None
    }
}

/// SQLite version: `X.Y.Z` with optional `.W` fourth component.
fn parse_sqlite_version(tail: &[u8]) -> Option<String> {
    let mut end = 0;
    let mut dots = 0;
    while end < tail.len() {
        let b = tail[end];
        if b.is_ascii_digit() {
            end += 1;
        } else if b == b'.' {
            dots += 1;
            if dots > 3 {
                break;
            }
            end += 1;
        } else {
            break;
        }
    }
    if dots < 2 || end == 0 {
        return None;
    }
    let terminator_ok = matches!(tail.get(end), None | Some(&0) | Some(&b' '));
    if !terminator_ok {
        return None;
    }
    std::str::from_utf8(&tail[..end]).ok().map(str::to_string)
}

/// Semver triple `X.Y.Z` with no letter suffix.
fn parse_semver_triple(tail: &[u8]) -> Option<String> {
    let mut end = 0;
    let mut dots = 0;
    while end < tail.len() {
        let b = tail[end];
        if b.is_ascii_digit() {
            end += 1;
        } else if b == b'.' {
            dots += 1;
            if dots > 2 {
                break;
            }
            end += 1;
        } else {
            break;
        }
    }
    if dots != 2 || end == 0 {
        return None;
    }
    std::str::from_utf8(&tail[..end]).ok().map(str::to_string)
}

/// ncurses: `X.Y` then an optional `.YYYYMMDD` patch date, then NUL.
/// Returns the release version `X.Y` (#1526).
fn parse_ncurses_version(tail: &[u8]) -> Option<String> {
    let end = tail.iter().position(|&b| b == 0)?;
    let text = std::str::from_utf8(&tail[..end]).ok()?;
    let mut parts = text.split('.');
    let (major, minor) = (parts.next()?, parts.next()?);
    let patch_date = parts.next();
    let numeric = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if parts.next().is_some()
        || !numeric(major)
        || !numeric(minor)
        || patch_date.is_some_and(|d| d.len() != 8 || !numeric(d))
    {
        return None;
    }
    Some(format!("{major}.{minor}"))
}

/// PCRE version: `X.Y YYYY-MM-DD`.
fn parse_pcre_version(tail: &[u8]) -> Option<String> {
    let mut end = 0;
    let mut seen_dot = false;
    while end < tail.len() {
        let b = tail[end];
        if b.is_ascii_digit() {
            end += 1;
        } else if b == b'.' && !seen_dot {
            seen_dot = true;
            end += 1;
        } else {
            break;
        }
    }
    if !seen_dot || end == 0 {
        return None;
    }
    // Confirm ` YYYY-MM-DD` follows.
    let after = &tail[end..];
    if after.len() < 11 || after[0] != b' ' {
        return None;
    }
    let date = &after[1..11];
    let looks_like_date = date[0..4].iter().all(|b| b.is_ascii_digit())
        && date[4] == b'-'
        && date[5..7].iter().all(|b| b.is_ascii_digit())
        && date[7] == b'-'
        && date[8..10].iter().all(|b| b.is_ascii_digit());
    if !looks_like_date {
        return None;
    }
    std::str::from_utf8(&tail[..end]).ok().map(str::to_string)
}

/// OpenJDK version (milestone 026). Two schemes:
/// - **Modern (JEP 322)**: `<major>.<minor>.<security>(+<build>)?`
///   e.g. `21.0.1+12`, `17.0.5`. Each segment is 1+ digits; `+build`
///   is optional but commonly present in HotSpot banners.
/// - **Legacy (Java 8)**: `8u<update>(-b<build>)?` e.g. `8u362-b09`,
///   `8u362`. `-b<build>` is optional.
///
/// Returns the matched version string verbatim (preserving `+12` or
/// `-b09` suffix) so consumers see what symbol-server / CVE
/// databases key on. Terminates on whitespace / NUL / non-version-char.
fn parse_openjdk_version(tail: &[u8]) -> Option<String> {
    parse_openjdk_modern(tail).or_else(|| parse_openjdk_legacy(tail))
}

/// JEP 322 form: `<digits>.<digits>.<digits>(+<digits>)?`.
fn parse_openjdk_modern(tail: &[u8]) -> Option<String> {
    let mut end = 0;
    let mut dots = 0;
    while end < tail.len() {
        let b = tail[end];
        if b.is_ascii_digit() {
            end += 1;
        } else if b == b'.' {
            dots += 1;
            if dots > 2 {
                break;
            }
            end += 1;
        } else {
            break;
        }
    }
    if dots != 2 || end == 0 {
        return None;
    }
    // Optional `+<digits>` build suffix.
    if tail.get(end) == Some(&b'+') {
        let mut e = end + 1;
        let build_start = e;
        while e < tail.len() && tail[e].is_ascii_digit() {
            e += 1;
        }
        // Only accept if at least one digit followed the `+`.
        if e > build_start {
            end = e;
        }
    }
    // Must terminate on a non-version char.
    let terminator_ok = match tail.get(end) {
        Some(&c) => !c.is_ascii_alphanumeric() && c != b'.' && c != b'+',
        None => true,
    };
    if !terminator_ok {
        return None;
    }
    std::str::from_utf8(&tail[..end]).ok().map(str::to_string)
}

/// Java-8 form: `8u<digits>(-b<digits>)?`.
fn parse_openjdk_legacy(tail: &[u8]) -> Option<String> {
    if tail.len() < 3 || &tail[..2] != b"8u" {
        return None;
    }
    let mut end = 2;
    let update_start = end;
    while end < tail.len() && tail[end].is_ascii_digit() {
        end += 1;
    }
    // Must have at least one digit after `8u`.
    if end == update_start {
        return None;
    }
    // Optional `-b<digits>` build suffix.
    if tail.len() >= end + 2 && &tail[end..end + 2] == b"-b" {
        let mut e = end + 2;
        let build_start = e;
        while e < tail.len() && tail[e].is_ascii_digit() {
            e += 1;
        }
        if e > build_start {
            end = e;
        }
    }
    // Must terminate on a non-version char.
    let terminator_ok = match tail.get(end) {
        Some(&c) => !c.is_ascii_alphanumeric() && c != b'-',
        None => true,
    };
    if !terminator_ok {
        return None;
    }
    std::str::from_utf8(&tail[..end]).ok().map(str::to_string)
}

/// 40-char lowercase hex git SHA.
fn parse_git_sha(tail: &[u8]) -> Option<String> {
    if tail.len() < 40 {
        return None;
    }
    let sha = &tail[..40];
    if sha
        .iter()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        std::str::from_utf8(sha).ok().map(str::to_string)
    } else {
        None
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Two variants sharing a slug would make the slug ambiguous as a
    /// mapping-table key, and the curated CPE table (`generate/cpe_table.toml`) is keyed by slug.
    ///
    /// `CuratedLibrary::iter()` rather than any list of variants: see
    /// the derive's comment above the enum, and
    /// `generate::cpe::tests::mappings_cover_all_curated_libraries`,
    /// which is the check #1246 is actually about.
    #[test]
    fn slugs_are_unique_and_lowercase() {
        use strum::IntoEnumIterator;

        let mut slugs: Vec<&str> = CuratedLibrary::iter().map(|l| l.slug()).collect();
        assert!(!slugs.is_empty(), "CuratedLibrary::iter() yielded nothing");
        let before = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(
            slugs.len(),
            before,
            "two CuratedLibrary variants share a slug"
        );
        for slug in &slugs {
            assert!(
                !slug.is_empty()
                    && slug
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "slug {slug:?} is not a lowercase ASCII table key"
            );
        }
    }

    fn region(inner: &[u8]) -> Vec<u8> {
        // Pad with a leading NUL so the first embed starts at a
        // C-string boundary.
        let mut v = vec![0u8];
        v.extend_from_slice(inner);
        v.push(0);
        v
    }

    #[test]
    fn openssl_positive() {
        let r = region(b"OpenSSL 3.0.11 19 Sep 2023");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::OpenSsl);
        assert_eq!(hits[0].version, "3.0.11");
    }

    #[test]
    fn openssl_letter_suffix() {
        let r = region(b"OpenSSL 1.1.1w 11 Sep 2023");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].version, "1.1.1w");
    }

    #[test]
    fn openssl_no_false_positive_on_library_name_alone() {
        // `OpenSSL` followed by a word, not a version.
        let r = region(b"error building with OpenSSL enabled, need to install");
        let hits = scan(&r);
        assert!(hits.is_empty(), "no version → no hit; got {hits:?}");
    }

    #[test]
    fn zlib_requires_copyright_context() {
        let with_copyright = region(b"deflate 1.2.13 Copyright 1995-2022");
        let without = region(b"deflate 1.2.13 status unavailable");
        assert_eq!(scan(&with_copyright).len(), 1);
        assert_eq!(scan(&with_copyright)[0].library, CuratedLibrary::Zlib);
        assert_eq!(scan(&with_copyright)[0].version, "1.2.13");
        assert!(scan(&without).is_empty());
    }

    /// #1525: zlib's literal is `" deflate X.Y.Z Copyright ..."`, with a
    /// leading space, so the byte before `deflate` is never NUL. These
    /// bytes are copied from Nix builds of zlib 1.3.2 and 1.3.1
    /// (`libz.1.3.2.dylib`, `libz.1.3.1.dylib`), NUL to NUL.
    #[test]
    fn s1525_zlib_real_literal_with_leading_space() {
        let real_132 =
            b"\0 deflate 1.3.2 Copyright 1995-2026 Jean-loup Gailly and Mark Adler \0";
        let real_131 =
            b"\0 deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0";
        for (bytes, want) in [(&real_132[..], "1.3.2"), (&real_131[..], "1.3.1")] {
            let hits = scan(bytes);
            assert_eq!(hits.len(), 1, "{want}: {hits:?}");
            assert_eq!(hits[0].library, CuratedLibrary::Zlib);
            assert_eq!(hits[0].version, want);
        }
    }

    /// #1525: `deflate` in the middle of other text is not zlib's banner.
    /// Spec 1643 (FR-002): the byte before zlib's leading space may be any
    /// non-identifier byte, so a second space is accepted; an identifier
    /// before the space (`see deflate`) is still rejected. The census found
    /// no whitespace-preceded zlib banner on any target
    /// (`specs/1643-version-string-boundary/measurements/census.md`).
    #[test]
    fn s1525_zlib_mid_string_still_rejected() {
        let two_spaces = b"\0  deflate 1.3.2 Copyright 1995-2026 \0";
        let mid_text = b"\0see deflate 1.3.2 Copyright 1995-2026 \0";
        assert_eq!(scan(two_spaces).len(), 1);
        assert!(scan(mid_text).is_empty());
    }

    /// #1526: bytes copied NUL to NUL from Nix builds: expat 2.9.0 and
    /// 2.7.5 (`libexpat.1.13.0.dylib`, `libexpat.1.11.3.dylib`) and ncurses
    /// 6.6 (`libncursesw.6.dylib`).
    #[test]
    fn s1526_expat_and_ncurses_real_literals() {
        let cases: [(&[u8], CuratedLibrary, &str); 3] = [
            (b"\0expat_2.9.0\0", CuratedLibrary::Expat, "2.9.0"),
            (b"\0expat_2.7.5\0", CuratedLibrary::Expat, "2.7.5"),
            // The release version; the patch date is dropped (NVD CPEs and
            // nixpkgs both call this build 6.6).
            (b"\0ncurses 6.6.20251230\0", CuratedLibrary::Ncurses, "6.6"),
        ];
        for (bytes, lib, want) in cases {
            let hits = scan(bytes);
            assert_eq!(hits.len(), 1, "{want}: {hits:?}");
            assert_eq!(hits[0].library, lib);
            assert_eq!(hits[0].version, want);
        }
    }

    /// #1526: the same libraries' other strings, which share a prefix
    /// but carry no version, do not match.
    #[test]
    fn s1526_prefixes_without_a_version_do_not_match() {
        for bytes in [
            &b"\0expat_heap_increase_tolerable\0"[..],
            b"\0expat_free\0",
            b"\0ncurses library\0",
        ] {
            assert!(scan(bytes).is_empty(), "{:?}", String::from_utf8_lossy(bytes));
        }
    }

    #[test]
    fn sqlite_positive() {
        let r = region(b"SQLite version 3.44.2");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::Sqlite);
        assert_eq!(hits[0].version, "3.44.2");
    }

    #[test]
    fn sqlite_four_segment_version() {
        let r = region(b"SQLite version 3.44.2.1");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].version, "3.44.2.1");
    }

    #[test]
    fn curl_positive() {
        let r = region(b"libcurl/8.4.0");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::Curl);
        assert_eq!(hits[0].version, "8.4.0");
    }

    #[test]
    fn pcre_vs_pcre2_disambiguation() {
        let r1 = region(b"PCRE 8.45 2021-06-15");
        let r2 = region(b"PCRE2 10.42 2022-12-11");
        assert_eq!(scan(&r1)[0].library, CuratedLibrary::Pcre);
        assert_eq!(scan(&r1)[0].version, "8.45");
        assert_eq!(scan(&r2)[0].library, CuratedLibrary::Pcre2);
        assert_eq!(scan(&r2)[0].version, "10.42");
    }

    #[test]
    fn boringssl_git_sha() {
        // 40-char lowercase hex git SHA (not 44).
        let r = region(b"BoringSSL aaaabbbbccccddddeeeeffff0000111122223333");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::BoringSsl);
        assert_eq!(hits[0].version, "aaaabbbbccccddddeeeeffff0000111122223333");
    }

    #[test]
    fn bare_version_without_prefix_doesnt_match() {
        // SC-005 control: a number that looks like a version but has
        // no library-name prefix must NOT match.
        let r = region(b"3.0.11 is the magic number");
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn mid_string_openssl_not_matched() {
        // Library name appears mid-string, not at a C-string boundary.
        let r = region(b"Using OpenSSL 3.0.11 for crypto");
        // Even though the name+version appears correctly, our boundary
        // rule (NUL-prefix) rejects the match because `OpenSSL` is
        // preceded by `Using `. This tightens the false-positive
        // surface. Users who want the match can store OpenSSL's ID
        // string at a string boundary (which real libraries do).
        let hits = scan(&r);
        assert!(hits.is_empty(), "mid-string match must be rejected; got {hits:?}");
    }

    #[test]
    fn dedup_within_single_scan() {
        // Two copies of the same embed → one match.
        let mut v = vec![0u8];
        v.extend_from_slice(b"OpenSSL 3.0.11 19 Sep 2023");
        v.push(0);
        v.extend_from_slice(b"OpenSSL 3.0.11 19 Sep 2023");
        v.push(0);
        let hits = scan(&v);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn two_different_libraries_both_match() {
        let mut v = vec![0u8];
        v.extend_from_slice(b"OpenSSL 3.0.11 foo");
        v.push(0);
        v.extend_from_slice(b"libcurl/8.4.0");
        v.push(0);
        let hits = scan(&v);
        assert_eq!(hits.len(), 2);
    }

    // ====================================================================
    // Milestone 026 — easy-4 cohort (GnuTLS / LibreSSL / LLVM / OpenJDK)
    // ====================================================================

    #[test]
    fn gnutls_positive() {
        let r = region(b"GnuTLS 3.7.10");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::GnuTls);
        assert_eq!(hits[0].version, "3.7.10");
    }

    #[test]
    fn gnutls_no_version_no_match() {
        // Bare `GnuTLS` with no version should not match.
        let r = region(b"GnuTLS");
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn libressl_positive() {
        let r = region(b"LibreSSL 3.8.2");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::LibreSsl);
        assert_eq!(hits[0].version, "3.8.2");
    }

    #[test]
    fn libressl_distinct_from_openssl() {
        // Region with both signatures present at NUL boundaries
        // → exactly two matches, no double-emit. Tests prefix
        // anchoring: LibreSSL must NOT also fire the OpenSSL detector.
        let mut v = vec![0u8];
        v.extend_from_slice(b"LibreSSL 3.8.2");
        v.push(0);
        v.extend_from_slice(b"OpenSSL 3.0.11 19 Sep 2023");
        v.push(0);
        let hits = scan(&v);
        assert_eq!(hits.len(), 2, "expected 2 matches (LibreSSL + OpenSSL); got {hits:?}");
        let libs: std::collections::HashSet<CuratedLibrary> =
            hits.iter().map(|h| h.library).collect();
        assert!(libs.contains(&CuratedLibrary::LibreSsl));
        assert!(libs.contains(&CuratedLibrary::OpenSsl));
    }

    #[test]
    fn llvm_positive() {
        let r = region(b"LLVM version 17.0.6");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::Llvm);
        assert_eq!(hits[0].version, "17.0.6");
    }

    #[test]
    fn llvm_bare_name_does_not_match() {
        // `LLVM ERROR:` (clang/lld error string) must NOT match —
        // strict prefix requires `LLVM version `.
        let r1 = region(b"LLVM ERROR: bitcode is invalid");
        assert!(scan(&r1).is_empty(), "got {:?}", scan(&r1));
        // `LLVM 17.0.0` (without `version `) must NOT match — same
        // strictness rule.
        let r2 = region(b"LLVM 17.0.0");
        assert!(scan(&r2).is_empty(), "got {:?}", scan(&r2));
    }

    #[test]
    fn openjdk_modern_version_with_build() {
        let r = region(b"OpenJDK 21.0.1+12");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::OpenJdk);
        assert_eq!(hits[0].version, "21.0.1+12");
    }

    #[test]
    fn openjdk_modern_no_build_suffix() {
        let r = region(b"OpenJDK 17.0.5");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::OpenJdk);
        assert_eq!(hits[0].version, "17.0.5");
    }

    /// #1258 — the shape measured in the official images.
    #[test]
    fn php_x_powered_by_header_yields_the_interpreter_version() {
        let r = region(b"X-Powered-By: PHP/8.3.35");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1, "got {hits:?}");
        assert_eq!(hits[0].library, CuratedLibrary::Php);
        assert_eq!(hits[0].version, "8.3.35");
    }

    /// A bare version must not match. This is not a hypothetical: the real
    /// `/usr/local/bin/php` carries `8.3.35` twice over besides the header,
    /// so a prefix-less matcher would have two other things to latch onto.
    #[test]
    fn php_bare_version_without_the_header_does_not_match() {
        let r = region(b"8.3.35");
        assert!(scan(&r).is_empty(), "got {:?}", scan(&r));
    }

    /// The header with nothing parseable after it must not match either.
    #[test]
    fn php_header_without_a_version_does_not_match() {
        let r = region(b"X-Powered-By: PHP/");
        assert!(scan(&r).is_empty(), "got {:?}", scan(&r));
    }

    /// `PHP/` alone is too weak to be the anchor — the full header is.
    #[test]
    fn php_slash_version_alone_does_not_match() {
        let r = region(b"PHP/8.3.35");
        assert!(scan(&r).is_empty(), "got {:?}", scan(&r));
    }

    #[test]
    fn openjdk_legacy_8u_version() {
        let r = region(b"OpenJDK 8u362-b09");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::OpenJdk);
        assert_eq!(hits[0].version, "8u362-b09");
    }

    // ====================================================================
    // Issue #1244 — the Node.js runtime
    //
    // Byte shapes here are the ones measured in real node binaries, not
    // invented: the NUL-preceded `node.js/v<semver>` literal, and the
    // second occurrence that abuts the next literal with no NUL between.
    // ====================================================================

    #[test]
    fn node_positive() {
        let r = region(b"node.js/v24.14.1");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].library, CuratedLibrary::Node);
        assert_eq!(hits[0].version, "24.14.1");
    }

    #[test]
    fn node_unterminated_literal_stops_at_the_version() {
        // Measured in node 24.14.1: one of the two occurrences is a
        // length-counted literal that runs straight into the next one.
        let mut v = vec![0u8];
        v.extend_from_slice(b"node.js/v24.14.1Protocol");
        v.push(0);
        let hits = scan(&v);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].version, "24.14.1");
    }

    #[test]
    fn node_bare_version_without_prefix_doesnt_match() {
        let r = region(b"24.14.1");
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn node_prefix_without_version_doesnt_match() {
        let r = region(b"node.js/v");
        assert!(scan(&r).is_empty());
        let r = region(b"node.js/vnext");
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn node_two_digit_version_needs_all_three_segments() {
        // `parse_semver_triple` requires exactly two dots, so a
        // major.minor User-Agent is not accepted as a runtime version.
        let r = region(b"node.js/v24.14");
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn node_nightly_prerelease_yields_the_triple() {
        let r = region(b"node.js/v25.0.0-nightly20260101abcdef");
        let hits = scan(&r);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].version, "25.0.0");
    }

    #[test]
    fn node_and_its_bundled_openssl_both_match() {
        // A real node binary carries both; the runtime must not mask
        // the library it bundles, nor the reverse. `OpenSSL 3.5.5` is
        // the string measured in node 24.14.1's .rodata.
        let mut v = vec![0u8];
        v.extend_from_slice(b"node.js/v24.14.1");
        v.push(0);
        v.extend_from_slice(b"OpenSSL 3.5.5 ");
        v.push(0);
        let hits = scan(&v);
        assert_eq!(hits.len(), 2, "got {hits:?}");
        assert!(hits
            .iter()
            .any(|h| h.library == CuratedLibrary::Node && h.version == "24.14.1"));
        assert!(hits
            .iter()
            .any(|h| h.library == CuratedLibrary::OpenSsl && h.version == "3.5.5"));
    }

    #[test]
    fn node_mid_string_not_matched() {
        // Same rule as `mid_string_openssl_not_matched`: a User-Agent
        // header assembled into a longer literal is not a boundary.
        let r = region(b"User-Agent: node.js/v24.14.1");
        let hits = scan(&r);
        assert!(hits.is_empty(), "mid-string match must be rejected; got {hits:?}");
    }

    #[test]
    fn node_dedup_across_both_measured_occurrences() {
        // Measured: two distinct occurrences per binary, same version.
        let mut v = vec![0u8];
        v.extend_from_slice(b"node.js/v24.14.1");
        v.push(0);
        v.extend_from_slice(b"node.js/v24.14.1Protocol");
        v.push(0);
        let hits = scan(&v);
        assert_eq!(hits.len(), 1);
    }

    /// Spec 1643 T003: the census classifies the byte before each literal
    /// and gives both rules' verdicts.
    fn one_hit(bytes: &[u8]) -> CensusHit {
        let hits = census(bytes);
        assert_eq!(hits.len(), 1, "{:?}: {hits:?}", String::from_utf8_lossy(bytes));
        hits.into_iter().next().unwrap()
    }

    #[test]
    fn s1643_census_nul_preceded_ncurses() {
        let h = one_hit(b"lead\0ncurses 6.6.20251230\0");
        assert_eq!((h.library, h.version.as_str(), h.preceding), (CuratedLibrary::Ncurses, "6.6", "nul"));
        assert!(h.accepted_main && h.accepted_new);
        assert_eq!(h.offset, 5);
        assert_eq!(h.context_before, b"lead\0");
    }

    #[test]
    fn s1643_census_dash_preceded_ncurses() {
        let h = one_hit(b"lead-ncurses 6.6.20251230\0");
        assert_eq!(h.preceding, "other_non_identifier");
        assert!(!h.accepted_main && h.accepted_new);
    }

    #[test]
    fn s1643_census_space_preceded_ncurses() {
        let h = one_hit(b"lead ncurses 6.6.20251230\0");
        assert_eq!(h.preceding, "whitespace");
        assert!(!h.accepted_main && h.accepted_new);
    }

    #[test]
    fn s1643_census_identifier_preceded_ncurses() {
        let h = one_hit(b"leadxncurses 6.6.20251230\0");
        assert_eq!(h.preceding, "identifier");
        assert!(!h.accepted_main && !h.accepted_new);
    }

    #[test]
    fn s1643_census_zlib_classifies_the_byte_before_its_space() {
        let dash = one_hit(b"lead- deflate 1.3.2 Copyright 1995-2026 \0");
        assert_eq!((dash.library, dash.preceding, dash.offset), (CuratedLibrary::Zlib, "other_non_identifier", 5));
        assert!(!dash.accepted_main && dash.accepted_new);
        let nul = one_hit(b"lead\0 deflate 1.3.2 Copyright 1995-2026 \0");
        assert_eq!(nul.preceding, "nul");
        assert!(nul.accepted_main && nul.accepted_new);
    }

    #[test]
    fn s1643_census_openssl_keeps_the_nul_rule() {
        let h = one_hit(b"lead-OpenSSL 3.0.11\0");
        assert_eq!((h.library, h.preceding), (CuratedLibrary::OpenSsl, "other_non_identifier"));
        assert!(!h.accepted_main && !h.accepted_new);
    }

    #[test]
    fn s1643_census_at_region_start() {
        let h = one_hit(b"ncurses 6.6.20251230\0");
        assert_eq!(h.preceding, "start");
        assert!(h.accepted_main && h.accepted_new);
    }

    #[test]
    fn s1643_census_json_is_raw() {
        let j = one_hit(b"lead-ncurses 6.6.20251230\0").to_json();
        for needle in [r#""library":"ncurses""#, r#""preceding":"other_non_identifier""#, r#""accepted_new":true"#] {
            assert!(j.contains(needle), "{needle} not in {j}");
        }
    }

    /// Spec 1643: one canonical banner per variant. Exhaustive with no
    /// wildcard arm, so a new variant needs a sample before this compiles.
    fn sample(lib: CuratedLibrary) -> &'static [u8] {
        match lib {
            CuratedLibrary::OpenSsl => b"OpenSSL 3.0.11\0",
            CuratedLibrary::BoringSsl => b"BoringSSL 0123456789abcdef0123456789abcdef01234567\0",
            CuratedLibrary::Zlib => b" deflate 1.3.2 Copyright 1995-2026 Jean-loup Gailly and Mark Adler \0",
            CuratedLibrary::Sqlite => b"SQLite version 3.44.2\0",
            CuratedLibrary::Curl => b"libcurl/8.5.0\0",
            CuratedLibrary::Pcre => b"PCRE 8.45 2021-06-15\0",
            CuratedLibrary::Pcre2 => b"PCRE2 10.42 2022-12-11\0",
            CuratedLibrary::GnuTls => b"GnuTLS 3.8.3\0",
            CuratedLibrary::LibreSsl => b"LibreSSL 3.8.2\0",
            CuratedLibrary::Llvm => b"LLVM version 17.0.6\0",
            CuratedLibrary::OpenJdk => b"OpenJDK 21.0.1+12\0",
            CuratedLibrary::Php => b"X-Powered-By: PHP/8.3.35\0",
            CuratedLibrary::Node => b"node.js/v24.14.1\0",
            CuratedLibrary::Expat => b"expat_2.7.5\0",
            CuratedLibrary::Ncurses => b"ncurses 6.6.20251230\0",
            // Anchor and candidate together: the prefix scan must still
            // find nothing (spec 1644).
            CuratedLibrary::Bzip2 => b"bzip2/libbzip2: internal error\x001.0.8, 13-Jul-2019\0",
            CuratedLibrary::Gmp => b"GNU MP: Cannot allocate memory\x006.3.0\0",
            CuratedLibrary::Liblzma => b"5.8.1\0",
        }
    }

    fn behind(prev: Option<u8>, literal: &[u8]) -> Vec<u8> {
        let mut r = b"lead".to_vec();
        r.extend(prev);
        r.extend_from_slice(literal);
        r
    }

    fn finds(region: &[u8], lib: CuratedLibrary) -> bool {
        scan(region).iter().any(|m| m.library == lib)
    }

    /// FR-001's anchored libraries, written out from the spec's table.
    const ANCHORED: [CuratedLibrary; 3] = [CuratedLibrary::Zlib, CuratedLibrary::Expat, CuratedLibrary::Ncurses];

    /// Spec 1644's whole-string libraries, written out from its data model.
    const WHOLE_STRING: [CuratedLibrary; 3] = [CuratedLibrary::Bzip2, CuratedLibrary::Gmp, CuratedLibrary::Liblzma];

    /// Spec 1643 FR-001/FR-004 (R3): every variant behind NUL matches, and
    /// behind `-` matches exactly when FR-001's table classes it anchored.
    /// The expectation is the spec's table (`ANCHORED`), not
    /// `CuratedLibrary::boundary`, which the scanner itself reads: comparing
    /// the scanner with `boundary()` would pass whatever `boundary()` said
    /// (mutations M2-M5, `measurements/mutations.md`).
    #[test]
    fn s1643_every_variant_follows_its_class() {
        use strum::IntoEnumIterator;
        for lib in CuratedLibrary::iter() {
            let anchored = ANCHORED.contains(&lib);
            assert_eq!(lib.boundary() == Boundary::Anchored, anchored, "{lib:?}: class differs from FR-001");
            // Spec 1644: a whole-string library never matches through the
            // prefix scan, whatever precedes its sample.
            let whole = WHOLE_STRING.contains(&lib);
            assert_eq!(lib.boundary() == Boundary::WholeString, whole, "{lib:?}: class differs from spec 1644");
            if whole {
                for prev in [None, Some(0), Some(b'-')] {
                    let r = if prev.is_none() { sample(lib).to_vec() } else { behind(prev, sample(lib)) };
                    assert!(!finds(&r, lib), "{lib:?} found by the prefix scan behind {prev:?}");
                }
                continue;
            }
            assert!(finds(&behind(Some(0), sample(lib)), lib), "{lib:?} behind NUL");
            let dash = finds(&behind(Some(b'-'), sample(lib)), lib);
            assert_eq!(dash, anchored, "{lib:?} behind '-'");
        }
    }

    /// Spec 1643 FR-002: an anchored pattern behind any non-identifier byte.
    #[test]
    fn s1643_anchored_patterns_accept_a_non_identifier_byte() {
        for lib in ANCHORED {
            for prev in [0u8, b'-', 0xff, b' ', b'/', b'.', b':', 0x80] {
                assert!(finds(&behind(Some(prev), sample(lib)), lib), "{lib:?} behind {prev:#04x}");
            }
            assert!(finds(sample(lib), lib), "{lib:?} at the region's start");
        }
    }

    /// Spec 1643 FR-002: an identifier character still rejects.
    #[test]
    fn s1643_anchored_patterns_reject_an_identifier_byte() {
        for lib in ANCHORED {
            for &prev in b"xL9_" {
                assert!(!finds(&behind(Some(prev), sample(lib)), lib), "{lib:?} behind {:?}", prev as char);
            }
        }
    }

    /// Spec 1643 FR-002: zlib's form without the leading space keeps the
    /// NUL rule.
    #[test]
    fn s1643_zlib_without_its_space_keeps_the_nul_rule() {
        let bare = &sample(CuratedLibrary::Zlib)[1..];
        assert!(finds(&behind(Some(0), bare), CuratedLibrary::Zlib));
        assert!(!finds(&behind(Some(b'-'), bare), CuratedLibrary::Zlib));
    }

    /// Spec 1643 (analysis I1, SC-006): the census's `accepted_new` hits are
    /// exactly what `scan` returns, for every variant behind every possible
    /// preceding byte and at the region's start. The census tries every
    /// pattern and then applies the rule; `scan` first narrows the patterns
    /// by the preceding byte, so this is also the check that the narrowing
    /// drops nothing the rule would accept.
    #[test]
    fn s1643_census_agrees_with_the_scanner() {
        use std::collections::HashSet;
        use strum::IntoEnumIterator;
        for lib in CuratedLibrary::iter() {
            let prevs = std::iter::once(None).chain((0..=255u8).map(Some));
            for prev in prevs {
                let r = if prev.is_none() { sample(lib).to_vec() } else { behind(prev, sample(lib)) };
                let from_scan: HashSet<(CuratedLibrary, String)> =
                    scan(&r).into_iter().map(|m| (m.library, m.version)).collect();
                let from_census: HashSet<(CuratedLibrary, String)> = census(&r)
                    .into_iter()
                    .filter(|h| h.accepted_new)
                    .map(|h| (h.library, h.version))
                    .collect();
                assert_eq!(from_scan, from_census, "{lib:?} behind {prev:?}");
            }
        }
    }

    // ---- Spec 1644: anchored whole-string libraries -------------------

    fn verdicts(region: &[u8], syms: &[&str]) -> Vec<AnchoredVerdict> {
        let syms: Vec<String> = syms.iter().map(|s| s.to_string()).collect();
        anchored_verdicts(region, &syms)
    }

    fn verdict_of(region: &[u8], syms: &[&str], lib: CuratedLibrary) -> Option<Verdict> {
        verdicts(region, syms).into_iter().find(|v| v.library == lib).map(|v| v.verdict)
    }

    /// FR-009: the production list is exactly the whole-string class, and
    /// each member has an anchor; no other variant has one.
    #[test]
    fn s1644_whole_string_list_matches_the_class() {
        use strum::IntoEnumIterator;
        for lib in CuratedLibrary::iter() {
            let whole = lib.boundary() == Boundary::WholeString;
            assert_eq!(WHOLE_STRING_LIBRARIES.contains(&lib), whole, "{lib:?}");
            assert_eq!(lib.anchor().is_some(), whole, "{lib:?} anchor");
        }
    }

    #[test]
    fn s1644_each_library_versioned_with_anchor_and_one_candidate() {
        let v = Verdict::Versioned;
        assert_eq!(
            verdict_of(b"\0\n\nbzip2/libbzip2: internal error number %d.\n\x001.0.8, 13-Jul-2019\0", &[], CuratedLibrary::Bzip2),
            Some(v("1.0.8".into()))
        );
        assert_eq!(verdict_of(b"GNU MP: Cannot allocate memory\x006.3.0\0", &[], CuratedLibrary::Gmp), Some(v("6.3.0".into())));
        assert_eq!(verdict_of(b"5.8.1\0", &["lzma_version_string"], CuratedLibrary::Liblzma), Some(v("5.8.1".into())));
        assert_eq!(verdict_of(b"5.7.1alpha\0", &["lzma_version_number"], CuratedLibrary::Liblzma), Some(v("5.7.1alpha".into())));
        assert_eq!(verdict_of(b"5.7.2beta\0", &["lzma_version_number"], CuratedLibrary::Liblzma), Some(v("5.7.2beta".into())));
    }

    #[test]
    fn s1644_no_anchor_no_verdict() {
        assert!(verdicts(b"\x001.0.8, 13-Jul-2019\x006.3.0\x005.8.1\0", &[]).is_empty());
        // A symbol outside the list anchors nothing.
        assert!(verdicts(b"5.8.1\0", &["lzma_code"]).is_empty());
    }

    #[test]
    fn s1644_two_candidates_are_ambiguous() {
        let v = verdicts(b"GNU MP:\x006.3.0\x006.2.1\0", &[]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].verdict, Verdict::Ambiguous);
        assert_eq!(v[0].candidates.len(), 2);
    }

    #[test]
    fn s1644_a_repeated_candidate_counts_once() {
        assert_eq!(verdict_of(b"GNU MP:\x006.3.0\x006.3.0\0", &[], CuratedLibrary::Gmp), Some(Verdict::Versioned("6.3.0".into())));
    }

    /// FR-005a: a string both anchored libraries accept versions neither.
    #[test]
    fn s1644_a_shared_candidate_versions_neither() {
        let v = verdicts(b"GNU MP:\x005.8.1\0", &["lzma_version_string"]);
        assert_eq!(v.len(), 2);
        for x in &v {
            assert_eq!(x.verdict, Verdict::AmbiguousShared, "{:?}", x.library);
            assert_eq!(x.shared.len(), 1);
        }
        // A candidate only one of them accepts is unaffected.
        assert_eq!(
            verdict_of(b"GNU MP:\x005.8.1\x005.7.1alpha\0", &["lzma_version_string"], CuratedLibrary::Liblzma),
            Some(Verdict::Versioned("5.7.1alpha".into()))
        );
        assert_eq!(
            verdict_of(b"GNU MP:\x005.8.1\x005.7.1alpha\0", &["lzma_version_string"], CuratedLibrary::Gmp),
            Some(Verdict::AmbiguousShared)
        );
    }

    #[test]
    fn s1644_anchor_with_no_candidate() {
        assert_eq!(verdict_of(b"GNU MP:\0requires 6.3.0 or later\0", &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate));
        assert_eq!(verdict_of(b"4.999.8beta-21-g1d92\0", &["lzma_version_string"], CuratedLibrary::Liblzma), Some(Verdict::NoCandidate));
        assert_eq!(verdict_of(b"bzip2/libbzip2\x001.0.8\0", &[], CuratedLibrary::Bzip2), Some(Verdict::NoCandidate));
    }

    /// FR-004: whole strings only. The region's start counts as a boundary;
    /// an unterminated tail does not.
    #[test]
    fn s1644_candidate_boundaries() {
        assert_eq!(verdict_of(b"6.3.0\0GNU MP:\0", &[], CuratedLibrary::Gmp), Some(Verdict::Versioned("6.3.0".into())));
        assert_eq!(verdict_of(b"GNU MP:\x006.3.0", &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate));
        assert_eq!(verdict_of(b"GNU MP:\0x6.3.0\0", &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate));
        assert_eq!(verdict_of(b"GNU MP:\x006.3.0x\0", &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate));
        assert_eq!(verdict_of(b"GNU MP:\x006.3\0", &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate));
        assert_eq!(verdict_of(b"GNU MP:\x0012345.3.0\0", &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate));
    }

    /// FR-004 as amended: a binary byte may precede a candidate (GMP's
    /// `gmp_version` follows a constant table in `.rodata`); printable text
    /// may not.
    #[test]
    fn s1644_candidate_after_binary_data() {
        for prev in [0xffu8, 0x01, 0x80, 0x7f] {
            let mut r = b"GNU MP:\0".to_vec();
            r.extend_from_slice(&[0x9d, 0x07, prev]);
            r.extend_from_slice(b"6.3.0\0");
            let v = verdicts(&r, &[]);
            assert_eq!(v[0].verdict, Verdict::Versioned("6.3.0".into()), "behind {prev:#04x}");
            assert_eq!(v[0].candidates[0].preceding, "binary");
        }
        for &prev in b" \t\n-v_" {
            let mut r = b"GNU MP:\0\xff".to_vec();
            r.push(prev);
            r.extend_from_slice(b"6.3.0\0");
            assert_eq!(verdict_of(&r, &[], CuratedLibrary::Gmp), Some(Verdict::NoCandidate), "behind {:?}", prev as char);
        }
        assert_eq!(verdicts(b"GNU MP:\x006.3.0\0", &[])[0].candidates[0].preceding, "nul");
        assert_eq!(verdicts(b"6.3.0\0GNU MP:\0", &[])[0].candidates[0].preceding, "start");
    }

    #[test]
    fn s1644_bzip2_date_form_is_exact() {
        let anchor = b"bzip2/libbzip2\0".to_vec();
        for bad in [&b"1.0.8, 13-Foo-2019"[..], b"1.0.8, 13-Jul-219", b"1.0.8, 123-Jul-2019", b"1.0.8 13-Jul-2019", b"1.0.8, 13-Jul-2019x"] {
            let mut r = anchor.clone();
            r.extend_from_slice(bad);
            r.push(0);
            assert_eq!(verdict_of(&r, &[], CuratedLibrary::Bzip2), Some(Verdict::NoCandidate), "{}", String::from_utf8_lossy(bad));
        }
        assert_eq!(
            verdict_of(b"bzip2/libbzip2\x001.0.6, 6-Sept-2010\x001.0.6, 6-Sep-2010\0", &[], CuratedLibrary::Bzip2),
            Some(Verdict::Versioned("1.0.6".into()))
        );
    }

    #[test]
    fn s1644_census_json() {
        let v = verdicts(b"GNU MP:\x006.3.0\0", &[]);
        let j = v[0].to_json();
        for needle in [r#""library":"gmp""#, r#""kind":"text""#, r#""value":"GNU MP:""#, r#""verdict":"versioned""#, r#""version":"6.3.0""#] {
            assert!(j.contains(needle), "{needle} not in {j}");
        }
        let s = verdicts(b"GNU MP:\x005.8.1\0", &["lzma_version_string"]);
        let j = s[0].to_json();
        assert!(j.contains(r#""verdict":"ambiguous-shared""#) && j.contains(r#""with":"liblzma""#), "{j}");
        assert!(!j.contains(r#""version""#), "{j}");
    }

    #[test]
    fn s1644_bzip2_form_without_anchor_is_census_only() {
        let c = bzip2_form_without_anchor(b"x\x001.0.8, 13-Jul-2019\0");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].version, "1.0.8");
        assert!(bzip2_form_without_anchor(b"bzip2/libbzip2\x001.0.8, 13-Jul-2019\0").is_empty());
        assert!(scan(b"x\x001.0.8, 13-Jul-2019\0").is_empty());
    }
}
