# Reading monorepo SBOMs

**Audience**: security teams, dashboard authors, SBOM-processing tools, compliance reporters — anyone consuming a mikebom SBOM from a repository that contains **more than one project boundary** (multi-workspace pip / npm / cargo / go / maven / etc.).

**Purpose**: this doc is the destination of the `monorepo shape detected: ...` advisory log line mikebom emits at scan time when N > 1 workspaces are found. It teaches you how to slice a monorepo SBOM per-workspace via `jq`, why mikebom chose an annotations-only approach (vs restructuring the SBOM), and how to compose scoped consumption workflows.

**Baseline signals introduced by milestone 176**:

| Row | Annotation | Scope | Emitted when |
|---|---|---|---|
| C120 | `mikebom:workspace-member` | per-component | component has ≥1 derivable workspace root (i.e., it came from a manifest / lockfile — not file-tier) |
| C121 | `mikebom:workspaces-detected` | document | scan detected ≥1 workspace (union of every C120 value non-empty) |

Both wire shapes: JSON-encoded array of workspace root-relative paths in a string. Forward-slash separator on all platforms per FR-010.

---

## 1. What counts as a "workspace"?

A **workspace** is any directory (relative to the scan root, or the scan root itself) that mikebom's package-database readers identify as a project boundary — anchored by:

- **pyproject.toml** — pip / uv / poetry
- **package.json** — npm / pnpm / yarn
- **Cargo.toml** — cargo
- **go.mod** — go
- **pom.xml** — maven
- **Gemfile** / **\*.gemspec** — bundler
- **build.gradle{.kts}** — gradle
- **Package.swift** — swift
- **libs.versions.toml** — kotlin (catalog)
- **{apk,dpkg,rpm}/db/…** — installed OS-package databases (image scans)
- **requirements\*.txt** / **Pipfile.lock** / **uv.lock** / **poetry.lock** at directory roots without a matching pyproject.toml (design-tier pip)
- \...and every equivalent readers add in future milestones.

