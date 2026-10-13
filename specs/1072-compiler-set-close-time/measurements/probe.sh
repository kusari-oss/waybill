#!/usr/bin/env bash
# Milestone 1072 probes (#1144). Answers one question the spec needed:
#
#   Can a single path appear in more than one compiler invocation's
#   write_set within one trace? If so, the single close-time hash of
#   that path would be attributed to invocations that did not produce
#   it -- the same class of false statement #1144 is about.
#
# Probe 1 reuses milestone 1070's committed strace (real kernel,
# Colima aarch64 6.8.0-136) rather than re-tracing; probe 2 runs
# locally on any host, because inode identity needs no eBPF.
#
# Findings: findings.md.
set -euo pipefail
REPO="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
M1070="$REPO/specs/1070-trace-relative-source-reads/measurements"
FX="$REPO/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge"

# --- Probe 1: paths opened for writing by more than one pid ----------
# strace -f prefixes each line with the pid, so "two writers of one
# path" is a group-by. A positive control follows, because a zero
# result here is indistinguishable from a broken extraction.
writers() {
  grep -E 'openat2?\(.*O_(WRONLY|RDWR)' "$1" \
    | sed -E 's/^([0-9]+) +openat2?\((AT_FDCWD|[0-9]+)<?[^,]*>?, "([^"]+)".*/\1\t\3/' \
    | sort -u \
    | awk -F'\t' '{n[$2]=n[$2]" "$1; c[$2]++}
                  END {for (p in c) if (c[p]>1) printf "%d pids:%s  %s\n", c[p], n[p], p}' \
    | sort -rn
}

echo "## Probe 1a: parse coverage (must be equal, or the result is meaningless)"
raw=$(grep -cE 'openat2?\(.*O_(WRONLY|RDWR)' "$M1070/fixture.strace")
parsed=$(grep -E 'openat2?\(.*O_(WRONLY|RDWR)' "$M1070/fixture.strace" \
  | sed -E 's/^([0-9]+) +openat2?\((AT_FDCWD|[0-9]+)<?[^,]*>?, "([^"]+)".*/\1\t\3/' \
  | grep -cE '^[0-9]+	')
echo "raw=$raw parsed=$parsed"

echo "## Probe 1b: real multi-writer paths"
writers "$M1070/fixture.strace"

echo "## Probe 1c: POSITIVE CONTROL -- inject a synthetic second writer"
# If this does not surface the injected path, probe 1b's silence proves
# nothing. Re-emits one real artifact open under a fake pid.
{ grep -E 'openat2?\(.*O_(WRONLY|RDWR)' "$M1070/fixture.strace"
  grep -E 'openat2?\(.*O_(WRONLY|RDWR)' "$M1070/fixture.strace" \
    | grep -m1 -E 'full\.rmeta|\.rlib|deps/' | sed -E 's/^[0-9]+/999999/'
} > /tmp/m1072-control.strace
writers /tmp/m1072-control.strace

# --- Probe 2: hard link vs copy for the user-facing artifact ---------
# m1070 observed `linkat deps/safe_only-<hash> -> release/safe-only` on
# Linux. Inode identity settles link-vs-copy without a kernel trace.
echo "## Probe 2: user-facing artifact vs its deps/ original"
rm -rf /tmp/m1072-fx
cargo build --release --manifest-path "$FX/Cargo.toml" --target-dir /tmp/m1072-fx -q
cd /tmp/m1072-fx/release
for f in safe-only deps/safe_only-*; do
  case "$f" in *.d) continue;; esac
  [ -f "$f" ] || continue
  if stat -f '' . >/dev/null 2>&1; then
    stat -f "ino=%i links=%l  %N" "$f"        # BSD / macOS
  else
    stat -c "ino=%i links=%h  %n" "$f"        # GNU / Linux
  fi
done
echo "content:"
shasum -a 256 safe-only deps/safe_only-* 2>/dev/null \
  | grep -v '\.d$' | awk '{print substr($1,1,16), $2}'

echo "## Probe 3: did rustc's temporaries survive?"
ls -d deps/rmeta* deps/*.temp-archive 2>/dev/null || echo "none (expected: #1144's case)"

# --- Probe 4: paths READ by more than one compiler invocation -------
# The mirror of probe 1. Probe 1 asked whether two invocations can WRITE
# one path (no). This asks whether two can READ one path (yes, routinely),
# which is what makes a distinct-path count differ from an entry count and
# why the budget must not be charged per entry.
#
# strace -f lists threads as separate pids, so a reader pid must be walked
# back up the clone() edges to the rustc that owns it -- otherwise one
# rustc's worker threads look like several invocations.
echo "## Probe 4a: rustc invocations in the trace"
rustc_pids=$(grep -E '^[0-9]+ +execve\("[^"]*/rustc"' "$M1070/fixture.strace" \
  | sed -E 's/^([0-9]+).*/\1/' | sort -un)
echo "rustc pids: $(echo "$rustc_pids" | tr '\n' ' ')"

# parent[child]=parent, from every clone/vfork return
parents() {
  grep -E '^[0-9]+ +(clone3?|vfork)\(.*\) += [0-9]+$' "$M1070/fixture.strace" \
    | sed -E 's/^([0-9]+) +(clone3?|vfork)\(.*= ([0-9]+)$/\3 \1/' | sort -u
}

echo "## Probe 4b: owning invocation of each reader of one shared .rlib"
# libsafe is depended on by BOTH fixture binaries, so its .rlib is the
# natural candidate for a path read by more than one invocation.
target=$(grep -oE '[^"]*liblibsafe-[0-9a-f]+\.rlib' "$M1070/fixture.strace" | head -1)
echo "path: $target"
readers=$(grep -E 'openat2?\(' "$M1070/fixture.strace" \
  | grep -vE 'O_(WRONLY|RDWR)' | grep -F "\"$target\"" \
  | sed -E 's/^([0-9]+).*/\1/' | sort -un)
parents > /tmp/m1072-parents.txt
for r in $readers; do
  cur=$r; hops=0
  # walk up until we hit a rustc pid or run out of parents
  while [ $hops -lt 20 ]; do
    if echo "$rustc_pids" | grep -qx "$cur"; then echo "reader $r -> rustc $cur"; break; fi
    next=$(awk -v c="$cur" '$1==c {print $2; exit}' /tmp/m1072-parents.txt)
    [ -n "$next" ] || { echo "reader $r -> (not under rustc; pid $cur)"; break; }
    cur=$next; hops=$((hops+1))
  done
done

echo "## Probe 4c: how many paths have more than one reader at all"
grep -E 'openat2?\(' "$M1070/fixture.strace" | grep -vE 'O_(WRONLY|RDWR)' \
  | sed -E 's/^([0-9]+) +openat2?\((AT_FDCWD|[0-9]+)<?[^,]*>?, "([^"]+)".*/\1\t\3/' \
  | grep -E '^[0-9]+	' | sort -u \
  | awk -F'\t' '{c[$2]++} END {n=0; for (p in c) if (c[p]>1) n++; print n " paths"}'
