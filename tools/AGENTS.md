# tools\

Diagnostics and the 1.x → 2.0 config migration, not part of the build pipeline or the installer.

## migrate-config-v1-to-v2.ps1 / .sh

The offline migration of a 1.x config to 2.0, run by hand (`-DryRun` / `--dry-run` shows the changes): the binary carries no migration code, by decision, and 2.0 simply ignores the moved server.ini keys. Twins (Windows `%LOCALAPPDATA%\llama.cpp\config`, Linux `~/.config/llama.cpp`, plus a copy out of `/etc/llama.cpp` when the new directory is empty): keep their rules in step. What moves, each written as the launch 1.x actually ran:

- `LoadMode` (or the legacy `Mlock`/`NoMmap` pair: both true = `mlock`, `NoMmap` alone = `none`) → `load-mode` in every preset, replacing the preset's own, because 1.x put `-lm` on the router line ALWAYS (auto when unset). Under auto only a disagreeing preset key is rewritten; with none of the three keys in server.ini the step is skipped.
- `Device` → `device` in every preset, and `TensorSplit` → `tensor-split` with it: a server-wide device list won over the preset's and took its split along, so with a `Device` and no `TensorSplit` the preset's own `tensor-split` is DROPPED (its weights were positional over another list). A `TensorSplit` alone is written alone.
- `OverrideTensor` → `override-tensor` in every preset, overwriting the preset's own: the server-wide rules REPLACED a preset's rather than adding to them.
- `SplitMode` → `split-mode` in every preset, only when it is `none`/`tensor`/`row`: a server-wide `layer` was dropped on read.
- `Fit` → `fit = on|off` in every preset, replacing the preset's own: 1.x always put `-fit` on the router line, `off` unless `Fit = true`.
- `PrefillAssistant = false` → `prefill-assistant = false` in every preset (1.x passed `--no-prefill-assistant` to every model); `true` was llama.cpp's default and writes nothing.
- `CacheReuse` above 0 → `cache-reuse` in every preset, replacing the preset's own (1.x put `--cache-reuse` on the router line).
- `MmprojDevice` → `mmproj-device` only where the preset has an `mmproj` and no device of its own (the env `MTMD_BACKEND_DEVICE` it rode loses to a preset key).
- Then the keys, their commented placeholders and the comment block above each go from server.ini. Both `presets.ini` and `presets.ini.disabled`; each touched file is backed up as `<file>.v1.bak`; line endings and UTF-8 without BOM are kept.
- **No moved key left in server.ini means already migrated, and the script exits untouched.** Without that guard a second run reads the absence as 1.x's `auto` and rewrites every preset's `load-mode`.

## report-memory.ps1

