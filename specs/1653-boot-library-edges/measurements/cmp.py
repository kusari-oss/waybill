import json, sys
a = json.load(open(sys.argv[1])); a = a.get("libraries", a); b = json.load(open(sys.argv[2]))
dep = lambda d: {k: v["depends"] for k, v in d.items()}
A, B = dep(a), dep(b)
diff = [(k, A[k], B[k]) for k in sorted(set(A) & set(B)) if A[k] != B[k]]
print(sys.argv[1], "libs", len(A), len(B), "only-bindist", sorted(set(A) - set(B)), "only-nixpkgs", sorted(set(B) - set(A)),
      "edges", sum(map(len, A.values())), sum(map(len, B.values())), "differ", len(diff))
for d in diff: print("  ", d)
