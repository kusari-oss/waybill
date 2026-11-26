#!/usr/bin/env bash
# Spec 1660 FR-001: do concurrent processes get the same temporary-file name?
# Starts N processes at once, B batches per N, for each mode, and counts
# duplicate names within each batch. Appends to collide-results.tsv.
# Usage: collide.sh RUN_NUMBER
set -eu
cd "$(dirname "$0")"
run=${1:-1}
cargo build -q --release --manifest-path collide/Cargo.toml
bin=collide/target/release/collide
[ -f collide-results.tsv ] || printf 'run\tmode\tN\tbatches\tnames\tduplicates\n' > collide-results.tsv
tmp=$(mktemp -d)
for mode in real fixed-seed; do
  arg=""; [ "$mode" = fixed-seed ] && arg="--fixed-seed"
  for N in 18 36 72; do
    dups=0; names=0
    for b in $(seq 1 50); do
      out="$tmp/$mode-$N-$b"
      : > "$out"
      for i in $(seq 1 "$N"); do "$bin" $arg >> "$out" & done
      wait
      names=$((names + $(wc -l < "$out")))
      dups=$((dups + $(sort "$out" | uniq -d | wc -l)))
    done
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$run" "$mode" "$N" 50 "$names" "$dups" >> collide-results.tsv
  done
done
rm -rf "$tmp"
