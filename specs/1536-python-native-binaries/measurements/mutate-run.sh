#!/bin/bash
# Spec 1536 T017: apply each mutation, run the integration suite, record the
# failing tests, restore the file with cp, and check the tree is clean.
set -u
REPO=/Users/mlieberman/Projects/mikebom
D=/private/tmp/claude-501/-Users-mlieberman-Projects-mikebom/e93b0155-273d-4de5-8b5f-de35ec790c78/scratchpad/mut1536
cd "$REPO"
: > "$D/results.tsv"
for m in M1 M2 M3 M4 M5 M6 M7 M8 M9 M10 M11 M12 M13; do
  path=$(grep -A1 "\"$m\":" "$D/mutate.py" | head -1 | sed -E 's/.*\("([^"]+)".*/\1/')
  cp "$REPO/$path" "$D/backup"
  if ! python3 -I -B "$D/mutate.py" "$REPO" "$m" > /dev/null; then
    echo -e "$m\tANCHOR-MISS\t$path" >> "$D/results.tsv"; cp "$D/backup" "$REPO/$path"; continue
  fi
  cargo test -p waybill --test python_owned_binaries > "$D/$m.log" 2>&1
  status=$?
  failed=$(grep -E '^test .* FAILED$' "$D/$m.log" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sort | tr '\n' ' ')
  compile=$(grep -c '^error' "$D/$m.log")
  cp "$D/backup" "$REPO/$path"
  clean=$(git -C "$REPO" diff --quiet -- "$path" && echo clean || echo DIRTY)
  echo -e "$m\texit=$status\tcompile_errors=$compile\t$clean\t$path\t$failed" >> "$D/results.tsv"
done
echo DONE >> "$D/results.tsv"
