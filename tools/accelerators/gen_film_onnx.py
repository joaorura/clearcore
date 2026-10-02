#!/usr/bin/env python3
"""Gera os ONNX do DFNet3 com FiLM a partir do asset aprovado (tarefa T3 do spike).

Uso: gen_film_onnx.py <dir com enc.onnx/erb_dec.onnx/df_dec.onnx/config.ini> <dir de saida>

Insere x' = x * gamma + beta na saida do Relu que alimenta Transpose->GRU de enc e df_dec.
Saidas em <dir de saida>:
  enc_film.onnx, df_dec_film.onnx           gamma/beta como ENTRADAS extras [1, S, 256]
  film_identity_asset.tar.gz                 tar.gz com os dois acima + erb_dec.onnx + config.ini
  enc_film_baked.onnx, df_dec_film_baked.onnx  gamma=1.5, beta=0.1 como INITIALIZERS (sem entradas extras)
  film_baked_asset.tar.gz                    tar.gz equivalente com os baked
"""
import gzip, io, os, sys, tarfile
import numpy as np
import onnx
import onnxruntime as ort
from onnx import TensorProto, helper, numpy_helper

SITES = {
    "enc": "/emb_gru/linear_in/1/Relu_output_0",
    "df_dec": "/df_gru/linear_in/linear_in.1/Relu_output_0",
}
HIDDEN = 256
BAKED_GAMMA, BAKED_BETA = 1.5, 0.1


def add_film(model: onnx.ModelProto, site: str) -> onnx.ModelProto:
    g = model.graph
    if any(i.name in ("gamma", "beta") for i in g.input):
        raise SystemExit("modelo ja tem gamma/beta")
    producers = [i for i, nd in enumerate(g.node) if site in nd.output]
    if len(producers) != 1:
        raise SystemExit(f"site {site}: {len(producers)} produtores (esperado 1)")
    for name in ("gamma", "beta"):
        g.input.append(helper.make_tensor_value_info(name, TensorProto.FLOAT, [1, "S", HIDDEN]))
    scaled, filmed = site + "_gamma", site + "_film"
    for nd in g.node:
        for k, x in enumerate(nd.input):
            if x == site:
                nd.input[k] = filmed
    at = producers[0] + 1
    g.node.insert(at, helper.make_node("Mul", [site, "gamma"], [scaled], name="film_mul"))
    g.node.insert(at + 1, helper.make_node("Add", [scaled, "beta"], [filmed], name="film_add"))
    onnx.checker.check_model(model)
    return model


def bake(model: onnx.ModelProto, gamma: float, beta: float) -> onnx.ModelProto:
    """Troca as entradas gamma/beta por initializers constantes [1, 1, 256]."""
    g = model.graph
    keep = [i for i in g.input if i.name not in ("gamma", "beta")]
    del g.input[:]
    g.input.extend(keep)
    g.initializer.append(numpy_helper.from_array(np.full((1, 1, HIDDEN), gamma, "f4"), "gamma"))
    g.initializer.append(numpy_helper.from_array(np.full((1, 1, HIDDEN), beta, "f4"), "beta"))
    onnx.checker.check_model(model)
    return model


def feeds(name: str, steps: int, seed: int) -> dict:
    r = np.random.RandomState(seed)
    if name == "enc":
        return {"feat_erb": r.randn(1, 1, steps, 32).astype("f4"),
                "feat_spec": r.randn(1, 2, steps, 96).astype("f4")}
    return {"emb": r.randn(1, steps, 512).astype("f4"),
            "c0": r.randn(1, 64, steps, 96).astype("f4")}


def run(model_bytes: bytes, f: dict):
    return ort.InferenceSession(model_bytes, providers=["CPUExecutionProvider"]).run(None, f)


def same(xs, ys) -> bool:
    return all(np.array_equal(a, b) for a, b in zip(xs, ys))


def pack(path: str, enc: str, erb_dec: str, df_dec: str, config: str) -> None:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        for member, src in (("enc.onnx", enc), ("erb_dec.onnx", erb_dec),
                            ("df_dec.onnx", df_dec), ("config.ini", config)):
            data = open(src, "rb").read()
            info = tarfile.TarInfo(f"tmp/export/{member}")
            info.size, info.mtime, info.mode = len(data), 0, 0o644
            tar.addfile(info, io.BytesIO(data))
    with gzip.GzipFile(path, "wb", mtime=0) as gz:
        gz.write(buf.getvalue())


def main(src: str, dst: str) -> None:
    os.makedirs(dst, exist_ok=True)
    steps = 8
    for name, site in SITES.items():
        orig = open(f"{src}/{name}.onnx", "rb").read()
        film = add_film(onnx.load_from_string(orig), site)
        film_bytes = film.SerializeToString()
        open(f"{dst}/{name}_film.onnx", "wb").write(film_bytes)
        baked = bake(onnx.load_from_string(film_bytes), BAKED_GAMMA, BAKED_BETA)
        baked_bytes = baked.SerializeToString()
        open(f"{dst}/{name}_film_baked.onnx", "wb").write(baked_bytes)
        for seed in range(5):
            f = feeds(name, steps, seed)
            base = run(orig, f)
            ident = run(film_bytes, {**f, "gamma": np.ones((1, steps, HIDDEN), "f4"),
                                     "beta": np.zeros((1, steps, HIDDEN), "f4")})
            diff = max(float(np.abs(a - b).max()) for a, b in zip(base, ident))
            exact = same(base, ident)
            print(f"{name} seed={seed} identidade max_abs_diff={diff!r} bit_exato={exact}")
            if not exact:
                raise SystemExit(f"{name}: FiLM em identidade NAO e bit-exato no ORT")
        f = feeds(name, steps, 99)
        base = run(orig, f)
        moved = run(film_bytes, {**f, "gamma": np.full((1, steps, HIDDEN), BAKED_GAMMA, "f4"),
                                 "beta": np.full((1, steps, HIDDEN), BAKED_BETA, "f4")})
        changed = not same(base, moved)
        print(f"{name} gamma={BAKED_GAMMA} beta={BAKED_BETA} altera a saida: {changed}")
        if not changed:
            raise SystemExit(f"{name}: FiLM nao-identidade nao altera a saida (grafo nao esta ligado)")
        baked_out = run(baked_bytes, f)
        eq = same(moved, baked_out)
        print(f"{name} baked == entradas com mesmos valores: {eq}")
        if not eq:
            raise SystemExit(f"{name}: versao baked difere da versao com entradas")
    pack(f"{dst}/film_identity_asset.tar.gz", f"{dst}/enc_film.onnx", f"{src}/erb_dec.onnx",
         f"{dst}/df_dec_film.onnx", f"{src}/config.ini")
    pack(f"{dst}/film_baked_asset.tar.gz", f"{dst}/enc_film_baked.onnx", f"{src}/erb_dec.onnx",
         f"{dst}/df_dec_film_baked.onnx", f"{src}/config.ini")
    print("ok", f"{dst}/film_identity_asset.tar.gz", f"{dst}/film_baked_asset.tar.gz")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
