---
title: Building on macOS
type: reference
updated: 2026-11-14
---

# Building on macOS

The macOS build from prerequisites to package, kept in one place because it differs from the other
systems in every phase. What it shares with them is linked where it applies, and the phases are in
[Build pipeline](build-pipeline.md).

**The macOS build is Apple Silicon only and untested.** It was written on Linux and compile-checked
for `aarch64-apple-darwin` only, so the first run on a Mac is its test. `configure` refuses another
architecture. The phases are in `xtask/src/macos/`, and everything generated goes to `build/macos/`.

**Backends.** Metal is part of the system, and Vulkan runs through MoltenVK. No CUDA or ROCm exists
there. The CPU backend keeps `GGML_CPU_ALL_VARIANTS`, which upstream implements on Apple as
`apple_m1`, `apple_m2_m3` and `apple_m4`.

`xtask/cmake/macos.toml` follows upstream's own macOS release build:

| Option | Why |
| --- | --- |
| `GGML_CUDA=OFF`, `GGML_HIP=OFF` | Neither exists there |
| `GGML_METAL=ON` | Stated rather than inherited |
| `GGML_METAL_EMBED_LIBRARY=ON` | No `default.metallib` to find at run time |
| `CMAKE_OSX_ARCHITECTURES=arm64` | Apple Silicon only |
| `CMAKE_OSX_DEPLOYMENT_TARGET=13.3` | The oldest macOS the binaries run on, upstream's value |
| `LLAMA_BUILD_BORINGSSL=ON` | HTTPS through a static BoringSSL fetched at configure time: no OpenSSL dylib, no Homebrew path |

**Prerequisites.** `scripts/00-install-prerequisites.sh` does these in order:

1. Checks the Xcode Command Line Tools with `xcode-select -p`. Their installer is a GUI dialog, so
   it is reported and not run.
2. Installs the Homebrew packages `cmake ninja node` (`--skip brew`). Homebrew itself is checked,
   not installed.
