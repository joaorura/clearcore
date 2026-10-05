import pandas as pd, numpy as np
df=pd.read_csv('results_long.csv')
print(len(df), df.speaker.nunique(), df.cond.nunique(), sorted(df.cond.unique()))
g=df.groupby('speaker').agg(n=('cond','size'),nmix=('mix','nunique'),ncond=('cond','nunique'))
print(g.n.value_counts().to_dict(), g.nmix.value_counts().to_dict(), g.ncond.value_counts().to_dict())
print(df.groupby(['speaker','mix','cond']).size().max())
print('NaN',df[['sisdri','stoi','sisdr','stoi_in']].isna().sum().to_dict(),'inf',np.isinf(df[['sisdri','stoi','sisdr']]).sum().sum())
print(df.corpus.value_counts().to_dict(), df.drop_duplicates('speaker').corpus.value_counts().to_dict())
df['d']=df.cond.str.extract(r'^d(\d+)_k')[0].astype(float)
dd=df[df.d.notna()].groupby(['speaker','corpus','mix','kind','d'])[['sisdri','stoi']].mean().reset_index()
ctl=df[df.cond.isin(['neutral','wrong','noisy'])].drop(columns='d').rename(columns={'cond':'d'})[['speaker','corpus','mix','kind','d','sisdri','stoi']]
rng=np.random.default_rng(0)
def boot(a,n=2000):
    idx=rng.integers(0,len(a),(n,len(a))); b=a[idx].mean(1); return a.mean(),*np.percentile(b,[2.5,97.5])
def table(a,c,label):
    print('\n==',label,'locutores',a.speaker.nunique(),'misturas',a.groupby('speaker').mix.nunique().sum())
    ps=a.groupby(['speaker','d'])[['sisdri','stoi']].mean().reset_index()
    pv=ps.pivot(index='speaker',columns='d',values='sisdri');pt=ps.pivot(index='speaker',columns='d',values='stoi')
    cs=c.groupby(['speaker','d'])[['sisdri','stoi']].mean().reset_index()
    cv=cs.pivot(index='speaker',columns='d',values='sisdri');ct=cs.pivot(index='speaker',columns='d',values='stoi')
    for d in [6,12,30,60,90]:
        m,lo,hi=boot(pv[d].values);dl=(pv[d]-pv[12]).values;dm,dlo,dhi=boot(dl);sd=(pt[d]-pt[12]).values;sm,slo,shi=boot(sd)
        print(f'd{d}: SISDRi {m:.3f} [{lo:.3f},{hi:.3f}] | D {dm:+.3f} [{dlo:+.3f},{dhi:+.3f}] | STOI {pt[d].mean():.4f} D {sm:+.4f} [{slo:+.4f},{shi:+.4f}] | melhor/pior {(dl>0).sum()}/{(dl<0).sum()} (STOI {(sd>0).sum()}/{(sd<0).sum()})')
    for k in ['noisy','neutral','wrong']:
        m,lo,hi=boot(cv[k].values);dl=(pv[12]-cv[k]).values;dm,dlo,dhi=boot(dl);sd=(pt[12]-ct[k]).values;sm,slo,shi=boot(sd)
        print(f'{k}: SISDRi {m:+.3f} [{lo:+.3f},{hi:+.3f}] STOI {ct[k].mean():.4f} | d12-{k}: {dm:+.3f} [{dlo:+.3f},{dhi:+.3f}] STOI {sm:+.4f} [{slo:+.4f},{shi:+.4f}] spk melhor/pior {(dl>0).sum()}/{(dl<0).sum()}')
    # wrong vs d12
    return pv
table(dd,ctl,'TODOS')
for k in ['INT','INTN','NOISE']: table(dd[dd.kind==k],ctl[ctl.kind==k],k)
for c in dd.corpus.unique(): table(dd[dd.corpus==c],ctl[ctl.corpus==c],c)
# sanity
print('\nSISDRi extremos',df.sisdri.describe().round(2).to_dict())
pm=df[df.cond.str.startswith('d')|df.cond.isin(['neutral','wrong'])].copy()
pm['piorou']=pm.stoi<pm.stoi_in
print('frac STOI<noisy por cond:',pm.groupby(pm.cond.where(~pm.cond.str.startswith('d'),'d*')).piorou.mean().round(3).to_dict())
print('por d:',pm[pm.cond.str.startswith('d')].assign(d=lambda x:x.cond.str.extract(r'd(\d+)')[0]).groupby('d').piorou.mean().round(3).to_dict())
pm['sipior']=pm.sisdri<0
print('frac SISDRi<0 d*,neutral,wrong:',pm.groupby(pm.cond.where(~pm.cond.str.startswith('d'),'d*')).sipior.mean().round(3).to_dict())
print('por kind STOI piorou (d*):',pm[pm.cond.str.startswith('d')].groupby('kind').piorou.mean().round(3).to_dict())
print('stoi_in medio',df[df.cond=='noisy'].stoi.mean().round(4), 'por kind',df[df.cond=='noisy'].groupby('kind').stoi.mean().round(3).to_dict())
print(df[df.cond=='noisy'].sisdri.abs().max())
dd.to_csv('partial_analysis.csv',index=False)
