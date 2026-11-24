# Data model: hpack `!include` and YAML merge keys

## Resolution of one `package.yaml` (in memory)

| Step | Input | Output |
|---|---|---|
| 1. Splice includes (R1, R5) | the manifest's text, its path, the scanned root | one text with every resolvable block-context `!include` replaced by its file's re-indented text; a list of `IncludeResolution` |
| 2. Rename merge keys (R3, R4) | step 1's text | the same text with each `<<` key renamed `<<__waybill_merge_<n>` |
| 3. Parse | step 2's text | a `serde_yaml::Value`; an alias with no anchor still fails here (`malformed-yaml`) |
| 4. Apply merges (R3) | the value | the value with every renamed key folded in under the measured precedence, and removed |
| 5. Existing path | the value | `strip_tagged` (an unresolved include becomes `partial`), typed deserialize, `lower` |

A manifest with no `!include` and no `<<` passes steps 1, 2 and 4 unchanged, so its output is byte-identical (FR-007).

## IncludeResolution

| Field | Meaning |
|---|---|
| `manifest` | the `package.yaml` the include belongs to (FR-005) |
| `file` | the file containing the `!include` (the manifest, or an included file) |
| `include` | the path as written |
| `outcome` | `Resolved`, or `Unresolved(reason)` with `reason` one of `missing`, `outside-root`, `cycle`, `too-deep`, `unreadable` or `unsupported-position` (an enum, matched exhaustively) |

## C200 additions (`waybill:hpack-read-outcome`, R6)

Each key appears only when its list is non-empty.

```json
"includes-resolved":   [{"manifest": "<package.yaml>", "file": "<path>", "include": "<as written>"}],
"includes-unresolved": [{"manifest": "<package.yaml>", "file": "<path>", "include": "<as written>", "reason": "<reason>"}]
```

- **A manifest with any unresolved include** is listed under `manifests-partial` with `unmodelled-construct: include`, as today.
- **A manifest whose includes all resolve**, and that uses no `defaults:`, is `manifests-read`.

## Merge precedence (R3), as a rule

For a mapping `M` with explicit keys `E` and merge sources `S1 … Sn` in document order, where a sequence-form source contributes its mappings in sequence order:

```
result(k) = E[k]        if k in E
          = Si[k]       for the smallest i with k in Si
```

A source that is not a mapping contributes nothing.
