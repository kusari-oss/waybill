#!/usr/bin/env bash
# Spec 1530 research probe: how does `git` behave on the repositories the
# submodule reader will run it against?
#
#   R1  Which candidate commands execute a program named in the repository's
#       own .git/config (core.fsmonitor, core.hooksPath, core.pager), and
#       does a command-line `-c` override stop it?
#   R2  What does git do with a repository it considers foreign-owned
#       (safe.directory)? Simulated with GIT_TEST_ASSUME_DIFFERENT_OWNER=1.
#   R3  On a fresh clone with the submodule NOT initialised, is the gitlink
#       readable, and does .git/modules/<name>/HEAD exist?
#
# Everything is built under a throwaway directory from local `file://`
# repositories. Nothing is fetched.
#
# Usage: probe/git-safety.sh <scratch-dir>
set -euo pipefail
S=${1:?scratch dir}
rm -rf "$S"; mkdir -p "$S"; cd "$S"
export GIT_CONFIG_NOSYSTEM=1 HOME="$S/home"; mkdir -p "$HOME"
git config --global user.email probe@example.invalid
git config --global user.name probe
git config --global init.defaultBranch main
git config --global protocol.file.allow always

git version

# --- fixture: a dependency repo and a super-project pinning it ------------
git init -q dep && git -C dep commit -q --allow-empty -m dep
DEP_SHA=$(git -C dep rev-parse HEAD)
git init -q app
git -C app submodule -q add "$S/dep" third_party/waybill-fixture-dep
git -C app commit -q -m add
git clone -q "$S/app" fresh            # submodule left uninitialised
echo "R3 dep commit:                 $DEP_SHA"
echo "R3 fresh ls-tree gitlink:      $(git -C fresh ls-tree -r HEAD | awk '$1=="160000"{print $1, $2, $3, $4}')"
echo "R3 fresh .git/modules exists:  $([ -e fresh/.git/modules ] && echo yes || echo no)"
echo "R3 fresh submodule dir empty:  $([ -z "$(ls -A fresh/third_party/waybill-fixture-dep)" ] && echo yes || echo no)"

# --- R1: hostile repo-local config ----------------------------------------
MARK="$S/marker"
cat > "$S/payload.sh" <<EOF
#!/bin/sh
echo "\$0 \$*" >> "$MARK"
exit 0
EOF
chmod +x "$S/payload.sh"
git -C fresh config core.fsmonitor "$S/payload.sh"
git -C fresh config core.pager "$S/payload.sh"
mkdir -p "$S/hooks"; for h in post-checkout post-index-change reference-transaction; do cp "$S/payload.sh" "$S/hooks/$h"; done
git -C fresh config core.hooksPath "$S/hooks"

run() {  # label, command...
  rm -f "$MARK"
  local label=$1; shift
  if "$@" >/dev/null 2>"$S/err"; then rc=0; else rc=$?; fi
  printf 'R1 %-60s exit=%s payload_ran=%s\n' "$label" "$rc" "$([ -s "$MARK" ] && echo YES || echo no)"
}
run "rev-parse --show-toplevel"            git -C fresh rev-parse --show-toplevel
run "ls-tree -r -z HEAD"                   git -C fresh ls-tree -r -z HEAD
run "ls-files --stage -z"                  git -C fresh ls-files --stage -z
run "status --porcelain"                   git -C fresh status --porcelain
run "-c core.fsmonitor=false ls-files -s"  git -C fresh -c core.fsmonitor=false ls-files --stage -z
run "-c core.fsmonitor=false status"       git -C fresh -c core.fsmonitor=false -c core.hooksPath=/dev/null status --porcelain
run "--no-pager log -1"                    git -C fresh --no-pager log -1
# The git commands waybill already runs in scanned repositories:
run "existing: describe --tags --exact-match HEAD" git -C fresh describe --tags --exact-match HEAD
run "existing: describe --tags --always"   git -C fresh describe --tags --always
run "existing: rev-parse HEAD"             git -C fresh rev-parse HEAD
run "existing: remote get-url origin"      git -C fresh remote get-url origin
run "existing: config --get remote.origin.url" git -C fresh config --get remote.origin.url
run "control: describe --dirty"            git -C fresh describe --tags --always --dirty
run "log -1 (pager config, stdout not a tty)" git -C fresh log -1

# --- R2: foreign-owned repository -----------------------------------------
if out=$(GIT_TEST_ASSUME_DIFFERENT_OWNER=1 git -C fresh ls-tree -r HEAD 2>&1); then rc=0; else rc=$?; fi
echo "R2 assume-different-owner ls-tree: exit=$rc first-line=$(printf '%s' "$out" | head -n 1)"
if out=$(GIT_TEST_ASSUME_DIFFERENT_OWNER=1 git -C fresh rev-parse --show-toplevel 2>&1); then rc=0; else rc=$?; fi
echo "R2 assume-different-owner rev-parse: exit=$rc first-line=$(printf '%s' "$out" | head -n 1)"
