"""Synthetic pDFNet3 Pro v3 FULL stand-in respecting the daemon contract.

Single stateful graph, S=1 per hop:
  inputs : feat_erb[1,1,1,32] feat_spec[1,2,1,96] speaker_embedding[1,192] speaker_present[1]
           h_in[1,1,64] erb_buf_in[1,1,2,32]
  outputs: m[1,1,1,32] coefs[1,1,96,10] lsnr[1,1,1] h_out erb_buf_out
Ops (opset 14; LayerNorm decomposed, tract 0.19 has no LayerNormalization): Gemm/MatMul, GRU,
ReduceMean/Pow/Sqrt, Softmax, Einsum, Sigmoid, Tanh, Concat, Slice.
Usage: python -I make_synth.py OUT_DIR
"""
import hashlib, io, json, math, os, sys, tarfile
import torch, torch.nn as nn

torch.manual_seed(1234)
H, K, D, E = 64, 16, 128, 192

class Synth(nn.Module):
    def __init__(self):
        super().__init__()
        self.inp = nn.Linear(3 * 32 + 2 * 96, H)
        self.gru = nn.GRU(H, H, batch_first=True)
        self.tok = nn.Sequential(nn.Linear(E, 16), nn.Linear(16, K * D))
        self.norm = nn.LayerNorm(D)
        self.q = nn.Linear(H, D)
        self.o = nn.Linear(D, H)
        self.m = nn.Linear(H, 32)
        self.c = nn.Linear(H, 96 * 10)
        self.l = nn.Linear(H, 1)
        with torch.no_grad():
            self.o.weight.mul_(4.0)
            self.l.bias.fill_(1.0)

    def forward(self, feat_erb, feat_spec, speaker_embedding, speaker_present, h_in, erb_buf_in):
        x = torch.cat([erb_buf_in, feat_erb], dim=2)          # [1,1,3,32]
        erb_buf_out = x[:, :, 1:, :]
        f = torch.cat([x.reshape(1, 1, 96), feat_spec.reshape(1, 1, 192)], dim=-1)
        y, h_out = self.gru(torch.tanh(self.inp(f)), h_in)     # [1,1,H]
        tokens = self.norm(self.tok(speaker_embedding).reshape(1, K, D))
        q = self.q(y)
        scores = torch.einsum("bqd,bkd->bqk", q, tokens) / math.sqrt(D)
        ctx = torch.einsum("bqk,bkd->bqd", torch.softmax(scores, dim=-1), tokens)
        y2 = y + speaker_present.reshape(1, 1, 1) * self.o(ctx)
        m = torch.sigmoid(self.m(y2)).reshape(1, 1, 1, 32)
        coefs = (0.5 * torch.tanh(self.c(y2))).reshape(1, 1, 96, 10)
        lsnr = (10.0 * self.l(y2)).reshape(1, 1, 1)
        return m, coefs, lsnr, h_out, erb_buf_out

def main(out):
    os.makedirs(out, exist_ok=True)
    model = Synth().eval()
    args = (torch.zeros(1, 1, 1, 32), torch.zeros(1, 2, 1, 96), torch.zeros(1, E), torch.zeros(1),
            torch.zeros(1, 1, H), torch.zeros(1, 1, 2, 32))
    ins = ["feat_erb", "feat_spec", "speaker_embedding", "speaker_present", "h_in", "erb_buf_in"]
    outs = ["m", "coefs", "lsnr", "h_out", "erb_buf_out"]
    onnx_path = os.path.join(out, "pdfnet3_pro_v3_full.onnx")
    torch.onnx.export(model, args, onnx_path, input_names=ins, output_names=outs,
                      opset_version=14, dynamo=False)
    data = open(onnx_path, "rb").read()
    shapes = {n: list(a.shape) for n, a in zip(ins, args)}
    with torch.no_grad():
        o = model(*args)
    oshapes = {n: list(t.shape) for n, t in zip(outs, o)}
    pairs = [("h_in", "h_out"), ("erb_buf_in", "erb_buf_out")]
    state = {n for p in pairs for n in p}
    manifest = {
        "format_version": 1,
        "model": "pdfnet3-pro-v3-full",
        "synthetic": True,
        "hop": 480,
        "sample_rate": 48000,
        "opset": 14,
        "files": {"pdfnet3_pro_v3_full.onnx": {"sha256": hashlib.sha256(data).hexdigest(),
                                               "size": len(data)}},
        "graphs": {"pdfnet3_pro_v3_full": {
            "file": "pdfnet3_pro_v3_full.onnx",
            "inputs": {n: {"dtype": "float32", "shape": shapes[n]} for n in ins if n not in state},
            "outputs": {n: {"dtype": "float32", "shape": oshapes[n]} for n in outs if n not in state},
            "state_pairs": [{"in": a, "out": b, "dtype": "float32", "init": "zeros",
                             "shape": shapes[a]} for a, b in pairs],
        }},
    }
    mpath = os.path.join(out, "manifest.json")
    with open(mpath, "w") as fh:
        json.dump(manifest, fh, indent=2)
    tpath = os.path.join(out, "pdfnet3-pro-v3-full-synthetic.tar.gz")
    with tarfile.open(tpath, "w:gz", format=tarfile.USTAR_FORMAT) as tar:
        for name in ["manifest.json", "pdfnet3_pro_v3_full.onnx"]:
            info = tarfile.TarInfo(name)
            payload = open(os.path.join(out, name), "rb").read()
            info.size = len(payload); info.mtime = 0; info.mode = 0o644
            tar.addfile(info, io.BytesIO(payload))
    print(tpath, hashlib.sha256(open(tpath, "rb").read()).hexdigest(), len(data))

if __name__ == "__main__":
    main(sys.argv[1])
