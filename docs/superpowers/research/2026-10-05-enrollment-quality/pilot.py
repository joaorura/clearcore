"""Piloto de inferência: duração do áudio de enrollment x qualidade do pDFNet3 (M2, t3/ckpt-22500). Sem treino/rede."""
import sys, os, json, hashlib, time, csv
os.environ.setdefault("OMP_NUM_THREADS", "1")
import numpy as np
R = '/home/joaorura/orca/projects/clearcore-train'
sys.path.insert(0, R + '/src')
S = os.path.dirname(os.path.abspath(__file__))
SEED = 20261005
DURS = [6, 12, 30, 60, 90]
K = 3
N_MIX = 24
CORPORA = ['librispeech-dev-clean', 'vctk-0.92', 'cml-tts-pt']
PER_CORPUS = 8
SR = 48000
CKPT = R + '/runs/m2/t3/checkpoints/ckpt-00022500.pt'
ONNX = R + '/runs/m3/enrollment.onnx'

def H(*p):
    return int.from_bytes(hashlib.sha256('|'.join(map(str, p)).encode()).digest()[:8], 'big')

def vad_clean(x16, margin_frames=2, fr=320):
    """Remove silêncio: VAD de energia 20 ms, limiar max(-50 dBFS, max-30 dB), margem 40 ms. Devolve lista de trechos."""
    n = len(x16) // fr
    if n == 0: return []
    f = x16[:n * fr].astype(np.float64).reshape(n, fr)
    lv = 10 * np.log10((f * f).mean(1) + 1e-12)
    act = lv >= max(-50.0, lv.max() - 30.0)
    d = act.copy()
    for s in range(1, margin_frames + 1):
        d[s:] |= act[:-s]; d[:-s] |= act[s:]
    segs, i = [], 0
    while i < n:
        if d[i]:
            j = i
            while j < n and d[j]: j += 1
            if (j - i) * fr >= 2 * 320: segs.append(x16[i * fr:j * fr].astype(np.float32))
            i = j
        else: i += 1
    return segs

def xfade_concat(segs, cf=320):
    ramp = np.sin(np.linspace(0, np.pi / 2, cf, dtype=np.float32)) ** 2
    out = segs[0].copy()
    for s in segs[1:]:
        if len(s) <= cf or len(out) <= cf:
            out = np.concatenate([out, s]); continue
        mid = out[-cf:] * (1 - ramp) + s[:cf] * ramp
        out = np.concatenate([out[:-cf], mid, s[cf:]])
    return out

_G = {}
def init():
    import torch
    torch.set_num_threads(1)
    from cctrain.train.checkpoint import load_checkpoint
    from cctrain.train.stage_f import StageFHParams, build_models
    import onnxruntime as ort
    ck = load_checkpoint(CKPT)
    hp = ck.extra.get('hp', {})
    h = StageFHParams(device='cpu', **{k: v for k, v in hp.items() if k in ('log_gamma_bound', 'beta_bound')})
    m, _, _ = build_models(h, 'cpu'); m.load_state_dict(ck.model); _G['model'] = m.eval()
    so = ort.SessionOptions(); so.intra_op_num_threads = 1
    _G['ort'] = ort.InferenceSession(ONNX, so, providers=['CPUExecutionProvider'])
    from cctrain.paths import Paths
    _G['paths'] = Paths.from_env()
    from cctrain.m1 import pools
    from cctrain.m2.sex import load_speaker_sex
    from pathlib import Path
    dev = pools.by_split(pools.speech_records(_G['paths']), 'dev')
    spk = {}
    for r in dev:
        if r.source_id in CORPORA: spk.setdefault(r.speaker_key, []).append(r)
    _G['spk'] = spk
    _, nd = pools.noise_pools(); _G['noise'] = sorted(nd, key=lambda r: r.path)
    _G['sex'] = load_speaker_sex(Path(R) / 'manifests/speakers/sex-v1.jsonl', _G['paths'].data / 'manifests/speakers/sex-cv27-pt-v1.jsonl')

def load16(r):
    from cctrain.data.audio import load_audio
    from cctrain.data.resample import to_16k
    x, sr = load_audio(os.path.join(_G['paths'].data, r.path)); return to_16k(x, sr)

