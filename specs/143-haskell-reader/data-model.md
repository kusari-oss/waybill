# Data Model — milestone 143 Haskell reader (Phase 1)

Defines parsed in-memory representations of `cabal.project.freeze`, `stack.yaml.lock`, `stack.yaml`, `cabal.project`, `*.cabal`, and `package.yaml` (detect-only) + their mapping to `PackageDbEntry` flowing through the existing `read_all` pipeline.

## 1. Input artifacts

### 1.1 `cabal.project.freeze`

cabal-install line-format pinning file. Single top-level `constraints:` keyword followed by comma-separated entries (multi-line continuations permitted). Each entry is one of:

- Exact pin: `<package-name> ==<version>`
- Flag toggle: `<package-name> +<flag-name>` / `<package-name> -<flag-name>` — SKIPPED per Edge Case
- Range constraint: `<package-name> >=<version> && <<version>` (or any cabal-syntax range) — design-tier emission with range preserved

### 1.2 `stack.yaml.lock`

YAML lockfile produced by Stack 2.1+. Parsed via `serde_yaml::from_str::<StackYamlLock>(text)`. Schema (relevant fields):

```yaml
# Lock file, version 1
snapshots:
  - completed:
      sha256: "<64-hex>"
      size: <integer>
      url: "https://..."
    original:
      resolver: lts-22.0
packages:
  - completed:
      hackage: aeson-2.2.0.0@sha256:<hash>,<size>
      pantry-tree: {...}
    original:
      hackage: aeson-2.2.0.0
```

Per the Q3-style content-shape gate, the reader requires top-level `snapshots:` as an array before treating the file as authoritative; failing files warn-and-skip.

### 1.3 `stack.yaml`

Stack project config. YAML. Relevant fields:

- `resolver:` — Stackage snapshot identifier (`lts-22.0` / `nightly-2024-01-15` / `ghc-9.6.4`)
- `packages:` — local-package list (ignored per research §R7; filesystem walk handles discovery)
- `extra-deps:` — additional pins beyond the snapshot (parsed when `stack.yaml.lock` absent)

### 1.4 `cabal.project`

Multi-package project descriptor. Cabal-DSL format. Per research §R7, the reader IGNORES this file's `packages:` field for discovery (filesystem walk catches all `*.cabal`s); the file's PRESENCE is just a reader-activation signal per FR-001.

### 1.5 `*.cabal`

Per-package descriptor. Cabal-DSL line-format-with-indentation. Top-level fields outside any stanza: `name:`, `version:`, `license:`, `author:`, etc. Stanza opener regex: `^(library|executable|test-suite|benchmark|foreign-library)(?:\s+(\S+))?\s*$`. Each stanza contains indented `<field>: <value>` lines including `build-depends:` and `build-tool-depends:`.

### 1.6 `package.yaml` (detect-only, Q3)

Hpack source-of-truth. The reader DOES NOT parse this file — only detects its presence alongside a Hpack-generated `*.cabal` (header regex match) to emit the FR-015 diagnostic per Q3.

## 2. Parsed intermediate types

### 2.1 `CabalFreezeEntry` (private to `haskell.rs`)

```rust
#[derive(Debug, Clone)]
enum CabalFreezeEntry {
    ExactPin {
        name: String,    // lowercased
        version: String,
    },
    RangeConstraint {
        name: String,
        range: String,   // raw range string preserved verbatim
    },
    // FlagToggle skipped at parse time per Edge Case
}
```

### 2.2 `StackLockEntry` (private to `haskell.rs`)

```rust
#[derive(Debug, Clone)]
enum StackLockEntry {
    /// Hackage extra-dep: `original.hackage = "aeson-2.2.0.0"` (parsed by splitting on the LAST dash).
    Hackage {
        name: String,        // lowercased
        version: String,
    },
    /// Git-source extra-dep: `original: {git: ..., commit: ...}` — out of scope for v1; warn-and-skip.
    Git { /* unused — caller filters out */ },
}

#[derive(Debug, Clone)]
struct StackSnapshot {
    resolver: String,        // from snapshots[].original.resolver (e.g., "lts-22.0")
    sha256: Option<String>,  // from snapshots[].completed.sha256
}
```

### 2.3 `CabalManifest` (private to `haskell.rs`)