**Reused, not reinvented**: milestone 176 does not add new detection logic. If a reader already emits a main-module component for a workspace member (as pip does for langflow's 9 workspaces per m068; npm for its 2 workspaces per m066), that workspace path becomes a first-class C120 value automatically. See `reading-a-mikebom-sbom.md` §3.1 for the C120/C121 wire contract; see `sbom-format-mapping.md` for the audit against CDX / SPDX 2.3 / SPDX 3 native constructs (KEEP-NO-NATIVE).

**Sentinel value `"."`**: the scan root itself is a first-class workspace path when a manifest lives at the root. If your scan target is `/src/langflow` and there's a root `pyproject.toml` alongside `src/frontend/package.json`, C121 will contain `[".", "src/frontend"]`. The `"."` sentinel matches the m068 pip precedent for "workspace at scan root."

## 2. Per-workspace filtering — the primary use case

You received a CVE against `pkg:pypi/pyyaml` and want to know which of your subprojects declare or lock it. One jq call:

```bash
jq -r '.components[]
       | select(.purl | startswith("pkg:pypi/pyyaml"))
       | .properties[]?
       | select(.name == "mikebom:workspace-member")
       | .value | fromjson | .[]' scan.cdx.json
```

Output: one workspace-path line per affected subproject. Feed it to your triage pipeline to scope remediation.

**Inverse query** — "which components does workspace `src/frontend` own?":

```bash
jq -r '.components[]
       | select((.properties[]?
                 | select(.name == "mikebom:workspace-member")
                 | .value | fromjson
                 | contains(["src/frontend"])))
       | .purl' scan.cdx.json
```

Output: the PURLs pinned or declared in `src/frontend`, INCLUDING any hoisted / shared deps whose C120 array names both `src/frontend` and other workspaces. That's the correct behavior — a shared dep is present in every workspace it's pinned in, not just the "first" one.

## 3. Enumerating workspaces without walking components

`components[]` is often thousands of entries in a real monorepo scan. For "how many subprojects does this SBOM cover?" and "what are they?", walk C121 at doc-scope directly:

```bash
jq '.metadata.properties[]?
    | select(.name == "mikebom:workspaces-detected")
    | .value | fromjson' scan.cdx.json
```

Output: the sorted array of every workspace path. The value is guaranteed by construction to equal the union of every per-component `mikebom:workspace-member` value (the FR-012 cross-annotation invariant) — no need to double-check by walking `components[]`.

**Verify the invariant** (integrity check for archival SBOMs):

```bash
jq '
  [.components[]?.properties[]?
   | select(.name == "mikebom:workspace-member")
   | .value | fromjson | .[]] | unique as $union
  | .metadata.properties[]?
  | select(.name == "mikebom:workspaces-detected")
  | .value | fromjson
  | {union: $union, detected: ., match: (. == $union)}
' scan.cdx.json
```

`.match` MUST be `true` for any post-176 SBOM. `false` would indicate emission drift or hand-editing.

## 4. Format-neutral consumption

The recipes above target CDX 1.6. The same signals ride SPDX 2.3 and SPDX 3.0.1 too — the wire location differs but the semantics are identical (validated by the C120/C121 `SymmetricEqual` parity extractors).

### SPDX 2.3

C120 lives in each Package's `annotations[]` as a `MikebomAnnotationCommentV1` envelope with `field = "mikebom:workspace-member"`. C121 is a document-scope annotation on the `SpdxDocument`.

```bash
jq -r '.packages[]
       | select(.annotations[]?
                | .comment | fromjson?
                | .field == "mikebom:workspace-member"
                and (.value | fromjson | contains(["src/frontend"])))
       | .name + " " + (.versionInfo // "no-version")' scan.spdx.json
```

### SPDX 3.0.1

Both C120 and C121 are typed `Annotation` graph elements targeting the Package IRI (C120) or `SpdxDocument` root IRI (C121):

```bash
jq -r '.["@graph"][]
       | select(.type == "Annotation" and (.statement | fromjson? | .field == "mikebom:workspaces-detected"))
       | .statement | fromjson | .value | fromjson | .[]' scan.spdx3.json
```

## 5. Composition patterns

### Pattern A — per-workspace CVE dashboard

```bash
# Given a CVE feed at cves.json (array of {purl_prefix, cve_id}),
# emit a per-workspace impact matrix.
jq --slurpfile cves cves.json '
  ($cves[0]) as $cves
  | reduce .components[] as $c ({};
      ($c.properties[]? | select(.name == "mikebom:workspace-member") | .value | fromjson) as $wss
      | reduce ($cves[]) as $cve (.;
          if ($c.purl | startswith($cve.purl_prefix))
          then reduce $wss[] as $ws (.;
                 .[$ws] += [$cve.cve_id])
          else . end))
' scan.cdx.json
```

Output: `{"src/frontend": ["CVE-2024-1234"], "src/backend": ["CVE-2024-1234", "CVE-2024-5678"]}` — one entry per affected workspace, listing every CVE hit.

### Pattern B — per-workspace license inventory

```bash
jq -r '
  .components[]
  | (.properties[]? | select(.name == "mikebom:workspace-member") | .value | fromjson) as $wss
  | .licenses[]?.license.id? as $lic
  | select($lic)
  | $wss[] as $ws
  | [$ws, $lic] | @tsv
' scan.cdx.json | sort -u
```

Output: TSV of `(workspace, license_spdx_id)` unique pairs — a per-subproject license report.

## 6. What C120/C121 do NOT restructure

**Zero SBOM shape changes** (per FR-008): mikebom's `components[]` array remains flat. `dependencies[]` edge graph is unchanged. `metadata.component` (the CDX BOM subject) is still one auto-selected root — see m127 for the selection heuristic. The 10-way ambiguous root selection on langflow is UNCHANGED post-176 (`langflow-base@0.10.2` still wins the coin flip). But now consumers can slice the SBOM per-workspace regardless of that choice.

**Follow-up milestones will build on C120 as the substrate**:

- **m177 (candidate)** — nested CDX composition per workspace via `metadata.component.components[]`. C120 is the workspace-membership signal; m177 would restructure emission around it.
- **m178 (candidate)** — per-workspace multi-SBOM emission. One `<workspace>.cdx.json` per detected workspace, plus the current merged SBOM as an index.

Both follow-ups treat m176 as the primitive — emit once, restructure many.

## 7. Advisory-log integration in CI

When mikebom's scan detects N > 1 workspaces AND produces ≥1 component, it emits exactly one INFO-level log line on stderr containing the stable substring `"monorepo shape detected: "`. Wire this into your CI dashboards:

```bash
mikebom sbom scan --path . --output scan.cdx.json 2> scan.stderr
if grep -qF 'monorepo shape detected: ' scan.stderr; then
  echo "::notice::mikebom detected a monorepo shape — consumers can filter per-workspace via mikebom:workspace-member"
fi
```

Suppressed on single-project scans (N ≤ 1) — no noise on non-monorepo repos. Not gated on `--offline` — the remediation is entirely consumer-side jq slicing (no network needed).

## 8. Cross-references

- **[reading-a-mikebom-sbom.md §3.1](reading-a-mikebom-sbom.md)** — full wire contract for C120 + C121 alongside every other vulnerability-scoping signal.
- **[sbom-format-mapping.md — Section C](sbom-format-mapping.md)** — C120 + C121 rows with the KEEP-NO-NATIVE rejected-alternatives audit.
- **[component-tiers.md](component-tiers.md)** — the m133 file-tier discriminator that file-tier components use INSTEAD of C120 (file-tier components have no workspace attribution by definition).

## 9. Milestone context

- **m176 (this feature)** — C120 per-component + C121 doc-scope emission across CDX / SPDX 2.3 / SPDX 3.0.1. Advisory-log gate. Zero SBOM shape change.
- **m173** — cache-warming advisory log precedent (identical grep-substring stability contract; different substring `"Prime the cache with --warm-go-cache="`).
- **m147** — `mikebom:peer-edge-targets` array-in-string wire shape precedent.
- **m134** — `mikebom:purl-collisions-detected` array-in-string wire shape precedent.
- **m127** — root-component selection heuristic (unchanged post-176).
- **m133** — file-tier component emission (file-tier explicitly excluded from C120 per FR-002).
- **m066 / m068 / m064 / m053 / m070** — per-ecosystem reader main-module logic that populates the workspace-boundary signal C120 reuses.
