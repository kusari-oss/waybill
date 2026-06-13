# Data Model — Line-stable walker-audit allow-list

**Feature**: 117-line-stable-allowlist
**Date**: 2026-06-13

This feature changes the per-entry shape of ONE existing data artifact: the walker-audit allow-list. There are no new persisted entities — the file's location, name, encoding, and sort policy are all unchanged from milestone 115. Only the line-shape changes.

## Entity: Allow-list Entry

**File**: `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` (unchanged from milestone 115)
**Encoding**: UTF-8 (ASCII subset in practice) (unchanged)
**Line endings**: LF (unchanged)
**Final-newline policy**: file ends with a single LF (unchanged)
**Sort order**: `LC_ALL=C sort -u` (unchanged)
**Strict-enforcement bootstrap**: missing or empty file fails CI red (unchanged from milestone-115 FR-010)

### Line format

| Field | Milestone 115 (OLD) | This feature (NEW) | Change |
|---|---|---|---|
| 1 | `<relative-path>` | `<relative-path>` | unchanged |
| 2 | `<line-number>` | — | **removed** |
| 3 | `<matched-line-content>` | `<matched-line-content>` | unchanged |
| Separator | `:` between fields 1–2 and 2–3 | `:` between fields 1 and 3 | one fewer colon |

**Concrete examples** (same walker, both shapes):

```text
# Milestone 115 (OLD):
mikebom-cli/src/scan_fs/package_db/maven.rs:1249:fn walk_m2_jars(repo_cache: &MavenRepoCache) -> Vec<PathBuf> {

# This feature (NEW):
mikebom-cli/src/scan_fs/package_db/maven.rs:fn walk_m2_jars(repo_cache: &MavenRepoCache) -> Vec<PathBuf> {
```

The first field stays the relative path to the source file from the repo root. The third field (now second) stays the verbatim line content the grep matched. The middle `:1249:` is gone.

### Invariants

The file MUST satisfy these invariants at every commit; the CI gate enforces them implicitly via the diff:

1. **Coverage**: The set of non-blank-non-comment lines, after `LC_ALL=C sort -u` normalization, is byte-equal to the live `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed 's/^\([^:]*\):[0-9]*:/\1:/' | LC_ALL=C sort -u` output. Same coverage rule as milestone 115; only the sed step is new.
2. **No duplicates**: The `-u` step in the sort guarantees this. Byte-identical entries collapse. Per the spec edge-cases, byte-identical entries cannot arise from valid Rust source (two function declarations producing identical line text would be a compile error).
3. **Sort-stability**: The file content matches `LC_ALL=C sort -u file > file.tmp && diff file file.tmp` cleanly. Same rule as milestone 115.
4. **Final-newline**: file ends with exactly one trailing LF. Same rule as milestone 115.
5. **Non-empty**: file has at least one entry. The strict-enforcement bootstrap rule from milestone 115 FR-010 is preserved by this feature's FR-012.
6. **NEW shape**: every entry MUST be in `<file>:<content>` form. There is NO middle line-number column. (Forgiveness note: the CI step applies the sed-strip on read, so an accidentally OLD-form entry compares correctly — but the committed file SHOULD always be NEW-form per FR-002. The forgiveness exists to absorb drift, not to invite it.)

### Lifecycle

The file is bootstrapped to the NEW shape at this PR's ship time by running:

```bash
LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \
  | sed 's/^\([^:]*\):[0-9]*:/\1:/' \
  | LC_ALL=C sort -u \
  > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt
```

After PR merge, the file is updated only by PRs that legitimately add or remove a walker exception per the milestone-115 workflow, now using the line-stripped regen command.

## Entity: Comparison Pipeline

The CI step's shell pipeline that turns the live audit output and the committed allow-list into byte-comparable streams.

### Milestone 115 (OLD) pipeline

```bash
EXPECTED=$(grep -v '^#' "$ALLOWLIST" | grep -v '^$' | LC_ALL=C sort -u)
LIVE=$(LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | LC_ALL=C sort -u)
diff -u <(printf '%s\n' "$EXPECTED") <(printf '%s\n' "$LIVE")
```

### This feature (NEW) pipeline

```bash
STRIP_LINE_NUMBERS='s/^\([^:]*\):[0-9]*:/\1:/'

EXPECTED=$(grep -v '^#' "$ALLOWLIST" | grep -v '^$' | sed "$STRIP_LINE_NUMBERS" | LC_ALL=C sort -u)
LIVE=$(LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed "$STRIP_LINE_NUMBERS" | LC_ALL=C sort -u)
diff -u <(printf '%s\n' "$EXPECTED") <(printf '%s\n' "$LIVE")
```

The ONLY change is the addition of `| sed "$STRIP_LINE_NUMBERS"` to both `$EXPECTED` and `$LIVE` extractions, BEFORE the `LC_ALL=C sort -u`. Everything else (the grep, the include scoping, the comment/blank-line stripping, the sort, the diff, the failure-message contract, the success line, the step's name, the step's slot in the job) is byte-identical to milestone 115.

### Invariants

1. **Symmetric application**: the strip step applies to BOTH `$EXPECTED` and `$LIVE`. The diff compares strip-vs-strip, not strip-vs-raw — otherwise the comparison is asymmetric and trivially fails.
2. **Strip BEFORE sort**: the strip removes the line-number column so that sorting the resulting lines lex-orders by file path then by content — line numbers don't perturb sort order in the NEW form. (In the OLD form, two walkers in the same file sorted by line number; in the NEW form they sort by content, which is the more semantically meaningful order.)
3. **POSIX-compatible regex**: `^\([^:]*\):[0-9]*:` uses BRE syntax that works identically on macOS BSD `sed` and Linux GNU `sed`. No `-E` / `-r` flag required.

### Performance

- ~28 lines of input to sed per pipeline step. Wall time: <1 ms per sed invocation, <10 ms total contribution.
- Total step wall time: unchanged from milestone 115's measured <500 ms; well under the SC-002 5-second budget.

## Validation rules summary

| Rule | Source | Where enforced |
|---|---|---|
| `<file>:<content>` shape | This feature FR-002 / Decision 1 | Allow-list bootstrap regen + the strip step applied on read |
| Symmetric strip | This feature Decision 2 | CI step pipeline (both `$EXPECTED` and `$LIVE`) |
| Single-PR cutover | This feature FR-012 + Decision 3 | The PR that introduces this feature ships both the YAML edit AND the regenerated allow-list |
| Failure message preserved | This feature FR-003 + Decision 4 | CI step's `echo` lines preserved; `+++ live:` header updated to match new pipeline |
| Strict-enforcement bootstrap inherited | This feature FR-012 | Missing/empty allow-list still fails CI red (unchanged from milestone-115 FR-010) |
| 5-state catch-rate | This feature FR-004 to FR-008 | New walker, removed walker, renamed walker, signature-changed walker, relocated walker all still fail CI red |
| False-positive elimination | This feature FR-009 | 50-line helper above existing walker passes CI green |

No state transitions (no lifecycle FSM); the file is read-once per CI invocation and compared.
