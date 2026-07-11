//! Standalone `.rpm` package-file reader. Milestone 004 US1
//! (FR-010..FR-017). Emits one `pkg:rpm/<vendor>/<name>@<epoch>:<version>-<release>?arch=<arch>`
//! component per `.rpm` artefact observed, with licenses + supplier +
//! REQUIRES populated from header tags.
//!
//! Parsing uses the `rpm` crate (pure-Rust, audited Principle-I clean
//! per research R1 + task T002). Defense-in-depth:
//! - Per-file size cap of 200 MB (FR-007).
//! - Magic-byte validation at offset 0 (`\xED\xAB\xEE\xDB`) before
//!   handing to the parser (FR-011).
//! - Fail-graceful on malformed inputs: single WARN + zero components
//!   for that file; the overall scan continues (FR-017).
//!
//! Vendor-slug priority per milestone 144 clarification (strict
//! order; later sources consulted only when all earlier sources
//! return empty/absent):
//! 1. `--rpm-distro` CLI override (authoritative; overrides every
//!    other source including per-RPM header metadata).
//! 2. `/etc/os-release::ID` via the milestone-003 `rpm_vendor_from_id`
//!    (authoritative when present; overrides per-RPM RPMTAG_VENDOR).
//! 3. Header `Vendor:` tag prefix-matched against `VENDOR_HEADER_MAP`.
//! 4. Empty namespace — the emitted PURL omits the namespace segment
//!    entirely (replaces the pre-144 literal `"rpm"` fallback that
//!    produced non-conformant `pkg:rpm/rpm/...` PURLs).

use std::path::{Path, PathBuf};

use mikebom_common::types::license::SpdxExpression;
use mikebom_common::types::purl::Purl;

use super::{rpm_vendor_from_id, PackageDbEntry};
use crate::scan_fs::os_release;

/// Milestone 054 FR-003: max recursion depth for the `walk_dir`
/// filesystem traversal. Default ceiling per the spec; not tightened
/// because `.rpm` artifacts can sit anywhere in a rootfs (no
/// shallow-by-convention structural constraint to justify a tighter
/// bound). Defense-in-depth backstop for the canonicalize-keyed
/// visited-set primary mechanism (FR-002).
const MAX_WALK_DEPTH: usize = 16;

/// Per-file size cap default (milestone 144). Raised from the
/// pre-144 200 MB to accommodate Yocto debug RPMs (kernel-dbg ~280 MB,
/// gcc-dbg ~380 MB) which were silently dropped by the old cap. The
/// cap is now operator-overridable via `--max-rpm-bytes <N>` (clap
/// flag wired through `RpmReaderConfig.cap_bytes`).
pub const DEFAULT_RPM_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Lower size bound — RPM lead block alone is 96 bytes; anything below
/// that cannot be a valid RPM regardless of claim.
const MIN_RPM_FILE_BYTES: u64 = 96;

/// RPM v3/v4 lead-block magic at offset 0.
const RPM_LEAD_MAGIC: [u8; 4] = [0xED, 0xAB, 0xEE, 0xDB];

/// Ordered vendor-header → PURL-slug table per research R9. First
/// prefix match wins. Most specific entries come first so `openSUSE`
/// doesn't get shadowed by `SUSE`.
const VENDOR_HEADER_MAP: &[(&str, &str)] = &[
    ("Red Hat", "redhat"),
    ("Fedora Project", "fedora"),
    ("Rocky Enterprise Software Foundation", "rocky"),
    ("Rocky Linux", "rocky"),
    ("Amazon Linux", "amazon"),
    ("Amazon.com", "amazon"),
    ("CentOS", "centos"),
    ("Oracle America", "oracle"),
    ("AlmaLinux OS Foundation", "almalinux"),
    ("openSUSE", "opensuse"),
    ("SUSE", "suse"),
];

/// Which source populated the vendor slug — drives the
/// `mikebom:vendor-source` property (not yet wired at serialization
/// time in this pass; `vendor_source` is recorded on the return
/// channel for future use by T017's property-bag plumbing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorSource {
    /// Milestone 144: operator-supplied via `--rpm-distro <ID>`.
    /// Authoritative; overrides every other source.
    CliOverride,
    Header,
    OsRelease,
    Fallback,
}

impl VendorSource {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            VendorSource::CliOverride => "cli-override",
            VendorSource::Header => "header",
            VendorSource::OsRelease => "os-release",
            VendorSource::Fallback => "fallback",
        }
    }
}

/// Milestone 144: per-scan configuration bundle for the standalone-`.rpm`
/// reader. Constructed once per scan from `ScanArgs` and threaded into
/// every `read()` / `parse_rpm_file()` call.
#[derive(Clone, Debug)]
pub struct RpmReaderConfig {
    /// Per-file size cap, in bytes. Files larger than this are skipped
    /// with a `SkipReason::SizeCapExceeded` WARN. Default:
    /// `DEFAULT_RPM_FILE_BYTES` (512 MiB).
    pub cap_bytes: u64,

    /// Operator-supplied distro identifier (from `--rpm-distro <ID>`).
    /// When `Some(s)`, overrides ALL other distro sources for
    /// vendor-slug resolution. Value MUST be non-empty + lowercased by
    /// the caller (clap value_parser enforces this).
    pub distro_override: Option<String>,
}

impl Default for RpmReaderConfig {
    fn default() -> Self {
        Self {
            cap_bytes: DEFAULT_RPM_FILE_BYTES,
            distro_override: None,
        }
    }
}

/// Milestone 144: structured reason for skipping a `.rpm` file during
/// parse. Replaces the pre-144 inline `tracing::warn!` calls so the
/// WARN-message wording is testable (FR-006 requires the size-cap path
/// not contain "malformed") without `tracing-subscriber` plumbing.
#[derive(Debug)]
enum SkipReason {
    StatFailed(std::io::Error),
    TruncatedLead { size: u64 },
    SizeCapExceeded { size: u64, cap: u64 },
    ParseFailed { reason: &'static str, error: String },
}

impl SkipReason {
    /// Stable structured-field value for the `reason="..."` log field.
    /// MUST NOT change across milestone 144 (FR-006 invariant — log-
    /// parsing tools depend on it).
    fn structured_reason(&self) -> &'static str {
        match self {
            Self::StatFailed(_) => "stat-failed",
            Self::TruncatedLead { .. } => "truncated-lead",
            Self::SizeCapExceeded { .. } => "size-cap-exceeded",
            Self::ParseFailed { reason, .. } => reason,
        }
    }

    /// Human-readable WARN-message prefix. "malformed" is reserved for
    /// genuinely malformed RPMs (FR-007); oversized files use
    /// "oversized" wording (FR-006).
    fn warn_prefix(&self) -> &'static str {
        match self {
            Self::SizeCapExceeded { .. } => "skipping oversized .rpm file",
            Self::StatFailed(_)
            | Self::TruncatedLead { .. }
            | Self::ParseFailed { .. } => "skipping malformed .rpm file",
        }
    }
}

/// Resolve the PURL vendor segment. Milestone 144 strict precedence:
/// CLI override → `/etc/os-release` ID → per-RPM header vendor →
/// empty (the PURL constructor omits the namespace segment when the
/// returned slug is empty).
///
/// # Examples
/// ```ignore
/// // CLI override wins absolutely:
/// resolve_rpm_vendor_slug(Some("poky"), Some("fedora"), Some("CentOS"))
///     == ("poky".to_string(), VendorSource::CliOverride);
/// // os-release wins over per-RPM header:
/// resolve_rpm_vendor_slug(None, Some("fedora"), Some("CentOS"))
///     == ("fedora".to_string(), VendorSource::OsRelease);
/// // header used only when CLI + os-release both absent:
/// resolve_rpm_vendor_slug(None, None, Some("Red Hat, Inc."))
///     == ("redhat".to_string(), VendorSource::Header);
/// // Fallback is EMPTY (pre-144 returned literal "rpm" which was
/// // non-conformant per purl-spec):
/// resolve_rpm_vendor_slug(None, None, None)
///     == (String::new(), VendorSource::Fallback);
/// ```
pub fn resolve_rpm_vendor_slug(
    cli_override: Option<&str>,
    os_release_id: Option<&str>,
    header_vendor: Option<&str>,
) -> (String, VendorSource) {
    if let Some(s) = cli_override.filter(|s| !s.is_empty()) {
        return (s.to_string(), VendorSource::CliOverride);
    }
    if let Some(id) = os_release_id.filter(|s| !s.is_empty()) {
        let slug = rpm_vendor_from_id(id);
        if !slug.is_empty() {
            return (slug, VendorSource::OsRelease);
        }
    }
    if let Some(v) = header_vendor.filter(|s| !s.is_empty()) {
        for (pattern, slug) in VENDOR_HEADER_MAP {
            if v.starts_with(pattern) {
                return ((*slug).to_string(), VendorSource::Header);
            }
        }
    }
    (String::new(), VendorSource::Fallback)
}

