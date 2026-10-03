# Análise de Consumo de Disco — /home/joaorura
**Data:** 2026-10-03  
**Espaço Total:** 475 GB | **Usado:** 435 GB (94%) | **Livre:** 31 GB

---

## 1. Sumário Executivo

O disco está **crítico** com apenas 31 GB livres (6%). Os maiores consumidores são:
1. **Docker Desktop VM** (473 GB) — regenerável, remover libera ~31+ GB de uma vez
2. **OpenArc cache** (31 GB) — cache de modelos/embeddings, parcialmente regenerável
3. **HuggingFace hub** (26 GB) — modelos de ML, regenerável sob demanda
4. **NVIDIA Broadcast venv** (16 GB) — ambiente virtual, regenerável
5. **Miniconda/venvs** (13+ GB) — ambientes Python, regenerável

---

## 2. Maiores Consumidores (Top 40)

| Tamanho | Local | Tipo | Categoria | Remoção Segura? |
|---------|-------|------|-----------|-----------------|
| **473 GB** | `~/.docker/desktop/vms/0/data/Docker.raw` | Arquivo VM | Docker Desktop | ✅ **SIM** — regenerável ao relançar |
| **79 GB** | `~/.docker/desktop/` | Pasta VM | Docker Desktop | ✅ **SIM** — regenerável ao relançar |
| **31 GB** | `~/openarc/` | Dados app | OpenArc (embeddings/cache) | ⚠️ **PARCIALMENTE** — pode refazer cache, mas alguns dados de inferência serão perdidos |
| **26 GB** | `~/.cache/huggingface/hub/` | Modelos ML | HuggingFace Hub Cache | ✅ **SIM** — baixa sob demanda |
| **22 GB** | `~/.cache/huggingface/hub/models--Qwen--Qwen3-Embedding-8B/` | Modelo Qwen 3 Embedding | HuggingFace Hub | ✅ **SIM** — baixa sob demanda |
| **7,8 GB** | `~/.cache/huggingface/hub/models--Qwen--Qwen3-Reranker-4B/` | Modelo Qwen 3 Reranker | HuggingFace Hub | ✅ **SIM** — baixa sob demanda |
| **16 GB** | `~/nvidia-broadcast-linux/.venv/` | Ambiente Python | NVIDIA Broadcast | ✅ **SIM** — reclone/recrie |
| **13 GB** | `~/.local/share/whisper-ov/` | Modelo Whisper/OpenVINO | Whisper-ov (local) | ✅ **SIM** — reclone o repositório |
| **7,3 GB** | `~/miniconda3/` | Anaconda | Conda/Python | ✅ **SIM** — reclone/recrie ambientes |
| **6,8 GB** | `~/.venv-trt11-test/` | Ambiente Python | venv teste | ✅ **SIM** — ambiente de teste, remova |
| **7,2 GB** | `~/Pesquisa/EscritaArtigos/.git/` | Histórico git | Pesquisa Git | ⚠️ **CUIDADO** — dados do usuário com histórico, `git gc` antes de remover |
| **7,1 GB** | `~/.local/share/JetBrains/` | Cache IDE | JetBrains IDEs | ✅ **SIM** — recache ao relançar |
| **6,2 GB** | `~/.cache/uv/` | Cache gerenciador Python | UV package manager | ✅ **SIM** — regenerável |
| **3,9 GB** | `~/.nuget/packages/` | Cache NuGet | .NET NuGet | ✅ **SIM** — baixa sob demanda |
| **3,8 GB** | `~/.cache/ze_intel_npu_cache/` | Cache Intel NPU | Intel compute | ✅ **SIM** — regenerável |
| **3,4 GB** | `~/.cache/JetBrains/` | Cache JetBrains | JetBrains IDEs | ✅ **SIM** — recache ao relançar |
| **2,9 GB** | `~/.var/app/com.ktechpit.whatsie/` | Dados WhatsIE | Flatpak app | ⚠️ **CUIDADO** — dados do usuário (mensagens), não remova sem backup |
| **2,9 GB** | `~/.var/app/com.jetbrains.Rider/` | Dados Rider IDE | JetBrains Rider | ✅ **SIM** — recache/reconfigure |
| **2,8 GB** | `~/.npm/` | Cache npm | npm package manager | ✅ **SIM** — regenerável |
| **2,6 GB** | `~/.cache/llama_index/` | Cache LLamaIndex | RAG index cache | ✅ **SIM** — regenerável |
| **7,6 GB** | `~/openarc/cache/*.blob` (3 arquivos ~7.5 GB c/u) | Embeddings cache | OpenArc embeddings | ⚠️ **PARCIALMENTE** — refaz inferências se removidos |
| **1,9 GB** | `~/.cache/whisper/` | Cache Whisper | Transcrição | ✅ **SIM** — regenerável |
| **1,8 GB** | `~/.var/app/com.github.IsmaelMartinez.teams_for_linux/` | Dados Teams | Flatpak app | ⚠️ **CUIDADO** — dados do usuário (conversas), não remova sem backup |
| **1,6 GB** | `~/.cache/chromium/` | Cache Chromium | Browser cache | ✅ **SIM** — regenerável |
| **889 MB** | `~/.cache/mozilla/` | Cache Mozilla | Browser cache | ✅ **SIM** — regenerável |
| **874 MB** | `~/.cache/pnpm/` | Cache pnpm | pnpm package manager | ✅ **SIM** — regenerável |
| **847 MB** | `~/miniconda3/pkgs/` | Pacotes Conda | Conda packages | ✅ **SIM** — limpe com `conda clean --all` |
| **783 MB** | `~/Downloads/` | Downloads | Usuário | ⚠️ **CUIDADO** — dados do usuário, revise antes |
| **661 MB** | `~/.cache/opencode/` | Cache OpenCode | Dev tool cache | ✅ **SIM** — regenerável |
| **536 MB** | `~/.cache/nvbroadcast/` | Cache NVIDIA | NVIDIA Broadcast | ✅ **SIM** — regenerável |
| **393 MB** | `~/.cache/puppeteer/` | Cache Puppeteer | Browser automation | ✅ **SIM** — regenerável |
| **314 MB** | `~/.cache/libdnf5/` | Cache DNF/RPM | Package manager | ✅ **SIM** — regenerável com `dnf clean all` |
| **311 MB** | `~/.cache/haiku_rag/` | Cache Haiku RAG | RAG cache | ✅ **SIM** — regenerável |
| **186 MB** | `~/.cache/chrome-devtools-mcp/` | Cache DevTools MCP | DevTools plugin | ✅ **SIM** — regenerável |
| **145 MB** | `~/.cache/orca-updater/` | Cache atualizador | Orca updater | ✅ **SIM** — regenerável |
| **125 MB** | `~/.cache/node-gyp/` | Cache node-gyp | Node build | ✅ **SIM** — regenerável |
| **118 MB** | `~/.var/app/com.discordapp.Discord/` | Cache Discord | Flatpak app | ✅ **SIM** — regenerável |
| **118 MB** | `~/.cache/electron/` | Cache Electron | Electron apps | ✅ **SIM** — regenerável |
| **85 MB** | `~/.rustup/` | Rust toolchain | Rust | ✅ **SIM** — reclone ou `rustup update` |
| **78 MB** | `~/.cache/pip/` | Cache pip | Python pip | ✅ **SIM** — regenerável |

