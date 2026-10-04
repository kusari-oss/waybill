#!/usr/bin/env bash
# Walker-audit allow-list check (milestones 115 / 117 / 664; issue #378).
#
# Every `fn walk_*` under waybill-cli/src/scan_fs/ must be listed in
# waybill-cli/src/scan_fs/walk.audit-allowlist.txt, so new filesystem walkers
# go through review (see that file's rationale doc). Run by CI
# (.github/workflows/ci.yml) and by scripts/pre-pr.sh: one script, so the
# local gate and CI cannot drift. Before this was shared, the check lived
# only in CI and a local-green PR could still fail it (#1111).
#
# Run from the repository root. Exits non-zero on mismatch.

set -u
ALLOWLIST="waybill-cli/src/scan_fs/walk.audit-allowlist.txt"
# Milestone 117 (#347): strip the absolute line-number column from
# both the live grep output AND the committed allow-list before
# diffing, so unrelated insertions above an allow-listed walker
# don't trigger false-positive failures from pure position drift.
# Applied symmetrically so a hand-edited OLD-shape line in the
# allow-list (with the `:NNN:` middle column) still compares
# correctly against the new line-stripped live output (forgiveness
# toward drift; the committed file SHOULD always be in NEW shape
# per milestone 117 FR-002). BRE regex works identically on
# GNU sed (Linux) and BSD sed (macOS).
STRIP_LINE_NUMBERS='s/^\([^:]*\):[0-9]*:/\1:/'
FAIL_HEADLINE="[FAIL] Walker-audit allow-list mismatch — see waybill-cli/src/scan_fs/walk.rs's module-level comment for the exception policy."
FAIL_POINTER_NEW="If your PR intentionally adds a new walker exception, see CONTRIBUTING.md § Walker-audit CI gate."
FAIL_POINTER_BAD="If your PR did NOT intend to add a walker, remove the new fn walk_* function and use scan_fs::walk::safe_walk instead."
# Milestone 664 US3 T065 (FR-008 diagnostic): every new
# scan-tree walker MUST route through the shared
# `walk_registry` — direct `safe_walk` callers or new
# `fn walk_*` functions are only allowed in the 4
# categories documented in the T064 rationale doc.
FAIL_POINTER_M664="New safe_walk caller detected outside shared registry. See specs/664-single-pass-walker/spec.md FR-008 for the migration policy. Per-entry classification lives at waybill-cli/src/scan_fs/walk.audit-allowlist.rationale.md — new entries MUST fall into (A) FR-005 permanent escape hatch, (B) deferred-reader retention, (C) non-scan-tree walker, or (D) shared-walker infrastructure. If your new walker is a scan-tree package_db reader, migrate to walk_registry per specs/664-single-pass-walker/quickstart.md."

# Precheck: missing or empty allow-list → fail closed per
# FR-010 strict-enforcement bootstrap. A future PR that
# accidentally or maliciously deletes the file gets the
# same red CI as an unauthorized walker addition.
if [ ! -f "$ALLOWLIST" ]; then
  echo "$FAIL_HEADLINE" >&2
  echo "" >&2
  echo "ERROR: $ALLOWLIST is missing." >&2
  echo "" >&2
  echo "This file is the bootstrap baseline for the walker-audit CI gate (feature 115)." >&2
  echo "If you intentionally removed it, restore from the previous commit:" >&2
  echo "    git show HEAD~1:$ALLOWLIST > $ALLOWLIST" >&2
  echo "" >&2
  echo "See CONTRIBUTING.md § Walker-audit CI gate for the file's purpose." >&2
  exit 1
fi
EXPECTED=$(grep -v '^#' "$ALLOWLIST" | grep -v '^$' | sed "$STRIP_LINE_NUMBERS" | LC_ALL=C sort -u)
if [ -z "$EXPECTED" ]; then
  echo "$FAIL_HEADLINE" >&2
  echo "" >&2
  echo "ERROR: $ALLOWLIST is empty (zero non-blank, non-comment lines)." >&2
  echo "" >&2
  echo "This file is the bootstrap baseline for the walker-audit CI gate (feature 115)." >&2
  echo "Empty is not a valid state per FR-010 strict-enforcement bootstrap." >&2
  echo "" >&2
  echo "See CONTRIBUTING.md § Walker-audit CI gate for the file's purpose." >&2
  exit 1
fi

# Live audit pattern. LC_ALL=C pins lex order across runners.
# `--include='*.rs'` scopes to Rust source so the audit doesn't
# match its own allow-list file (`walk.audit-allowlist.txt`
# lives under scan_fs/ for adjacency per SC-005, and every
# entry inside it contains the substring `fn walk_`).
#
# Issue #378: a function whose name starts with `walk_` but
# does NOT walk the filesystem (in-memory iterators, test
# functions, etc.) MAY opt out of the audit by placing a
# `// walker-audit:` sigil comment on the line immediately
# above the function signature. The sigil's text after the
# colon is free-form developer audit-trail. The pre-filter
# below drops any match whose immediate predecessor line
# carries the sigil; surviving matches still go through the
# allow-list diff. Single sigil prefix per FR (issue body
# acceptance criteria); the prefix matches whole-token
# so `// walker-audit-elsewhere:` does NOT opt out.
LIVE=$(LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' waybill-cli/src/scan_fs/ | while IFS=: read -r path line content; do
  prev=$((line - 1))
  if [ "$prev" -ge 1 ]; then
    prev_line=$(LC_ALL=C sed -n "${prev}p" "$path" 2>/dev/null)
    case "$prev_line" in
      *"// walker-audit:"*) continue;;
    esac
  fi
  printf '%s:%s:%s\n' "$path" "$line" "$content"
done | sed "$STRIP_LINE_NUMBERS" | LC_ALL=C sort -u)

# GNU date gives nanoseconds; BSD date (macOS) prints a literal `N`, so
# fall back to 0 there rather than abort on non-numeric arithmetic.
now_ns() { local t; t=$(date +%s%N 2>/dev/null); case "$t" in *[!0-9]*|'') echo 0;; *) echo "$t";; esac; }
START_NS=$(now_ns)
if DIFF_OUT=$(diff -u <(printf '%s\n' "$EXPECTED") <(printf '%s\n' "$LIVE")); then
  END_NS=$(now_ns)
  ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
  ENTRY_COUNT=$(printf '%s\n' "$EXPECTED" | wc -l | tr -d ' ')
  echo "Walker-audit allow-list check: OK (${ENTRY_COUNT} entries; ${ELAPSED_MS} ms)"
else
  echo "$FAIL_HEADLINE" >&2
  echo "" >&2
  echo "--- waybill-cli/src/scan_fs/walk.audit-allowlist.txt (expected)" >&2
  echo "+++ live: grep -rEn --include='*.rs' 'fn walk[_(]' waybill-cli/src/scan_fs/ | sed -e 's/^\([^:]*\):[0-9]*:/\1:/' | sort -u (actual)" >&2
  printf '%s\n' "$DIFF_OUT" | tail -n +3 >&2
  echo "" >&2
  echo "$FAIL_POINTER_NEW" >&2
  echo "$FAIL_POINTER_BAD" >&2
  echo "" >&2
  echo "$FAIL_POINTER_M664" >&2
  exit 1
fi
