# llama-cpp-config

GUI + CLI configurator for [llama.cpp-framework](..).

```text
llama-cpp-config                  → launch the GUI
llama-cpp-config server show      → print server.ini
llama-cpp-config server set ...   → update server.ini fields
llama-cpp-config preset list      → list models with their presets
llama-cpp-config preset show ...  → print one preset
llama-cpp-config preset delete .. → remove one preset
```

## GUI

Built with [Slint](https://slint.dev). The nav rail switches between three tabs and carries the server run controls (Start/Stop over Open chat UI over Refresh) at its bottom, reachable from any tab. **Refresh** re-reads `server.ini` / `presets.ini`, re-scans the models directory, and reloads integration state — use it after adding a model file or hand-editing a config file outside the GUI, without restarting. A status footer shows the llama-server state (running / not running) and version.

Each tab body is its own Slint component (`ui\server_page.slint`, `ui\models_page.slint`, `ui\integrations_page.slint`); `ui\app.slint` is the `AppWindow` shell that owns all properties/callbacks and forwards them into the pages.

### Server tab — `server.ini`

- Fields: port, hostname, mlock, threads, cache reuse, threads-batch, models-max, models-dir (with a Browse button), GPU device.
- **Multi-GPU split** (machine-wide default, overridable per-model): **GPU split mode** (`--split-mode`: none/layer/row) and **tensor split** (`--tensor-split`, e.g. `3,1`) control how a model is distributed across GPUs — identical on CUDA and HIP.
- A read-only, multi-line **Command Line** card shows the exact `llama-server` invocation **Start** would run, one `--flag value` per line joined with the shell's line-continuation character (`` ` `` on Windows, `\` on Linux) so you can paste it straight into a terminal.

### Models tab — `presets.ini`

Scans `.gguf` files under `ModelsDir`, shows the presets table, and edits per-model settings. The editor is grouped into cards by concern:

- **Assets** — the file pickers and speculator selection: model file (`--model`), MMProj (`--mmproj`), ONE draft-model dropdown that merges MTP heads (scanned from `mtps\`) and DFlash drafters (scanned from `dflashs\`), both feeding `--model-draft`, plus the spec-type dropdown. Picking an MTP head auto-selects `--spec-type draft-mtp`; picking a DFlash drafter auto-selects `--spec-type draft-dflash`. Either way, when a server GPU device is set the draft is pinned to it (`device-draft`, `n-gpu-layers-draft = 99`); otherwise it falls back to CPU (`n-gpu-layers-draft = 0`).
- **Model info** (read-only, between Assets and Hardware Config) — reads GGUF metadata through llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser; see `gguf.rs`): dense vs MoE (+ expert counts), layer count, trained context, GQA shape, quant, and whether it embeds MTP layers — plus whether a matching MTP/DFlash drafter is present. For MoE models a **MoE layers** row shows how many layers carry experts (with a "saves VRAM (slower)" note), sizing the `--n-cpu-moe` field. A **MMProj** row (projector type, vision/audio modality, encoder depth, image/patch size) and a **Draft file** row (the drafter's arch/layers and, for DFlash, the trained `block_size` → the implied `--spec-draft-n-max` ceiling) appear when those are selected. Reads are synchronous and uncached; if `ggml-base.dll` can't be loaded the box shows "unavailable".
- **Hardware Config** (directly under Model info) — every placement knob: GPU device (`--device`), GPU split mode + tensor split (overriding the server default), GPU layers (`--n-gpu-layers`), MoE CPU layers (`--n-cpu-moe`), draft device (`--device-draft`), draft GPU layers (`--n-gpu-layers-draft`).
- **Advanced** — ctx-size, parallel, batch/ubatch, KV cache types, flash-attn, sampling, reasoning, etc.
- **Speculative decoding (MTP / DFlash)** (last, below Advanced) — only **Draft n-max** (`--spec-draft-n-max`, max drafted tokens per step; DFlash clamps it to the model's trained `block_size - 1`, e.g. 15).

Field behaviors:

- **GPU-device fields are dropdowns** populated from `llama-server --list-devices` (e.g. `CUDA0`, `Vulkan0`) — the server-wide device, the per-preset model device, and the per-preset draft device.
- The **GPU layers**, **MoE CPU layers**, and **Draft GPU layers** fields are sliders ranging `0..` the model's (or draft's) layer count read from the GGUF header, each with an **auto** checkbox (default on) that omits the flag and disables the slider. When the selected draft is an MTP/nextn head (no transformer layers, `block_count 0`) the Draft GPU layers control becomes a plain CPU/GPU switch instead of a slider.
- Controls that don't apply are disabled: the speculative fields (spec-type, draft n-max, draft device, draft GPU layers) need a draft available — either a draft model selected or the main model embedding MTP heads — MoE CPU layers needs an MoE model, and split-mode / tensor-split (both server-wide and per-preset) are disabled when a single GPU device is pinned.
- Changes are written to `presets.ini`, preserving hand-edits to sections not currently touched.

> **Note:** gemma4 MTP heads (arch `gemma4-assistant`, `n_layer=0`) crash under multi-device memory fitting, so to run MTP on GPU pin the model **and** draft to a single device that has room for the chosen context (e.g. `device = CUDA0` + `device-draft = CUDA0`); leaving the draft on CPU works at any context but is slower.

### Integrations tab

Toggle which models appear in `opencode.json`'s `provider.llama.cpp.models` list, and copy a Claude Code env-variable snippet.

## Config files

| File | Location | Format |
|------|----------|--------|
| `server.ini` | `%LOCALAPPDATA%\llama.cpp\config\` | INI, `[Server]` section |
| `presets.ini` | `%LOCALAPPDATA%\llama.cpp\config\` | INI, one `[model-id]` section per model |

On Linux / macOS, `%LOCALAPPDATA%\llama.cpp` maps to `$HOME/.local/share/llama.cpp`.

## Build

```powershell
# From within this directory:
cargo build --release

# Or via the parent framework build script (builds both llama.cpp and llama-cpp-config):
..\02-build.ps1
```

The build script (`build.rs`) converts `resources\llama.ico` to a PNG at compile time (using the `ico` crate) for the Slint GUI, and on Windows embeds the ICO as an EXE resource via `winresource`.

## Source layout

| File | Purpose |
|------|---------|
| `src\main.rs` | Entry point: no args → GUI, subcommand → CLI dispatcher |
| `src\cli.rs` | Clap subcommands: `server` (show/set), `preset` (list/show/delete) |
| `src\gui.rs` | Slint GUI: window setup in `run()`, per-tab wiring in `wire_*` helpers, status polling, form ↔ preset conversion |
| `src\server_cfg.rs` | Read/write `server.ini` |
| `src\presets.rs` | Read/write `presets.ini` (the `Preset` schema and INI round-trip) |
| `src\model_scan.rs` | Walk `ModelsDir` for `.gguf` files; build model/draft option lists |
| `src\gguf.rs` | Read GGUF metadata for the "Model info" box via llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser): model (dense/MoE + layer split, layers, ctx, GQA, quant, embedded MTP), mmproj (clip), and draft (layers, DFlash `block_size`); read synchronously, uncached |
| `src\devices.rs` | Enumerate GPU backends via `llama-server --list-devices` |
| `src\ini.rs` | Minimal INI parser/writer (no external crate) |
| `src\paths.rs` | Platform-specific config and log paths |
| `src\integrations.rs` | opencode.json model list, Claude Code snippet |
| `src\runstate.rs` | Detect if `llama-server` is running; render the launch command line |
| `src\net_ifaces.rs` | Enumerate local network interfaces (for hostname suggestion) |
| `src\server_version.rs` | Parse `llama-server --version` output |
| `src\single_instance.rs` | Windows single-instance mutex + window activation (Win32 FFI) |
| `ui\app.slint` | `AppWindow` shell: nav rail, run controls, modals, footer, tray; owns all properties/callbacks |
| `ui\server_page.slint` | Server tab component |
| `ui\models_page.slint` | Models tab component (preset editor) |
| `ui\integrations_page.slint` | Integrations tab component |
| `ui\components.slint` | Shared visual pieces (`SectionCard`, `LabeledField`) |
| `ui\types.slint` | Shared Slint structs (`PresetSummary`, `PresetForm`, `IntegrationModel`) |
| `build.rs` | Compile-time ICO → PNG, embed EXE resource on Windows |

## Code conventions

- Zero Clippy warnings (checked manually).
- OS portability via `#[cfg(windows)]` / `#[cfg(not(windows))]` compile-time branching — no runtime OS detection.
- No external INI crate: `ini.rs` is a simple hand-rolled INI reader/writer (~100 lines).
- GUI callbacks are `Send + 'static` closures passed to `slint::ComponentHandle::global()`.
- The `AppWindow` (`ui\app.slint`) owns every property and callback the Rust side drives; the per-tab page components receive them via Slint bindings (`<=>` for in-out data, one-way for derived props, `=> { root.x() }` for callbacks). Adding a UI field that Rust reads/writes means declaring it on `AppWindow` **and** forwarding it into the relevant page component.