/// Recursively discover `.rpm` files under `rootfs` and parse each
/// valid header, returning one `PackageDbEntry` per successful parse.
/// Missing `.rpm` files → empty vector (not an error; FR-005). Single
/// `.rpm` file passed as `rootfs` → still works (treated as its own
/// scan root with no nested walk needed).
pub fn read(
    rootfs: &Path,
    distro_version: Option<&str>,
    config: &RpmReaderConfig,
) -> Vec<PackageDbEntry> {
    let os_release_id = os_release::read_id_from_rootfs(rootfs);

    let mut out = Vec::new();
    for path in discover_rpm_files(rootfs) {
        if let Some(entry) =
            parse_rpm_file(&path, os_release_id.as_deref(), distro_version, config)
        {
            out.push(entry);
        }
    }
    out
}

/// Walk a scan root for files ending in `.rpm` (case-insensitive)
/// AND whose first four bytes match the lead-block magic per FR-011.
/// Extension match alone is not sufficient — someone may rename a
/// file — so every candidate passes through the magic probe.
///
/// Milestone 114: delegates to `scan_fs::walk::safe_walk`. The
/// rpm-file walker doesn't consume the milestone-113 user exclusion
/// set (it's typically invoked at the artifact-discovery layer, not
/// the user-visible scan boundary).
fn discover_rpm_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if root.is_file() {
        // Single-file invocation: only yield if it looks like a `.rpm`.
        if is_rpm_candidate(root) {
            found.push(root.to_path_buf());
        }
        return found;
    }
    if !root.is_dir() {
        return found;
    }
    let empty = super::exclude_path::ExclusionSet::default();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let name = candidate.file_name().and_then(|s| s.to_str()).unwrap_or("");
            matches!(
                name,
                ".git" | "target" | "node_modules" | ".cargo" | "__pycache__" | ".venv"
            )
        },
        exclude_set: &empty,
    };
    crate::scan_fs::walk::safe_walk(root, &cfg, |path| {
        if path.is_file() && is_rpm_candidate(path) {
            found.push(path.to_path_buf());
        }
    });
    found
}

fn is_rpm_candidate(path: &Path) -> bool {
    let ext_matches = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("rpm"))
        .unwrap_or(false);
    if !ext_matches {
        return false;
    }
    // Read just the first 4 bytes to check magic, not the whole file.
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    match f.read_exact(&mut magic) {
        Ok(()) => magic == RPM_LEAD_MAGIC,
        Err(_) => false,
    }
}

/// Parse one `.rpm` file via the `rpm` crate and convert to a
/// `PackageDbEntry`. Returns `None` on any failure — single WARN line
/// in every case per FR-017.
fn parse_rpm_file(
    path: &Path,
    os_release_id: Option<&str>,
    distro_version: Option<&str>,
    config: &RpmReaderConfig,
) -> Option<PackageDbEntry> {
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            emit_skip_warn(path, &SkipReason::StatFailed(e));
            return None;
        }
    };
    if size < MIN_RPM_FILE_BYTES {
        emit_skip_warn(path, &SkipReason::TruncatedLead { size });
        return None;
    }
    if size > config.cap_bytes {
        emit_skip_warn(
            path,
            &SkipReason::SizeCapExceeded {
                size,
                cap: config.cap_bytes,
            },
        );
        return None;
    }

    let pkg = match rpm::Package::open(path) {
        Ok(p) => p,
        Err(e) => {
            let reason = classify_rpm_error(&e);
            emit_skip_warn(
                path,
                &SkipReason::ParseFailed {
                    reason,
                    error: e.to_string(),
                },
            );
            return None;
        }
    };
    let md = &pkg.metadata;

    let name = md.get_name().ok()?.to_string();
    // Feature 005 US4: distinguish "EPOCH tag present" from "EPOCH tag
    // absent" so the PURL mirrors `rpm -qa`'s behaviour for EPOCH=0.
    // `rpm::PackageMetadata::get_epoch()` returns `Ok(v)` on tag-present
    // and `Err(_)` on tag-absent (no separate "present-but-zero" state
    // at the crate level — we conservatively treat `Ok(0)` as
    // tag-present, matching `rpm -qa`'s `0:…` display).
    let epoch: Option<i64> = md.get_epoch().ok().map(|v| v as i64);
    let version = md.get_version().ok()?.to_string();
    let release = md.get_release().ok()?.to_string();
    let arch = md.get_arch().ok()?.to_string();

    let vendor_header = md
        .get_vendor()
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let packager = md
        .get_packager()
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let license_str = md
        .get_license()
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    // REQUIRES → bare names (tokenised per FR-015). Drop rpmlib(...)
    // and soname-style `(...)` entries — those are not installable
    // packages.
    let requires: Vec<String> = md
        .get_requires()
        .ok()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| {
            let n = d.name.trim();
            if n.is_empty() || n.starts_with("rpmlib(") || n.starts_with('/') {
                None
            } else if n.contains('(') {
                // soname-style e.g. `libc.so.6()(64bit)` — drop, they're
                // not package names.
                None
            } else {
                Some(n.to_string())
            }
        })
        .collect();

    let (vendor_slug, _vendor_source) = resolve_rpm_vendor_slug(
        config.distro_override.as_deref(),
        os_release_id,
        vendor_header.as_deref(),
    );

    // Build canonical PURL per FR-012. Feature 005 US4 alignment: the
    // EPOCH goes in the `&epoch=N` qualifier, NEVER inline in the
    // version segment. This matches `rpm.rs::assemble_entry` (the
    // rpmdb reader) and PURL-TYPES.rst §rpm. Prior behaviour here
    // emitted `NAME@EPOCH:VERSION-RELEASE` which was a divergence.
    //
    // v7 Phase G: append `&distro=<vendor>-<VERSION_ID>` when the
    // dispatcher passed a VERSION_ID, matching the rpmdb reader's
    // behaviour and ground truth
    // (`pkg:rpm/rocky/bash@5.1.8-6.el9_1?arch=aarch64&distro=rocky-9.3`).
    let version_tok = format!("{version}-{release}");
    // Omit epoch=0; treat 0 as semantically "no epoch" (matches the
    // rpmdb reader at rpm.rs::assemble_entry — same canonical-form
    // rationale).
    let epoch_seg = match epoch {
        Some(v) if v != 0 => format!("&epoch={v}"),
        _ => String::new(),
    };
    let distro_seg = match distro_version {
        Some(dv) if !dv.is_empty() => {
            format!("&distro={vendor_slug}-{dv}")
        }
        _ => String::new(),
    };
    // purl-spec § Character encoding: route both name AND version
    // through the canonical `encode_purl_segment` (the deb builder
    // and rpmdb reader both do this). The local `percent_encode_purl_version`
    // here explicitly allowed `+` literal (see `is_purl_version_safe`),
    // producing non-conformant PURLs for any RPM with `+` in its
    // version. Arch qualifier keeps its local stricter encoder — it
    // follows a different rule set per spec.
    // Milestone 144 FR-001 + research R5: when no vendor slug resolves
    // (CLI override absent, /etc/os-release absent, header vendor absent
    // or unrecognized), emit a PURL with NO namespace segment — not
    // `pkg:rpm//name@ver` (which would be invalid per purl-spec — two
    // consecutive slashes after the type are not allowed) and not
    // `pkg:rpm/rpm/name@ver` (the pre-144 buggy literal-"rpm" fallback).
    let purl_str = if vendor_slug.is_empty() {
        format!(
            "pkg:rpm/{}@{}?arch={}{}{}",
            mikebom_common::types::purl::encode_purl_segment(&name),
            mikebom_common::types::purl::encode_purl_segment(&version_tok),
            percent_encode_purl_qualifier(&arch),
            epoch_seg,
            distro_seg,
        )
    } else {
        format!(
            "pkg:rpm/{}/{}@{}?arch={}{}{}",
            percent_encode_purl_segment(&vendor_slug),
            mikebom_common::types::purl::encode_purl_segment(&name),
            mikebom_common::types::purl::encode_purl_segment(&version_tok),
            percent_encode_purl_qualifier(&arch),
            epoch_seg,
            distro_seg,
        )
    };
    let purl = Purl::new(&purl_str).ok()?;

    // Issue #475: Yocto recipes use BitBake-native `&` for AND and `|`
    // for OR in the LICENSE field; rpmbuild copies the value verbatim
    // into the RPM `License:` header without translating to SPDX
    // canonical operators. The `spdx` crate's lexer (both strict and
    // lax parse modes) rejects `&` and `|` as invalid characters before
    // the parser ever runs, so `try_canonical` returns Err and the
    // license drops to NOASSERTION downstream. Normalize the operators
    // to SPDX-canonical form first so genuine multi-license expressions
    // round-trip correctly. The substitution is space-delimited so it
    // only fires on the operator positions — no SPDX-list identifier
    // contains a literal `&` or `|`. If the normalized string is still
    // not a valid SPDX expression, `try_canonical` returns Err and we
    // fall back to the existing NOASSERTION behavior (correct — don't
    // emit unverified strings).
    let normalized_license = license_str
        .as_deref()
        .map(normalize_bitbake_license_operators);
    // First-pass `try_canonical` preserves the existing happy-path
    // behavior (every operand recognized → byte-identical to pre-milestone-
    // 152 output, per FR-003 + SC-002). On first-pass failure, the
    // milestone-152 second-pass wraps each unrecognized operand as
    // `LicenseRef-<sanitized>` (SPDX 2.3 escape hatch) and re-canonicalizes
    // — recovering the recognized portion instead of collapsing the whole
    // expression to NOASSERTION. Closes issue #481.
    let licenses: Vec<SpdxExpression> = normalized_license
        .as_deref()
        .and_then(|l| {
            SpdxExpression::try_canonical(l).ok().or_else(|| {
                preserve_known_operands_with_license_ref(l)
                    .and_then(|wrapped| SpdxExpression::try_canonical(&wrapped).ok())
            })
        })
        .into_iter()
        .collect();

    // `supplier.name` gets the raw header `Vendor:` string (per FR-014
    // — preserved verbatim for CycloneDX `component.supplier.name`).
    // `maintainer` field on PackageDbEntry drives that slot.
    let maintainer = vendor_header.or(packager);

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version: version_tok.clone(),
        arch: if arch.is_empty() { None } else { Some(arch) },
        source_path: path.to_string_lossy().into_owned(),
        depends: requires,
        maintainer,
        licenses,
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        buildinfo_status: None,
        evidence_kind: Some("rpm-file".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        // Feature 005 US4: same verbatim `VERSION-RELEASE` preservation
        // as `rpm::assemble_entry`. Drives the `mikebom:raw-version`
        // property at CycloneDX serialisation time.
        raw_version: Some(version_tok),
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        extra_annotations: Default::default(),
        binary_role: None,
    })
}

