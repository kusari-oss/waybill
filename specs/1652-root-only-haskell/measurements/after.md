# Spec 1652: after the change

**Binaries:**
- `main` at `317fbae6`, built from a `git worktree` at `main`.
- The branch, built from the working tree.

Both builds are debug builds, and every scan is `--offline` with an isolated `HOME`.

## T023: nothing else changes (FR-007, SC-004)

**What was scanned:** every fixture under `waybill-cli/tests/fixtures/project_discovery_haskell/` (f1–f6), by both binaries, in all three formats. Three ways:
- the default `all`;
- `--split=workspace`;
- `--split=directory`.

**How it was compared:** both trees were masked the same way, UUIDs to `<UUID>` and RFC 3339 timestamps to `<TS>`, rewritten with sorted keys, and compared with `diff -r`.

| | files | differing |
|---|---|---|
| `main` | 144 | — |
| branch | 144 | **0** |

**Positive control:** the same pipeline on f1 under `--project-discovery=root-only` reports 12 differing lines. These are:
- C104 `complete` → `partial`;
- the new C105 reason;
- C244;
- C200's new key.

So a zero above means equal, not unexamined.
