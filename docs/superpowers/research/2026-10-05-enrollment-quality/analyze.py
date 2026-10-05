import pandas as pd, numpy as np, sys
S=sys.argv[1]
df=pd.read_csv(S+'/results_long.csv')
spk_n=df.speaker.nunique(); print('locutores',spk_n,'misturas/loc',df.groupby('speaker').mix.nunique().describe()[['min','max']].tolist())
df['d']=df.cond.str.extract(r'^d(\d+)_k')[0].astype(float)
# média sobre K por (speaker,mix,d)
dd=df[df.d.notna()].groupby(['speaker','corpus','mix','kind','d'])[['sisdri','stoi','sisdr']].mean().reset_index()
ctl=df[df.cond.isin(['neutral','wrong','noisy'])].rename(columns={'cond':'d'})[['speaker','corpus','mix','kind','d','sisdri','stoi','sisdr']]
rng=np.random.default_rng(0)
def boot(per,n=10000):
    idx=rng.integers(0,len(per),(n,len(per))); b=per[idx].mean(1); return per.mean(),*np.percentile(b,[2.5,97.5])
def table(sub_dd,sub_ctl,label):
    print(f'\n=== {label}  (misturas={sub_dd.groupby("speaker").mix.nunique().sum()}, locutores={sub_dd.speaker.nunique()})')
    ps=sub_dd.groupby(['speaker','d'])[['sisdri','stoi']].mean().reset_index()
    pv=ps.pivot(index='speaker',columns='d',values='sisdri'); pt=ps.pivot(index='speaker',columns='d',values='stoi')
    cs=sub_ctl.groupby(['speaker','d'])[['sisdri','stoi']].mean().reset_index()
    cv=cs.pivot(index='speaker',columns='d',values='sisdri'); ct=cs.pivot(index='speaker',columns='d',values='stoi')
    print('d | SI-SDRi [IC95] | dSISDRi vs 12 [IC95] | STOI | dSTOI vs 12 [IC95] | melhor/pior/(empate) vs 12 (SI-SDRi)')
    for d in [6,12,30,60,90]:
        m,lo,hi=boot(pv[d].values); dl=(pv[d]-pv[12]).values; dm,dlo,dhi=boot(dl)
        st=boot(pt[d].values)[0]; sd=(pt[d]-pt[12]).values; sm,slo,shi=boot(sd)
        print(f'{d:>3} | {m:.2f} [{lo:.2f},{hi:.2f}] | {dm:+.2f} [{dlo:+.2f},{dhi:+.2f}] | {st:.4f} | {sm:+.4f} [{slo:+.4f},{shi:+.4f}] | {(dl>0).sum()}/{(dl<0).sum()}')
    print('controles (SI-SDRi, STOI) e Δ de d=12 sobre eles:')
    for c in ['noisy','neutral','wrong']:
        m,lo,hi=boot(cv[c].values); dl=(pv[12]-cv[c]).values; dm,dlo,dhi=boot(dl); sm=boot((pt[12]-ct[c]).values)
        print(f'  {c:8s} {m:+.2f} [{lo:+.2f},{hi:+.2f}] STOI {ct[c].mean():.4f} | d12 menos {c}: SI-SDRi {dm:+.2f} [{dlo:+.2f},{dhi:+.2f}]  STOI {sm[0]:+.4f} [{sm[1]:+.4f},{sm[2]:+.4f}]')
    # neutro->d12 vs d90-d12 escala
    return pv
table(dd,ctl,'TODAS')
kd=dd.kind; kc=ctl.kind
for k,lab in [('INT','interferente sem ruído'),('INTN','interferente+ruído'),('NOISE','só ruído')]:
    table(dd[dd.kind==k],ctl[ctl.kind==k],lab)
for c in df.corpus.unique():
    table(dd[dd.corpus==c],ctl[ctl.corpus==c],'corpus '+c)
# variância entre sorteios K
v=df[df.d.notna()].copy(); v['k']=v.cond.str[-1]
w=v.groupby(['speaker','mix','d','k']).sisdri.mean().reset_index().groupby(['speaker','mix','d']).sisdri.std().groupby('d').mean()
print('\nDP médio (entre K=3 sorteios, mesma mistura) do SI-SDRi por d:\n',w.round(3).to_string())
# monotonia por locutor: tendência
pv=dd.groupby(['speaker','d']).sisdri.mean().unstack()
sl=[np.polyfit(np.log([6,12,30,60,90]),r.values,1)[0] for _,r in pv.iterrows()]
print('inclinação por log(d) (dB por e-fold), média',np.mean(sl),'IC',boot(np.array(sl))[1:])
dd.to_csv(S+'/per_mix_meanK.csv',index=False)
pv.to_csv(S+'/per_speaker_sisdri.csv')
