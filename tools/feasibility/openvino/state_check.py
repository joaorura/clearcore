#!/usr/bin/env python3
"""Check frame-by-frame state divergence on OpenVINO against continuous multi-frame execution."""

import os
import sys
import tarfile
import numpy as np
import openvino as ov


def get_export_dir() -> str:
    if "CLEARCORE_EXPORT_DIR" in os.environ:
        return os.environ["CLEARCORE_EXPORT_DIR"]
    default_tmp = "/tmp/clearcore-dfnet3-export/tmp/export"
    if os.path.exists(os.path.join(default_tmp, "enc.onnx")):
        return default_tmp
    scratchpad_dir = "/tmp/claude-1000/-home-joaorura-orca-workspaces-clearcore-hippocamp/463e3f9e-5f34-4ae2-a2be-58d4067aa248/scratchpad/ov-feasibility/asset/tmp/export"
    if os.path.exists(os.path.join(scratchpad_dir, "enc.onnx")):
        return scratchpad_dir
    repo_asset = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../vendor/approved/df-compatible-release-asset-v1.bin"))
    if os.path.exists(repo_asset):
        target_dir = "/tmp/clearcore-dfnet3-export"
        os.makedirs(target_dir, exist_ok=True)
        with tarfile.open(repo_asset, "r:gz") as tar:
            tar.extractall(path=target_dir)
        if os.path.exists(os.path.join(target_dir, "tmp/export/enc.onnx")):
            return os.path.join(target_dir, "tmp/export")
    raise RuntimeError("Could not locate DeepFilterNet3 ONNX export directory. Set CLEARCORE_EXPORT_DIR.")


def run(core, S, erb, spec, export_dir):
    m = core.read_model(os.path.join(export_dir, "enc.onnx"))
    m.reshape({"feat_erb": [1, 1, S, 32], "feat_spec": [1, 2, S, 96]})
    cm = core.compile_model(m, "CPU")
    r = cm.create_infer_request()
    r.infer({"feat_erb": erb, "feat_spec": spec})
    return {o.get_any_name(): r.get_tensor(o).data.copy() for o in cm.outputs}


def main():
    export_dir = get_export_dir()
    core = ov.Core()
    rng = np.random.default_rng(0)
    T = 8
    erb = rng.standard_normal((1, 1, T, 32)).astype(np.float32)
    spec = rng.standard_normal((1, 2, T, 96)).astype(np.float32)
    full = run(core, T, erb, spec, export_dir)
    print("full S=8 emb shape", full["emb"].shape)
    # frame a frame, S=1, sem estado carregado entre chamadas
    frames = [run(core, 1, erb[:, :, t : t + 1], spec[:, :, t : t + 1], export_dir) for t in range(T)]
    for name in ["emb", "lsnr"]:
        seq = np.concatenate([f[name] for f in frames], axis=1)
        d = np.abs(seq - full[name])
        print("%s: max|S=1 por frame - S=8| por frame t = %s" % (name, np.round(d.reshape(T, -1).max(axis=1), 4)))
    # primeiro frame deve coincidir (estado zero e padding causal igual)
    # os seguintes divergem se o estado (GRU e contexto temporal da conv) nao for carregado


if __name__ == "__main__":
    main()
