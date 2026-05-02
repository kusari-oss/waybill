# Research: Trivy + Syft Go main-module patterns

**Created**: 2026-05-02
**Purpose**: Comparative analysis to validate milestone 053's design choices against established SBOM tools (Trivy and Syft).

## Sources consulted

- Trivy: `pkg/dependency/parser/golang/mod/parse.go`, `pkg/fanal/analyzer/language/golang/mod/mod.go`, `pkg/fanal/types/package.go`, `pkg/sbom/io/encode.go`, `pkg/sbom/cyclonedx/marshal.go`
- Syft: `syft/pkg/cataloger/golang/parse_go_mod.go`, `syft/pkg/cataloger/golang/package.go`, `syft/pkg/cataloger/golang/licenses.go`

## Findings by spec choice

### 1. PURL shape — `pkg:golang/<module>@<version>`

- **Trivy**: builds `packageID(m.Mod.Path, m.Mod.Version)`; CDX/SPDX serializer renders as `pkg:golang/<module>@<version>`. Same shape as milestone 053.
- **Syft**: `pkg:golang/<namespace>/<name>@<version>` — splits at the last `/` so multi-segment module paths like `github.com/cli/cli/v2` aren't escaped (per purl-spec issue #63).
- **Verdict**: Mikebom matches both. No change.

### 2. Version resolution

- **Trivy** (`parse.go`): uses whatever's on the `module` line (`m.Mod.Version`) — for go.mod main modules this is essentially always empty, so Trivy emits the root with empty version. **No git fallback.**
- **Syft** (`parse_go_mod.go`): same — main module version is `m.Mod.Version` from `modfile.Parse`. For binaries Syft has a separate `(devel)` sentinel that it strips. **No git-describe.**
- **Verdict**: Mikebom's `git describe --tags --exact-match HEAD` → `git describe --tags --always` → `v0.0.0-unknown` ladder is **better** than both. Neither tool attempts VCS resolution for the source-tree main module. Justifiable.

### 3. Component-role / main-module tagging (the substantive gap)

- **Trivy** (`pkg/sbom/io/encode.go::rootComponent`): emits the main-module as **CDX `metadata.component` with `Root: true` and `type: "application"`** — a standards-native field, no custom property. For SPDX, sets `primaryPackagePurpose: APPLICATION` and uses `documentDescribes` + `DESCRIBES`. The `RelationshipRoot` enum in `pkg/fanal/types/package.go` distinguishes Root from Direct/Indirect/Workspace.
- **Syft**: doesn't distinguish a Go main-module specifically. Source-tree scans get a synthetic top-level `Source` component (directory/file) recorded as `metadata.component` (CDX) or via SPDX `DESCRIBES` + `PrimaryPackagePurpose: FILE`/`CONTAINER`. Go modules from `go.mod` are all type `GoModulePkg` indistinguishably.
- **Verdict**: The original milestone 053 plan emitted the main-module as a flat `components[]` sibling tagged via `mikebom:component-role: main-module` custom property. **Per Principle V (native fields first), pivot to Trivy's pattern**: `metadata.component` for CDX, `primaryPackagePurpose: APPLICATION` + `documentDescribes` for SPDX, with `mikebom:component-role` retained as a supplementary signal. **Spec updated** at FR-001a + FR-004 + SC-008.

### 4. DependsOn relationship

- **Trivy** (`encode.go`): main module → directs uses `core.RelationshipDependsOn`, mapped to CDX `dependsOn` and SPDX `DEPENDS_ON`. Edges live in `dependencies[root.ID]` from the parser.
- **Syft** (`parse_go_mod.go::buildModuleRelationships`): uses `artifact.DependencyOfRelationship` (inverse direction); flips to `DEPENDS_ON` in SPDX serialization. **Important caveat**: Syft only builds these edges when `usePackagesLib` is on (i.e., when the Go toolchain is available). Without it, Syft emits no main-module → require edges from go.mod alone — same gap milestone 053 is fixing.
- **Verdict**: Mikebom matches Trivy. No change.

### 5. Indirect (`// indirect`) requires

- **Trivy** (`parse.go`): includes them when go.mod's `go` directive is ≥ 1.17, tags them `RelationshipIndirect`, and *does not* attach them under root via `DependsOn` (only Direct goes under root). Orphaned indirects (no cache-resolved transitive parent) get reparented under root by `addOrphanIndirectDepsUnderRoot` when the cache is missing.
- **Syft**: emits all `Require` entries as packages regardless of `// indirect`; the indirect bit is preserved in metadata but not reflected in relationship structure beyond what `packages.Load` resolves.
- **Verdict**: **Deliberate divergence** — milestone 053 emits all go.mod-declared requires (direct AND indirect) as edges from the main-module unconditionally. Trivy's "only direct, with orphan reparenting" is more semantically accurate but more complex; mikebom's simpler approach gives offline scans more edges to work with (the issue #102 case has zero cache, where Trivy's rules would orphan-reparent anyway, so net behavior is similar). Documented in Edge Cases section.

### 6. Polyglot doc-root

- **Trivy** (`encode.go`): single root component (filesystem/repository) becomes `metadata.component`; each ecosystem becomes an `application`-type child with `RelationshipContains` from root, and ecosystem packages hang under that. No "primary-ecosystem" pick.
- **Syft**: single `Source` (directory/image) as `metadata.component`; all packages siblings beneath, no per-ecosystem grouping component.
- **Verdict**: Trivy's nested pattern is the cleaner model — but adopting it for milestone 053 would require restructuring how mikebom emits per-ecosystem components, which is a bigger scope. **Keep milestone 053's approach** (synthetic super-root with multi-DESCRIBES, ecosystem-name-sorted) and track the Trivy-style nested grouping as future work tied to issue #104 (per-ecosystem main-modules).

### 7. LICENSE-file detection at workspace root

- **Trivy**: classifies LICENSE files only inside `$GOPATH/pkg/mod/...` and `vendor/` for *dependencies*. Does not detect the workspace's own LICENSE for the root module.
- **Syft**: same — `licenses.go` resolves licenses for required modules via the local Go cache; the synthetic Source component carries no detected license.
- **Verdict**: Both tools skip workspace-root LICENSE detection. Milestone 053's choice (defer to follow-up issue #103, emit empty `licenses` on main-module) is consistent with prevailing practice. No change.

## Spec changes applied

1. **FR-001a (placement)** added — main-module promoted to native CDX `metadata.component` and SPDX `primaryPackagePurpose: APPLICATION` + `documentDescribes`.
2. **FR-004** updated — `mikebom:component-role` is now supplementary (was primary).
3. **SC-008** added — verifies native-field emission via fixture-based assertions.
4. **Edge Cases** updated — explicit Trivy-divergence note for indirect requires.
5. **Clarifications** appended — comparative-analysis table + Q4 (placement decision) recorded.

## Spec choices left as-is (validated against trivy/syft)

- PURL shape (FR-001).
- Version ladder (FR-001).
- DependsOn relationship type (FR-003).
- LICENSE detection deferred to #103 (FR-005).
- Polyglot super-root (FR-008).

## Out-of-scope, tracked elsewhere

- Trivy-style nested-application polyglot root structure: defer to issue #104 (per-ecosystem main-modules).
- LICENSE content matching (askalono / SPDX-License-Identifier header scan): issue #103.
