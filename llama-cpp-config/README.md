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

Built with [Slint](https://slint.dev). The nav rail switches between three tabs and carries the server run controls (Start/Stop over Open chat UI over View logs over Refresh) at its bottom, reachable from any tab. A status footer shows the llama-server state (running / not running) and version.

The non-obvious run controls:

- **Open chat UI** launches the URL the RUNNING server was started with (`AppState.launched_url`, snapshotted at launch — a save while running changes the file, not the listening process; the footer says "restart llama-server to apply"). When the GUI didn't start the server it falls back to the SAVED host+port (`AppState.chat_url`, derived in Rust via `ServerConfig::client_host()` — a `0.0.0.0` bind maps to `localhost`, since a listen address isn't a connectable one; the Integrations base URL uses the same mapping).
- **View logs** opens `logs\llama-server.log` in an independent, non-modal window (`LogWindow`) that follows the file live like `tail -f`, leaving the main window fully interactive while it is open. Auto-scroll (checkbox, on by default) parks the cursor at the end; untick it to read scrolled-back text while the server keeps writing. A header readout ("17.3 MB · updated 5 s ago") ages every tick, so a live-but-quiet tail (llama-server goes silent between requests) reads differently from a stuck one. Always enabled — the log outlives the process, so it's exactly what you want after a crash or failed start. The tail mechanics (500 ms timer armed/stopped with the window, bounded in-memory buffer, truncation/rotation handling) live in `gui\log_window.rs` and `ui\log_window.slint`.
- **Refresh** (the button, or F5) re-reads `server.ini` / `presets.ini`, re-scans the models directory, reloads integration state, and re-probes the run status, llama-server version, and GPU devices (the exe can change under us — e.g. a `02-build.ps1` rerun). Use it after adding a model file or hand-editing a config file outside the GUI, without restarting.

Each tab body is its own Slint component (`ui\server_page.slint`, `ui\models_page.slint`, `ui\integrations_page.slint`). All shared state — every property and callback the pages and Rust both touch — lives in a single `export global AppState` (`ui\state.slint`); the pages read/write `AppState.<name>` directly and Rust drives it via `app.global::<AppState>()`. `ui\app.slint` is just the window chrome (nav rail, run controls, modals, footer) plus the tray.

Modal dialogs (New/Clone picker, Rename, the Delete confirmation, the discard-confirm guard, and the chat-template preview) overlay the whole window: Esc or a backdrop click dismisses them, and the app-wide shortcuts (Ctrl+S, Ctrl+N — which jumps to the Models tab first, Ctrl+1–3, F5) are inert while any one is open (`AppState.modal_open`). The New/Clone picker is keyboard-completable: when the filter narrows the list to exactly one model, Enter picks and confirms it.

Any navigation that would replace a dirty form — switching presets, Refresh/F5, opening New…/Clone…/Rename… — first asks to discard the unsaved edits (`gui::confirm_discard_then` parks the action Rust-side until the verdict). What counts as "dirty", and how the Integrations list survives a rebuild:

