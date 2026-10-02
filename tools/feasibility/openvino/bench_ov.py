#!/usr/bin/env python3
"""Benchmark DeepFilterNet3 ONNX graphs on OpenVINO devices (CPU, GPU, NPU)."""

import os
import sys
import tarfile
import time
import numpy as np
import openvino as ov


def get_export_dir() -> str:
    if "CLEARCORE_EXPORT_DIR" in os.environ:
        return os.environ["CLEARCORE_EXPORT_DIR"]
    default_tmp = "/tmp/clearcore-dfnet3-export/tmp/export"
    if os.path.exists(os.path.join(default_tmp, "enc.onnx")):
        return default_tmp
    repo_asset = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../vendor/approved/df-compatible-release-asset-v1.bin"))
    if os.path.exists(repo_asset):
        target_dir = "/tmp/clearcore-dfnet3-export"
        os.makedirs(target_dir, exist_ok=True)
        with tarfile.open(repo_asset, "r:gz") as tar:
            tar.extractall(path=target_dir)
        if os.path.exists(os.path.join(target_dir, "tmp/export/enc.onnx")):
            return os.path.join(target_dir, "tmp/export")
    raise RuntimeError("Could not locate DeepFilterNet3 ONNX export directory. Set CLEARCORE_EXPORT_DIR.")


def main():
    if len(sys.argv) < 2:
        print("Usage: bench_ov.py <DEVICE> (e.g. CPU, GPU, NPU)")
        sys.exit(1)

    dev = sys.argv[1]
    export_dir = get_export_dir()
    core = ov.Core()
    tot50 = 0.0
    tot99 = 0.0
    ok = True
    for name in ["enc", "erb_dec", "df_dec"]:
        path = os.path.join(export_dir, name + ".onnx")
        try:
            m = core.read_model(path)
            shapes = {}
            for inp in m.inputs:
                ps = inp.get_partial_shape()
                shapes[inp.get_any_name()] = [d.get_length() if d.is_static else 1 for d in ps]
            m.reshape(shapes)
            t0 = time.perf_counter()
            cm = core.compile_model(m, dev)
            tc = (time.perf_counter() - t0) * 1e3
            req = cm.create_infer_request()
            feeds = {k: np.zeros(v, dtype=np.float32) for k, v in shapes.items()}
            for _ in range(20):
                req.infer(feeds)
            ts = []
            for _ in range(200):
                t = time.perf_counter()
                req.infer(feeds)
                ts.append((time.perf_counter() - t) * 1e6)
            ts = np.array(ts)
            p50, p99 = np.percentile(ts, 50), np.percentile(ts, 99)
            tot50 += p50
            tot99 += p99
            print(
                "pid=%d %s %-8s OK compile=%.0fms p50=%.0fus p99=%.0fus min=%.0fus"
                % (os.getpid(), dev, name, tc, p50, p99, ts.min()),
                flush=True,
            )
        except Exception as e:
            ok = False
            msg = str(e).replace("\n", " | ")
            print("%s %-8s ERROR %s: %s" % (dev, name, type(e).__name__, msg[:700]), flush=True)
    if ok:
        print(
            "%s SUM(3 graphs) p50=%.0fus p99=%.0fus (soma dos percentis, estimativa)"
            % (dev, tot50, tot99)
        )


if __name__ == "__main__":
    main()