```rust
#[derive(Debug, Clone, Default)]
struct CabalManifest {
    name: Option<String>,            // from top-level `name:`; lowercased
    version: Option<String>,         // from top-level `version:`
    stanzas: Vec<CabalStanza>,
    hpack_generated: bool,           // true when HPACK_HEADER_RE matches; drives Q3 warn
}

#[derive(Debug, Clone)]
struct CabalStanza {
    kind: StanzaKind,                // Library | Executable | TestSuite | Benchmark | ForeignLibrary
    label: Option<String>,           // executable/test-suite/benchmark name; None for library
    build_depends: Vec<DeclaredDep>,
    build_tool_depends: Vec<DeclaredDep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StanzaKind {
    Library,
    Executable,
    TestSuite,
    Benchmark,
    ForeignLibrary,
}

#[derive(Debug, Clone)]
struct DeclaredDep {
    name: String,        // lowercased per Hackage casing convention
    range: Option<String>, // raw range string when present; None when absent (cabal-syntax permits bare names)
}
```

### 2.4 `LifecycleScope` derivation per Q2

```rust
fn stanza_lifecycle_scope(kind: StanzaKind) -> LifecycleScope {
    match kind {
        StanzaKind::Library | StanzaKind::Executable | StanzaKind::ForeignLibrary => LifecycleScope::Runtime,
        StanzaKind::TestSuite | StanzaKind::Benchmark => LifecycleScope::Development,
    }
}
```

`build-tool-depends` always maps to `LifecycleScope::Development` regardless of the surrounding stanza per FR-010.

Q2 most-binding precedence: when the same dep name appears in multiple stanzas, take the `min` of the stanza scopes' "binding strength" (Runtime > Development). Implemented as:

```rust
fn merge_scope(existing: LifecycleScope, new: LifecycleScope) -> LifecycleScope {
    match (existing, new) {
        (LifecycleScope::Runtime, _) | (_, LifecycleScope::Runtime) => LifecycleScope::Runtime,
        _ => LifecycleScope::Development,
    }
}
```

## 3. GHC boot-library allowlist (Q1, FR-014)

Hardcoded `const GHC_STDLIB_ALLOWLIST: &[&str]` at module scope:

```rust
const GHC_STDLIB_ALLOWLIST: &[&str] = &[
    "base",
    "ghc-prim",
    "template-haskell",
    "integer-gmp",
    "integer-simple",
    "array",
    "bytestring",
    "containers",
    "deepseq",
    "directory",
    "filepath",
    "ghc",
    "mtl",
    "parsec",
    "pretty",
    "process",
    "stm",
    "text",
    "time",
    "transformers",
    "unix",
    "Win32",
];
```

Lookup at emission time: `if GHC_STDLIB_ALLOWLIST.iter().any(|s| s.eq_ignore_ascii_case(&entry.name)) { add mikebom:ghc-stdlib annotation }`. Per FR-014, the annotation is informational and does NOT gate emission.

## 4. Output mapping → `PackageDbEntry`

### 4.1 Source-tier emission from `cabal.project.freeze` exact-pin

```rust
PackageDbEntry {
    purl: Purl::new(&format!("pkg:hackage/{}@{}", entry.name, entry.version))?,
    name: entry.name.clone(),
    version: Some(entry.version.clone()),
    extra_annotations: {
        let mut m = btree_map! {
            "mikebom:source-type" => json!("hackage-freeze"),
            "mikebom:evidence-kind" => json!("cabal-freeze"),
        };
        if GHC_STDLIB_ALLOWLIST.iter().any(|s| s.eq_ignore_ascii_case(&entry.name)) {
            m.insert("mikebom:ghc-stdlib".to_string(), json!("true"));
        }
        m
    },
    sbom_tier: Some("source".to_string()),
    lifecycle_scope: Some(LifecycleScope::Runtime),  // freeze entries are runtime unless context-tagged later
    // ... other PackageDbEntry fields default-initialized
}
```

### 4.2 Source-tier emission from `stack.yaml.lock` extra-deps

Identical to §4.1 but with `mikebom:source-type = "hackage-stack-lock"` + `mikebom:evidence-kind = "stack-yaml-lock"`.

### 4.3 Snapshot placeholder emission (FR-005)

```rust
let purl_str = match snapshot.resolver.as_str() {
    r if r.starts_with("lts-") || r.starts_with("nightly-") => {
        format!("pkg:generic/stackage-{}@{}", r, snapshot.sha256.as_deref().unwrap_or("unspecified"))
    }
    r if r.starts_with("ghc-") => {
        format!("pkg:generic/{}@{}", r, snapshot.sha256.as_deref().unwrap_or("unspecified"))
    }
    r => {
        // Unknown resolver shape — defensive fallback
        format!("pkg:generic/{}@{}", r, snapshot.sha256.as_deref().unwrap_or("unspecified"))
    }
};

PackageDbEntry {
    purl: Purl::new(&purl_str)?,
    name: snapshot.resolver.clone(),
    version: snapshot.sha256.clone().unwrap_or_else(|| "unspecified".to_string()),
    extra_annotations: btree_map! {
        "mikebom:source-type" => json!("hackage-snapshot"),
        "mikebom:evidence-kind" => json!("stack-yaml-lock"),
        "mikebom:stackage-resolver" => json!(snapshot.resolver.clone()),
    },
    sbom_tier: Some(if snapshot.sha256.is_some() { "source" } else { "design" }.to_string()),
    lifecycle_scope: Some(LifecycleScope::Runtime),
    // ...
}
```

