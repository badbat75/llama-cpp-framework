---
title: Packaging and install
type: reference
updated: 2026-11-01
---

# Packaging and install

What `xtask package` puts in the MSI, the `.deb` and the `.rpm`, and how `xtask install` lays a
package down. The `.pkg` is on [the macOS page](build-macos.md). The MSI's own authoring is in
[installer\AGENTS.md](../installer/AGENTS.md), and the phases and what breaks are in
[Build pipeline](build-pipeline.md).

## Packaging

**Every package is self-contained: the target machine needs the GPU drivers and little else.** On
Windows that also means the VC++ runtime. Each OS stages into `build/<os>/staging/`, and the staged
tree is the installed tree.

| OS | File in `dist/` |
| --- | --- |
| Windows | `llama-cpp-framework-v<framework>-<llama>-x64.msi` (`arm64` on an ARM build) |
| Linux | `llama-cpp-framework-v<framework>-<llama>-amd64.deb`, `-x86_64.rpm` (`arm64`, `aarch64` on ARM) |
| macOS | `llama-cpp-framework-v<framework>-<llama>-arm64.pkg` |

For example, `llama-cpp-framework-v2.2.0-v0.7.0-x64.msi`.

- `<framework>` is the `[package]` version of `llama-cpp-config/Cargo.toml`: the configurator and
  the framework are versioned together.
- `<llama>` is `git describe --tags` of the clone, which prefers the annotated release tag over the
  nightly on the same commit.
- Anything but an exact `vX.Y.Z` falls back to `b<rev-list --count>`, shaped so a non-release
  package cannot pass for one.

**Every OS ships the benchmark method read-only** in `share/benchmarks/`: each `standard-<X.Y>.ini`,
`standard.versions`, and the prompts with their images. `bench_share.rs` stages it.

**llama.cpp's test harnesses are pruned on every OS.** On Windows the installer can put `bin\` on
the system PATH. On Linux and macOS every staged executable gets a command link. Either way,
shipping them would drop generically named `test-*` commands into every shell. cmake stages every
executable with an `install()` rule, so count them from the staging tree, not the build directory.