Reads only `%LOCALAPPDATA%\llama.cpp\`, so it runs on an installed machine. Slices the memory of the **currently running** llama-server across llama-server's load log (every buffer's size and device → the *requested* footprint) and Windows' `GPU Process Memory` counters (dedicated VRAM vs shared system RAM actually handed out), and cross-checks them: an oversubscribed device does not fail to allocate, WDDM silently pages the excess over PCIe and the only symptom is slow decoding. Tensor placement is an **`of which tensor override`** sub-row under `model weights`, never a row of its own and never summed (an override *moves* a tensor, it allocates nothing extra). The host section names the pinned share of the model weights (the shared-GPU-memory number). `-Json` for machine output.

Placement sources: the **rules** come from the ROUTER's `spawning server instance with args:` block (the already-merged winner, not `presets.ini`/`server.ini`), which sits above the `device_info` banner and is matched to the child by its `[port]` prefix, never by proximity. The **hits** come from `tensor <name> (<n> MiB <type>) buffer type overridden to <buft>`, which is `LLAMA_LOG_DEBUG` (verbosity **5**), so at `-lv 4` no hits does not mean nothing matched; the report says which.

Parsing traps, all handled:

- **Which block**: matched to a live listener by PORT (`Get-LivePorts`; the log records no PIDs), never "the newest block". The `device_info` banner and `buffer size` lines are TRACE (`-lv >= 4`), so at `LogVerbosity` 3 the live child writes neither and the newest block belongs to a dead run. An older block is cut at the next block's banner, and the failure path names the cause (live port plus its `-lv`).
- `sched_reserve` is **re-emitted** for the draft context after the mmproj loads (clip ownership comes from the `reserve_compute_meta` source line, not a banner latch), and the compute graph is **re-reserved** when slots come up (collapse per `(owner, kind, device)`, do not sum). `sched_reserve` groups are assigned by group, not by the open context: a later group goes to whichever reference group (model's or draft's) its total is closer to.
- **Spill test**: shared memory beyond that backend's pinned `*_Host` buffers, never the arithmetic alone and never raw shared (the log accounts for no driver context, WDDM can trim other processes, and `free at load` is a snapshot). An uncorroborated overflow reads `AT THE LIMIT`, not paging; the same subtraction stops a host compute buffer charted under the card from reading as a spill. The threshold is **512 MiB**, clearing the driver's own non-local memory present even with free VRAM (~290-360 MiB on the 4070 Super/CUDA, ~210 MiB on the R9700/HIP).
- With `--fit on` the model is **loaded twice** (a `no_alloc` dry run, then the real one): collapse override lines by tensor name.
- `CPU_Mapped` (mmap'd weights) is **host memory, not a device**: match host on `^CPU`, not `^CPU$`.
- A **sliding-window model builds two KV caches** (`llama_kv_cache_iswa`) announced with the same line: latch on the `creating non-SWA / creating SWA KV cache` banner, or de-duplication eats one.
- A **separate draft/MTP head prints its own `print_info` block** after the model's: arch/params/file type/file size take the FIRST occurrence.
- The `buffer size` regex must be exhaustive (**model / KV / RS / compute / output / LoRA / DSV4 `<name>` state**): an unmatched kind vanishes from the balance sheet.
- A separate **draft FILE** loads after `loading draft model` but before its own `llama_context`: the banner is latched, or its weights are charged to the model and then de-duplicated away.

A rule ending in **`=CPU` does not mean ordinary RAM**: with a GPU backend up and mmap off, CPU-side weights live in that backend's *pinned* host buffer (`CUDA_Host`/`ROCm_Host`, charted as **shared GPU memory**); with mmap on, the same debug line is logged and the tensor is served from `CPU_Mapped`, leaving the pinned buffer at 0 MiB. So the pinned verdict is gated on a pinned model buffer actually appearing in the `buffer size` lines, and an `of which` **larger than its parent row** is suppressed and the mismatch reported.

## pcie-bench\

A host <-> GPU transfer micro-benchmark over the three paths of ggml-cuda's internal AllReduce (`allreduce.cu`), which `split-mode tensor` reduces through on Windows (no NCCL/RCCL; `GGML_CUDA_ALLREDUCE` defaults to `internal` off Linux) by staging through pinned host memory, so the SLOWER card's link bounds tensor parallelism: DMA copies, 16-byte kernel stores/loads into mapped host memory, and the arrival-token handshake. One `pcie-bench.cpp` built twice by `build.ps1` into `build\windows\pcie-bench\`: as HIP (ROCm's clang as CXX + `hip::device` + the force-included `patches\hip\<major>\` wrapper below clang 24, the same route as `02-build.ps1`) and as CUDA through `pcie-bench.cu`, which only `#include`s it (nvcc from the newest CUDA dist, `Find-Dist`). It also writes the `compile_commands.json` VS Code's IntelliSense reads; CMake Tools is kept from configuring it (`.vscode\settings.json`). The HIP exe gets the dist's runtime trio beside it (load order, as the installer). Purpose: seeding `GGML_CUDA_AR_COPY_THRESHOLD` / `GGML_CUDA_AR_COPY_CHUNK_BYTES` and measuring a slot's real bandwidth, which Windows does not expose for a card behind a switch. HIP-on-Windows traps it encodes: DMA copies are timed wall-clock (event timing reports impossible figures), and the handshake flushes the stream (`hipStreamQuery`) after the launch, since a kernel does not start until a later API call submits it. Reference numbers: `pcie-bench\README.md`.
