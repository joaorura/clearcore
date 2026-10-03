# Levantamento de Ambiente: Viabilidade de Fine-Tune DeepFilterNet3

**Data:** 2026-10-03  
**Máquina:** Notebook pessoal (Fedora 44)  
**Objetivo:** Avaliar se esta máquina comporta fine-tune de DeepFilterNet3

---

## 1. Hardware: GPU

```
GPU Model: NVIDIA RTX PRO 1000 (Blackwell, Professional Card)
VRAM Total: 8151 MiB (8 GB)
VRAM Usado: 413 MiB (5% - kwin_wayland + Electron + Python broadcast)
Driver: 615.71.09 (KMD/CUDA UMD 13.4)
Limite de Potência: 50 W
Clock Max GPU: 3090 MHz (graphics), 2152 MHz (atualmente)
Clock Max Memory: 12001 MHz (9001 MHz atual)
```

**Avaliação GPU:**
- **8 GB VRAM é apertado** para fine-tune de DeepFilterNet3 (modelo ~100M params)
- Requer batch-size pequeno (1-4) e quantização FP16/INT8
- Card profissional (não é consumer), com estabilidade melhor que desktop, mas termal limit = 50W é baixo
- **Viável mas marginal** — treino possível, mas com ajustes agressivos

---

## 2. CPU & RAM

```
CPU: Intel Core(TM) Ultra 7 265H
Cores: 16 (1 thread por core, singlethreaded 5.3 GHz)
Suporte: AVX2, AVX, SSE4.2, RDRAND, SHA-NI, BMI2 (bom para NN)

RAM Total: 30 GB
RAM Usado: 26 GB (86.7%)
RAM Disponível: 4.1 GB (13.3%)

Swap Total: 47 GB (32 GB swapfile + 16 GB zram)
Swap Usado: 40 GB (85%)
Swap Disponível: 7.6 GB (15%)

Pico de RAM por processo:
  Rider IDE: 1.7 GB
  QtWebEngine (WhatsApp): 1.4 GB
  Docker Desktop QEMU: 1.2 GB
  Teams: 648 MB
  Chromium: 253 MB
```

**Avaliação RAM/Swap:**
- **CRÍTICA:** RAM está a 86%, swap a 85% — máquina já está perto do limite
- Docker Desktop (qemu-system-x86_64) consome 1.2 GB de RAM continuamente
- Qualquer treino vai forçar swap agressivamente (TensorFlow/PyTorch podem alocar 15-20GB com batches grandes)
- Zram (16GB) oferece compressão, mas com performance hit em CPU

---

## 3. Disco

```
Partição Principal (/dev/dm-0, LUKS, btrfs):
  Total: 475 GB
  Usado: 435 GB (91.6%)
  Disponível: 31 GB (8.4%)
  Status: ⚠️  CRÍTICA — próxima ao cheio

/tmp (tmpfs):
  Total: 16 GB
  Usado: 2.0 GB
  Disponível: 14 GB
  Status: OK para temporários durante treino

Maiores diretórios em ~:
  .docker/desktop:        79 GB (Docker Desktop VM image)
  orca (repositórios):    59 GB
  .cache (caches):        51 GB
    - .cache/huggingface: 26 GB (modelos baixados)
    - .cache/uv:           6.2 GB (Python wheels)
    - .cache/JetBrains:    3.4 GB
    - .cache/NPU:          3.8 GB
  openarc (projetos):     31 GB
  Pesquisa (trabalho):    30 GB
  .local (local binaries): 29 GB
  openarc/models:         7.1 GB (cache de modelos)
  miniconda3/pkgs:        847 MB
  .config/chromium:       2.9 GB

Docker/Podman:
  Docker images:          10.33 GB (2.44 GB reclaimável)
  Docker volumes:         28.58 GB (22.74 GB reclaimável)
  Docker build cache:     7.618 GB (3.147 GB reclaimável)
  Podman images:          64 MB (negligenciável)
```

**Avaliação Disco:**
- **CRÍTICA:** 94% de ocupação — não há margem
- Fine-tune DeepFilterNet3 vai precisar de:
  - Dataset: 20-50 GB
  - Checkpoints durante treino: 10-20 GB
  - Logs/tensorboard: 1-5 GB
  - **Total necessário: ~100 GB** — **não há espaço**

---

## 4. Python & Ambientes

