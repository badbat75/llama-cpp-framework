# Configurator inventory for the web UI (pai-config / pai, through pai-bff)

Read-only research, 2026-11-14, on the working tree before commit 4a7994e. Nothing was edited or run.

**Moving tree.** Another session is editing the tree right now. Its work in progress is the resolution of ModelsDir under the Windows manager: `server_cfg::effective_models_dir`, `seeds_models_dir` and `models_dir` in `crates/app/src/server_cfg.rs`, which are still `todo!()` with red tests, the callers in `gui.rs`, `gui/*.rs`, `hf.rs`, `presets.rs` and `cli.rs`, and the new key `download-folder-not-writable`. Line numbers in those files may drift.

**Conventions.**
- Every rule below was read in code at the location given, unless it is marked **(inference)** or **(doc only)**.
- **Layer** says where a rule lives:
  - **core**: `crates/core` (pai-core). The manager, the CLI and the GUI all link it.
  - **app**: `crates/app` (pai-app). Shared by the GUI and the CLI. The bff does not depend on it today: `crates/bff` links pai-core only (deps lint at `crates/bff/src/tests/deps_lint.rs`).
  - **manager**: `crates/manager`.
  - **GUI**: `crates/config`, Rust or `.slint`. A rule here cannot be reused by the bff as it stands.
- Short paths used throughout:

  | Short | Path |
  |---|---|
  | `CK` | `crates/core/src/checks.rs` |
  | `CSC` | `crates/core/src/server_cfg.rs` |
  | `ASC` | `crates/app/src/server_cfg.rs` |
  | `SC` | `crates/app/src/server_control.rs` |
  | `P` | `crates/app/src/presets.rs` |
  | `F` | `crates/config/src/form.rs` |
  | `SF` | `crates/config/src/server_form.rs` |
  | `MT` | `crates/config/src/gui/models_tab.rs` |
  | `BT` | `crates/config/src/gui/bench_tab.rs` |
  | `G` | `crates/config/src/gui.rs` |
  | `MP` | `crates/config/ui/models_page.slint` |
  | `SP` | `crates/config/ui/server_page.slint` |
  | `ST` | `crates/config/ui/state.slint` |
  | `AS` | `crates/config/ui/app.slint` |
  | `CO` | `crates/config/ui/components.slint` |
  | `SV` | `crates/manager/src/manager/service.rs` |

---

## 0. The data path in one picture

- **The backend seam.** Every read and write of `server@default.ini`, `presets.ini` and `presets.ini.disabled` goes through `SC` `Backend`:
  - Manager on Linux and Windows; Direct on macOS (`SC:625 backend_kind`).
  - `read_config`/`write_config` are at `SC:201/214`. Under the Manager they call `ReadFile` and `WriteFile`. Under Direct they read and write the per-user files atomically, **with no content checks** (`SC:214-221`).
- **The fixed instance.** Every call names the instance `default` (`SC:14 INSTANCE`, `ConfigFile::manager_name` `SC:134-140`).
- **Full-text writes.** The client rewrites whole file texts:
  - server.ini comes from `render_for` (`CSC`). It is fully generated; hand-edits do not survive.
  - presets.ini comes from `P` `render_section` (P:1715-2095), splicing one section and keeping the other sections verbatim.
  - The manager then checks only the sections that changed (`CK:375-392`).
- **What the bff has today.** It calls only `ListInstances` and `Status` (`crates/bff/src/lib.rs:71-95`). It runs as `private-ai-bff`:
  - Linux: a hardened system user (`ProtectHome`, `ProtectSystem=strict`, `xtask/src/linux/system_files.rs:150-182`).
  - Windows: `NT SERVICE\private-ai-bff`, admitted to the pipe for the unauthenticated calls only (`crates/manager/src/manager/windows.rs:22-30`).

---

## 1. Inventory per tab and window

### 1.1 Server tab (`SP`, `G`/`gui/server_tab.rs`, `SF`, `CSC`, `ASC`)

**File and saving.**
- File: `server@default.ini` `[Server]` (Direct: per-user `server.ini`).
- The form is `ServerForm` (`ui/types.slint`). `server_dirty = server_form != server_form_base` (ST:86). Save is enabled only when dirty (ST:89, also Ctrl+S AS:163).
- Save handler (`gui/server_tab.rs:20-60`):
  1. `server_form::invalid_numbers` (GUI).
  2. `form_to_config` (GUI).
  3. `ASC::save`, which runs `validate_for_save` (core), then `check_startup_count` (app), then `write_config` (the manager checks again), then `create_dir_all(models_dir)` unless the folders are left unset.
- Revert reloads the file.
- Status after a save: `server-saved`, or `server-saved-restart` while the server is running (a save never touches the running process).

| Card / field (SP line) | INI key | Type, widget | Default when unset | Range / values, where enforced | Dependencies |
|---|---|---|---|---|---|
| Network / Port (SP:47-56) | `Port` | int, `DefaultLineEdit` number + "default" box | omitted; llama.cpp 9931 (`CSC DEFAULT_PORT`; rendered as a hint line) | 1..=65535: GUI `SF::invalid_numbers` (SF:98, range from `CK SERVER_INT_KEYS` CK:148); manager `CK::server_file` CK:409; render keeps `n>0` only (0/negative become unset) | none |
| Network / Bind to (SP:58-75) | `Hostname` | comma list, one checkbox per address from `net_ifaces::interfaces()` (localhost, 0.0.0.0, each detected IPv4) | `localhost` (`hostname_or_default`); always written | `0.0.0.0` exclusive: `net_ifaces::toggle` (core `crates/core/src/net_ifaces.rs:58`), rows disabled while it is ticked (`build_rows` :137-166). Manager: every host must resolve (`UnresolvableHost`) and must not collide with another instance's port+address (`AddressInUse`) (SV:461-477, 288-308). Render canonicalizes `a,b` (`join_hosts`) | interface list read on the **local** machine |
| Network / Base URL (SP:77-86) | `OpencodeBaseUrl` | text + "Default" box | unset = `http://<client_host>:<port>` (`CSC opencode_base_url_or_default`; `client_host` maps 0.0.0.0 to localhost) | no `;`/`#` (core `validate_for_save` + manager `SERVER_TEXT_KEYS` CK:183); the GUI strips a trailing `/` and `/v1` (SF `form_to_config`, GUI only) | drives the Integrations tab and Open chat UI |
| Network / API key (SP:88-101) | `OpencodeApiKey` | text + "No key" box (field disabled while ticked) | unset = none | no `;`/`#` (core + manager) | Integrations; makes `ReadFile(server@<id>.ini)` need the `control` permission (SV:389-404) |
| Resources / CPU threads (SP:109-118) | `Threads` | int, `AutoSlider` 0..`cpu_threads_max` + "auto" box | omitted | slider max = this machine's logical CPU count (`G:305`), GUI only; render keeps `n>0` | **local** CPU count |
| Resources / Batch threads (SP:120-128) | `ThreadsBatch` | same | omitted | same | same |
| Resources / Max models (SP:130-138) | `ModelsMax` | int, `DefaultLineEdit` + box | omitted; llama.cpp 4 (`CK::LLAMA_MODELS_MAX_DEFAULT` CK:99); hint shows 4 | any i32 (GUI, manager); render keeps `n>=0`. Startup count: `load-on-startup` presets ≤ ModelsMax, none when ModelsMax=0 (app `ASC:24-31` via `P::check_startup_count`; manager `CK::server_file` CK:419-420) | presets' `load-on-startup` |
| Resources / Memory margin (SP:140-148) | `ModelsMemoryMargin` | int MiB, `DefaultLineEdit` + box | omitted; llama.cpp 0 = off | 0..: GUI and manager; CLI turns a negative into unset | none |
| Resources / Models folder (SP:150-162) | `ModelsDir` | path + Browse (native dialog, local) | unset → `~/.llama.cpp` of the **process user** (`CSC default_models_dir`), written resolved, except under the Windows manager where it stays unset (`SC:311 dirs_left_unset`, `render_for`) | no `;`/`#`: core `validate_for_save` checks the RESOLVED value; manager checks the explicit one | model scans, downloads and HW-monitor disk all follow the SAVED value |
| Advanced / Web UI (SP:169-176) | `Webui` | bool checkbox | true; always written | none | off disables the MCP proxy toggle (SP:185) and Open chat UI; launch adds `--no-ui` |
| Advanced / Web UI MCP proxy (SP:178-187) | `WebuiMcpProxy` | bool | false; always written | none | `enabled: webui`; the value is kept but not passed while the UI is off |
| Advanced / Log level (SP:189-198) | `LogVerbosity` | `EnumComboBox` over `Options.log_levels` (CO:119: OUTPUT:0 … DEBUG:5) | 4; always written | GUI clamps to 0..5 (`SF:48 log_level_label`); **no check in core, manager or CLI** (CLI `--log-verbosity` takes any i32) | none |
| Snapshots / Save on shutdown (SP:210-217) | `SaveStateOnShutdown` | bool | false; always written | none | enables the folder field |
| Snapshots / Snapshot folder (SP:219-233) | `StateDir` | path + Browse | unset → `data_root()/state` of the **process user** (`crates/core/src/slot_state.rs`), written resolved (except the Windows manager) | no `;`/`#` on the explicit value (core, manager) | `enabled: save_state_on_shutdown` |
| SDK tuning / ROCm warning (SP:272) | none | read-only `devices::rocm_warning` | none | none | local device probe |
| SDK tuning / hipBLASLt (SP:278-291) | `RocblasUseHipblaslt` | tri-state default/on/off (`Options.tri_states`) | unset = never export | parse_bool; unknown = unset | env `ROCBLAS_USE_HIPBLASLT` |
| SDK tuning / Profiler attach (SP:293-300) | `RocprofAttach` | bool | false (hint line) | none | env `ROCP_TOOL_ATTACH=1` |
| SDK tuning / Tensor-split AllReduce (SP:309-317) | `CudaAllreduce` | enum default/nccl/internal/none (`Options.allreduce_modes` CO:111) | unset | closed list `CSC ALLREDUCE_MODES`, normalized on read, in the form and in the CLI (clap possible values); an unknown value in the file reads as unset; **the manager does not check it** | the Models split-mode strip reads it (`split-tensor-mixed-backends`) |
| SDK tuning / Disable GPU graphs (SP:319-326) | `CudaDisableGraphs` | bool | false | none | env `GGML_CUDA_DISABLE_GRAPHS=1` |
| SDK tuning / KV rotation off (SP:335-342) | `AttnRotDisable` | bool | false | none | env `LLAMA_ATTN_ROT_DISABLE=1` |
| Command line (SP:354-395) | none | read-only render of `launch::server_args` + `env_vars` | none | none | follows the SAVED file |

