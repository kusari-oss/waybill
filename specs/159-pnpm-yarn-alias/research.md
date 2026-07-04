# Research: Milestone 159 (pnpm/yarn npm-alias syntax)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md)
**Plan**: [plan.md](./plan.md)

Phase-0 outline of unknowns + design decisions. No NEEDS-CLARIFICATION markers survived the /speckit-clarify session; this research pins down the technical details for the impl phase.

## R1 — Pnpm alias-VALUE grammar

**Decision**: The pnpm alias-value syntax mirrors the existing snapshot-key syntax that `parse_pnpm_key` at `pnpm_lock.rs:129` already handles. Two shapes observed 2026-07-04 on `test-podman-desktop`:

1. **Quoted value with peer-suffix**: `local-name: 'aliased-name@version(peer-suffix)'`
   ```yaml
   react-helmet-async: '@slorber/react-helmet-async@1.3.0(react-dom@18.3.1(react@18.2.0))(react@18.2.0)'
   ```

2. **Unquoted value without peer-suffix**: `local-name: aliased-name@version`
   ```yaml
   string-width-cjs: string-width@4.2.3
   ```

Detection rule: iff the local-name (dep key) differs from the parsed value's canonical name, treat it as an alias. In shape (1), local `react-helmet-async` ≠ parsed `@slorber/react-helmet-async` → alias. In shape (2), local `string-width-cjs` ≠ parsed `string-width` → alias. When they match (e.g. `foo@1.0.0: foo@1.0.0`), no alias.

**Rationale**: Reuses the existing `parse_pnpm_key` grammar. Zero new parser code — the differentiator is a name-inequality check on the ALREADY-PARSED value's canonical name.

**Alternatives considered**:

- **A. Detect alias via presence of `(` in the value string**: rejected — some valid non-alias values also carry peer suffixes when the local-name IS the aliased name (rare but observed).
- **B. Parse the entire value as a fresh grammar**: rejected — reinvents `parse_pnpm_key` for zero gain.

## R2 — Yarn v1 key-side alias grammar

**Decision**: Yarn v1 lockfile entries can carry multiple comma-joined specs on a single key line. The alias marker is `@npm:<aliased-name>` where the version-spec portion is `npm:<aliased-name>` (with the aliased name being either scoped `@scope/name` or unscoped `name`).

Observed shape 2026-07-04 on `test-guac-visualizer`:

```
"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":
  version "2.6.4"
  resolved "https://registry.yarnpkg.com/@cosmos.gl/graph/-/graph-2.6.4.tgz#..."
```

