---
title: Packaging and install
type: reference
updated: 2026-11-14
---

# Packaging and install

What `xtask package` puts in the MSI and in each `.deb` and `.rpm`, and how `xtask install` lays the
packages down. The `.pkg` files are on [the macOS page](build-macos.md). The MSI's own authoring is in
[installer\AGENTS.md](../installer/AGENTS.md), and the phases and what breaks are in
[Build pipeline](build-pipeline.md).

## Packaging

**What a machine installs is self-contained: the target needs the GPU drivers and little else.** On
Windows that also means the VC++ runtime. Each OS stages into `build/<os>/staging/`, and the staged
tree is the installed tree.

**Windows ships one MSI. Linux ships eight packages per format and macOS four, so a machine can leave out
the runtime of a GPU it does not have, and a headless server the GUI.** `xtask/src/common/components.rs` splits the staged tree by
path, so no file lands in two packages or in none. On Linux the engine, llama.cpp, is packaged apart
from the framework, under its own name and version (`PLAN.md` step 11, R12); the MSI and the macOS
`.pkg` files get the same split in later slices of that step (3 and 4), and until then keep the
layout described here and on [the macOS page](build-macos.md).

| OS | Package | File in `dist/` |
| --- | --- | --- |
| Windows | The MSI | `private-ai-framework-v<framework>-<llama>-x64.msi` (`arm64` on an ARM build); `private-ai-framework-v<framework>-x64.msi` once step 11 slice 3 lands |
| Linux | The framework: `config`, `cli`, `bff`, `core` | `private-ai-framework-<package>-v<framework>-amd64.deb`, `-x86_64.rpm` (`arm64`, `aarch64` on ARM) |
| Linux | The engine, `llama.cpp` | `llama.cpp-v<release>+pai<N>-amd64.deb`, `-x86_64.rpm` (the same on ARM) |
| Linux | Its runtimes, `llama.cpp-rocm`, `-cuda`, `-vulkan` | `llama.cpp-<sdk>-v<sdk version>-amd64.deb`, `-x86_64.rpm` (the same on ARM) |
| macOS | `config`, `cli`, `llama`, `vulkan` | [The macOS page](build-macos.md) |

For example, `private-ai-framework-core-v2.99.1-amd64.deb`, `llama.cpp-v1.0.0+pai1-amd64.deb`,
`llama.cpp-rocm-v10.1.0-amd64.deb`, `private-ai-framework-v2.99.0-v0.9.0-x64.msi`. Release 2.4 and
earlier named them `llama-cpp-framework-*`: the new packages upgrade those in place (below, per format).

- `<framework>` is the `[workspace.package]` version of the root `Cargo.toml`, one for the
  `pai-*` crates: the configurator and the framework are versioned together.
