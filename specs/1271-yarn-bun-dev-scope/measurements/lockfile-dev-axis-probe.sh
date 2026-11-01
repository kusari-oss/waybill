#!/bin/sh
# Which npm-ecosystem lockfiles preserve the development/runtime axis, and
# where in the file does it live?
#
# Issue #1297 states the pnpm v9 answer as inference and asks that whoever
# fixes it confirm the shape against a real project first. This probe
# answers the same question for the two readers still at zero
# LifecycleScope::Development sites: yarn (v1 and Berry) and bun.
#
# Three project shapes, because they give different answers:
#   single          one package.json, one runtime dep, one dev dep
#   monorepo        a private root plus one workspace member owning the dev dep
#   monorepo_objform the same, with the root declaring workspaces in the
#                   object form {"packages": [...], "nohoist": [...]} that
#                   yarn v1 also accepts -- a glob-expanding implementation
#                   has to handle both forms
#
# One runtime and one dev dependency, both tiny and dependency-free, so each
# emitted lockfile is small enough to read whole. Real coordinates are used
# deliberately -- the probe measures a package manager's output format, and a
# synthetic name cannot be resolved. Fixtures committed to the repository use
# waybill-fixture-* names instead.
#
# --ignore-scripts throughout: the probe reads text and never needs a
# package's install hooks to run.
#
# Usage: lockfile-dev-axis-probe.sh [output-dir]
set -eu
OUT="${1:-$(mktemp -d)}"

for shape in single monorepo monorepo_objform; do
  for pm in yarn1 berry bun; do
    d="$OUT/$shape/$pm"
    mkdir -p "$d"
    if [ "$shape" = single ]; then
      cat > "$d/package.json" <<'JSON'
{
  "name": "shape-probe",
  "version": "1.0.0",
  "dependencies": { "ms": "2.1.3" },
  "devDependencies": { "tiny-invariant": "1.3.3" }
}
JSON
    else
      mkdir -p "$d/packages/member"
      if [ "$shape" = monorepo_objform ]; then
        cat > "$d/package.json" <<'JSON'
{
  "name": "mono-root",
  "version": "1.0.0",
  "private": true,
  "workspaces": { "packages": ["packages/*"], "nohoist": ["**/ms"] },
  "dependencies": { "ms": "2.1.3" }
}
JSON
      else
      cat > "$d/package.json" <<'JSON'
{
  "name": "mono-root",
  "version": "1.0.0",
  "private": true,
  "workspaces": ["packages/*"],
  "dependencies": { "ms": "2.1.3" }
}
JSON
      fi
      cat > "$d/packages/member/package.json" <<'JSON'
{
  "name": "member",
  "version": "1.0.0",
  "dependencies": { "ms": "2.1.3" },
  "devDependencies": { "tiny-invariant": "1.3.3" }
}
JSON
    fi
  done

  ( cd "$OUT/$shape/yarn1" && yarn install --ignore-scripts --non-interactive --no-progress >/dev/null 2>&1 ) \
    || echo "yarn1/$shape: install failed"
  ( cd "$OUT/$shape/berry" && yarn set version berry >/dev/null 2>&1 && yarn install >/dev/null 2>&1 ) \
    || echo "berry/$shape: install failed"
  ( cd "$OUT/$shape/bun" && bun install --ignore-scripts >/dev/null 2>&1 ) \
    || echo "bun/$shape: install failed"
done

for shape in single monorepo monorepo_objform; do
  for pm in yarn1 berry bun; do
    d="$OUT/$shape/$pm"
    lock=$(ls "$d"/yarn.lock "$d"/bun.lock 2>/dev/null | head -1 || true)
    printf '\n===== %s / %s (%s) =====\n' "$shape" "$pm" "${lock:+$(basename "$lock")}"
    [ -n "${lock:-}" ] || { echo "  no lockfile produced"; continue; }
    if grep -n 'devDependencies' "$lock" >/dev/null 2>&1; then
      echo "  dev axis PRESENT:"
      grep -n 'devDependencies' "$lock" | sed 's/^/    /'
    else
      echo "  dev axis ABSENT (count of the substring 'dev' in the file: $(grep -c dev "$lock" || true))"
    fi
    case "$shape" in monorepo*)
      if grep -n 'member' "$lock" >/dev/null 2>&1; then
        echo "  workspace member named in the lockfile:"
        grep -n 'member' "$lock" | sed 's/^/    /'
      else
        echo "  workspace member NOT named in the lockfile"
      fi
      if grep -n 'tiny-invariant' "$lock" >/dev/null 2>&1; then
        echo "  the member's dev dependency IS resolved into the lockfile (present but unclassified)"
      else
        echo "  the member's dev dependency is absent from the lockfile"
      fi
      ;;
    esac
    echo "  --- whole file ---"
    sed 's/^/    /' "$lock"
  done
done