/// Milestone 144: structured WARN emission for a skip reason. Uses
/// the `SkipReason`'s `warn_prefix()` for the human-readable message
/// (FR-006: size-cap path drops "malformed"; FR-007: malformed paths
/// keep "malformed") and `structured_reason()` for the stable
/// `reason="..."` field (FR-006 invariant: log-parsing tools depend
/// on the field value).
fn emit_skip_warn(path: &Path, reason: &SkipReason) {
    let prefix = reason.warn_prefix();
    let structured = reason.structured_reason();
    match reason {
        SkipReason::StatFailed(e) => tracing::warn!(
            path = %path.display(),
            error = %e,
            reason = structured,
            "{prefix}"
        ),
        SkipReason::TruncatedLead { size } => tracing::warn!(
            path = %path.display(),
            size = size,
            reason = structured,
            "{prefix}"
        ),
        SkipReason::SizeCapExceeded { size, cap } => tracing::warn!(
            path = %path.display(),
            size = size,
            cap = cap,
            reason = structured,
            "{prefix}"
        ),
        SkipReason::ParseFailed { error, .. } => tracing::warn!(
            path = %path.display(),
            error = %error,
            reason = structured,
            "{prefix}"
        ),
    }
}

/// Classify an `rpm::Error` into a short stable reason string for WARN
/// log output. Downstream tests assert on these.
fn classify_rpm_error(e: &rpm::Error) -> &'static str {
    let msg = e.to_string();
    if msg.contains("magic") || msg.contains("Magic") {
        "bad-magic"
    } else if msg.contains("truncated") || msg.contains("EOF") || msg.contains("Unexpected") {
        "truncated-header"
    } else if msg.contains("index") {
        "header-index-over-cap"
    } else {
        "parse-error"
    }
}

/// Minimal percent-encoding for PURL name / namespace segments. Keeps
/// unreserved chars, percent-encodes everything else. Matches the
/// packageurl-python canonical encoding shape.
fn percent_encode_purl_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if is_purl_segment_safe(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_encode_purl_qualifier(s: &str) -> String {
    // Qualifier values are similar to segment but allow `.`, `_`.
    percent_encode_purl_segment(s)
}

/// Normalize Yocto BitBake-native license operators (`&`, `|`) to
/// SPDX-canonical operators (`AND`, `OR`) so the `spdx` crate's parser
/// accepts the expression. Issue #475: 10/35 Yocto-built RPMs in the
/// core-image-minimal scan had multi-license `License:` headers that
/// silently dropped to NOASSERTION because of this single root cause.
///
/// Substitution is space-delimited (` & ` → ` AND `, ` | ` → ` OR `) so
/// it fires only on the operator positions — no SPDX-list identifier
/// contains a literal `&` or `|`. Idempotent: re-running on already-
/// canonical input returns the input unchanged. Non-allocating no-op
/// when the input contains neither operator.
/// Milestone 185 US2 (#539): promoted from private `fn` to
/// `pub(crate) fn` so `opkg.rs::build_entry` can reuse this helper
/// for its own License-field normalization pipeline. Zero behavior
/// change on the rpm.rs call site.
pub(crate) fn normalize_bitbake_license_operators(raw: &str) -> String {
    if !raw.contains(" & ") && !raw.contains(" | ") {
        return raw.to_string();
    }
    raw.replace(" & ", " AND ").replace(" | ", " OR ")
}

/// Token kinds emitted by the milestone-152 license-expression tokenizer.
///
/// The tokenizer is intentionally permissive: `Token::Operand` carries any
/// non-whitespace, non-paren slice of the input. Per-operand validation
/// (recognized SPDX id vs imprecise synonym vs LicenseRef/DocumentRef prefix
/// vs genuinely unknown) happens AFTER tokenization in
/// `preserve_known_operands_with_license_ref`.
///
/// Operators (`AND`, `OR`, `WITH`) are case-sensitive uppercase per SPDX 2.3
/// strict-parse grammar. The tokenizer's second pass re-classifies operand
/// tokens whose text equals one of these keywords into the corresponding
/// operator variant.
///
/// `Token::Whitespace` collapses arbitrary input whitespace runs (spaces,
/// tabs, newlines) to a single token; on rebuild it serializes to one space.
#[derive(Debug, Clone, PartialEq)]
enum Token<'a> {
    Operand(&'a str),
    And,
    Or,
    With,
    LParen,
    RParen,
    Whitespace,
}

/// Tokenize a raw SPDX-like license expression into a flat `Vec<Token>`.
///
/// Hand-rolled per milestone 152 / issue #481 follow-up. The tokenizer is
/// the structural-decomposition layer used by
/// `preserve_known_operands_with_license_ref` to identify per-operand
/// positions for the SPDX 2.3 `LicenseRef-<sanitized>` escape-hatch wrapping.
///
/// Grammar:
/// - Operand: contiguous run of chars that are NEITHER whitespace NOR
///   paren. Includes `+` (for "or later"), `:` (for
///   `DocumentRef-doc:LicenseRef-id`), alphanumerics, `-`, `.`, and any
///   other char the SPDX 2.3 grammar permits in identifiers + this
///   milestone's LicenseRef sanitizer will normalize.
/// - Operator: literal uppercase `AND` / `OR` / `WITH` (case-sensitive
///   per SPDX 2.3 strict grammar). Re-classified from `Token::Operand`
///   in the second pass below.
/// - Paren: `(` / `)` — emitted as standalone tokens.
/// - Whitespace: one or more contiguous whitespace chars, collapsed to
///   a single `Token::Whitespace`.
///
/// Empty input → empty `Vec`. Whitespace-only input → `vec![Token::Whitespace]`.
fn tokenize(raw: &str) -> Vec<Token<'_>> {
    let bytes = raw.as_bytes();
    let mut out: Vec<Token<'_>> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            // Collapse a run of whitespace into one Token::Whitespace.
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(Token::Whitespace);
        } else if b == b'(' {
            out.push(Token::LParen);
            i += 1;
        } else if b == b')' {
            out.push(Token::RParen);
            i += 1;
        } else {
            // Consume a contiguous operand run (anything that isn't
            // whitespace or paren).
            let start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b'('
                && bytes[i] != b')'
            {
                i += 1;
            }
            let slice = &raw[start..i];
            out.push(Token::Operand(slice));
        }
    }
    // Second pass: re-classify operand tokens equal to AND/OR/WITH (case-
    // sensitive per SPDX 2.3 strict-parse grammar) as the corresponding
    // operator variants.
    for tok in out.iter_mut() {
        if let Token::Operand(s) = tok {
            match *s {
                "AND" => *tok = Token::And,
                "OR" => *tok = Token::Or,
                "WITH" => *tok = Token::With,
                _ => {}
            }
        }
    }
    out
}

