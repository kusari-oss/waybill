"""Spec 1665 mutations (T010). Each mutation is one or more exact string
replacements in haskell.rs. The test set runs under nextest, the file is
restored from a backup, and the restore is checked with filecmp.

usage: python3 -I -B mutate.py [M1 M2 ...]   (MUT_BACKUP sets the backup dir)
"""
import filecmp, os, shutil, subprocess, sys

R = subprocess.run(['git', 'rev-parse', '--show-toplevel'], capture_output=True, text=True).stdout.strip()
os.chdir(R)
H = 'waybill-cli/src/scan_fs/package_db/haskell.rs'
B = os.environ.get('MUT_BACKUP', '/tmp/mut1665-bak')
NEXTEST = os.environ.get('NEXTEST', 'cargo nextest').split()
os.makedirs(B, exist_ok=True)
shutil.copy(H, B + '/haskell.rs')

TESTS = 'binary(lock_package_source) | test(/parse_cabal_freeze_exact_pins|parse_stack_lock_schema_v1/)'
M = [
    ('M1 no stack-lock source', [
        ('source_path: entry.source.to_string_lossy().into_owned(),', 'source_path: String::new(),')]),
    ('M2 no freeze source', [
        ('source_path: source.to_string_lossy().into_owned(),', 'source_path: { let _ = source; String::new() },')]),
    ('M3 first-wins', [
        ('pin_at.insert(purl_key, out.len());', 'let _ = (purl_key, out.len());')]),
    ('M4 merge keyed by name', [
        ('if let Some(&i) = pin_at.get(&purl_key) {', 'if let Some(&i) = pin_at.get(&component.name) {'),
        ('pin_at.insert(purl_key, out.len());', 'let _ = purl_key; pin_at.insert(component.name.clone(), out.len());')]),
    ('M6 merge only within the stack-lock loop', [
        ('    let lock_components = freeze_entries\n        .iter()\n        .map(build_freeze_component)\n        .chain(stack_lock_entries.iter().map(build_stack_lock_component));',
         '    for c in freeze_entries.iter().map(build_freeze_component) {\n        if seen_purls.insert(c.purl.as_str().to_string()) {\n            out.push(c);\n        }\n    }\n    let lock_components = stack_lock_entries.iter().map(build_stack_lock_component);')]),
]
only = sys.argv[1:]
for name, reps in M:
    if only and name.split()[0] not in only:
        continue
    s = open(H).read()
    for o, n in reps:
        assert s.count(o) == 1, (name, o[:60], s.count(o))
        s = s.replace(o, n)
    open(H, 'w').write(s)
    r = subprocess.run(NEXTEST + ['run', '-p', 'waybill', '--color', 'never', '--no-fail-fast', '-E', TESTS],
                       capture_output=True, text=True)
    out = r.stdout + r.stderr
    fails = sorted({l.strip() for l in out.splitlines() if l.strip().startswith('FAIL [')})
    summ = [l.strip() for l in out.splitlines() if 'Summary' in l]
    comp = 'error[' in out
    shutil.copy(B + '/haskell.rs', H)
    assert filecmp.cmp(H, B + '/haskell.rs', shallow=False)
    verdict = 'KILLED' if (fails or comp) else 'SURVIVED'
    print(f"{name}: {verdict} | {'compile error ' if comp else ''}{summ[-1] if summ else ''}")
    for f in fails:
        print('   ', f)
    sys.stdout.flush()
