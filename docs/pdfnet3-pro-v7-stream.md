# SPEC — pDFNet3 Pro v7 (A2-300k) em streaming, com gate de silêncio C

Modelo `pdfnet3-pro-v7-a2-300k`. Checkpoint de origem `runs/v7-A2/snapshots/ckpt-00006249.pt` (branch
`train/pro-v7`; SHA-256 no `onnx/manifest.json`). Gerado por `scripts/export_v7.py` (código em
`src/cctrain/export/pdfnet3_v7.py`). Este documento basta para implementar o runtime em Rust; nenhum passo exige ler o
PyTorch.

O v7 roda **sempre com perfil**: é o modo isolamento (manter a voz do usuário cadastrado e cortar as outras). Sem perfil,
o app continua no caminho que já tem (DFNet3 base / Pro v3); este asset não tem `speaker_present`.

## 1. Arquivos

| Arquivo | Uso |
|---|---|
| `onnx/enroll_tokens.onnx` | 1× por perfil: features do cadastro → 48 tokens |
| `onnx/enc_conv.onnx` | todo quadro analisado: convs do encoder |
| `onnx/detector.onnx` | todo quadro analisado (1 quadro atrás): detector de casamento |
| `onnx/enc_post.onnx` | quadro atrasado 21: GRU do encoder, lsnr, cross-attention, FiLM do DF |
| `onnx/erb_dec.onnx`, `onnx/df_dec.onnx` | quadro atrasado, só se o estágio não for ZERO |
| `onnx/manifest.json` | formas, pares de estado, roteamento, SHA-256 de cada membro |
| `onnx/config.ini` | cópia byte a byte do `config.ini` do DFNet3 v0.5.6 aprovado (o mesmo do Pro v3) |
| `onnx/resampler-48k-16k-v1.coeffs.f32` | 241 coeficientes f32 LE do FIR 48→16 kHz das features de nível (§5) |
| `pdfnet3-pro-v7-a2-300k-stateful-asset-v1.tar.gz` | os membros acima num tar determinístico (formato do asset do Pro v3) |
| `vectors/<caso>/` | vetores de paridade para o teste em Rust (§10) |
| `parity.json`, `tract-replay-report.json` | paridade PyTorch × ORT e ORT × tract 0.19.16, custo |

Opset 12, float32, formas fixas, S = 1 quadro por chamada. Ops: Conv, ConvTranspose, GRU, Einsum, Gemm/MatMul,
Softmax, Erf, Tanh, Exp, Sigmoid, Relu, ReduceMean/ReduceMax/ReduceL2, Sqrt e ops de forma. Validado em ONNX Runtime
1.30 e tract 0.19.16 (`into_optimized`).

## 2. Quadro e DSP (igual ao runtime atual)

- 48 kHz mono, hop 480 amostras (10 ms), FFT 960, janela e síntese do `libdf`/`deep_filter` (DFNet3), 481 bins,
  32 bandas ERB (`erb_widths` do `DF(48000, 960, 480, 32, 2)`), DF nos 96 primeiros bins, ordem DF 5,
  lookahead de conv 2 e de DF 2 quadros.
- `feat_erb[t]` = `erb_norm(erb(spec), α = 0,99)` → `[1,1,1,32]`; `feat_spec[t]` = `unit_norm(spec[..96], α = 0,99)`,
  real e imaginário em canais → `[1,2,1,96]`. Exatamente o que o `DfTract` do app já calcula.
- **Atalho de silêncio digital** (como hoje): quadro de entrada com média quadrática < 1e-7 não é analisado (nenhum
  grafo roda, nenhum estado avança, saída zero). Tudo abaixo conta em **quadros analisados** `t = 0, 1, 2, …`
  (os não silenciosos, na ordem). As FIFOs, o nível (§5) e o gate (§7) só avançam em quadros analisados.

## 3. Entradas por perfil (uma vez)

- `speaker_embedding [1,192]`: o ECAPA-TDNN L2-normalizado do cadastro, o mesmo vetor do contrato do Pro v3
  (`enrollment.onnx` atual). Constante por sessão.
- `tokens [1,48,128]`: saída do `enroll_tokens.onnx` (§6). Pode ser guardada no perfil (determinística dado o
  áudio do cadastro e o embedding).

## 4. Ordem das chamadas por quadro analisado `t`

Estados: todo `<nome>_in` recebe o `<nome>_out` da chamada anterior **do mesmo grafo**; zeros no início da sessão.
FIFO `F` do runtime com as saídas do `enc_conv` (`x, e0, e1, e2, e3, c0`) dos últimos 22 quadros (`t-21 … t`).
Constantes: `D = 1` (atraso do detector), `L = 20` (lookahead do detector), `P = D + L = 21`.

