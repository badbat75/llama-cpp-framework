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

- **Server** — edit `server.ini` (port, hostname, mlock, threads, cache reuse, threads-batch, models-max, models-dir, GPU device). Browse button for the models directory; the GPU device is a dropdown of detected backends (see Models below).
- **Models** — scan `.gguf` files under `ModelsDir`, display the presets table, and configure per-model settings (ctx-size, n-gpu-layers, flash-attn, sampling, etc.) through inline fields. Includes a **Speculative decoding / MTP** card: pick a draft/MTP head GGUF (scanned from `mtps\`), a spec-type, the draft's GPU layers, and the draft device. Picking an MTP head auto-selects `draft-mtp` and, when a server GPU device is set, pins the draft to it (`device-draft = <server device>`, `n-gpu-layers-draft = 99`); otherwise it falls back to CPU (`n-gpu-layers-draft = 0`). These map to llama-server's `--model-draft`, `--spec-type`, `--n-gpu-layers-draft`, and `--device-draft`. **GPU-device fields are dropdowns** populated from `llama-server --list-devices` (e.g. `CUDA0`, `Vulkan0`) — the server-wide device, the per-preset model device, and the per-preset draft device. Note: gemma4 MTP heads (arch `gemma4-assistant`, `n_layer=0`) crash under multi-device memory fitting, so to run MTP on GPU pin the model **and** draft to a single device that has room for the chosen context (e.g. `device = CUDA0` + `device-draft = CUDA0`); leaving the draft on CPU works at any context but is slower. Changes are written to `presets.ini`, preserving hand-edits to sections not currently touched.
- **Integrations** — toggle which models appear in `opencode.json`'s `provider.llama.cpp.models` list, and copy a Claude Code env-variable snippet.

A status footer at the bottom shows the llama-server state (running / not running) and version.

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
