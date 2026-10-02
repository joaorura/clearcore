#!/usr/bin/env python3
"""Gera o ONNX SINTETICO usado pelos testes do pipeline de enrollment.

Nao e um modelo de produto: nao extrai embedding de locutor nenhum. Ele so cumpre o CONTRATO de
E/S que o `SpeakerEnrollmentEngine` exige do asset `voice-enrollment-asset-v1` (os pesos reais vem
do repositorio de treino, ainda indisponivel):

  entrada   audio      float32 [1, N]   16 kHz mono
  saidas    gamma_enc, beta_enc, gamma_df, beta_df   float32 [256] cada

Os valores dependem do audio (media e media do modulo) para os testes provarem que a saida vem da
inferencia. Uso: gen_enrollment_test_onnx.py <arquivo de saida .onnx>
"""
import sys

import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper

HIDDEN = 256


def build() -> onnx.ModelProto:
    def const(name: str, value: float):
        return numpy_helper.from_array(np.full((HIDDEN,), value, "f4"), name)

    nodes = [
        helper.make_node("ReduceMean", ["audio"], ["mean"], axes=[1], keepdims=0),
        helper.make_node("Abs", ["audio"], ["abs_audio"]),
        helper.make_node("ReduceMean", ["abs_audio"], ["abs_mean"], axes=[1], keepdims=0),
        helper.make_node("Tanh", ["mean"], ["t_mean"]),
        helper.make_node("Tanh", ["abs_mean"], ["t_abs"]),
        helper.make_node("Mul", ["t_mean", "c_gain"], ["g_enc_delta"]),
        helper.make_node("Add", ["one", "g_enc_delta"], ["gamma_enc"]),
        helper.make_node("Mul", ["t_mean", "c_bias"], ["beta_enc"]),
        helper.make_node("Mul", ["t_abs", "c_gain"], ["g_df_delta"]),
        helper.make_node("Add", ["one", "g_df_delta"], ["gamma_df"]),
        helper.make_node("Mul", ["t_abs", "c_bias"], ["beta_df"]),
    ]
    graph = helper.make_graph(
        nodes,
        "enrollment_contract_test",
        [helper.make_tensor_value_info("audio", TensorProto.FLOAT, [1, "N"])],
        [helper.make_tensor_value_info(n, TensorProto.FLOAT, [HIDDEN])
         for n in ("gamma_enc", "beta_enc", "gamma_df", "beta_df")],
        initializer=[const("one", 1.0), const("c_gain", 0.5), const("c_bias", 0.05)],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 8
    onnx.checker.check_model(model)
    return model


if __name__ == "__main__":
    onnx.save(build(), sys.argv[1])
    print("ok", sys.argv[1])
