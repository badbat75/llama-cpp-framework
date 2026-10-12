#!/usr/bin/env bash
# Build llama.cpp with CUDA + Vulkan + HIP (ROCm) support on Linux, the
# counterpart of 02-build.ps1: same release-tag selection, same out-of-tree
# patches, same checked-in cmake-options.psd1. docs/build-pipeline.md explains
# the decisions both scripts share; the comments here cover what differs.
#
# Then builds llama-cpp-config (the Rust GUI + CLI configurator) and puts it
# next to llama-server. There is no packaging step on Linux: the product of
# this script is the build tree, build/linux/llama.cpp-cmake/bin.

. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/common.sh"
load_build_config

say ''
say_warn '  === llama.cpp (CMake) ==='

git_l() { git -C "$CFG_LLAMA_CPP_DIR" "$@"; }

# Clone llama.cpp if missing, otherwise fetch. Either way the build is from the
# latest RELEASE tag (vX.Y.Z), never master HEAD and never a bNNNN nightly.
if [[ ! -f "$CFG_LLAMA_CPP_DIR/CMakeLists.txt" ]]; then
    say_warn "llama.cpp not found at $CFG_LLAMA_CPP_DIR, cloning..."
    git clone https://github.com/ggerganov/llama.cpp "$CFG_LLAMA_CPP_DIR"
else
    say_head 'Fetching latest llama.cpp (branches + tags)...'
    git_l fetch origin --tags
fi

