"""Synthetic stand-in for the pDFNet3 Pro v3 FULL asset, in the real manifest format.

Three tiny stateful graphs (S=1) wired like the real trio: `enc` (audio + speaker conditioning,
states enc_h, feat_erb_buf) -> `erb_dec` (routed emb, erb_gain; state erb_h) and `df_dec`
(routed emb + runtime speaker conditioning; state c0_buf). Same I/O names as the real export.
Weights are random (seed 1234): the output is not denoised audio, only a contract fixture.

Usage: python -I make_synth_v3.py <out_dir>   (torch + onnx; e.g. the clearcore-train venv)
"""
import hashlib, io, json, os, sys, tarfile

import torch
from torch import nn

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
torch.manual_seed(1234)


class Enc(nn.Module):
    def __init__(self):
        super().__init__()
        self.inp = nn.Linear(32 * 3 + 32, 32)
        self.spk = nn.Linear(192, 32)
        self.gru = nn.GRU(32, 32, batch_first=True)
        self.gain = nn.Linear(32, 32)
        self.lsnr = nn.Linear(32, 1)

    def forward(self, feat_erb, feat_spec, speaker_embedding, speaker_present, enc_h_in, feat_erb_buf_in):
        hist = torch.cat([feat_erb_buf_in, feat_erb], dim=2)  # [1,1,3,32]
        spec_e = (feat_spec ** 2).sum(dim=1).reshape(1, 1, 32, 3).mean(dim=-1)
        x = torch.tanh(self.inp(torch.cat([hist.reshape(1, 1, 96), torch.log1p(spec_e)], dim=-1)))
        x = x + speaker_present.reshape(1, 1, 1) * torch.tanh(4.0 * self.spk(speaker_embedding)).reshape(1, 1, 32)
        emb, enc_h_out = self.gru(x, enc_h_in)
        erb_gain = torch.sigmoid(self.gain(emb)).reshape(1, 1, 1, 32)
        lsnr = (8.0 + 6.0 * torch.tanh(self.lsnr(emb))).reshape(1, 1, 1)
        return emb, lsnr, erb_gain, enc_h_out, hist[:, :, 1:, :]


class ErbDec(nn.Module):
    def __init__(self):
        super().__init__()
        self.gru = nn.GRU(32, 24, num_layers=2, batch_first=True)
        self.out = nn.Linear(24, 32)

    def forward(self, emb, erb_gain, erb_h_in):
        y, erb_h_out = self.gru(emb, erb_h_in)
        m = torch.sigmoid(self.out(y)).reshape(1, 1, 1, 32) * erb_gain
        return m, erb_h_out


class DfDec(nn.Module):
    def __init__(self):
        super().__init__()
        self.spk = nn.Linear(192, 32)
        self.low = nn.Linear(32 + 96, 8)
        self.out = nn.Linear(8, 960)
        with torch.no_grad():
            self.out.weight.mul_(0.05)
            self.out.bias.zero_()

    def forward(self, emb, speaker_embedding, speaker_present, c0_buf_in):
        z = emb.reshape(1, 32) + speaker_present.reshape(1, 1) * torch.tanh(4.0 * self.spk(speaker_embedding))
        c = self.out(torch.tanh(self.low(torch.cat([z, c0_buf_in.mean(dim=1).reshape(1, 96)], dim=-1)))).reshape(1, 1, 96, 10)
        ident = torch.zeros(1, 1, 96, 10)
        ident[..., 4] = 1.0  # tap 2, real part: the aligned frame
        new = torch.tanh(c[..., 0:1].reshape(1, 1, 96))  # [1,1,96]
        c0_buf_out = torch.cat([c0_buf_in[:, 1:, :], new], dim=1)
        return c + ident, c0_buf_out


GRAPHS = {
    "enc": (Enc(), {"feat_erb": [1, 1, 1, 32], "feat_spec": [1, 2, 1, 96], "speaker_embedding": [1, 192],
                    "speaker_present": [1]},
            [("enc_h_in", "enc_h_out", [1, 1, 32]), ("feat_erb_buf_in", "feat_erb_buf_out", [1, 1, 2, 32])],
            {"emb": [1, 1, 32], "lsnr": [1, 1, 1], "erb_gain": [1, 1, 1, 32]}),
    "erb_dec": (ErbDec(), {"emb": [1, 1, 32], "erb_gain": [1, 1, 1, 32]},
                [("erb_h_in", "erb_h_out", [2, 1, 24])], {"m": [1, 1, 1, 32]}),
    "df_dec": (DfDec(), {"emb": [1, 1, 32], "speaker_embedding": [1, 192], "speaker_present": [1]},
               [("c0_buf_in", "c0_buf_out", [1, 4, 96])], {"coefs": [1, 1, 96, 10]}),
}

files, graphs_manifest, blobs = {}, {}, {}
for name, (module, inputs, states, outputs) in GRAPHS.items():
    module.eval()
    in_names = list(inputs) + [s[0] for s in states]
    out_names = list(outputs) + [s[1] for s in states]
    dummy = tuple(torch.zeros(s) for s in list(inputs.values()) + [s[2] for s in states])
    buf = io.BytesIO()
    torch.onnx.export(module, dummy, buf, input_names=in_names, output_names=out_names,
                      opset_version=12, dynamo=False, do_constant_folding=True)
    data = buf.getvalue()
    fname = f"{name}.onnx"
    blobs[fname] = data
    files[fname] = {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
    graphs_manifest[name] = {
        "file": fname,
        "inputs": {k: {"dtype": "float32", "shape": v} for k, v in inputs.items()},
        "outputs": {k: {"dtype": "float32", "shape": v} for k, v in outputs.items()},
        "state_pairs": [{"dtype": "float32", "in": i, "out": o, "shape": s, "init": "zeros"} for i, o, s in states],
    }

manifest = {
    "model": "pdfnet3-pro-v3-full", "format_version": 1, "hop": 480, "sample_rate": 48000,
    "synthetic": True, "files": files, "graphs": graphs_manifest,
    "routing": {"erb_dec": {"emb": "enc.emb", "erb_gain": "enc.erb_gain"},
                "df_dec": {"emb": "enc.emb", "speaker_embedding": "runtime", "speaker_present": "runtime"}},
}
blobs["manifest.json"] = json.dumps(manifest, indent=2, sort_keys=True).encode()
tar_path = os.path.join(OUT, "pdfnet3-pro-v3-full-synthetic.tar.gz")
raw = io.BytesIO()
with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as tar:
    for name in sorted(blobs):
        info = tarfile.TarInfo(name); info.size = len(blobs[name]); info.mtime = 0; info.mode = 0o644
        tar.addfile(info, io.BytesIO(blobs[name]))
import gzip
with open(tar_path, "wb") as f:
    f.write(gzip.compress(raw.getvalue(), mtime=0))
print(tar_path, hashlib.sha256(open(tar_path, "rb").read()).hexdigest(), os.path.getsize(tar_path))
