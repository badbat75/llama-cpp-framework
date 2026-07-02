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

Built with [Slint](https://slint.dev), the GUI has three tabs:

- **Server** — edit `server.ini` (port, hostname, mlock, threads, cache reuse, threads-batch, models-max, models-dir, GPU device, GPU split mode, tensor split). Browse button for the models directory; the GPU device is a dropdown of detected backends (see Models below). **GPU split mode** (`--split-mode`: none/layer/row) and **tensor split** (`--tensor-split`, e.g. `3,1`) control how a model is distributed across multiple GPUs — identical on CUDA and HIP; these are machine-wide defaults, overridable per-model on the Models tab. A read-only, multi-line **Command Line** card shows the exact `llama-server` invocation that **Start** would run, one `--flag value` per line joined with the shell's line-continuation character (`` ` `` on Windows, `\` on Linux) so you can paste it straight into a terminal.
- **Models** — scan `.gguf` files under `ModelsDir`, display the presets table, and configure per-model settings (ctx-size, n-gpu-layers, flash-attn, sampling, per-model multi-GPU split via split-mode / tensor-split overriding the server default, etc.) through inline fields. The **GPU layers**, **MoE CPU layers**, and **Draft GPU layers** fields are sliders ranging `0..` the model's (or draft's) layer count read from the GGUF header, each with an **auto** checkbox (default on) that omits the flag and disables the slider. When the selected draft is an MTP/nextn head (no transformer layers, `block_count 0`) the Draft GPU layers control becomes a plain CPU/GPU switch instead of a slider. Controls that don't apply are disabled: the speculative fields (spec-type, draft n-max, draft device, draft GPU layers) need a draft available — either a draft model selected or the main model embedding MTP heads — MoE CPU layers needs an MoE model, and split-mode / tensor-split (both server-wide and per-preset) are disabled when a single GPU device is pinned. A read-only **Model info** box (between the Assets and Hardware Config cards) reads GGUF metadata through llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser): dense vs MoE (+ expert counts), layer count, trained context, GQA shape, quant, and whether it embeds MTP layers — plus whether a matching MTP/DFlash drafter is present. For MoE models a **MoE layers** row shows how many layers carry experts (with a "saves VRAM (slower)" note), sizing the "MoE CPU layers" (`--n-cpu-moe`) field. It also adds a **MMProj** row (projector type, vision/audio modality, encoder depth, image/patch size) and a **Draft file** row (the selected drafter's arch/layers and, for DFlash, the trained `block_size` → the implied `--spec-draft-n-max` ceiling) when those are selected. Reads are synchronous and uncached (see `gguf.rs`); if `ggml-base.dll` can't be loaded the box shows "unavailable". The preset editor is grouped by concern: an **Assets** card holds the file pickers and speculator selection — the model, the MMProj, one draft-model dropdown that merges MTP heads (scanned from `mtps\`) and DFlash drafters (scanned from `dflashs\`) — both feeding `--model-draft` — and the spec-type dropdown. A **Hardware Config** card (directly under Model info) collects every placement knob — GPU device, GPU split mode + tensor split, GPU layers, MoE CPU layers, draft device, and draft GPU layers. A **Speculative decoding (MTP / DFlash)** card at the very bottom (below Advanced) holds only **Draft n-max** (`--spec-draft-n-max`, the max drafted tokens per step; DFlash clamps it to the model's trained `block_size - 1`, e.g. 15). Picking an MTP head auto-selects `--spec-type draft-mtp`; picking a DFlash drafter auto-selects `--spec-type draft-dflash`. Either way, when a server GPU device is set the draft is pinned to it (`device-draft = <server device>`, `n-gpu-layers-draft = 99`); otherwise it falls back to CPU (`n-gpu-layers-draft = 0`). **GPU-device fields are dropdowns** populated from `llama-server --list-devices` (e.g. `CUDA0`, `Vulkan0`) — the server-wide device, the per-preset model device, and the per-preset draft device. Note: gemma4 MTP heads (arch `gemma4-assistant`, `n_layer=0`) crash under multi-device memory fitting, so to run MTP on GPU pin the model **and** draft to a single device that has room for the chosen context (e.g. `device = CUDA0` + `device-draft = CUDA0`); leaving the draft on CPU works at any context but is slower. Changes are written to `presets.ini`, preserving hand-edits to sections not currently touched.
- **Integrations** — toggle which models appear in `opencode.json`'s `provider.llama.cpp.models` list, and copy a Claude Code env-variable snippet.

The server run controls (Start/Stop stacked over Open chat UI over Refresh) sit at the bottom of the left nav rail, reachable from any tab. **Refresh** re-reads `server.ini` / `presets.ini`, re-scans the models directory, and reloads integration state — use it after adding a model file or hand-editing a config file outside the GUI, without restarting the app. A status footer at the bottom shows the llama-server state (running / not running) and version.

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
| `main.rs` | Entry point: no args → GUI, subcommand → CLI dispatcher |
| `cli.rs` | Clap subcommands: `server` (show/set), `preset` (list/show/delete) |
| `gui.rs` | Slint GUI: bindings, callbacks, status polling |
| `server_cfg.rs` | Read/write `server.ini` |
| `presets.rs` | Read/write `presets.ini`, model scan logic |
| `model_scan.rs` | Walk `ModelsDir` for `.gguf` files |
| `gguf.rs` | Read GGUF metadata for the "Model info" box via llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser): model (dense/MoE + layer split, layers, ctx, GQA, quant, embedded MTP), mmproj (clip: projector/modality/encoder/image), and draft (layers, DFlash `block_size`); read synchronously, uncached |
| `ini.rs` | Minimal INI parser/writer (no external crate) |
| `paths.rs` | Platform-specific config and log paths |
| `integrations.rs` | opencode.json model list, Claude Code snippet |
| `runstate.rs` | Detect if `llama-server` is running (platform-specific) |
| `net_ifaces.rs` | Enumerate local network interfaces (for hostname suggestion) |
| `server_version.rs` | Parse `llama-server --version` output |
| `ui/app.slint` | Slint declarative UI definition |
| `build.rs` | Compile-time ICO → PNG, embed EXE resource on Windows |

## Code conventions

- Zero Clippy warnings (`#![warn(clippy::all)]` off by default, checked manually).
- OS portability via `#[cfg(windows)]` / `#[cfg(not(windows))]` compile-time branching — no runtime OS detection.
- No external INI crate: `ini.rs` is a simple hand-rolled INI reader/writer (~100 lines).
- GUI callbacks are `Send + 'static` closures passed to `slint::ComponentHandle::global()`.
