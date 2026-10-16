#!/usr/bin/env bash
# Run a gate and report ITS result, not a pipeline's.
#
# Usage:
#   scripts/gate.sh [--expect REGEX]... NAME -- COMMAND [ARGS...]
#
# Example:
#   scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
#
# Two failure modes this exists to remove, both of which CLAUDE.md warns about
# in prose and both of which keep happening anyway:
#
# 1. **A pipeline's status is not the gate's.** `./scripts/pre-pr.sh | tail -20`
#    reports tail's status: a successful filter over the output of a script that
#    died at clippy. The habitual fix is to run to a file and read `$?`, which is
#    four steps where the broken form is one. This is the one step.
#
#    So that even a piped invocation cannot lose the truth, the command's status
#    is appended INTO the log as its last lines. `tail` can cut the summary off
#    stdout; it cannot alter the log.
#
# 2. **Absence of failure is not success.** A build that never compiled contains
#    no failures either. `--expect` is the positive signal: a pattern that must
#    appear in the log, repeatable, and a gate that exits 0 without printing it
#    fails here. That is the difference between "nothing went wrong" and "the
#    thing happened".
#
# Logs land in $GATE_LOG_DIR (default target/gate/), which is gitignored.
set -uo pipefail

EXPECT=()
while [ $# -gt 0 ]; do
    case "$1" in
        --expect) EXPECT+=("${2:?--expect needs a regex}"); shift 2 ;;
        --) shift; break ;;
        -*) echo "gate.sh: unknown option $1" >&2; exit 2 ;;
        *) NAME="$1"; shift ;;
    esac
done
: "${NAME:?usage: gate.sh [--expect REGEX]... NAME -- COMMAND [ARGS...]}"
if [ $# -eq 0 ]; then
    echo "gate.sh: no command (did you forget --?)" >&2
    exit 2
fi

LOG_DIR="${GATE_LOG_DIR:-target/gate}"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/$NAME.log"

"$@" >"$LOG" 2>&1
STATUS=$?

# Decide once, from the command's own output, BEFORE writing anything into the
# log. Re-grepping afterwards reads the footer below, which quotes the very
# patterns it reports on, so a missing signal would be found in the line saying
# it was missing.
RESULT=pass
MISSING=()
FOUND=()
if [ "$STATUS" -ne 0 ]; then
    RESULT=fail
fi
for re in ${EXPECT+"${EXPECT[@]}"}; do
    if grep -Eq -- "$re" "$LOG"; then
        FOUND+=("$re")
    else
        MISSING+=("$re")
        RESULT=fail
    fi
done

{
    echo
    echo "GATE_NAME=$NAME"
    echo "GATE_EXIT=$STATUS"
    for re in ${MISSING+"${MISSING[@]}"}; do
        echo "GATE_MISSING_EXPECTED=$re"
    done
    echo "GATE_RESULT=$RESULT"
} >>"$LOG"

echo "gate: $NAME"
echo "  exit:   $STATUS"
echo "  log:    $LOG"
for re in ${FOUND+"${FOUND[@]}"}; do
    echo "  expect: found   /$re/"
done
for re in ${MISSING+"${MISSING[@]}"}; do
    echo "  expect: MISSING /$re/"
done
echo "  result: $RESULT"

if [ "$RESULT" = pass ]; then
    exit 0
fi
# A gate that exited 0 but printed nothing expected is the worse case: it
# looks green. Say so loudly, and distinguish it from a command that failed.
if [ "$STATUS" -eq 0 ]; then
    echo "gate.sh: $NAME exited 0 but the log is missing ${#MISSING[@]} expected signal(s); treating as failure" >&2
fi
exit 1