- For Refresh/F5, "dirty" also counts pending Integrations toggles (`gui::integrations_dirty` — those rows have no form baseline, so it compares them against the on-disk `opencode.json`).
- The other write paths that rebuild the Integrations list (preset save/rename/clone/delete, server save) MERGE instead of asking: existing rows keep their pending in-UI toggle, only new ids take the disk value (`gui::refresh_integrations` vs `refresh_integrations_reset`, whose reset-to-disk semantics are reserved for F5 — behind the guard — and the Integrations tab's own Save/Revert).

### Server tab — `server.ini`

- Fields: port, hostname, mlock, threads, cache reuse, threads-batch, models-max, models-dir (with a Browse button), GPU device. **threads** and **threads-batch** are sliders capped at the machine's logical-processor count, each with a "default" checkbox that omits the flag (let llama.cpp pick). The scalar fields (port, cache reuse, models-max) are `DefaultSpinBox`es — a "default" checkbox omits the flag, or an explicit integer.
- **Multi-GPU split** (machine-wide default, overridable per-model): **GPU split mode** (`--split-mode`: none/layer/row) and **tensor split** (`--tensor-split`, e.g. `3,1`) control how a model is distributed across GPUs — identical on CUDA and HIP.
- **Advanced** card: **Web UI MCP proxy** (`--webui-mcp-proxy`, on by default — the bundled chat UI needs it to call MCP tools), **Fit to VRAM** (`-fit on|off`, off by default so a preset's "offload all layers" isn't silently overridden), and **Log level** (`-lv`, framework default 4). These were previously fixed policy flags; they now round-trip through `server.ini` and default to the same framework values when untouched.
- A read-only, multi-line **Command Line** card shows the exact `llama-server` invocation **Start** would run, one `--flag value` per line joined with the shell's line-continuation character (`` ` `` on Windows, `\` on Linux) so you can paste it straight into a terminal. It auto-sizes to its content (no inner scrollbar).

### Models tab — `presets.ini`

Scans `.gguf` files under the SAVED `ModelsDir`'s fixed subfolders — `models\` (main models), `mmprojs\`, `mtps\`, `dflashs\` (`model_scan::Category::subdir`); a file sitting at the `ModelsDir` root is not scanned, and an unsaved Server-tab `ModelsDir` edit doesn't move the scans until saved (like every client-facing projection, they follow the file) — shows the presets table, and edits per-model settings. The editor is grouped into cards by concern:

- **Assets** — the file pickers and speculator selection: model file (`--model`), MMProj (`--mmproj`), and ONE draft-model dropdown that merges MTP heads (scanned from `mtps\`) and DFlash drafters (scanned from `dflashs\`), both feeding `--model-draft`, plus the spec-type dropdown. The pick policy lives in Rust (`apply_draft_pick` in `gui/models_tab.rs`, unit-tested), not in the `.slint` handler:
  - Picking an MTP head auto-selects `--spec-type draft-mtp`; picking a DFlash drafter auto-selects `--spec-type draft-dflash`.
  - Either way, when a server GPU device is set the draft is pinned to it (`device-draft`, `n-gpu-layers-draft = 99` — the shared "all layers" sentinel, `Options.all_layers` / `form::ALL_LAYERS`, equality test-asserted); otherwise it falls back to CPU (`n-gpu-layers-draft = 0`).
- **Model info** (read-only, between Assets and Hardware Config) — reads GGUF metadata through llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser; see `gguf.rs`): dense vs MoE (+ expert counts), layer count, trained context, GQA shape, quant, and whether it embeds MTP layers — plus whether a matching MTP/DFlash drafter is present. A **Chat template** row reports the embedded `tokenizer.chat_template` as `Jinja (embedded)` / `embedded (non-Jinja)` / `none` (the `{%`/`{{` heuristic mirrors llama.cpp's `common_chat_verify_template`), with a **Preview** button — shown only when a template is present — that opens the raw template text in a modal. For MoE models a **MoE layers** row shows how many layers carry experts (with a "saves VRAM (slower)" note), sizing the `--n-cpu-moe` field. A **MMProj** row (projector type, vision/audio modality, encoder depth, image/patch size) and a **Draft file** row (the drafter's arch/layers and, for DFlash, the trained `block_size` → the implied `--spec-draft-n-max` ceiling) appear when those are selected. Reads are synchronous and uncached; if `ggml-base.dll` can't be loaded the box shows "unavailable" and the load is retried on the next read (only success is memoized), so a DLL that appears later — e.g. a finishing `02-build.ps1` — is picked up without a restart.
- **Hardware Config** (directly under Model info) — the placement knobs: GPU device, GPU split mode + tensor split (overriding the server default), GPU layers, MoE CPU layers, draft device, draft GPU layers.
- The remaining cards group the runtime knobs by concern: **Resource / context** (ctx-size, parallel, batch/ubatch), **KV cache** (K/V cache-type, flash-attn, cache-ram), **Chat / reasoning** (jinja, reasoning, reasoning-format), **Sampling overrides** (temp, top-k, top-p, min-p, repeat/presence penalty), **Advanced** (chat-template-kwargs), and **Speculative decoding (MTP / DFlash)** (last: Draft n-max). The field-by-field schema — each INI key, its `--flag`, and the "default checkbox omits the flag" numeric pattern (`DefaultSpinBox` for ints, `DefaultLineEdit` for floats) — is owned by the `presets.rs` `Preset` field docs; the per-widget rationale (why top-k is an int SpinBox, the `SegmentedControl`s, the `DefaultLineEdit`'s reveal-on-uncheck) lives in `ui\models_page.slint`.

Field behaviors:

- **GPU-device fields are dropdowns** populated from `llama-server --list-devices` (e.g. `CUDA0`, `Vulkan0`) — the server-wide device, the per-preset model device, and the per-preset draft device. The probe runs async at startup and on Refresh/F5; its result is cached Rust-side (`devices::probed()`), not published as Slint state.
- The **GPU layers**, **MoE CPU layers**, and **Draft GPU layers** fields are sliders ranging `0..` the model's (or draft's) layer count read from the GGUF header, each with a **default** checkbox (on-screen label; the backing form field is `*_auto`) — checked by default — that omits the flag and disables the slider. When the selected draft is an MTP/nextn head (no transformer layers, `block_count 0`) the Draft GPU layers control becomes a two-option on-GPU/on-CPU selector (a `SegmentedControl`, not a `Switch` — so its state is a pure read of the model and never goes stale) instead of a slider.
- Controls that don't apply are disabled: the speculative fields (spec-type, draft n-max, draft device, draft GPU layers) need a draft available — either a draft model selected or the main model embedding MTP heads — MoE CPU layers needs an MoE model, and split-mode / tensor-split (both server-wide and per-preset) are disabled when a single GPU device is pinned.
- Changes are written to `presets.ini`, preserving hand-edits to sections not currently touched. Path fields (model / mmproj / model-draft, and the server side's ModelsDir) containing `;` or `#` are rejected at save: the INI format can't escape them, so they would silently reload truncated (`ini::reject_comment_markers`).

> **Note:** gemma4 MTP heads crash under multi-device memory fitting — pin model **and** draft to one device, or leave the draft on CPU. The full story (and the auto-pin the draft picker applies) lives in the `presets.rs` field docs, the single authority for this caveat.

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

The build script (`build.rs`) first regenerates `resources\llama.ico` if it's missing (it's gitignored — `resources\generate-llama-ico.mjs` rasterizes it from the llama.cpp clone's webui logo, so a fresh checkout needs the clone plus node+npm), then converts it to two PNGs at compile time (using the `ico` crate) for the Slint GUI — the plain icon plus a green-dotted "running" variant the tray switches to while llama-server is up — and on Windows embeds the ICO as an EXE resource via `winresource`.

## Source layout

| File | Purpose |
|------|---------|
| `src\main.rs` | Entry point: no args → GUI, subcommand → CLI dispatcher |
| `src\cli.rs` | Clap subcommands: `server` (show/set), `preset` (list/show/delete) |
| `src\gui.rs` | Slint GUI module root: `run()` (window setup), the shared `State` cache, and all `load_*` / `refresh_*` / `apply_*` / `spawn_*` helpers |
| `src\gui\` | Per-tab callback wiring, one file each — `server_tab.rs`, `models_tab.rs`, `integrations_tab.rs`, `tray.rs`, plus `log_window.rs` (the View-logs window: tail-follow state machine + poll timer) — each a `wire()` reaching `gui`'s helpers via `use super::*` |
| `src\form.rs` | `PresetForm` ↔ `presets::Preset` conversion (`preset_to_form` / `form_to_preset`) + a round-trip test; defaults sourced from `Preset::default()` |
| `src\server_form.rs` | `ServerForm` ↔ `server_cfg::ServerConfig` conversion (`config_to_form` / `form_to_config`) + a round-trip test — the server-side mirror of `form.rs` |
| `src\proc.rs` | `run_hidden()`: launch a child process with `CREATE_NO_WINDOW` on Windows (shared by the device / version / run-state probes); `prepend_rocm_path()`: put the ROCm bin dir on every llama-server child's PATH so ggml-hip.dll loads (the HIP SDK installer never adds it, and ggml silently skips an unloadable backend → HIP GPUs would enumerate as Vulkan-only) |
| `src\server_cfg.rs` | Read/write `server.ini` (`from_keys` / `render` back `load` / `save`; save→load round-trip test pins the key names + `keep` rules) |
| `src\presets.rs` | Read/write `presets.ini` (the `Preset` schema and INI round-trip) |
| `src\model_scan.rs` | Walk `ModelsDir` for `.gguf` files; build model/draft option lists |
| `src\gguf.rs` | Read GGUF metadata for the "Model info" box via llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser): model (dense/MoE + layer split, layers, ctx, GQA, quant, embedded MTP), mmproj (clip), and draft (layers, DFlash `block_size`); read synchronously, uncached — pure field-extraction logic |
| `src\gguf\ffi.rs` | The `ggml-base.dll` FFI behind `gguf.rs`: dynamic DLL load + a `KvSource` over a live `gguf_context` (Windows); a `None` stub elsewhere. Public surface: `ffi::open(path)` |
| `src\devices.rs` | Enumerate GPU backends via `llama-server --list-devices` |
| `src\ini.rs` | Minimal INI parser/writer (no external crate) |
| `src\paths.rs` | Platform-specific config and log paths (`LLAMA_CPP_CONFIG_DATA_ROOT` redirects the whole tree — opencode.json and home-derived defaults included — test-only escape hatch, not an end-user knob); also locates the ROCm runtime (`rocm_bin_dir`: `HIP_PATH`, else newest `%ProgramFiles%\AMD\ROCm\<ver>`) |
| `src\integrations.rs` | opencode.json model list, Claude Code snippet |
| `src\runstate.rs` | Detect if `llama-server` is running; start/stop it; render the launch command line (incl. `--webui-mcp-proxy` / `-fit` / `-lv`, exposed on the Server tab's Advanced card and defaulting to the framework's on / off / 4) |
| `src\net_ifaces.rs` | Enumerate local network interfaces — populates the Server tab's "Bind to" dropdown |
| `src\server_version.rs` | Parse `llama-server --version` output |
| `src\single_instance.rs` | Windows single-instance mutex + window activation (Win32 FFI) |
| `ui\app.slint` | `AppWindow` window chrome + `AppTray`: nav rail, run controls, modals, footer (re-exports `LogWindow` for the Rust codegen) |
| `ui\log_window.slint` | `LogWindow`: the independent View-logs window (read-only TextEdit tail + Auto-scroll checkbox), pushed-in state like the tray |
| `ui\state.slint` | `export global AppState`: all shared properties + callbacks (declared once), driven from Rust via `app.global::<AppState>()` |
| `ui\server_page.slint` | Server tab component |
| `ui\models_page.slint` | Models tab component (preset editor) |
| `ui\integrations_page.slint` | Integrations tab component |
| `ui\components.slint` | Shared visual pieces — layout/chrome (`SectionCard`, `LabeledField`, `InfoRow`, `FormActions`, `ModalOverlay`), the custom inputs (`SegmentedControl`, `MappedComboBox`, `EnumComboBox`, `DefaultSpinBox`/`DefaultLineEdit`, `AutoSlider`, `SplitFields`), the list-row pieces (`SelectableRow`, `AccentBar`, `ListPanel`), and the `Tokens`/`Options` globals. Per-component rationale (incl. the `EnumComboBox` #11970 workaround and the `all_layers` sentinel) lives in the file's own header. |
| `ui\types.slint` | Shared Slint structs (`PresetSummary`, `PresetForm`, `ServerForm`, `IntegrationModel`) |
| `src\tests\` | End-to-end / cross-cutting tests (internal `#[cfg(test)] mod tests`). `ui_bindings.rs`: headless Slint-testing-backend test — editable widgets must track the model after an edit, guarding the one-way-binding staleness bug (v1.1.1). `save_flow.rs`: drives the real Models + Integrations tab wiring (save → reload → reselect, revert, delete, rename, clone, New…-dialog id de-conflict, the discard-confirm guard, the Integrations rebuild invariant) against a temp config dir. `binding_lint.rs`: text-scans every `ui\*.slint` for one-way `AppState` bindings on self-assigning widgets — the per-instance complement to ui_bindings' per-kind coverage |
| `build.rs` | Compile-time ICO → two PNGs (plain + running-dot tray variant), embed EXE resource on Windows; emits Slint element debug info for non-release builds (needed by the UI test) |

## Code conventions

- Zero Clippy warnings (checked manually).
- OS portability via `#[cfg(windows)]` / `#[cfg(not(windows))]` compile-time branching — no runtime OS detection.
- No external INI crate: `ini.rs` is a hand-rolled INI reader/writer with an explicit behavioral contract in its header (case/whitespace-tolerant section lookup, inline-comment stripping matching llama-server's own preset PEG, EOL detection, atomic writes).
- GUI callbacks are `Send + 'static` closures passed to `slint::ComponentHandle::global()`.
- Every property and callback the Rust side drives lives in the `AppState` global (`ui\state.slint`), declared once. Rust uses `app.global::<AppState>().set_x()/get_x()/on_x()`; the pages reference `AppState.x` directly. Adding a UI field that Rust reads/writes is a **one-file** change in `ui\state.slint` — no per-page re-declaration or forwarding. (The tray, `AppTray`, and the View-logs window, `LogWindow`, are separate roots and keep their own pushed-in state.)
- Editable widgets that back a scalar `AppState` field bind two-way (`<=>`), never one-way (`prop: AppState.x`): a one-way binding breaks the instant the user edits the field (Slint's "overwritten bindings" rule), leaving it stale on the next preset switch / Revert. The recognized one-way cases are:
  - **Read-only displays** — the `model_info_*` texts, the integration status/baseURL fields, the Server tab's Command Line `TextEdit`, the Claude-env snippet `LineEdit`.
  - **Per-row model widgets inside a `for`** — the integration checkboxes, `checked: item.enabled` + a `toggled` callback. Safe ONLY because the sole in-place row write originates from the clicked widget itself; any other enabled-state change must rebuild the whole model so the delegates get fresh bindings (see `gui/integrations_tab.rs`).
  - **The `labels`/`values`/`index` ComboBox split** — see `MappedComboBox`.
  - **String-valued ComboBoxes** — a `current-value` two-way binding does NOT move the selection on a model change (Slint #11970), so `EnumComboBox` drives `current-index` instead, derived from the string `value` and pushed on change like the slider below (see `ui\components.slint`).
  - **Sliders** — the std `Slider` imperatively self-assigns `value` on every drag, so NO declarative binding on it survives. `AutoSlider` flows outward via `changed(v)` and pushes external updates back in via a `changed shown` hook (see `ui\components.slint`).
  - **`SegmentedControl`** — the reactive `RadioGroup`/`Switch` replacement for a picker over a non-bool model: reads `current` purely, reports clicks via `activated`. The draft on-GPU/on-CPU control uses it for exactly this reason.

## Tests

`cargo test` runs the per-module round-trip **unit** tests (INI / form / version — inline `#[cfg(test)] mod tests` in each file) plus the cross-cutting **end-to-end** tests under `src\tests\`. Unit tests stay next to the code they cover; e2e tests that span modules or need a built `AppWindow` live in `src\tests\` (an internal `#[cfg(test)]` module tree, so they reach `crate::…` directly — no lib/bin split needed; a top-level `tests\` dir would compile as a separate crate that can't see a binary crate's internals).

`src\tests\ui_bindings.rs` drives the real `AppWindow` on Slint's testing backend and asserts each editable-widget kind (LineEdit / SpinBox / CheckBox / the AutoSlider's Slider) still tracks a fresh model value after a simulated edit — the guard against the one-way-binding staleness class of bug. The `DefaultSpinBox` / `DefaultLineEdit` composites are exercised here via their inner widgets (the `preset-ctx-size` SpinBox, the `preset-temp` LineEdit) for the `value` leg; their `default`-checkbox leg is guarded statically by `binding_lint` (both call-site bindings, on every instance). It needs Slint element debug info, which `build.rs` emits only for non-release profiles, so run it with the default (debug) profile: `cargo test` works, `cargo test --release` cannot find the widgets.

`src\tests\save_flow.rs` continues in the same window (the testing backend is a process-global, single-threaded platform, so all e2e phases share ui_bindings' single `#[test]`): it wires the real Models + Integrations tab callbacks via a test-only seam in `gui.rs` and exercises save → reload → reselect → re-baseline, Revert, delete's clear-selection, the rename and clone funnels, the discard-confirm guard on a dirty form (New…, and Rename…'s Rust-routed entry point), the New…-dialog's id de-conflict (a second New… on the same model must yield `<id>-2`, not overwrite), and the Integrations row-checkbox rebuild invariant (a clicked delegate's broken one-way binding must be replaced — not `set_row_data`-patched — by `refresh_integrations`). All config IO is redirected at a temp dir through the `LLAMA_CPP_CONFIG_DATA_ROOT` env var (see `paths::data_root`), so the flow never touches the user's real `%LOCALAPPDATA%\llama.cpp` — and because that env var is process-wide and never restored, unit tests elsewhere must never touch `paths::`.

`src\tests\binding_lint.rs` is a plain text scan (its own `#[test]`, no Slint backend): it walks every `ui\*.slint` and fails on a one-way `prop: AppState.…` binding on a self-assigning widget property (LineEdit/TextEdit `text`, CheckBox/Switch `checked`, SpinBox/Slider `value`, ComboBox `current-*`, and the `DefaultSpinBox`/`DefaultLineEdit` composites' `value` + `default`), honoring the sanctioned escapes above (`<=>`, `read-only: true`, non-AppState expressions, custom components). ui_bindings proves each widget *kind* behaves; this catches the *new instance* someone wires one-way.
