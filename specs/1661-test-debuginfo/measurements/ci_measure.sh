#!/usr/bin/env bash
# Spec 1661 T007-T009: CI with debug info full / line-tables-only / none, 2 runs each.
# Branches: 1661-test-debuginfo (full), 1661-ci-lt, 1661-ci-none. Runs in parallel per round.
set -u
cd "$(git rev-parse --show-toplevel)"
M=specs/1661-test-debuginfo/measurements
R=kusari-oss/waybill
LOGS=$(mktemp -d)
latest_run() { gh run list -R $R -w ci.yml -b "$1" -L 1 --json databaseId -q '.[0].databaseId'; }
wait_run() { while [ "$(gh run view "$1" -R $R --json status -q .status)" != completed ]; do sleep 60; done; }
record() { # run setting
  local run=$1 setting=$2
  for lane in linux-x86_64 macos-latest windows-latest; do
    local job; job=$(gh run view "$run" -R $R --json jobs -q ".jobs[]|select(.name==\"Lint + test ($lane)\")|.databaseId")
    [ -z "$job" ] && continue
    local log=$LOGS/$run-$lane.log
    gh api --allow-escape-sequences repos/$R/actions/jobs/$job/logs 2>/dev/null | python3 -I -B -c "import re,sys; sys.stdout.write(re.sub(r'\x1b\[[0-9;]*m','',sys.stdin.read()))" > "$log"
    local build; build=$(grep -E 'Finished `test` profile' "$log" | sed -E 's/.* in //' | sort | tail -1)
    local step; step=$(gh api repos/$R/actions/jobs/$job -q '.steps[]|select(.name|startswith("Tests"))|"\(.started_at) \(.completed_at)"' | python3 -I -B -c "
import sys; from datetime import datetime
a,b=sys.stdin.read().split(); f=lambda s: datetime.fromisoformat(s.replace('Z','+00:00')); print(int((f(b)-f(a)).total_seconds()))")
    local bt; bt=$(grep -E ' (PASS|FAIL) \[.*backtrace_resolves_source' "$log" | head -1 | awk '{print $2}')
    local fl=no; grep -qE '^\s+at \S*backtrace_resolves_source\.rs:[0-9]+' "$log" && fl=yes
    [ "$bt" = PASS ] && fl=yes
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$run" "$lane" "$setting" "$build" "$step" "${bt:-n/a}" "$fl" >> $M/ci.tsv
    { echo "### $run $lane $setting"; grep -E ' FAIL \[' "$log" | sed -E 's/^[^ ]* +//; s/ +\[[^]]*\] +\([^)]*\) +/ /' | sort -u; } >> $M/ci-failures.md
  done
}
# Round 1: the PR's own run is the full baseline.
gh workflow run ci.yml -R $R --ref 1661-ci-lt; gh workflow run ci.yml -R $R --ref 1661-ci-none; sleep 30
for b in 1661-test-debuginfo 1661-ci-lt 1661-ci-none; do eval "r1_${b//-/_}=$(latest_run $b)"; done
for v in $r1_1661_test_debuginfo $r1_1661_ci_lt $r1_1661_ci_none; do wait_run $v; done
record $r1_1661_test_debuginfo full; record $r1_1661_ci_lt line-tables-only; record $r1_1661_ci_none none
echo "round 1 done: $r1_1661_test_debuginfo $r1_1661_ci_lt $r1_1661_ci_none"
# Round 2.
for b in 1661-test-debuginfo 1661-ci-lt 1661-ci-none; do gh workflow run ci.yml -R $R --ref $b; done; sleep 30
for b in 1661-test-debuginfo 1661-ci-lt 1661-ci-none; do eval "r2_${b//-/_}=$(latest_run $b)"; done
for v in $r2_1661_test_debuginfo $r2_1661_ci_lt $r2_1661_ci_none; do wait_run $v; done
record $r2_1661_test_debuginfo full; record $r2_1661_ci_lt line-tables-only; record $r2_1661_ci_none none
echo "round 2 done: $r2_1661_test_debuginfo $r2_1661_ci_lt $r2_1661_ci_none"
rm -rf "$LOGS"