### 4.4 Design-tier emission from `*.cabal` (no lockfile)

For each `DeclaredDep` from the Q2-unioned stanzas:

```rust
let sanitized_range = dep.range.as_deref().unwrap_or("unspecified");
let purl_str = format!("pkg:hackage/{}@{}", dep.name, sanitize_purl_version(sanitized_range));

PackageDbEntry {
    purl: Purl::new(&purl_str)?,
    name: dep.name.clone(),
    version: Some(sanitized_range.to_string()),
    extra_annotations: {
        let mut m = btree_map! {
            "mikebom:source-type" => json!("hackage-cabal-design"),
            "mikebom:evidence-kind" => json!("cabal-pkg-descriptor"),
            "mikebom:requirement-range" => json!(dep.range.clone().unwrap_or_default()),
        };
        if GHC_STDLIB_ALLOWLIST.iter().any(|s| s.eq_ignore_ascii_case(&dep.name)) {
            m.insert("mikebom:ghc-stdlib".to_string(), json!("true"));
        }
        m
    },
    sbom_tier: Some("design".to_string()),
    lifecycle_scope: Some(per_q2_merged_scope),  // result of merge_scope across stanzas
    // ...
}
```

### 4.5 Main-module emission per `*.cabal` (FR-013)

```rust
let main_name = manifest.name.clone().unwrap_or_else(|| parent_dir_basename(path));
let main_version = manifest.version.clone().unwrap_or_else(|| "0.0.0-unknown".to_string());
let purl_str = format!("pkg:hackage/{}@{}", main_name, main_version);

PackageDbEntry {
    purl: Purl::new(&purl_str)?,
    name: main_name,
    version: Some(main_version),
    extra_annotations: btree_map! {
        "mikebom:component-role" => json!("main-module"),
        "mikebom:source-type" => json!("hackage-main-module"),
    },
    sbom_tier: Some(if has_lockfile { "source" } else { "design" }.to_string()),
    depends: union_of_all_stanza_dep_names,
    // ... per FR-006 + Q2 union
}
```