3. Installs Rust as [on Linux](build-prerequisites.md#linux-prerequisites).
4. Runs `xtask prereq`: sccache (the `aarch64-apple-darwin` asset) and the Vulkan SDK.

**Dist layout.** The layout is `<root>/VulkanSDK/<Version>/macos`, with the root defaulting to
`~/dists`, [configurable as on the other systems](build-prerequisites.md#dist-layout-and-root).

- The newest version is LunarG's `sdk/latest/mac.txt`.
- The zip `vulkansdk-macos-<version>.zip` is checked against `files.json` (`mac` key) and unpacked
  with `ditto`. It needs 4 GiB free.
- Its installer app runs unattended:
  `--root <dir> --accept-licenses --default-answer --confirm-command install copy_only=1`.
  `copy_only=1` writes nothing to `/usr/local` and needs no sudo.
- `find_vulkan` (`xtask/src/macos/dists.rs`) requires the marker, `vulkan.h`, `glslc`,
  `libvulkan.1.dylib` and the MoltenVK manifest.

**Configure** writes `build/macos/config-build.toml`: Apple's `clang` and `clang++` by full path
(`xcrun --find`), the dist root, the clone, build type and jobs.

**Build** uses the [shared checkout and patches](build-compiling.md#release-tag-and-patches) and
[options](build-compiling.md#cmake-options), Apple's clang and Ninja. The Vulkan SDK's include
directory, loader and `glslc` are
[pinned as on Linux](build-compiling.md#pinning-the-vulkan-sdk-and-the-cuda-toolkit). The build
tree's binaries find the SDK's loader through their build RPATH. A changed compiler path wipes the build directory, since a
new Xcode can move it.

**Package.** `pkgbuild` writes four `.pkg` files from one staged tree, split by path as
[on Linux](build-packaging.md#packaging). It refuses to run without `llama-server`, `pai-config` and `pai`
in the build's `bin/` (`xtask build framework` copies the last two there), and refuses a built
`pai-config` whose version differs from the root `Cargo.toml`'s. It takes no product: `xtask package
llama` or `framework` is refused on macOS, which builds every `.pkg` at once. `xtask build llama` and
`build framework` work as on Linux.

**The macOS split of the engine from the framework is step 11 slice 4 in `PLAN.md`, not done.** The
table below is the `.pkg` set before step 11. `components.rs` and `components::file_stem`, which macOS
shares with Linux, already changed with the Linux split: read from the code, and not built on a Mac,
`xtask package` would now also write a `core` package holding `cli.toml`, and name the engine's file
`llama.cpp-v<framework>-arm64.pkg`, the Vulkan one `llama.cpp-vulkan-v<sdk>-arm64.pkg` and the
framework's without the llama.cpp tag. Slice 4 settles it.

| Package | File | Identifier ends in |
| --- | --- | --- |
| `config` | `private-ai-framework-config-v<framework>-<llama>-arm64.pkg` | `.config` |
| `cli` | `private-ai-framework-cli-v<framework>-<llama>-arm64.pkg` | `.cli` |
| `llama` | `private-ai-framework-llama-v<framework>-<llama>-arm64.pkg` | `.llama` |
| `vulkan` | `private-ai-framework-vulkan-v<sdk>-arm64.pkg` | `.vulkan` |

The identifier starts with `org.privateai.framework`. Release 2.4's packages had other names and identifiers, and nothing in `xtask` removes a 2.4 install under `/opt/llama.cpp`. `config`, `cli` and `llama` carry the framework's
version, and `vulkan` the SDK's, or the framework's for an SDK outside the layout.

**A `.pkg` has no dependency mechanism, so nothing ties the four together.** `xtask install` lays down the newest of
each with `installer`, or only the files given. Without the `vulkan` package ggml skips that backend and Metal runs.

| Path | Package | Holds |
| --- | --- | --- |
| `/opt/private-ai/bin/` | `llama`; `pai-config` in `config`, `pai` in `cli` | The build's `bin/` without `test-*`, `pai-config` and `pai` beside the server |
| `/opt/private-ai/lib/` | `vulkan` | The Vulkan loader and MoltenVK |
| `/opt/private-ai/share/benchmarks/` | `cli` | The benchmark method |
| `/opt/private-ai/share/strings/` | `config`; `cli.toml` in `llama` | The GUI's `en_US.toml`, and `cli.toml`, the text `pai` prints (so `pai` needs the `llama` package) |
| `/opt/private-ai/share/licenses/<suffix>/` | Each its own | The licence texts, with the MoltenVK and Vulkan loader ones in `vulkan/`; see [Packaging](build-packaging.md#packaging) |
| `/usr/local/bin/` | `llama`; `pai-config` in `config`, `pai` in `cli` | A symlink per executable |
| `/usr/local/share/vulkan/icd.d/private-ai-framework-MoltenVK_icd.json` | `vulkan` | The driver manifest, with an absolute `library_path` into `/opt/private-ai/lib` |

- The manifest goes there because the loader does not look next to itself. A system-wide Vulkan SDK
  install adds its own MoltenVK manifest there too.
- The bundled libraries are the `otool -L` closure of the staged files restricted to the SDK. Any
  other non-system dependency, such as a Homebrew dylib, fails the phase.
- Every staged Mach-O loses its build rpaths and gets `@loader_path`, plus `@loader_path/../lib` in
  `bin/`.
- Each then gets an ad hoc signature (`codesign --sign -`), because arm64 runs no unsigned or
  modified-and-unsigned code.
- The package is unsigned and not notarized, so Gatekeeper blocks it when downloaded through a
  browser.

**The configurator** builds with its existing non-Linux, non-Windows fallbacks: start at login, GPU
monitoring and similar are not ported. Its paths follow the Linux ones (`~/.local/share/private-ai`,
`~/.config/private-ai`) per `paths.rs`, and so does the move of release 2.4's trees at its start.
