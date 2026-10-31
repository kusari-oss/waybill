#!/usr/bin/env bash
# #1185 — every place a kernel program can lose or fabricate a record is
# enumerated and classified. This stops a new one appearing unexamined.
#
# The bug that opened #1185 was one discarded `Result` on a bounded
# machine-wide map, at the only site in the open path that recorded nothing
# when it returned early. It was found by reading the file, and reading is
# not a method: the same read had passed over
# `COMPILER_INVOCATIONS`'s doc comment, which claimed a gating behaviour no
# program implemented.
#
# So the sites are committed to a list and diffed. A new one fails this
# check, and the fix is to classify it in
# specs/1270-stable-probe-tracepoints/measurements/drop-site-inventory.md
# and re-run with --update. Classifying is the point; the list is only what
# forces it.
#
# Entries are `<file>:<content>` with no line number, following m117: line
# numbers churn on every edit above them, so a line-keyed list produces
# diffs that carry no information and get regenerated without reading.
#
#   scripts/audit-drop-sites.sh            # check, exits 1 on drift
#   scripts/audit-drop-sites.sh --update   # re-record after classifying
set -euo pipefail

cd "$(dirname "$0")/.."
LIST=waybill-ebpf/src/programs/drop-sites.audit.txt
INVENTORY=specs/1270-stable-probe-tracepoints/measurements/drop-site-inventory.md

# Deliberately conservative. It matches ordinary control flow too, and that
# is the right trade: a pattern that over-matches produces rows to
# classify, where one that under-matches produces a silent gap -- which is
# the entire subject of #1185.
collect() {
    grep -nE "return|let _ = |unwrap_or|\.is_err\(\)" waybill-ebpf/src/programs/*.rs \
        | grep -vE ":[0-9]+:[[:space:]]*//" \
        | sed -E 's|^waybill-ebpf/src/programs/||; s|:[0-9]+:[[:space:]]*|:|' \
        | sed -E 's|[[:space:]]+$||' \
        | sort
}

if [[ "${1:-}" == "--update" ]]; then
    collect > "$LIST"
    echo "recorded $(wc -l < "$LIST" | tr -d ' ') sites in $LIST"
    echo "now classify any new row in $INVENTORY -- the list alone asserts nothing"
    exit 0
fi

if [[ ! -f "$LIST" ]]; then
    echo "FAIL: $LIST is missing; run with --update to create it" >&2
    exit 1
fi

CURRENT=$(mktemp)
trap 'rm -f "$CURRENT"' EXIT
collect > "$CURRENT"

if diff -u "$LIST" "$CURRENT" > /tmp/drop-sites.diff 2>&1; then
    echo ">>> drop-site audit passed: $(wc -l < "$LIST" | tr -d ' ') sites, all accounted for"
    exit 0
fi

echo "FAIL: the set of places a kernel program can lose or fabricate a record has changed" >&2
echo >&2
sed -n '4,200p' /tmp/drop-sites.diff >&2
echo >&2
echo "A line prefixed '+' is a new site. Classify it in" >&2
echo "  $INVENTORY" >&2
echo "as SCOPE, COUNTED, SILENT-DROP or SILENT-SUBSTITUTION, then re-run" >&2
echo "with --update. A SILENT-DROP or SILENT-SUBSTITUTION needs a counter" >&2
echo "before it is acceptable -- that is what #1185 was." >&2
echo >&2
echo "A line prefixed '-' is a site that went away, which is usually good," >&2
echo "but check it was removed rather than moved out of the grep's reach." >&2
exit 1
