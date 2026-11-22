# Data model: spec 1643

All in-process, per binary. Nothing persists beyond the emitted document.

## `CuratedLibrary` (existing enum)

Unchanged variants. One new method:

- `boundary(self) -> Boundary`: an exhaustive `match`, no wildcard arm (FR-004, R3).

| variant | boundary |
|---|---|
| `Zlib`, `Expat`, `Ncurses` | `Anchored` |
| `OpenSsl`, `BoringSsl`, `Sqlite`, `Curl`, `Pcre`, `Pcre2`, `GnuTls`, `LibreSsl`, `Llvm`, `OpenJdk`, `Php`, `Node` | `NulOnly` |

## `Boundary` (new enum)

- `Anchored`: accepts when the match starts the region, or when the preceding byte is not in `[A-Za-z0-9_]` (FR-002).
- `NulOnly`: accepts when the match starts the region, or when the preceding byte is `0x00` (FR-003, `main`'s rule).
- `accepts(self, region: &[u8], pos: usize) -> bool`.

zlib's leading-space form applies `Anchored`'s byte test to `region[pos - 2]`, the byte before the space (R4). Its no-space form uses the NUL rule.

## `CensusHit` (new, log-only output)

One per pattern occurrence found without any boundary test (R5).

| field | meaning |
|---|---|
| `library` | slug |
| `version` | parsed version |
| `preceding` | `start` \| `nul` \| `non_identifier` \| `identifier` |
| `accepted_main` | `main`'s rule would accept it |
| `accepted_new` | FR-002/FR-003 would accept it |
| `context_before`, `context_after` | 24 bytes each side, hex |

These are emitted as `tracing::debug!(target: "waybill::version_census", path, …)` from `prepare_held`, only when that target is enabled. They never reach the document.

## `EmbeddedVersionMatch` (existing)

Unchanged. A match accepted by the relaxed rule is indistinguishable from a NUL-preceded one (FR-007).