- `<release>` (`<llama>` in the MSI's name) is `git describe --tags` of the clone, which prefers the
  annotated release tag over the nightly on the same commit. On Linux `xtask package` refuses a clone
  that is not on an exact `vX.Y.Z` release; elsewhere anything else falls back to
  `b<rev-list --count>`, shaped so a non-release package cannot pass for one.
- `<N>` is our build revision of that release, `LLAMA_CPP_REVISION` in `components.rs` (1), raised
  when the patches or the build options change on the same release.
- `<sdk version>` is the version of the SDK in the dist layout. An SDK outside the layout takes the
  framework's version.

**Every OS ships the benchmark method read-only** in `share/benchmarks/`: each `standard-<X.Y>.ini`,
`standard.versions`, and the prompts with their images. `bench_share.rs` stages it, and on Linux and
macOS it is in the `cli` package.

**Every OS ships the configurator's text read-only** in `share/strings/`: `en_US.toml`, the GUI's
language file, and `cli.toml`, the text every program prints (`pai`, `pai-manager`, `pai-bff`).
`strings_share.rs` stages every `*.toml` of `crates/core/strings/`; on Linux `cli.toml` is in the
`core` package, which every other framework package requires, and the GUI's language files are in
`config`. macOS: [its page](build-macos.md).

**llama.cpp's test harnesses are pruned on every OS.** On Windows the installer can put `bin\` on
the system PATH. On Linux every staged executable lands in `/usr/bin`, and on macOS each gets a command
link. Either way,
shipping them would drop generically named `test-*` commands into every shell. cmake stages every
executable with an `install()` rule, so count them from the staging tree, not the build directory.

**Which runtime libraries is never a list.** It is the closure of what the staged binaries link,
restricted to what resolves inside a dist. Everything else stays a system library.

**Every package carries its licences.** The MSI holds them in `share\licenses\` beside `bin\`. On Linux
each package holds its own `/usr/share/private-ai/licenses/<package>/`, and on macOS its own
`/opt/private-ai/share/licenses/<package>/`, since no two packages can own one file (the directory is
named by the component: `llama/` for the `llama.cpp` package). `licenses.rs` (`xtask/src/common/`) stages them:

- This project's licence, llama.cpp's and what llama.cpp's own `licenses/` holds, the configurator's
  icon font, and a text for every bundled library.
- `README.txt` says which text covers which library. llama.cpp's own libraries are covered by its
  licence.
- **The table in `licenses.rs` decides which licence a library has; nothing is guessed.** The package
  is refused when a bundled library (any `.dll`, `.dylib`, `.so` or `.so.*` under the staged tree) is
  not named there, or when its text cannot be found. A new library in a dist is read before it ships.
- A text comes from the dist when it has one, so it matches the bundled version: ROCm's
  `share\doc\<name>\LICENSE*`, CUDA's `LICENSE`, OpenSSL's `license.txt`. Otherwise it comes from
  `installer/licenses/`, whose README says where each was taken from: the Vulkan loader is Apache
  2.0 there on every OS, since it is Khronos' open-source loader code, whatever package carries it.
- NVIDIA's terms are not an open-source licence. They install as `NVIDIA-CUDA-EULA.txt`, and
  `ggml-cuda` needs them too, since it links NVIDIA's static runtime: on Linux both `llama.cpp` and
  `llama.cpp-cuda` carry them.

### The MSI (Windows)

**`xtask package` checks WiX first, then stages, builds into `dist/` and validates.** The check
costs seconds, not a staging run: tool found, EULA accepted, extensions present (added when
missing).

- WiX resolves as `--wix <wix.exe>`, then the recorded `WixExe`, then PATH and the dotnet tools
  directory.
- A non-pinned version gets a warning. WiX 6.0.2 builds the same `.wxs` without the v7 EULA.
- `wix msi validate` runs the ICEs, with ICE61 suppressed.
- It refuses a package of 2 GiB or more, GitHub's asset limit.
- On the 2.8 GB staging tree: a 1.16 GiB MSI in 2 to 3.5 minutes of `wix build`, plus about 40 s of
  validation.

**Staging** uses `cmake --install`, then removes `test*.exe` and `include\`, which never shipped.
Filtering happens in staging and not in the `.wxs`, so what is staged is exactly what ships. The
`.wxs` harvests `bin\`, `lib\`, `share\` and `.kpack\` whole, but for `pai-manager.exe` and `pai-bff.exe`,
each the key path of the component that installs its service. `pai-config.exe`, `pai.exe`,
`pai-manager.exe`, `pai-bff.exe` and `llama.ico` are staged into `bin\` too, from cargo's release output (`msi::CARGO_EXES`), so an administrative image (`msiexec /a`) compares 1:1 with the staging
tree. A missing icon is regenerated here.

| Bundled into `bin\` | How |
| --- | --- |
| OpenSSL `libcrypto-N-x64.dll`, `libssl-N-x64.dll` | Names scanned from every staged binary, copied from `OpenSSLDir\bin` |
| GPU runtimes | Import closure, restricted to the ROCm dist's `bin\`, the CUDA dist's `bin\x64` or `bin`, the Vulkan SDK's `Bin\`, and its `Runtime\` for `vulkan-1.dll` |
| `amd_comgr*.dll` | By name from the ROCm dist: no import table shows it |
| `rocblas\library`, `hipblaslt\library` (`.kpack\blas_lib_*` beside `bin\`) | For the architectures in `GpuTargets` only, plus whatever names no architecture |

The GPU runtimes add about 1.6 GB on top of the binaries. It refuses a `ggml-hip.dll` without
`hipblas.dll`, a `ggml-cuda.dll` without `cublas64_*.dll`, or a `ggml-vulkan.dll` without
`vulkan-1.dll`, since ggml would silently drop that backend on every other machine.

**The Vulkan loader is bundled, because a driver does not guarantee one.** A machine can have the
GPU driver and no `vulkan-1.dll` in System32. The SDK ships no loader of its own, only the import
lib and the runtime installer `Helpers\VulkanRT.exe`, and LunarG no longer publishes the standalone
runtime archive the SDK's version used to ship, so the prerequisites script copies the DLL out of
System32, where that installer leaves it, into `<DistRoot>\VulkanSDK\<version>\windows\Runtime\`.
The DLL must pass Authenticode (`O=LunarG, Inc.`): a driver's own loader, signed by the vendor, is
not bundled, and an SDK already installed gets it on the next run. Staging copies the DLL through
the import closure, and its licence is the vendored Apache 2.0 of the loader's own code, the same
`vulkan-loader.txt` Linux's `libvulkan.so.1` carries.

**The MSI shows a licence page** (WixUI `LicenseAgreementDlg`, with the accept checkbox) between the
welcome page and `CustomizeDlg`. `xtask package` writes its text to `build\windows\license.rtf`
(not installed): which text covers what, the MIT licence, and NVIDIA's terms in full. A silent or
basic-UI install does not show it.

**The MSI installs the `private-ai-manager` service and the machine data tree.** The service is
`pai-manager.exe serve` as LocalSystem, started at boot and restarted after a failure; it runs each
instance as its own service, `llama.cpp_<id>`, under the virtual account `NT SERVICE\llama.cpp_<id>`
(created with the manual start type; automatic once the configurator's "start llama-server when the
system starts" is on, and the SCM then starts it at boot: the manager starts none itself), and serves the configurator over `\\.\pipe\private-ai-manager`. `%ProgramData%\private-ai\` holds
`config\` (the instances' server and preset files, which users reach only through the manager),
`logs\` (each instance's `llama-server@<id>.log`) and `models\`, each folder with a protected DACL,
and outlives an uninstall. A removal, a repair or the old half of an upgrade runs
`pai-manager uninstall-services` to delete the instances' services, which the manager creates at run
time and Windows Installer does not know; the new manager recreates them. The `.wxs` is checked by xtask's tests, the Windows
code type-checked only: none of it has run on Windows yet. Service, pipe, ACLs and the test steps:
[installer\AGENTS.md](../installer/AGENTS.md).

**The MSI installs the web service `private-ai-bff`:** `pai-bff.exe service`, started at boot, as the
virtual account `NT SERVICE\private-ai-bff`, restarted 5 s after a failure. The SCM stops it before its
files are replaced or removed and deletes it on uninstall; the process stopper leaves it alone. It
reads `config\bff.ini` through the read `NT SERVICE\ALL SERVICES` has on `config\`. Type-checked only,
not yet run on Windows: the open checks are in [installer\AGENTS.md](../installer/AGENTS.md).

**An upgrade from release 2.4 is a `MajorUpgrade`:** `private-ai-framework.wxs` keeps the UpgradeCode
of `llama-cpp-framework.wxs`, so it removes the old product first. A directory chosen in 2.4, read from
the old `HKLM\Software\llama.cpp` key, is kept; the old default `Program Files\llama.cpp\` moves to
`Program Files\private-ai\`. The guard against a non-MSI install checks the old and the new Apps entry,
and `StopProcesses` stops a 2.4 `llama-cpp-config.exe` too, which it also asks for the graceful stop when
the install has neither `pai.exe` nor `pai-config.exe`. Details and the test step:
[installer\AGENTS.md](../installer/AGENTS.md).

**System libraries stay external:** the VC++ redist x64 (`libomp140.x86_64.dll` included) and the
GPU drivers. cudart is static. The load-order reasons are in [installer\AGENTS.md](../installer/AGENTS.md).

<details>
<summary>Why the OpenSSL scan works the way it does</summary>

The DLLs are normal imports. Without them, the server fails at load on a machine with no OpenSSL.
The app directory wins the DLL search order, so they go in `bin\`.

- Names are scanned from the binaries, because the ABI number moves.
- Every staged `.exe` and `.dll` is scanned, never a name pattern. llama.cpp tools are thin
  launchers plus impl DLLs: `llama-server.exe` imports `llama-server-impl.dll`, which carries the
  OpenSSL imports. A name-based scan breaks on the next rename.
- Do not copy `*.dll` wholesale. Most of `OpenSSLDir\bin` is providers: `legacy`, `p_test`,
  `p_ossltest`, `p_minimal`.
- The scan reads 4 MB chunks with a 64-byte overlap, because `ggml-hip.dll` is about 1 GB. A search
  for the prefix gates the match.
- No OpenSSL import found in any binary fails the package, since the scan or the link has broken.

</details>

### The `.deb` and `.rpm` (Linux)

**`xtask package` builds eight packages per format from one staged tree, about 3.8 GiB installed with all
of them.** `--deb` or `--rpm` builds one format. It refuses to run without `pai-config` and `pai` in the
build's `bin/` or `pai-manager` and `pai-bff` in `cargo-target/release/`, when the built `pai-config --version` differs from the root `Cargo.toml`'s (a bump
without a rebuild), and when the llama.cpp clone is not on a `vX.Y.Z` release tag.

| Package | Holds | Version | `.deb` / `.rpm` |
| --- | --- | --- | --- |
| `private-ai-framework-config` | `pai-config` (the GUI), its language files, the desktop entry and its icon | `<framework>` | not measured |
| `private-ai-framework-cli` | `pai` (the command line), the benchmark method; no graphics library | `<framework>` | not measured |
| `private-ai-framework-bff` | `pai-bff` (the web service), `private-ai-bff.service` and the sysusers entry of its user | `<framework>` | not measured |
| `private-ai-framework-core` | `pai-manager`, `private-ai-manager.service`, the D-Bus and polkit policies, the sysusers entry of `private-ai`, `cli.toml`, the manager's maintainer scripts | `<framework>` | not measured |
| `llama.cpp` | llama.cpp's executables and libraries, the ggml backends (`libggml-hip.so`, `libggml-cuda.so`, `libggml-vulkan.so`), libomp, the instance unit `llama.cpp@.service` and the scripts that stop and restart the instances; nothing else of the framework | `1:<release>+pai<N>`, `1:1.0.0+pai1` | not measured |
| `llama.cpp-rocm` | The ROCm libraries with RCCL, and the kernel stores | The SDK's, `10.1.0` | 831 / 922 MiB |
| `llama.cpp-cuda` | The CUDA libraries and NCCL | `<cuda>+nccl.<nccl>`, `13.5.0+nccl.2.32.3` | 655 / 675 MiB |
| `llama.cpp-vulkan` | The Vulkan loader | The SDK's, `1.4.363.1` | 2 / 2 MiB |

The runtimes' sizes are of the v1.0.0 build (framework 2.4.0), as `xtask package` printed them, rounded
up to the MiB: they hold the same files as then. The framework's packages and `llama.cpp` divide 2.4's
`config` (18 / 21 MiB) and `llama` (190 / 229 MiB) another way and have not been measured.

**The engine carries its own version, the framework packages only the framework's.** `llama.cpp` is
`1:<release>+pai<N>`: the epoch `1:` because Ubuntu 26.04 ships its own `llama.cpp` at `8681+dfsg-1`,
which apt would otherwise rank above `1.0.0` (the epoch is compared first), and `+pai<N>` our build
revision. The `.rpm` spells it Epoch `1`, Version `<release>`, Release `pai<N>`; every other `.rpm` has
Release `1` and no epoch. A newer llama.cpp therefore installs without a framework rebuild.
`xtask install` compares the epochs and reads an rpm's version with `%{EVR}`.

**A runtime package holds only the external SDK's libraries, so it changes only with the SDK.** A new
llama.cpp on the same SDKs leaves the three as they are. Each is optional: a backend whose runtime is
missing is skipped at start, with only a log line.

**The relations:**

| Relation | `.deb` | `.rpm` |
| --- | --- | --- |
| `config` needs `cli` and `core`; `cli` and `bff` need `core`; each at exactly its version | `Depends` `(= <version>)` | `Requires` `= <version>` |
| `config`, `cli` and `core` need the engine at least at the release they were built against; `bff` does not run it | `Depends: llama.cpp (>= 1:<release>)` | `Requires: llama.cpp >= 1:<release>` |
| `llama.cpp` asks for each runtime at the version it was built against | `Recommends` `(= <version>)` | `Recommends` `= <version>` |
| `llama.cpp` refuses any other version of a runtime, whose symbols a backend may not resolve | `Breaks` `(<< <version>)`, `(>> <version>)` | `Conflicts` `< <version>`, `> <version>` |
| `llama.cpp` keeps out the distribution's package of the same commands | `Conflicts: llama.cpp-tools` (Ubuntu) | `Conflicts: llama-cpp` (Fedora) |
| `config`, `llama.cpp` and the runtimes replace `llama-cpp-framework`, the single package of the releases up to 2.2.0 | `Conflicts`, `Replaces` | `Obsoletes` `<= 2.2.0` |
| The same replace their component's package of releases 2.3 and 2.4 (`llama-cpp-framework-config`, `-llama`, `-rocm`, `-cuda`, `-vulkan`) and stand in for it; `cli`, `bff` and `core` had none | `Conflicts`, `Replaces`, `Provides` `(= <version>)` | `Obsoletes` `< 2.4.1` (`config`, `llama.cpp`) or `<= <version>` (a runtime), `Provides` `= <EVR>` |

**The `Obsoletes` is versioned, the `.deb` pair is not.** rpm matches an unversioned `Obsoletes` against
every version of the name, future ones included, so it would remove a package that later takes the name
back: rpmbuild warns about it. dpkg warns about no such thing and its `Replaces` is only about taking over
the old files, so those stay unversioned. Against the 2.4 component packages, `< 2.4.1` catches every `2.4.0+llama.cpp.<tag>`
(rpm orders it above 2.4.0, below 2.4.1); a runtime is versioned by its SDK, with no last version known
under the old name, so its own version bounds the old package.

**Checked on a real install** (Ubuntu 26.04, 2026-11-14, the `.deb` files of 2.99.1 with llama.cpp
`1:1.0.0+pai1`, reported by the user): the eight packages installed over 2.4.0 and removed its five
packages; the users `private-ai` and `private-ai-bff` were created, `private-ai-manager` and
`private-ai-bff` were active and no `llama-server` was started; `pai --version` printed `2.99.1`; polkit
refused `nobody` the manager's `Stop`, `WriteFile` and `ReadFile` of `server@default.ini` with
`NotAuthorized`; the bff answered `/api/version`, `/api/instances` with `[]` and 200, and 503
`unavailable` with the manager stopped. The `.rpm` files have not been installed.

On the same install, the same day, with the start at boot of step 12 (reported by the user):
`pai server boot on` left `systemctl is-enabled llama.cpp@default` at `enabled` and created the link in
`multi-user.target.wants/`, `pai server boot off` left it `disabled`, and the Settings toggle matched
systemd both ways. Reinstalling at the same version after `llama.cpp@.service` moved from `core` to
`llama.cpp` needed a one-off `dpkg --force-overwrite`, since the installed `core` of that version still owned the
file. Only pre-release builds of one version hit it; no fix is planned. The stop and restart of the services by the scripts (below) were checked in
xtask's tests with a fake `systemctl`, not on this install.

| Path | Package | Holds |
| --- | --- | --- |
| `/usr/bin/` | `llama.cpp`; `pai-config` in `config`, `pai` in `cli` | llama.cpp's executables, `pai-config` and `pai`, the real files |
| `/usr/libexec/private-ai/` | `core`; `pai-bff` in `bff` | `pai-manager` and `pai-bff`, which the units call by full path |
| `/usr/lib/private-ai/` | `llama.cpp` | llama.cpp's libraries (`libllama`, `libmtmd`) and the ggml backends, a private directory, never a system library directory |
| `/usr/lib/private-ai/lib/` | The runtimes; `llvm/lib/libomp.so` in `llama.cpp` | The bundled runtimes |
| `/usr/lib/private-ai/.kpack/` | `llama.cpp-rocm` | hipBLASLt's `blas_lib_*` and RCCL's `rccl_lib_*` archives; ROCm looks in `<libdir>/../.kpack`, so they cannot sit at the top |
| `/usr/share/private-ai/benchmarks/` | `cli` | The benchmark method |
| `/usr/share/private-ai/strings/` | `config`; `cli.toml` in `core` | The GUI's `en_US.toml`, and `cli.toml`, the text every program prints |
| `/usr/share/private-ai/licenses/<component>/` | Each its own (`llama/` for `llama.cpp`) | The licence texts and their `README.txt` |
| `/usr/share/` | `config`; `doc/<package>/copyright` in each | The desktop entry `org.privateai.Config.desktop` ("Private AI Framework" in the menu), its icon, the copyright file |
| `/usr/lib/systemd/system/`, `/usr/lib/sysusers.d/`, `/usr/share/dbus-1/system.d/`, `/usr/share/polkit-1/actions/` | `core`; `llama.cpp@.service` in `llama.cpp`; `private-ai-bff.service` and `private-ai-bff.conf` in `bff` | The services' system files, below |

**Each package's `copyright` and `Homepage` (rpm `URL`) name its own source.** The framework's
packages carry this project's MIT licence and repository; `llama.cpp` carries llama.cpp's own licence
and its upstream, `https://github.com/ggml-org/llama.cpp`; a runtime's describes the SDK libraries it
redistributes, with the same upstream.

**Staging skips `cmake --install` and places each file of the build's `bin/` by name** (`place` in
`xtask/src/linux/package.rs`): a `lib*.so*` in `/usr/lib/private-ai`, every other executable in
`/usr/bin`, library symlinks kept as symlinks. `pai-manager` and `pai-bff` come from cargo's target dir.
`paths::llama_bin` finds `llama-server` beside `pai-config` and `pai` in `/usr/bin`, and from `pai-manager` in
`/usr/libexec/private-ai` through `../../bin`; `paths::installed_share_dir` finds the data in
`/usr/share/private-ai` from `/usr/bin` and from `/usr/libexec/private-ai` alike
(`paths::installed_share_candidates`), so the services print texts, not bare string keys. The rpm `%files` owns no system directory, and the private libraries stay out
of `Provides`.

