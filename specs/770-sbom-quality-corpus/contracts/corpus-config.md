# Contract: Corpus Configuration

**File**: `xtask/corpus/quality-corpus.toml` (committed, hand-edited, reviewed)
**Consumer**: `xtask::quality::config`
**Stability**: Breaking changes to this shape require a spec amendment.

## C-1 — Top level

```toml
# Version of sbomqs this corpus's scores were authored against.
# MUST match the version .github/workflows/ci.yml installs.
sbomqs_version = "v2.0.6"

# Per-target scan budget unless overridden. Seconds.
default_timeout_secs = 600
```

## C-2 — A target

```toml
[[targets]]
name       = "go-cobra"
url        = "https://github.com/spf13/cobra"
sha        = "a655097faf7d54f78933a815984b9919d51a05d2"   # v1.9.1
ecosystem  = "go"                                          # documentation only; never gates
# timeout_secs = 900                                       # optional override

# Observed 2026-09-03 (offline):
#   wall=136ms sbomqs=7.59 pkgs=7 files=0 edges=7 depth=2 flat=false
#   waybill self-report: complete
[targets.expect]
# Every key optional. An absent key means "observe, never fail" (FR-020).
# pkgs      = { min = 5,   max = 15 }
# files     = { min = 0,   max = 5 }
# sbomqs    = { min = 7.0, max = 8.5 }
# edges     = { min = 5,   max = 20 }
# max_depth = { min = 2,   max = 6 }
# wall_ms   = { min = 10,  max = 5000 }
# flat      = false
```

**C-2.1** — `name` MUST be unique across the file. Duplicates are a configuration error.
**C-2.2** — `sha` MUST be 40 lowercase hex characters. A `ref = "main"` key is reserved for the
FR-003 moving-reference mode and MUST be rejected with an explicit "not yet supported" message
rather than ignored.
**C-2.3** — The `[targets.expect]` table MAY be absent entirely.
**C-2.4** — Every range is **inclusive** at both bounds. `min > max` is a configuration error.
**C-2.5** — `flat` is a bare boolean, not a range. `true` asserts the graph is expected to be
flat (legitimate for lockfile-less upstreams); `false` asserts it is expected to have depth.

## C-3 — Comment convention (normative for authors, not for the parser)

Each target SHOULD carry an `# Observed <date> (offline):` comment recording the measured
values at the time bounds were authored, plus waybill's self-report. This is the mechanism by
which hand-authored ranges stay explicable to the next reader, and it is the reason this file
is TOML rather than JSON (research R1).

## C-4 — Parse-time validation

The parser MUST reject, before any repository is fetched:

| Condition | Message class |
|---|---|
| duplicate `name` | configuration error |
| `sha` not 40 lowercase hex | configuration error |
| `ref` key present | configuration error — not yet supported |
| any range with `min > max` | configuration error |
| `sbomqs` bound outside 0.0–10.0 | configuration error |
| empty `targets` | configuration error |
| `sbomqs_version` empty | configuration error |

Configuration errors are reported **together** — all of them, not the first — and exit
non-zero without fetching anything.
