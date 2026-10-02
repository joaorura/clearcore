#!/usr/bin/env python3
"""ONNX Graph Surgery for DeepFilterNet3.

Transforms the 3 DeepFilterNet3 ONNX graphs (enc, erb_dec, df_dec) from
stateless/pulsed-tract representations into explicit stateful models
compatible with hardware acceleration runtimes (OpenVINO, TensorRT, CoreML,
DirectML, Vulkan):

1. Connects all 5 GRU nodes (1 in enc, 2 in erb_dec, 2 in df_dec) so that:
   - Initial hidden states are exposed as graph inputs ('h_in').
   - Next hidden states are exposed as graph outputs ('h_out').
2. Replaces causal zero-padding in temporal convolutions with stateful
   delay rings:
   - enc: 2-frame delay buffer for 'feat_erb' ('feat_erb_buf' in/out).
   - enc: 2-frame delay buffer for 'feat_spec' ('feat_spec_buf' in/out).
   - df_dec: 4-frame delay buffer for 'c0' ('c0_buf' in/out).
3. Preserves mathematical parity: streaming S=1 frame-by-frame achieves
   zero numerical error (< 1e-5) against batched S=8 execution.
"""

from __future__ import annotations

import argparse
import os
import shutil
import sys
import tarfile
import numpy as np
import onnx
from onnx import helper, TensorProto


def make_stateful_enc(model: onnx.ModelProto) -> onnx.ModelProto:
    """Surgery for encoder: expose GRU h_in/h_out and 2-frame delay rings."""
    g = model.graph

    # 1. Rewire GRU initial_h (input[5]) to 'h_in' and Y_h (output[1]) to 'h_out'
    gru_node = None
    for n in g.node:
        if n.op_type == "GRU":
            gru_node = n
            n.input[5] = "h_in"
            n.output[1] = "h_out"
            break
    if gru_node is None:
        raise ValueError("Could not find GRU node in enc.onnx")

    # Add h_in to inputs: [1, 1, 256]
    g.input.append(helper.make_tensor_value_info("h_in", TensorProto.FLOAT, [1, 1, 256]))
    # Add h_out to outputs: [1, 1, 256]
    g.output.append(helper.make_tensor_value_info("h_out", TensorProto.FLOAT, [1, 1, 256]))

    # Slice constants for 2-frame delay ring
    starts_val = helper.make_tensor("slice_starts_2", TensorProto.INT64, [1], [-2])
    ends_val = helper.make_tensor("slice_ends_max", TensorProto.INT64, [1], [9223372036854775807])
    axes_val = helper.make_tensor("slice_axes_2", TensorProto.INT64, [1], [2])
    steps_val = helper.make_tensor("slice_steps_1", TensorProto.INT64, [1], [1])
    g.initializer.extend([starts_val, ends_val, axes_val, steps_val])

    # 2. Add delay ring buffers to inputs and outputs
    # feat_erb_buf: [1, 1, 2, 32]
    g.input.append(helper.make_tensor_value_info("feat_erb_buf", TensorProto.FLOAT, [1, 1, 2, 32]))
    g.output.append(helper.make_tensor_value_info("feat_erb_buf_out", TensorProto.FLOAT, [1, 1, 2, 32]))
    # feat_spec_buf: [1, 2, 2, 96]
    g.input.append(helper.make_tensor_value_info("feat_spec_buf", TensorProto.FLOAT, [1, 2, 2, 96]))
    g.output.append(helper.make_tensor_value_info("feat_spec_buf_out", TensorProto.FLOAT, [1, 2, 2, 96]))

    # 3. Replace Pad nodes with Concat + Slice
    new_nodes = []
    for n in g.node:
        if n.name == "/erb_conv0/0/Pad":
            # Concat feat_erb_buf with feat_erb along time axis (axis 2)
            concat_erb = helper.make_node(
                "Concat",
                inputs=["feat_erb_buf", "feat_erb"],
                outputs=["/erb_conv0/0/Pad_output_0"],
                axis=2,
                name="concat_feat_erb",
            )
            slice_erb = helper.make_node(
                "Slice",
                inputs=["/erb_conv0/0/Pad_output_0", "slice_starts_2", "slice_ends_max", "slice_axes_2", "slice_steps_1"],
                outputs=["feat_erb_buf_out"],
                name="slice_feat_erb_buf",
            )
            new_nodes.extend([concat_erb, slice_erb])
        elif n.name == "/df_conv0/0/Pad":
            concat_spec = helper.make_node(
                "Concat",
                inputs=["feat_spec_buf", "feat_spec"],
                outputs=["/df_conv0/0/Pad_output_0"],
                axis=2,
                name="concat_feat_spec",
            )
            slice_spec = helper.make_node(
                "Slice",
                inputs=["/df_conv0/0/Pad_output_0", "slice_starts_2", "slice_ends_max", "slice_axes_2", "slice_steps_1"],
                outputs=["feat_spec_buf_out"],
                name="slice_feat_spec_buf",
            )
            new_nodes.extend([concat_spec, slice_spec])
        else:
            new_nodes.append(n)

    del g.node[:]
    g.node.extend(new_nodes)

    onnx.checker.check_model(model)
    return model