**What the README leaves out.** The README's Server section does not mention `CudaDisableGraphs` or `AttnRotDisable`. Both exist in the schema (CSC), in the GUI (SP:319-342) and in the CLI (`--cuda-disable-graphs`, `--attn-rot-disable`).

### 1.2 Models tab (presets.ini)

The full field table (around 70 fields) follows: card, label key, INI key, type, widget, new-preset value, range and where it is enforced, and the enable expression.

**Enforcement codes:**
- (f) = GUI `F::invalid_numbers` (F:732), ranges from `CK::PRESET_INT_KEYS`;
- (m) = manager `CK::preset_section` (CK:351);
- (v) = app `P::validate_for_save` (P:1466).

**General rules:**
- A key is written only when non-empty or `Some`. Exceptions: `fit`, `jinja`, `mmproj-offload`, `reasoning` and `reasoning-format` are always written; `load-on-startup`, `embeddings` and `no-host` are written only when true; `prefill-assistant` only when false.
- `; url =` and `; placement =` are comments, not keys (P:899-922).
- A new preset (`Preset::default` P:675-760) writes only `jinja=true`, `mmproj-offload=true`, `reasoning=auto`, `reasoning-format=auto` and `fit=off`.
- Numeric fields are text plus a "default" box. Layer counts are `AutoSlider`s. Tri-states are `default/on/off` `SegmentedControl`s.

| Card | Label | INI key | Type / widget | New preset | Range / values (enforced where) | Enable / visibility |
|---|---|---|---|---|---|---|
| list | on/off switch | which file holds the section | bool | on | see 2.2 | always |
| header | id | `[id]` | string; Rename only | `new_preset_id` P:1657 | `[A-Za-z0-9._-]+` (v, m); unique case-insensitively across both files (app de-conflict; manager within one file only) | |
| Assets | Model file | `model` | path, combo over the `models/` scan | picked | non-empty (GUI MT:140, m CK:353); no `;`/`#` (v, m) | |
| Assets | Project URL | `; url =` | text + Open | "" | single line (v P:1481) | Open only for http(s) (MP:217) |
| Assets | MMProj | `mmproj` | combo over the `mmprojs/` scan | "" | no `;`/`#` (v, m) | |
| Assets | Draft model | `model-draft` | combo over `mtps/` + `dflashs/` | "" | no `;`/`#` (v, m) | pick sets spec-type and the draft pin (2.3) |
| Assets | Drafter | `spec-type` `draft-*` entry | combo none/draft-mtp/draft-dflash/draft-dspark/draft-simple | "" | none | `enabled: draft_active` (ST:343 = `model_draft!="" \|\| embeds_mtp`) |
| Assets | N-gram drafters | `spec-type` `ngram-*` entries | 4 checkboxes (mod, simple, map-k, map-k4v); `ngram-cache` kept untouched | none | none | always |
| Model info | read-only | none | GGUF reads (libggml-base, **local**) | none | none | `model_info_ready` etc. |
| GPU distribution | placement note | `; placement =` | text + Clear | "" | single line (v P:1487) | shown if non-empty |
| GPU distribution | split mode | `split-mode` | combo default(=layer)/none/tensor/row (`gpu_split.rs:191`) | "" | not checked (m, v); unknown values kept | `enabled: selected != 1` (CO:~948) |
| GPU distribution | device rows | `device` | ordered CSV of ids: checkbox + drag handle per probed non-CPU device; "not detected" rows kept | "" | not checked | probe is **local** `llama-server --list-devices` |
| GPU distribution | weights / blocks | `tensor-split` | weights or block sliders, Auto / Even | "" | not checked; junk reads as 0 (`gpu_split.rs:239`) | weights editable when ≥2 checked and explicit; blocks under layer mode with a known model (CO:~854-859, ~1242-1249) |
| Tensor placement | rule rows | `override-tensor` | kind / layers / device (probed + CPU + custom) / regex / picker | "" | pattern and device per rule (v, m `CK::override_tensor`); `,`/`=` stripped from typed patterns (`tensor_override.rs:395`); layers ≤999, ≤256 (:314) | Choose… needs model tensors |
| Hardware | GPU layers | `n-gpu-layers` | AutoSlider 0..`n_layer` (or 99) | unset | UI only; **not in `PRESET_INT_KEYS`**, so no manager check | |
| Hardware | MoE CPU layers | `n-cpu-moe` | AutoSlider | unset | none | `usable: has_moe` |
| Hardware | MoE cache MiB | `moe-cache-mib` | int | unset | ≥0 (f, m); >0 refused with split-mode tensor (v, m); >0 below the load floor refused (**GUI only** MT:117-133) | `enabled: has_moe` |
| Hardware | Dense FFN CPU layers | `n-cpu-ffn` | AutoSlider | unset | none | always |
| Hardware | Op offload / Repack | `op-offload` / `repack` | tri | unset | none | |
| Hardware | No host buffer | `no-host` | bool (true only) | false | none | |
| Hardware | Lazy tensors | `lazy-mode` | default/on/auto/off | "" | not checked | |
| Hardware | Load mode | `load-mode` | combo default/auto/none/mmap/mlock/mmap+mlock/dio (CO:102) | "" | normalized on read and in the form (`normalize_load_mode`); not checked by m/v | |
| Hardware | Fit to VRAM | `fit` | bool, always written | off | forced off unless `fit_allowed` (ctx-size AND n-gpu-layers both default; P:2130, F, ST:127, AS:138-143): **GUI form only** | |
| Hardware | Fit min context | `fit-ctx` | int | unset | ≥1 (f, m) | `enabled: fit` (value kept) |
| Hardware | Fit free VRAM | `fit-target` | MiB CSV | "" | digits/commas (v, m) | `enabled: fit` |
| Hardware | Draft device | `device-draft` | combo auto + probed + custom | "" | none | `enabled: draft_file_active` (ST:352) |
| Hardware | Draft GPU layers | `n-gpu-layers-draft` | AutoSlider, or on-GPU(99)/on-CPU(0) for a blockless MTP head | unset | none | `draft_file_active` |
| Resource | Context size | `ctx-size` | int | unset | ≥1 (f, m), so `ctx-size = 0` cannot be written | |
| Resource | Parallel | `parallel` | int | unset | ≥1 (f, m) | |
| Resource | Unified KV | `kv-unified` | tri | unset | none | `enabled: !parallel_default` (value kept) |
| Resource | Per-slot context | `kv-unified-per-slot` | int | unset | ≥1 (f, m) | |
| Resource | Batch / U-batch | `batch-size` / `ubatch-size` | int | unset | ≥1 (f, m); ubatch ≤ batch not checked | |
| Embeddings | Embedding mode | `embeddings` | bool (true only) | false | none | |
| Embeddings | Pooling | `pooling` | default/none/mean/cls/last/rank | "" | not checked | |
| KV cache | K / V type | `cache-type-k/-v` | combo (CO:95: default,f32,f16,bf16,f8_e4m3,q8_0,q5_1,q5_0,q4_1,q4_0,iq4_nl) | "" | warning strip only (`kv_cache::warning` kv_cache.rs:207) | |
| KV cache | Flash attention | `flash-attn` | tri | unset | none | |
| KV cache | Cache RAM | `cache-ram` | int | unset | any (f, m) | |
| KV cache | Cache reuse | `cache-reuse` | int | unset | ≥1 (f, m) | |
| KV cache | Ctx checkpoints / checkpoint step | `ctx-checkpoints` / `checkpoint-min-step` | int | unset | ≥0 (f, m) | |
| Chat | Chat template | `jinja` | bool, always | true | none | |
| Chat | Prefill assistant | `prefill-assistant` | bool (false only) | unset | none | |
| Chat | Reasoning | `reasoning` | auto/on/off, always | auto | not checked | |
| Chat | Reasoning format | `reasoning-format` | auto/none/deepseek, always | auto | not checked | |
| Chat | Reasoning effort | `reasoning-effort` | default/minimal/low/medium/high/xhigh/max | "" | not checked | |
| Chat | Keep past thinking | `reasoning-preserve` | tri | unset | none | |
| Chat | Thinking budget | `reasoning-budget` | int | unset | any (f, m) | |
| Chat | Budget message | `reasoning-budget-message` | text | "" | no `;`/`#` (v, m) | |
| Chat | Max output | `n-predict` | int | unset | any (f, m) | |
| Sampling | temp, top-p, min-p, repeat-penalty, presence-penalty | same names | float text | unset | **none**: unparseable text becomes unset silently (`ini::parse_float` accepts `,`) | |
| Sampling | Top-K | `top-k` | int | unset | any (f, m) | |
| Sampling | Backend sampling | `backend-sampling` | tri | unset | none | |
| Advanced | Load on startup | `load-on-startup` | bool (true only) | false | startup count (2.2) | |
| Advanced | Template kwargs | `chat-template-kwargs` | raw JSON text | "" | no `;`/`#` (v, m); JSON not validated | |
| Vision | Vision on GPU | `mmproj-offload` | bool, always | true | none | `enabled: mmproj!=""` |
| Vision | Vision GPU | `mmproj-device` | combo default + probed | "" | none | `mmproj!="" && mmproj_offload` |
| Vision | Min / Max image tokens | `image-min-tokens` / `image-max-tokens` | int | unset | ≥1 (f, m); min ≤ max not checked | `mmproj!=""` |
| Speculative | Draft n-max | `spec-draft-n-max` | int | unset | ≥1 (f, m) | `draft_active` |
| Speculative | Draft p-min | `spec-draft-p-min` | float | unset | none | `draft_active` |
| Speculative | Draft n-min / n-ubatch | `spec-draft-n-min` / `-n-ubatch` | int | unset | ≥0 (f, m) | `draft_active` |
| Speculative | Draft K/V type | `spec-draft-type-k/-v` | combo | "" | warning only | `draft_active` |
| Speculative | ngram-mod n-match / n-min / n-max | `spec-ngram-mod-*` | int | unset | 1..1024 / 0..1024 / 0..1024 (CK:104-106; f, m) | `spec_type_has(..,"ngram-mod")` |
| Speculative | ngram-simple, map-k, map-k4v size-n / size-m | `spec-ngram-<t>-size-n/-m` | int | unset | 1..1024 (f, m) | that type ticked |
| Speculative | ngram-map-k4v min-hits | `spec-ngram-map-k4v-min-hits` | int | unset | 1..65535 (f, m) | `ngram-map-k4v` ticked |
| Footer | Save / Revert | none | none | none | Save: `selected>=0 && id!="" && preset_dirty` (ST:131) | |

