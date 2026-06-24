# Research — milestone 139 CocoaPods reader

Resolves the Phase 0 open items from `plan.md`'s Technical Context: Constitution Principle V audit + purl-spec authority audit, on-disk schemas (`Podfile.lock` + `Podfile` + `Pods/Manifest.lock`), subspec PURL encoding correction, integration site within `read_all`, the language-reader pattern selection, multi-target/multi-project handling, per-file error posture.

## R1: Constitution Principle V audit — `cocoapods` PURL type is purl-spec-blessed

**Decision**: Emit `pkg:cocoapods/<pod>@<version>[#<subspec>]` per [purl-spec `cocoapods-definition.md`](https://github.com/package-url/purl-spec/blob/main/types-doc/cocoapods-definition.md). No `mikebom:*` annotation introduced for identity. Source-discriminator surfaces via the existing `mikebom:source-type` annotation (parity-catalog C1, introduced in milestone 002).

**Rationale**:

The purl-spec defines `cocoapods` explicitly:

- **Namespace**: PROHIBITED — "there is no namespace".
- **Default repository URL**: `https://cdn.cocoapods.org/`.
- **Name case-sensitivity**: CASE-SENSITIVE — names cannot contain whitespace, `+`, or begin with `.`.
- **Subspec encoding**: PURL `#subpath` mechanism (NOT a `?subspec=` qualifier per the initial spec guess). Spec text: *"The purl subpath is used to represent a pods subspec (if present)."* Canonical examples: `pkg:cocoapods/ShareKit@2.0#Twitter`, `pkg:cocoapods/GoogleUtilities@7.5.2#NSData+zlib`.
- **Multi-level subspecs**: the cocoapods type doc doesn't show explicit examples, but purl-spec base rules treat subpath as a `/`-separated path of percent-encoded segments. `Firebase/Database/Realtime` emits as `pkg:cocoapods/Firebase@10.20.0#Database/Realtime` (raw `/` between segments; percent-encode unsafe chars within segments per RFC 3986).

**Source-discriminator handling**:

For trunk (default) / git: emit under `pkg:cocoapods/` per spec. The `mikebom:source-type` annotation reuses the existing C1 parity-catalog row. CocoaPods contributes new VALUES (`cocoapods-trunk`, `cocoapods-git`, `cocoapods-main-module`) without altering wire shape.

For path-sourced pods: the purl-spec doesn't define an addressable PURL for filesystem-local pods. The reader emits `pkg:generic/<pod>@<version>` placeholder + `mikebom:source-type = "cocoapods-path"` annotation as discriminator. This is a **parity-bridge** per Principle V's escape clause.

**syft/trivy divergence**: both syft and trivy fold the subspec into the name (`pkg:cocoapods/Firebase/Database@1.0.0`) instead of using subpath. This is purl-spec-non-conformant. Per Principle V, mikebom emits the spec-conformant `#subpath` form. Emitting a syft/trivy-compatibility `mikebom:also-known-as` annotation is deferred to v1.1 (similar to the milestone-134 divergent-PURL pattern but not blocking this milestone — operators who need syft/trivy compat can use those tools directly).

**Alternatives considered**:

- **Use `?subspec=` qualifier** (the initial spec assumption). Rejected: directly contradicts purl-spec. Phase 0 correction applied.
- **Fold subspec into name** (syft/trivy pattern). Rejected: violates Principle V (standards-native first).
- **Skip path pods entirely**. Rejected: real source-tree deps; surfacing with annotated placeholder is more transparent.

## R2: `Podfile.lock` schema (CocoaPods 1.0+ — universal in 2026 production iOS)

**Decision**: Parse a small typed subset via `serde_yaml::Value`-level dispatch (the `PODS:` array is heterogeneous: each entry is a string OR a single-key map). Required-in-modern-1.0+ keys: `PODS`, `DEPENDENCIES`. Everything else `Option<T>` with `#[serde(default)]`.

**Schema** (per [cocoapods-core `lockfile.rb`](https://github.com/CocoaPods/Core/blob/master/lib/cocoapods-core/lockfile.rb) `HASH_KEY_ORDER` constant):

```yaml
PODS:
  - "AFNetworking (4.0.1)"                    # bare-string entry (no transitive deps)
  - "BananaLib (1.0)":                        # map entry (with transitive deps)
      - "monkey (< 1.0.9, ~> 1.0.1)"
  - "Firebase/Core (10.20.0)":                # subspec entry
      - "FirebaseCore (~> 10.20)"

DEPENDENCIES:
  - AFNetworking (~> 4.0)
  - Firebase/Core

SPEC REPOS:                                   # which spec repo resolved each pod
  trunk:
    - AFNetworking
  "https://github.com/acme/private-specs.git":
    - InternalLib

EXTERNAL SOURCES:                             # per-pod source overrides
  MyFork:
    :git: "https://github.com/foo/my-fork.git"
    :branch: "main"
  LocalLib:
    :path: "../packages/local-lib"

CHECKOUT OPTIONS:                             # resolved 40-char SHAs for git sources
  MyFork:
    :commit: "abc123def456...40chars"
    :git: "https://github.com/foo/my-fork.git"

SPEC CHECKSUMS:                               # SHA-1 of each pod's podspec (root-keyed)
  AFNetworking: "abc123...40hex"
  Firebase: "def456...40hex"                  # ROOT-keyed — covers Firebase/Core, Firebase/Auth

PODFILE CHECKSUM: "fed987...40hex"
COCOAPODS: 1.15.2
```

Fields consumed by mikebom v1:
- `PODS[*]` — each entry is the pod's pinned spec; subspecs appear with `/` (e.g. `Firebase/Core (10.20.0)`)
- `DEPENDENCIES[*]` — direct-dep names; FR-004 dep-edge attribution source
- `EXTERNAL SOURCES{name}` — discriminator for git/path/podspec source; provides `:git` URL + operator-declared `:branch`/`:tag`/`:commit`/`:path`
- `CHECKOUT OPTIONS{name}` — RESOLVED 40-char SHA in `:commit` for git sources (per Q2 + Phase 0 confirmation)
- `SPEC CHECKSUMS{root-pod-name}` — SHA-1 hex per FR-008; ROOT-keyed (subspecs of the same root share the same checksum)

Fields NOT consumed v1: `SPEC REPOS` (private-spec-repo provenance deferred per spec Out-of-Scope), `PODFILE CHECKSUM` (informational), `COCOAPODS` (informational).

**`PODS:` entry shape polymorphism**: each YAML element is either a string (no transitive deps) or a single-key map (key = pod-spec string, value = transitive deps array). Parsed via `serde_yaml::Value` then dispatched.

**Subspec entries in `PODS:`**: Per Phase 0 research, subspecs appear as their own entries (`Firebase/Core (10.20.0)`); the parent root pod does NOT automatically appear unless separately depended-on. Each subspec emits as a distinct mikebom component.

**Alternatives considered**:

- **Shell out to `pod` for resolved metadata**. Rejected: `pod` not guaranteed on scan host; CocoaPods is a Ruby gem; cross-platform reader principle.
- **Parse via cocoapods-core's Ruby code via some FFI shim**. Rejected: violates Principle I.

## R3: `Podfile` DSL — regex-only line-by-line extraction

**Decision**: Regex extraction (matches gem reader posture from milestone 069). NO Ruby evaluation.

Patterns:
- **`target` block**: `^\s*target\s+['"]([^'"]+)['"]\s+do\b` — both single + double quotes accepted (Ruby DSL standard). First-target wins for FR-012 main-module name derivation.
- **`pod` declaration**: `^\s*pod\s+['"]([^'"]+)['"](?:\s*,\s*['"]([^'"]+)['"])?` — captures pod name + optional first-string constraint. Hash options (`:git => '...'` etc.) parsed via separate scan if needed for design-tier mode.
- **`platform` directive**: `^\s*platform\s+:(\w+)(?:\s*,\s*['"]([^'"]+)['"])?` — informational; not consumed v1.

**Conditional blocks** (`if`/`unless`/`def`): per Phase 0 research recommendation, extract every `pod '…'` line regardless of conditional nesting (matches gem reader posture). Operators who need precision can use `pod outdated` directly. Adding a `mikebom:cocoapods-extraction-mode = "conditional-flattened"` annotation when nested inside a conditional is deferred to v1.1 (low practical value vs implementation complexity).

**Comments**: `#` line comments. Strip via `line.split_once('#').map(|(s, _)| s).unwrap_or(line).trim_end()` before regex match.

**Alternatives considered**:

- **Shell out to a Ruby evaluator**. Rejected: Principle I + host-portability.
- **Embed a Ruby parser** (`tree-sitter-ruby` crate). Rejected: significantly increases dep tree for marginal accuracy gain; gem reader's posture is the established pattern.

## R4: `Pods/Manifest.lock` semantics

**Decision**: Per Phase 0 research + Q3 clarification, treat `Manifest.lock` as byte-equivalent to `Podfile.lock` at install time. When `Podfile.lock` is present, IGNORE `Manifest.lock` (FR-011 — avoid double-counting). When `Manifest.lock` is the ONLY lockfile, parse it identically but emit components with `mikebom:sbom-tier = "deployed"` and `mikebom:evidence-kind = "cocoapods-manifest-lock"` per Q3.

Phase 0 research confirmed: CocoaPods enforces byte-equivalence via the `check_manifest_lock` build phase Xcode script (literal `diff Podfile.lock Pods/Manifest.lock`); mismatch fails the build. No intentional divergence.

**Layered container scans**: same posture as composer's installed.json walker — discover every `Pods/Manifest.lock` under the scan root regardless of sibling-Podfile.lock pairing. Cross-project same-PURL duplicates collapse via the standard orchestrator `seen_purls` dedup.

## R5: purl-spec `cocoapods` canonical form (post-corrections)

**Decision**: Honor purl-spec verbatim. Specifically:

- **Base form**: `pkg:cocoapods/<pod>@<version>` (no namespace).
- **Pod name**: case-preserved verbatim from lockfile (CocoaPods is case-sensitive per spec).
- **Version**: verbatim from lockfile (`(version)` form parsed: strip parentheses, preserve any pre-release / build-metadata suffix).
- **Subspec**: PURL `#subpath` form per Phase 0 correction. `Firebase/Core` → `pkg:cocoapods/Firebase@10.20.0#Core`. Multi-level → `pkg:cocoapods/Firebase@10.20.0#Database/Realtime` (raw `/` between segments).
- **Git source**: `pkg:cocoapods/<pod>@<version>?vcs_url=git+<git-url>`. URL from `EXTERNAL SOURCES{pod}{:git}`. Resolved 40-char SHA from `CHECKOUT OPTIONS{pod}{:commit}` flows into `mikebom:vcs-ref` annotation (NOT embedded in PURL version — the lockfile's `(version)` already conveys upstream identity).
- **Path source**: `pkg:generic/<pod>@<version>` placeholder + `mikebom:source-type = "cocoapods-path"` annotation (R1).
- **SHA-1 hash**: from `SPEC CHECKSUMS{root}` — ROOT-keyed per Phase 0 correction. Subspec components look up by ROOT pod name; all subspecs share the same SHA-1.

**`mikebom:source-type` value set** (reuses parity-catalog C1):

| Source kind | `mikebom:source-type` value |
|---|---|
| Trunk (default CDN) | `cocoapods-trunk` |
| Trunk (private spec repo) | `cocoapods-trunk` (provenance deferred per spec Out-of-Scope) |
| Git | `cocoapods-git` |
| Path | `cocoapods-path` |
| Main-module (FR-012) | `cocoapods-main-module` |

`cocoapods-` prefix avoids collision per the milestone-122 / 137 / 138 precedent.

## R6: Integration site within `read_all`

**Decision**: register `pub mod cocoapods;` in `mikebom-cli/src/scan_fs/package_db/mod.rs` (placed alphabetically between `pub mod cmake;` and `pub mod composer;`) and add a call site in `read_all` between `cmake::read` and `composer::read`.

```rust
out.extend(cocoapods::read(rootfs, include_dev, exclude_set));
```

No `collect_claimed_paths` integration. No divergence-record set (signature: returns `Vec<PackageDbEntry>` directly).

**`include_dev` parameter**: accepted but treated as no-op (CocoaPods doesn't carry runtime/dev classification at the Podfile.lock level — pod dependencies are runtime; test-target pods can be inferred from Podfile target blocks but per-target attribution is deferred to v1.1). Mirrors milestone-137/138 posture.

## R7: Multi-target / multi-project handling + per-file error posture

**Decision**: per FR-010 + FR-011 + Q1+Q3 clarifications.

Algorithm:

1. **Walker pass A** — discover every `Podfile.lock` under the scan root via `safe_walk`. For each:
   1. Parse via `serde_yaml`. On error → warn + skip project entirely (or fall back to sibling Podfile design-tier if it exists).
   2. Check for sibling `Podfile` to derive main-module name; if absent, use parent-dir basename per Q1.
   3. Emit main-module per FR-012; emit one component per `PODS:` entry.
   4. Skip walking the project's `Pods/Manifest.lock` (FR-011 — would double-count).

2. **Walker pass B** — discover every `Pods/Manifest.lock` whose PARENT project root has no sibling `Podfile.lock` (per FR-011). For each: parse + emit at `deployed`-tier per Q3.

3. **Walker pass C** — for projects with `Podfile` but no `Podfile.lock` (per pass A miss), emit design-tier components from `Podfile`'s `pod` lines per FR-005.

4. **Same-PURL dedup**: standard orchestrator-level `seen_purls` HashSet. First entry wins.

**Per-file error matrix**:

| Condition | Behavior | Justification |
|---|---|---|
| Source tree has no `Podfile.lock` / `Podfile` / `Pods/Manifest.lock` | Return `Vec::new()` | Clean no-op (FR-006) |
| `Podfile.lock` parses; sibling `Podfile` parseable | Standard source-tier emission + Podfile-derived main-module | Common case |
| `Podfile.lock` parses; no Podfile OR Podfile target block missing | Source-tier emission + dir-basename main-module per Q1 | Lockfile-only / container scans |
| `Podfile.lock` malformed; sibling `Podfile` present | Warn + design-tier from Podfile per FR-007 | Best-effort preservation |
| `Podfile.lock` malformed; no sibling Podfile | Warn + skip project | Cannot recover |
| `Manifest.lock` parses; no sibling `Podfile.lock` | Deployed-tier emission per Q3 + R4 | Container layer scans |
| `Manifest.lock` parses; sibling `Podfile.lock` exists | Ignore Manifest.lock per FR-011 | Avoid double-counting |
| Per-entry malformed (missing version, malformed PODS shape) | Warn + skip single entry | Forward compat |
| Empty `PODS:` block | Emit just main-module; no warning | Fresh-failed `pod install` |
| Multi-level subspec name contains chars unsafe in PURL subpath | Percent-encode segments per RFC 3986; preserve raw `/` between segments | R5 |

## R8: Performance considerations

**Decision**: no performance budget violations expected.

- Per-project: read `Podfile.lock` (~10–30 KB typical), parse via `serde_yaml`. Estimated 2–8 ms per project on warm cache.
- Typical Firebase-using iOS app (~100 pods post-subspec-expansion): ~5 ms total. Heavy enterprise app (~250 pods): ~15 ms.
- Source-tree walker discovery cost: same as composer's existing walker — sub-millisecond on typical repos, ~100 ms on heavy container scans.

The no-CocoaPods-detected fast path: walker finds no relevant files; reader returns empty Vec; statistically free.

---

## Summary of Phase 0 resolutions

| Unknown | Decision | Reference |
|---|---|---|
| Principle V audit | `cocoapods` is purl-spec-blessed; SUBPATH for subspecs (NOT qualifier — Phase 0 correction) | R1 |
| `Podfile.lock` schema | Parse typed subset + `serde_yaml::Value` dispatch for `PODS:` polymorphism | R2 |
| `Podfile` DSL extraction | Regex line-by-line; no Ruby eval; gem-reader posture | R3 |
| `Manifest.lock` semantics | Byte-equivalent to Podfile.lock; deployed-tier when standalone (Q3) | R4 |
| purl-spec canonical form | `#subpath` for subspecs; case-preserved names; root-keyed SPEC CHECKSUMS | R5 |
| Integration site | `read_all` dispatcher alphabetically between cmake and composer | R6 |
| Multi-project / error posture | Three walker passes; warn-and-skip per FR-007 | R7 |
| Performance | ~15 ms on heavy enterprise iOS app; no budget concerns | R8 |

All Phase 0 unknowns resolved. Ready for Phase 1 (data-model + contracts + quickstart).
