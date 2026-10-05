# Project guide

## Project overview

Windows-only PowerShell scripts to install prerequisites, configure, build, and package [llama.cpp](https://github.com/ggerganov/llama.cpp) with CUDA, Vulkan, and ROCm (HIP) support, and NSIS installer packaging. Not compatible with Linux or macOS. llama.cpp's built-in chat interface (served by `llama-server`) is used as the frontend; no separate web UI is bundled.

A companion Rust binary (`llama-cpp-config`) provides a GUI + CLI configurator for `server.ini` and per-model `presets.ini`.

## Map

This file is the map; the details of each area live in the guide named beside it. **Read that guide before changing the area.** Subdirectory `AGENTS.md` files are scoped to their folder.

- **Build pipeline** → [docs\build-pipeline.md](docs/build-pipeline.md) (scripts, configure line, sccache, LTO, lld, HIP targets, build parallelism):
  - `00-install-prerequisites.ps1`: idempotent toolchain bootstrapper (winget packages, sccache from its GitHub release, the ROCm/TheRock dist pinned in `installer\dist-pins.psd1`, llama.cpp release check, environment report). Safe to re-run.
  - `01-configure.ps1`: detects paths and tools, writes `build\config-build.psd1` and the generated `.cargo\config.toml`.
  - `02-build.ps1`: checks out the latest llama.cpp **release** tag (never a `bNNNN` nightly), applies `patches\llama.cpp\`, builds llama.cpp (CMake + Ninja) and `llama-cpp-config` (cargo).
  - `03-package.ps1`: stages into `build\staging\`, builds the NSIS installer into `.\dist\`.
  - `common.ps1`: shared bootstrap dot-sourced by 02 and 03 (`$cfg`, ROCm PATH, the compile-time HIP vars, `Enable-VsDevShell`). The HIP vars (`LLVM_PATH` above all) must NEVER be set machine-wide: they break the driver's HIP runtime.
  - `cmake-options.psd1`: the machine-independent cmake `-D` options, each with its reason. `build\config-build.psd1`: the machine-derived half, generated.
  - `patches\hip\<major>\` (HIP header workaround) and `patches\llama.cpp\` (provisional out-of-tree patches): each has its own README.
- **Installer** → [installer\AGENTS.md](installer/AGENTS.md): NSIS template, `dist-pins.psd1` (the single source of truth for external distributions), `install-runtime-deps.ps1` (end-user helper, including the HIP runtime staging that makes BF16/F16 run on RDNA4).
- **Configurator** → [llama-cpp-config\AGENTS.md](llama-cpp-config/AGENTS.md) (tabs, common-change checklists, footguns) and [llama-cpp-config\README.md](llama-cpp-config/README.md) (architecture, conventions, tests).
- **Diagnostics** → [tools\AGENTS.md](tools/AGENTS.md): `report-memory.ps1` (requested vs actual GPU memory of the running server) and `pcie-bench\` (host <-> GPU transfer benchmark).
- **Per-GPU guides** → [docs\AGENTS.md](docs/AGENTS.md): rules for the `docs\*.md` configuration references.
- `resources\llama.ico`: icon for the NSIS installer and `llama-cpp-config.exe`. **Generated, not checked in** (gitignored): `resources\generate-llama-ico.mjs` rasterizes llama.cpp's webui logo (`tools\ui\src\lib\assets\logo.svg`) as a `#111111` glyph on a white rounded tile, matching upstream's PWA icons. `llama-cpp-config\build.rs` (and `03-package.ps1`) runs it whenever the file is missing; it needs node+npm and the llama.cpp clone, so a fresh checkout needs `02-build.ps1` before a bare `cargo build`/`cargo test`. Delete `llama.ico` to regenerate after an upstream logo change.

## Key conventions

- **No em-dashes anywhere in this repo.** Not in comments, doc comments, user-facing strings, the comment text written into `server.ini` / `presets.ini`, docs, or commit messages. Use `:` for an apposition or explanation, `;` between two independent clauses, `,` for an aside or trailing afterthought, or parentheses when a pair wrapped an aside. The one sanctioned exception is a `—` that is DATA rather than prose: the glyph the GPU table shows for a not-applicable cell (`gpu_split.rs`, `components.slint`), the assertions pinning it, and its entry in `binding_lint.rs`'s `RENDERABLE` whitelist.
- `common.ps1` is dot-sourced by `02-build.ps1` and `03-package.ps1`. Other scripts (`00-install-prerequisites.ps1`, `01-configure.ps1`) read `build\config-build.psd1` directly.
- `build\config-build.psd1` (build-time) uses single-quoted PowerShell-data-file syntax. Runtime config files (`server.ini`, `presets.ini`) are plain INI written as UTF-8 without BOM.
- All build artifacts (source clones, cmake binary dir, NSIS staging, sccache cache, `config-build.psd1`, generated `.nsi`) live under `.\build\` (gitignored). The two exceptions are cargo's `llama-cpp-config\target\` and the generated `.cargo\config.toml` (gitignored separately: cargo only discovers it by walking up from the cwd). The final installer goes to `.\dist\`. `rm -rf build\` wipes everything C++-side including the sccache but leaves the Rust `target\` alone.
- The installer registers in `HKLM\Software\llama.cpp` (InstallDir) and supports upgrades (uninstalls previous version first). User runtime state lives under `%LOCALAPPDATA%\llama.cpp\`:
  - `config\server.ini` (single `[Server]` section), `config\presets.ini` (per-model presets for `llama-server --models-preset`), `config\presets.ini.disabled` (switched-off presets, never read by llama-server), `config\settings.ini` (the configurator's preferences), `config\bench-prompt.txt` + `config\bench-prompt-long.txt` (the live benchmark's prompts, seeded on first use, edited in place; `settings.ini` `BenchPromptFile` picks which, or points elsewhere).
  - `logs\llama-server.log` (both output streams; on stop, a file LARGER than `settings.ini` `LogRotateKb`, 1024 by default, is filed as `logs\llama-server-<yyyymmdd-hhmmss>.log`, closing instant in UTC; a run that ended without a stop is filed at the next start under its last-write time; newest 10 kept; `LogRotate = false` keeps a single growing file: `runstate.rs` module header).
  - `bench\bench-<stamp>.{jsonl,md,log}` (one run each, written as it goes) and `bench\sweep-<stamp>.{jsonl,md}` (one `bench sweep` digest each; named `sweep-` so the saved-runs list, which globs `bench-*.jsonl`, skips them).
  - Only when `SaveStateOnShutdown` is on: `state\<model>.llamastate` (KV-cache snapshots sized by the tokens the slot HOLDS, not `ctx-size`: ~4.4 GiB at 128k tokens on Qwen3.8-27B, ~8.3 GiB for a full 262k; overridable via `StateDir` since the default sits on the system drive).
  - The uninstaller asks (default No) whether to delete the whole tree, so it survives reinstall/upgrade by default.

## Release process

1. **Bump version** in `llama-cpp-config\Cargo.toml` (`[package] version = "X.Y.Z"`). The next build also refreshes the crate's line in `Cargo.lock`: both files belong to the release commit.
2. **Run `02-build.ps1`**: builds llama.cpp (fetches and checks out the latest `vX.Y.Z` **release** tag; `bNNNN` nightlies are not candidates) and llama-cpp-config.
3. **Run `03-package.ps1`**: stages binaries, builds the NSIS installer to `.\dist\llama-cpp-framework-vX.Y.Z-vA.B.C-x64-setup.exe` (framework version first, bundled llama.cpp release second).
4. **Commit** all changes. Subject: **`Release vX.Y.Z: <summary>`**, the SAME first line as the tag (step 5); a `feat:`/`docs:`-style subject is not the convention for release commits. Body: a prose summary, closing with a **`Bundles llama.cpp <tag> (up from <previous tag>).`** line, e.g. `Bundles llama.cpp v0.2.0 (up from v0.1.2).`.
5. **Tag**: `git tag -a X.Y.Z -m "Release vX.Y.Z: <short summary>..." && git push origin X.Y.Z`
   - Tag name: **no `v` prefix** (e.g. `1.5.1`, not `v1.5.1`).
   - Tag message: starts with `Release vX.Y.Z: <summary>` on first line, then blank line, then bullet points of changes. Match the style of previous tags (`git show 1.5.0`).
6. **Create release**: `gh release create X.Y.Z --title "vX.Y.Z (llama.cpp vA.B.C)" --notes "<markdown body>"`
   - Title format: **`vX.Y.Z (llama.cpp vA.B.C)`** (with `v` prefix, includes the bundled llama.cpp release).
   - Body: detailed markdown with sections (`##`, `###`), bullet points, code formatting. Match the style of previous releases (`gh release view 1.5.0`). End with test count and bundled llama.cpp release.
7. **Attach installer**: `gh release upload X.Y.Z dist\llama-cpp-framework-vX.Y.Z-vA.B.C-x64-setup.exe`

**Formatting conventions** (older releases show a `bNNNN` llama.cpp token instead of `vA.B.C`):

| Artifact | Format | Example |
|---|---|---|
| Commit subject | `Release vX.Y.Z: <summary>` (matches the tag) | `Release v1.5.1: proxy/gateway support...` |
| Tag name | `X.Y.Z` (no `v`) | `1.5.1` |
| Tag message | `Release vX.Y.Z: <summary>\n\n- bullets...` | `Release v1.5.1: proxy/gateway support...` |
| Release title | `vX.Y.Z (llama.cpp vA.B.C)` | `v1.11.2 (llama.cpp v0.2.0)` |
| Installer name | `llama-cpp-framework-vX.Y.Z-vA.B.C-x64-setup.exe` | `llama-cpp-framework-v1.11.2-v0.2.0-x64-setup.exe` |