**List actions** (MP:119-152): New… (always enabled); Clone…, Rename… and Delete (need a selection); a filter field. Switched-off rows are muted and listed after the enabled ones.

### 1.3 Models tab: derived values and dependencies (all GUI form or app helpers)

- **fit.** `fit_allowed(ctx_default, ngl_default)` (P:2130 app; Slint mirror ST:127). It is applied in `preset_to_form` / `form_to_preset` and unticks the box live (AS:138-143). `render_section` writes `p.fit` as given, so only the GUI enforces it.
- **Draft pick** (`draft_picked` MT:466-505).
  - The spec comes from the file's header (`gguf::draft_spec_type` gguf:697), with the folder as fallback.
  - `apply_draft_pick` (MT:882) sets `model-draft` and `spec_type_with_drafter` (P:967), keeping the n-gram entries.
  - When the draft is not yet configured it pins `device-draft` to `draft_pin_device` (MT:912: the first device, or the LAST device with a share for dflash/dspark under layer mode) with `n-gpu-layers-draft=99`, or `0` with no device.
  - `apply_draft_pick` and `draft_pin_device` are GUI code. A web UI needs them moved to app or re-implemented.
- **spec-type list helpers.** `spec_type_drafter`, `spec_type_has`, `spec_type_with_drafter`, `spec_type_with_ngram` (P:933-990, app).
- **Prunes** (app functions, called from the GUI):
  - `prune_inactive_draft_keys` (P:1022);
  - `prune_inactive_moe_cache` (P:1083);
  - `prune_unused_ngram_keys` (P:1092).
  - When they run: on model-info refresh (MT:1200-1227, which marks the form dirty with `save-to-persist` + `dropped-*`); at Save (MT:114-135; the draft/MoE prunes only if `model_info_ready`); and on Clone (MT:1351-1376, which also drops a below-floor MoE cache: `clone-dropped`).
  - New, download, CLI and bench sweep writes do not prune (inference from the callers).
- **GPU table** (app `crates/app/src/gpu_split.rs`):
  - parse/render :262; `toggle` :418; `move_by` :438; `set_weight` :459; `set_blocks` :495; `set_auto` :551; `set_even` :557.
  - Blocks: `Layout` :305, `block_counts` :358, `block_ranges` :388.
  - The Slint row building and the share strings `unused` / `even` / `100%` are GUI (`crates/config/src/gpu_split.rs:36-108`).
- **Tensor override rows** (app `tensor_override.rs`):
  - `KINDS` :96-204, `parse_layers` :314, `add` :405 (seeds `^token_embd\.weight` on the first GPU), `device_options` :532 (includes the CPU), `validate`.
  - The picker is `tensor_picker.rs` (app).
  - The per-row problem text is GUI (`crates/config/src/tensor_override.rs:47`).
- **Warning strips:**
  - KV: `kv_cache::warning` (app).
  - Split mode: composed in Slint (ST:159-167) from `gguf::sm_tensor_warning` (app), `flash_attn=="off"`, `device==""` and `mixes_cuda_and_rocm` (app) with the server's `CudaAllreduce`. **The composition and its order are GUI only.**
  - Embeddings pin: `gguf::embd_pin_warning` (app), shown when `pins_embeddings_to_gpu`.
  - MoE floor hint: `moe_cache::floor().hint()` (app).

### 1.4 Benchmark tab (`crates/config/ui/bench_page.slint`, `BT`, `crates/app/src/bench*.rs`)

**Fields and defaults** (seeded at the first refresh, BT:444-475):