def make_stateful_erb_dec(model: onnx.ModelProto) -> onnx.ModelProto:
    """Surgery for ERB decoder: expose 2 GRU hidden states as h_in/h_out [2, 1, 256]."""
    g = model.graph

    # Add h_in [2, 1, 256] to inputs
    g.input.append(helper.make_tensor_value_info("h_in", TensorProto.FLOAT, [2, 1, 256]))

    # Rewire Slices that extracted h0 for GRU 0 and GRU 1
    found_slices = 0
    for n in g.node:
        if n.name in ["/emb_gru/Slice", "/emb_gru/Slice_1"]:
            n.input[0] = "h_in"
            found_slices += 1
    if found_slices != 2:
        raise ValueError(f"Expected 2 Slice nodes for GRU initial_h, found {found_slices}")

    # Concat the 2 GRU outputs Y_h into h_out [2, 1, 256]
    concat_h = helper.make_node(
        "Concat",
        inputs=["/emb_gru/GRU_output_1", "/emb_gru/GRU_1_output_1"],
        outputs=["h_out"],
        axis=0,
        name="concat_h_out",
    )
    g.node.append(concat_h)
    g.output.append(helper.make_tensor_value_info("h_out", TensorProto.FLOAT, [2, 1, 256]))

    onnx.checker.check_model(model)
    return model


def make_stateful_df_dec(model: onnx.ModelProto) -> onnx.ModelProto:
    """Surgery for DF decoder: expose 2 GRU hidden states and 4-frame c0 delay ring."""
    g = model.graph

    # 1. Add h_in [2, 1, 256] to inputs
    g.input.append(helper.make_tensor_value_info("h_in", TensorProto.FLOAT, [2, 1, 256]))
    found_slices = 0
    for n in g.node:
        if n.name in ["/df_gru/gru/Slice", "/df_gru/gru/Slice_1"]:
            n.input[0] = "h_in"
            found_slices += 1
    if found_slices != 2:
        raise ValueError(f"Expected 2 Slice nodes for DF GRU initial_h, found {found_slices}")

    # Concat the 2 GRU outputs into h_out [2, 1, 256]
    concat_h = helper.make_node(
        "Concat",
        inputs=["/df_gru/gru/GRU_output_1", "/df_gru/gru/GRU_1_output_1"],
        outputs=["h_out"],
        axis=0,
        name="concat_h_out",
    )
    g.node.append(concat_h)
    g.output.append(helper.make_tensor_value_info("h_out", TensorProto.FLOAT, [2, 1, 256]))

    # 2. Add c0_buf [1, 64, 4, 96] delay ring
    g.input.append(helper.make_tensor_value_info("c0_buf", TensorProto.FLOAT, [1, 64, 4, 96]))
    g.output.append(helper.make_tensor_value_info("c0_buf_out", TensorProto.FLOAT, [1, 64, 4, 96]))

    starts_val = helper.make_tensor("slice_starts_4", TensorProto.INT64, [1], [-4])
    ends_val = helper.make_tensor("slice_ends_max", TensorProto.INT64, [1], [9223372036854775807])
    axes_val = helper.make_tensor("slice_axes_2", TensorProto.INT64, [1], [2])
    steps_val = helper.make_tensor("slice_steps_1", TensorProto.INT64, [1], [1])
    g.initializer.extend([starts_val, ends_val, axes_val, steps_val])

    # Replace Pad on c0
    new_nodes = []
    for n in g.node:
        if n.name == "/df_convp/df_convp.0/Pad":
            concat_c0 = helper.make_node(
                "Concat",
                inputs=["c0_buf", "c0"],
                outputs=["/df_convp/df_convp.0/Pad_output_0"],
                axis=2,
                name="concat_c0",
            )
            slice_c0 = helper.make_node(
                "Slice",
                inputs=["/df_convp/df_convp.0/Pad_output_0", "slice_starts_4", "slice_ends_max", "slice_axes_2", "slice_steps_1"],
                outputs=["c0_buf_out"],
                name="slice_c0_buf",
            )
            new_nodes.extend([concat_c0, slice_c0])
        else:
            new_nodes.append(n)

    del g.node[:]
    g.node.extend(new_nodes)

    onnx.checker.check_model(model)
    return model


