# Quickstart: verifying the m667 bun.lock edge fix

**Audience**: waybill maintainer + issue #723 reporter, verifying that the fix produces the promised outcome on their fixture(s).

## 5-step recipe

### Step 1 — Set up the minimal reproduction

Two files, no `node_modules`, no network.

`package.json`:
```json
{
  "name": "repro",
  "version": "1.0.0",
  "dependencies": {
    "parent-pkg": "1.0.0"
  }
}
```

`bun.lock`:
```jsonc
{
  "lockfileVersion": 1,
  "workspaces": {
    "": {
      "name": "repro",
      "dependencies": {
        "parent-pkg": "1.0.0"
      }
    }
  },
  "packages": {
    "parent-pkg": ["parent-pkg@1.0.0", "", { "dependencies": { "child-pkg": "^1.0.0" } }, "sha512-AAAA..."],
    "child-pkg": ["child-pkg@1.0.0", "", {}, "sha512-BBBB..."]
  }
}
```

### Step 2 — Baseline against pre-fix waybill (skip if you don't have a pre-fix binary handy)

```bash
waybill-v0.2.0 sbom scan --path . --offline --format cyclonedx-json --output pre-fix.cdx.json
grep 'graph completeness\|orphan_reason' pre-fix.cdx.json | head
```