Detection rule: for each comma-separated spec in the key, check whether the version-spec-part (after the local-name's terminal `@`) starts with `npm:`. If so, the substring after `npm:` is the aliased-name. Combined with the entry's `version:` line (`"2.6.4"`), this produces the aliased-canonical identity `@cosmos.gl/graph@2.6.4`.

**Grammar (BNF)**:

```text
yarn_v1_key    ::= spec ("," spec)*
spec           ::= '"' local_name "@" version_spec '"'
                 | local_name "@" version_spec
local_name     ::= "@" scope "/" name | name
version_spec   ::= "npm:" aliased_name | range_spec
aliased_name   ::= "@" scope "/" name | name
range_spec     ::= <yarn v1 range syntax — semver / git / file / etc.>
```

**Rationale**: Yarn v1's `npm:` prefix is the documented alias marker (per Yarn Classic docs). Parsing is a simple string check on the version-spec after the local-name's `@` separator.

**Alternatives considered**:

- **A. Detect via `@npm:` substring anywhere in the key**: rejected — a package named `@npm:foo` (theoretical but valid registry name) would false-positive.
- **B. Only detect when EVERY spec in the comma-joined key uses `npm:`**: rejected — Yarn accepts mixed keys where one spec is aliased and another is a plain range (as in the `@cosmograph/cosmos` example). Both specs resolve to the same aliased target; the alias signal is present as long as ANY spec uses `npm:`.

## R3 — Component identity: local-name vs aliased-name

**Decision**: When an alias is detected, the emitted component's PURL uses the ALIASED canonical name + version. The local-name is preserved ONLY in the `mikebom:pnpm-alias` / `mikebom:yarn-alias` annotation, not in any identity-carrying field.

**Rationale**: 

- **Vulnerability lookup**: CVE feeds (OSV, GHSA, npm advisories) key on the npm-registry identity, which is the ALIASED name. A component emitted at `pkg:npm/string-width-cjs@4.2.3` would never match a CVE against `string-width@4.2.3`.
- **Constitution Principle IX (Accuracy)**: emitting components at the aliased identity is the correct npm-registry-normalized shape.
- **Constitution Principle X (Transparency)**: the `mikebom:pnpm-alias` annotation preserves the local-name so consumers can audit the resolution and trace back to the lockfile source.

**Alternatives considered**:

- **A. Emit both local-name AND aliased-name components (dual-emit)**: rejected — creates spurious duplicate entries in the SBOM. Consumers would see 2× the alias-affected count, inflating vulnerability-scan noise.
- **B. Emit local-name component with an aliased-name-via-annotation field**: rejected — vulnerability scanners don't parse `mikebom:*` annotations; they key on PURL. Local-name PURL never matches.
- **C. Emit local-name as the primary and add a `pkg:npm/aliased@v` alias-purl in `properties[]`**: rejected — CDX has no established "alternate PURL" property; the emitted primary PURL is what consumers act on.

## R4 — Edge rewriting

**Decision**: When another dep references the LOCAL-NAME (e.g. `hosted-server-mgmt` depending on `@cosmograph/cosmos "^1.1.1"` per FR-005), mikebom rewrites the edge's target to the aliased-canonical identity so the graph is fully connected via the resolved PURL.

The rewrite happens in the parser layer (INSIDE `parse_pnpm_lock` / `parse_yarn_lock`), NOT in a separate downstream pass. Rationale: alias mappings are per-lockfile (mikebom doesn't cross-resolve aliases across multiple lockfiles); building + consuming the mapping in the same function keeps scope contained.

**Data flow**:

1. First pass over the lockfile: build `alias_map: HashMap<String, AliasedIdentity>` where key = local-name, value = `(aliased-name, aliased-version)`.
2. Second pass: for each `PackageDbEntry.depends: Vec<String>`, look up each dep-name in `alias_map`. If found, rewrite the dep-name to the aliased-name so `scan_fs/mod.rs`'s `name_to_purl` lookup keys against the emitted component's identity.

**Rationale**: This matches the existing milestone-157 `parse_pnpm_lock` two-pass shape (build snapshots lookup, then walk packages) — reusing an established pattern.

**Alternatives considered**:

- **A. Rewrite in the graph resolver post-parse**: rejected — the graph resolver at `scan_fs/mod.rs:700` doesn't know about the alias context; adding an alias-map param would create a leaky abstraction.
- **B. Store both local-name AND aliased-name as depends entries (fan-out)**: rejected — creates spurious edges when the local-name doesn't appear in another component's `depends`.

## R5 — Annotation wire format (Q1 raw-string confirmed)

**Decision**: The `mikebom:pnpm-alias` and `mikebom:yarn-alias` annotations use RAW STRING values equal to the local-name only. No envelope JSON. Follows Q1 clarification 2026-07-04 + milestone-158's `mikebom:graph-completeness` precedent.

Per-format wire shape:

- **CDX 1.6**: `components[].properties[]` entry `{"name": "mikebom:pnpm-alias", "value": "react-helmet-async"}`.
- **SPDX 2.3**: per-package `Annotation` with `comment = "mikebom:pnpm-alias=react-helmet-async"`.
- **SPDX 3.0.1**: per-package `Annotation` element with `subject = <package IRI>`, `statement = "mikebom:pnpm-alias=react-helmet-async"`.

**Rationale**: Uniform raw-string across formats. Consumers do `jq '.components[] | select(.properties[]? | .name == "mikebom:pnpm-alias") | .properties[] | select(.name == "mikebom:pnpm-alias") | .value'` to enumerate.

## R6 — Parity catalog slot allocation

**Decision**: New rows C106 (`mikebom:pnpm-alias`) and C107 (`mikebom:yarn-alias`). Milestone 158 registered C104 + C105 (graph-completeness pair); this milestone claims the next 2 slots. Both use `Directionality::SymmetricEqual` and `order_sensitive: false` — same shape as milestone-158's C104/C105.

**Rationale**: Component-scope annotations of this kind (single-string value, mirrored across 3 formats) fit the established `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macro pattern from milestone 127.

## R7 — Milestone-090 no-alias verification

**Decision**: Empirically confirmed 2026-07-04 that NO milestone-090 fixture (11 ecosystems × 3 formats = 33 goldens) contains alias syntax. Verified via grep:

```bash
# Pnpm alias-value shape: quoted-string value starting with @ OR unquoted name@version pattern
grep -rn "': '" /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/npm/ 2>/dev/null | head
# Yarn npm: alias marker
grep -rn "@npm:" /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/ 2>/dev/null | head
```

Both grep patterns return zero matches on milestone-090 fixture tree. SC-003 dual-side byte-identity guard is therefore trivially achievable — no fixture regeneration needed for the 11 milestone-090 goldens.

**Rationale**: Confirms the milestone can ship without regenerating base goldens (unlike milestone-158 which added the graph-completeness annotation universally). Byte-identity on the 33 non-alias goldens is preserved automatically.

## R8 — Observability signal (FR-010/FR-011)

**Decision**: Follow the milestone-157 FR-007 / milestone-158 FR-013 pattern for tracing log emission:

- **FR-010** warn-level tracing log on malformed alias:
  ```rust
  tracing::warn!(
      lockfile = %source_path,
      local_name = %local_name,
      raw_value = %raw_value,
      "npm-alias parse failed (skipping entry)"
  );
  ```

- **FR-011** info-level tracing log on alias-resolution completion per lockfile:
  ```rust
  tracing::info!(
      lockfile_path = %source_path,
      alias_count = alias_count,
      alias_ecosystem = %eco,  // "pnpm" or "yarn"
      "npm-alias resolution completed"
  );
  ```

Both message strings are literal + grep-friendly for CI-log analysis.

**Rationale**: Consistency with recent milestones' observability convention. Consumers of mikebom logs can pattern-match on `"npm-alias resolution completed"` + `alias_count` to detect and quantify alias usage per scan.

## R9 — CHANGELOG entry shape

**Decision**: Follow the milestone-157/158 CHANGELOG entry template. Include:

- Motivation paragraph naming issue #493 + the milestone-157 Round-2 audit as discovery source.
- Two-part fix summary (alias detection + edge rewrite + annotation emission).
- Q1/Q2 clarification bullets.
- Empirical impact table (3 test repos × 10 dropped edges).
- Consumer jq recipe for filtering by `mikebom:pnpm-alias` / `mikebom:yarn-alias`.

**Consumer jq recipe** (draft):

```bash
# List all alias-resolved npm components in a mikebom SBOM
jq '.components[] | select((.properties // [])[] | .name | test("^mikebom:(pnpm|yarn)-alias$")) | {purl, alias: (.properties[] | select(.name | test("-alias$")) | .value)}' sbom.cdx.json

# Count alias-affected components per ecosystem
jq '[.components[] | .properties // [] | .[] | .name | select(test("^mikebom:(pnpm|yarn)-alias$"))] | group_by(.) | map({(.[0]): length}) | add' sbom.cdx.json
```

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data model + contracts).
