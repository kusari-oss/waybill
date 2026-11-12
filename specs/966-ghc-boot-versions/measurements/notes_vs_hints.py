import sys, re, html, os
hints={}; cur=None
for line in open(sys.argv[1]):
    m=re.match(r'^(ghc-[\d.]+):\s*$',line)
    if m: cur=m.group(1)[4:]; hints[cur]={}; continue
    m=re.match(r'^\s+(\S+):\s*\'?([^\'\s]+)',line)
    if m and cur: hints[cur][m.group(1)]=m.group(2)
tot_agree=tot_dis=0
for line in open(sys.argv[2]):
    v,code,n=line.split()
    if n=='0': continue
    t=open(os.path.join(sys.argv[3],f'notes-{v}.html'),encoding='utf-8',errors='replace').read()
    sec=t[t.index('id="included-libraries"') if 'id="included-libraries"' in t else t.index('Included libraries</'):]
    sec=sec[:sec.index('</table>')]
    rows=re.findall(r'<tr[^>]*>(.*?)</tr>',sec,re.S)
    notes={}
    for r in rows:
        cells=[html.unescape(re.sub(r'<[^>]+>','',c)).strip() for c in re.findall(r'<td[^>]*>(.*?)</td>',r,re.S)]
        if len(cells)>=2 and cells[0]: notes[cells[0]]=cells[1]
    h=hints.get(v,{})
    shared=set(notes)&set(h)
    dis=[(n,notes[n],h[n]) for n in sorted(shared) if notes[n]!=h[n]]
    tot_agree+=len(shared)-len(dis); tot_dis+=len(dis)
    print(f"{v}: notes {len(notes)} hints {len(h)} shared {len(shared)} disagree {dis} notes-only {sorted(set(notes)-set(h))} hints-only {sorted(set(h)-set(notes))}")
print("TOTAL agree",tot_agree,"disagree",tot_dis)
