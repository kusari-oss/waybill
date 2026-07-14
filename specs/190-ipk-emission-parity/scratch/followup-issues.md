# m190 follow-up issue drafts

Per spec Assumptions bullet + Q4 answer A (Session 2026-07-13), the epoch fix
scope was deliberately restricted to opkg (ipk reader). dpkg (deb) and apk
readers may harbor the same class of bug and should be audited independently.
These drafts SHOULD be filed as GitHub issues after m190 lands (post-merge).

---

## Draft 1 — dpkg audit

**Title**: sbom scan (dpkg): audit for inline-epoch PURL bug (m190 follow-up to #552)

**Body**:

Analogous to #552 (opkg): verify whether the dpkg reader emits epoch-prefixed
Debian versions (`<digits>:<version>-<release>`) as `?epoch=<N>` PURL qualifiers
per purl-spec, or embeds them inline in the version segment.

## Repro

```bash
# Build a synthetic .deb with an epoch (e.g., Version: 1:2.0-r0) and place
# it in a scan directory.
mikebom sbom scan --offline --format cyclonedx-json --path /tmp/deb-epoch/ --output /tmp/out.json
jq '.components[] | select(.name == "test") | {name, version, purl}' /tmp/out.json
# Observe: check if `version` starts with `1:` and if `purl` contains `@1:...`.
# Expected form: version=`2.0-r0`, purl=`pkg:deb/<distro>/test@2.0-r0?arch=<arch>&epoch=1`.
```

## Reference implementation

m190 (opkg-side) uses `parse_opkg_version_with_epoch` in
`mikebom-cli/src/scan_fs/package_db/ipk_file.rs` + extends `build_opkg_purl`
to accept an `Option<u32>` epoch. dpkg reader should mirror the pattern
against `mikebom-cli/src/scan_fs/package_db/dpkg.rs` (or the equivalent
file where the deb PURL is built).

## Related

- #552 (closed by m190) — opkg epoch handling.
- rpm reader emission pattern: `mikebom-cli/src/scan_fs/package_db/rpm_file.rs:397-411`.

---

## Draft 2 — apk audit

**Title**: sbom scan (apk): audit for inline-epoch PURL bug (m190 follow-up to #552)

**Body**:

Analogous to #552 (opkg): verify whether the apk (Alpine `.apk`) reader
emits epoch-prefixed versions correctly as `?epoch=<N>` PURL qualifiers.

Apk uses `<epoch>:<upstream>-r<release>` style versioning in some contexts.
Verify current behavior and, if inline-epoch is emitted, mirror the m190
opkg-side fix in `mikebom-cli/src/scan_fs/package_db/apk*.rs`.

## Repro

Similar to Draft 1 but with a synthetic `.apk`.

## Reference implementation

Same as Draft 1 — the `parse_opkg_version_with_epoch` helper is a small
pure function; port to the apk reader if apk versions can carry an
epoch prefix in the wire format.

---

## Filing action

When m190 merges:
1. Open GitHub issue with Draft 1 title + body.
2. Open GitHub issue with Draft 2 title + body.
3. Cross-link both to m190's PR in the "Follow-up work" section of the PR body.