def load48(r):
    from cctrain.data.audio import load_audio, resample
    x, sr = load_audio(os.path.join(_G['paths'].data, r.path)); return resample(x, sr, SR)

def split_pool(sk):
    recs = sorted(_G['spk'][sk], key=lambda r: H(SEED, 'split', r.path))
    nm = max(8, int(round(0.3 * len(recs))))
    return recs[:nm], recs[nm:]   # (mistura, enrollment)

def enroll_stream(sk, k):
    """Fluxo de fala sem silêncio (>=90 s) a partir do pool de enrollment; ordem sorteada por (spk,k). Prefixo = duração d."""
    _, pool = split_pool(sk)
    order = sorted(pool, key=lambda r: H(SEED, 'enr', sk, k, r.path))
    segs = []; tot = 0
    for r in order:
        for s in vad_clean(load16(r)):
            segs.append(s); tot += len(s)
        if tot >= 90 * 16000 + 16000: break
    if tot < 90 * 16000: raise RuntimeError(f'{sk}: pool de enrollment com {tot/16000:.0f}s < 90s')
    return xfade_concat(segs)

def embed(a16):
    o = _G['ort'].run(None, {'audio': a16[None].astype(np.float32)})
    return o[4].astype(np.float32), np.concatenate(o[:4]).astype(np.float32)

def build_mixtures(sk):
    from cctrain.data.mix import MixKind, crop_or_pad, loop_to_length, make_mixture
    mix_recs, _ = split_pool(sk)
    corpus = sk.split(':')[0]; sex = _G['sex'].get(sk)
    peers = [s for s in sorted(_G['spk']) if s != sk and s.split(':')[0] == corpus and (sex is None or _G['sex'].get(s) == sex)]
    if not peers: peers = [s for s in sorted(_G['spk']) if s != sk and s.split(':')[0] == corpus]
    plan = [('INT', 0.0, None)] * 5 + [('INT', 5.0, None)] * 5 + [('INTN', 0.0, 10.0)] * 4 + [('INTN', 5.0, 10.0)] * 4 + \
           [('NOISE', None, 0.0), ('NOISE', None, 5.0), ('NOISE', None, 10.0)] * 2
    out = []
    for i, (kind, sir, snr) in enumerate(plan):
        rng = np.random.default_rng([SEED, H(sk) % (2**31), i])
        t = mix_recs[int(rng.integers(len(mix_recs)))]
        tx = load48(t)
        n = min(len(tx), int(rng.uniform(4, 8) * SR))
        target = crop_or_pad(tx, n, rng)
        interf = None; isk = None
        if kind != 'NOISE':
            isk = peers[int(rng.integers(len(peers)))]
            ir = _G['spk'][isk][int(rng.integers(len(_G['spk'][isk])))]
            interf = loop_to_length(load48(ir), n, rng)
        nz = None
        if kind != 'INT':
            nr = _G['noise'][int(rng.integers(len(_G['noise'])))]
            nz = loop_to_length(load48(nr), n, rng)
        mk = {'INT': MixKind.TARGET_INTERFERER, 'INTN': MixKind.TARGET_INTERFERER_NOISE, 'NOISE': MixKind.TARGET_NOISE}[kind]
        m = make_mixture(target, [interf] if interf is not None else [], nz, kind=mk,
                         snr_db=snr if snr is not None else float('nan'), sir_db=sir if sir is not None else 0.0)
        out.append(dict(i=i, kind=kind, sir=sir, snr=snr, noisy=m.noisy, clean=m.clean, target_utt=t.path, interferer=isk))
    return out

def enhance(emb, x):
    from cctrain.eval.functional import enhance_offline, OFFLINE_DELAY
    from cctrain.eval.runtime_path import align_runtime
    m = _G['model']
    y, _ = enhance_offline(m.base, x) if emb is None else enhance_offline(m.base, x, film_model=m, embedding=emb)
    return align_runtime(y, x.size, delay=OFFLINE_DELAY)

