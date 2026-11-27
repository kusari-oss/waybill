import subprocess, shutil, filecmp, sys, os
R=subprocess.run(['git','rev-parse','--show-toplevel'],capture_output=True,text=True).stdout.strip(); os.chdir(R)
N='waybill-cli/src/scan_fs/package_db/haskell/nix_refs.rs'
H='waybill-cli/src/scan_fs/package_db/haskell.rs'
B=os.environ.get('MUT_BACKUP','/tmp/mut1664-bak')
os.makedirs(B,exist_ok=True)
for f in (N,H): shutil.copy(f, B+'/'+os.path.basename(f))
M=[
('M2 accept quoted paths',N,'        let tok = &rest[..end];','        let tok = rest.trim_start_matches(\'"\');\n        let tok = &tok[..tok.find(|c: char| c.is_whitespace() || c == \'"\').unwrap_or(tok.len())];'),
('M3 skip comment stripping',N,'let s = strip_comments(text);','let s = text.to_string();'),
('M4 allow ${',N,'\n            && !tok.contains("${");',';'),
('M5 drop root containment',N,'out.starts_with(rootfs).then_some(out)','Some(out)'),
('M6 omit C248',H,'        e.extra_annotations.insert(\n            "waybill:membership-source".to_string(),','        let _ = (\n            "waybill:membership-source".to_string(),'),
('M7 omit .nix from source-files',H,'                e.extra_source_paths.push(f);','                let _ = f;'),
('M8 C248 on declared dirs',H,'.filter(|c| c.starts_with(project_dir) && !declared.contains(*c))','.filter(|c| c.starts_with(project_dir))'),
('M9 drop lazy guard',H,'    if rejected.is_empty() {\n        return Vec::new();\n    }\n',''),
('M10 no newline between tokens',N,'let rest = s[from..].trim_start();',"let rest = s[from..].trim_start_matches(' ');"),
('M11 require bare callCabal2nix',N,'if s[..at].chars().next_back().is_some_and(is_ident_char) {',"if s[..at].chars().next_back().is_some_and(|c| is_ident_char(c) || c == '.') {"),
('M13 drop the quoted-name requirement',N,"let Some(rest) = rest.strip_prefix('\"') else {\n            continue;\n        };","let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '\"');"),
('M12 C248 on every component of the manifest',H,'if e.source_type.as_deref() != Some("hackage-main-module") {','if false {'),
]
only=sys.argv[1:] 
for name,f,o,n in M:
    if only and name.split()[0] not in only: continue
    s=open(f).read(); assert s.count(o)==1,(name,s.count(o))
    open(f,'w').write(s.replace(o,n))
    r=subprocess.run(['cargo','nextest','run','-p','waybill','--color','never','--no-fail-fast','-E','test(/nix_refs::|nix_admit_tests::/) | binary(nix_built_membership)'],capture_output=True,text=True)
    out=r.stdout+r.stderr
    fails=[l.strip() for l in out.splitlines() if l.strip().startswith('FAIL [')]
    summ=[l.strip() for l in out.splitlines() if 'Summary' in l]
    comp='error[' in out
    shutil.copy(B+'/'+os.path.basename(f), f)
    assert filecmp.cmp(f, B+'/'+os.path.basename(f), shallow=False)
    print(f"{name}: {'KILLED' if (fails or comp) else 'SURVIVED'} | {'compile error' if comp else ''} {summ[-1] if summ else ''}")
    for x in sorted(set(fails)): print('   ',x)
    sys.stdout.flush()