```
Versões Instaladas:
  /usr/bin/python3 → python3.14 (cpython-3.14.7)
  /usr/bin/python3.13 (disponível)
  uv: python3.12 em ~/.local/share/uv/python/
  uv: python3.11 em ~/.local/share/uv/python/
  
miniconda3: 7.3 GB instalado
  - Nenhum ambiente conda ativo/nomeado
  - Base environment: python3.14
  - Pacotes: 847 MB em ~/miniconda3/pkgs

Caches Python:
  pip: 78 MB
  uv wheels: 6.2 GB
  huggingface: 26 GB (modelos)
```

**Avaliação Python:**
- ✓ Python 3.12/3.13/3.14 disponíveis
- ✓ uv está configurado e ativo
- ✓ Conda disponível (mas sem ambientes isolados)
- Possível criar env isolado com `uv venv` ou `python -m venv`

---

## 5. Rust & Build Tools

```
Status de Ferramentas:
  cargo:   NOT IN PATH (exit code 3)
  rustc:   NOT IN PATH (exit code 3)
  maturin: NOT IN PATH (exit code 3)

Diretórios Rust:
  ~/.cargo/bin:       vazio
  ~/.rustup/toolchains: vazio
```

**Avaliação Rust:**
- ⚠️ **RUST NÃO ESTÁ INSTALADO NO PATH DO SISTEMA**
- DeepFilterNet3 tem dataloader em Rust (recompilação via maturin)
- Precisa instalar Rust toolchain antes de qualquer build
- Rustup não está inicializado

---

## 6. Recursos Térmicos & Poder

```
Configuração da Máquina:
  Classe: Notebook pessoal (~50W TDP)
  Tipo: Laptop CPU + dGPU (RTX PRO 1000)
  Thermals: Passivo (sem info de dissipador ativo visível em nvidia-smi)

Observações:
  - RTX PRO 1000 está a 45°C em repouso (50W de potência disponível)
  - Desktop similar com CPU 16-core + GPU 8GB teria ~180W total
  - Risco de throttling térmico em execução prolongada (treino)
  - Climatização de laptop não é projetada para treino contínuo
```

**Avaliação Térmica:**
- ⚠️ Risco de throttling — TDP de 50W é limite para GPU apenas
- Fine-tune de horas vai aquecer o device
- Recomenda-se cooling externo ou parar em caso de thermal limit (nvidia-smi vai mostrar Thermal Throttle)

---

## Orçamento Realista para Fine-Tune

### Espaço em Disco
| Recurso                  | Tamanho    | Fonte    |
|--------------------------|-----------|----------|
| Dataset (speech, 16kHz)   | 20-50 GB  | Entrada  |
| Modelo base (DeepFilterNet3) | 100 MB | Download |
| Checkpoints (N épocas)   | 5-10 GB   | Treino   |
| Logs/tensorboard          | 1-2 GB    | Treino   |
| Cache Python (wheels)     | 2 GB      | Env setup |
| **Total Necessário**      | **~80-100 GB** | |
| **Disponível Agora**      | **31 GB** | ⚠️ INSUFICIENTE |

### Memória RAM/VRAM
| Fase          | Estimativa   | Observação |
|---------------|-------------|-----------|
| Modelo + batch size 2 | ~4 GB VRAM | Apenas inferência |
| Treino BF16/FP16, batch 2 | ~6 GB VRAM | Backprop + optimizer state |
| RAM auxiliar (dataloader + OS) | ~10 GB | Swap vira obrigatório |
| **Peak observado**     | **~16-20 GB** | Com swap ativo |
| **Disponível agora**   | **4.1 GB + 7.6 GB swap** | Marginal |

### Tempo Estimado
- Dataset load + preprocessing: 2-5 min
- Epoch (batch 2, 10k samples): 30-60 min
- N epochs (3-5 típico): 2-5 horas
- **Total: 3-7 horas contínuas**, com throttling provável após hora 1-2

---

## Caches Limpáveis (sem prejudicar development)

### Altamente Recomendado Remover
| Cache                         | Tamanho | Impacto de remover |
|-------------------------------|--------|-------------------|
| Docker Desktop volume cache   | 22.74 GB | Dev Docker para + leve |
| Docker build cache            | 3.147 GB | Rebuilds mais lentos |
| Docker images (unused)        | 2.438 GB | Negligenciável se rebuild |
| **Subtotal liberável**        | **~28 GB** | Imediato + permanente |

