# Orca / Clearcore - Realtime Noise Suppression

## Overview
First-party realtime noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, PipeWire C bridge (Linux), WaveRT PortCls driver (Windows), CoreAudio HAL (macOS), and Rust supervisor daemon.

## Package Managers & Distribution
- **Fedora / RHEL (DNF via Copr):**
  `sudo dnf copr enable joaorura/clearcore && sudo dnf install -y clearcore`
- **Windows (WinGet):**
  `winget install joaorura.Clearcore`
- **Ubuntu / Debian (APT):**
  - *Method 1 (Official One-Liner - Recommended):*  
    `curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.1/Clearcore-0.1.0-beta.1_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb`
  - *Method 2 (Modular GitHub Pages APT - Solution A):*  
    `echo "deb [trusted=yes] https://joaorura.github.io/clearcore/apt stable main" | sudo tee /etc/apt/sources.list.d/clearcore.list && sudo apt update && sudo apt install -y clearcore`

## Standalone Application & Packaging (Turnkey Distribution)

The desktop application is completely self-contained. When launched, the application automatically starts its companion daemon (`realtime-noise-service`) in the background if not already running, verifies/creates the virtual microphone, and minimizes to the system tray. Exiting via the tray cleanly stops the daemon.

### Build & Package Standalone Installers
- **Build & Package Everything (Linux / macOS):** `./package.sh`
- **Build & Package Everything (Windows):** `.\package.bat` ou `.\package.ps1`
- **Output:**
  - Linux: `release/Clearcore-linux-x64/` (run `./clearcore` directly or `./install.sh` for system menu integration) + `Clearcore-linux-x64.tar.gz`
  - Windows: `release/Clearcore-win32-x64/` (portable `Clearcore.exe`) + NSIS (`Clearcore-Setup.nsi`) and Inno Setup (`Clearcore-Setup.iss`) to produce single-click `Clearcore-Setup.exe` that installs the app and WaveRT audio driver automatically.
  - macOS: `release/Clearcore-darwin-x64/` (`Clearcore.app` with embedded CoreAudio HAL driver bundle) + `install.sh` + `.tar.gz`

## Quick Commands

### Development Mode (Vite HMR + Electron + Sidecar Daemon)
To run the full development environment with live React hot reloading and automatic backend daemon:
- **Linux & macOS:** `./dev.sh` (or `cd crates/app-tauri && npm run dev`)
- **Windows:** `.\dev.bat` ou `.\dev.ps1` (or `cd crates/app-tauri && npm run dev`)
- **React Frontend Only (Browser):** `cd crates/app-tauri && npm run dev:ui`

`./dev.sh` builds the daemon in **release** (incremental; the first build takes longer) because the
denoiser does not keep up with real-time audio in a debug build, and the Electron app starts it with
core dumps disabled.

**Voice enrollment model (development only).** Generating a voice profile needs the M3 development
asset. `dev.sh` passes these variables on to the daemon (started by Electron); without them,
"Gerar perfil" ends in `ENROLL_MODEL_NOT_CONFIGURED` (samples are still recorded and denoised):

| Variable | Value |
| :--- | :--- |
| `CLEARCORE_DEV_ENROLLMENT_ASSET` | Absolute path of `voice-enrollment-asset-v1.tar.gz`. |
| `CLEARCORE_DEV_ENROLLMENT_SHA256` | SHA-256 of the whole file (64 lowercase hex). |