```
para cada quadro analisado t:
  1. enc_conv(feat_erb[t], feat_spec[t], speaker_embedding)            -> F.push(x, e0, e1, e2, e3, c0)
  2. se t >= 1:                                                        # o quadro t-1 já tem as 120 amostras futuras
       lvl = nivel(t-1)                                                # §5
       detector(x_det = F[t-1].x, level = lvl, tokens, speaker_embedding) -> probs[t-1] [1,1,3], o[t-1] [1,1,19]
  3. se t >= P:   t' = t - P                                           # quadro de saída
       enc_post(x_post = F[t'].x, o = o[t'+L] (= o[t-1]), speaker_embedding)
              -> emb, emb_erb, erb_gain, lsnr, df_gamma, df_beta
       g = gateC.step(q = probs[t-1][1] + probs[t-1][2], lsnr)           # §7
       se lsnr < -10:  estágio ZERO: quadro de saída = 0; erb_dec e df_dec NÃO rodam (estados não avançam)
       senão:
         m     = erb_dec(emb_erb, F[t'].e3, F[t'].e2, F[t'].e1, F[t'].e0, erb_gain)   # [1,1,1,32]
         coefs = df_dec(emb, F[t'].c0, df_gamma, df_beta)                            # [1,1,96,10] = [96][5][re,im]
       aplicar (m, coefs) ao espectro do quadro t' exatamente como hoje (anéis do DfTract: máscara ERB no quadro t'-2,
       DF sobre os quadros t'-4 … t'), multiplicar o espectro de saída do quadro por g, sintetizar.
```

O espectro de análise precisa então ser atrasado `P = 21` quadros antes dos anéis atuais: guarde os espectros
(481 complexos) numa FIFO de 21 e alimente os anéis com o quadro `t'` quando ele for processado. Tudo o mais na
aplicação da máscara/DF/síntese é o código atual.

Partida: nos quadros `t < 21` só rodam os passos 1–2; a saída é silêncio. **Fim de stream / flush** (para não perder
os 210 ms finais): rodar mais 21 iterações com `feat_erb = feat_spec = 0` no `enc_conv` (só para empurrar a FIFO);
na primeira delas roda o detector do último quadro real (com o nível calculado com zeros depois do fim); nas demais o
detector não roda e `enc_post` usa o `o`/`probs` do último quadro real.

## 5. Features de nível (entrada `level [1,1,3]` do detector)

Sobre a mistura de entrada reamostrada a 16 kHz pelo FIR `resampler-48k-16k-v1` (241 taps, arquivo
`resampler-48k-16k-v1.coeffs.f32`, f32 LE), **não** o decimador do `resample.rs` do app:

```
y16[n] = Σ_{k=0}^{240} h[k] · x48[3n + 120 − k]      (x48 fora do sinal = 0; soma em f32)
```

A amostra 16 kHz `n` precisa de `x48` até `3n + 120`: o quadro analisado `t` (amostras 16 kHz `160t … 160t+159`)
fica completo quando chegam 120 amostras do quadro `t+1` — por isso o detector roda 1 quadro atrás (`D = 1`).
`x48` é a sequência dos quadros **analisados** concatenados.

Por quadro `t` (recorrência causal, estado `m`, `pk` em f64):

```
e  = f32( 10·log10( mean_{i<160}( y16[160t+i]² ) + 1e-12 ) )      # média em f64
w  = clamp((e + 75) / 45, 0, 1)                                     # f32
a  = 1 − exp(−0,01 / 2,0)            # τ = 2 s
dec = 3,0 · 0,01                     # pico decai 3 dB/s
se t == 0:  m = e;  pk = e
m  = m + a·w·(e − m)
pk = max(e, pk − dec)
level = [ (e − m)/20,  (e − pk)/20,  w ]                            # f32
```

## 6. Enrollment (1× por perfil)

1. Áudio do cadastro a 48 kHz: as falas do usuário recortadas, concatenadas com 250 ms de silêncio entre elas,
   truncado em 12 s (o mesmo áudio que já vai ao ECAPA). Se tiver menos de 64 quadros, completar com zeros até 64.
2. Features sobre o áudio inteiro, como em §2 mas **sem** o atalho de silêncio (um `DF` novo, todos os quadros):
   `feat_erb [N,32]`, `feat_spec [N,96,2]` (re, im). Arredondar para f16 e voltar a f32 (o treino guarda em f16).
3. Atividade por quadro: `lv[i] = 10·log10(mean(x48[480i … 480i+479]²) + 1e-20)` (f64);
   `act[i] = lv[i] ≥ max(max(lv) − 30, −75)`; se `max(lv) ≤ −75`, todos inativos.
4. Escolha dos 4 trechos de 64 quadros: `frac[s]` = fração de quadros ativos em `[s, s+64)` para
   `s = 0 … N−64`; `good` = os `s` com `frac ≥ 0,5` (crescente); se vazio, os 4 `s` de maior `frac` (desempate pelo
   menor `s`), ordenados. `starts = good[round(linspace(0, |good|−1, 4))]` (arredondamento meio-par, como `numpy.round`).
5. `enroll_tokens(enr_feat_erb [1,4,64,32], enr_feat_spec [1,4,64,96,2], enr_act [1,4,64] em {0,1},
   speaker_embedding)` → `tokens [1,48,128]`. O grafo já faz o `pad_feat` do treino (adianta 2 quadros dentro de cada
   trecho) e a máscara de atividade.

## 7. Gate de silêncio C (pós-processamento por quadro)