- **Engine.** live or synthetic; default live. Caveats come from `bench::caveats` (English on purpose: they are stored in reports).
- **Presets.** An ordered multi-select of ENABLED presets (`presets::load_all`). Position 1 is the baseline (BT:1434-1457). Run is disabled with none selected.
- **Live half** (enabled only for Spec = custom or under Override):
  - **Prompts.** A checkbox list: the shipped `*.txt|*.json` in `paths::bench_prompts_dir()` plus `BenchPromptFile` and added files. Per row: Edit (desktop editor), Remove (non-shipped rows only), and a readout of chars, bytes and sha256 of the normalized text.
  - **Temperature.** Float, or "Preset" (None). Default 0.
  - **Max output tokens.** Default 256; 0 = no cap.
  - **Image.** Optional jpg/png.
  - **Image size.** `WxH` or `original`; default 1344x896.
  - **Concurrency.** single, prefill, decode, prefill+decode or all.
- **Synthetic half:** prompt lengths `2048,8192,32768`; Generate 128; depths `0,8192,32768`.
- **Repetitions:** 3.
- **Campaign:**
  - **Spec.** `custom`, then `standard-<v>` newest first, then `file: <path>`.
  - Load… and Save as….
  - **Override:** a snapshot diff that becomes a `spec::Derive`.
  - **Name** and **Question:** free text, unchecked.
  - **A/B key:** `none` or a `bench::sweep::SWEEPABLE` key (sweep.rs:119).
  - **Values.**
- **Saved runs:**
  - **Database** field (`BenchDb`): Browse; the readout says how many campaigns it holds.
  - **Run** combo: `db::campaigns`.
  - **Open report:** writes `$TMP/pai-config-<name>.md` and opens it.

**Checks at Run, in order** (`on_bench_run` BT:1705-1784):

| # | Rule | Location | Key | Layer |
|---|---|---|---|---|
| 1 | Already running: no-op | BT:1709 | none | GUI |
| 2 | `invalid_numbers(mode)`: reps ≥1; live max-tokens ≥0, image size format, image is a file; synthetic lists non-negative and n_gen ≥0. Skipped for live + spec + no Override | BT:689-732 | `not-run` + `number-*`, `image-size-format`, `image-not-a-file`, `list-entry-not-number` | **GUI** composition (helpers in core/app); `image-not-a-file` GUI only |
| 3 | DB path persisted; `;`/`#` blocks Run | BT:425, :1717; `settings.rs:186-187` | `database-not-saved` | GUI flow over an app check |
| 4 | Ticked prompts re-read fresh; unreadable or empty refused; none ticked refused | BT:338-350, :1730-1744 | `prompt-unreadable`, `prompt-empty`, `tick-a-prompt` | app loader; `tick-a-prompt` GUI only |
| 5 | `bench::validate(plan)` | `bench.rs:525-571` | `select-a-preset`, `reps-at-least-one`, `live-needs-prompt[-empty]`, `max-tokens-negative`, `concurrent-needs-live`, `give-a-length` | app (shared with the CLI) |
| 6 | Spec load/parse; spec runs live only; image prompts only on presets with an mmproj | `spec.rs:166-314`, `:711-765` | `unknown-spec`, `spec-*`, `spec-runs-live` | app |
| 7 | A/B: values text non-empty and ≥2 values (GUI); `parse_values` (`;` or `,`, `a..b` ≤64) and `check_values` per selected preset (app) | BT:1163-1176; `sweep.rs:340`, `:407-440` | `ab-type-values`, `ab-needs-values` (GUI); `values-empty`, `range-*`, `not-sweepable`, `key-value-refused` (app) | split |
| 8 | Concurrency `all`: the scenarios every preset supports (`parallel` ≥2, absent = 4) | `sweep.rs:1135` | none | app |
| 9 | Synthetic with the server up: the GUI stops it (snapshot save included) | BT:1766-1782 | `stop-server-before-synthetic` | GUI (CLI fails instead) |
| 10 | GPU lock: refused if held or (Linux) other holders of `/dev/kfd` or `/dev/nvidia*` | `gpu_lock.rs:80-92` | `gpu-lock-held`, `gpus-in-use` | app (GUI `try_acquire` refuses; CLI `acquire` waits) |
| 11 | Worker: server up for live, down for synthetic; preset still present; DB writable | `exec.rs:336-410` | inline English (exec.rs:370, 377, 394; bench.rs:913) | app; violates "no text in code" |
| 12 | Steps write pass and A/B overrides through `presets::save` (manager `configure`) and restart the server | `exec.rs:120-216`, `spec.rs:845-862` | `override-not-written`, `override-not-restored`, `cannot-restart-server` | app |

**Save as…** (BT:828-905): the name must not be empty, `standard` or `custom` (`spec-name-taken`); `invalid_numbers(Live)` must pass; a user image is refused (inline English BT:849); the written text must parse back. All GUI.

### 1.5 Settings tab (`crates/config/ui/settings_page.slint`, `gui/settings_tab.rs`, `crates/app/src/settings.rs`, `crates/app/src/startup.rs`)

There is no Save button: every control applies on change, read-modify-write (settings_tab.rs:63-80). On failure the footer shows `settings-save-failed` and the control rolls back. The file is `config_dir()/settings.ini` `[Settings]`, **per user**.

| Control / key | Type, default | Validation | Notes |
|---|---|---|---|
| Start at login (no key: the entry is the state) | bool | none | Linux `~/.config/autostart/org.privateai.Config.desktop`; Windows HKCU Run `org.privateai.Config` (`startup.rs:154-169, 337-341`). Disabled where unsupported. `startup-change-failed` |
| Start minimized (no key) | bool | none | `enabled` only while the login entry exists |
| Start llama-server at boot (no key) | bool | manager `configure` | Manager only. `toggle_boot` (SC:279): a refusal restores the manager's re-read state. `start-at-boot-on/off` |
| `StartServerOnLaunch` | bool, false | none | shown only on Direct (macOS); `start_at_launch` (SC:301) |
| `LogRotate` | bool, true | none | enabled only when `rotates_log_file()` (Direct); otherwise greyed, value kept, note `log-rotate-journald` / `log-rotate-service` (SC:657) |
| `LogRotateKb` | u32, 1024 | **GUI only**: `commit_log_rotate_kb` (settings_tab.rs:199-233); empty restores the stored value, bad text `not-a-size-in-kb`; the app parser reads bad values as the default (settings.rs:138-142) | same gating |
| `BenchPromptFile` | path, "" = shipped 43k prompt | no `;`/`#` (app settings.rs:186) | written by the Bench Run (first ticked prompt) |
| `BenchDb` | path, "" = `data_root()/bench/bench.db` | no `;`/`#` (settings.rs:187) | Bench Database field |
| `HwMonitorHidden` | CSV | none | HW Monitor checkboxes |
| `MainWindowSize`, `LogWindowSize`, `HwMonitorWindowSize`, `DownloadsWindowSize` | `WxH`, each side 100..16384 | parse only | not meaningful on the web |

There is no settings editor in the CLI.

### 1.6 Integrations tab (`gui/integrations_tab.rs`, `crates/app/src/integrations.rs`)

- **OpenCode.**
  - Status line: drift between the enabled presets and the file.
  - Save calls `opencode_plan` (integrations.rs:163). An empty plan gives `opencode-up-to-date`. Otherwise a preview with one ticked row per change (the key is masked); unticked rows are `keep`. `save_opencode_models` (:227) writes `~/.config/opencode/opencode.json` of the **process user**, only on change, preserving everything outside `provider.llama.cpp.models`.
  - Open: opens the folder.
- **Claude Code:** a read-only env snippet (`claude_code_env_script` :391).
- **Inputs:** server.ini `OpencodeBaseUrl` (`/v1` appended) and `OpencodeApiKey`; enabled presets; the GGUF trained context (`effective_ctx`, :630; the local GGUF read); `fit_may_cut_ctx` omits `limit`.
- The CLI has no integrations command.

### 1.7 Hugging Face dialog and Downloads window (`gui/hf_dialog.rs`, `gui/downloads.rs`, `crates/app/src/hf.rs`, `crates/app/src/hf_fit.rs`)

**Inputs:**
- One field, classified by `hf::classify_input` (hf.rs:1153) as a search or a spec.
- A typed token: masked, never stored. Empty means `hf::token_from_env()` (`HF_TOKEN`, `HF_TOKEN_PATH`, `$HF_HOME/token`, `~/.cache/huggingface/token`, hf.rs:529-563).
- A candidate row; an mmproj tick; a drafter pill (none/mtp/dflash/dspark).
- Placement estimates: curl range-GETs of the remote headers against `devices::probed()` and system RAM (local).

