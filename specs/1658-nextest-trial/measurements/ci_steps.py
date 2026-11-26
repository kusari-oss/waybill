"""Print Tests/Doctests step seconds per job for one CI run. Usage: python3 -I -B ci_steps.py RUN_ID PHASE  (reads `gh api` JSON on stdin)"""
import json, sys
from datetime import datetime
run, phase = sys.argv[1], sys.argv[2]
def t(x): return datetime.fromisoformat(x.replace("Z", "+00:00"))
for job in json.load(sys.stdin)["jobs"]:
    tests = doc = None
    for st in job.get("steps", []):
        if not st.get("started_at") or not st.get("completed_at"):
            continue
        secs = (t(st["completed_at"]) - t(st["started_at"])).total_seconds()
        if st["name"].startswith("Tests") and "ebpf" not in st["name"]:
            tests = (secs, st["conclusion"])
        elif st["name"].startswith("Doctests"):
            doc = (secs, st["conclusion"])
    if tests:
        print(f"{run}\t{job['name']}\t{phase}\t{tests[0]:.0f} ({tests[1]})\t{'' if not doc else f'{doc[0]:.0f} ({doc[1]})'}")
