# Data model — nixpkgs security declarations as VEX

All state is in-process for one scan, matching every milestone since 002.
Nothing here persists; it is emitted and dropped.

## `DeclarationSource`

Which package set a candidate attribute was found in, and therefore how it was
reached.

| Variant | Meaning |
|---|---|
| `TopLevel` | `pkgs.<pname>` |
| `Haskell` | `pkgs.haskellPackages.<pname>` |
| `Python3` | `pkgs.python3Packages.<pname>` |
| `Perl` | `pkgs.perlPackages.<pname>` |

Ordered: the first set that yields a *confirmed* attribute wins. Recorded so a
consumer can see which namespace answered, and so coverage can be reported per
set as R1 measures it.

## `AttributeResolution`

The outcome of trying to reach one closure member's declaration. Every member
gets exactly one.

| Variant | Meaning |
|---|---|
| `Confirmed { source, declarations }` | An attribute was found and its output path matched the member's. Its declarations, possibly empty, are authoritative for this member. |
| `PathMismatch` | An attribute of that name exists but builds something else. **The declaration is discarded.** Measured at 9% of members; these are the false attributions FR-001b prevents. |
| `NoAttribute` | No candidate in any probed set. Measured at 18%. |

`PathMismatch` and `NoAttribute` are both *unchecked* for reporting purposes
(FR-001c) and MUST NOT be reported as "carries no declaration". They are kept
distinct internally because they say different things about why: one is a
name collision, the other a reach limit, and a future milestone widening the
probed sets will move members between them.

## `Declaration`

One `meta.knownVulnerabilities` entry.

| Field | Notes |
|---|---|
| `text` | Verbatim. FR-010a — the maintainer's words are the value. |
| `identifiers` | Advisory identifiers extracted from `text`; may be empty. Extraction MUST NOT consume the text (FR-003). Recognised shapes (FR-011a): `CVE-YYYY-N`, `GHSA-xxxx-xxxx-xxxx`, and `<Vendor>-YYYY-N` (e.g. `Sonatype-2015-0286`). |

A declaration with a non-empty `identifiers` produces VEX (FR-006). A
declaration with an empty `identifiers` produces the per-component annotation
(FR-010). A declaration naming several identifiers produces one statement each
(FR-004) and is not split for the annotation.

*Amended by #1051.* This field was `cves` and matched `CVE-YYYY-N` only, so a
declaration naming a GHSA or a vendor advisory was filed as prose and reached
the SBOM annotation rather than VEX. The shapes were chosen from a whole-tree
census (`measurements/kv-identifier-census.py`, README Q5): at nixpkgs
`a799d3e3`, those three shapes cover every identifier among 159 entries, and
none of them matches anything in the 36 prose entries.

## `EvidenceGrade` — extended, not replaced

Existing at `nix/closure/patches.rs`:

| Variant | Wire | Meaning |
|---|---|---|
| `FilenameDerived` | `filename-derived` | A CVE read out of a patch filename (milestone 1035). |
| **`NixpkgsDeclared`** | `nixpkgs-declared` | **New.** A maintainer assertion in `meta.knownVulnerabilities`. |

The enum was built single-variant specifically so a stronger provenance could
be distinguished (R7). `GradedCve` has no grade-less constructor, so the new
source inherits that enforcement rather than restating it.

Ordering matters and is encoded: `NixpkgsDeclared` outranks `FilenameDerived`,
which is what FR-012's reconciliation consults. It is a provenance ordering,
not a confidence score — no number is implied because none was measured.

## `ReconciliationOutcome`

Produced when a declaration and a milestone-1035 patch statement name one CVE
on one component.

| Field | Notes |
|---|---|
| `component` | The subject both spoke about. |
| `cve` | The identifier both named. |
| `withheld` | The patch-derived `not_affected` that was not emitted. |

Counted at document scope (FR-013a) so "no patch statement produced" stays
distinguishable from "a patch statement withheld". The patch itself remains in
`pedigree.patches[]`, which milestone 1035 emits independently of VEX — what
is withheld is the suppression, not the evidence (FR-013).

## `NixpkgsSecuritySummary`

The document-scope record, sibling to milestone 1035's `NixClosureSummary` and
built the same way.

| Field | Drives |
|---|---|
| `members_checked` | SC-007a |
| `members_unchecked` | FR-001d, SC-007a |
| `confirmed_by_set` | R1's per-set breakdown |
| `declarations_total` | |
| `declarations_without_cve` | FR-011 — CVE-only, unchanged by FR-011a |
| `distinct_cves` | CVE-only, unchanged by FR-011a |
| `declarations_without_identifier` | FR-011a — declarations naming no recognised identifier: the ones that remain prose |
| `distinct_identifiers` | FR-011a — every distinct identifier, CVEs included |
| `reconciliations_withheld` | FR-013a, SC-005a |
| `accepted_insecure` | FR-015 — whether any confirmed member carried a declaration at all |

`accepted_insecure` is derived rather than observed: a member carrying a
declaration is in the closure, and Nix refuses to evaluate such a package
unless permitted, so its presence *is* the permission (FR-015). The field
records that inference, and FR-016 bounds what may be said about it — a
blanket `NIXPKGS_ALLOW_INSECURE=1` is indistinguishable from a targeted
permission, so the claim is about the build, never about the operator's
intent.

## Relationships to existing types

- `ResolvedComponent` — gains the per-component prose annotation. No new field.
- `OpenVexStatement` / `OpenVexProduct` — reused unchanged. The
  `subcomponents` field milestone 1035 added carries the FR-006a shape.
- `NixClosureSummary` — unchanged. This summary is a sibling, not an
  extension, because the two can degrade independently: the closure can
  succeed while the declaration pass fails.