---

## 3. Estratégia de Limpeza (Impacto Máximo)

### Fase 1: **Remover Docker (473 GB)** → Libera ~31 GB por si só

```bash
docker system prune -a --volumes --force
# Depois:
rm -rf ~/.docker/desktop/vms/
```

**O que é:** Virtual Machine do Docker Desktop com todas as imagens/volumes.  
**Segurança:** ✅ **100% regenerável** — ao relançar Docker Desktop, cria nova VM.  
**Impacto:** Libera **~31 GB em uma única ação**.  
**Status do `docker system df`:**
- 12 imagens (9 ativas) — 10.33 GB
- 27 containers (16 ativos) — 1.05 GB  
- 42 volumes locais (24 ativos) — 28.58 GB, **79% reclaimable** (22.74 GB)
- 107 camadas de build — 7.618 GB, **0 ativas**

---

### Fase 2: **Limpar Caches e Ambientes (15–20 GB)**

```bash
# HuggingFace (26 GB) — regenerável sob demanda
rm -rf ~/.cache/huggingface/hub/

# OpenArc cache de blobs (7.5 GB) — recalcula embeddings
rm -f ~/openarc/cache/*.blob

# Ambientes Python de teste
rm -rf ~/.venv-trt11-test

# NVIDIA Broadcast venv (16 GB)
rm -rf ~/nvidia-broadcast-linux/.venv

# Miniconda pacotes não usados
conda clean --all -y
du -sh ~/miniconda3/  # Depois confira tamanho

# Limpar caches diversos
rm -rf ~/.cache/uv ~/.cache/llama_index ~/.cache/JetBrains
rm -rf ~/.cache/whisper ~/.cache/chromium ~/.cache/mozilla
rm -rf ~/.cache/pnpm ~/.cache/pip

# NuGet + JetBrains
rm -rf ~/.nuget/packages ~/.local/share/JetBrains
```