### Recomendado Considerar
| Cache                         | Tamanho | Impacto de remover |
|-------------------------------|--------|-------------------|
| .cache/huggingface (modelos)  | 26 GB   | Redownload se usado |
| openarc/models                | 7.1 GB  | Redownload se research |
| .cache/ze_intel_npu_cache     | 3.8 GB  | NPU reinitializes (sec) |
| .cache/JetBrains (IDE)        | 3.4 GB  | IDE reindex (min) |
| .cache/uv (wheels)            | 6.2 GB  | pip download (depende dataset) |
| **Subtotal limpar parcial**   | **~30+ GB** | Depende de workflow |

### Não Remover
- ~/.local (binários, JetBrains toolbox, etc)
- ~/orca (repositórios em desenvolvimento)
- ~/Pesquisa, ~/Desenvolvimento (work in progress)

---

## Riscos & Mitigação

| Risco                           | Probabilidade | Severidade | Mitigação |
|---------------------------------|---------------|------------|-----------|
| **OOM (Out of Memory)**         | ALTA          | CRÍTICA    | Batch size 1, FP16, reduzir épocas |
| **Swap thrashing**              | ALTA          | ALTO       | Monitorar `vmstat 1`, limpar .docker |
| **Disco cheio durante treino**  | MÉDIA         | CRÍTICA    | Liberar 50+ GB antes de iniciar |
| **Thermal throttling GPU**      | MÉDIA         | MÉDIO      | External cooler, room temp <20°C |
| **Rust toolchain não existe**   | CERTA         | BLOQUEADOR | Instalar rustup antes de build |
| **Modelo não cabe em VRAM**     | BAIXA         | MÉDIO      | Usar quantização INT8 ou FP8 |
| **Notebook desligua por power** | BAIXA         | CRÍTICA    | Manter plugged, limpar ventilador |

---

## Conclusão & Recomendações

### ✓ Viável?
**SIM, mas com restrições severas.** Fine-tune é possível, mas:
1. **Liberar 50+ GB em disco** (remover Docker é mandatório)
2. **Instalar Rust toolchain** (maturin para dataloader)
3. **Usar batch-size 1-2** e FP16
4. **Aceitar 5-10 horas de treino** com pausas/checkpoints
5. **Monitorar térmico** continuamente

### ⚠️ O Que Mudar Antes

#### Imediato (obrigatório)
- [ ] **Remover Docker Desktop volumes: `docker system prune -a --volumes`** (22.74 GB)
- [ ] **Instalar Rust:** `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [ ] **Limpar hf models** caso não sejam críticos: `rm -rf ~/.cache/huggingface` (26 GB)

#### Antes de Iniciar Treino
- [ ] Parar Docker: `systemctl --user stop docker.service`
- [ ] Fechar IDEs: Rider (1.7 GB), VSCode, etc
- [ ] Parar navegadores: Chromium, Teams, WhatsApp
- [ ] Executar `uv pip list` para confirmar env limpo
- [ ] Verificar `nvidia-smi` — garantir VRAM < 500 MB

#### Durante Treino
- [ ] Monitorar: `nvidia-smi -l 1` (a cada 1 seg)
- [ ] Monitorar swap: `vmstat 1 | grep si/so` (< 1000 indica OOM iminente)
- [ ] Salvar checkpoints a cada N batches
- [ ] Se GPU throttle: permitir descanso de 10 min a cada hora

#### Depois (liberar recursos)
- [ ] Executar `docker system df` — verificar economia
- [ ] Executar `ncdu ~` — revisar o que cresceu (logs/checkpoints)

### 📊 Resumo Executivo

| Métrica            | Valor        | Viável? |
|--------------------|-------------|---------|
| **VRAM disponível** | 8 GB        | ✓ Sim (apertado) |
| **RAM disponível**  | 4.1 GB      | ⚠️ Sim (com swap) |
| **Disco disponível**| 31 GB       | ✗ **NÃO — precisa 80-100 GB** |
| **CPU cores**       | 16          | ✓ Sim |
| **Rust/Build tools**| Não instalado | ✗ **NÃO — precisa instalar** |
| **Thermals**        | 50W limit   | ⚠️ Sim (com risco) |
| **Overall**        | **CONDICIONAL** | **Após liberar disco + instalar Rust** |

---

## Referências

- DeepFilterNet3: https://github.com/Rikorose/DeepFilterNet
- nvidia-smi output: 2026-10-03 00:12:32
- Máquina: Intel Core Ultra 7 265H + RTX PRO 1000 (Blackwell)
- Sistema: Fedora 44, Kernel 7.2.8-200.fc44.x86_64
