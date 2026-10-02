#!/usr/bin/env python3
"""Inspect DeepFilterNet3 ONNX graphs for opset, operators, GRUs, inputs and outputs."""

import collections
import os
import sys
import tarfile
import onnx


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


def shp(t):
    tt = t.type.tensor_type
    dims = []
    for d in tt.shape.dim:
        if d.HasField("dim_value"):
            dims.append(str(d.dim_value))
        elif d.HasField("dim_param"):
            dims.append("'" + d.dim_param + "'")
        else:
            dims.append("?")
    return onnx.TensorProto.DataType.Name(tt.elem_type), "[" + ",".join(dims) + "]"


def main():
    export_dir = get_export_dir()
    for name in ["enc", "erb_dec", "df_dec"]:
        path = os.path.join(export_dir, name + ".onnx")
        m = onnx.load(path)
        g = m.graph
        init = {i.name for i in g.initializer}
        print("=== %s  opset=%s  ir=%s  nodes=%d" % (
            name,
            [(o.domain or "ai.onnx", o.version) for o in m.opset_import],
            m.ir_version,
            len(g.node),
        ))
        for t in g.input:
            if t.name in init:
                continue
            print("  IN  %-14s %s %s" % ((t.name,) + shp(t)))
        for t in g.output:
            print("  OUT %-14s %s %s" % ((t.name,) + shp(t)))
        ops = collections.Counter(n.op_type for n in g.node)
        print("  ops:", dict(sorted(ops.items())))
        print(
            "  GRU nodes:",
            ops.get("GRU", 0),
            " LSTM:",
            ops.get("LSTM", 0),
            " Loop/Scan:",
            ops.get("Loop", 0),
            ops.get("Scan", 0),
        )
        for n in g.node:
            if n.op_type == "GRU":
                attrs = {
                    a.name: (a.i if a.type == 2 else a.ints and list(a.ints))
                    for a in n.attribute
                }
                print("   GRU", n.name, "inputs=", list(n.input), "outputs=", list(n.output), attrs)


if __name__ == "__main__":
    main()
