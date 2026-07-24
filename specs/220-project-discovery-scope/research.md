# Research: `--project-discovery=<mode>` — cap main-module discovery scope

**Feature**: 220-project-discovery-scope | **Date**: 2026-07-24

## R1 — Filter placement: post-discovery vs pre-discovery

**Decision**: Post-discovery filter. Readers continue to walk + discover as they do today; the m220 scope-filter pass runs AFTER `enumerate_workspace_roots` populates the main-module set, drops out-of-scope main-modules + their unreachable transitive components, and threads the retained set into every downstream emitter.

**Rationale**: 
- Pre-discovery cap (each reader's walker checks the mode + skips main-module-defining files below root except workspace-declared members) would require per-reader touch points across cargo/npm/gem/pip/golang/maven/yocto/etc. — 10+ small edits, each with subtle interactions with the reader's own workspace-member logic.
- Post-discovery filter reuses m215's `SubprojectRoot` + `project_for_root` BFS infrastructure verbatim. Zero per-reader changes. Cost: readers still walk + open manifests, so `--project-discovery=root-only` doesn't yield a perf win — but the correctness win + downstream SBOM cleanliness is what the operator asked for. Perf optimization (skipping walker work) is a future milestone if a real workload shows it matters.

**Alternatives considered**:
- **Pre-discovery per-reader cap**: rejected as scope creep for m220. Deferred as m220b if perf becomes a concern.
- **Hybrid post-filter with per-reader skip hint**: rejected — introduces a per-reader signaling protocol for zero MVP win.

## R2 — Workspace-member preservation: consuming the existing `waybill:workspace-member` annotation

**Decision**: The scope-filter treats a component as "workspace member of a root main-module" iff its `extra_annotations.get("waybill:workspace-member")` is `Some(...)`. Every reader that supports workspaces today (cargo per m127, npm per m147/pnpm/yarn, go per m161, maven per m085) already stamps this annotation on member components. m220 reuses that signal verbatim — NO new per-reader detection heuristics.

**Rationale**: The scan_cmd.rs at :3234-3695 already computes a "workspaces detected" summary by inspecting `waybill:workspace-member` values. m220's filter is trivially additive: when a component carries the annotation AND its value's source-dir is under a root main-module's directory, it's in-scope under root-only. When a component lacks the annotation AND its source-dir is nested under scan-root (not at depth-0), it's an independent nested project → out-of-scope under root-only.

**Ecosystem detection matrix** (as of m219):
| Ecosystem | Workspace-declared members detected? | Signal (in `waybill:workspace-member`) |
|--|--|--|
| Cargo (m127 + m201) | ✅ Yes | Value is the workspace-root PURL |
| npm/pnpm/yarn (m147/m180) | ✅ Yes | Value is the workspace-root package name |
| Go workspaces (m161) | ✅ Yes | Value derived from `go.work` `use` directives |
| Maven multi-module (m085) | ✅ Yes | Value from `<modules>` in root POM |
| pyproject (poetry / hatch / setuptools) | ⚠️ Partial | Reader-dependent; m220 defers to existing behavior |
| gem | ❌ No workspace concept | m220 treats every discovered gem main-module as independent |
| Others (composer, dart, ipk, alpm, ...) | Varies | Same — reader's existing decision applies |

**Alternatives considered**:
- **Introducing a new `waybill:workspace-member-of` annotation with root-PURL value**: rejected — `waybill:workspace-member` already carries the root identity (readers stamp it with the workspace-root's identifying value).
- **Reading each ecosystem's manifest files at filter-time to re-detect workspaces**: rejected — duplicates every reader's existing logic + adds per-ecosystem code paths m220 explicitly avoids.

## R3 — BFS-projection reuse: same shape as m215's `project_for_root`

**Decision**: Reuse `crate::generate::split::project_for_root` verbatim. Given a `Vec<SubprojectRoot>` of "in-scope roots" (root-level main-modules + optionally their workspace members under root-only), BFS-project each root into a `SplitProjection` (m215 type), then union the components + relationships. Drop anything not in the union.

**Rationale**: m215's `project_for_root` at `waybill-cli/src/generate/split.rs:220` already handles: (1) BFS over dep-edge relationships; (2) preserving root component ordering; (3) demoting sibling main-modules reached via cross-deps (avoid m127 confusion); (4) self-contained relationship filtering. All the same semantics m220 needs. Zero new code for the BFS algorithm.

**Small delta**: m215's per-projection BFS drops sibling main-modules' component-role. m220's filter runs over MULTIPLE projections (one per in-scope root); union them → shared components are legitimately included (both roots reach them); each root's own main-module role is preserved.

## R4 — Root-level manifest detection

**Decision**: A main-module is "root-level" iff its source-dir (from `source_dir_for` at `split.rs:123`) is either empty OR canonicalizes to the scan-root path itself.

```rust
fn is_root_level(root: &SubprojectRoot, scan_root: &Path) -> bool {
    let source_dir_str = root.source_dir.to_string_lossy();
    if source_dir_str.is_empty() { return true; }
    // Canonicalize both sides — handles symlinks + relative-path differences.
    let source_canon = std::fs::canonicalize(&root.source_dir).ok();
    let scan_canon = std::fs::canonicalize(scan_root).ok();
    matches!((source_canon, scan_canon), (Some(a), Some(b)) if a == b)
}
```

**Rationale**: m215's `source_dir_for` already returns the manifest's parent directory (m215 R2 established this). Testing that against `scan_root` (canonicalized on both sides) is the standard "is this at the top level" check. Falls back gracefully when either canonicalization fails.

**Edge cases**:
- Empty source_dir (subproject IS the scan root): considered root-level.
- Scan-root is itself a symlink: canonicalization resolves it. Fine.
- Broken symlink under scan-root that points nowhere: source_dir_for returned the parent (not the target); canonicalize may fail; treated as non-root-level (conservative — fewer false-positives).

## R5 — `ProjectDiscoveryMode` enum shape

**Decision**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]  // → "all", "root-only", "strict"
pub enum ProjectDiscoveryMode {
    /// Current behavior. Every reader discovers main-modules
    /// wherever they find qualifying manifests. SC-005 byte-identity.
    #[default]
    All,
    /// Discover only root-level main-modules + their ecosystem-native
    /// workspace-declared members (via existing `waybill:workspace-
    /// member` annotation). Independent nested projects dropped.
    RootOnly,
    /// Discover only root-level main-modules; ignore even ecosystem-
    /// native workspace-member declarations. Truly literal shallow.
    Strict,
}

impl ProjectDiscoveryMode {
    /// Return whether `root` is in-scope under this mode.
    pub fn is_root_in_scope(&self, root: &SubprojectRoot, scan_root: &Path) -> bool {
        match self {
            Self::All => true,
            Self::RootOnly | Self::Strict => is_root_level(root, scan_root),
        }
    }
    /// Return whether workspace-declared members of in-scope roots
    /// should be walked. `All` → yes (they'd be walked anyway).
    /// `RootOnly` → yes (US2 requirement). `Strict` → no.
    pub fn follows_workspace_members(&self) -> bool {
        !matches!(self, Self::Strict)
    }
}

impl std::fmt::Display for ProjectDiscoveryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use clap::ValueEnum;
        f.write_str(
            self.to_possible_value()
                .expect("ProjectDiscoveryMode variants all have possible values via ValueEnum derive")
                .get_name(),
        )
    }
}
```

**Rationale**: Enum-with-method mirrors m219 `SplitMode` verbatim. Display renders lowercase kebab-case (`root-only`, not `RootOnly`) via `to_possible_value().get_name()` — same B1-remediation lesson from m219 (matches CLI wire form so INFO-log substring assertions work). `#[default]` on `All` variant + `#[derive(Default)]` avoids the clippy `derivable_impls` lint.

