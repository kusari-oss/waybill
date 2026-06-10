# CLI Contract: `--pkg-alias`

**Feature**: 111-pkg-alias-binding

## Flag definition

```
--pkg-alias <LHS=RHS>
    Declare that the binary-tier PURL `LHS` should be treated as the
    source-tier PURL `RHS` when computing cross-tier binding (milestone
    072). Both sides MUST be canonical-form PURLs. Repeatable; multiple
    aliases compose. Requires `--bind-to-source` to have effect;
    supplied otherwise, the flag emits a warning and the alias is
    discarded from the emitted SBOM.

    Also settable via `MIKEBOM_PKG_ALIAS` (comma-separated entries).

    See `specs/111-pkg-alias-binding/` for full semantics.
```

## Syntax

| Element | Rule |
|---|---|
| Outer separator | `=` — qualifier-aware: PURL qualifiers legitimately contain `=` (e.g. `pkg:generic/baz?file-sha256=<hex>` as LHS), so the parser tries each `=` position left-to-right and accepts the first split whose right side starts with `pkg:` and whose two sides both canonicalize |
| LHS | Canonical-form PURL string |
| RHS | Canonical-form PURL string |
| LHS == RHS | Rejected (`AliasError::LhsEqualsRhs`) |
| Repeated flag | Multiple `--pkg-alias` invocations are unioned into one `AliasMap` |
| Conflicting RHS for same LHS | Rejected (`AliasError::ConflictingRhs`) |
| Same RHS for different LHSes | Accepted |

## Environment variable

```
MIKEBOM_PKG_ALIAS=pkg:generic/baz=pkg:cargo/baz@1.0.0,pkg:generic/qux=pkg:cargo/qux@2.0.0
```

| Element | Rule |
|---|---|
| Entry separator | `,` |
| Per-entry shape | Same as `--pkg-alias` value |
| Whitespace around `,` | Trimmed |
| Empty entries (`,,`) | Skipped silently |
| Conflict between env-var and CLI flag | Treated as a single composed alias-list; if the union contains a conflict, fail at parse time (FR-008) |

## Error messages (FR-008, FR-009 — SC-003 single-line-actionable contract)

| Trigger | Message |
|---|---|
| Missing `=` | `error: --pkg-alias value 'pkg:generic/baz' is missing the '=' separator; expected format: 'LHS_PURL=RHS_PURL'` |
| Malformed LHS | `error: --pkg-alias LHS PURL 'pkg:bad@@thing' failed to parse: <PurlError details>` |
| Malformed RHS | `error: --pkg-alias RHS PURL 'pkg:cargo/...' failed to parse: <PurlError details>` |
| LHS == RHS | `error: --pkg-alias LHS 'pkg:cargo/baz@1.0.0' identical to RHS; aliases must specify distinct PURLs (did you mean to declare a different RHS?)` |
| Conflicting RHS | `error: --pkg-alias LHS 'pkg:generic/baz' declared twice with conflicting RHS values: 'pkg:cargo/baz@1.0.0' and 'pkg:cargo/baz@1.1.0' (only one mapping per LHS is permitted; resolve the conflict and re-run)` |

All errors exit with non-zero status before any scan I/O.

## Warning messages

| Trigger | Message (warn-level) |
|---|---|
| `--pkg-alias` supplied without `--bind-to-source` (FR-010) | `WARN: --pkg-alias declared (N entries) but --bind-to-source was not supplied; aliases have no effect on this scan and will not appear in the emitted SBOM. Add --bind-to-source <SOURCE_SBOM> to enable cross-tier binding.` |
| Alias LHS unused (no scan-output component matched) (FR-011) | `INFO: --pkg-alias LHS 'pkg:generic/qux' did not match any scan-output component; no alias applied for this entry.` (Info, not warn, because operator typos are common and an info log is enough signal.) |

## Interaction with other flags

| Flag | Interaction |
|---|---|
| `--bind-to-source` | Required for `--pkg-alias` to have effect (FR-010) |
| `--component-id` (milestone 073) | Independent; both may be supplied in the same invocation. `--component-id` adds identifier annotations; `--pkg-alias` rewrites binding-match input. No mutual exclusion enforced. |
| `--fingerprints-corpus` (milestone 108) | Independent. Aliases apply to any scan-output component regardless of which reader produced it. |

## Versioning + back-compat

- The flag is purely additive; no existing flag's semantics change.
- Pre-feature scans (no `--pkg-alias` supplied) produce byte-identical SBOMs to pre-feature baselines (SC-004).
- Pre-feature consumers reading an alias-bearing SBOM see the extended envelope with two extra fields (`alias_from`, `alias_to`); serde-default ignore-unknown-fields means they deserialize without error and report the binding result as if it were a non-aliased verified/weak binding (`applied_alias` sibling absent in their output).