Example only (the path is this machine's `clearcore-train` M3 run; use your own copy):

```bash
CLEARCORE_DEV_ENROLLMENT_ASSET=/home/joaorura/orca/projects/clearcore-train/runs/m3/voice-enrollment-asset-v1.tar.gz \
CLEARCORE_DEV_ENROLLMENT_SHA256=bd4d6dd941f8527b5011a2bae78169148f33155e25d30c5707e88963e7ea824d \
./dev.sh
```

The asset is never copied to `vendor/approved/`, pinned or distributed (see `docs/ipc-v1.md` §3.1.7).

**Development isolation model (pDFNet3 with FiLM).** The approved base DFNet3 cannot apply a voice
profile (no FiLM inputs), and on machines with an Intel NPU `auto` picks `openvino-npu`, which never
applies one either. To test the profile end to end, give the daemon the pDFNet3 produced by
`clearcore-train` (M3). `dev.sh` passes these on too; without them the daemon uses the base model and
"Gerar perfil" is refused with `ENROLL_BACKEND_UNSUPPORTED`:

| Variable | Value |
| :--- | :--- |
| `CLEARCORE_DEV_PDFNET3_ASSET` | Absolute path of `pdfnet3-release-asset-v1.tar.gz`. |
| `CLEARCORE_DEV_PDFNET3_SHA256` | SHA-256 of the whole file (64 lowercase hex). Both or neither. |

With both set, the daemon forces the `tract` backend with this model for every selection
(`GetStatus`: `dev_base_model: "pdfnet3-dev"`, `voice_profile_supported: true`). If the archive is
missing or does not match, it keeps the base model and reports a fixed code in
`dev_base_model_error`. This is the M2 NO-GO checkpoint (no proven isolation benefit): development
only, never approved, never copied to `vendor/approved/`. Example only:

```bash
CLEARCORE_DEV_ENROLLMENT_ASSET=/home/joaorura/orca/projects/clearcore-train/runs/m3/voice-enrollment-asset-v1.tar.gz \
CLEARCORE_DEV_ENROLLMENT_SHA256=bd4d6dd941f8527b5011a2bae78169148f33155e25d30c5707e88963e7ea824d \
CLEARCORE_DEV_PDFNET3_ASSET=/home/joaorura/orca/projects/clearcore-train/runs/m3/pdfnet3-release-asset-v1.tar.gz \
CLEARCORE_DEV_PDFNET3_SHA256=42dfc577fdf8a881ecbafce7777bf6f0a4cf914ffc1aaff2580aec0cbac79505 \
./dev.sh
```

### Standalone Desktop App Execution (Production Package)
- **Run Standalone App (Linux):** `./release/Clearcore-linux-x64/clearcore`
- **Run Standalone App (Windows):** `.\release\Clearcore-win32-x64\Clearcore.exe`
- **Run Standalone App (macOS):** `open ./release/Clearcore-darwin-x64/Clearcore.app`

### Developer Convenience & Lifecycle

#### Linux & macOS
- **Start Everything (Daemon + Electron Tray):** `./start-all.sh`
- **Stop Everything:** `./stop-all.sh`
- **Check Virtual Mic:** `./scripts/check-virtual-mic.sh [--status|--json|--recreate|--set-default]`
- **Configure Autostart on Boot (Tray):** `./scripts/setup-autostart.sh [enable|disable|status]`

#### Windows (CMD & PowerShell)
- **Start Everything (Daemon + Electron Tray):** `start-all.bat` ou `.\start-all.ps1`
- **Stop Everything:** `stop-all.bat` ou `.\stop-all.ps1`
- **Check Virtual Mic:** `powershell .\scripts\check-virtual-mic-windows.ps1 [-Status|-Json|-Recreate|-SetDefault]`
- **Configure Autostart on Boot (Tray):** `.\scripts\setup-autostart.bat [enable|disable|status]` ou `powershell .\scripts\setup-autostart.ps1 -Action [enable|disable|status]`

#### IPC Status & Modes (Cross-Platform)
- **Check Status (IPC):** `cargo run --release -p realtime-noise-app-tauri -- --status`
- **Active Mode (DeepFilterNet):** `cargo run --release -p realtime-noise-app-tauri -- --mode active`
- **Bypass Mode:** `cargo run --release -p realtime-noise-app-tauri -- --mode bypass`
- **Mute Mode:** `cargo run --release -p realtime-noise-app-tauri -- --mode mute`

### Build & Test
- **Package Desktop Application:** `./package.sh`
- **Build PipeWire Helper (Linux):** `ninja -C platform/linux/helper/build`
- **Build Rust Service:** `cargo build --release -p realtime-noise-service`
- **Build All:** `cargo build --release`
- **Run Offline Gate:** `./scripts/check-offline.sh`

## Orca IDE Integration
Tasks are configured in `.vscode/tasks.json` and can be triggered directly via `Ctrl+Shift+B` or Command Palette (`Tasks: Run Task`).
Debug and Launch profiles are located in `.vscode/launch.json`.

---

## MCP Tools & Knowledge Graph Integration (Codegraph, Serena, Enquire)

> [!CAUTION]
> **OBRIGATÓRIO: Pesquisa Semântica Antes de Leitura (`Codegraph` / `Serena`)**
> É expressamente proibido fazer leituras sequenciais cegas (`view_file` repetido de arquivos inteiros) para "procurar" código.
> Agentes DEVEM usar ferramentas semânticas e AST primeiro:
> 1. Use `codegraph:codegraph_explore` para traçar fluxo de execução e entender impacto antes de tocar em código.
> 2. Use `serena:find_symbol` ou `serena:get_symbols_overview` para localizar o símbolo e obter o corpo exato e números de linha.
> 3. Use `serena:find_referencing_symbols` para analisar call sites.
> Apenas após localizar cirurgicamente o ponto de edição deve ser feita a leitura/alteração pontual.

This workspace integrates three Model Context Protocol (MCP) servers designed for deep semantic code navigation, AST-aware refactoring, and structured knowledge management: **Codegraph**, **Serena**, and **Enquire**.

### Tool Selection & Decision Matrix

Always prefer the highest-leverage semantic tool over generic text-scanning loops:

| Objective / Task | Recommended Tool | Fallback | Prohibited Anti-Pattern |
| :--- | :--- | :--- | :--- |
| **Trace execution / call paths across codebase** | `codegraph:codegraph_explore` | `serena:find_referencing_symbols` | Recursive grep loops and sequential file reads |
| **Inspect blast radius & code before refactoring** | `codegraph:codegraph_explore` | `view_file` | Ingesting entire files without checking dependents |
| **Inspect symbol hierarchy of a single file** | `serena:get_symbols_overview` | `view_file` (lines 1–100) | Ingesting 1000+ line files whole |
| **Inspect specific function / struct / method body** | `serena:find_symbol` (`include_body=True`) | `codegraph:codegraph_explore` | Grepping identifier and calculating line offsets manually |
| **Trace references and call sites of a symbol** | `serena:find_referencing_symbols` | `codegraph:codegraph_explore` | Running repo-wide `grep -rn "symbol"` |
| **Find trait or interface implementations** | `serena:find_implementations` | `codegraph:codegraph_explore` | Manual directory search / ripgrep |
| **Check compilation / LSP diagnostics** | `serena:get_diagnostics_for_file` | `run_command` (cargo check) | Blindly applying edits without diagnostic confirmation |
| **Atomic symbol rename across entire workspace** | `serena:rename_symbol` | Multi-file text edits | Ad-hoc regex scripts or sed |
| **Replace method / function / class body** | `serena:replace_symbol_body` | `replace_file_content` | Whole-file rewrites or manual slicing |
| **Insert types/functions at start/end of file** | `serena:insert_before_symbol` / `insert_after_symbol` | `replace_file_content` | Manually calculating EOF offsets |
| **Multi-file pattern replacement with dry-run** | `serena:replace_in_files` | Manual edits | Sequential shell scripts or untested mass replaces |
| **Persist architectural decisions and conventions** | `serena:write_memory` / `read_memory` | Ephemeral scratchpad | Writing conventions to temporary scratch notes |
| **Search Obsidian notes / architecture docs** | `enquire:obsidian_search` / `obsidian_hyde_search` | Ripgrep on vault | Reading raw markdown files directly from vault |
| **Build LLM context pack from Obsidian vault** | `enquire:obsidian_context_pack` | `enquire:obsidian_read_note` | Manual copy-pasting of notes and backlinks |
| **Structured metadata query in Obsidian vault** | `enquire:obsidian_dataview_query` | `enquire:obsidian_frontmatter_search` | Custom YAML parsers with python/bash |
| **Traverse links / graph path in Obsidian vault** | `enquire:obsidian_find_path` / `obsidian_get_note_neighbors` | `enquire:obsidian_get_backlinks` | Manually scraping `[[wikilinks]]` |

---

### 1. Codegraph (`codegraph`)
A SQLite knowledge graph indexing symbols, AST edges, and dynamic hops across 30+ languages (Rust, C/C++, TypeScript, Python, etc.).
- **Primary Tool:** `codegraph_explore` (returns verbatim, line-numbered source code with caller context and blast radius).
- **Token Optimization:** Cross-call deduplication can be enabled via `CODEGRAPH_EXPLORE_DEDUP=1` to prevent redundant source payload across queries.
- **Dynamic Dispatch:** Follows dynamic hops, callbacks, and channel sends across language boundaries (e.g. C PipeWire callbacks into Rust FFI).
- **Exact Identifiers & Suggestions:** Requires exact symbol names (e.g. `AudioProcessor::process_frame` or `crate::engine::AudioContext`). If a symbol is mistyped, the tool returns "did-you-mean" candidate suggestions.
- **Rules & Invariants:**
  - Code returned by `codegraph_explore` is already verbatim and line-numbered: **do not re-read with `view_file`**.
  - Respect staleness banners: `⚠️ Some files referenced below were edited since the last index sync...` means only those specific files need direct reading.
  - If a directory lacks `.codegraph/`, do not run `codegraph init` automatically without user consent.

#### Example Scenario: Tracing Audio Buffer Across Boundaries
```json
{
  "query": "pipewire_process_callback tract_onnx_infer",
  "projectPath": "/home/joaorura/orca/workspaces/clearcore/hippocamp"
}
```
*Result:* Returns the topological call path from `platform/linux/helper` C bridge -> `crates/filter-capi` -> `crates/engine` -> `crates/runtime-openvino`/`tract` without reading files manually.

---

### 2. Serena (`serena`)
LSP-powered semantic code intelligence server providing AST symbol navigation, diagnostic queries, atomic refactoring, and persistent memory.

> [!CAUTION]
> **Line Numbering:** Line numbers returned by Serena tools are **0-based**. When translating Serena line numbers to native tools (such as `replace_file_content` or `view_file`), **you must add 1 (+1)**.
> **Parallel Mutations:** Never dispatch parallel mutation tool calls (`replace_content`, `replace_symbol_body`, etc.) on the **same file** in a single turn, as offsets will shift and corrupt code. Serena handles parallel calls sequentially across distinct files safely.

- **Symbol Path Syntax:** Always use forward slashes (`/`) for hierarchical symbol paths, regardless of language (e.g., `AudioProcessor/set_bypass`, not `::` or `.`).
- **Semantic Inspection:**
  - `get_symbols_overview(relative_path)`: Inspect file structure without ingesting body lines.
  - `find_symbol(name_path_pattern, relative_path, include_body)`: Retrieve exact symbol body without full-file overhead.
  - `find_referencing_symbols(name_path, relative_path)`: Contextual call sites across the workspace.
  - `find_implementations(name_path, relative_path)`: Locate trait implementations.
  - `get_diagnostics_for_file(relative_path, min_severity="warning")`: Check compiler errors/warnings before/after changes.
- **Structured Refactoring:**
  - `rename_symbol(name_path, relative_path, new_name)`: Atomic project-wide rename across declarations, usages, and imports.
  - `safe_delete_symbol(name_path_pattern, relative_path)`: Refuses deletion if references exist.
  - `replace_symbol_body(name_path, relative_path, body)`: Replaces method/function body cleanly.
  - `insert_before_symbol` / `insert_after_symbol`: Inserts code relative to top-level symbols.
  - `replace_in_files(needle, repl, mode, dry_run=true)`: Preview diffs across multiple files before committing changes.
- **Project Memories (`serena`):**
  - Graph-based memory hierarchy initialized in this repository:
    - `mem:core` (root node and references to domains)
    - `mem:tech_stack` (pinned toolchains, runtimes, audio drivers)
    - `mem:suggested_commands` (real developer lifecycle, dev, package, offline gate)
    - `mem:conventions` (zero-allocation realtime audio, `unsafe_code = forbid`, `unwrap_used = deny`)
    - `mem:task_completion` (Definition of Done and verification checklist)
  - Access via `read_memory(memory_name)` and manage via `write_memory` / `edit_memory`.

#### Example Scenario: Inspecting and Editing a Method Safely
```json
{
  "name_path_pattern": "AudioProcessor/set_bypass",
  "relative_path": "crates/engine/src/lib.rs",
  "include_body": true
}
```
*Result:* Returns only the `set_bypass` method body and 0-based lines (e.g., lines 45–52). The agent can then call `replace_symbol_body` directly, or map to lines 46–53 for native tools.

---

### 3. Enquire (`enquire`)
Topological and semantic integration with Obsidian Personal Knowledge Management (PKM) vaults, architecture decision records (ADRs), and research notes.
- **Search & Retrieval:**
  - `obsidian_search`: Multi-signal search combining BM25 (FTS5), TF-IDF, dense vector embeddings, and graph proximity (`graph_boost=true`).
  - `obsidian_hyde_search`: Hypothetical Document Embeddings search. Requires `hypothetical_answer` to match conceptual intent rather than exact keywords.
  - `obsidian_context_pack`: Assembles a compact markdown package of notes, backlinks, and daily context constrained to `budget_tokens`.
- **Graph Traversal:**
  - `obsidian_resolve_wikilink`: Resolves `[[Note#Section|Alias]]` to vault paths without manual regex parsing.
  - `obsidian_get_backlinks` / `obsidian_get_outbound_links`: Bidirectional link exploration.
  - `obsidian_find_path`: Shortest-path BFS traversal between two concepts in the knowledge graph.
  - `obsidian_get_communities`: Louvain modularity clustering over the vault graph.
- **Structured Queries & Vault Hygiene:**
  - `obsidian_dataview_query`: Execute structured queries: `(LIST|TABLE col1, col2) FROM ("folder"|#tag) [WHERE condition] [SORT field] [LIMIT n]`.
  - `obsidian_validate_note_proposal`: Lints proposed note drafts before saving to prevent broken wikilinks or malformed YAML frontmatter.
  - `obsidian_lint_wiki`: Vault health audit for orphan notes, dead ends, stubs, and broken references.

#### Example Scenario: Context Pack for Architecture Research
```json
{
  "query": "DeepFilterNet3 isolation model and voice profile integration",
  "budget_tokens": 3000,
  "include_backlinks": true
}
```

---

### Monorepo Crate Topology & Navigation Guide

When searching or refactoring code in Clearcore, target the relevant crate directly rather than running global searches:

- **`crates/engine`**: Core audio engine orchestrator. Manages real-time audio graph bridging, model switching, ring buffer state, and backpressure.
- **`crates/studio-dsp`**: Deterministic, pure digital signal processing routines (gain, limiter, noise gate, EQ). **Zero-allocation (`no_alloc`) realtime invariants apply here**.
- **`crates/supervisor`**: Background daemon management, privilege escalation, lifecycle handling, autostart, and crash handling.
- **`crates/service`**: IPC and protocol communication layer exposing status, mode switching (`active`, `bypass`, `mute`), and audio telemetry to the frontend.
- **`crates/app-tauri`**: Electron / React frontend interface, tray controller, and user settings.
- **`platform/linux/helper`**: Low-level C bridge connecting PipeWire virtual microphone nodes to the Rust engine (`crates/filter-capi`).
- **`crates/runtime-*` (`runtime-openvino`, `runtime-tensorrt`, `runtime-coreml`)**: Hardware-accelerated neural network execution backends.


