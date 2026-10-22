//! The source package an OS binary package was built from.
//!
//! Every OS packaging system records this, under a different name:
//! apk's `o:` (origin) field, dpkg's `Source:` control field, rpm's
//! `SOURCERPM` tag, opkg's `Source:`. Until #1248 all four readers
//! parsed past it.
//!
//! It matters because OS advisory feeds are published against the
//! *source* name, not the installed one. `node:24.14.1-alpine` ships
//! `libcrypto3` and `libssl3` at `3.5.6-r0`, both with origin
//! `openssl`; querying OSV for either installed name returns nothing,
//! and for `openssl` returns 25 advisories including a Critical one
//! that the image is in range for. The same holds on deb
//! (`libssl3` → 0, `openssl` → 52) and rpm (`openssl-libs` → 0,
//! `openssl` → 12).
//!
//! # Why a PURL and not a bare name
//!
//! purl-spec defines no `upstream` qualifier for `apk`, `deb` or
//! `rpm` — their type definitions declare only `arch` (and `epoch`
//! for rpm). What it *does* define is `arch=source`, described in the
//! deb type definition as: "The special value arch=source identifies
//! a source package." So the source package has a spec-native
//! identifier of its own, and it is exactly the one the advisory
//! records carry — OSV's Alpine entries are keyed
//! `pkg:apk/alpine/openssl?arch=source`, Debian's
//! `pkg:deb/debian/openssl?arch=source&distro=bookworm`.
//!
//! Emitting that PURL rather than a name therefore hands a consumer
//! the advisory feed's own key, and needs no new vocabulary. Each
//! reader builds it with the PURL builder it already has, passing
//! `arch=source` in place of the machine architecture.
//!
//! Syft instead appends a non-spec `upstream=<name>` qualifier to the
//! installed package's own PURL. That was considered and declined:
//! it mutates the identifier waybill uses as a component's identity
//! and dedup key, and it carries no source version.

/// Per-component annotation carrying the source package's PURL.
///
/// Read natively by the SPDX 2.3 and SPDX 3 emitters, which have a
/// slot for an additional package identifier; carried as a property
/// in CycloneDX, which does not.
pub const ANN_SOURCE_PACKAGE: &str = "waybill:source-package";

/// The comment both SPDX emitters attach to the source-package
/// identifier, and that the parity extractors match on to tell it
/// apart from the installed package's own PURL in the same array.
///
/// Shared so the emitters and the extractors cannot drift: an
/// extractor keyed on a copy of this string would silently read empty
/// if an emitter's wording changed, and a parity row that reads empty
/// on both sides compares equal.
pub const SOURCE_PACKAGE_COMMENT: &str = "source package this binary package was built from";

/// Record the source package's PURL on a component.
///
/// Emits whenever the reader found the field, **including when the
/// source name equals the installed name** (the common case —
/// `zlib`, `busybox` and `musl` are each their own origin). Dropping
/// the equal case would make the annotation's absence ambiguous
/// between "built from itself" and "the packaging metadata did not
/// say", which is the distinction a consumer needs in order to know
/// whether it may fall back to the installed name.
///
/// `source_purl` must already be a well-formed PURL carrying
/// `arch=source`; callers build it with their own ecosystem's
/// builder. A malformed value is dropped rather than emitted, since a
/// consumer cannot do anything with an identifier it cannot parse.
pub fn annotate(
    annotations: &mut std::collections::BTreeMap<String, serde_json::Value>,
    source_purl: &str,
) {
    if waybill_common::types::purl::Purl::new(source_purl).is_err() {
        tracing::warn!(
            purl = source_purl,
            "source-package PURL is malformed; dropping the annotation"
        );
        return;
    }
    annotations.insert(
        ANN_SOURCE_PACKAGE.to_string(),
        serde_json::Value::String(source_purl.to_string()),
    );
}