Parâmetros: piso −25 dB, τ = 0,5, hangover 200 ms, release 50 ms, condição de lsnr −3 dB. Estado `g = 1`, `hang = 0`
no início da sessão; avança uma vez por quadro de saída `t'` (inclusive nos de estágio ZERO).

```
floor = 10^(−25/20);  H = 20 quadros;  a = 1 − exp(−10/50)
fala  = (q ≥ 0,5) OU (lsnr ≥ −3)         # q = probs[1] + probs[2] = P(alvo) + P(outra voz), lsnr do enc_post
hang  = fala ? H : max(hang − 1, 0)
alvo  = (fala OU hang > 0) ? 1 : floor
g     = alvo > g ? alvo : g + a·(alvo − g) # sobe na hora, desce com um polo
saída do quadro t' ×= g
```

Só vira silêncio quando o detector **e** o lsnr concordam; o `q` já tem 200 ms de lookahead, então o começo da palavra
chega antes. Sem gate: `g ≡ 1`.

## 8. Latência

| | |
|---|---|
| base (STFT + lookahead do DFNet3, como hoje) | 1 440 amostras = 30 ms |
| lookahead do detector | 20 quadros = 200 ms |
| espera do nível (FIR 48→16 kHz) | 1 quadro = 10 ms |
| **total** | **240 ms** (11 520 amostras) |

Na comparação com os vetores: `expected_out_*.wav` é a saída da réplica do runtime atual (já com a latência base de
1 440 amostras); a saída do runtime v7 fica mais 21 quadros (10 080 amostras) atrás dela.

## 9. Custo medido (CPU, 1 thread, por segundo de áudio)

| | enc_conv | detector | enc_post | erb_dec | df_dec | total | enroll (1×) |
|---|---|---|---|---|---|---|---|
| tract 0.19.16 (`into_optimized`, máquina de desenvolvimento) | 13 ms | 6 ms | 30 ms | 28 ms | 20 ms | **≈ 96 ms** | 40 ms |
| ONNX Runtime 1.30 (rig, laço Python) | 44 ms | 26 ms | 59 ms | 31 ms | 29 ms | ≈ 188 ms | 4 ms |

Bem abaixo do tempo real (RTF ≈ 0,1–0,2). erb_dec/df_dec só rodam fora do estágio ZERO.

## 10. Vetores de paridade (`vectors/<caso>/`)

Falantes do Common Voice pt (CC0) e ruído rosa sintético; nenhum áudio do usuário. Casos: `alvo_interf_sir0_snr10`
(alvo + outra voz, SIR 0, SNR 10), `alvo_snr0` (alvo sozinho em ruído forte), `so_interf_snr20` (só a outra voz).
Cada caso (índice em `case.json`; `.f32` = float32 LE com a forma no índice):

- `input.wav` (48 kHz f32, 6 s, sem silêncio digital), `enroll.wav` (áudio do cadastro), `speaker_embedding.f32`
  (o embedding usado: o ECAPA do banco do treino; use este vetor no teste, não o recalculado),
- `enr_feat_erb.f32`, `enr_feat_spec.f32`, `enr_act.f32` (passo 6.2–6.4) e `tokens.f32` (saída esperada do grafo),
- `level.f32 [T,3]` (§5), `q_*.f32`, `lsnr_*.f32` por quadro de saída, `gate_gain.f32` (§7),
- `expected_out_nogate.wav`, `expected_out_gateC.wav`: saída esperada (alinhada como em §8).

Paridade medida (`parity.json`, `tract-replay-report.json`): PyTorch offline × ORT em streaming: lsnr ≤ 2e-5 dB,
máscara ≤ 2,1e-6, coefs ≤ 3,1e-6, q ≤ 5,4e-7, gate exato, áudio final SNR ≥ 126 dB (|erro| ≤ 2,7e-8);
ORT × tract por chamada: |erro| ≤ 6,2e-6. Meta sugerida para o teste em Rust: SNR ≥ 60 dB no áudio e
|erro| ≤ 1e-3 em lsnr/q/gate.

## 11. O que muda em relação ao contrato do Pro v3

- 6 grafos em vez de 3; `enc` dividido em `enc_conv` (quadro atual) e `enc_post` (quadro atrasado); novo `detector`
  e `enroll_tokens`.
- Sem `speaker_present` (perfil sempre ligado). Sem perfil: usar o caminho atual.
- `erb_dec` recebe `emb_erb` (não `emb`); `df_dec` recebe `df_gamma`/`df_beta` (FiLM já com os deltas do detector) em
  vez de `speaker_embedding`/`speaker_present`.
- Perfil: além do embedding, guardar as features dos 4 trechos (ou os tokens).
- Runtime: FIFO de 22 quadros das saídas do `enc_conv`, FIFO de 21 espectros antes dos anéis atuais, FIR 48→16 kHz
  v1 + recorrência de nível, gate C, flush de 21 quadros; latência +210 ms.
- O asset tem um membro `.f32` (coeficientes do FIR); o leitor de `stateful_asset.rs` hoje aceita só `.onnx`,
  `config.ini` e notas de texto.