| Rule | Location | Key | Layer |
|---|---|---|---|
| Spec/URL/repo id/quant tag forms | hf.rs:408-493 | `enter-a-repository`, `not-a-hf-url`, `url-not-a-model`, `no-repo-in-url`, `not-a-repo-id-expected`, `not-a-quant-tag` | app |
| Query non-empty, no control characters, ≤200 characters | hf.rs:1167-1184 | `enter-a-search`, `search-control-characters`, `search-too-long` | app |
| Token: printable ASCII, no `"` or `\` | hf.rs:512-523 | `token-bad-characters` | app |
| One request at a time; stale answers dropped | hf_dialog.rs:142 | none | GUI |
| llama-server found | hf.rs:1825 | `server-not-found-for-download` | app |
| **One active download per repository** (case-insensitive) | downloads.rs:278-282, :555; Resume :367-375 | `download-busy`, `resume-busy` | **GUI only** (in-process; a CLI run is not guarded) |
| Sidecar exists; index in range | hf.rs:1855-1866 | `hf-sidecar-missing`, `hf-no-match` | app |
| ModelsDir writable (uncommitted) | hf.rs:1804-1821 | `download-folder-not-writable` | app |
| Repo path safety, target under `<ModelsDir>/<subdir>` | hf.rs:1885-1950 | `not-a-repo-id`, `refused-repo-path` | app |
| Existing target with a different size refused (shared mmproj name) | hf.rs:2403-2413 | `hf-target-conflict` | app |
| Preset save: `validate_for_save`, id re-derived at save | hf.rs:2895-2904 | `hf-preset-not-saved` | app |
| Quit with downloads asks; cancel = close stdin (keeps the partial), 15 s grace | downloads.rs:929-1000 | none | GUI |

**CLI `preset download`.** Same app checks, token from the environment only, `--id` checked (`id-flag-invalid`, `id-flag-taken`, cli.rs:1107), fresh device probe, no busy-repo guard.

**Disk and processes:**
- `curl` for HTTPS.
- A `llama-server` child in router download mode with `LLAMA_CACHE=<ModelsDir>/.hf-staging`.
- Files land in `<ModelsDir>/{models,mmprojs,mtps,dflashs}/<org>/<repo>/`, written **as the process user**.

### 1.8 HW Monitor window (`gui/hw_window.rs`, `crates/app/src/hwmon*.rs`, `crates/app/src/devices.rs`)

- Linux and Windows. One sample per second while open.
- Linux sources: sysfs amdgpu, NVML, `/proc/stat`, `/proc/meminfo`, `/sys/block` for the ModelsDir disk.
- Windows sources: PDH, D3DKMT, NVML, `GetSystemTimes`, `GetPerformanceInfo`, `IOCTL_DISK_PERFORMANCE`.
- Static facts: `llama-server --list-devices`, plus this binary re-run as `sdk-devices`, plus PCI sysfs, udev DMI or SMBIOS. No root needed.
- Persists `HwMonitorHidden`. There are no constraints to port.

### 1.9 View-logs window (`gui/log_window.rs`, SC)

- **Sources** (`log_source_for` SC:643-655):
  - Linux: the journal of `llama.cpp@default.service` (`journalctl -u … -f -n <rows> -o cat`). It needs root or the groups `systemd-journal`, `adm` or `wheel`; otherwise `log-journal-denied` (SC:403).
  - Windows: `%ProgramData%\llama.cpp\logs\llama-server@default.log`.
  - macOS: `data_root/logs/llama-server.log`.
- Auto-scroll and Copy all. Bounded rows (`log_window.rs:41-60`).
- One-shot reads (`log_text` SC:529) serve the "server gone" footer and the bench memory record.

### 1.10 Import dialog, run controls, tray, Refresh

- **Import** (`crates/app/src/migration.rs:79-127`):
  - Offered under the Manager when the user has a per-user `server.ini` and the manager lacks one of the three files.
  - It copies, in order, server, presets, disabled; only the missing files; stopping at the first refusal.
  - Keys: `import-wrote`, `import-refused`, `import-nothing`. CLI: `pai config import`.
  - The source is the **user's** `~/.config/private-ai`.
- **Run controls:**
  - Start calls `backend().start()`.
  - Stop: if `SaveStateOnShutdown`, first `slot_state::save_all` (HTTP), then `backend().stop()`.
  - Open chat UI: enabled when running && !stopping && `chat_ui_enabled` (ST:50-64; `launched_url` versus a custom Base URL).
  - Restart exists only in the CLI (`pai control restart`); the GUI has none.
- **Discard guard** `confirm_discard_then` (G:807-819): before preset switch, Refresh, New, Clone, Rename and Show preset. The on/off switch is not guarded.
- **Refresh/F5** (G:931-948): re-reads every file, rescans ModelsDir, and re-probes the status, the version and the devices.

---

## 2. Every constraint and check, by layer

### 2.1 pai-core: `CK` (run by the manager on every write; also called by app)

| Rule | Function | Key |
|---|---|---|
| Integer in range, for the 29 `PRESET_INT_KEYS` and `SERVER_INT_KEYS` (`Port` 1..65535, `ModelsMax` any, `ModelsMemoryMargin` ≥0) | `whole_number` CK:186; tables CK:110-152 | `not-whole-number-in-range` |
| No `;`/`#` in the preset text keys (model, mmproj, model-draft, chat-template-kwargs, override-tensor, reasoning-budget-message, fit-target) and the server text keys (ModelsDir, StateDir, OpencodeBaseUrl, OpencodeApiKey) | `comment_markers` CK:197; CK:172-183 | `comment-markers` |
| Preset id `[A-Za-z0-9._-]+` | `preset_id` CK:214 | `invalid-preset-id` |
| Preset has a `model` | `preset_section` CK:353 | `preset-no-model` |
| `fit-target` MiB list | `fit_target` CK:225 | `fit-target-format` |
| `override-tensor` rules have a pattern and a device | `override_tensor` CK:240 | `override-no-pattern`, `override-no-device` |
| `moe-cache-mib` > 0 with split-mode tensor | `moe_cache_split` CK:259 | `moe-cache-with-tensor-split` |
| load-on-startup count ≤ ModelsMax; none when ModelsMax=0 | `startup_count` CK:271 | `load-on-startup-over-max`, `load-on-startup-unlimited` |
| Duplicate section id (case-insensitive) within one file | `preset_sections` CK:375-392 | `preset-name-taken` |
| File-level entry points | `server_file` CK:401; `presets_file` CK:427 (changed sections only; startup count only when a changed section is flagged); `presets_disabled_file` CK:440 (no count; switching off never refused) | |
| `0.0.0.0` exclusive in the bind list | `net_ifaces::toggle` (`crates/core/src/net_ifaces.rs:58`), a UI helper, not a check | none |
| Instance id `[a-z0-9-]{1,32}`, first character alphanumeric | `paths::valid_instance_id` (`crates/core/src/paths.rs:234-239`) | via the manager's `InvalidId` |

Refusals travel as `Refusal::encode` (key + U+001F + values, CK:47-66). The client turns them back into text with `Refusal::text` (CK:73-93), so the bff can return the key and its values and let `fe/` localize them.

### 2.2 pai-app (shared by GUI and CLI; not yet a bff dependency)

| Rule | Location | Key |
|---|---|---|
| server.ini save: comment markers on the resolved ModelsDir and the explicit StateDir/Base URL/API key | `CSC::validate_for_save` (core, called by `ASC::save` :23) | `comment-markers` |
| server.ini save: startup count against the new ModelsMax | `ASC:24-31` | `load-on-startup-*` |
| preset save: id, comment markers, URL and placement note single-line, fit-target, override-tensor, moe+tensor | `P::validate_for_save` P:1466-1496 | `invalid-preset-id`, `comment-markers`, `project-url-one-line`, `placement-note-one-line`, `fit-target-format`, `override-*`, `moe-cache-with-tensor-split` |
| preset save into presets.ini: startup count against `default`'s ModelsMax | `check_startup_for` P:1527 | `load-on-startup-*` |
| on/off switch: startup count when switching on; section must exist | `set_enabled` P:1333-1341; `move_between` P:1385-1394 | `load-on-startup-*`, `no-preset-named` |
| Rename: empty, unchanged, charset, taken in the other file; same-file clash via `ini::text::rename_section` (core) | P:1572-1620; `crates/core/src/ini.rs:321` | `new-preset-id-empty`, `new-preset-id-unchanged`, `preset-id-charset`, `preset-name-taken`, `section-exists`, `section-not-found` (en only) |
| New/Clone id de-conflict across both files | `new_preset_id` P:1657, `unique_id` P:1634 | silent |
| Delete files: in-use detection across both files; bounded empty-folder removal | `crates/app/src/linked_files.rs:188` `users`, :278 `delete`, :309 `remove_empty_dirs` | `kept-in-use`, `could-not-delete`, … |
| Prunes (draft keys, MoE cache, ngram keys) | P:1022/1083/1092 | `dropped-*` notes (shown by the GUI) |
| `fit_allowed` rule (the function only) | P:2130 | none |
| MoE cache floor arithmetic (data only, no refusal) | `crates/app/src/moe_cache.rs` | none |
| Bench `validate`, spec parsing, sweep values, GPU lock | `bench.rs:525`, `spec.rs`, `sweep.rs`, `gpu_lock.rs` | see 1.4 |
| settings.ini paths: comment markers | `settings.rs:186-187` | `comment-markers` |
| HF input, token, repo and path safety, target conflict | hf.rs (see 1.7) | see 1.7 |

