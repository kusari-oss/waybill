#!/usr/bin/env bash
# Linux lane: Tests step seconds and nextest summed test seconds for one CI run. Usage: ci_linux.sh RUN_ID PHASE
set -eu
cd "$(git rev-parse --show-toplevel)"
run=$1; phase=$2
step=$(gh api "repos/kusari-oss/waybill/actions/runs/$run/jobs?per_page=100" | python3 -I -B specs/1658-nextest-trial/measurements/ci_steps.py "$run" "$phase" | awk -F'\t' '$2=="Lint + test (linux-x86_64)"{print $4}')
job=$(gh api "repos/kusari-oss/waybill/actions/runs/$run/jobs?per_page=100" -q '.jobs[]|select(.name=="Lint + test (linux-x86_64)")|.id')
summed=$(gh run view --job "$job" --log -R kusari-oss/waybill | grep -E ' (PASS|FAIL) \[' | sed -E 's/.* (PASS|FAIL) \[ *([0-9.]+)s\].*/\2/' | awk '{s+=$1}END{printf "%.0f",s}')
printf '%s\tlinux\t%s\t%s\t%s\n' "$run" "$phase" "$step" "$summed"
