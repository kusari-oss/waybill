# Spec 1657 (#1712): move the corpus harness's self-tests out of the module
# tree every consumer includes, into one dedicated test binary.
#   python3 -I -B move_harness_tests.py waybill-cli/tests
# Inline `#[cfg(test)] ... mod NAME { ... }` blocks are cut from each harness
# file; whole-file test modules (`#[cfg(test)] mod X;` in invariants/mod.rs)
# are moved; manifest.rs's trailing audit-test section is moved. Paths that
# reached the old parent through `super::` are rewritten to the absolute
# `crate::corpus_harness_195::...` path. Prints what moved.
import os, re, sys
T = sys.argv[1]; H = os.path.join(T, 'corpus_harness_195'); ST = os.path.join(T, 'corpus_harness_self_tests')
decls = []  # (module path in the self-test crate, file path relative to T)

def modpath(rel):  # 'invariants/checks/x.rs' -> ['invariants','checks','x']; 'invariants/mod.rs' -> ['invariants']
    parts = rel[:-3].split('/')
    return parts[:-1] if parts[-1] == 'mod' else parts

def rewrite(text, owner):  # owner: the module the code used to live inside (list)
    def absolute(p): return 'crate::corpus_harness_195' + ''.join('::' + x for x in p)
    text = text.replace('super::super::', absolute(owner[:-1]) + '::')
    return re.sub(r'\bsuper::', absolute(owner) + '::', text)

def write(dest_mod, body):
    rel = os.path.join('corpus_harness_self_tests', *dest_mod) + '.rs'
    os.makedirs(os.path.dirname(os.path.join(T, rel)), exist_ok=True)
    open(os.path.join(T, rel), 'w').write(body)
    decls.append(('_'.join(dest_mod), rel))

for root, _, files in os.walk(H):
    for f in sorted(files):
        if not f.endswith('.rs'): continue
        path = os.path.join(root, f); rel = os.path.relpath(path, H); mp = modpath(rel)
        if not os.path.exists(path): continue
        lines = open(path).read().split('\n'); out = []; i = 0; changed = False
        while i < len(lines):
            if lines[i].strip() in ('#[cfg(test)]', '#[cfg(all(test, unix))]'):
                j = i + 1
                while j < len(lines) and lines[j].startswith('#['): j += 1
                m = re.match(r'^(pub(\(crate\))? )?mod (\w+) \{$', lines[j]) if j < len(lines) else None
                if m:
                    k = j + 1
                    while lines[k] != '}': k += 1
                    attrs = [l for l in lines[i+1:j]]
                    body = [l[4:] if l.startswith('    ') else l for l in lines[j+1:k]]
                    write(mp + [m.group(3)], '\n'.join(attrs and [] or []) + rewrite('\n'.join(body), mp) + '\n')
                    decls[-1] = decls[-1] + ('\n'.join(['#[cfg(unix)]'] if lines[i].strip() == '#[cfg(all(test, unix))]' else []) + ('\n' if attrs else '') + '\n'.join(attrs),)
                    i = k + 1; changed = True
                    if i < len(lines) and lines[i] == '': i += 1
                    continue
                m2 = re.match(r'^mod (\w+);$', lines[j]) if j < len(lines) else None
                if m2:  # whole-file test module
                    src = os.path.join(root, m2.group(1) + '.rs')
                    write(mp + [m2.group(1)], rewrite(open(src).read(), mp))
                    decls[-1] = decls[-1] + ('\n'.join(lines[i+1:j]),)
                    os.remove(src); i = j + 1; changed = True; continue
            out.append(lines[i]); i += 1
        if rel == 'manifest.rs':
            cut = next(n for n, l in enumerate(out) if 'Manifest audit tests' in l) - 1
            write(['manifest_audit_tests'], 'use crate::corpus_harness_195::manifest::*;\n\n' + rewrite('\n'.join(out[cut:]), ['manifest']))
            decls[-1] = decls[-1] + ('',)
            out = out[:cut]; changed = True
        if changed: open(path, 'w').write('\n'.join(out).rstrip('\n') + '\n')

top = ['//! Spec 1657 (#1712): the corpus harness\'s own tests, compiled into one',
       '//! binary. They used to sit inside `corpus_harness_195/`, which 17 test',
       '//! files include, so every one of them compiled and ran all 144.', '',
       '#[path = "corpus_harness_195/mod.rs"]', 'mod corpus_harness_195;', '']
for name, rel, attrs in sorted(decls, key=lambda d: d[1]):
    top += [a for a in attrs.split('\n') if a and a != '#[cfg(test)]' and not a.startswith('#[cfg(test)]')] + [f'#[path = "{rel}"]', f'mod {name};', '']
open(os.path.join(T, 'corpus_harness_self_tests.rs'), 'w').write('\n'.join(top))
for name, rel, _ in sorted(decls, key=lambda d: d[1]): print(name, rel)
print('modules moved:', len(decls))