**ggml finds its backends through `GGML_BACKEND_DIR=/usr/lib/private-ai`** (`xtask/cmake/linux.toml`),
which ggml searches before the executable's directory and the working directory
(`ggml_backend_load_best` in `ggml/src/ggml-backend-reg.cpp`). So while the `llama.cpp` package is installed,
a `llama-server` built in the tree loads the installed backends, not its own; with nothing installed
there, it finds its own beside it.

**The runtimes are the `ldd` closure of the staged files**, restricted to the ROCm, CUDA and NCCL
dists and the Vulkan SDK's loader directory. Each lands under its soname with symlinks dereferenced.

- `libcuda.so.1` is the NVIDIA driver's and stays out.
- If the system loader answered for Vulkan, the SDK's `libvulkan.so.1` is taken by name.
- A `libggml-hip.so` without `libhipblas.so.*`, or a `libggml-cuda.so` without `libcublas.so.*`, is
  refused.
- `libamd_comgr.so.3` is a plain `DT_NEEDED` of `libamdhip64.so.7`, so the closure carries it.
- `librccl.so.1` and its dependencies arrive through the same closure.
- A RUNPATH precedes the system library directories, so no system ROCm can get mixed in, short of
  `LD_LIBRARY_PATH`.

**The ROCm part mirrors the dist's `lib/` layout** (`lib/`, `lib/llvm/lib/`,
`lib/rocm_sysdeps/lib/`, `lib/rocblas/library/`, `lib/hipblaslt/library/`). AMD's libraries find
each other and their kernel files through `$ORIGIN`-relative RUNPATHs and paths, so a mirrored tree
needs no patching of anything AMD shipped. `libomp.so` comes along in the `llama.cpp` package, not `llama.cpp-rocm`,
because ggml-base itself links the dist's OpenMP runtime.

