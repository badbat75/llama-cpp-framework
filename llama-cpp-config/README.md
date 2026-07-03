# llama-cpp-config

GUI + CLI configurator for [llama.cpp-framework](..).

```text
llama-cpp-config                  → launch the GUI
llama-cpp-config gui              → same, explicit
llama-cpp-config server show      → print server.ini
llama-cpp-config server set ...   → update server.ini fields
llama-cpp-config preset list      → list models with their presets
llama-cpp-config preset show ...  → print one preset
llama-cpp-config preset delete .. → remove one preset
```

## GUI

Built with [Slint](https://slint.dev). The nav rail switches between three tabs and carries the server run controls (Start/Stop over Open chat UI over Refresh) at its bottom, reachable from any tab. **Refresh** re-reads `server.ini` / `presets.ini`, re-scans the models directory, and reloads integration state — use it after adding a model file or hand-editing a config file outside the GUI, without restarting. A status footer shows the llama-server state (running / not running) and version.

Each tab body is its own Slint component (`ui\server_page.slint`, `ui\models_page.slint`, `ui\integrations_page.slint`). All shared state — every property and callback the pages and Rust both touch — lives in a single `export global AppState` (`ui\state.slint`); the pages read/write `AppState.<name>` directly and Rust drives it via `app.global::<AppState>()`. `ui\app.slint` is just the window chrome (nav rail, run controls, modals, footer) plus the tray.

Modal dialogs (New/Clone picker, Rename) overlay the whole window: Esc or a backdrop click dismisses them, and the app-wide shortcuts (Ctrl+S, Ctrl+N, Ctrl+1–3, F5) are inert while one is open (`AppState.modal_open`).

### Server tab — `server.ini`

- Fields: port, hostname, mlock, threads, cache reuse, threads-batch, models-max, models-dir (with a Browse button), GPU device. **threads** and **threads-batch** are sliders capped at the machine's logical-processor count, each with an "auto" checkbox that omits the flag (let llama.cpp pick).
- **Multi-GPU split** (machine-wide default, overridable per-model): **GPU split mode** (`--split-mode`: none/layer/row) and **tensor split** (`--tensor-split`, e.g. `3,1`) control how a model is distributed across GPUs — identical on CUDA and HIP.
- A read-only, multi-line **Command Line** card shows the exact `llama-server` invocation **Start** would run, one `--flag value` per line joined with the shell's line-continuation character (`` ` `` on Windows, `\` on Linux) so you can paste it straight into a terminal.

### Models tab — `presets.ini`

Scans `.gguf` files under `ModelsDir`, shows the presets table, and edits per-model settings. The editor is grouped into cards by concern:

- **Assets** — the file pickers and speculator selection: model file (`--model`), MMProj (`--mmproj`), ONE draft-model dropdown that merges MTP heads (scanned from `mtps\`) and DFlash drafters (scanned from `dflashs\`), both feeding `--model-draft`, plus the spec-type dropdown. Picking an MTP head auto-selects `--spec-type draft-mtp`; picking a DFlash drafter auto-selects `--spec-type draft-dflash`. Either way, when a server GPU device is set the draft is pinned to it (`device-draft`, `n-gpu-layers-draft = 99`); otherwise it falls back to CPU (`n-gpu-layers-draft = 0`). The pick policy lives in Rust (`apply_draft_pick` in `gui/models_tab.rs`, unit-tested), not in the `.slint` handler.
- **Model info** (read-only, between Assets and Hardware Config) — reads GGUF metadata through llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser; see `gguf.rs`): dense vs MoE (+ expert counts), layer count, trained context, GQA shape, quant, and whether it embeds MTP layers — plus whether a matching MTP/DFlash drafter is present. For MoE models a **MoE layers** row shows how many layers carry experts (with a "saves VRAM (slower)" note), sizing the `--n-cpu-moe` field. A **MMProj** row (projector type, vision/audio modality, encoder depth, image/patch size) and a **Draft file** row (the drafter's arch/layers and, for DFlash, the trained `block_size` → the implied `--spec-draft-n-max` ceiling) appear when those are selected. Reads are synchronous and uncached; if `ggml-base.dll` can't be loaded the box shows "unavailable".
- **Hardware Config** (directly under Model info) — every placement knob: GPU device (`--device`), GPU split mode + tensor split (overriding the server default), GPU layers (`--n-gpu-layers`), MoE CPU layers (`--n-cpu-moe`), draft device (`--device-draft`), draft GPU layers (`--n-gpu-layers-draft`).
- **Resource / context** — ctx-size, parallel seqs, batch/ubatch (all always-valued `SpinBox`es).
- **KV cache** — K/V cache-type dropdowns, flash-attn, cache-ram.
- **Chat / reasoning** — jinja, reasoning + reasoning-format (`SegmentedControl`s).
- **Sampling overrides** — temp, top-k/p, min-p, repeat/presence penalty (blank = unset).
- **Advanced** — chat-template-kwargs only.
- **Speculative decoding (MTP / DFlash)** (last) — only **Draft n-max** (`--spec-draft-n-max`, max drafted tokens per step; DFlash clamps it to the model's trained `block_size - 1`, e.g. 15).

Field behaviors:

- **GPU-device fields are dropdowns** populated from `llama-server --list-devices` (e.g. `CUDA0`, `Vulkan0`) — the server-wide device, the per-preset model device, and the per-preset draft device.
- The **GPU layers**, **MoE CPU layers**, and **Draft GPU layers** fields are sliders ranging `0..` the model's (or draft's) layer count read from the GGUF header, each with an **auto** checkbox (default on) that omits the flag and disables the slider. When the selected draft is an MTP/nextn head (no transformer layers, `block_count 0`) the Draft GPU layers control becomes a two-option on-GPU/on-CPU selector (a `SegmentedControl`, not a `Switch` — so its state is a pure read of the model and never goes stale) instead of a slider.
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
| `src\gui.rs` | Slint GUI module root: `run()` (window setup), the shared `State` cache, and all `load_*` / `refresh_*` / `apply_*` / `spawn_*` helpers |
| `src\gui\` | Per-tab callback wiring, one file each — `server_tab.rs`, `models_tab.rs`, `integrations_tab.rs`, `tray.rs` (each a `wire()` reaching `gui`'s helpers via `use super::*`) |
| `src\form.rs` | `PresetForm` ↔ `presets::Preset` conversion (`preset_to_form` / `form_to_preset`) + a round-trip test; defaults sourced from `Preset::default()` |
| `src\server_form.rs` | `ServerForm` ↔ `server_cfg::ServerConfig` conversion (`config_to_form` / `form_to_config`) + a round-trip test — the server-side mirror of `form.rs` |
| `src\proc.rs` | `run_hidden()`: launch a child process with `CREATE_NO_WINDOW` on Windows (shared by the device / version / run-state probes) |
| `src\server_cfg.rs` | Read/write `server.ini` (`from_keys` / `render` back `load` / `save`; save→load round-trip test pins the key names + `keep` rules) |
| `src\presets.rs` | Read/write `presets.ini` (the `Preset` schema and INI round-trip) |
| `src\model_scan.rs` | Walk `ModelsDir` for `.gguf` files; build model/draft option lists |
| `src\gguf.rs` | Read GGUF metadata for the "Model info" box via llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser): model (dense/MoE + layer split, layers, ctx, GQA, quant, embedded MTP), mmproj (clip), and draft (layers, DFlash `block_size`); read synchronously, uncached — pure field-extraction logic |
| `src\gguf\ffi.rs` | The `ggml-base.dll` FFI behind `gguf.rs`: dynamic DLL load + a `KvSource` over a live `gguf_context` (Windows); a `None` stub elsewhere. Public surface: `ffi::open(path)` |
| `src\devices.rs` | Enumerate GPU backends via `llama-server --list-devices` |
| `src\ini.rs` | Minimal INI parser/writer (no external crate) |
| `src\paths.rs` | Platform-specific config and log paths (`LLAMA_CPP_CONFIG_DATA_ROOT` redirects the whole tree, opencode.json included — test-only escape hatch, not an end-user knob) |
| `src\integrations.rs` | opencode.json model list, Claude Code snippet |
| `src\runstate.rs` | Detect if `llama-server` is running; render the launch command line |
| `src\net_ifaces.rs` | Enumerate local network interfaces (for hostname suggestion) |
| `src\server_version.rs` | Parse `llama-server --version` output |
| `src\single_instance.rs` | Windows single-instance mutex + window activation (Win32 FFI) |
| `ui\app.slint` | `AppWindow` window chrome + `AppTray`: nav rail, run controls, modals, footer |
| `ui\state.slint` | `export global AppState`: all shared properties + callbacks (declared once), driven from Rust via `app.global::<AppState>()` |
| `ui\server_page.slint` | Server tab component |
| `ui\models_page.slint` | Models tab component (preset editor) |
| `ui\integrations_page.slint` | Integrations tab component |
| `ui\components.slint` | Shared visual pieces: `SectionCard`, `LabeledField`, `InfoRow` (read-only label→value row), `SegmentedControl`, `MappedComboBox` (labels/values/index combo with a bounds-checked `picked`), `AutoSlider` (auto-checkbox + slider + readout), `ModalOverlay` (dim-backdrop dialog shell used by the New/Clone + Rename dialogs; Esc dismisses), `FormActions` (the Revert + primary-Save row ending every tab), and the `Tokens` / `Options` globals (canonical muted-text / selection alphas, semantic status colors `ok`/`err`/`off`; shared option lists) |
| `ui\types.slint` | Shared Slint structs (`PresetSummary`, `PresetForm`, `ServerForm`, `IntegrationModel`) |
| `src\tests\` | End-to-end / cross-cutting tests (internal `#[cfg(test)] mod tests`). `ui_bindings.rs`: headless Slint-testing-backend test — editable widgets must track the model after an edit, guarding the one-way-binding staleness bug (v1.1.1). `save_flow.rs`: drives the real Models-tab wiring (save → reload → reselect, revert, delete) against a temp config dir |
| `build.rs` | Compile-time ICO → PNG, embed EXE resource on Windows; emits Slint element debug info for non-release builds (needed by the UI test) |

## Code conventions

- Zero Clippy warnings (checked manually).
- OS portability via `#[cfg(windows)]` / `#[cfg(not(windows))]` compile-time branching — no runtime OS detection.
- No external INI crate: `ini.rs` is a simple hand-rolled INI reader/writer (~100 lines).
- GUI callbacks are `Send + 'static` closures passed to `slint::ComponentHandle::global()`.
- Every property and callback the Rust side drives lives in the `AppState` global (`ui\state.slint`), declared once. Rust uses `app.global::<AppState>().set_x()/get_x()/on_x()`; the pages reference `AppState.x` directly. Adding a UI field that Rust reads/writes is a **one-file** change in `ui\state.slint` — no per-page re-declaration or forwarding. (The tray, `AppTray`, is a separate root and keeps its own pushed-in state.)
- Editable widgets that back a scalar `AppState` field bind two-way (`<=>`), never one-way (`prop: AppState.x`): a one-way binding breaks the instant the user edits the field (Slint's "overwritten bindings" rule), leaving it stale on the next preset switch / Revert. The recognized one-way cases are: read-only displays (the `model_info_*` texts, the integration status/baseURL fields, the Server tab's Command Line `TextEdit`, the Claude-env snippet `LineEdit`), per-row model widgets inside a `for` (the integration checkboxes, `checked: item.enabled` + a `toggled` callback — safe ONLY because the sole in-place row write originates from the clicked widget itself; any other enabled-state change must rebuild the whole model so the delegates get fresh bindings, see `gui/integrations_tab.rs`), the `labels`/`values`/`index` ComboBox split (see `MappedComboBox`), sliders (a `Slider`/`AutoSlider` reads `value:` one-way + writes back via `changed`, never `<=>` — a Slider has no model-owned two-way `value` the way a SpinBox does), and `SegmentedControl` (the reactive `RadioGroup`/`Switch` replacement for a picker over a non-bool model — reads `current` purely, reports clicks via `activated`; the draft on-GPU/on-CPU control uses it for exactly this reason).

## Tests

`cargo test` runs the per-module round-trip **unit** tests (INI / form / version — inline `#[cfg(test)] mod tests` in each file) plus the cross-cutting **end-to-end** tests under `src\tests\`. Unit tests stay next to the code they cover; e2e tests that span modules or need a built `AppWindow` live in `src\tests\` (an internal `#[cfg(test)]` module tree, so they reach `crate::…` directly — no lib/bin split needed; a top-level `tests\` dir would compile as a separate crate that can't see a binary crate's internals).

`src\tests\ui_bindings.rs` drives the real `AppWindow` on Slint's testing backend and asserts each editable-widget kind (LineEdit / SpinBox / CheckBox) still tracks a fresh model value after a simulated edit — the guard against the one-way-binding staleness class of bug. It needs Slint element debug info, which `build.rs` emits only for non-release profiles, so run it with the default (debug) profile: `cargo test` works, `cargo test --release` cannot find the widgets.

`src\tests\save_flow.rs` continues in the same window (the testing backend is a process-global, single-threaded platform, so all e2e phases share ui_bindings' single `#[test]`): it wires the real Models-tab callbacks via a test-only seam in `gui.rs` and exercises save → reload → reselect → re-baseline, Revert, and delete's clear-selection sequence. All config IO is redirected at a temp dir through the `LLAMA_CPP_CONFIG_DATA_ROOT` env var (see `paths::data_root`), so the flow never touches the user's real `%LOCALAPPDATA%\llama.cpp`.
