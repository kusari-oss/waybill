#!/usr/bin/env bash
# Realise each wrapper's default output from cache only (--max-jobs 0: substitute, never build).
set -uo pipefail
R="${RECALL_CORPUS_DIR:?set RECALL_CORPUS_DIR to the recall-corpus work directory (wrappers/, out/, realise/)}"
mkdir -p "$R/realise"
for p in "$@"; do
  s=$(date +%s)
  nix build --max-jobs 0 --no-link --print-out-paths "$R/wrappers/$p#default" >"$R/realise/$p.out" 2>"$R/realise/$p.err"
  rc=$?
  echo "$p exit=$rc $(( $(date +%s)-s ))s out=$(tr '\n' ' ' < "$R/realise/$p.out")" | tee -a "$R/realise/exits.txt"
done