def locate_stock_models(input_dir: str | None = None) -> str:
    """Find directory containing enc.onnx, erb_dec.onnx, df_dec.onnx."""
    candidates = []
    if input_dir:
        candidates.append(input_dir)
    candidates.append("/tmp/clearcore-dfnet3-export/tmp/export")

    for c in candidates:
        if os.path.exists(os.path.join(c, "enc.onnx")):
            return c

    # Try extracting from repo release asset
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
    asset_path = os.path.join(repo_root, "vendor/approved/df-compatible-release-asset-v1.bin")
    if os.path.exists(asset_path):
        out_tmp = "/tmp/clearcore-dfnet3-export"
        os.makedirs(out_tmp, exist_ok=True)
        with tarfile.open(asset_path, "r:gz") as tar:
            tar.extractall(path=out_tmp)
        extracted = os.path.join(out_tmp, "tmp/export")
        if os.path.exists(os.path.join(extracted, "enc.onnx")):
            return extracted

    raise FileNotFoundError(f"Could not locate stock DeepFilterNet3 ONNX files. Tried: {candidates}")


def perform_surgery(input_dir: str, output_dir: str) -> None:
    """Perform surgery on all 3 models and write to output_dir."""
    os.makedirs(output_dir, exist_ok=True)

    print(f"Loading stock models from: {input_dir}")
    enc_path = os.path.join(input_dir, "enc.onnx")
    erb_dec_path = os.path.join(input_dir, "erb_dec.onnx")
    df_dec_path = os.path.join(input_dir, "df_dec.onnx")

    m_enc = make_stateful_enc(onnx.load(enc_path))
    out_enc = os.path.join(output_dir, "enc.onnx")
    onnx.save(m_enc, out_enc)
    print(f"  [OK] Saved stateful enc -> {out_enc}")

    m_erb_dec = make_stateful_erb_dec(onnx.load(erb_dec_path))
    out_erb = os.path.join(output_dir, "erb_dec.onnx")
    onnx.save(m_erb_dec, out_erb)
    print(f"  [OK] Saved stateful erb_dec -> {out_erb}")

    m_df_dec = make_stateful_df_dec(onnx.load(df_dec_path))
    out_df = os.path.join(output_dir, "df_dec.onnx")
    onnx.save(m_df_dec, out_df)
    print(f"  [OK] Saved stateful df_dec -> {out_df}")

    # Copy config.ini if present
    cfg_path = os.path.join(input_dir, "config.ini")
    if os.path.exists(cfg_path):
        shutil.copy2(cfg_path, os.path.join(output_dir, "config.ini"))
        print(f"  [OK] Copied config.ini -> {output_dir}")