CUDA (`libcudart`, `libcublas`, `libcublasLt`, about 575 MiB), NCCL and the Vulkan loader go flat
into `lib/`, in the `cuda` and `vulkan` packages.

**Kernel data, which `ldd` cannot show, is copied only for `gpu_targets`,** files or per-arch
directories alike, plus whatever names no architecture.

| Store | Packaged | Whole dist |
| --- | --- | --- |
| `lib/rocblas/library` | 359 MiB | 810 MiB |
| `lib/hipblaslt/library` | 467 MiB | 2198 MiB |
| `.kpack` | 91 MiB | 2258 MiB |

**RUNPATH is rewritten with `patchelf` on the ELF files staged in `/usr/bin` and `/usr/lib/private-ai`
only** (the runtimes keep AMD's), after `strip --strip-unneeded` and never before, since stripping a
patched file can drop the section patchelf appended. The value is absolute,
`/usr/lib/private-ai:/usr/lib/private-ai/lib:/usr/lib/private-ai/lib/llvm/lib`. `LD_LIBRARY_PATH`
still wins over RUNPATH. That no file is opened under the dists or the build tree was checked by hand
(extracting the `.deb`, `strace` of a generation per backend, not recorded) with a `$ORIGIN`-relative
RUNPATH; it has not been repeated with the absolute one.

**No config file is shipped.** The server and preset files (`server@<id>.ini`, `presets.ini`,
`presets.ini.disabled`) are in `/etc/private-ai/`, which systemd creates at the manager's start
(`ConfigurationDirectory=`) and which only the manager writes. `pai-config` keeps `settings.ini` per
user in `${XDG_CONFIG_HOME:-~/.config}/private-ai/` (`paths::config_dir`) and moves release 2.4's
`~/.config/llama.cpp/` and `~/.local/share/llama.cpp/` there at its first start
([Migration from 2.4](../crates/config/README.md#migration-from-24)).

**The services' system files are in `core`, beside `pai-manager`, the instance unit in `llama.cpp`, and the web service's unit and user in `bff`** (`xtask/src/linux/system_files.rs`,
installed 0644, overwritten on every upgrade; local changes go in a drop-in, `systemctl edit`):

| File | What it does |
| --- | --- |
| `/usr/lib/systemd/system/private-ai-manager.service` | `pai-manager serve` as root, `Type=dbus` on `org.privateai.Manager1`, `HOME=/var/lib/private-ai` so a blank `ModelsDir` means the same as for the instances, `ConfigurationDirectory=private-ai`, `Restart=on-failure`, enabled for `multi-user.target` |
| `/usr/lib/systemd/system/llama.cpp@.service` | `pai-manager run-instance %i` as user `private-ai` (supplementary groups `render`, `video`), `StateDirectory=` for its home `/var/lib/private-ai` and the data root under it, `Restart=on-abnormal`, `ConditionFileIsExecutable=/usr/libexec/private-ai/pai-manager` (without the framework it does not run: the engine does not require it); `[Install] WantedBy=multi-user.target`, so an instance starts at boot once enabled, which no package does (the configurator's "start llama-server when the system starts", through the manager's `SetBootEnabled`); its output goes to journald |
| `/usr/share/dbus-1/system.d/org.privateai.Manager1.conf` | Only root owns the name; anyone may call it, and polkit decides in each call |
| `/usr/share/polkit-1/actions/org.privateai.manager.policy` | `org.privateai.manager.control` (start, stop, restart): `yes` in the active session, `auth_admin` otherwise; `org.privateai.manager.configure` (writing a file): `auth_admin_keep` |
| `/usr/lib/sysusers.d/private-ai.conf` | The system user `private-ai`, home `/var/lib/private-ai`, in `render` and `video` |
| `/usr/lib/systemd/system/private-ai-bff.service` (`bff`) | `pai-bff`, `Type=exec`, as the static user and group `private-ai-bff`, hardened (`ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `RemoveIPC=yes`, `NoNewPrivileges=yes`, `RestrictSUIDSGID=yes`), with no capability, no device, the kernel and cgroups out of reach and only `AF_UNIX`, `AF_INET`, `AF_INET6` sockets; it reads `/etc/private-ai/bff.ini` and writes nothing; `Restart=on-failure`, enabled for `multi-user.target` |
| `/usr/lib/sysusers.d/private-ai-bff.conf` (`bff`) | The system user `private-ai-bff`, with no home and no shell |

**The bff runs as a static user, not a `DynamicUser=`:** it calls the manager on the system bus, and
dbus-daemon drops a dynamic user's connection at `Hello`. On Ubuntu 26.04 (dbus-daemon 1.16.2) a
`DynamicUser=yes` bff answered `/api/instances` with 503; the reproduction and the upstream reports
are in the unit's test in `system_files.rs`. The sandbox `DynamicUser=` implied is written out instead.

Their texts (the unit descriptions, the polkit descriptions and messages) are read in place by systemd
and polkit, so they are literal in the code, not in the strings files.

**The maintainer scripts are `core`'s, `llama.cpp`'s and `bff`'s**, in debhelper's form for the `.deb`
(`postinst`, `prerm`, `postrm`) and Fedora's systemd macros written out for the `.rpm` (`%pre`, `%post`,
`%preun`, `%postun`, `%posttrans`). **Each package stops the services it holds before its files are
replaced, on upgrade as on removal, and an upgrade starts again what was running.** The `.deb` stops in
`prerm` (`upgrade`, `remove`, `deconfigure`; not under `DPKG_ROOT`), the `.rpm` on an upgrade in the new
package's `%pre`, since rpm runs the old package's `%preun` only after the new files are in place
(rpm-scriptlets(7), EXECUTION ORDER).

| When | `core`: the manager | `llama.cpp`: the instances |
| --- | --- | --- |
| Install | `systemd-sysusers private-ai.conf`, enable the manager, `daemon-reload`, reload the bus policies, start the manager (the `.rpm` in `%posttrans`, once the old package's files are gone) | `daemon-reload`; no instance is enabled or started |
| Upgrade | Stop the manager; afterwards the same as an install, the manager restarted (the `.rpm` only while it is enabled) | Record the active `llama.cpp@*` units in `/run/llama.cpp.instances` and stop them; afterwards `daemon-reload`, start each recorded unit, delete the record |
| Removal | Stop the manager; `daemon-reload` (the `.rpm` also disables it) | Stop every instance; `daemon-reload`, delete the record. An enabled instance stays enabled: its `multi-user.target.wants/` link is left behind (open, `PLAN.md` step 12) |
| Purge (`.deb`) | Forget the manager's enablement state; `/etc/private-ai` and `/var/lib/private-ai` are kept | Nothing more |

`core`'s scripts never touch the instances, and none enables one: an instance starts at boot only once
the user enables it. `bff`'s scripts do the same as `core`'s for `private-ai-bff.service` alone:
`systemd-sysusers private-ai-bff.conf`, then enable and start it on install, stop it before an upgrade
and restart it after (the `.rpm` in `%posttrans`, while it is enabled), stop it on removal (the `.rpm`
also disables it), and forget its enablement state on purge. These scripts run in xtask's tests in a
sandbox with a fake `systemctl`.

The `.deb` `Depends` on `systemd | systemd-standalone-sysusers | systemd-sysusers` for those scripts;
the `.rpm` of `core` and of `bff` `Requires(pre)`, `(post)`, `(preun)`, `(postun)` and `(posttrans)` systemd (`llama.cpp`'s none), and `core`'s takes weak dependencies on the `render` and `video` groups, which the sysusers `m` lines
create when missing.

<details>
<summary>Dependencies, payload and metadata</summary>

**`.deb` `Depends`** come from `dpkg-shlibdeps` over each package's staged ELF files, with the library
directories of all eight given as private paths, so a library another package carries asks for nothing.
`libvulkan1` is dropped because the loader is bundled.

- `libggml-cuda.so` and the `libcu*` files are left out of the scan. They link the driver's
  `libcuda.so.1`, which no Debian package provides, and `dpkg-shlibdeps` fails on a library it
  cannot find.
- A fixed list is the fallback of every package but the runtimes (font libraries for `config` alone). A runtime asks for nothing it does not bring along.
- What the configurator only `dlopen`s (xkbcommon, Wayland, X11, EGL) is `Recommends` of `config` alone:
  neither the Vulkan loader nor the Vulkan backend names those libraries.

**`.rpm`** keeps rpm's automatic scan.

- `__requires_exclude` covers every library of the eight, and `libcuda`.
- Those libraries are excluded from `Provides` too.
- `rpmbuild` runs against a throwaway `_dbpath`, since a Debian host has no rpm database.

**Payload and modes.** The payload is zstd in both formats, because xz takes many minutes on 3 GiB
of GPU kernels. Modes are normalized to 755 and 644, because the repo drive may hand out
group-writable ones.

**Icon.** It is the largest PNG frame of `resources/llama.ico`, cut out of the container by xtask,
so no image tool is needed.

**Licence metadata.** The `.rpm` `License:` field is computed from what each package bundles: an SPDX
expression, with `LicenseRef-NVIDIA-CUDA-EULA` for NVIDIA's terms. `usr/share/doc/<pkg>/copyright`
includes that package's `share/licenses/<package>/README.txt` and points to the directory.

**Maintainer.** `PRIVATE_AI_MAINTAINER` sets it, else git's `user.name` and `user.email`.

</details>

## Install

**`xtask install` covers first install, upgrade, same-version reinstall and downgrade with one
command.** On Windows it takes the last-written MSI of this architecture in `dist/`, or the file given. On
Linux and macOS it takes the last-written file of each package, or only the files given: that is how to
leave out the runtime of a GPU the machine does not have. `--dry-run` prints the commands.

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Command | `msiexec /i <msi> /qb /norestart /l*v <log>` | Package manager, see below | `sudo installer -pkg <pkg> -target /`, per package |
| Downgrade | Removes the installed version first: `msiexec /x {ProductCode} /qb` | Package manager's own | Laid down over it |
| Running programs | The MSI's `StopProcesses` action stops them | A llama-server outside the services is refused (`--force` overrides) | Refused (`--force` overrides) |
| Success check | `DisplayVersion` matches the package | `pai-config --version`, when `config` is among the files | `pai-config --version`, when `config` is among the files |

**Windows.**

- Basic UI, not `/qn`: the package is per-machine, and only a UI level that can show the UAC prompt
  elevates from an unelevated shell.
- All features go to the `InstallDir` an earlier install recorded, or the default. For another
  choice, run the `.msi` itself.
- An upgrade and a same-version repackage are the MSI's own `MajorUpgrade`.
- The MSI refuses a downgrade, hence the removal first. The ProductCode comes from the Apps entry's
  `UninstallString`, and basic UI keeps the user data.
- Success reads `DisplayVersion` of `HKLM\...\Uninstall\private-ai`. Exit 3010 (restart required)
  counts as success with a warning.

**Linux.**

| Format | Command |
| --- | --- |
| `.deb` | `apt-get install -y --reinstall --allow-downgrades`, every file in one transaction |
| `.rpm` with dnf | `dnf install`, `downgrade` and `reinstall`, one command per action, the rising versions first |
| `.rpm` with zypper | `zypper --non-interactive install --force --allow-unsigned-rpm`, every file in one transaction |
| `.rpm` otherwise | `rpm -Uvh --replacepkgs --oldpackage`, every file in one transaction |

- The format comes from `ID` and `ID_LIKE` in `/etc/os-release`, and the package managers on PATH
  only second, since a Debian build host has `rpm` too.
- A running llama-server is refused because rocBLAS and hipBLASLt load their kernel files lazily: a
  server outliving the upgrade would read the new kernels with the old libraries. Only a
  `llama-server` outside the framework's services counts, one run from `/usr/bin` or mapping a file
  of `/usr/lib/private-ai` (`/proc/<pid>/cmdline`, `/proc/<pid>/maps`). A process in the cgroup of a
  system unit `llama.cpp@*.service` or `private-ai-*.service` under `/system.slice/`
  (`/proc/<pid>/cgroup`) is a service, which its package's scripts stop and start again. Any other
  program (an open `pai-config`, a `pai bench run`, a `llama-bench`) is ignored without a message.
- It warns when a package other than a runtime predates the last `xtask build`. A runtime comes out of an SDK,
  which the build does not change.
- `sudo` is added when not root, here and on macOS.
