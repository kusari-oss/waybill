# Contract — `--exclude-path` CLI surface

**Feature**: 113-exclude-path-flag

## Flag definition

```text
--exclude-path <PATH_OR_PATTERN>
```

| Property | Value |
|---|---|
| `clap` attribute | `#[arg(long, value_name = "PATH_OR_PATTERN", action = ArgAction::Append)]` |
| Repeatable | Yes (`ArgAction::Append`) |
| Default | empty vec |
| Scope | global flag on `mikebom scan` (positioned alongside `--exclude-scope`) |
| Help text (one-liner) | `Skip directory subtrees matching <PATH_OR_PATTERN> during scan. Repeat for multiple entries. Pattern entries (containing *, ?, or [) match at any depth; literal entries are anchored at scan root. Also via MIKEBOM_EXCLUDE_PATH. See <user-guide/cli-reference#--exclude-path>.` |

## Env-var fallback

```text
MIKEBOM_EXCLUDE_PATH=<entry-list>
```

| Property | Value |
|---|---|
| Separator | platform `path-list` separator — `:` on Unix, `;` on Windows |
| Empty entries | Silently dropped (matches shell `$PATH` behavior) |
| Combine rule | Env entries are concatenated AFTER CLI entries (deterministic order for the transparency annotation) |
| Precedence | Equal — union, not override |

## Validation timing

| Failure | When detected | Exit code | Message form |
|---|---|---|---|
| Empty entry (`--exclude-path ""`) | At parse, before any walker | 2 | `error: --exclude-path entry was empty` |
| Malformed pattern (unmatched bracket, invalid glob syntax) | At parse, before any walker | 2 | `error: --exclude-path entry <verbatim>: <globset::Error description>` |
| All other entries | Never fail at parse; benign no-match if path absent | n/a | n/a |

## Examples

```text
# Single literal path
mikebom scan --exclude-path tests/fixtures /path/to/repo

# Pattern matching any depth
mikebom scan --exclude-path '**/testdata' /path/to/repo

# Multiple entries (repeated flag)
mikebom scan --exclude-path tests/fixtures --exclude-path examples /path/to/repo

# Mixed literal + pattern via env var (Unix shell)
MIKEBOM_EXCLUDE_PATH='tests/fixtures:**/testdata' mikebom scan /path/to/repo
```

## What this contract DOES NOT cover

- Pattern dialect (resolved in research R1: `globset`)
- Per-walker integration plumbing (resolved in research R4)
- Transparency annotation payload (separate contract: `annotations.md`)