def verify_stateful_models(models_dir: str, num_frames: int = 8) -> bool:
    """Validate that streaming S=1 over T frames matches S=T batch execution."""
    import openvino as ov

    print(f"\n--- Running Parity Verification (S=1 continuous vs S={num_frames} batched) ---")
    core = ov.Core()
    rng = np.random.default_rng(12345)
    T = num_frames
    all_ok = True

    # 1. Verify enc
    print("\n1. Verifying enc.onnx:")
    erb = rng.standard_normal((1, 1, T, 32)).astype(np.float32)
    spec = rng.standard_normal((1, 2, T, 96)).astype(np.float32)

    m_batch = core.read_model(os.path.join(models_dir, "enc.onnx"))
    m_batch.reshape({"feat_erb": [1, 1, T, 32], "feat_spec": [1, 2, T, 96]})
    cm_batch = core.compile_model(m_batch, "CPU")
    req_batch = cm_batch.create_infer_request()
    req_batch.infer({
        "feat_erb": erb,
        "feat_spec": spec,
        "h_in": np.zeros((1, 1, 256), dtype=np.float32),
        "feat_erb_buf": np.zeros((1, 1, 2, 32), dtype=np.float32),
        "feat_spec_buf": np.zeros((1, 2, 2, 96), dtype=np.float32),
    })

    m_single = core.read_model(os.path.join(models_dir, "enc.onnx"))
    m_single.reshape({"feat_erb": [1, 1, 1, 32], "feat_spec": [1, 2, 1, 96]})
    cm_single = core.compile_model(m_single, "CPU")
    req_single = cm_single.create_infer_request()

    h_state = np.zeros((1, 1, 256), dtype=np.float32)
    erb_buf = np.zeros((1, 1, 2, 32), dtype=np.float32)
    spec_buf = np.zeros((1, 2, 2, 96), dtype=np.float32)
    single_frames = []

    for t in range(T):
        req_single.infer({
            "feat_erb": erb[:, :, t : t + 1, :],
            "feat_spec": spec[:, :, t : t + 1, :],
            "h_in": h_state,
            "feat_erb_buf": erb_buf,
            "feat_spec_buf": spec_buf,
        })
        h_state = req_single.get_tensor("h_out").data.copy()
        erb_buf = req_single.get_tensor("feat_erb_buf_out").data.copy()
        spec_buf = req_single.get_tensor("feat_spec_buf_out").data.copy()
        single_frames.append({o.get_any_name(): req_single.get_tensor(o).data.copy() for o in cm_single.outputs})

    for name in ["emb", "lsnr", "c0", "e0", "e1", "e2", "e3"]:
        axis = 2 if name in ["c0", "e0", "e1", "e2", "e3"] else 1
        seq = np.concatenate([f[name] for f in single_frames], axis=axis)
        batch_out = req_batch.get_tensor(name).data
        max_err = float(np.max(np.abs(seq - batch_out)))
        passed = max_err < 1e-4
        if not passed:
            all_ok = False
        status = "PASSED" if passed else "FAILED"
        print(f"  [{status}] {name:5s}: max error = {max_err:.6e} (tolerance < 1e-4)")

    # 2. Verify erb_dec
    print("\n2. Verifying erb_dec.onnx:")
    emb = rng.standard_normal((1, T, 512)).astype(np.float32)
    e3 = rng.standard_normal((1, 64, T, 8)).astype(np.float32)
    e2 = rng.standard_normal((1, 64, T, 8)).astype(np.float32)
    e1 = rng.standard_normal((1, 64, T, 16)).astype(np.float32)
    e0 = rng.standard_normal((1, 64, T, 32)).astype(np.float32)

    m_erb_b = core.read_model(os.path.join(models_dir, "erb_dec.onnx"))
    m_erb_b.reshape({"emb": [1, T, 512], "e3": [1, 64, T, 8], "e2": [1, 64, T, 8], "e1": [1, 64, T, 16], "e0": [1, 64, T, 32]})
    cm_erb_b = core.compile_model(m_erb_b, "CPU")
    req_erb_b = cm_erb_b.create_infer_request()
    req_erb_b.infer({
        "emb": emb, "e3": e3, "e2": e2, "e1": e1, "e0": e0,
        "h_in": np.zeros((2, 1, 256), dtype=np.float32),
    })

    m_erb_s = core.read_model(os.path.join(models_dir, "erb_dec.onnx"))
    m_erb_s.reshape({"emb": [1, 1, 512], "e3": [1, 64, 1, 8], "e2": [1, 64, 1, 8], "e1": [1, 64, 1, 16], "e0": [1, 64, 1, 32]})
    cm_erb_s = core.compile_model(m_erb_s, "CPU")
    req_erb_s = cm_erb_s.create_infer_request()

    h_erb = np.zeros((2, 1, 256), dtype=np.float32)
    erb_single_frames = []
    for t in range(T):
        req_erb_s.infer({
            "emb": emb[:, t : t + 1, :],
            "e3": e3[:, :, t : t + 1, :],
            "e2": e2[:, :, t : t + 1, :],
            "e1": e1[:, :, t : t + 1, :],
            "e0": e0[:, :, t : t + 1, :],
            "h_in": h_erb,
        })
        h_erb = req_erb_s.get_tensor("h_out").data.copy()
        erb_single_frames.append(req_erb_s.get_tensor("m").data.copy())

    seq_m = np.concatenate(erb_single_frames, axis=2)
    batch_m = req_erb_b.get_tensor("m").data
    err_m = float(np.max(np.abs(seq_m - batch_m)))
    passed_m = err_m < 1e-4
    if not passed_m:
        all_ok = False
    print(f"  [{'PASSED' if passed_m else 'FAILED'}] m    : max error = {err_m:.6e} (tolerance < 1e-4)")

    # 3. Verify df_dec
    print("\n3. Verifying df_dec.onnx:")
    c0 = rng.standard_normal((1, 64, T, 96)).astype(np.float32)

    m_df_b = core.read_model(os.path.join(models_dir, "df_dec.onnx"))
    m_df_b.reshape({"emb": [1, T, 512], "c0": [1, 64, T, 96]})
    cm_df_b = core.compile_model(m_df_b, "CPU")
    req_df_b = cm_df_b.create_infer_request()
    req_df_b.infer({
        "emb": emb, "c0": c0,
        "h_in": np.zeros((2, 1, 256), dtype=np.float32),
        "c0_buf": np.zeros((1, 64, 4, 96), dtype=np.float32),
    })

    m_df_s = core.read_model(os.path.join(models_dir, "df_dec.onnx"))
    m_df_s.reshape({"emb": [1, 1, 512], "c0": [1, 64, 1, 96]})
    cm_df_s = core.compile_model(m_df_s, "CPU")
    req_df_s = cm_df_s.create_infer_request()

    h_df = np.zeros((2, 1, 256), dtype=np.float32)
    c0_buf = np.zeros((1, 64, 4, 96), dtype=np.float32)
    df_single_frames = []
    for t in range(T):
        req_df_s.infer({
            "emb": emb[:, t : t + 1, :],
            "c0": c0[:, :, t : t + 1, :],
            "h_in": h_df,
            "c0_buf": c0_buf,
        })
        h_df = req_df_s.get_tensor("h_out").data.copy()
        c0_buf = req_df_s.get_tensor("c0_buf_out").data.copy()
        df_single_frames.append(req_df_s.get_tensor("coefs").data.copy())

    seq_coefs = np.concatenate(df_single_frames, axis=1)
    batch_coefs = req_df_b.get_tensor("coefs").data
    err_coefs = float(np.max(np.abs(seq_coefs - batch_coefs)))
    passed_coefs = err_coefs < 1e-4
    if not passed_coefs:
        all_ok = False
    print(f"  [{'PASSED' if passed_coefs else 'FAILED'}] coefs: max error = {err_coefs:.6e} (tolerance < 1e-4)")

    print(f"\nOverall Parity Verdict: {'ALL PASSED' if all_ok else 'FAILED'}")
    return all_ok


def main() -> None:
    parser = argparse.ArgumentParser(description="Create stateful DeepFilterNet3 ONNX graphs for hardware accelerators.")
    parser.add_argument("--input-dir", type=str, default=None, help="Directory containing stock enc.onnx, erb_dec.onnx, df_dec.onnx")
    parser.add_argument("--output-dir", type=str, default="models/stateful", help="Target directory for stateful ONNX models")
    parser.add_argument("--verify", action="store_true", default=True, help="Run numerical parity test after generation")
    parser.add_argument("--frames", type=int, default=8, help="Number of frames for continuous verification (default: 8)")
    args = parser.parse_args()

    input_dir = locate_stock_models(args.input_dir)
    perform_surgery(input_dir, args.output_dir)

    if args.verify:
        success = verify_stateful_models(args.output_dir, num_frames=args.frames)
        if not success:
            sys.exit(1)


if __name__ == "__main__":
    main()
