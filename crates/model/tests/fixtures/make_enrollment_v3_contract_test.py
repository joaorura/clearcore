"""Generates enrollment-v3-contract-test.onnx: a synthetic model with the v3 enrollment I/O.

Input `audio` f32 [1, N]; output `speaker_embedding` f32 [1, 192], L2-normalized. The embedding
is a fixed random projection of four audio statistics (mean, mean square, mean |x|, peak), so
different audio gives different embeddings. NOT a speaker model; contract tests only.

Run: python -I make_enrollment_v3_contract_test.py   (needs `onnx` and `numpy`)
"""
import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper

rng = np.random.default_rng(3)
w = rng.normal(size=(4, 192)).astype(np.float32)
b = rng.normal(scale=0.05, size=(1, 192)).astype(np.float32)

audio = helper.make_tensor_value_info("audio", TensorProto.FLOAT, [1, "N"])
out = helper.make_tensor_value_info("speaker_embedding", TensorProto.FLOAT, [1, 192])
inits = [numpy_helper.from_array(w, "w"), numpy_helper.from_array(b, "b"),
         numpy_helper.from_array(np.array([1], dtype=np.int64), "axes1"),
         numpy_helper.from_array(np.array([1e-9], dtype=np.float32), "eps")]
nodes = [
    helper.make_node("ReduceMean", ["audio"], ["mean"], axes=[1], keepdims=1),
    helper.make_node("Mul", ["audio", "audio"], ["sq"]),
    helper.make_node("ReduceMean", ["sq"], ["msq"], axes=[1], keepdims=1),
    helper.make_node("Abs", ["audio"], ["ab"]),
    helper.make_node("ReduceMean", ["ab"], ["mab"], axes=[1], keepdims=1),
    helper.make_node("ReduceMax", ["ab"], ["peak"], axes=[1], keepdims=1),
    helper.make_node("Concat", ["mean", "msq", "mab", "peak"], ["feat"], axis=1),
    helper.make_node("MatMul", ["feat", "w"], ["proj"]),
    helper.make_node("Add", ["proj", "b"], ["raw"]),
    helper.make_node("Mul", ["raw", "raw"], ["raw2"]),
    helper.make_node("ReduceSum", ["raw2"], ["ss"], keepdims=1),
    helper.make_node("Add", ["ss", "eps"], ["sse"]),
    helper.make_node("Sqrt", ["sse"], ["nrm"]),
    helper.make_node("Div", ["raw", "nrm"], ["speaker_embedding"]),
]
graph = helper.make_graph(nodes, "enrollment_v3_contract", [audio], [out], inits)
model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
model.ir_version = 8
onnx.checker.check_model(model)
onnx.save(model, "enrollment-v3-contract-test.onnx")