def run_speaker(sk, limit=None):
    if 'model' not in _G: init()
    from cctrain.eval.functional import si_sdr, film_vector
    from cctrain.eval import metrics as M
    t0 = time.time()
    emb = {}
    for k in range(K):
        st = enroll_stream(sk, k)
        for d in DURS:
            emb[(d, k)] = embed(st[:d * 16000])
    # locutor errado: outro locutor do mesmo corpus (mesmo sexo se conhecido), enrollment 12 s, k=0
    corpus = sk.split(':')[0]; sex = _G['sex'].get(sk)
    cands = [s for s in sorted(_G['spk']) if s != sk and s.split(':')[0] == corpus and (sex is None or _G['sex'].get(s) == sex)]
    wk = cands[H(SEED, 'wrong', sk) % len(cands)]
    wemb = embed(enroll_stream(wk, 0)[:12 * 16000])[0]
    # consistência: vetor do onnx == gerador PyTorch
    chk = float(np.abs(film_vector(_G['model'].film, emb[(12, 0)][0]) - emb[(12, 0)][1]).max())
    rows = []
    mixes = build_mixtures(sk)
    if limit: mixes = mixes[:limit]
    for mx in mixes:
        x, c = mx['noisy'], mx['clean']
        base = dict(speaker=sk, corpus=corpus, mix=mx['i'], kind=mx['kind'], sir=mx['sir'], snr=mx['snr'], wrong_spk=wk)
        si_in = si_sdr(x, c); st_in = float(M.stoi(x, c, SR)[0])
        def rec(cond, y):
            rows.append({**base, 'cond': cond, 'sisdr': si_sdr(y, c), 'sisdri': si_sdr(y, c) - si_in,
                         'stoi': float(M.stoi(y, c, SR)[0]), 'stoi_in': st_in, 'sisdr_in': si_in})
        rec('noisy', x)
        rec('neutral', enhance(None, x))
        rec('wrong', enhance(wemb, x))
        for (d, k), (e, _) in emb.items():
            rec(f'd{d}_k{k}', enhance(e, x))
    return dict(speaker=sk, rows=rows, film_check=chk, seconds=time.time() - t0)

def select_speakers():
    init()
    rng = np.random.default_rng(SEED)
    sel = []
    for c in CORPORA:
        ks = sorted(k for k in _G['spk'] if k.split(':')[0] == c)
        dur = {k: sum(r.num_samples / r.sr for r in _G['spk'][k]) for k in ks}
        ks = [k for k in ks if dur[k] >= 450]
        if c == 'librispeech-dev-clean':
            F = [k for k in ks if _G['sex'].get(k) == 'F']; M_ = [k for k in ks if _G['sex'].get(k) == 'M']
            pick = sorted(F, key=lambda k: H(SEED, k))[:PER_CORPUS // 2] + sorted(M_, key=lambda k: H(SEED, k))[:PER_CORPUS // 2]
        else:
            pick = sorted(ks, key=lambda k: H(SEED, k))[:PER_CORPUS]
        sel += pick
    return sel

if __name__ == '__main__':
    import multiprocessing as mp
    mode = sys.argv[1]
    if mode == 'select':
        sel = select_speakers()
        info = {k: dict(sex=_G['sex'].get(k), n_utt=len(_G['spk'][k]), dur_s=round(sum(r.num_samples / r.sr for r in _G['spk'][k])),
                        n_mix_utt=len(split_pool(k)[0]), n_enr_utt=len(split_pool(k)[1])) for k in sel}
        json.dump(info, open(S + '/speakers.json', 'w'), indent=1); print(json.dumps(info, indent=1))
    elif mode == 'test':
        sel = json.load(open(S + '/speakers.json')); sk = list(sel)[int(sys.argv[2])]
        init(); r = run_speaker(sk, limit=int(sys.argv[3]))
        print(r['seconds'], r['film_check'], len(r['rows']))
        for row in r['rows'][:22]: print(row['cond'], round(row['sisdri'], 2), round(row['stoi'], 3))
    elif mode == 'run':
        sel = list(json.load(open(S + '/speakers.json')))
        out = S + '/results_long.csv'; first = True
        with mp.get_context('spawn').Pool(4, initializer=init) as p, open(out, 'w', newline='') as f:
            w = None
            for res in p.imap_unordered(run_speaker, sel):
                print(res['speaker'], round(res['seconds']), 'film_check', res['film_check'], flush=True)
                if w is None:
                    w = csv.DictWriter(f, fieldnames=list(res['rows'][0])); w.writeheader()
                w.writerows(res['rows']); f.flush()
