import glob, os, re, time, resource, csv, json, sys
import numpy as np, soundfile as sf, onnxruntime as ort
from scipy.signal import resample_poly
R='/home/joaorura/orca/projects/clearcore-train/'
SR=16000
SPK=[('train',s) for s in ['9351','10107','5677','4367','12249','2961']]+[('test','5739'),('test','3050'),('dev','5417'),('dev','6549')]
so=ort.SessionOptions(); so.intra_op_num_threads=4; so.inter_op_num_threads=1
sess=ort.InferenceSession(R+'runs/m3/enrollment.onnx',so,providers=['CPUExecutionProvider'])
NAMES=['gamma_enc','beta_enc','gamma_df','beta_df']
def load(split,spk,secs=100):
    files=sorted(glob.glob(f'{R}data/raw/cml-tts-pt/{split}/audio/{spk}/*/*.flac'))
    out=[];n=0
    for f in files:
        x,sr=sf.read(f,dtype='float32')
        if x.ndim>1:x=x.mean(1)
        x=resample_poly(x,SR//np.gcd(SR,sr),sr//np.gcd(SR,sr)).astype(np.float32)
        out.append(x);n+=len(x)
        if n>=secs*SR:break
    x=np.concatenate(out)[:secs*SR]
    return x,len(out)
def emb(x):
    a=np.clip(x,-1,1)[None].astype(np.float32)
    t=time.perf_counter()
    o=sess.run(NAMES,{'audio':a})
    return [v.reshape(-1) for v in o],time.perf_counter()-t
def met(a,b):
    A=np.concatenate(a);B=np.concatenate(b)
    cos=lambda p,q:float(p@q/(np.linalg.norm(p)*np.linalg.norm(q)+1e-12))
    rel=lambda p,q:float(np.linalg.norm(p-q)/(np.linalg.norm(q)+1e-12))
    r={'cos':cos(A,B),'rel':rel(A,B)}
    for n,p,q in zip(NAMES,a,b): r['cos_'+n]=cos(p,q);r['rel_'+n]=rel(p,q)
    return r
def pink(n,rng):
    w=rng.standard_normal(n); F=np.fft.rfft(w); f=np.arange(len(F));f[0]=1
    p=np.fft.irfft(F/np.sqrt(f),n); return (p/p.std()).astype(np.float32)
DS=[2,4,6,8,10,12,15,20,30,45,60,90]
rows=[];stats=[];R12={};sp={};lat={}
rng=np.random.default_rng(0)
for split,spk in SPK:
    x,nf=load(split,spk,100); sp[spk]=x
    print(spk,split,len(x)/SR,'s',nf,'files',flush=True)
    r12,_=emb(x[:12*SR]); R12[spk]=r12
    j,_=emb(x[60*SR:72*SR]); rows.append(dict(spk=spk,cond='floor_J',d=12,**met(j,r12)))
    for d in DS:
        v,t=emb(x[:d*SR]); lat.setdefault(d,[]).append(t)
        rows.append(dict(spk=spk,cond='clean',d=d,**met(v,r12)))
        for n,a in zip(NAMES,v): stats.append(dict(spk=spk,d=d,vec=n,min=float(a.min()),max=float(a.max()),std=float(a.std()),norm=float(np.linalg.norm(a)),finite=bool(np.isfinite(a).all())))
    # noisy
    nz=pink(len(x),rng)
    for d in [6,12,30,60]:
        s=x[:d*SR]; n=nz[:d*SR]; g=np.sqrt((s**2).mean()/(10**(1.0)*(n**2).mean()))
        y=s+g*n; m=np.abs(y).max()
        if m>1:y=y/m
        v,_=emb(y); rows.append(dict(spk=spk,cond='noisy10dB',d=d,**met(v,r12)))
# inter-speaker
ks=list(R12)
for a in ks:
    for b in ks:
        if a!=b: rows.append(dict(spk=a,cond='inter',d=12,other=b,**met(R12[a],R12[b])))
keys=['cos','rel']+[p+n for n in NAMES for p in ['cos_','rel_']]
with open('metrics.csv','w',newline='') as f:
    w=csv.DictWriter(f,['spk','cond','d','other']+keys);w.writeheader()
    for r in rows:w.writerow(r)
with open('sanity.csv','w',newline='') as f:
    w=csv.DictWriter(f,list(stats[0]));w.writeheader();w.writerows(stats)
json.dump({str(d):v for d,v in lat.items()},open('latency.json','w'))
print('peak RSS MB',resource.getrusage(resource.RUSAGE_SELF).ru_maxrss/1024)