The main-module itself does NOT carry `mikebom:ghc-stdlib` (it's never a boot library) or `mikebom:stackage-resolver` (the snapshot is a sibling component, not the package itself).

## 5. Reader entry-point flow

```rust
pub fn read(rootfs: &Path, _include_dev: bool, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();

    // Phase A — discover all artifacts.
    let cabal_paths = discover_cabal_files(rootfs, exclude_set);
    let freeze_paths = discover_cabal_freezes(rootfs, exclude_set);
    let stack_lock_paths = discover_stack_locks(rootfs, exclude_set);
    let stack_yaml_paths = discover_stack_yamls(rootfs, exclude_set);
    let package_yaml_paths = discover_package_yamls(rootfs, exclude_set);

    // FR-008 / SC-004: no-op when no Haskell artifacts present.
    if cabal_paths.is_empty() && freeze_paths.is_empty()
        && stack_lock_paths.is_empty() && stack_yaml_paths.is_empty() {
        return out;
    }

    // Phase B — parse each artifact (warn-and-skip on per-file error per FR-009).
    let freeze_data = parse_cabal_freezes(&freeze_paths);
    let (stack_lock_data, stack_snapshots) = parse_stack_locks(&stack_lock_paths, &stack_yaml_paths);
    let cabal_manifests = parse_cabal_manifests(&cabal_paths);

    // Phase C — Q3 Hpack-detect-and-warn (FR-015).
    emit_hpack_warnings(&cabal_manifests, &package_yaml_paths);

    // Phase D — emit components.
    // D1: freeze entries → source-tier (§4.1)
    for entry in freeze_data {
        if let Some(component) = build_freeze_component(&entry) {
            let purl_key = component.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(component);
            }
        }
    }
    // D2: stack lockfile extra-deps → source-tier (§4.2)
    for entry in stack_lock_data {
        if let Some(component) = build_stack_lock_component(&entry) {
            let purl_key = component.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(component);
            }
        }
    }
    // D3: stackage snapshot placeholders (§4.3)
    for snapshot in stack_snapshots {
        if let Some(component) = build_snapshot_placeholder(&snapshot) {
            let purl_key = component.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(component);
            }
        }
    }
    // D4: per-*.cabal main-module + design-tier emission when no lockfile (§4.4 + §4.5)
    for (cabal_path, manifest) in &cabal_manifests {
        let has_lockfile = lockfile_present_for(cabal_path, &freeze_paths, &stack_lock_paths);
        if let Some(main) = build_main_module(manifest, cabal_path, has_lockfile) {
            let purl_key = main.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main);
            }
        }
        if !has_lockfile {
            for component in build_design_tier_components(manifest, cabal_path) {
                let purl_key = component.purl.as_str().to_string();
                if seen_purls.insert(purl_key) {
                    out.push(component);
                }
            }
        }
    }

    out
}
```

**Performance note** (per Technical Context): ≤2 ms per freeze line. Heavy multi-package project (~400 deps across 5 sub-packages): ~15 ms. `seen_purls` HashSet dominates the dedup loop; for typical scans the O(N) PURL lookups are O(1) amortized.

## 6. Cross-format emission

### 6.1 CycloneDX 1.6

Each `PackageDbEntry` flows through the existing `mikebom-cli/src/generate/cyclonedx/builder.rs` pipeline. The `mikebom:evidence-kind` value-set MUST be extended to include `"cabal-freeze"`, `"stack-yaml-lock"`, and `"cabal-pkg-descriptor"` (per the milestone-141 + 142 precedent).

The `mikebom:source-type` allowlist (if curated) extends with `"hackage-freeze"`, `"hackage-stack-lock"`, `"hackage-snapshot"`, `"hackage-cabal-design"`, `"hackage-main-module"`. The `mikebom:ghc-stdlib` and `mikebom:stackage-resolver` annotations flow through the builder's general `extra_annotations` propagation path (per milestone-141 precedent — the builder is permissive about `mikebom:*` namespace; only `evidence-kind` is strict-allowlist-validated).

If `mikebom:stackage-resolver` needs propagation to `metadata.component` when a snapshot placeholder is the dominant project (rare — usually a regular `*.cabal` main-module wins), apply the milestone-142 F6 pattern: extend `metadata.rs`'s curated allowlist. **Decision deferred to implementation**: verify empirically during US2 test development; current expectation is no propagation needed (snapshot placeholder is never the main-module).

### 6.2 SPDX 2.3 + SPDX 3.0.1

Per the milestone-071 annotation parity layer, all `mikebom:*` annotations surface via the standard `annotations[]` shape (SPDX 2.3) or document-scope `Annotation` (SPDX 3) with the `MikebomAnnotationCommentV1` envelope. No special handling needed.

`mikebom:lifecycle-scope` for test/benchmark/build-tool deps flows through the milestone-052 native-field path (CDX `scope` / SPDX 2.3 `*_DEPENDENCY_OF` / SPDX 3 `LifecycleScopeType`); not an `mikebom:*` annotation.

## 7. Validation table

| Rule | Source | Enforcement site |
|---|---|---|
| `pkg:hackage/` PURL shape | research §R1 + purl-spec | `build_freeze_component` + `build_stack_lock_component` + `build_design_tier_components` + `build_main_module` |
| Hackage names lowercased | research §R1 + purl-spec hackage-definition | regex capture step in `parse_cabal_freezes` + `parse_stack_locks` |
| `pkg:generic/<resolver>@<sha>` snapshot placeholder | research §R5 + FR-005 | `build_snapshot_placeholder` |
| Q1 GHC-stdlib annotation on allowlisted names | spec FR-014 + research §R6 audit | §4.1 + §4.4 emit steps (allowlist match) |
| Q2 multi-stanza union + most-binding-scope precedence | spec FR-006 + research §R4 | `merge_scope` + `build_design_tier_components` |
| Q3 Hpack-detect-and-warn | spec FR-015 + research §R4 | `emit_hpack_warnings` (Phase C) |
| Flag-only constraints skipped | spec Edge Case + FR-002 | `parse_cabal_freezes` skip branch |
| Range constraints → design-tier with mikebom:requirement-range | spec FR-002 trailing clause | `parse_cabal_freezes` range-branch emit |
| Test/benchmark/build-tool-depends → development scope | spec FR-010 + Q2 | `stanza_lifecycle_scope` + build_tool_depends override |
| Multiple *.cabal in one dir → alphabetically-first wins | spec FR-013 + Edge Case | `parse_cabal_manifests` dedup step |
| Main-module fallback (name = parent-dir basename, version = "0.0.0-unknown") | spec FR-013 | `build_main_module` |
| Per-file warn-and-skip | spec FR-009 | top-level `read()` match arms |
| No-op on non-Haskell trees | spec FR-008 + SC-004 | top-level `read()` early return |
| No network access | spec FR-012 | static — no `reqwest`/`tokio` use in `haskell.rs` |
| Content-shape gate on stack.yaml.lock | research §R3 (mirrors milestone-142 Q3) | `parse_stack_locks` validation step |
