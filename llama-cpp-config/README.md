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

- Fields: port, hostname, mlock, no-mmap, threads, cache reuse, threads-batch, models-max, models-dir (with a Browse button). **threads** and **threads-batch** are sliders capped at the machine's logical-processor count, each with a "default" checkbox that omits the flag (let llama.cpp pick). The scalar fields (port, cache reuse, models-max) are `DefaultSpinBox`es — a "default" checkbox omits the flag, or an explicit integer.
- **Memory residency**: **mlock** (`--mlock`, on by default) locks whatever is resident into physical RAM; **no-mmap** (`--no-mmap`, off by default — llama.cpp mmaps the GGUF) reads the weights into RAM up front instead. The two pair up: with mmap on, pages fault in from the file as they are first touched, so mlock alone only pins what has already been read.
- **GPU distribution** card — see [the table](#gpu-distribution-table) below (machine-wide default, overridable per-model), plus **Vision encoder**: which GPU the mmproj/CLIP image encoder runs on. That is a separate dropdown because the encoder **ignores `--device`** — llama.cpp gives it the first GPU backend the registry offers, so on a mixed CUDA+ROCm box it lands on the NVIDIA card even when the model is on the AMD one, holds VRAM there for the model's whole lifetime, and computes only on image requests. There is no flag for it: the field is written to `server.ini` as `MmprojDevice` and exported to the child as the `MTMD_BACKEND_DEVICE` env var (`runstate::env_vars`, which the Command Line card renders too).
- **Tensor placement** card — [the same table](#tensor-placement-table) as the Models tab, machine-wide (`server.ini` `OverrideTensor` → `--override-tensor`). Unlike the GPU card above, this one does not merely provide a *default*: anything set here **replaces** every preset's own rules rather than adding to them, and a device this machine doesn't have stops the **server** from starting (not just the model that would have used it). Leave it empty to let each preset keep its own placement.
- **Advanced** card: **Web UI MCP proxy** (`--webui-mcp-proxy`, on by default — the bundled chat UI needs it to call MCP tools), **Fit to VRAM** (`-fit on|off`, off by default so a preset's "offload all layers" isn't silently overridden), and **Log level** (`-lv`, framework default 4). These were previously fixed policy flags; they now round-trip through `server.ini` and default to the same framework values when untouched.
- A read-only, multi-line **Command Line** card shows the exact `llama-server` invocation **Start** would run, one `--flag value` per line joined with the shell's line-continuation character (`` ` `` on Windows, `\` on Linux) so you can paste it straight into a terminal. It auto-sizes to its content (no inner scrollbar).

### Models tab — `presets.ini`

Scans `.gguf` files under the SAVED `ModelsDir`'s fixed subfolders — `models\` (main models), `mmprojs\`, `mtps\`, `dflashs\` (`model_scan::Category::subdir`); a file sitting at the `ModelsDir` root is not scanned, and an unsaved Server-tab `ModelsDir` edit doesn't move the scans until saved (like every client-facing projection, they follow the file) — shows the presets table, and edits per-model settings. The editor is grouped into cards by concern:

- **Assets** — the file pickers and speculator selection: model file (`--model`), MMProj (`--mmproj`) with a **Vision on GPU** checkbox (`--mmproj-offload`/`--no-mmproj-offload`: off keeps the image encoder on CPU entirely — *which* GPU it lands on otherwise is the Server tab's Vision encoder field), and ONE draft-model dropdown that merges MTP heads (scanned from `mtps\`) and DFlash drafters (scanned from `dflashs\`), both feeding `--model-draft`, plus the spec-type dropdown. The pick policy lives in Rust (`apply_draft_pick` in `gui/models_tab.rs`, unit-tested), not in the `.slint` handler:
  - Picking an MTP head auto-selects `--spec-type draft-mtp`; picking a DFlash drafter auto-selects `--spec-type draft-dflash`.
  - Either way, when a GPU is selected the draft is pinned to it — the FIRST one, if several (`device-draft` takes a single device, and a multi-device draft split is exactly what crashes MTP heads) — with `n-gpu-layers-draft = 99` (the shared "all layers" sentinel, `Options.all_layers` / `form::ALL_LAYERS`, equality test-asserted); otherwise it falls back to CPU (`n-gpu-layers-draft = 0`).
- **Model info** (read-only, between Assets and Hardware Config) — reads GGUF metadata through llama.cpp's own reader (runtime-loaded `ggml-base.dll`, no reimplemented parser; see `gguf.rs`): dense vs MoE (+ expert counts), layer count, trained context, GQA shape, quant, and whether it embeds MTP layers — plus whether a matching MTP/DFlash drafter is present. A **Chat template** row reports the embedded `tokenizer.chat_template` as `Jinja (embedded)` / `embedded (non-Jinja)` / `none` (the `{%`/`{{` heuristic mirrors llama.cpp's `common_chat_verify_template`), with a **Preview** button — shown only when a template is present — that opens the raw template text in a modal. For MoE models a **MoE layers** row shows how many layers carry experts (with a "saves VRAM (slower)" note), sizing the `--n-cpu-moe` field. A **MMProj** row (projector type, vision/audio modality, encoder depth, image/patch size) and a **Draft file** row (the drafter's arch/layers and, for DFlash, the trained `block_size` → the implied `--spec-draft-n-max` ceiling) appear when those are selected. Reads are synchronous and uncached; if `ggml-base.dll` can't be loaded the box shows "unavailable" and the load is retried on the next read (only success is memoized), so a DLL that appears later — e.g. a finishing `02-build.ps1` — is picked up without a restart.
- **GPU distribution** (directly under Model info) — [the table](#gpu-distribution-table), overriding the server-wide default.
- **Tensor placement** (under GPU distribution) — [the table](#tensor-placement-table) for `--override-tensor`: the exceptions to the card above, i.e. tensors that go on a device of their own whatever their layer does.
- **Hardware Config** — the remaining placement knobs: GPU layers, MoE CPU layers, draft device, draft GPU layers.
- The remaining cards group the runtime knobs by concern: **Resource / context** (ctx-size, parallel, batch/ubatch), **KV cache** (K/V cache-type, flash-attn, cache-ram), **Chat / reasoning** (jinja, reasoning, reasoning-format, [keep-past-thinking](#keep-past-thinking-vs-a-raw-preserve_thinking-kwarg)), **Sampling overrides** (temp, top-k, top-p, min-p, repeat/presence penalty), **Advanced** (chat-template-kwargs), and **Speculative decoding (MTP / DFlash)** (last: Draft n-max). The field-by-field schema — each INI key, its `--flag`, and the "default checkbox omits the flag" numeric pattern (`DefaultSpinBox` for ints, `DefaultLineEdit` for floats) — is owned by the `presets.rs` `Preset` field docs; the per-widget rationale (why top-k is an int SpinBox, the `SegmentedControl`s, the `DefaultLineEdit`'s reveal-on-uncheck) lives in `ui\models_page.slint`.

Field behaviors:

- The **draft device** and **Vision encoder** fields are dropdowns populated from `llama-server --list-devices` (e.g. `CUDA0`, `Vulkan0`). The probe runs async at startup and on Refresh/F5; its result is cached Rust-side (`devices::probed()`), not published as Slint state — the GPU table gets its rows built from it in Rust too (`gpu_split::build_rows`).
- The **GPU layers**, **MoE CPU layers**, and **Draft GPU layers** fields are sliders ranging `0..` the model's (or draft's) layer count read from the GGUF header, each with a **default** checkbox (on-screen label; the backing form field is `*_auto`) — checked by default — that omits the flag and disables the slider. When the selected draft is an MTP/nextn head (no transformer layers, `block_count 0`) the Draft GPU layers control becomes a two-option on-GPU/on-CPU selector (a `SegmentedControl`, not a `Switch` — so its state is a pure read of the model and never goes stale) instead of a slider.
- Controls that don't apply are disabled: spec-type and draft n-max need a draft available (a draft model selected, or the main model embedding MTP heads); **draft device and Draft GPU layers need a draft FILE** — see the note below; MoE CPU layers needs an MoE model; split-mode needs other than exactly one GPU checked.
- Changes are written to `presets.ini`, preserving hand-edits to sections not currently touched. Path fields (model / mmproj / model-draft, and the server side's ModelsDir) containing `;` or `#` are rejected at save: the INI format can't escape them, so they would silently reload truncated (`ini::reject_comment_markers`).

> **Note — embedded MTP heads ignore the draft placement fields.** llama.cpp reads `--device-draft` and `--n-gpu-layers-draft` only when a separate `--model-draft` file is given (both live inside `if (has_dft())`). With MTP heads embedded in the main GGUF — `spec-type` set, no `model-draft` — it builds the draft context against the *target model*, so the draft runs on the model's own GPU and both keys are silently ignored. Setting them there is how a GPU ends up looking assigned to the draft while never drafting a token, so the UI disables them and says so. A **separate** MTP head file (e.g. gemma4-assistant) is the case where they do apply — and there, pin to ONE device: multi-device memory fitting crashes those heads. The full story lives in the `presets.rs` field docs, the single authority for this caveat.

### GPU distribution table

`--tensor-split` is a positional vector indexed over the devices named by `--device`, **in `--device` order**. Typing `3,1` into a text box therefore means nothing until you know which devices those are — and with `--device` unset, "those devices" is every detected backend, which on a mixed box is a CUDA card, two ROCm devices (one of them an iGPU) and three duplicate Vulkan views of the same three GPUs. So the two settings share one widget (`GpuSplitTable`, used by both tabs): a row per detected GPU with a checkbox, its VRAM, an editable weight, and the derived share, plus a summary line showing the exact flags produced.

Rows are built in Rust (`gpu_split::build_rows`): the **checked devices first, in split order**, then the rest in probe order. So the table read top to bottom *is* the `--device` list and its `--tensor-split` vector. Checking a box **appends** the device to the split; the **≡ drag handle** on each checked row is how you reorder it — the weight rides along with its device, so the proportions survive a move.

Position 0 is not cosmetic: it is `devices[0]`, which is also llama.cpp's **`main_gpu`** (it defaults `--main-gpu` to 0 and the framework never overrides it). With `--split-mode none` that is the *only* GPU llama.cpp keeps; under `layer` it takes the first slice of layers. Without the handle there was no way to put a chosen GPU at the head.

> The `--list-devices` ids are **not stable** across driver states — the same machine has enumerated its discrete AMD card as `ROCm0` on one boot and `ROCm1` on another, with the iGPU taking the other slot. Presets pin by id, so the table always shows the device **name** next to it: check the name, not the number.

The four states it can express map 1:1 onto llama.cpp:

| checked | `device` | `tensor-split` | meaning |
|---|---|---|---|
| 0 | *(blank)* | *(blank)* | llama.cpp uses all detected devices |
| 1 | `ROCm1` | *(blank)* | one GPU, nothing to split |
| ≥2, **Auto** | `ROCm1,CUDA0` | *(blank)* | llama.cpp splits by **free** VRAM at load |
| ≥2, explicit | `ROCm1,CUDA0` | `3,1` | 75% / 25% |

Note that blank-with-2-GPUs is *auto-by-free-VRAM*, **not** an even split — hence **Auto** and **Even** are two separate buttons, and the weight SpinBoxes stay disabled until the split is explicit (Even is the way in). That is also what keeps the one-way `for`-row bindings honest: a weight edit never rewrites another row's weight, so it needs no model rebuild — while toggle / Auto / Even do rebuild, giving every delegate a fresh binding. All of it is guarded: `gpu_split`'s unit tests for the rules, and a `save_flow` phase that clicks the real checkboxes and asserts what reaches the INI.

A selected device the probe doesn't know — a stale id, another machine's GPU, or simply the async probe not landed yet — is kept as a `(not detected)` checked row, so a save can never silently drop it.

> **Note:** a server-wide selection **overrides** every preset's own, because llama-server's router passes its own CLI args on top of each preset (`preset.merge(base_preset)`). The Models tab shows a warning strip when that is in effect.

### Tensor placement table

`--override-tensor` (`-ot`) is the exception list to the table above: `<regex>=<buffer type>` rules that send every tensor whose **name** matches the regex to a device of its own, whatever its layer does. llama.cpp matches with `std::regex_search` (`llama-model-loader.cpp`) and resolves the buffer type against every backend's `ggml_backend_dev_buffer_type` — so the legal targets are exactly the ids `--list-devices` prints, **plus `CPU`** (an unknown one is a hard `throw`: the model does not load). The CPU being a first-class target is why this table offers it and the GPU table filters it out.

The reason it exists — and the case the **+ Add override** button seeds in one click:

> llama.cpp keeps `token_embd.weight` in **host** memory even when it reports `offloaded 33/33 layers to GPU`. The embedding lookup is a `get_rows` over a handful of tokens: cheap on CPU, and worth a GB or two of VRAM. But with a GPU backend active that host buffer is **pinned** (it shows up in the log as `ROCm_Host` / `CUDA_Host`), and Windows counts pinned host memory as **Shared GPU memory** — so a model that fits in VRAM with room to spare still shows GBs of shared allocation, and the compute graph carries a CPU split it didn't need. On a big-vocab BF16 model the table is huge (`n_vocab 248320 × n_embd 4096 × 2 B` = 1940 MiB), which is when `token_embd\.weight=ROCm0` is worth the VRAM it costs.

The pattern is a **dropdown**, not a free-text field, because the value's grammar is unforgiving in two ways that llama.cpp applies *before* it parses anything, neither of them escapable:

- rules are split on `,` — so a `{1,2}` quantifier inside a regex tears its own rule in half, and the half without an `=` is a fatal `invalid value`;
- each rule is split at its **first** `=` — so an `=` inside the regex silently eats the pattern's tail.

So the three canned patterns (Embedding table, Output head, and MoE experts — llama.cpp's own `LLM_FFN_EXPS_REGEX` verbatim, i.e. `MoE experts → CPU` *is* `--cpu-moe`) cover what people reach for, and **Custom regex…** reveals a text field that strips those two characters on the Rust side. A row's kind is *derived* from its pattern, with no hidden "is custom" bit to drift from a hand-edited INI — which is why picking Custom clears the regex rather than keeping the canned one it was switched from (a row still holding `\.ffn_…_exps` would simply read back as the MoE kind).

Rules apply **in order** — the first match wins — so no edit re-sorts them. A device the probe doesn't know is kept as a `(custom)` dropdown entry and flagged on its row, exactly as in the GPU table; a rule left with **no** device (only a hand-edited INI can produce one) is refused at save, because llama.cpp's failure mode for it is a throw during arg parsing, i.e. "the model didn't load" and nothing else. Same one-way `for`-row binding contract as the GPU table, with the same escape: add / remove / kind / device rebuild the row model, while a keystroke in the regex field must **not** (it would recreate the field mid-edit and drop the caret) — and it can afford not to, since it changes no other row. Guarded by `tensor_override`'s unit tests for the rules and a `save_flow` phase that drives the real callbacks, types into the real regex field, and asserts what reaches the INI.

The same table appears **twice**, once per scope: per-preset (`presets.ini` `override-tensor`) and server-wide (`server.ini` `OverrideTensor`). Same widget, same rules module — but not the same reach:

> **Note:** a server-wide value does not merely out-rank a preset's rules, it **replaces** them. `-ot` is a `push_back` onto a vector inside llama.cpp, so "the two lists concatenate" is the natural guess and it is **wrong**: the router merges its own CLI args into each preset as a key→value map (`common_preset::merge` → `options[opt] = val`, with the CLI as the writer), so exactly one `--override-tensor` ever reaches a child — the server's. Verified against b9976: a preset asking for `token_embd\.weight=CPU` under a server-wide `token_embd\.weight=CUDA0` spawns its child with the CUDA0 rule and no trace of the CPU one. The Models tab shows a warning strip naming the value that shadows it (`AppState.tensor_override_warning`), the twin of the GPU table's.

One asymmetry between the two scopes: the buffer type in the server-wide value is parsed by the **router itself**, at startup, against its own device registry. A rule naming a device this machine doesn't have therefore fails with `unknown buffer type` and **the server does not start at all** — where the same mistake in a preset only kills the model that uses it. (The two dropdowns are also built per-scope, not shared: `device_options` appends a `(custom)` entry per unknown device id, so its length — and every row's `device_index` — depends on that table's own rules.)

### Keep past thinking, vs. a raw `preserve_thinking` kwarg

**Keep past thinking** (Chat / reasoning card) is `--reasoning-preserve` / `--no-reasoning-preserve`: replay the reasoning trace of *every* past assistant turn back to the model, not just the last one. It is a **tri-state** — `default` omits the flag and leaves the template to its own behaviour, which is a third instruction, not the absence of one. It is orthogonal to **Reasoning** (`--reasoning`, i.e. *whether the model thinks at all*): this one is about what happens to the thinking it already did.

It is also the **only** supported lever, and the reason it is a field of its own rather than something to hand-write into **Template kwargs** below it. llama.cpp turns this flag into the kwarg `preserve_reasoning`, and `caps_apply_preserve_reasoning` (`common/jinja/caps.cpp`) expands *that* into three template variables at once:

```
preserve_thinking = v      clear_thinking = !v      truncate_history_thinking = !v
```

because templates disagree on which name they read — Qwen3.6 and LFM2.5 read `preserve_thinking`, GLM-4.7 reads `clear_thinking`, Nemotron reads `truncate_history_thinking`. Setting one of those three by hand as a `chat-template-kwargs` entry therefore works only on the templates that happen to use that name, and is a **silent no-op** on the rest — and it skips the capability probe either way, so `llama-server` still logs `chat template supports preserving reasoning, consider enabling it via --reasoning-preserve` as though nothing were set (verified against b9976). The flag sets all three and is checked, which is why the field writes it and not the kwarg.

Two things it does not promise. It only bites where the template actually gates on those variables: a template that emits history reasoning **unconditionally** (Ornith-1.0 does — no gate at all) already preserves it, and reads as "supported" to the probe purely because the trace shows up in the rendered output. And it is not free: the preserved trace is re-sent as prompt tokens every turn. What it buys, on a template that *does* gate — the Qwen3.6 form is `if preserve_thinking or loop.index0 > ns.last_query_index` — is a **stable prefix**: without it, an assistant turn is rendered *with* its thinking while it is the newest, then re-rendered *without* it as soon as the next user message arrives. That retroactive rewrite of an already-cached prefix is precisely what a KV cache cannot survive.

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
| `src\devices.rs` | Enumerate GPU backends via `llama-server --list-devices`: id, name, and total/free VRAM per device |
| `src\gpu_split.rs` | The GPU distribution table's rules: parse/render the `device` + `tensor-split` string pair, build the rows, apply an edit (toggle / weight / Auto / Even). Pure, fully unit-tested |
| `src\tensor_override.rs` | The tensor-placement table's rules: parse/render the `override-tensor` rule list, build the rows + both dropdown models, apply an edit (add / remove / kind / device / regex), sanitize the regex, validate the value llama.cpp would `throw` on. Pure, fully unit-tested |
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
| `ui\components.slint` | Shared visual pieces — layout/chrome (`SectionCard`, `LabeledField`, `InfoRow`, `FormActions`, `ModalOverlay`), the custom inputs (`SegmentedControl`, `MappedComboBox`, `EnumComboBox`, `DefaultSpinBox`/`DefaultLineEdit`, `AutoSlider`, `GpuSplitTable`, `TensorOverrideTable`), the list-row pieces (`SelectableRow`, `AccentBar`, `ListPanel`), and the `Tokens`/`Options` globals. Per-component rationale (incl. the `EnumComboBox` #11970 workaround and the `all_layers` sentinel) lives in the file's own header. |
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
  - **Per-row model widgets inside a `for`** — the integration checkboxes (`checked: item.enabled` + a `toggled` callback), the GPU table's row checkbox + weight SpinBox, and the tensor-placement table's two row ComboBoxes + regex LineEdit. Safe ONLY because the sole in-place row write originates from the clicked/typed-into widget itself; any other change must rebuild the whole model so the delegates get fresh bindings (see `gui/integrations_tab.rs`, `gui::refresh_gpu_rows`, `gui::refresh_tensor_rows` — which is also why neither a GPU weight edit nor a regex keystroke touches another row, and so needs no rebuild: rebuilding mid-keystroke would recreate the field and drop the caret).
  - **The `labels`/`values`/`index` ComboBox split** — see `MappedComboBox`.
  - **String-valued ComboBoxes** — a `current-value` two-way binding does NOT move the selection on a model change (Slint #11970), so `EnumComboBox` drives `current-index` instead, derived from the string `value` and pushed on change like the slider below (see `ui\components.slint`).
  - **Sliders** — the std `Slider` imperatively self-assigns `value` on every drag, so NO declarative binding on it survives. `AutoSlider` flows outward via `changed(v)` and pushes external updates back in via a `changed shown` hook (see `ui\components.slint`).
  - **`SegmentedControl`** — the reactive `RadioGroup`/`Switch` replacement for a picker over a non-bool model: reads `current` purely, reports clicks via `activated`. The draft on-GPU/on-CPU control uses it for exactly this reason.
- **Glyphs.** Text drawn with the default font may only use codepoints Segoe UI actually has — the allowlist is `RENDERABLE` in `src\tests\binding_lint.rs` (every entry checked against the font's cmap, not assumed), and a `.slint` string literal using anything else fails the build. Nothing falls back: a codepoint Segoe UI lacks draws as a correctly-laid-out **empty box**, which is how the tensor table's remove button first shipped reading `✕` (U+2715, Dingbats). Valid Slint, valid UTF-8, no warning — only a human looking at the window could catch it, which is what the lint replaces. An **icon** on a `Text` need not be an allowlist entry — name the icon font at the call site, `text: "\u{E946}"; font-family: "Segoe Fluent Icons";`, as `LabeledField`'s hint glyph does (the scan skips `\u{…}` escapes to keep that path open). That escape is **not** available on a std `Button`: it has no `font-family` property, so its label is stuck with the default font and its glyph must be one Segoe UI covers (hence the tensor table's remove button reads `×`, U+00D7, and not an icon-font close glyph).

## Tests

`cargo test` runs the per-module round-trip **unit** tests (INI / form / version — inline `#[cfg(test)] mod tests` in each file) plus the cross-cutting **end-to-end** tests under `src\tests\`. Unit tests stay next to the code they cover; e2e tests that span modules or need a built `AppWindow` live in `src\tests\` (an internal `#[cfg(test)]` module tree, so they reach `crate::…` directly — no lib/bin split needed; a top-level `tests\` dir would compile as a separate crate that can't see a binary crate's internals).

`src\tests\ui_bindings.rs` drives the real `AppWindow` on Slint's testing backend and asserts each editable-widget kind (LineEdit / SpinBox / CheckBox / the AutoSlider's Slider) still tracks a fresh model value after a simulated edit — the guard against the one-way-binding staleness class of bug. The `DefaultSpinBox` / `DefaultLineEdit` composites are exercised here via their inner widgets (the `preset-ctx-size` SpinBox, the `preset-temp` LineEdit) for the `value` leg; their `default`-checkbox leg is guarded statically by `binding_lint` (both call-site bindings, on every instance). It needs Slint element debug info, which `build.rs` emits only for non-release profiles, so run it with the default (debug) profile: `cargo test` works, `cargo test --release` cannot find the widgets.

`src\tests\save_flow.rs` continues in the same window (the testing backend is a process-global, single-threaded platform, so all e2e phases share ui_bindings' single `#[test]`): it wires the real Models + Integrations tab callbacks via a test-only seam in `gui.rs` and exercises save → reload → reselect → re-baseline, Revert, delete's clear-selection, the rename and clone funnels, the discard-confirm guard on a dirty form (New…, and Rename…'s Rust-routed entry point), the New…-dialog's id de-conflict (a second New… on the same model must yield `<id>-2`, not overwrite), and the Integrations row-checkbox rebuild invariant (a clicked delegate's broken one-way binding must be replaced — not `set_row_data`-patched — by `refresh_integrations`). All config IO is redirected at a temp dir through the `LLAMA_CPP_CONFIG_DATA_ROOT` env var (see `paths::data_root`), so the flow never touches the user's real `%LOCALAPPDATA%\llama.cpp` — and because that env var is process-wide and never restored, unit tests elsewhere must never touch `paths::`.

`src\tests\binding_lint.rs` is a plain text scan (its own `#[test]`, no Slint backend): it walks every `ui\*.slint` and fails on a one-way `prop: AppState.…` binding on a self-assigning widget property (LineEdit/TextEdit `text`, CheckBox/Switch `checked`, SpinBox/Slider `value`, ComboBox `current-*`, and the `DefaultSpinBox`/`DefaultLineEdit` composites' `value` + `default`), honoring the sanctioned escapes above (`<=>`, `read-only: true`, non-AppState expressions, custom components). ui_bindings proves each widget *kind* behaves; this catches the *new instance* someone wires one-way.
