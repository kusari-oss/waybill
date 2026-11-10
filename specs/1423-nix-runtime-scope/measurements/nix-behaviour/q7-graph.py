"""hlint's runtime closure as a reference graph (from `nix path-info --json -r`, narinfo
references only; no NAR is read). Reports what reaches what once -doc outputs and/or the
GHC compiler are removed, and the direct referrers of every -doc output and of ghc.
usage: python3 -I -B q7-graph.py <pathinfo.json> <root-store-path>"""
import json, os, re, sys
from collections import Counter
info = json.load(open(sys.argv[1])); root = sys.argv[2]
name = lambda p: re.sub(r"^[a-z0-9]{32}-", "", os.path.basename(p))
refs = {p: [r for r in v["references"] if r != p] for p, v in info.items()}
def reach(skip):
    seen, st = set(), [root]
    while st:
        p = st.pop()
        if p in seen or skip(p): continue
        seen.add(p); st += refs.get(p, [])
    return seen
is_doc = lambda p: name(p).endswith("-doc")
is_ghc = lambda p: re.fullmatch(r"ghc-\d+(\.\d+)*", name(p)) is not None
full = reach(lambda p: False)
nodoc = reach(is_doc)
noghc = reach(is_ghc)
neither = reach(lambda p: is_doc(p) or is_ghc(p))
print(f"runtime closure: {len(full)}  (-doc outputs: {sum(map(is_doc, full))})")
print(f"without -doc outputs: {len(nodoc)}; without ghc: {len(noghc)}; without both: {len(neither)}")
print("\nreachable only through a -doc output (excl. the -doc outputs themselves):",
      sorted(name(p) for p in full - nodoc if not is_doc(p)))
print("\nreachable only through ghc:", sorted(name(p) for p in full - noghc))
print("\nremaining when both are removed:", sorted(name(p) for p in neither))
rev = {}
for p, rs in refs.items():
    for r in rs: rev.setdefault(r, []).append(p)
docs = sorted(p for p in full if is_doc(p))
kinds = Counter()
for d in docs:
    for q in rev.get(d, []): kinds["doc referenced by -doc" if is_doc(q) else "doc referenced by non-doc"] += 1
print("\nreferrers of -doc outputs:", dict(kinds))
nondoc_referrers = sorted({name(q) for d in docs for q in rev.get(d, []) if not is_doc(q)})
print("non-doc paths that reference some -doc output:", len(nondoc_referrers), nondoc_referrers)
print("\ndirect referrers of ghc:", sorted({name(q) for p in full if is_ghc(p) for q in rev.get(p, [])}))
print("ghc's own references:", sorted(name(r) for p in full if is_ghc(p) for r in refs[p]))

# Shortest reference chain from the root to selected targets, avoiding -doc outputs and ghc.
from collections import deque
def chain(target_name, skip):
    prev, q = {root: None}, deque([root])
    while q:
        p = q.popleft()
        if name(p) == target_name:
            out = []
            while p: out.append(name(p)); p = prev[p]
            return " -> ".join(reversed(out))
        for r in refs.get(p, []):
            if r not in prev and not skip(r): prev[r] = p; q.append(r)
    return None
print()
for t in ["hspec-2.11.17", "tasty-1.5.4", "test-framework-0.8.2.3", "doctest-0.24.3", "QuickCheck-2.15.0.1",
          "ChasingBottoms-1.3.1.17", "hscolour-1.25", "compiler-rt-libc-21.1.8", "libcxx-21.1.6+apple-sdk-26.4"]:
    print(f"chain avoiding -doc and ghc to {t}: {chain(t, lambda p: is_doc(p) or is_ghc(p))}")
for t in ["cctools-1010.6", "clang-21.1.8", "llvm-21.1.8", "bash-interactive-5.3p9", "aeson-2.2.4.1-doc"]:
    print(f"chain to {t}: {chain(t, lambda p: False)}")