**Pre-fix observation** (from issue #723):
```
graph completeness computed  value=partial  reachable_count=2  total_count=3  orphan_count=1
pkg:npm/child-pkg@1.0.0   →  hoisted-unused
```

### Step 3 — Run the post-fix build

Build waybill from the m667 branch (or use the release-tagged build once it lands):

```bash
cargo build --release --bin waybill
./target/release/waybill sbom scan --path . --offline --format cyclonedx-json --output post-fix.cdx.json
```

### Step 4 — Verify the fix's outcome

```bash
grep 'graph completeness' post-fix.cdx.json
jq '.dependencies[] | select(.ref == "pkg:npm/parent-pkg@1.0.0")' post-fix.cdx.json
jq '.components[] | select(.purl == "pkg:npm/child-pkg@1.0.0") | .properties[]? | select(.name == "waybill:orphan-reason")' post-fix.cdx.json
```

**Expected post-fix output**:
```
graph completeness computed  value=complete  reachable_count=3  total_count=3  orphan_count=0

{
  "ref": "pkg:npm/parent-pkg@1.0.0",
  "dependsOn": ["pkg:npm/child-pkg@1.0.0"]
}

# No output from the third jq command — the waybill:orphan-reason
# annotation is absent because child-pkg is now reachable.
```

### Step 5 — Diff pre-fix vs post-fix

```bash
diff <(jq -S '.dependencies' pre-fix.cdx.json) <(jq -S '.dependencies' post-fix.cdx.json)
```

Expected diff: one new edge line for `parent-pkg → child-pkg`.

## Verifying advanced scenarios

### Multi-version fixture (SC-004)

Encode the same package at two versions under two different parents:

```jsonc
{
  "packages": {
    "app": ["app@1.0.0", "", { "dependencies": { "big": "1.0.0", "small": "2.0.0" } }],
    "big": ["big@1.0.0", "", { "dependencies": { "minimatch": "^3.0.0" } }],
    "big/minimatch": ["minimatch@3.1.2", "", {}, "sha..."],
    "small": ["small@2.0.0", "", { "dependencies": { "minimatch": "^5.0.0" } }],
    "small/minimatch": ["minimatch@5.1.6", "", {}, "sha..."]
  }
}
```

Verify both edges land on the correct version copy:

```bash
jq '.dependencies[] | select(.ref == "pkg:npm/big@1.0.0")' out.cdx.json
# → dependsOn: ["pkg:npm/minimatch@3.1.2"]  ← NOT 5.1.6

jq '.dependencies[] | select(.ref == "pkg:npm/small@2.0.0")' out.cdx.json
# → dependsOn: ["pkg:npm/minimatch@5.1.6"]  ← NOT 3.1.2
```

### Scoped-name resolver fixture (SC-005)

```jsonc
{
  "packages": {
    "app": ["app@1.0.0", "", { "dependencies": { "@fast-csv/format": "^4.0.0" } }],
    "@fast-csv/format": ["@fast-csv/format@4.3.6", "", { "dependencies": { "@types/node": "^22.0.0" } }],
    "@fast-csv/format/@types/node": ["@types/node@22.5.0", "", {}, "sha..."]
  }
}
```

Verify the resolver picks the scope-nested `@types/node`:

```bash
jq '.dependencies[] | select(.ref == "pkg:npm/%40fast-csv/format@4.3.6")' out.cdx.json
# → dependsOn: ["pkg:npm/%40types/node@22.5.0"]  ← scope-nested version
```

### Optional-deps fixture (SC-006 for the m180 pattern)

```jsonc
{
  "packages": {
    "app": ["app@1.0.0", "", { "dependencies": { "parent": "1.0.0" } }],
    "parent": ["parent@1.0.0", "", { "optionalDependencies": { "opt-child": "^1.0.0" } }],
    "opt-child": ["opt-child@1.0.0", "", {}, "sha..."]
  }
}
```

Verify the edge is optional:

```bash
jq '.dependencies[] | select(.ref == "pkg:npm/parent@1.0.0")' out.cdx.json
# → the edge scope-decoration lives on the TARGET's component, not the edge itself
jq '.components[] | select(.purl == "pkg:npm/opt-child@1.0.0") | .scope' out.cdx.json
# → "optional"  (matches m180 CDX-emission convention)
jq '.components[] | select(.purl == "pkg:npm/opt-child@1.0.0") | .properties[]? | select(.name == "waybill:optional-derivation")' out.cdx.json
# → {"name": "waybill:optional-derivation", "value": "bun-optional-dependencies"}
```

## Troubleshooting

### "My real-world monorepo still shows some orphans"

The fix targets edges declared inside the lockfile's `packages` metadata. Orphans post-fix can indicate:

- **Unmet peer dependencies** — a `peerDependencies` entry whose target isn't in the lockfile at all. Pre-fix these silently dropped; post-fix they emit a `bun.lock edge dropped: parent=... dep=... reason=unresolved` warn-log. Legit gap in the lockfile, not a fix regression.
- **Optional dependencies whose target isn't installed** — same warn-log; expected behavior per npm semantics.
- **Non-npm ecosystems in the monorepo** — a Python or Ruby workspace with its own deps has its own reader and its own orphan behavior; unrelated to m667.

Filter the warn-log to size the residual:
```bash
waybill sbom scan --path <monorepo> --offline --format cyclonedx-json --output out.cdx.json 2>scan.log
grep -c 'bun.lock edge dropped' scan.log
```

If the count is small (dozens vs the pre-fix hundreds), the fix worked and the residuals are legit lockfile inconsistencies.

### "The edge exists but points at the wrong version"

Check `entry.depends` at the reader level via a debug print. If the reader's `depends` vec is correct but the emitted edge is wrong, it's a graph-builder / `name_to_purl` issue — file a follow-up bug. If the reader's `depends` vec is wrong, the R2 resolver has a bug — file against m667.

### "The fix emits an edge that shouldn't exist"

An edge to a non-lockfile-declared target = a fix regression against C1 (no phantom edges from name-collision resolution). File a follow-up bug with the fixture.

## Reference

- Spec: [`spec.md`](./spec.md)
- Plan: [`plan.md`](./plan.md)
- Research: [`research.md`](./research.md)
- Data model: [`data-model.md`](./data-model.md)
- Contract: [`contracts/depends-emission.md`](./contracts/depends-emission.md)
- Issue: [#723](https://github.com/kusari-oss/waybill/issues/723)
- m180 optional-derivation precedent: `waybill-cli/src/scan_fs/package_db/npm/package_lock.rs:310-324`
- m147 issue #262 `<name> <version>` disambiguation: `waybill-cli/src/scan_fs/mod.rs:635-644`
- m179 `LifecycleScope::Optional` + `RelationshipType::OptionalDependsOn`: `waybill_common/src/resolution.rs:415-525`
