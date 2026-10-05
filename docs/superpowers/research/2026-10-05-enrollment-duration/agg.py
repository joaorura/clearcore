import numpy as np, json, csv
class T:
    def __init__(s,rows,cols): s.r=rows; s._c={c:np.array([r[c] for r in rows]) for c in rows[0]}
    def __getattr__(s,k): return s._c[k]
    def __getitem__(s,k):
        if isinstance(k,str): return s._c[k]
        return T([r for r,b in zip(s.r,k) if b],list(s._c))
    def groupby(s,key,sort=True):
        ks=[key] if isinstance(key,str) else key
        keys=[tuple(r[k] for k in ks) for r in s.r]
        u=sorted(set(keys)) if sort else list(dict.fromkeys(keys))
        for g in u: yield (g[0] if isinstance(key,str) else g), T([r for r,kk in zip(s.r,keys) if kk==g],list(s._c))
def rd(f,num):
    rows=list(csv.DictReader(open(f)))
    for r in rows:
        for k in r:
            if k in num: r[k]=float(r[k])
    return rows
mrows=rd('metrics.csv',())
for r in mrows:
    r['d']=int(r['d'])
    for k in r:
        if k not in('spk','cond','d','other'): r[k]=float(r[k])
m=T(mrows,0); lat=json.load(open('latency.json'))
srows=list(csv.DictReader(open('sanity.csv')))
for r in srows:
    r['d']=int(r['d']); r['finite']=r['finite']=='True'
    for k in ('min','max','std','norm'): r[k]=float(r[k])
s=T(srows,0)
fl=m[m.cond=='floor_J']; it=m[m.cond=='inter']
ms=lambda x:f'{x.mean():.4f}±{x.std():.4f}'
out=['# Resumo\n']
out.append(f"Piso J: cos {ms(fl.cos)} | rel {ms(fl.rel)}\nInter-locutor (90 pares): cos {ms(it.cos)} | rel {ms(it.rel)}\n")
fm,im=fl.rel.mean(),it.rel.mean()
out.append('| d | cos | rel L2 | rel/piso | rel/inter | lat med ms |\n|---|---|---|---|---|---|')
c=m[m.cond=='clean']
for d,g in c.groupby('d'):
    out.append(f"| {d} | {ms(g.cos)} | {ms(g.rel)} | {g.rel.mean()/fm:.2f} | {g.rel.mean()/im:.2f} | {np.median(lat[str(d)])*1000:.0f} |")
out.append('\nPor vetor (rel L2 medio):\n| d | '+' | '.join(['gamma_enc','beta_enc','gamma_df','beta_df'])+' |\n|---|---|---|---|---|')
for d,g in c.groupby('d'): out.append(f'| {d} | '+' | '.join(f"{g['rel_'+n].mean():.3f} (cos {g['cos_'+n].mean():.3f})" for n in ['gamma_enc','beta_enc','gamma_df','beta_df'])+' |')
out.append('\nPiso/Inter por vetor (rel): '+'; '.join(f"{n}: J {fl['rel_'+n].mean():.3f} / inter {it['rel_'+n].mean():.3f}" for n in ['gamma_enc','beta_enc','gamma_df','beta_df']))
out.append('\nRuido 10 dB (vs R12 limpo):\n| d | cos | rel | clean rel |\n|---|---|---|---|')
n=m[m.cond=='noisy10dB']
for d,g in n.groupby('d'): out.append(f"| {d} | {ms(g.cos)} | {ms(g.rel)} | {c[c.d==d].rel.mean():.4f} |")
out.append('\nSanidade: nao-finitos='+str((~s.finite.astype(bool)).sum()))
out.append('| d | vec | min | max | std | norma (medias) |\n|---|---|---|---|---|---|')
for (d,v),g in s.groupby(['d','vec'],sort=False): out.append(f"| {d} | {v} | {g['min'].mean():.3f} | {g['max'].mean():.3f} | {g['std'].mean():.3f} | {g.norm.mean():.3f} |")
open('resumo.md','w').write('\n'.join(out));print('\n'.join(out))