## R6 — CLI-flag shape

**Decision**: 

```rust
#[arg(
    long = "project-discovery",
    value_enum,
    default_value = "all",
    require_equals = true,
)]
pub project_discovery: ProjectDiscoveryMode,
```

- `default_value = "all"` (not `default_missing_value`) — the flag isn't a bool; presence-vs-absence isn't meaningful, only the value is. Default when omitted = `All`.
- `require_equals = true` — prevents `--project-discovery root-only` (space-separated) which would consume the next positional arg. Matches m173 `--warm-go-cache`, m219 `--split=<mode>`, m207 `--offline=` precedents.
- Env-var bridge: `WAYBILL_PROJECT_DISCOVERY` (set in scan_cmd.rs when CLI arg is non-default; readable by scan_fs), matching m173/m218 env-bridge pattern.

**Rationale**: This shape differs slightly from m219's `--split[=<mode>]` because `--project-discovery` doesn't have a "bare flag = feature-on" semantic (there's no "off" state; every scan uses SOME mode). Every scan has a project-discovery mode; the operator's only choice is which mode. `default_value` handles that cleanly.

**Alternatives considered**:
- Match m219's `Option<Mode>` with `default_missing_value`: rejected — asymmetric with the always-required semantic; muddier for consumers.
- Env-only (no CLI flag): rejected — first-class scan-shaping decisions belong in the CLI surface.

## R7 — Component-scope filter (BFS from in-scope roots)

**Decision**: Given `Vec<SubprojectRoot>` of in-scope roots after R5's filter, run:

```rust
fn apply_scope_filter(
    components: Vec<ResolvedComponent>,
    relationships: Vec<Relationship>,
    in_scope_roots: &[SubprojectRoot],
    mode: ProjectDiscoveryMode,
) -> (Vec<ResolvedComponent>, Vec<Relationship>, ProjectDiscoveryReport) {
    if mode == ProjectDiscoveryMode::All {
        // Default: no filter, no clone, no BFS. Return inputs verbatim.
        return (components, relationships, ProjectDiscoveryReport::all_default());
    }
    // Collect the union of BFS-reachable component-PURLs from every in-scope root.
    let mut reachable: BTreeSet<String> = BTreeSet::new();
    for root in in_scope_roots {
        let projection = crate::generate::split::project_for_root(root, &components, &relationships);
        for c in &projection.components {
            reachable.insert(c.purl.as_str().to_string());
        }
    }
    // Additionally, under RootOnly, include every workspace-member
    // component whose `waybill:workspace-member` annotation points at
    // any in-scope root's PURL.
    if mode == ProjectDiscoveryMode::RootOnly {
        let in_scope_root_purls: BTreeSet<String> = in_scope_roots.iter()
            .map(|r| r.purl_string.clone()).collect();
        for c in &components {
            if let Some(v) = c.extra_annotations.get("waybill:workspace-member") {
                if let Some(root_ref) = v.as_str() {
                    if in_scope_root_purls.contains(root_ref) {
                        reachable.insert(c.purl.as_str().to_string());
                    }
                }
            }
        }
    }
    // Filter both slices.
    let filtered_components: Vec<ResolvedComponent> = components.into_iter()
        .filter(|c| reachable.contains(c.purl.as_str())).collect();
    let filtered_relationships: Vec<Relationship> = relationships.into_iter()
        .filter(|r| reachable.contains(&r.from) && reachable.contains(&r.to)).collect();
    let report = ProjectDiscoveryReport {
        mode,
        root_main_modules: in_scope_roots.len(),
        workspace_members_followed: /* count from filtered set */,
        nested_projects_ignored: /* original main-module count - in_scope_roots.len() */,
    };
    (filtered_components, filtered_relationships, report)
}
```

**Rationale**: 
- Default path is zero-cost — no clone, no BFS, no annotation walk.
- Under non-default, BFS union covers "everything reachable from in-scope roots" — this correctly retains transitively-shared deps + drops orphaned components.
- Under `RootOnly`, the second pass adds workspace-member components whose annotation points at an in-scope root — this covers the case where a workspace-member's PURL doesn't appear as a BFS target from the root (e.g., if the root doesn't declare an explicit dep on the member but the member is still tagged as part of the workspace). Belt-and-suspenders for FR-004.
- Under `Strict`, only the first BFS pass runs (no workspace-member annotation follow-up), so members' components get dropped — matches US3 semantic.

## R8 — Doc-scope annotation `waybill:project-discovery-mode` (C140)

**Decision**: New parity-catalog row C140. Doc-scope annotation whose value is the mode string (`"root-only"` or `"strict"`). Emitted iff mode ≠ `All` (silence-on-default per m217 C136 + m219 C137-C139 precedent). Landing slots:
- **CDX**: `metadata.properties[]` entry.
- **SPDX 2.3**: doc-level `Annotation` on `SPDXRef-DOCUMENT` via `MikebomAnnotationCommentV1` envelope.
- **SPDX 3**: `Annotation` element on the SpdxDocument root IRI.

**Rationale**: Same shape as m217 C136 (`waybill:go-toolchain-detected`) which is the closest doc-scope-annotation precedent. Standards-native audit per Principle V documented in plan.md.

**Wire example** (CDX):

```json
{
  "metadata": {
    "properties": [
      {"name": "waybill:project-discovery-mode", "value": "root-only"}
    ]
  }
}
```

**Silence-on-default rationale**: If we emitted `"value": "all"` on every default-mode scan, we'd churn every existing golden. Absent-when-default preserves SC-005 byte-identity across the m215 + prior fixture goldens.

## R9 — Interaction with `--split[=<mode>]`

**Decision**: Orthogonal composition. The m220 scope filter runs BEFORE the m215/m219 split-emit pipeline consumes `enumerate_workspace_roots` output. Under `--project-discovery=root-only --split=<anything>`, split-emit sees the already-filtered main-module set — typically 1-2 roots — and emits accordingly.

**Rationale**: Both flags govern the same pipeline stage (`enumerate_workspace_roots` output). m220's filter is a strict subset-selection; m215/m219's split-mode grouping happens after. Composition is: "filter first, then group."

**Interaction matrix**:

| Discovery | Split | Result |
|--|--|--|
| `all` (default) | `workspace` (default) | m215 default: 1 SBOM per main-module |
| `all` | `directory` | m219: 1 SBOM per directory group |
| `root-only` | `workspace` | 1 SBOM per root-level main-module (typically 1) |
| `root-only` | `directory` | 1 SBOM for the root directory (all root-level main-modules merge if same dir) |
| `strict` | `workspace` | 1 SBOM per root-level main-module (workspace members excluded) |
| `strict` | `directory` | 1 SBOM for the root directory (workspace members excluded) |

## R10 — Test fixtures

**Decision**: Three new fixtures under `waybill-cli/tests/fixtures/project_discovery/`:

1. **`polyglot_nested_independent/`** (US1 SC-001):
   - `Cargo.toml` at root (`[package] name = "p220-root"` + a dep like serde)
   - `Cargo.lock` at root
   - `services/api/{package.json, package-lock.json}` (npm project, nested; NOT declared as any workspace member)
   - `services/worker/{go.mod, go.sum}` (Go project, nested; NOT declared as any workspace member)
   - Expected `root-only`: only cargo main-module + cargo transitive deps in SBOM.
   - Expected `all`: 3 main-modules + all 3 ecosystems' deps.

2. **`cargo_workspace_with_independent_neighbor/`** (US2 SC-003 + SC-004):
   - `Cargo.toml` at root (`[workspace] members = ["crates/api", "crates/worker"]`)
   - `Cargo.lock` at root
   - `crates/api/Cargo.toml` + `crates/worker/Cargo.toml` (workspace members)
   - `bench/Gemfile` + `bench/Gemfile.lock` (independent Ruby project, nested; NOT a workspace member)
   - Expected `root-only`: workspace-root main-module + 2 workspace-member components + all cargo deps; NO `pkg:gem/*`.
   - Expected `strict`: workspace-root main-module + its own deps only; NO workspace members; NO `pkg:gem/*`.

3. **`polyglot_root/`** (edge case SC-005 parity — multiple root-level manifests):
   - `Cargo.toml` at root
   - `package.json` + `package-lock.json` at root (also root-level)
   - Expected `root-only`: BOTH root-level main-modules in SBOM (both ARE at scan-root; not nested).

## R11 — Documentation surface

**Decision**: Author `docs/reference/project-discovery.md` (NEW page) with 6 sections:

1. **What the modes mean** (table with when-to-choose guidance).
2. **Interaction matrix vs `--split[=<mode>]`** (per R9 table).
3. **Per-ecosystem workspace-member detection rules** (per R2 matrix).
4. **Worked examples**: Cargo-workspace + polyglot-monorepo + m216 Gemfile-only Ruby app.
5. **FR-011 doc-scope annotation** (C140 payload + landing slots).
6. **Extensibility contract** for future modes (`explicit=<paths>`, `depth=<N>`) — matches m219 contract page.

Linked from README's "SBOM interpretation" section (in `docs/reference/sbom-scopes.md` post-PR-#639) + `docs/index.md` top-level reference list.