### 2.3 Manager only (`crates/manager`)

- **File names.** Only `presets.ini`, `presets.ini.disabled` and `server@<valid id>.ini` (SV:126-148). Anything else is `BadFileName`.
- **Content checks** (`check_contents` SV:484-508):
  - `server_file` against presets.ini on disk;
  - `presets_file` with **the minimum ModelsMax over all instances** (the client uses `default`'s only);
  - `presets_disabled_file`.
- **Address check** (SV:461-477): every host must resolve (`UnresolvableHost`), and the port plus address must not collide with another instance's, wildcards included (`AddressInUse`). **No client-side equivalent.**
- **Authorization** (SV:368-444, `crates/manager/src/manager/dbus.rs`, `crates/manager/src/manager/windows.rs:71-143`): see section 4.

### 2.4 Checks that exist ONLY in the GUI layer (`crates/config`): not reusable by the bff as they stand

| # | Check | Location | Key | Shared equivalent? |
|---|---|---|---|---|
| G1 | Server Save: integer text fields parse and are in range when "default" is unticked (Port, ModelsMax, ModelsMemoryMargin) | `SF:98 invalid_numbers` (ranges from core) | `not-saved-invalid` + `number-empty`/`number-not` (core `ini::int_problem`) | Manager `whole_number` on the INI text (Linux/Windows). Neither app nor the Direct path checks |
| G2 | Server: Log level clamped to 0..5 (dropdown) | `SF:48` | none | **none**: CLI, core and manager accept any int |
| G3 | Server: Base URL trailing `/` and `/v1` stripped | `SF form_to_config` | none | **none**: the CLI stores the URL as typed |
| G4 | Server: thread sliders capped at the local logical CPU count | `G:305`, SP:115/126 | none | none (CLI any; ≤0 means unset) |
| G5 | Preset Save: integer fields parse and are in range | `F:732 invalid_numbers` | `not-saved-invalid` | Manager only; **not in `P::validate_for_save`**, so not on macOS, `preset download` or bench sweep |
| G6 | Preset Save: model non-empty | MT:140 | `pick-model-before-saving` | Manager `preset-no-model`; not in `validate_for_save` |
| G7 | Preset Save: id non-empty | MT:136 | `preset-id-empty` | core `preset_id` (via v and m) |
| G8 | Preset Save: `moe-cache-mib` below the load floor | MT:117-133 | `moe-cache-below-floor` (en only) | **none** (only the arithmetic is in app `moe_cache.rs`) |
| G9 | `fit` forced off unless ctx-size and n-gpu-layers are both default | F (`preset_to_form` / `form_to_preset`), ST:127, AS:138-143 | none | the function is app P:2130, but no writer other than the form applies it |
| G10 | Draft pick: spec-type from the header, draft device pin, `n-gpu-layers-draft` 99/0 | MT:466-505, `apply_draft_pick` MT:882, `draft_pin_device` MT:912 | none | none (the helpers it calls are in app) |
| G11 | Prunes at model-info refresh and Save; the Clone-time silent drop of a below-floor MoE cache | MT:114-135, :1200-1227, :1351-1376 | `save-to-persist`, `dropped-*`, `clone-dropped` | prune functions in app; when to apply them is GUI |
| G12 | Enable/disable gating of preset fields (draft_active, draft_file_active, has_moe, mmproj, fit, parallel_default, split-mode with one GPU, n-gram tuning groups) | ST:127/343/352/353, MP, CO | none | none; Slint expressions only. Disabled values are kept and written (see 3) |
| G13 | Split-mode warning composition (tensor arch, flash-attn off, no device, CUDA+ROCm mix vs AllReduce) | ST:159-167 | `sm-tensor-warning`, `split-tensor-*` | parts in app; the composition is Slint |
| G14 | New: model picked; Clone: selection and source still present | MT:302, :840, :318 | `pick-model-first`, `select-preset-to-clone`, `clone-source-gone` | n/a |
| G15 | Delete dialog: an in-use file cannot be ticked | MT:971-980 | `in-use-kept` | app re-checks at delete (`linked_files::delete`) |
| G16 | Delete dialog: preset must be on disk | MT:1003 | `preset-not-on-disk` | CLI `preset-not-listed`; `presets::delete` is silent on a missing id |
| G17 | Tensor-override per-row problem text (empty pattern, pick a device, device not detected) | `crates/config/src/tensor_override.rs:47` | `override-empty-pattern`, `pick-a-device`, `device-not-detected` | save-time `CK::override_tensor` |
| G18 | Bench `invalid_numbers` composition per mode, and its skip under a spec | BT:689-732 | `not-run` | parts in app/core |
| G19 | Bench image path `is_file` | BT:709-712 | `image-not-a-file` | the CLI sniffs the format instead |
| G20 | Bench: at least one prompt ticked | BT:1730-1744 | `tick-a-prompt` | none |
| G21 | Bench A/B: values non-empty, ≥2 values | BT:1163-1171 | `ab-type-values`, `ab-needs-values` | none |
| G22 | Bench Save as…: reserved names, no user image, parse-back | BT:828-905 | `spec-name-taken`, inline English | none |
| G23 | Bench: a synthetic run stops the server first | BT:1766-1782 | `stop-server-before-synthetic` | the CLI fails instead |
| G24 | Bench: temperature unparseable → 0.0 (lenient, not a check) | BT:770 | none | the CLI refuses (`temp-not-a-number`) |
| G25 | HF: one active download per repository | downloads.rs:278-282 | `download-busy`, `resume-busy` | none (CLI unguarded) |
| G26 | HF: one request at a time, stale answers dropped | hf_dialog.rs:142 | none | none |
| G27 | Settings: `LogRotateKb` text check and restore | settings_tab.rs:199-233 | `not-a-size-in-kb` | app parser falls back to the default silently |
| G28 | Settings: Start minimized needs Start at login | settings_page.slint:46-57 | none | none |
| G29 | Rename button enabled only when new ≠ old and non-empty | AS:993 | none | app `rename` refuses the same |
| G30 | Save buttons enabled only when dirty (server, preset) | ST:86-89, ST:131 | none | n/a |

---

## 3. Verification: documented vs enforced, GUI vs CLI vs manager

Every item below was read in code.

1. **Model-required and integer ranges are missing from `P::validate_for_save`** (P:1466-1496). They are enforced by the GUI (MT:140, F:732) and by the manager (CK:353, 362-366). So on macOS (Direct, no manager), on `preset download` and on bench sweep writes, an empty model or an out-of-range int is caught only by the manager, and on macOS by nothing.
2. **MoE cache load floor is GUI-only** (MT:117-133), and only when the GGUF was read. The manager, `validate_for_save`, the CLI and bench sweep lack it. Clone drops the value silently instead of refusing (MT:1356-1373).
3. **The `fit_allowed` README claim** ("applied on load and on save", `crates/config/README.md` "Fit to VRAM") holds only in the form. `render_section` writes `fit` as given; bench sweep writes of `ctx-size`/`n-gpu-layers` keep `fit = on`. The README's "tensor-split, Tensor placement, MoE CPU layers and Dense FFN CPU layers must stay empty" with fit on is **documented but not enforced or warned anywhere**.
4. **The startup-count ModelsMax differs.** The client checks against instance `default`'s ModelsMax (P:1527-1534, ASC:24-31). The manager checks presets.ini against the **minimum over all instances** (SV:490-500). With several instances, a GUI-approved save can be refused by the manager. This is consistent but surprising, and the message is the same key.
5. **Server Log level:** the GUI clamps to 0..5 (SF:48). `pai server set --log-verbosity 9` or `-3` is written as given; no core or manager check.
6. **Server Base URL:** the GUI strips `/` and `/v1` (SF `form_to_config`); `pai server set --opencode-base-url https://x/v1` stores `/v1`, and `ensure_provider_section` then appends another `/v1` (`integrations.rs:535`). **(inference: the result is `/v1/v1`.)**
7. **Server Port on the CLI:** no client-side range. `--port 0` or a negative becomes unset at render (keep `n>0`); `--port 70000` is written and then refused by the manager (`not-whole-number-in-range`). On macOS (Direct) it is written unchecked.
8. **Server ModelsMemoryMargin:** the GUI refuses a negative (`not-saved-invalid`); the CLI silently turns it into unset (cli.rs:412-414).
9. **Hostname:** the GUI can only tick detected interfaces (0.0.0.0 exclusive via `net_ifaces::toggle`). `pai server set --hostname "0.0.0.0,localhost"` is not refused client-side. Render canonicalizes the list but does not enforce exclusivity **(read: `render_for` calls only `parse_hosts`/`join_hosts`)**. The manager resolves hosts and checks collisions but not exclusivity.
10. **Duplicate preset id across the two files** is guarded by the app (de-conflict, `rename_with` P:1620). The manager's `presets_disabled_file` / `presets_file` check uniqueness only within one file (CK:375-392), so a raw `WriteFile` (e.g. from the bff) can create an enabled and a disabled section with the same id.
11. **Values greyed out but still written:** `kv-unified` under auto parallel; `fit-ctx`/`fit-target` with fit off; image tokens and `mmproj-device` without an mmproj; `split-mode` with one GPU; `WebuiMcpProxy` with the UI off (by design, documented). Only the draft keys, ngram keys and MoE cache are pruned. The `kv-unified` field doc (P:~285) says the form "disables the control there", which is true, but the value survives.
12. **README draft pin is stale:** the README Assets bullet says "the FIRST one if several". `draft_pin_device` (MT:912-929) pins dflash/dspark under layer mode to the LAST device with a share. AGENTS.md's footgun text agrees with the code.
13. **README Model info is stale:** it claims to show "whether a matching MTP/DFlash drafter is present"; the code removed that (gguf.rs ~726-733, and AGENTS.md "Model info names only files the preset LOADS").
14. **README names `DefaultSpinBox`** (KV cache bullet, field schema line). SpinBox is banned; these are `DefaultLineEdit`. Path rejection is attributed to `ini::reject_comment_markers`; `validate_for_save` uses `checks::comment_markers` directly (same rule).
15. **`ctx-size = 0` cannot be written:** `CK:117` uses `INT_POSITIVE`, yet `integrations.rs` and the README treat an explicit `ctx-size = 0` as meaningful ("asks for the full context and is never cut"). It is reachable only by a hand edit, and the manager would then refuse any later rewrite of that section (whole_number on the changed section).
16. **Delete with shared files:** the GUI deletes the section and keeps in-use files (`kept-in-use`); the CLI refuses the whole command up front unless `--even-if-shared`. Documented, but a divergence the web UI must pick a side on. File deletion runs in the client process, not in the manager.
17. **Bench defaults and leniency:** max tokens 256 (GUI) vs 0 (CLI); temperature lenient (GUI) vs strict (CLI); GPU lock refuses (GUI) vs waits (CLI); n_gen ≥0 checked only by the GUI; image format sniffed only by the CLI.
18. **No validation anywhere** (searched; none found): float preset fields (and `parse` accepts `inf`/`NaN`); enum preset fields (split-mode, cache types, load-mode on write, lazy-mode, pooling, reasoning*, spec-type); `chat-template-kwargs` JSON; min ≤ max pairs (image tokens, spec-draft n-min/n-max, ngram n-min/n-max, ubatch ≤ batch); device ids existing; quantized V with flash-attn off (only a warning strip, although fatal at load per `kv-quantized-needs-fa`); `CudaAllreduce` in the manager.
19. **Strings coverage for a non-GUI client:** the GUI keys in 2.4 and the rename/section keys exist only in `en_US.toml`, not `cli.toml`. Literals that violate "no text in code": `(none)`, `MTP: `, `DFlash: ` (`model_scan.rs:174, 238-246`); `unused`, `even`, `100%` (`crates/config/src/gpu_split.rs:73-80`); English in `bench/exec.rs:370,377,394`, `bench.rs:913`, BT:579 and BT:849.
20. **Defaults resolved on the client and written as values:** the server save writes `ModelsDir` and `StateDir` RESOLVED with the saving process's home (`CSC default_models_dir` = `home_dir()/.llama.cpp`, `slot_state::default_state_dir` = `data_root()/state`) everywhere except under the Windows manager (`render_for`, `dirs_left_unset` SC:311). On Linux, a GUI save of a server file whose shipped defaults were removed, or a first save when they are unset, writes the user's home paths into `/etc/llama.cpp/server@default.ini`, which the instance user `private-ai` cannot read **(inference from `ProtectHome` and the per-user path)**. A bff save would write `private-ai-bff`'s home. The uncommitted work addresses the Windows half only.

---

## 4. Manager API surface today

**Transports.**
- D-Bus: interface `org.privateai.Manager1`, path `/org/privateai/Manager1` (`crates/manager/src/manager/dbus.rs:17-21`).
- Windows: pipe `\\.\pipe\private-ai-manager` (`crates/core/src/client/pipe.rs:178`). Framing: a u32 LE length, then JSON ≤ 1 MiB; one request, one reply (`client/pipe.rs:4-37`).
- The same 9 methods exist on both.
- Errors are `org.privateai.Manager1.Error.<Kind>`:
  - The message is data: the kind's name; the colliding id for `AddressInUse`; the host for `UnresolvableHost`; the service text for `Service`; the io kind for `Io`; the encoded `Refusal` for `Invalid`.
  - The pipe also has `BadRequest`, and closes the connection after it.

| Method | Args → return | Does | Auth | Used by |
|---|---|---|---|---|
| `ListInstances` | () → `as` | the regular files `server@<id>.ini` in `engine_config_dir` (`paths.rs:249-263`) | none | bff only (`crates/bff/src/lib.rs:71`) |
| `Status` | id → state (active/inactive/failed/activating/deactivating) | systemd `ActiveState` / SCM | none | GUI status poll (`G:1808` via `is_running`), CLI, bench, slot_state, bff |
| `Start` | id → () | `StartUnit llama.cpp@<id>.service` / SCM start (creates the service on first use) | control | GUI Start (`G:1576`), `pai control start`, bench sweep |
| `Stop` | id → () | `StopUnit`; the client polls Status up to `STOP_GRACE` | control | GUI Stop (`G:1744`), `pai control stop`, bench |
| `Restart` | id → () | `RestartUnit` | control | `pai control restart` only |
| `BootEnabled` | id → b | `GetUnitFileState == enabled` / start type AUTO | none | Settings tab (`settings_tab.rs:51`), `pai server boot` |
| `SetBootEnabled` | id, b → () | Enable/DisableUnitFiles + Reload / ChangeServiceConfig | configure | Settings tab toggle (`settings_tab.rs:168`), `pai server boot on\|off` |
| `ReadFile` | name → (exists, text) on D-Bus; text or null on the pipe | reads `engine_config_dir/<name>` | **control for `server@<id>.ini`** (it can hold the API key), none for the preset files (SV:389-404) | Server tab, Models tab, Integrations, bench, CLI `server show`/`preset *`, import |
| `WriteFile` | name, contents → () | auth → name → mutex → `check_contents` → address check → atomic write (tmp + fsync + rename; 0640 group `private-ai` for instance files, 0644 for presets) (SV:408-425, 199-243) | configure | Server Save, preset save, rename, delete, on/off, New/Clone, HF download preset, bench overrides, `pai server set`, import |

**Authorization:**
- **Linux.** polkit `org.privateai.manager.control`: active session `yes`, otherwise `auth_admin`. `org.privateai.manager.configure`: `auth_admin_keep` (`xtask/src/linux/system_files.rs:229-240`). Subject: the caller's bus name; any polkit failure means refused.
- **Windows.** `TokenAuthority`: control = interactive (S-1-5-4) or member of Administrators (S-1-5-32-544, even deny-only, i.e. non-elevated); configure = Administrators member (`crates/manager/src/manager/windows.rs:71-143`).
- **The bff.**
  - On Windows its service SID is admitted to the pipe, but its token is neither interactive nor admin, so it may only make the unauthenticated calls: ListInstances, Status, BootEnabled, ReadFile of the preset files (windows.rs:22-30, tests ~576-602).
  - On Linux, as a non-session system user, polkit `control` falls to `auth_admin` with no agent, so it is refused **(inference: no polkit `.rules` file grants it, searched in `xtask/` and `installer/`)**. The bff can therefore NOT start or stop, read `server@default.ini` or write anything today.
  - A web UI with configuration power needs a new authorization design: a polkit rule or group for `private-ai-bff`, or the bff acting for an authenticated user, plus its own authentication, since it is loopback-only now (`crates/bff/src/listen.rs:22-50`).

**Client mapping** (`crates/core/src/client.rs:107-123`): `client-not-authorized`, `client-invalid-id`, `client-no-instance`, `client-bad-file-name`, `client-address-in-use {id}`, `client-unresolvable-host {host}`, `client-service-failed {msg}`, `client-io-failed {kind}`, `client-unavailable`, `client-other {detail}`; `Invalid` maps to `Refusal::text()`. The bff today returns only `unavailable`, `not-authorized` or `failed` (`crates/bff/src/lib.rs:71-95`).

### 4.1 Features that do NOT go through the manager (what they read locally, and as whom)

| Feature | Reads / writes | As | bff (`private-ai-bff`) situation |
|---|---|---|---|
| settings.ini (all Settings keys, BenchDb, BenchPromptFile, window sizes, HwMonitorHidden) | `~/.config/private-ai/settings.ini` (`paths::config_dir` `crates/core/src/paths.rs:82`) | GUI user | not reachable (`ProtectHome`); needs a per-user store or a bff-owned one |
| Start at login | autostart `.desktop` / HKCU Run | GUI user's session | meaningless for a web UI |
| bench.db, gpu.lock, sweep-pending | `data_root()/bench/` or `BenchDb` | GUI user | not reachable; the bff would need its own DB or a shared one |
| Benchmark runs | spawns `llama-bench`, HTTP to llama-server, `/dev/kfd`/`/dev/nvidia*` scan, journal for the memory record, prompt/spec/image files at any path, `$TMP` report | GUI user | needs GPU device access, journal group, file access |
| opencode.json | `~/.config/opencode/opencode.json` (`paths.rs:351`) | GUI user | per user by nature: the bff cannot write a user's file |
| HF search/list/download | `curl`; token from env or `~/.cache/huggingface/token`; `llama-server` download child; writes `<ModelsDir>/.hf-staging` and the four subfolders | GUI user | needs write access to ModelsDir (`/var/lib/private-ai/.llama.cpp` on Linux), network egress (`RestrictAddressFamilies` allows INET), its own token |
| Model scans (pickers, Delete dialog, GGUF Model info, tensor picker, `effective_ctx`) | walks `ModelsDir/{models,mmprojs,mtps,dflashs}`; GGUF via libggml-base | GUI user | needs read access to ModelsDir; the bff links pai-core only (no gguf module) |
| Linked-file delete | deletes files and empty folders under ModelsDir (`linked_files.rs:278-309`) | GUI user | needs write access; today the only code path that deletes user files |
| Device probe | `llama-server --list-devices` (`devices.rs:273-290`), `sdk-devices` child | GUI user | machine-wide; the bff can spawn it if it may open the GPUs |
| HW Monitor | sysfs, procfs, NVML, D3DKMT, SMBIOS | any | machine-wide, readable |
| Logs | Linux journal (`systemd-journal`/`adm`/`wheel`); Windows instance log file; macOS per-user file | GUI user | bff needs a journal group on Linux |
| Snapshot save before stop | HTTP `/slots/{id}?action=save` to llama-server (`crates/app/src/slot_state.rs`) | GUI user | loopback HTTP, reachable |
| Default ModelsDir/StateDir resolution | `home_dir()` / `data_root()` of the PROCESS | GUI user | would resolve to the bff's home (see 3.20) |
| Interfaces for "Bind to", CPU count for thread sliders | `net_ifaces::interfaces()`, `available_parallelism` | local machine | fine (same machine), but must come from the server side, not the browser |
| Import / 2.4 migration | the user's own trees | GUI user | not applicable to the bff |

---

## 5. Hard-wired to llama.cpp as the only engine

- **Paths** (`crates/core/src/paths.rs`):
  - `ENGINE_DIR = "llama.cpp"` (:24).
  - `engine_config_dir` (:118-134): `/etc/llama.cpp` on Linux, `%ProgramData%\llama.cpp\config` on Windows.
  - `windows_machine_dirs` (:167-176): `%ProgramData%\llama.cpp\{config,logs,models,state}`.
  - `instance_models_dir` (:190-206) `/var/lib/private-ai/.llama.cpp`; `instance_state_dir` (:212-224); `instance_log` (:228) `llama-server@<id>.log`.
  - `engine_install_dir` reads `HKLM\Software\llama.cpp InstallDir` (:510-524); `/opt/llama.cpp` on macOS (:528).
  - `llama_bin` / `llama_server_exe` / `llama_bench_exe` (:421-633).
  - `default_models_dir` = `~/.llama.cpp` (`CSC`).
- **Unit and service names:**
  - systemd `llama.cpp@{id}.service` (`dbus.rs:218,254,277,286`), template `llama.cpp@.service` (`system_files.rs:116-144`, `ExecStart=pai-manager run-instance %i`).
  - The journal unit in `SC:350-352`.
  - SCM `llama.cpp_<id>` (`windows.rs:153`) with account `NT SERVICE\llama.cpp_<id>` (:187-198).
  - polkit messages say "llama.cpp servers".
- **Instance:** the GUI and CLI use only `default` (`SC:14`). The manager supports any `[a-z0-9-]{1,32}` id, an instance existing iff `server@<id>.ini` exists. Shipped defaults: `crates/core/defaults/server@default.ini` and `presets.ini`.
- **One shared presets.ini for every instance.** `launch::instance_command` passes the same `presets.ini` to every instance (`crates/core/src/launch.rs:223`). The manager's startup check uses the minimum ModelsMax over instances (SV:490-500). There is no per-instance or per-engine preset file, and no field in a section saying which engine serves it.
- **Preset format = `llama-server --models-preset`:**
  - `launch::server_args` (`launch.rs:18-122`) always launches the router with `--models-preset`.
  - Every `checks.rs` table names llama-server flags (`PRESET_INT_KEYS`, `PRESET_TEXT_KEYS`, `LLAMA_MODELS_MAX_DEFAULT`).
  - Every section must have `model`; `load-on-startup` is counted against `--models-max`.
  - `Preset` (P) is a struct of llama-server keys, and `render_section` writes only known keys **(inference)**.
  - The inline-comment rule (`;`/`#`) mirrors llama-server's preset reader (`crates/core/src/ini.rs`).
- **server.ini schema = llama-server router flags + ggml env vars** (`CSC`; `launch.rs` `env_vars` :129-169, `launch_command` :175-194 with `LLAMA_CACHE` and the ROCm PATH).
- **The manager API has no engine notion.** Requests take only an instance id or a file name (`crates/manager/src/manager/pipe.rs:22-51`, `dbus.rs:117-196`). `run-instance` always resolves `llama_server_exe` (`crates/manager/src/manager.rs:150-181`).
- **Client-side engine knowledge:**
  - Device ids come from `--list-devices`.
  - The GGUF reader is libggml-base; `arch_tables.rs` is generated from the llama.cpp clone.
  - `kv_cache`, `moe_cache`, `gpu_split` and `tensor_override` mirror llama.cpp internals.
  - HF downloads use llama-server's router download child.
  - Benchmarks use `llama-bench` and llama-server's `/v1/chat/completions` timings.
  - Snapshots use llama-server `/slots`.
- **For Strata to fit (inference):** at minimum, an engine type per instance (or per preset section), engine-specific config dirs/unit prefixes/launch paths in the manager, and per-engine check tables in `checks.rs`. Every preset rule above would then be scoped to the llama.cpp engine.
