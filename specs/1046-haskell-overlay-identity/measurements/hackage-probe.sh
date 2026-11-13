#!/usr/bin/env bash
# Milestone 1046 — how Hackage answers "is <name> <version> published?".
# Read-only, unauthenticated; run with no arguments. Prints one line per query.
set -uo pipefail
H=https://hackage.haskell.org
q() { # label url [accept]
  local acc="${3:-}"; local t0 t1 out code
  t0=$(python3 -I -B -c 'import time;print(time.time())')
  if [ -n "$acc" ]; then out=$(curl -s --max-time 20 -H "Accept: $acc" -o /tmp/hp.$$ -w '%{http_code}' "$2"); else out=$(curl -s --max-time 20 -o /tmp/hp.$$ -w '%{http_code}' "$2"); fi
  t1=$(python3 -I -B -c 'import time;print(time.time())')
  code=$out; ms=$(python3 -I -B -c "print(int(($t1-$t0)*1000))")
  # Print only what the decision reads: a JSON version list's head, or a
  # .cabal file's `name:` and `version:` lines (its other fields name people).
  local body
  if head -c1 /tmp/hp.$$ | grep -q '{'; then body=$(head -c 120 /tmp/hp.$$)
  elif [ "$code" = 200 ]; then body=$(grep -E '^(name|version):' /tmp/hp.$$ | tr -s ' ' | tr '\n' ' ')
  else body=$(head -c 80 /tmp/hp.$$ | tr '\n' ' '); fi
  printf '%s\tHTTP %s\t%sms\t%s\n' "$1" "$code" "$ms" "$body"
  rm -f /tmp/hp.$$
}
echo "## A. per-name version list: /package/<name>/preferred (JSON)"
q "slack-web preferred"            "$H/package/slack-web/preferred" application/json
q "tmp-postgres preferred"         "$H/package/tmp-postgres/preferred" application/json
q "HUnit preferred"                "$H/package/HUnit/preferred" application/json
q "hunit preferred (case)"         "$H/package/hunit/preferred" application/json
q "nonexistent preferred"          "$H/package/waybill-fixture-nonexistent/preferred" application/json
echo "## B. per-version existence: /package/<name>-<version>/<name>.cabal"
q "slack-web-2.3.0.0 cabal"        "$H/package/slack-web-2.3.0.0/slack-web.cabal"
q "slack-web-2.3.0.1 cabal (absent)" "$H/package/slack-web-2.3.0.1/slack-web.cabal"
q "tmp-postgres-1.35.0.0 cabal"    "$H/package/tmp-postgres-1.35.0.0/tmp-postgres.cabal"
q "HUnit-1.6.2.0 cabal"            "$H/package/HUnit-1.6.2.0/HUnit.cabal"
q "hunit-1.6.2.0 cabal (case)"     "$H/package/hunit-1.6.2.0/hunit.cabal"
echo "## C. latency, 5 runs each"
for i in 1 2 3 4 5; do q "run$i preferred"  "$H/package/slack-web/preferred" application/json; done
for i in 1 2 3 4 5; do q "run$i cabal"      "$H/package/slack-web-2.3.0.0/slack-web.cabal"; done
echo "## D. burst: 20 sequential per-version requests, status codes only"
for i in $(seq 1 20); do curl -s --max-time 20 -o /dev/null -w '%{http_code} ' "$H/package/slack-web-2.3.0.0/slack-web.cabal"; done; echo
echo "## E. a deprecated version: listed apart, still served"
curl -s --max-time 20 -H 'Accept: application/json' "$H/package/text/preferred" | python3 -I -B -c 'import json,sys; d=json.load(sys.stdin); dv=d.get("deprecated-version") or []; print("text deprecated-version count:", len(dv), "first:", dv[:1])' 
dv=$(curl -s --max-time 20 -H 'Accept: application/json' "$H/package/text/preferred" | python3 -I -B -c 'import json,sys; d=json.load(sys.stdin); dv=d.get("deprecated-version") or []; print(dv[0] if dv else "")')
[ -n "$dv" ] && q "text-$dv cabal (deprecated)" "$H/package/text-$dv/text.cabal"