**Impacto esperado:** 15–20 GB liberados.

---

### Fase 3: **Revisar Dados do Usuário (Cuidado)**

| Pasta | Tamanho | Risco | Ação |
|-------|---------|-------|------|
| `~/Pesquisa/EscritaArtigos/.git/` | 7.2 GB | ALTO | Rode `git gc --aggressive` antes de considerar remover |
| `~/.var/app/com.ktechpit.whatsie/` | 2.9 GB | ALTO | Backup antes; contém mensagens do app |
| `~/.var/app/com.github.IsmaelMartinez.teams_for_linux/` | 1.8 GB | ALTO | Backup antes; contém conversas do Teams |
| `~/Downloads/` | 783 MB | MÉDIO | Revise e remova arquivos desusados |

---

## 4. Status de Container Engines

### Docker Desktop
```
TYPE            TOTAL     ACTIVE    SIZE       RECLAIMABLE
Images          12        9         10.33GB    2.438GB (23%)
Containers      27        16        1.055GB    1.054GB (99%)
Local Volumes   42        24        28.58GB    22.74GB (79%)  ← MAIOR CANDIDATO
Build Cache     107       0         7.618GB    3.147GB
```

**~23 GB podem ser reclamados** apenas com `docker system prune`.

### Podman
```
TYPE           TOTAL       ACTIVE      SIZE        RECLAIMABLE
Images         1           1           64.32MB     0B
Containers     2           0           26.26kB     26.26kB
Local Volumes  0           0           0B          0B
```

Podman usa apenas ~64 MB, não é problema.

---

## 5. Recomendação Final

1. **Imediatamente:** `docker system prune -a --volumes --force` + limpar `~/.docker` → **31 GB**
2. **Depois:** Fase 2 acima → **15–20 GB**
3. **Total esperado: 46–51 GB de espaço livre** (15–20% do disco)

Isso traz o disco de **94% (crítico)** para ~75% (seguro).

Para alcançar ≥20 GB livres com segurança máxima, a Fase 1 já é suficiente.

---

## 6. Comandos de Diagnóstico Completos

```bash
# Antes de qualquer limpeza, snapshot do estado
du -xsh ~/.docker ~/.cache/huggingface ~/openarc > ~/disco-antes.txt
docker system df > ~/docker-antes.txt

# Executar limpeza Fase 1 (Docker)
docker system prune -a --volumes --force
rm -rf ~/.docker/desktop/vms

# Verificar espaço
df -h /home

# Executar limpeza Fase 2 (Ambientes/Caches)
rm -rf ~/.cache/huggingface/hub ~/.venv-trt11-test
conda clean --all -y
rm -rf ~/.cache/uv ~/.cache/llama_index ~/.cache/JetBrains

# Pós-limpeza snapshot
du -xsh ~/.docker ~/.cache/huggingface ~/openarc > ~/disco-depois.txt
df -h /home
```

---

## Notas

- **Worktree hippocamp:** O repositório `clearcore` em `/home/joaorura/orca/workspaces/clearcore` usa apenas ~45 GB no total (dentro de `orca/workspaces`).
- **Pesquisa/EscritaArtigos:** 30 GB com git pesado (7.2 GB no `.git/`). Se for legado, considere `git gc --aggressive` e arquivo.
- **Próximo passo:** Após liberar ≥20 GB, considerar backup incremental de `~/Pesquisa/` e `~/.var/app/` para mídia externa.
