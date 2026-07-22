# Contract: Application main-module emission for Gemfile-only Ruby projects

**Feature**: 216-gemfile-main-module
**Kind**: Component-emission contract (per-ecosystem-reader shape)
**Consumers**: every SBOM emitter (CDX / SPDX 2.3 / SPDX 3), the m127 root-selector ladder, the m215 split-mode enumerator, downstream SBOM consumers (vuln scanners, merge tools, registries).

## Emission predicate

Waybill emits an application main-module `ResolvedComponent` for a directory `D` if and only if ALL of:

1. `D/Gemfile` exists (file, exact filename, case-sensitive).
2. `D/*.gemspec` does NOT exist.
3. `D` is under `scan_root` at a walkable depth (bounded by `MAX_GEMSPEC_WALK_DEPTH`).
4. `D` is not under an install-state directory (`vendor/`, `gems/`, `specifications/`, `.bundle/`).

**Determinism**: emission order is lex-sorted by `D`'s path. Two scans of the same input tree produce identical `Vec<PackageDbEntry>` ordering.

## Emitted PURL shape

```
pkg:generic/<name>@<version>
```

Where:
- `<name>` = sanitized directory basename of `D` per the m215 slug rules (`waybill-cli/src/generate/split.rs::subject_slug`). Lowercase; unsafe filesystem characters stripped; non-ASCII stripped; truncated to 100 bytes.
- `<version>` = first non-empty result of:
  1. `git describe --tags --always` executed in `D` (2s timeout)
  2. `git describe --tags --always` executed in `scan_root` (2s timeout)
  3. Literal `"0.0.0-unknown"`

**Purl-spec compliance**: `pkg:generic/` is the purl-spec's explicit escape hatch for packages that don't fit a defined ecosystem type (per the spec's Clarifications section 2026-07-22 resolution).

## Emitted annotations

The component's `extra_annotations` BTreeMap MUST include at minimum:

| Key | Value | Rationale |
|---|---|---|
| `waybill:component-role` | `"main-module"` | Reused signal — makes the component a first-class split-axis + m127 root-selector candidate. |
| `waybill:package-shape` | `"application"` | NEW parity-bridging annotation — distinguishes Gemfile-derived main-modules from published-gem main-modules and from `pkg:generic/` components emitted by other paths. Value vocabulary starts with `"application"`; may grow to include `"library"` / `"binary"` / `"framework"` in future work. Documented in `docs/reference/sbom-format-mapping.md`. |

Additional annotations MAY be present but MUST NOT contradict the above.

## Precedence over existing gem-reader emission (FR-007)

When directory `D` carries BOTH `Gemfile` AND `*.gemspec`:
- The pre-existing m069 gemspec-derived main-module fires.
- The application main-module does NOT fire.

Enforcement: the walker predicate condition #2 (`D/*.gemspec` does NOT exist) is checked at Gemfile-detection time — same directory scan pass. No downstream dedup relies on PURL comparison; the gemspec branch and the application branch produce PURLs of different types (`pkg:gem/` vs `pkg:generic/`), so a PURL-based dedup would fail to detect the semantic collision.

## Interaction with the m069 gemspec-loop

The application-loop runs AFTER the m069 gemspec-loop within `gem::read`. The order preserves:
- gemspec-derived entries appear first in the output `Vec<PackageDbEntry>` (matches existing test expectations).
- Application entries append after, in lex-sorted-path order.

There is NO cross-loop merging (FR-007 guarantees zero PURL overlap by construction).

## Behavior when `Gemfile.lock` is absent (FR-006)

If `D/Gemfile.lock` does NOT exist:
- Main-module component IS still emitted (per FR-006).
- No transitive-dep components are added (there's no lockfile to parse).
- Downstream graph-completeness computation naturally downgrades to `partial` with `orphaned-components-detected: 0` and a document-level reason indicating the missing lock (existing m158 machinery handles this without any change).

## Backwards-compat guarantees

- **Non-Ruby scans**: NO change. Walker returns empty on directories without `Gemfile`; no emission delta.
- **Gemspec-only scans** (published-gem shape): NO change. FR-007 gemspec-wins.
- **Pre-feature `{cdx,spdx,spdx3}_regression` tests**: ALL PASS unchanged. No golden regeneration required.

**Test enforcement**: SC-004 gates this contract at CI time. Any golden drift on a non-Ruby-application fixture is a regression, not an expected change.

## Consumer-side semantics

**Vuln scanners** (Trivy, Grype, etc.): a `pkg:generic/<app-name>@<version>` PURL will not match any Ruby-specific CVE feed (RubyGems advisory DB, OSV Ruby ecosystem entries). This is correct — the application is NOT a published gem, so a name-collision with a published gem's CVE would be a false positive. Scanners that want to nonetheless cross-reference can inspect the `waybill:package-shape = "application"` annotation and apply their own heuristics.

**SBOM merge tools**: the `pkg:generic/` PURL is treated as unrelated to any `pkg:gem/` component with the same name — different types, different identities. This is intentional and correct.

**Downstream visualization tools**: consumers displaying an SBOM's root component now see the meaningful application name (`my-service`) instead of the m127 synthetic-placeholder fallback (`iac`).

## Contract stability

- **Annotation names** (`waybill:package-shape`) are public — renaming is a breaking change requiring a MAJOR constitution amendment.
- **Annotation value vocabulary** — additive changes (adding new values like `"library"`) are non-breaking. Semantic redefinition of existing values IS breaking.
- **Emission predicate** — tightening (adding more required conditions) is breaking; loosening (relaxing conditions) is non-breaking (a superset of directories continues to emit).
- **PURL type choice** — changing from `pkg:generic/` to something else (e.g., a future `pkg:ruby-app/` type registered with purl-spec upstream) is a breaking change requiring a migration.
