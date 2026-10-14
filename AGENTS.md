# Project guide

## Project overview

PowerShell scripts to install prerequisites, configure, build, and package [llama.cpp](https://github.com/ggerganov/llama.cpp) on Windows with CUDA, Vulkan, and ROCm (HIP) support, and NSIS installer packaging. Linux gets a bash counterpart (`00` to `03`) that ends in a `.deb` and an `.rpm` instead of the NSIS installer, plus `04-install.sh`, which installs the newest of them on the build machine; macOS is not supported. llama.cpp's built-in chat interface (served by `llama-server`) is used as the frontend; no separate web UI is bundled.

A companion Rust binary (`llama-cpp-config`) provides a GUI + CLI configurator for `server.ini` and per-model `presets.ini`.

## Map

This file is the map; the details of each area live in the guide named beside it. **Read that guide before changing the area.** Subdirectory `AGENTS.md` files are scoped to their folder.

- **Build pipeline** → [docs\build-pipeline.md](docs/build-pipeline.md) (scripts, configure line, sccache, LTO, lld, HIP targets, build parallelism):
  - `00-install-prerequisites.ps1`: idempotent toolchain bootstrapper (winget packages, sccache from its GitHub release, and the **newest stable** ROCm/TheRock dist, CUDA Toolkit and Vulkan SDK: nothing is pinned, `installer\dist-sources.psd1` says where each is looked up; llama.cpp release check, environment report). Safe to re-run.
  - `dists.ps1`: the SDK dist layout, dot-sourced by 00, 01 and `common.ps1`. Every GPU SDK the build links lives at **`<DistRoot>\<Name>\<Version>\windows`** (`TheRock`, `CUDA`, `VulkanSDK`; e.g. `E:\TheRock\10.1.0\windows`) and is found there and nowhere else (`Find-Dist`: no `HIP_PATH`/`CUDA_PATH`/`VULKAN_SDK`, no vendor default location). `DistRoot` defaults to `E:\` (`dist-sources.psd1`) and is configurable: `-DistRoot` on 00 and 01, or `LLAMA_FW_DIST_ROOT`; 01 records it in the config. OpenSSL (a system library, winget's `Program Files\OpenSSL-Win64`) and the tools (VS, cmake, NSIS, sccache...) are not on the layout.
  - `01-configure.ps1`: detects paths and tools, writes `build\windows\config-build.psd1` (the dist root included) and the generated `.cargo\config.toml`.
  - `02-build.ps1`: checks out the latest llama.cpp **release** tag (never a `bNNNN` nightly), applies `patches\llama.cpp\`, builds llama.cpp (CMake + Ninja) and `llama-cpp-config` (cargo).
  - `03-package.ps1`: stages into `build\windows\staging\` (the ROCm and CUDA runtime libraries the backends load included, copied out of the dists: the installer is self-contained but for the GPU drivers and the VC++ runtime), builds the NSIS installer into `.\dist\`.
  - `common.ps1`: shared bootstrap dot-sourced by 02 and 03 (`$cfg`, `$distRoot`, ROCm PATH, the compile-time HIP vars, `Enable-VsDevShell`). The HIP vars (`LLVM_PATH` above all) must NEVER be set machine-wide: they break the driver's HIP runtime.
  - `cmake-options.psd1`: the machine-independent cmake `-D` options, each with its reason. `build\windows\config-build.psd1`: the machine-derived half, generated.
  - **Linux** (`00-install-prerequisites.sh`, `01-configure.sh`, `02-build.sh`, `03-package.sh`, `04-install.sh`, `common.sh`, `installer\dist-sources.sh`): the bash counterpart of 00 to 03, plus 04, which installs the newest package in `.\dist\` (format by distribution; install, upgrade, same-version reinstall or downgrade alike). 02 ends at a working build tree with llama.cpp and `llama-cpp-config`; 03 packages it as a `.deb` and an `.rpm` in `.\dist\` (self-contained under `/opt/llama.cpp`: binaries in `bin/`, the ROCm, CUDA and Vulkan runtimes in `lib/`; a symlink per executable in `/usr/bin`, a desktop entry for the configurator; the config files are not in the package, `llama-cpp-config` keeps them per user in `~/.config/llama.cpp`). Dists install as `<Root>/<Version>/linux` under `/mnt/data` (`TheRock`, `CUDA`, `VulkanSDK`, plus the optional `NCCL`; RCCL and NCCL are what `--split-mode tensor` reduces through and exist on Linux only; the root is configurable as on Windows: `--dist-root` on 00 and 01, or `LLAMA_FW_DIST_ROOT`, recorded by 01 as `CFG_DIST_ROOT`), and everything generated goes to `build/linux/`, never into the Windows artifacts under `build/windows/`: see the "Linux" section of the guide.
  - `patches\hip\<major>\` (HIP header workaround) and `patches\llama.cpp\` (provisional out-of-tree patches): each has its own README.
- **Installer** → [installer\AGENTS.md](installer/AGENTS.md): NSIS template, `dist-sources.psd1` (where every external distribution is looked up; no versions), and what the installer bundles of the GPU runtimes and why (including the HIP runtime beside the exe that makes BF16/F16 run on RDNA4).
- **Configurator** → [llama-cpp-config\AGENTS.md](llama-cpp-config/AGENTS.md) (tabs, common-change checklists, footguns) and [llama-cpp-config\README.md](llama-cpp-config/README.md) (architecture, conventions, tests).
- **Diagnostics and migration** → [tools\AGENTS.md](tools/AGENTS.md): `migrate-config-v1-to-v2.ps1`/`.sh` (offline 1.x → 2.0 config rewrite), `report-memory.ps1` (requested vs actual GPU memory of the running server) and `pcie-bench\` (host <-> GPU transfer benchmark).
- **Per-GPU guides** → [docs\AGENTS.md](docs/AGENTS.md): rules for the `docs\*.md` configuration references.
- `resources\llama.ico`: icon for the NSIS installer and `llama-cpp-config.exe`. **Generated, not checked in** (gitignored): `resources\generate-llama-ico.mjs` rasterizes llama.cpp's webui logo (`tools\ui\src\lib\assets\logo.svg`) as a `#111111` glyph on a white rounded tile, matching upstream's PWA icons. `llama-cpp-config\build.rs` (and `03-package.ps1`) runs it whenever the file is missing; it needs node+npm and the llama.cpp clone, so a fresh checkout needs `02-build.ps1` before a bare `cargo build`/`cargo test`. Delete `llama.ico` to regenerate after an upstream logo change.

## Key conventions

- **No em-dashes anywhere in this repo.** Not in comments, doc comments, user-facing strings, the comment text written into `server.ini` / `presets.ini`, docs, or commit messages. Use `:` for an apposition or explanation, `;` between two independent clauses, `,` for an aside or trailing afterthought, or parentheses when a pair wrapped an aside. The one sanctioned exception is a `—` that is DATA rather than prose: the glyph the GPU table shows for a not-applicable cell (`gpu_split.rs`, `components.slint`), the assertions pinning it, and its entry in `binding_lint.rs`'s `RENDERABLE` whitelist.
- `common.ps1` is dot-sourced by `02-build.ps1` and `03-package.ps1`. Other scripts (`00-install-prerequisites.ps1`, `01-configure.ps1`) read `build\windows\config-build.psd1` directly.
- `build\windows\config-build.psd1` (build-time) uses single-quoted PowerShell-data-file syntax. Runtime config files (`server.ini`, `presets.ini`) are plain INI written as UTF-8 without BOM.
- All build artifacts (source clones, cmake binary dir, NSIS staging, sccache cache, `config-build.psd1`, generated `.nsi`) live under `.\build\` (gitignored), in a per-OS directory: `build\windows\` (`$buildRoot` in `common.ps1`; `build/linux/` on Linux). Cargo's target dir is included: `build\windows\cargo-target\` (`build/linux/cargo-target/` on Linux), set by the build scripts through `CARGO_TARGET_DIR`. The one exception is the generated `.cargo\config.toml` (gitignored separately: cargo only discovers it by walking up from the cwd); a bare `cargo build`/`cargo test` outside the scripts also still writes `llama-cpp-config\target\`. rust-analyzer in VS Code checks into `build\rust-analyzer\` (`.vscode\settings.json`, checked in, relative paths only; not per-OS, since VS Code settings cannot be). The same file keeps CMake Tools from configuring anything on its own (`tools\pcie-bench\build.ps1` is its only configure path: the VS generator the extension's kits pick finds no CUDA toolset from the dist layout); C/C++ IntelliSense reads the `compile_commands.json` that `build.ps1` writes into `build\windows\pcie-bench\`. The final installer goes to `.\dist\`. `rm -rf build\` wipes everything, the sccache and the Rust release build included.
- The installer registers in `HKLM\Software\llama.cpp` (InstallDir) and supports upgrades (uninstalls previous version first). User runtime state lives under `%LOCALAPPDATA%\llama.cpp\`:
  - `config\server.ini` (single `[Server]` section), `config\presets.ini` (per-model presets for `llama-server --models-preset`), `config\presets.ini.disabled` (switched-off presets, never read by llama-server), `config\settings.ini` (the configurator's preferences), `config\bench-prompt.txt` + `config\bench-prompt-long.txt` (the live benchmark's prompts, seeded on first use, edited in place; `settings.ini` `BenchPromptFile` picks which, or points elsewhere).
  - `logs\llama-server.log` (both output streams; on stop, a file LARGER than `settings.ini` `LogRotateKb`, 1024 by default, is filed as `logs\llama-server-<yyyymmdd-hhmmss>.log`, closing instant in UTC; a run that ended without a stop is filed at the next start under its last-write time; newest 10 kept; `LogRotate = false` keeps a single growing file: `runstate.rs` module header).
  - `bench\bench-<stamp>.{jsonl,md,log}` (one run each, written as it goes) and `bench\sweep-<stamp>.{jsonl,md}` (one `bench sweep` digest each; named `sweep-` so the saved-runs list, which globs `bench-*.jsonl`, skips them).
  - Only when `SaveStateOnShutdown` is on: `state\<model>.llamastate` (KV-cache snapshots sized by the tokens the slot HOLDS, not `ctx-size`: ~4.4 GiB at 128k tokens on Qwen3.8-27B, ~8.3 GiB for a full 262k; overridable via `StateDir` since the default sits on the system drive).
  - The uninstaller asks (default No) whether to delete the whole tree, so it survives reinstall/upgrade by default.
- On Linux the same tree is `~/.local/share/llama.cpp/`, except that everything `config\` holds lives in **`~/.config/llama.cpp/`** (`$XDG_CONFIG_HOME/llama.cpp` when that is set to an absolute path): `paths::config_dir`. The directory is per user and the same for a packaged install and a tree build; the packages ship no config directory and no post-install script. The Settings tab's "Start at login" is the HKCU Run registry entry on Windows and the XDG autostart file `~/.config/autostart/llama-cpp-config.desktop` on Linux (`startup.rs`); either way the entry itself is the state, with no INI mirror.

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