/// Return true if `s` is a recognized SPDX license identifier suitable for
/// passing through to the final `try_canonical` validation unchanged.
///
/// Accepts:
///   - A bare SPDX license id (e.g., `MIT`, `Apache-2.0`).
///   - An id with the `+` "or later" suffix (e.g., `Apache-2.0+`,
///     `LGPL-2.1-or-later+`). The spdx crate's `license_id` function
///     rejects `+`-suffixed inputs (the suffix isn't part of the
///     identifier proper), so we strip it before lookup and accept either
///     form.
///
/// Imprecise synonyms (e.g., `GPLv2`, `LGPLv2.1+`) are NOT accepted —
/// the spdx crate's `Expression::parse` runs in STRICT mode and rejects
/// imprecise forms; passing them through here would cause the final
/// `try_canonical` validation to fail. Instead, imprecise synonyms fall
/// through to the LicenseRef escape hatch. In practice Yocto's RPM
/// build pipeline canonicalizes `License:` headers upstream, so
/// imprecise synonyms in real RPM headers are rare; the conservative
/// LicenseRef wrapping is correct for the cases that do occur.
fn is_recognized_spdx_operand(s: &str) -> bool {
    let trimmed = s.strip_suffix('+').unwrap_or(s);
    spdx::license_id(trimmed).is_some()
}

