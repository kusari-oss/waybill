# Contract: Multi-root BFS reachability algorithm

**Milestone 158** • The FR-008 + FR-012 reachability pass.

## Inputs

- `components: &[ResolvedComponent]` — the full component list at emit-time.
- `dependency_edges: &HashMap<PurlKey, Vec<PurlKey>>` — the assembled `dependsOn` graph. Keys are canonical PURL keys; values are the outbound-edge target PURL keys. AFTER the milestone-158 workspace-peer linkage step has run (so root's `dependsOn` already includes `RootSelectionResult.losers`).
- `selection: &RootSelectionResult` — output of `crate::generate::root_selector::select_root(...)`.

## Output

`GraphCompletenessResult` per data-model.md.

## Algorithm

### Step 1 — Compute seed set (EcosystemRootSet)

```text
seeds := {}
per_ecosystem_root := {}
ecosystems_without_root := []

# Primary root (from milestone 127's select_root)
if selection.subject has a PurlKey:
    seeds.add(primary_purl_key)
    per_ecosystem_root[primary_ecosystem] := primary_purl_key

# Per-ecosystem tops
main_modules := [c for c in components if c.extra_annotations["mikebom:component-role"] == "main-module"]
by_ecosystem := group_by(main_modules, key=c.purl.ecosystem())

for ecosystem, mods in by_ecosystem:
    if ecosystem in per_ecosystem_root: continue  # already have primary
    top := pick_ecosystem_top(mods)
    if top exists:
        seeds.add(purl_key(top))
        per_ecosystem_root[ecosystem] := purl_key(top)
    else:
        ecosystems_without_root.append(ecosystem)

# Also: for each ecosystem represented in `components` but not in per_ecosystem_root,
# add it to ecosystems_without_root (there are emitted components but no confident root).
observed_ecosystems := {c.purl.ecosystem() for c in components}
for eco in observed_ecosystems:
    if eco not in per_ecosystem_root and eco not in ecosystems_without_root:
        ecosystems_without_root.append(eco)
```

### Step 2 — Pick ecosystem top

Reuse the milestone-127 heuristic in a per-ecosystem scope:

```text
def pick_ecosystem_top(candidates: List[ResolvedComponent]) -> Option<ResolvedComponent>:
    if len(candidates) == 0:
        return None
    if len(candidates) == 1:
        return candidates[0]
    # Prefer workspace-root marker
    workspace_roots := [c for c in candidates if c.extra_annotations["mikebom:is-workspace-root"] == "true"]
    if len(workspace_roots) == 1:
        return workspace_roots[0]
    # Fall back to longest common prefix of manifest paths
    lcp_top := longest_common_prefix_pick(candidates)
    if lcp_top is not None:
        return lcp_top
    # Give up
    return None
```

### Step 3 — Multi-source BFS

```text
visited := seeds.clone()
queue := VecDeque::from(seeds)

while node := queue.pop_front():
    for neighbor in dependency_edges.get(node, []):
        if visited.insert(neighbor):
            queue.push_back(neighbor)
```

### Step 4 — Classify

```text
total_count := components.len()
reachable_count := visited.len()
orphan_count := total_count - reachable_count

reason_codes := []

if ecosystems_without_root:
    reason_codes.append(MultiEcosystemPartialRoot { ecosystems: ecosystems_without_root })

if orphan_count > 0:
    # Orphans NOT covered by ecosystems_without_root
    # (an ecosystem with no root contributes ALL its components as orphans;
    # if we already flagged multi-ecosystem-partial-root for it, don't
    # double-count via orphaned-components-detected)
    ecosystem_root_orphan_count := count of orphans WHOSE ecosystem is in ecosystems_without_root
    residual_orphans := orphan_count - ecosystem_root_orphan_count
    if residual_orphans > 0:
        reason_codes.append(OrphanedComponentsDetected { orphan_count: residual_orphans })

if selection.losers is populated AND count of losers linked to root != len(selection.losers):
    reason_codes.append(WorkspacePeerDetectionDegraded { linked, detected })

# Caution-first (Q1): if any reason code is present but its enum variant doesn't
# match a documented one (impossible today given closed enum, but defensive), emit unknown.

# Determine value
if reason_codes.is_empty() and reachable_count == total_count:
    value := Complete
elif reason_codes.is_empty() and reachable_count != total_count:
    # Should never happen — orphans exist but no classifier fired. Caution-first: unknown.
    value := Unknown
elif all reason codes classified:
    value := Partial
else:
    value := Unknown
```

## Complexity

- Step 1: O(V) — one pass over components.
- Step 2: O(k · log k) worst case where k is number of candidates per ecosystem (usually 1–5).
- Step 3: O(V + E) — standard BFS.
- Step 4: O(V) — one pass to classify.

Total: **O(V + E)** — meets FR-008.

## Edge cases

- **`components` is empty**: return `GraphCompletenessResult { value: Complete, ... , total_count: 0, reachable_count: 0, orphan_count: 0 }`. (An empty SBOM is trivially complete.)
- **`selection.subject` is `OperatorOverride`**: The override-supplied root is the primary; still runs multi-root BFS from it + per-ecosystem tops.
- **`selection.subject` is `SyntheticPlaceholder`**: Synthetic root has no `dependsOn` edges → BFS from it reaches only itself → orphans = everything else. Multi-ecosystem tops (if any) provide the real coverage. If NO real roots exist, `multi-ecosystem-partial-root` covers all real ecosystems + `unknown` may be emitted.
- **Cycles in the dep-graph** (npm circular deps): BFS's visited-set naturally handles cycles. No infinite loops.
- **Component in `components` but not in `dependency_edges`**: treat as a leaf (no outbound edges). Still counted for reachability if reachable.
- **Component reachable via `dependency_edges` but not in `components`** (broken index): drop silently — the reachability count is over `components`, not the edge closure.