# ── Out-of-tree source patches (PROVISIONAL by design) ─────────────
# patches/llama.cpp/*.patch, shared with the Windows build: reverted before a
# checkout that moves HEAD, re-applied onto the fresh tag, and FATAL when one
# applies in neither direction (patches/llama.cpp/README.md).
shopt -s nullglob
LLAMA_PATCHES=("$REPO_ROOT"/patches/llama.cpp/*.patch)
shopt -u nullglob

# A failing `git apply --check` is an ANSWER here, not an error.
can_apply() { git_l apply --check "$@" 2>/dev/null; }

# The highest vX.Y.Z among the tags reachable from origin/master: --merged for
# reachability, -v:refname so v0.10.0 outranks v0.2.0, the strict regex so a
# -rc pre-release is not mistaken for a release.
LLAMA_TAG="$(git_l tag --list 'v[0-9]*' --merged origin/master --sort=-v:refname |
             grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n1 || true)"
[[ -n "$LLAMA_TAG" ]] || die "could not resolve a llama.cpp release tag (vX.Y.Z) reachable from origin/master in $CFG_LLAMA_CPP_DIR"

# Revert the patches only when the checkout actually moves HEAD: rewriting a
# patched file on a re-run would bump its mtime and cost a needless recompile.
if [[ "$(git_l rev-parse HEAD)" != "$(git_l rev-parse "$LLAMA_TAG^{commit}")" ]]; then
    for patch in "${LLAMA_PATCHES[@]}"; do
        can_apply --reverse -- "$patch" || continue
        git_l apply --reverse -- "$patch"
        say_dim "Reverted patch for a clean checkout: $(basename "$patch")"
    done
fi

say_head "Checking out llama.cpp release tag: $LLAMA_TAG"
git_l checkout --quiet --detach "$LLAMA_TAG"

for patch in "${LLAMA_PATCHES[@]}"; do
    name="$(basename "$patch")"
    if can_apply -- "$patch"; then
        git_l apply -- "$patch"
        say_head "Applied patch: patches/llama.cpp/$name"
    elif can_apply --reverse -- "$patch"; then
        say_dim "Patch already applied: patches/llama.cpp/$name"
    else
        die "patches/llama.cpp/$name applies in neither direction to $LLAMA_TAG. Upstream moved the code it touches: refresh the patch, or delete it if upstream now ships the same support. See patches/llama.cpp/README.md."
    fi
done

BUILD_DIR="$BUILD_ROOT/llama.cpp-cmake"
CACHE_FILE="$BUILD_DIR/CMakeCache.txt"
mkdir -p "$BUILD_DIR"

cache_get() {   # value of a CMakeCache entry, empty when absent
    [[ -f "$CACHE_FILE" ]] || return 0
    sed -nE "s|^$1:[^=]+=(.*)\$|\1|p" "$CACHE_FILE" | head -n1
}

# A cache written by a previous toolchain pins the old compiler path and the
# HIP paths derived from it: when the ROCm dist moves, start clean.
cached_cc="$(cache_get CMAKE_C_COMPILER)"
if [[ -n "$cached_cc" && "$cached_cc" != "$CFG_C_COMPILER" ]]; then
    say_warn 'Cached compiler changed:'
    say_warn "  $cached_cc -> $CFG_C_COMPILER"
    say_warn "  wiping $BUILD_DIR for a clean configure"
    rm -rf "$BUILD_DIR"; mkdir -p "$BUILD_DIR"
fi

# ── The configure line: checked-in options + this machine's paths ──
# cmake-options.psd1 is a PowerShell data file, read here with sed: every
# Defines entry sits alone on its line as 'NAME[:TYPE]=VALUE'. A quoted line
# of that block that does NOT have that shape fails the build before cmake
# sees it, as in 02-build.ps1: a typo must fail as a typo.
OPTIONS_FILE="$REPO_ROOT/cmake-options.psd1"
GENERATOR="$(tr -d '\r' < "$OPTIONS_FILE" | sed -nE "s/^[[:space:]]*Generator[[:space:]]*=[[:space:]]*'([^']+)'.*/\1/p" | head -n1)"
[[ -n "$GENERATOR" ]] || die "$OPTIONS_FILE: Generator is missing"
CMAKE_DEFINES=()
while IFS= read -r line; do
    if [[ "$line" =~ ^\'([A-Za-z_][A-Za-z0-9_]*(:[A-Z]+)?=[^\']*)\'$ ]]; then
        CMAKE_DEFINES+=("-D${BASH_REMATCH[1]}")
    else
        die "$OPTIONS_FILE: $line is not 'NAME[:TYPE]=VALUE'"
    fi
done < <(tr -d '\r' < "$OPTIONS_FILE" |
         sed -n '/^[[:space:]]*Defines[[:space:]]*=[[:space:]]*@(/,/^[[:space:]]*)/p' |
         sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' | grep "^'" || true)
(( ${#CMAKE_DEFINES[@]} )) || die "$OPTIONS_FILE: no Defines found"
say_dim "CMake options (cmake-options.psd1): ${CMAKE_DEFINES[*]#-D}"

CMAKE_ARGS=(
    -S "$CFG_LLAMA_CPP_DIR" -B "$BUILD_DIR" -G "$GENERATOR"
    "${CMAKE_DEFINES[@]}"
    "-DGPU_TARGETS=$CFG_GPU_TARGETS"
    # hip-config-amd.cmake seeds GPU_BUILD_TARGETS from GPU_TARGETS with a
    # set(... CACHE ...), a no-op once cached: cleared so an edited target
    # list is re-derived on every configure.
    -UGPU_BUILD_TARGETS
    "-DCMAKE_BUILD_TYPE=$CFG_BUILD_TYPE"
    "-DCMAKE_C_COMPILER=$CFG_C_COMPILER"
    "-DCMAKE_CXX_COMPILER=$CFG_CXX_COMPILER"
    # No global -march (see cmake-options.psd1). -Wno-error=incompatible-
    # pointer-types: the SSSE3 branch of ggml-cpu/arch/x86/quants.c, an error
    # by default under recent clang.
    '-DCMAKE_C_FLAGS=-w -Wno-error=incompatible-pointer-types'
    '-DCMAKE_CXX_FLAGS=-w'
    '-DCMAKE_CUDA_FLAGS=-w'
)

# lld by FLAG (-fuse-ld=lld in the three linker-flag variables), not
# CMAKE_LINKER as on Windows: the GNU-style drivers pick their linker by flag.
# Not CMAKE_LINKER_TYPE=LLD either, although that is the modern spelling:
# CMake 4.2 defines no linker types for the HIP language, and the configure
# dies in the HIP compiler check with "LINKER_TYPE 'LLD' is unknown or not
# supported by this toolchain". The flag reaches all four link drivers (clang
# for C/C++/HIP, g++ for CUDA, which finds ld.lld through the PATH common.sh
# sets). These are the only linker flags the build passes, and they are always
# passed, blank included, so none survives in the cache from an experiment or
# from a config that has since lost its lld; -U retires the cached linker type
# of a build dir configured before this was changed.
LD_FLAGS=''
[[ "${CFG_LINKER_TYPE:-}" == 'LLD' ]] && LD_FLAGS='-fuse-ld=lld'
CMAKE_ARGS+=(
    "-DCMAKE_EXE_LINKER_FLAGS=$LD_FLAGS"
    "-DCMAKE_SHARED_LINKER_FLAGS=$LD_FLAGS"
    "-DCMAKE_MODULE_LINKER_FLAGS=$LD_FLAGS"
    -UCMAKE_LINKER_TYPE
)

# ── HIP ────────────────────────────────────────────────────────────
# On Linux llama.cpp uses CMake's own HIP language (on Windows the HIP sources
# are compiled as CXX), so the device compiler is named separately: the dist's
# clang++, with the dist as its ROCm root (CMake would otherwise look for one
# relative to the compiler and in /opt/rocm). ROCM_PATH, set by common.sh, is
# what the ggml-hip CMakeLists itself reads. None of the MSVC <cmath> header
# workaround of 02-build.ps1 applies here.
if [[ -n "$CFG_HIP_PATH" ]]; then
    CMAKE_ARGS+=(
        "-DCMAKE_HIP_COMPILER=$CFG_HIP_PATH/lib/llvm/bin/clang++"
        "-DCMAKE_HIP_COMPILER_ROCM_ROOT=$CFG_HIP_PATH"
        '-DCMAKE_HIP_FLAGS=-w'
    )
else
    die "no ROCm dist configured (CFG_HIP_PATH is empty) but cmake-options.psd1 builds GGML_HIP. Run 00-install-prerequisites.sh, then 01-configure.sh."
fi

# ── Vulkan SDK: the newest installed version, pinned explicitly ────
# FindVulkan caches these four paths and reuses them without checking the
# files still exist, so they are passed on every configure and follow the
# newest SDK. VULKAN_SDK is also what ggml-vulkan adds to CMAKE_PREFIX_PATH.
VULKAN_DIR="$(find_vulkan)"
if [[ -n "$VULKAN_DIR" ]]; then
    say_head "Vulkan SDK: $VULKAN_DIR"
    export VULKAN_SDK="$VULKAN_DIR"
    export PATH="$VULKAN_DIR/bin:$PATH"
    CMAKE_ARGS+=(
        "-DVulkan_INCLUDE_DIR:PATH=$VULKAN_DIR/include"
        "-DVulkan_LIBRARY:FILEPATH=$(vulkan_loader "$VULKAN_DIR")"
        "-DVulkan_GLSLC_EXECUTABLE:FILEPATH=$VULKAN_DIR/bin/glslc"
    )
    [[ -x "$VULKAN_DIR/bin/glslangValidator" ]] &&
        CMAKE_ARGS+=("-DVulkan_GLSLANG_VALIDATOR_EXECUTABLE:FILEPATH=$VULKAN_DIR/bin/glslangValidator")
else
    say_warn "Vulkan SDK: none under $VULKAN_ROOT, leaving detection to CMake"
fi

# ── CUDA toolkit: the newest installed version ─────────────────────
# CMake picks nvcc once and caches ~40 toolkit paths next to it, so the newest
# toolkit is pinned on every configure. A changed CMAKE_CUDA_COMPILER alone
# makes CMake delete the WHOLE cache, so on a change only the CUDA half is
# reset: that language's identification files and the toolkit's cache entries
# (the GGML_CUDA_* knobs survive). Only the CUDA objects recompile.
CUDA_DIR="$(find_cuda)"
if [[ -n "$CUDA_DIR" ]]; then
    say_head "CUDA toolkit: $CUDA_DIR"
    export CUDA_PATH="$CUDA_DIR"
    nvcc="$CUDA_DIR/bin/nvcc"
    cached_nvcc="$(cache_get CMAKE_CUDA_COMPILER)"
    if [[ -n "$cached_nvcc" && "$cached_nvcc" != "$nvcc" ]]; then
        say_warn 'Cached CUDA toolkit changed:'
        say_warn "  $cached_nvcc -> $nvcc"
        say_warn '  resetting the CUDA detection only (the CUDA objects recompile, nothing else)'
        for d in "$BUILD_DIR"/CMakeFiles/[0-9]*/; do
            rm -rf "$d/CMakeCUDACompiler.cmake" "$d/CMakeDetermineCompilerABI_CUDA.bin" "$d/CompilerIdCUDA"
        done
        CMAKE_ARGS+=('-UCUDA_*' '-UCUDAToolkit_*' '-U_cmake_CUDAToolkit_*' -UFIND_PACKAGE_MESSAGE_DETAILS_CUDAToolkit)
    fi
    CMAKE_ARGS+=("-DCMAKE_CUDA_COMPILER:FILEPATH=$nvcc" "-DCUDAToolkit_ROOT:PATH=$CUDA_DIR")
    # The toolkit's library dir must be named as a build RPATH by hand: it is
    # one of nvcc's IMPLICIT link directories, and CMake leaves those out of
    # the RPATH it computes (a system toolkit is found through ldconfig, a
    # dist under /mnt/data is not), so libggml-cuda.so would fail to load
    # libcudart/libcublas and the CUDA backend would silently be absent.
    CMAKE_ARGS+=("-DCMAKE_BUILD_RPATH=$CUDA_DIR/lib")
    # nvcc's host compiler: the distro g++ (see 01-configure.sh), not the ROCm
    # clang that compiles everything else.
    [[ -n "${CFG_CUDA_HOST_COMPILER:-}" ]] &&
        CMAKE_ARGS+=("-DCMAKE_CUDA_HOST_COMPILER:FILEPATH=$CFG_CUDA_HOST_COMPILER")
else
    say_warn "CUDA toolkit: none under $CUDA_ROOT, leaving detection to CMake"
    CMAKE_ARGS+=(-UCMAKE_BUILD_RPATH)
fi

# ── sccache: use local cache if available ──────────────────────────
# The fixed path 00-install-prerequisites.sh installs to comes first (it is
# part of every compile command, so it must not move), PATH second. nvcc is
# intentionally NOT wrapped, for the reason given in 02-build.ps1 (the
# PTX-only compute_90 step, pending sccache PR #2809); -U clears a stale
# launcher left in the cache.
SCCACHE=''
if [[ -x "$SCCACHE_BIN" ]]; then SCCACHE="$SCCACHE_BIN"
elif command -v sccache >/dev/null; then SCCACHE="$(command -v sccache)"; fi
if [[ -n "$SCCACHE" ]]; then
    export SCCACHE_DIR="$BUILD_ROOT/.sccache"
    export SCCACHE_CACHE_SIZE='10G' SCCACHE_IDLE_TIMEOUT='0'
    export SCCACHE_MAX_FRAME_LENGTH='104857600'   # 100MB: GPU multi-arch objects are large
    export SCCACHE_CLIENT_SIDE='1'
    mkdir -p "$SCCACHE_DIR"
    say_head "sccache: $SCCACHE (cache: $SCCACHE_DIR)"
    CMAKE_ARGS+=(
        "-DCMAKE_C_COMPILER_LAUNCHER=$SCCACHE"
        "-DCMAKE_CXX_COMPILER_LAUNCHER=$SCCACHE"
        -UCMAKE_CUDA_COMPILER_LAUNCHER
    )
else
    say_dim 'sccache not found, building without compiler cache'
    CMAKE_ARGS+=(-UCMAKE_C_COMPILER_LAUNCHER -UCMAKE_CXX_COMPILER_LAUNCHER -UCMAKE_CUDA_COMPILER_LAUNCHER)
fi

say_head 'Configuring...'
cmake "${CMAKE_ARGS[@]}"

say_head 'Building...'
if (( CFG_BUILD_JOBS > 0 )); then cmake --build "$BUILD_DIR" -j "$CFG_BUILD_JOBS"
else cmake --build "$BUILD_DIR" -j; fi

# ── The chat UI must be IN the binary ──────────────────────────────
# llama.cpp never fails for a missing UI (a cmake WARNING and an empty page),
# so the check is made here, as in 02-build.ps1.
UI_DIST="$BUILD_DIR/tools/ui/dist/index.html"
UI_SRC_DIST="$CFG_LLAMA_CPP_DIR/tools/ui/dist/index.html"
UI_STAMP="$BUILD_DIR/tools/ui/.ui-stamp"
if [[ ! -f "$UI_DIST" && ! -f "$UI_SRC_DIST" ]]; then
    die "the web UI was not embedded ($UI_DIST is missing). LLAMA_BUILD_UI=ON needs node + npm on PATH (the npm failure is in the configure output above), the prebuilt fallback needs huggingface.co to be reachable."
fi
if [[ -f "$UI_STAMP" ]]; then
    say_warn "Web UI: PREBUILT assets ($(tr -d '[:space:]' < "$UI_STAMP")) downloaded from HuggingFace; the npm build did not produce any"
elif [[ -f "$UI_DIST" ]]; then
    say_dim 'Web UI: built from the checked-out sources (npm)'
else
    say_dim "Web UI: pre-built assets found in the source tree ($UI_SRC_DIST)"
fi

if [[ -n "$SCCACHE" ]]; then
    say ''
    say_head 'sccache stats:'
    "$SCCACHE" --show-stats || true
    "$SCCACHE" --stop-server >/dev/null 2>&1 || true
fi

say ''
say_ok "llama.cpp build complete: $BUILD_DIR/bin/"
# The binaries carry a build-tree RPATH to the ROCm, CUDA and Vulkan dists
# they were linked against, so they run from here with no LD_LIBRARY_PATH.
"$BUILD_DIR/bin/llama-server" --version 2>&1 | sed 's/^/  /' || die 'the freshly built llama-server does not start'
say ''

# ── llama-cpp-config (Rust) ────────────────────────────────────────
say ''
say_warn '  === llama-cpp-config (Rust) ==='

# rustup installs into ~/.cargo/bin and only a NEW login shell has it on PATH.
CARGO="$(command -v cargo || true)"
[[ -z "$CARGO" && -x "$HOME/.cargo/bin/cargo" ]] && CARGO="$HOME/.cargo/bin/cargo"
[[ -n "$CARGO" ]] || die 'cargo not found. Run 00-install-prerequisites.sh (it installs Rust through rustup).'
export PATH="$(dirname "$CARGO"):$PATH"
say_dim "cargo: $CARGO"

# The icon is generated, not checked in (build.rs bootstraps it the same way,
# but from the Windows clone location, build/llama.cpp): done here first, from
# the Linux clone. sharp ships one native package per platform, so the npm
# install also completes a node_modules that the Windows side created.
ICO="$REPO_ROOT/resources/llama.ico"
if [[ ! -f "$ICO" ]]; then
    say_head 'Generating resources/llama.ico from the llama.cpp webui logo...'
    ( cd "$REPO_ROOT/resources" &&
      npm install --no-save --silent sharp sharp-ico &&
      node generate-llama-ico.mjs "$CFG_LLAMA_CPP_DIR" "$ICO" )
fi

# Its own target dir under build/linux, not llama-cpp-config/target: that one
# belongs to the Windows build when both systems share the checkout.
export CARGO_TARGET_DIR="$BUILD_ROOT/cargo-target"
CARGO_ARGS=(build --release)
(( CFG_BUILD_JOBS > 0 )) && CARGO_ARGS+=(--jobs "$CFG_BUILD_JOBS")
( cd "$REPO_ROOT/llama-cpp-config" && "$CARGO" "${CARGO_ARGS[@]}" )

# Copied next to llama-server: the configurator looks for llama.cpp's binaries
# in its OWN directory first (the installer layout on Windows), and its
# dev-layout fallbacks name the Windows build tree, not build/linux.
install -m 755 "$CARGO_TARGET_DIR/release/llama-cpp-config" "$BUILD_DIR/bin/llama-cpp-config"
say_ok "llama-cpp-config build complete: $BUILD_DIR/bin/llama-cpp-config"
"$BUILD_DIR/bin/llama-cpp-config" --version | sed 's/^/  /'
say ''
