import sys,time,numpy as np,torch
R='/home/joaorura/orca/projects/clearcore-train'
sys.path.insert(0,R+'/src'); sys.path.insert(0,R+'/scripts')
import importlib.util
from cctrain.eval.functional import enhance_offline, film_vector
from cctrain.train.checkpoint import load_checkpoint
from cctrain.train.stage_f import StageFHParams, build_models
ck=load_checkpoint(R+'/runs/m2/t3/checkpoints/ckpt-00022500.pt')
hp=ck.extra.get('hp',{})
def load(dev):
    h=StageFHParams(device=dev, **{k:v for k,v in hp.items() if k in('log_gamma_bound','beta_bound')})
    m,_,_=build_models(h,dev); m.load_state_dict(ck.model); return m.eval()
import onnxruntime as ort
so=ort.SessionOptions(); so.intra_op_num_threads=4
s=ort.InferenceSession(R+'/runs/m3/enrollment.onnx',so,providers=['CPUExecutionProvider'])
rng=np.random.default_rng(0)
for sec in (6,12,90):
    a=(rng.standard_normal(16000*sec)*0.05).astype(np.float32)
    t=time.time(); o=s.run(None,{'audio':a[None]}); print('enroll',sec,time.time()-t,[x.shape for x in o])
x=(rng.standard_normal(48000*5)*0.05).astype(np.float32)
emb=o[4]
for dev in ('cpu','cuda'):
    torch.set_num_threads(4)
    m=load(dev)
    enhance_offline(m.base,x,film_model=m,embedding=emb)
    t=time.time()
    for _ in range(3): y,_=enhance_offline(m.base,x,film_model=m,embedding=emb)
    print(dev,(time.time()-t)/3,'s per 5s clip')
    v=film_vector(m.film.cpu() if dev=='cuda' else m.film,emb)
    print('vec diff',np.abs(v-np.concatenate([o[0],o[1],o[2],o[3]])).max())