**Which runtime libraries is never a list.** It is the closure of what the staged binaries link,
restricted to what resolves inside a dist. Everything else stays a system library.

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
`.wxs` harvests `bin\`, `lib\`, `share\` and `.kpack\` whole. `llama-cpp-config.exe` and `llama.ico`
are staged into `bin\` too, so an administrative image (`msiexec /a`) compares 1:1 with the staging
tree. A missing icon is regenerated here.

| Bundled into `bin\` | How |
| --- | --- |
| OpenSSL `libcrypto-N-x64.dll`, `libssl-N-x64.dll` | Names scanned from every staged binary, copied from `OpenSSLDir\bin` |
| GPU runtimes | Import closure, restricted to the ROCm dist's `bin\`, the CUDA dist's `bin\x64` or `bin`, the Vulkan SDK's `Bin\` |
| `amd_comgr*.dll` | By name from the ROCm dist: no import table shows it |
| `rocblas\library`, `hipblaslt\library` (`.kpack\blas_lib_*` beside `bin\`) | For the architectures in `GpuTargets` only, plus whatever names no architecture |

The GPU runtimes add about 1.6 GB on top of the binaries. It refuses a `ggml-hip.dll` without
`hipblas.dll`, or a `ggml-cuda.dll` without `cublas64_*.dll`, since ggml would silently drop that
backend on every other machine.

**System libraries stay external:** the VC++ redist x64 (`libomp140.x86_64.dll` included), the GPU
drivers, and `vulkan-1.dll`, which ships with any GPU driver and is not in the SDK as a DLL. cudart
is static. The load-order reasons are in [installer\AGENTS.md](../installer/AGENTS.md).

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

**`xtask package` builds both from one staged tree, about 1.2 GB each and 3.3 GB installed.**
`--deb` or `--rpm` builds one. It refuses to run when the built `llama-cpp-config --version` differs
from `Cargo.toml`, a bump without a rebuild.

**The package version is `<framework>+llama.cpp.<tag>`**, for example `2.0.0+llama.cpp.v0.7.0`. A
rebuild on a newer llama.cpp alone is then an upgrade to apt and dnf. dpkg and rpm both order
`2.0.0` < `2.0.0+llama.cpp.v0.7.0` < `2.0.0+llama.cpp.v0.10.0` < `2.0.1+...`.

| Path | Holds |
| --- | --- |
| `/opt/llama.cpp/bin/` | Executables, llama.cpp's libraries, ggml backends, `llama-cpp-config`, copied from the build's `bin/` |
| `/opt/llama.cpp/lib/` | The bundled runtimes |
| `/opt/llama.cpp/.kpack/` | hipBLASLt's `blas_lib_*` and RCCL's `rccl_lib_*` archives |
| `/opt/llama.cpp/share/benchmarks/` | The benchmark method |
| `/usr/bin/` | One relative symlink per executable; rpm warns on absolute ones |
| `/usr/share/` | The desktop entry ("llama.cpp Config" in the menu), its icon, the licences |

**`bin/` stays one directory, so it skips `cmake --install`,** which would split it into `bin/` and
`lib/`. ggml looks for its backends next to the executable (`/proc/self/exe`, so through a symlink
too), and `paths::llama_bin` looks for `llama-server` next to the configurator.

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
needs no patching of anything AMD shipped. `libomp.so` comes along, because the CPU backends link
the dist's OpenMP runtime.

CUDA (`libcudart`, `libcublas`, `libcublasLt`, about 575 MiB), NCCL and the Vulkan loader go flat
into `lib/`.

**Kernel data, which `ldd` cannot show, is copied only for `gpu_targets`,** files or per-arch
directories alike, plus whatever names no architecture.

| Store | Packaged | Whole dist |
| --- | --- | --- |
| `lib/rocblas/library` | 359 MiB | 810 MiB |
| `lib/hipblaslt/library` | 467 MiB | 2198 MiB |
| `.kpack` | 91 MiB | 2258 MiB |

**RUNPATH is rewritten with `patchelf` on llama.cpp's own ELF files only**, after
`strip --strip-unneeded` and never before, since stripping a patched file can drop the section
patchelf appended. The value is `$ORIGIN:$ORIGIN/../lib:$ORIGIN/../lib/llvm/lib`. `LD_LIBRARY_PATH`
still wins over RUNPATH. Extracting the `.deb` and tracing a generation per backend with `strace`
showed no file opened under the dists or the build tree (read by hand, not recorded).

**No config directory is shipped.** `llama-cpp-config` writes the config files per user, in
`${XDG_CONFIG_HOME:-~/.config}/llama.cpp/` (`paths::config_dir`). So the packages have no maintainer
scripts and create no group.

<details>
<summary>Dependencies, payload and metadata</summary>

**`.deb` `Depends`** come from `dpkg-shlibdeps` over the staged ELF files, bundled runtimes
included, with `libvulkan1` dropped because the loader is bundled.

- `libggml-cuda.so` and the `libcu*` files are left out of the scan. They link the driver's
  `libcuda.so.1`, which no Debian package provides, and `dpkg-shlibdeps` fails on a library it
  cannot find.
- A fixed list is the fallback.
- What the configurator and the Vulkan backend only `dlopen` (xkbcommon, Wayland, X11, EGL) is
  `Recommends`.
- It conflicts with `llama.cpp`.

**`.rpm`** keeps rpm's automatic scan.

- `__requires_exclude` covers every library the package carries, and `libcuda`.
- Those libraries are excluded from `Provides` too.
- `rpmbuild` runs against a throwaway `_dbpath`, since a Debian host has no rpm database.
- It conflicts with `llama-cpp`.

**Payload and modes.** The payload is zstd in both formats, because xz takes many minutes on 3 GiB
of GPU kernels. Modes are normalized to 755 and 644, because the repo drive may hand out
group-writable ones.

**Icon.** It is the largest PNG frame of `resources/llama.ico`, cut out of the container by xtask,
so no image tool is needed.

**Maintainer.** `LLAMA_FW_MAINTAINER` sets it, else git's `user.name` and `user.email`.

**A `-` in the llama.cpp token** becomes `.` in the version, since rpm's Version does not allow it.

</details>

## Install

**`xtask install` covers first install, upgrade, same-version reinstall and downgrade with one
command.** It takes the last-written package of this architecture in `dist/`, or the file given.
`--dry-run` prints the commands.

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Command | `msiexec /i <msi> /qb /norestart /l*v <log>` | Package manager, see below | `sudo installer -pkg <pkg> -target /` |
| Downgrade | Removes the installed version first: `msiexec /x {ProductCode} /qb` | Package manager's own | Laid down over it |
| Running programs | The MSI's `StopProcesses` action stops them | Refused (`--force` overrides) | Refused (`--force` overrides) |
| Success check | `DisplayVersion` matches the package | `llama-cpp-config --version` | `llama-cpp-config --version` |

**Windows.**

- Basic UI, not `/qn`: the package is per-machine, and only a UI level that can show the UAC prompt
  elevates from an unelevated shell.
- All features go to the `InstallDir` an earlier install recorded, or the default. For another
  choice, run the `.msi` itself.
- An upgrade and a same-version repackage are the MSI's own `MajorUpgrade`.
- The MSI refuses a downgrade, hence the removal first. The ProductCode comes from the Apps entry's
  `UninstallString`, and basic UI keeps the user data.
- Success reads `DisplayVersion` of `HKLM\...\Uninstall\llama.cpp`. Exit 3010 (restart required)
  counts as success with a warning.

**Linux.**

| Format | Command |
| --- | --- |
| `.deb` | `apt-get install -y --reinstall --allow-downgrades` |
| `.rpm` with dnf | `dnf install`, `reinstall` or `downgrade` |
| `.rpm` with zypper | `zypper --non-interactive install --force --allow-unsigned-rpm` |
| `.rpm` otherwise | `rpm -Uvh --replacepkgs --oldpackage` |

- The format comes from `ID` and `ID_LIKE` in `/etc/os-release`, and the package managers on PATH
  only second, since a Debian build host has `rpm` too.
- Running programs are refused because rocBLAS and hipBLASLt load their kernel files lazily. A
  server outliving the upgrade would read the new kernels with the old libraries.
- It warns when the package predates the last `xtask build`.
- `sudo` is added when not root, here and on macOS.