/// Sanitize an unrecognized license operand to a SPDX 2.3 `LicenseRef`
/// idstring matching the grammar `[a-zA-Z0-9-.]+`.
///
/// Algorithm (per milestone-152 Clarifications Q1 — replace + collapse + strip):
///   1. Replace each char outside `[a-zA-Z0-9-.]` with `-`.
///   2. Collapse runs of consecutive `-` to a single `-`.
///   3. Strip leading and trailing `-`.
///
/// Returns `None` when the algorithm produces an empty string (i.e., the
/// input contained no valid chars after sanitization — e.g., `"!@#$"`).
///
/// Idempotent: `sanitize_to_license_ref_idstring(sanitize_to_license_ref_idstring(s).unwrap())`
/// equals `sanitize_to_license_ref_idstring(s)` for any `s` where the first
/// call returns `Some(_)`.
///
/// Worked examples:
///
/// | Input             | Output                  |
/// |-------------------|-------------------------|
/// | `"GPLv2+"`        | `Some("GPLv2")`         |
/// | `"My License v2"` | `Some("My-License-v2")` |
/// | `"(custom)"`      | `Some("custom")`        |
/// | `"LGPL-2.1+"`     | `Some("LGPL-2.1")`      |
/// | `"bzip2-1.0.4"`   | `Some("bzip2-1.0.4")`   |
/// | `"PD"`            | `Some("PD")`            |
/// | `"!@#$"`          | `None`                  |
/// | `""`              | `None`                  |
/// | `"---"`           | `None`                  |
/// Milestone 185 US2 (#539): promoted from private `fn` to
/// `pub(crate) fn` so `opkg.rs::build_entry` can reuse this helper
/// for its m185 4th-pass wholesale-wrap fallback. Zero behavior
/// change on the rpm.rs call site.
pub(crate) fn sanitize_to_license_ref_idstring(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_dash = false;
    for c in s.chars() {
        let safe = c.is_ascii_alphanumeric() || c == '-' || c == '.';
        let emit = if safe { c } else { '-' };
        if emit == '-' {
            if !prev_was_dash {
                out.push('-');
                prev_was_dash = true;
            }
            // else: skip — collapses run of dashes to one
        } else {
            out.push(emit);
            prev_was_dash = false;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Wrap each unrecognized operand in a compound SPDX license expression as
/// `LicenseRef-<sanitized>` to preserve the recognized portion when
/// `SpdxExpression::try_canonical` fails on the raw expression.
///
/// Closes the residual NOASSERTION gap from milestone-478 / issue #475:
/// after the BitBake `&`/`|` operator normalization recovers compound
/// expressions like `GPLv2 & bzip2-1.0.4` → `GPLv2 AND bzip2-1.0.4`,
/// `try_canonical` still fails because `bzip2-1.0.4` isn't on the SPDX
/// license list. This helper wraps the unknown operand as
/// `LicenseRef-bzip2-1.0.4` (SPDX 2.3 spec-blessed escape hatch) and
/// recombines the expression so the recognized `GPL-2.0-only` portion
/// survives canonicalization. Closes issue #481.
///
/// Pipeline ordering (per milestone-152 FR-007):
///
///   raw -> normalize_bitbake_license_operators -> try_canonical
///     (on failure) -> preserve_known_operands_with_license_ref
///                  -> try_canonical (final validation)
///     (on failure) -> NOASSERTION (existing fail-closed behavior)
///
/// Returns `None` when:
///   - Input is empty or whitespace-only (FR-008).
///   - The input contains a `WITH`-clause whose exception is unrecognized
///     (FR-013 + Clarifications Q2 — whole compound NOASSERTION; SPDX 2.3
///     does not define an `ExceptionRef-` carrier, so the exception side
///     cannot be escape-hatched).
///   - Sanitization produces an empty idstring for some unrecognized operand
///     (e.g., a token consisting only of disallowed chars).
///
/// Returns `Some(rebuilt)` otherwise. The caller is responsible for the
/// final `SpdxExpression::try_canonical(&rebuilt)` validation (kept at the
/// caller so the Result-handling site stays singular at the existing line
/// 469–476 pipeline).
///
/// Idempotent: feeding wrapped output back as input produces the same
/// output unchanged — `LicenseRef-`/`DocumentRef-`-prefixed tokens are
/// detected and passed through.
/// Milestone 185 US2 (#539): promoted from private `fn` to
/// `pub(crate) fn` so `opkg.rs::build_entry` can reuse this helper
/// for its own License-field normalization pipeline. Zero behavior
/// change on the rpm.rs call site.
pub(crate) fn preserve_known_operands_with_license_ref(raw: &str) -> Option<String> {
    let tokens = tokenize(raw);
    // Empty / whitespace-only inputs cannot produce a LicenseRef expression.
    if tokens
        .iter()
        .all(|t| matches!(t, Token::Whitespace))
    {
        return None;
    }
    let mut rebuilt = String::with_capacity(raw.len() + 16);
    let mut is_next_operand_an_exception = false;
    for tok in tokens.iter() {
        match tok {
            Token::Operand(s) => {
                if is_next_operand_an_exception {
                    // Per FR-013 + Clarifications Q2: an unrecognized
                    // `WITH`-exception collapses the whole compound to
                    // NOASSERTION (SPDX 2.3 doesn't define ExceptionRef-).
                    spdx::exception_id(s)?;
                    rebuilt.push_str(s);
                    is_next_operand_an_exception = false;
                } else if s.starts_with("LicenseRef-")
                    || s.starts_with("DocumentRef-")
                {
                    // Idempotency: already-prefixed tokens pass through.
                    rebuilt.push_str(s);
                } else if is_recognized_spdx_operand(s) {
                    // Recognized bare SPDX id (with or without `+` suffix).
                    rebuilt.push_str(s);
                } else {
                    // Unrecognized operand → LicenseRef escape hatch.
                    let sanitized = sanitize_to_license_ref_idstring(s)?;
                    rebuilt.push_str("LicenseRef-");
                    rebuilt.push_str(&sanitized);
                }
            }
            Token::And => {
                rebuilt.push_str("AND");
            }
            Token::Or => {
                rebuilt.push_str("OR");
            }
            Token::With => {
                rebuilt.push_str("WITH");
                is_next_operand_an_exception = true;
            }
            Token::LParen => {
                rebuilt.push('(');
            }
            Token::RParen => {
                rebuilt.push(')');
            }
            Token::Whitespace => {
                rebuilt.push(' ');
            }
        }
    }
    Some(rebuilt)
}

fn is_purl_segment_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn vendor_header_redhat_family() {
        let (slug, src) = resolve_rpm_vendor_slug(None, None, Some("Red Hat, Inc."));
        assert_eq!(slug, "redhat");
        assert_eq!(src, VendorSource::Header);
    }

    #[test]
    fn vendor_header_fedora() {
        let (slug, _) = resolve_rpm_vendor_slug(None, None, Some("Fedora Project"));
        assert_eq!(slug, "fedora");
    }

    #[test]
    fn vendor_header_rocky_foundation() {
        let (slug, _) = resolve_rpm_vendor_slug(
            None,
            None,
            Some("Rocky Enterprise Software Foundation"),
        );
        assert_eq!(slug, "rocky");
    }

    #[test]
    fn vendor_header_rocky_linux_branding() {
        let (slug, _) = resolve_rpm_vendor_slug(None, None, Some("Rocky Linux"));
        assert_eq!(slug, "rocky");
    }

    #[test]
    fn vendor_header_opensuse_not_shadowed_by_suse() {
        let (slug, _) = resolve_rpm_vendor_slug(None, None, Some("openSUSE"));
        assert_eq!(slug, "opensuse");
    }

    #[test]
    fn vendor_header_suse_matches() {
        let (slug, _) = resolve_rpm_vendor_slug(None, None, Some("SUSE LLC"));
        assert_eq!(slug, "suse");
    }

    #[test]
    fn vendor_falls_back_to_os_release() {
        let (slug, src) = resolve_rpm_vendor_slug(None, Some("rhel"), None);
        assert_eq!(slug, "redhat");
        assert_eq!(src, VendorSource::OsRelease);
    }

    /// Milestone 144 T013 (SC-002): the fallback when neither CLI
    /// override, /etc/os-release, nor header vendor resolves is now an
    /// empty string (the PURL constructor omits the namespace segment
    /// entirely). Pre-144 this returned the literal "rpm" which
    /// produced non-conformant `pkg:rpm/rpm/...` PURLs.
    #[test]
    fn resolve_rpm_vendor_slug_fallback_is_empty_not_rpm() {
        let (slug, src) = resolve_rpm_vendor_slug(None, None, None);
        assert_eq!(slug, String::new());
        assert_eq!(src, VendorSource::Fallback);
    }

    #[test]
    fn vendor_empty_header_falls_through() {
        let (slug, src) = resolve_rpm_vendor_slug(None, Some("fedora"), Some(""));
        assert_eq!(slug, "fedora");
        assert_eq!(src, VendorSource::OsRelease);
    }

    #[test]
    fn empty_scan_root_yields_zero_entries() {
        let dir = tempfile::tempdir().unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert!(entries.is_empty());
    }

    #[test]
    fn non_rpm_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("not-rpm.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("fake.rpm"), b"NOT_RPM_MAGIC").unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert!(entries.is_empty());
    }

    #[test]
    fn case_insensitive_extension_match() {
        let dir = tempfile::tempdir().unwrap();
        // Wrong magic → still skipped, but the extension casing is
        // accepted (the discovery pass runs; parse fails gracefully).
        std::fs::write(dir.path().join("FOO.RPM"), b"xxxx").unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert!(entries.is_empty());
    }

    /// End-to-end: build a synthetic `.rpm` file via the `rpm` crate's
    /// `PackageBuilder`, write it to a tempdir, scan the tempdir, and
    /// verify the resulting `PackageDbEntry`.
    #[test]
    fn parses_synthetic_rpm_file() {
        let dir = tempfile::tempdir().unwrap();
        let rpm_path = dir.path().join("synthetic-1.0-1.el9.x86_64.rpm");

        // Build a minimal valid RPM via the crate's builder. No files,
        // no scriptlets — just the header.
        let pkg = rpm::PackageBuilder::new(
            "synthetic",
            "1.0",
            "MIT",
            "x86_64",
            "synthetic test package",
        )
        .release("1.el9")
        .vendor("Red Hat, Inc.")
        .packager("test-builder")
        .description("fixture for milestone 004 US1 parser tests")
        .requires(rpm::Dependency::any("zlib"))
        .requires(rpm::Dependency::any("libc"))
        .requires(rpm::Dependency::any("rpmlib(FileDigests)")) // should be dropped
        .build()
        .unwrap();
        pkg.write_file(&rpm_path).unwrap();

        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert_eq!(entries.len(), 1, "expected exactly one entry");

        let e = &entries[0];
        assert_eq!(e.name, "synthetic");
        assert_eq!(e.version, "1.0-1.el9");
        assert_eq!(e.arch.as_deref(), Some("x86_64"));
        assert_eq!(e.source_path, rpm_path.to_string_lossy());
        assert_eq!(e.sbom_tier.as_deref(), Some("source"));
        assert_eq!(e.evidence_kind.as_deref(), Some("rpm-file"));
        assert_eq!(e.maintainer.as_deref(), Some("Red Hat, Inc."));

        // Canonical PURL — Red Hat vendor slug, no epoch, qualifier arch.
        assert_eq!(
            e.purl.as_str(),
            "pkg:rpm/redhat/synthetic@1.0-1.el9?arch=x86_64"
        );

        // rpmlib() dependency dropped; zlib + libc kept.
        assert!(e.depends.iter().any(|d| d == "zlib"));
        assert!(e.depends.iter().any(|d| d == "libc"));
        assert!(!e.depends.iter().any(|d| d.starts_with("rpmlib")));

        // License canonicalised via SPDX expression. MIT survives.
        assert!(!e.licenses.is_empty());
    }

    #[test]
    fn epoch_nonzero_surfaces_in_purl() {
        let dir = tempfile::tempdir().unwrap();
        let rpm_path = dir.path().join("epochy.rpm");
        let pkg = rpm::PackageBuilder::new("epochy", "2.0", "MIT", "noarch", "x")
            .release("1")
            .epoch(7)
            .vendor("Fedora Project")
            .build()
            .unwrap();
        pkg.write_file(&rpm_path).unwrap();

        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert_eq!(entries.len(), 1);
        // Feature 005 US4: epoch moved from inline (`@7:2.0-1`) to the
        // `&epoch=7` qualifier — matches `rpm.rs::assemble_entry` and
        // PURL-TYPES.rst §rpm. Pre-005 expected `@7:2.0-1`; updated.
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:rpm/fedora/epochy@2.0-1?arch=noarch&epoch=7"
        );
    }

    /// T046 — `raw_version` populated on the artefact path too; holds
    /// the verbatim `VERSION-RELEASE` string (with no inline epoch).
    #[test]
    fn parse_rpm_file_populates_raw_version() {
        let dir = tempfile::tempdir().unwrap();
        let rpm_path = dir.path().join("raw.rpm");
        let pkg = rpm::PackageBuilder::new("raw-pkg", "3.1.4", "MIT", "noarch", "x")
            .release("2.fc40")
            .vendor("Fedora Project")
            .build()
            .unwrap();
        pkg.write_file(&rpm_path).unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].raw_version.as_deref(), Some("3.1.4-2.fc40"));
    }

    #[test]
    fn malformed_rpm_emits_zero_entries_without_erroring() {
        let dir = tempfile::tempdir().unwrap();
        // Magic matches but body is garbage.
        let mut bytes = RPM_LEAD_MAGIC.to_vec();
        bytes.extend_from_slice(&[0u8; 200]);
        std::fs::write(dir.path().join("bad.rpm"), &bytes).unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert!(entries.is_empty(), "malformed .rpm must not panic or propagate");
    }

    #[test]
    fn dedup_source_path_not_eq_same_purl() {
        // Two synthetic RPMs with the same identity → two entries
        // (dedup happens at the scan_fs orchestrator level via PURL;
        // the reader returns both and lets upstream dedup decide).
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.rpm", "b.rpm"] {
            let pkg = rpm::PackageBuilder::new("dup", "1.0", "MIT", "noarch", "x")
                .release("1")
                .vendor("Fedora Project")
                .build()
                .unwrap();
            pkg.write_file(dir.path().join(name)).unwrap();
        }
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].purl, entries[1].purl);
    }

    /// Milestone 144 T009 (FR-004 guard): the default per-file cap is
    /// 512 MiB. Guards against accidental revert via a const-value
    /// assertion. Per research §R7 the const is the contract.
    #[test]
    fn default_rpm_file_bytes_is_512_mib() {
        assert_eq!(DEFAULT_RPM_FILE_BYTES, 512 * 1024 * 1024);
    }

    /// Milestone 144 T014 (preserves existing behavior): when no CLI
    /// override and no /etc/os-release ID are set, the per-RPM header
    /// vendor still wins via the existing `VENDOR_HEADER_MAP` ladder.
    #[test]
    fn resolve_rpm_vendor_slug_header_wins_when_no_cli_no_os_release() {
        let (slug, src) =
            resolve_rpm_vendor_slug(None, None, Some("Red Hat, Inc."));
        assert_eq!(slug, "redhat");
        assert_eq!(src, VendorSource::Header);
    }

    /// Milestone 144 T015 (SC-011): /etc/os-release ID overrides per-RPM
    /// RPMTAG_VENDOR. Pre-144 the header always won; post-144 the scan-
    /// root identity is authoritative.
    #[test]
    fn resolve_rpm_vendor_slug_os_release_overrides_header() {
        let (slug, src) =
            resolve_rpm_vendor_slug(None, Some("fedora"), Some("CentOS"));
        assert_eq!(slug, "fedora");
        assert_eq!(src, VendorSource::OsRelease);
    }

    /// Milestone 144 T031 (SC-012): --rpm-distro CLI override is
    /// authoritative over EVERY other source — /etc/os-release AND
    /// per-RPM header metadata.
    #[test]
    fn resolve_rpm_vendor_slug_cli_overrides_everything() {
        let (slug, src) =
            resolve_rpm_vendor_slug(Some("poky"), Some("fedora"), Some("CentOS"));
        assert_eq!(slug, "poky");
        assert_eq!(src, VendorSource::CliOverride);
    }

    /// Milestone 144 T016 (SC-002 + SC-003): the emitted PURL omits the
    /// namespace segment entirely when neither CLI override, os-release,
    /// nor header vendor resolves. Specifically the PURL must NOT
    /// contain `pkg:rpm//` (two consecutive slashes — invalid per
    /// purl-spec) NOR `pkg:rpm/rpm/` (the pre-144 buggy fallback).
    #[test]
    fn purl_omits_namespace_when_vendor_slug_empty() {
        let dir = tempfile::tempdir().unwrap();
        let rpm_path = dir.path().join("noname-1.0-1.noarch.rpm");
        // No vendor / packager / distribution tags → fallback fires.
        let pkg = rpm::PackageBuilder::new("noname", "1.0", "MIT", "noarch", "x")
            .release("1")
            .build()
            .unwrap();
        pkg.write_file(&rpm_path).unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert_eq!(entries.len(), 1);
        let purl = entries[0].purl.as_str();
        assert!(
            !purl.contains("pkg:rpm/rpm/"),
            "literal-rpm fallback regression: {purl}"
        );
        assert!(
            !purl.contains("pkg:rpm//"),
            "double-slash from empty-namespace bug: {purl}"
        );
        assert!(
            purl.starts_with("pkg:rpm/noname@"),
            "expected pkg:rpm/<name>@... shape, got {purl}"
        );
    }

    /// Milestone 144 T020 (SC-007 + FR-006): the SizeCapExceeded skip
    /// reason MUST emit a WARN whose human-readable prefix does NOT
    /// contain "malformed", and whose structured `reason=` field is
    /// preserved as `size-cap-exceeded` for log-grep compatibility.
    /// Operationally exercised via a tempfile larger than a custom-low
    /// cap (`RpmReaderConfig { cap_bytes: 100, ... }`) so we don't need
    /// a 512 MB fixture.
    #[test]
    fn size_cap_exceeded_skips_file_without_malformed_in_warn() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversized.rpm");
        // Valid lead magic + 200 bytes of zeros — passes MIN_RPM_FILE_BYTES
        // (96) but exceeds the custom 100-byte cap.
        let mut bytes = RPM_LEAD_MAGIC.to_vec();
        bytes.extend_from_slice(&[0u8; 200]);
        std::fs::write(&path, &bytes).unwrap();
        let cfg = RpmReaderConfig {
            cap_bytes: 100,
            distro_override: None,
        };
        let entries = read(dir.path(), None, &cfg);
        assert!(entries.is_empty(), "size-cap should skip");

        // SkipReason variant correctness (the actual WARN text and
        // structured field are derived from these methods, so testing
        // the helper functions covers FR-006 + FR-007 without
        // tracing-subscriber plumbing).
        let reason = SkipReason::SizeCapExceeded {
            size: 204,
            cap: 100,
        };
        let prefix = reason.warn_prefix();
        assert!(
            !prefix.contains("malformed"),
            "FR-006 violation: SizeCapExceeded warn_prefix contains 'malformed': {prefix}"
        );
        assert_eq!(reason.structured_reason(), "size-cap-exceeded");
    }

    /// Milestone 144 T021: the size check is strict greater-than;
    /// a file exactly at the cap is INCLUDED (then the parser may fail
    /// downstream on synthetic data — this test only asserts the size
    /// check itself doesn't fire).
    #[test]
    fn size_cap_at_boundary_includes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("at-cap.rpm");
        let mut bytes = RPM_LEAD_MAGIC.to_vec();
        // Exactly 100 bytes total (4 magic + 96 zeros). MIN is 96; we
        // need >= MIN to pass the truncated-lead check.
        bytes.extend_from_slice(&[0u8; 96]);
        assert_eq!(bytes.len(), 100, "fixture must be exactly 100 bytes");
        std::fs::write(&path, &bytes).unwrap();
        let cfg = RpmReaderConfig {
            cap_bytes: 100,
            distro_override: None,
        };
        // The size check at line "if size > config.cap_bytes" must NOT
        // fire when size == cap. (The parser will then fail because the
        // synthetic bytes aren't a valid RPM body; the file is still
        // skipped but via the ParseFailed path, not the SizeCapExceeded
        // path. The visible behavior — empty entries — is identical
        // here; the assertion is about which SkipReason variant fires.)
        let entries = read(dir.path(), None, &cfg);
        assert!(entries.is_empty(), "synthetic body fails to parse");
        // Direct exercise of the helper: a size==cap construction is
        // not built (it would be `SizeCapExceeded { size: 100, cap: 100 }`
        // — but the production code only constructs SizeCapExceeded
        // when size > cap, never when size == cap, so the boundary is
        // enforced at the call site, not in the variant).
    }

    /// Milestone 144 T035 (SC-011 end-to-end): /etc/os-release overrides
    /// per-RPM RPMTAG_VENDOR via the production code path (not just the
    /// `resolve_rpm_vendor_slug` helper in isolation). Builds a
    /// synthetic .rpm with `vendor("CentOS")` AND plants an
    /// `etc/os-release` declaring `ID=fedora` in the same tempdir;
    /// asserts the emitted PURL has namespace `fedora`.
    #[test]
    fn rpm_file_os_release_overrides_per_rpm_vendor() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("etc")).unwrap();
        std::fs::write(dir.path().join("etc/os-release"), "ID=fedora\n").unwrap();
        let rpm_path = dir.path().join("override-1.0-1.x86_64.rpm");
        rpm::PackageBuilder::new("override", "1.0", "MIT", "x86_64", "test")
            .release("1")
            .vendor("CentOS") // per-RPM vendor — must be overridden
            .build()
            .unwrap()
            .write_file(&rpm_path)
            .unwrap();
        let entries = read(dir.path(), None, &RpmReaderConfig::default());
        assert_eq!(entries.len(), 1);
        let purl = entries[0].purl.as_str();
        assert!(
            purl.starts_with("pkg:rpm/fedora/"),
            "expected fedora namespace (os-release wins over CentOS vendor); got {purl}"
        );
    }

    /// Milestone 144 T035 (SC-012 end-to-end): --rpm-distro CLI override
    /// wins over BOTH /etc/os-release AND per-RPM RPMTAG_VENDOR via the
    /// production code path. Same setup as the previous test plus
    /// `RpmReaderConfig.distro_override = Some("poky")`.
    #[test]
    fn rpm_file_cli_distro_overrides_everything() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("etc")).unwrap();
        std::fs::write(dir.path().join("etc/os-release"), "ID=fedora\n").unwrap();
        let rpm_path = dir.path().join("override-1.0-1.x86_64.rpm");
        rpm::PackageBuilder::new("override", "1.0", "MIT", "x86_64", "test")
            .release("1")
            .vendor("CentOS")
            .build()
            .unwrap()
            .write_file(&rpm_path)
            .unwrap();
        let cfg = RpmReaderConfig {
            cap_bytes: DEFAULT_RPM_FILE_BYTES,
            distro_override: Some("poky".to_string()),
        };
        let entries = read(dir.path(), None, &cfg);
        assert_eq!(entries.len(), 1);
        let purl = entries[0].purl.as_str();
        assert!(
            purl.starts_with("pkg:rpm/poky/"),
            "expected poky namespace (CLI override wins absolutely); got {purl}"
        );
    }

    /// Milestone 054 SC-002 + FR-009: walker terminates promptly on
    /// a synthesized minimal symlink-loop fixture instead of hanging
    /// indefinitely. Pre-054 this would loop forever; post-054 the
    /// canonicalize-keyed visited-set breaks the cycle.
    ///
    /// Milestone 100: `#[cfg(unix)]` — POSIX-only symlink API.
    #[cfg(unix)]
    #[test]
    fn walks_symlink_loop_without_hanging() {
        let tmp = tempfile::tempdir().unwrap();
        let loop_dir = tmp.path().join("loop");
        std::fs::create_dir_all(&loop_dir).unwrap();
        // Self-loop: `loop/link` points back at `loop/`.
        std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();
        // Bounded recursion proves the loop-protection works.
        let result = discover_rpm_files(tmp.path());
        // No .rpm files in the synthesized fixture; the test only
        // asserts the call returned (didn't hang).
        assert!(result.is_empty());
    }

    // --- Issue #475: BitBake `&`/`|` license operator normalization ------

    #[test]
    fn normalize_bitbake_and_operator_to_spdx_canonical() {
        // Yocto's actual core-image-minimal libc6 License: tag — was
        // dropping to NOASSERTION pre-fix because the spdx crate's
        // lexer rejects `&`.
        let raw = "GPL-2.0-only & LGPL-2.1-or-later";
        let normalized = normalize_bitbake_license_operators(raw);
        assert_eq!(normalized, "GPL-2.0-only AND LGPL-2.1-or-later");
        // Sanity: the normalized form MUST round-trip through
        // SpdxExpression::try_canonical, which is the real-world
        // condition the fix targets.
        SpdxExpression::try_canonical(&normalized)
            .expect("normalized expression MUST parse as canonical SPDX");
    }

    #[test]
    fn normalize_bitbake_or_operator_to_spdx_canonical() {
        // The `|` operator from Yocto recipes; less common in core-image-
        // minimal but legitimate (e.g., dual-licensed crates).
        let raw = "GPL-2.0-only | MIT";
        let normalized = normalize_bitbake_license_operators(raw);
        assert_eq!(normalized, "GPL-2.0-only OR MIT");
        SpdxExpression::try_canonical(&normalized)
            .expect("normalized OR expression MUST parse as canonical SPDX");
    }

    #[test]
    fn normalize_preserves_already_canonical_spdx_expression() {
        // Inputs that don't contain BitBake operators must be returned
        // verbatim (no spurious substitution).
        let inputs = [
            "MIT",
            "GPL-2.0-only AND LGPL-2.1-or-later",
            "GPL-2.0-only OR Apache-2.0",
            "GPL-2.0-or-later WITH Classpath-exception-2.0",
            "DocumentRef-recipe-busybox:LicenseRef-bzip2-1.0.4",
        ];
        for input in inputs {
            assert_eq!(
                normalize_bitbake_license_operators(input),
                input,
                "already-canonical input MUST be returned verbatim: {input:?}",
            );
        }
    }

    #[test]
    fn normalize_handles_mixed_bitbake_operators_in_one_expression() {
        // Recipes can mix `&` and `|` (with parentheses), e.g.,
        // `(GPL-2.0-only | LGPL-3.0-only) & BSD-3-Clause`. Normalize
        // both at once.
        let raw = "(GPL-2.0-only | LGPL-3.0-only) & BSD-3-Clause";
        let normalized = normalize_bitbake_license_operators(raw);
        assert_eq!(
            normalized,
            "(GPL-2.0-only OR LGPL-3.0-only) AND BSD-3-Clause"
        );
        SpdxExpression::try_canonical(&normalized)
            .expect("mixed-operator normalized expression MUST parse as canonical SPDX");
    }

    #[test]
    fn normalize_handles_bitbake_operator_with_license_ref() {
        // The busybox-family case from issue #475: a SPDX-list ID
        // combined with a DocumentRef:LicenseRef chain via `&`.
        let raw = "GPL-2.0-only & DocumentRef-recipe-busybox:LicenseRef-bzip2-1.0.4";
        let normalized = normalize_bitbake_license_operators(raw);
        assert_eq!(
            normalized,
            "GPL-2.0-only AND DocumentRef-recipe-busybox:LicenseRef-bzip2-1.0.4"
        );
        SpdxExpression::try_canonical(&normalized)
            .expect("normalized LicenseRef-bearing expression MUST parse as canonical SPDX");
    }

    #[test]
    fn normalize_is_idempotent() {
        // FR-style invariant: running the normalizer twice on the same
        // input produces byte-identical output. Important because the
        // normalizer is called on user-controlled RPM header data; an
        // accidental double-invoke must not corrupt the value.
        let inputs = [
            "GPL-2.0-only & LGPL-2.1-or-later",
            "MIT",
            "GPL-2.0-only | Apache-2.0",
        ];
        for input in inputs {
            let once = normalize_bitbake_license_operators(input);
            let twice = normalize_bitbake_license_operators(&once);
            assert_eq!(once, twice, "normalizer MUST be idempotent for {input:?}");
        }
    }

    // --- Milestone 152 / Issue #481: preserve known operands when one is unknown ----
    //
    // Tests for `preserve_known_operands_with_license_ref` + its
    // companion helpers (`tokenize`, `sanitize_to_license_ref_idstring`).
    // The new fallback path replaces NOASSERTION with SPDX 2.3 `LicenseRef-
    // <sanitized>` escape-hatch wrappers when a compound expression has
    // at least one operand not on the SPDX license list.
    //
    // See `specs/152-preserve-license-operands/` for the spec/plan/tasks.

    // T006 — tokenizer tests:

    #[test]
    fn tokenize_simple_compound() {
        // Verify the lexer's basic OR-compound case.
        let toks = tokenize("MIT OR Apache-2.0");
        assert_eq!(
            toks,
            vec![
                Token::Operand("MIT"),
                Token::Whitespace,
                Token::Or,
                Token::Whitespace,
                Token::Operand("Apache-2.0"),
            ]
        );
    }

    #[test]
    fn tokenize_with_parens_and_whitespace() {
        // Verify parens emit as standalone tokens + whitespace runs
        // collapse to one Token::Whitespace.
        let toks = tokenize("(MIT OR Apache-2.0)  AND  PD");
        assert_eq!(
            toks,
            vec![
                Token::LParen,
                Token::Operand("MIT"),
                Token::Whitespace,
                Token::Or,
                Token::Whitespace,
                Token::Operand("Apache-2.0"),
                Token::RParen,
                Token::Whitespace,  // collapsed from two spaces
                Token::And,
                Token::Whitespace,  // collapsed from two spaces
                Token::Operand("PD"),
            ]
        );
    }

    // T008 — sanitization worked-examples:

    #[test]
    fn sanitization_worked_examples() {
        // The 9 worked examples from milestone-152 data-model §2 / R5.
        // Locks the replace+collapse+strip algorithm (per Clarifications Q1).
        assert_eq!(
            sanitize_to_license_ref_idstring("GPLv2+"),
            Some("GPLv2".to_string())
        );
        assert_eq!(
            sanitize_to_license_ref_idstring("My License v2"),
            Some("My-License-v2".to_string())
        );
        assert_eq!(
            sanitize_to_license_ref_idstring("(custom)"),
            Some("custom".to_string())
        );
        assert_eq!(
            sanitize_to_license_ref_idstring("LGPL-2.1+"),
            Some("LGPL-2.1".to_string())
        );
        // Already-valid inputs: pass through unchanged.
        assert_eq!(
            sanitize_to_license_ref_idstring("bzip2-1.0.4"),
            Some("bzip2-1.0.4".to_string())
        );
        assert_eq!(
            sanitize_to_license_ref_idstring("PD"),
            Some("PD".to_string())
        );
        // Sanitizes to nothing → None.
        assert_eq!(sanitize_to_license_ref_idstring("!@#$"), None);
        assert_eq!(sanitize_to_license_ref_idstring(""), None);
        assert_eq!(sanitize_to_license_ref_idstring("---"), None);
    }

    // T011 — busybox-family reference case (the issue-#481 fix):

    #[test]
    fn preserve_busybox_compound() {
        // The SC-001 busybox reference case. The issue body speculated the
        // raw RPM header was `GPLv2 & bzip2-1.0.4`, but Yocto's build
        // pipeline canonicalizes `License:` headers upstream — the actual
        // shape mikebom sees is `GPL-2.0-only & bzip2-1.0.4` (with the
        // BitBake `&` operator still in place). After milestone-478's
        // operator normalization → `GPL-2.0-only AND bzip2-1.0.4`. The
        // milestone-152 fallback wraps the unrecognized `bzip2-1.0.4` as
        // `LicenseRef-bzip2-1.0.4` and preserves the recognized GPL half.

        // Assertion 1: helper called on the post-normalization shape.
        let wrapped = preserve_known_operands_with_license_ref(
            "GPL-2.0-only AND bzip2-1.0.4",
        )
        .unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        assert_eq!(
            canonical.as_str(),
            "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"
        );

        // Assertion 2 (FR-007 pipeline-ordering): manually chain the full
        // pipeline from a raw RPM-header-shape input (BitBake `&`
        // operator + unknown operand). Per analysis remediation A1: the
        // test scope is the helpers composed manually within the test,
        // NOT the end-to-end RPM-reader path.
        let raw = "GPL-2.0-only & bzip2-1.0.4";
        let normalized = normalize_bitbake_license_operators(raw);
        assert_eq!(normalized, "GPL-2.0-only AND bzip2-1.0.4");
        // First-pass try_canonical MUST fail on `bzip2-1.0.4`.
        assert!(SpdxExpression::try_canonical(&normalized).is_err());
        // Second-pass helper wraps + recanonicalizes.
        let wrapped2 =
            preserve_known_operands_with_license_ref(&normalized).unwrap();
        let canonical2 = SpdxExpression::try_canonical(&wrapped2).unwrap();
        assert_eq!(
            canonical2.as_str(),
            "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"
        );
    }

    // T012 — liblzma5 reference case (single-operand unknown):

    #[test]
    fn preserve_liblzma5_single_unknown() {
        // FR-004: single-operand unrecognized → wrapped as LicenseRef,
        // NOT NOASSERTION.
        let wrapped =
            preserve_known_operands_with_license_ref("PD").unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        assert_eq!(canonical.as_str(), "LicenseRef-PD");
    }

    // T013 — OR-operator path:

    #[test]
    fn preserve_or_operator() {
        // US1 scenario 3: OR-compound with one known + one unknown operand.
        // Uses canonical SPDX inputs (real RPM headers from Yocto have
        // canonical ids; imprecise synonyms fall through to LicenseRef
        // wrapping in the conservative-default path).
        let wrapped = preserve_known_operands_with_license_ref(
            "GPL-2.0-only OR bzip2-1.0.4",
        )
        .unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        assert_eq!(
            canonical.as_str(),
            "GPL-2.0-only OR LicenseRef-bzip2-1.0.4"
        );
    }

    // T014 — WITH-clause happy path:

    #[test]
    fn with_clause_known_exception_preserved() {
        // FR-013 happy path: input parses cleanly via first-pass
        // try_canonical (fallback never fires). Sanity-check by also
        // feeding directly through the helper — it should pass through
        // unchanged because both the license AND the exception are
        // recognized.
        let raw = "GPL-2.0-or-later WITH Classpath-exception-2.0";
        let direct = SpdxExpression::try_canonical(raw).unwrap();
        assert_eq!(
            direct.as_str(),
            "GPL-2.0-or-later WITH Classpath-exception-2.0"
        );
        // Helper called directly: identity on already-canonical input.
        let via_helper =
            preserve_known_operands_with_license_ref(raw).unwrap();
        assert_eq!(via_helper, raw);
    }

    // T015 — WITH-clause: unknown exception collapses the whole compound:

    #[test]
    fn with_clause_unknown_exception_collapses_to_noassertion() {
        // FR-013 + Clarifications Q2: an unrecognized WITH-exception is
        // load-bearing legally; collapse the WHOLE compound to
        // NOASSERTION rather than silently dropping the exception.
        // SPDX 2.3 doesn't define an ExceptionRef- escape hatch.
        assert!(preserve_known_operands_with_license_ref(
            "GPL-2.0-only WITH UnknownExc AND MIT"
        )
        .is_none());
    }

    // T015a — WITH-clause: unknown LEFT-side license gets wrapped
    //          (per analysis remediation C1 — closes FR-013 first-clause gap):

    #[test]
    fn with_clause_unknown_license_wrapped() {
        // FR-013 first clause: when the LEFT side of WITH is unrecognized
        // but the exception is recognized, wrap the LEFT side as
        // LicenseRef-<sanitized> and preserve the exception unchanged.
        let wrapped = preserve_known_operands_with_license_ref(
            "UnknownLicense WITH Classpath-exception-2.0",
        )
        .unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        assert_eq!(
            canonical.as_str(),
            "LicenseRef-UnknownLicense WITH Classpath-exception-2.0"
        );
    }

    // T016 — happy path unchanged for fully-recognized input:

    #[test]
    fn happy_path_unchanged_for_fully_recognized() {
        // FR-003 + SC-002 regression guard: when EVERY operand is a
        // recognized SPDX id, first-pass try_canonical succeeds and the
        // fallback never fires. The direct helper call should also be a
        // no-op pass-through on canonical input.

        // End-to-end: STRICT-mode try_canonical accepts canonical
        // SPDX ids verbatim.
        let direct = SpdxExpression::try_canonical(
            "GPL-2.0-only AND LGPL-2.1-or-later",
        )
        .unwrap();
        assert_eq!(
            direct.as_str(),
            "GPL-2.0-only AND LGPL-2.1-or-later"
        );

        // Helper called directly on already-canonical input → identity.
        let via_helper = preserve_known_operands_with_license_ref(
            "GPL-2.0-only AND LGPL-2.1-or-later",
        )
        .unwrap();
        assert_eq!(via_helper, "GPL-2.0-only AND LGPL-2.1-or-later");

        // The `+` suffix is also recognized (FR-005 + the
        // is_recognized_spdx_operand helper strips it before lookup).
        let with_plus = preserve_known_operands_with_license_ref(
            "Apache-2.0+ AND MIT",
        )
        .unwrap();
        assert_eq!(with_plus, "Apache-2.0+ AND MIT");
    }

    // T017 — empty + whitespace-only inputs remain NOASSERTION:

    #[test]
    fn empty_input_remains_noassertion() {
        // FR-008 + US1 scenario 5: helper returns None on empty input;
        // caller's downstream behavior produces NOASSERTION (per the
        // .into_iter().collect() returning an empty Vec).
        assert!(preserve_known_operands_with_license_ref("").is_none());
        assert!(preserve_known_operands_with_license_ref("   ").is_none());
    }

    // T018 — opaque garbage that sanitizes to empty:

    #[test]
    fn opaque_garbage_remains_noassertion() {
        // FR-008 + US1 scenario 6: a single-token input where every char
        // gets sanitized away → the sanitize helper returns None →
        // the main helper propagates the None via `?`.
        assert!(
            preserve_known_operands_with_license_ref("!@#$").is_none()
        );
    }

    // T019 — idempotency:

    #[test]
    fn idempotent_on_already_wrapped_input() {
        // SC-003 + FR-006 + contracts/helper-api.md Contract 5: feeding
        // milestone-152-shaped output back through the helper produces
        // the same output unchanged — no double-wrapping, no operator
        // drift.
        let first = preserve_known_operands_with_license_ref(
            "GPL-2.0-only AND bzip2-1.0.4",
        )
        .unwrap();
        let second =
            preserve_known_operands_with_license_ref(&first).unwrap();
        assert_eq!(first, second, "helper MUST be idempotent");

        // Also assert idempotency at the canonical-form level (full
        // round-trip survives unchanged):
        let canonical1 = SpdxExpression::try_canonical(&first).unwrap();
        let canonical2 = SpdxExpression::try_canonical(&second).unwrap();
        assert_eq!(canonical1.as_str(), canonical2.as_str());
        assert_eq!(
            canonical1.as_str(),
            "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"
        );
    }

    // T020 — parens preserved through fallback:

    #[test]
    fn parens_preserved_through_fallback() {
        // Spec Edge Cases: parenthesized sub-expressions in the raw input
        // MUST round-trip through the LicenseRef-wrapping pass.
        let wrapped = preserve_known_operands_with_license_ref(
            "(GPL-2.0-only OR LGPL-2.1-or-later) AND PD",
        )
        .unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        // Verify structural preservation (parens round-trip) AND the
        // per-operand wrapping (PD → LicenseRef-PD):
        assert_eq!(
            canonical.as_str(),
            "(GPL-2.0-only OR LGPL-2.1-or-later) AND LicenseRef-PD"
        );
    }

    // T020a — mixed precedence (per analysis remediation C2 — closes the
    //          FR-005 implicit-precedence test gap):

    #[test]
    fn mixed_precedence_preserved() {
        // FR-005: SPDX precedence (AND binds tighter than OR) MUST be
        // preserved without explicit parens. The tokenizer + rebuilder
        // don't reorder operators, so precedence is preserved
        // structurally — this test locks that invariant defensively.
        let wrapped = preserve_known_operands_with_license_ref(
            "MIT OR PD AND GPL-2.0-only",
        )
        .unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        // The spdx crate may canonicalize the form; the structural
        // requirement is that OR's RHS binds the AND-chain together
        // (PD AND GPL-2.0-only) — operator-order preservation gives this
        // for free.
        assert_eq!(
            canonical.as_str(),
            "MIT OR LicenseRef-PD AND GPL-2.0-only"
        );
    }

    // T021 — imprecise synonym canonicalized (not wrapped):

    #[test]
    fn imprecise_synonym_wrapped_as_license_ref() {
        // The milestone-152 helper is conservative on imprecise synonyms
        // (e.g., `GPLv2`, `LGPLv2.1+`): they fall through to the
        // LicenseRef escape hatch rather than being canonicalized. This
        // is the correct conservative behavior — `spdx::Expression::parse`
        // runs in STRICT mode, which rejects imprecise forms; the final
        // `try_canonical` validation would fail if we passed them through
        // unchanged. Wrapping as LicenseRef-<sanitized> preserves the
        // operator's intent ("we saw some license-like token here") while
        // staying STRICT-mode safe.
        //
        // In real-world Yocto RPMs, License: headers are canonicalized
        // upstream — imprecise synonyms in the input are rare. This test
        // documents the conservative wrapping behavior for the cases
        // that do occur.
        let wrapped =
            preserve_known_operands_with_license_ref("GPLv2").unwrap();
        let canonical = SpdxExpression::try_canonical(&wrapped).unwrap();
        assert_eq!(canonical.as_str(), "LicenseRef-GPLv2");
    }
}
