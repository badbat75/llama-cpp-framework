#!/usr/bin/env bash
# Auto-detect paths and tools and generate build/linux/config-build.sh, the
# Linux counterpart of 01-configure.ps1. Run it after
# 00-install-prerequisites.sh and again whenever the ROCm dist changes (the
# compiler path and the GPU targets are derived from it). CUDA and Vulkan are
# not recorded here: 02-build.sh finds the newest of each on every run.
#
# Usage: ./01-configure.sh [--llama-cpp-dir <path>]   (default build/linux/llama.cpp)

. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/common.sh"

LLAMA_CPP_DIR="$BUILD_ROOT/llama.cpp"
while (( $# )); do
    case "$1" in
        --llama-cpp-dir)   LLAMA_CPP_DIR="${2:?--llama-cpp-dir needs a path}"; shift 2 ;;
        --llama-cpp-dir=*) LLAMA_CPP_DIR="${1#--llama-cpp-dir=}"; shift ;;
        -h|--help) sed -n '2,/^$/s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
LLAMA_CPP_DIR="$(readlink -m "$LLAMA_CPP_DIR")"

GAPS=()
row_ok()   { say_ok   "$(printf '  [OK] %-15s: %s' "$1" "$2")"; }
row_opt()  { say_warn "$(printf '  [--] %-15s: %s' "$1" "$2")"; }
row_miss() { say_err  "$(printf '  [!!] %-15s: %s' "$1" 'NOT FOUND')"; GAPS+=("$1: $2"); }

# GPU targets are DERIVED from the installed ROCm dist, not hand-maintained,
# by the same rule as 01-configure.ps1 (see Get-GpuTargets there for the full
# reasoning): the archs the dist ships BLAS kernels for, minus CDNA/Vega
# gfx9xx, plus the Ryzen iGPU archs. The gfx9xx drop is a parity choice here,
# not a driver limit as on Windows: Linux does drive the MI cards, but each
# arch costs ~56 MiB of libggml-hip.so and its share of a long compile, and
# the two builds are meant to cover the same hardware. Add them by hand in
# config-build.sh if this machine has one.
# gfx125x is dropped with them: the other data-center family (AMD ships it as
# the `gfx125X-dcgpu` dist, and the Windows multiarch dist has no kernels for
# it), and not a choice at all today, since llama.cpp v0.6.0 does not compile
# for it (mma.cuh uses the gfx12 WMMA builtins, which gfx1250 lacks: "needs
# target feature wmma-128b-insts"), failing the whole ggml-hip target.
GPU_TARGETS_IGPU_SAFETY=(gfx1035 gfx1036 gfx1103 gfx1152 gfx1153)
GPU_TARGETS_FALLBACK='gfx1010;gfx1011;gfx1012;gfx1030;gfx1031;gfx1032;gfx1033;gfx1034;gfx1035;gfx1036;gfx1100;gfx1101;gfx1102;gfx1103;gfx1150;gfx1151;gfx1152;gfx1153;gfx1200;gfx1201'

get_gpu_targets() {
    local hip="$1" archs
    [[ -n "$hip" ]] || return 0
    archs="$(find "$hip/.kpack" -maxdepth 1 -name 'blas_lib_gfx*.kpack' 2>/dev/null |
             sed -E 's|.*/blas_lib_(gfx[0-9a-zA-Z]+)\.kpack$|\1|')"
    if [[ -z "$archs" ]]; then
        archs="$(find "$hip/lib/rocblas/library" -maxdepth 1 -name 'TensileLibrary_lazy_gfx*.dat' 2>/dev/null |
                 sed -E 's|.*/TensileLibrary_lazy_(gfx[0-9a-zA-Z]+)\.dat$|\1|')"
    fi
    [[ -n "$archs" ]] || return 0
    { grep -vE '^gfx(9|125)' <<<"$archs" || true; printf '%s\n' "${GPU_TARGETS_IGPU_SAFETY[@]}"; } |
        sort -u | paste -sd';'
}

say ''
say_head '  llama.cpp-framework: Environment Check (Linux)'
say_head '  =============================================='
say ''

# --- Paths ---

# An explicit HIP_PATH / ROCM_PATH naming a usable dist wins; otherwise the
# newest complete one under the ROCm root.
HIP_DIR=''
for cand in "${HIP_PATH:-}" "${ROCM_PATH:-}"; do
    if [[ -n "$cand" && -x "$cand/lib/llvm/bin/clang++" ]]; then HIP_DIR="$(readlink -f "$cand")"; break; fi
done
[[ -n "$HIP_DIR" ]] || HIP_DIR="$(find_rocm)"
if [[ -n "$HIP_DIR" ]]; then row_ok HipPath "$HIP_DIR"
else row_opt HipPath 'not found (needed for HIP/ROCm; run 00-install-prerequisites.sh)'; fi

GPU_TARGETS="$(get_gpu_targets "$HIP_DIR")"
if [[ -n "$GPU_TARGETS" ]]; then
    row_ok GpuTargets "$(tr ';' '\n' <<<"$GPU_TARGETS" | wc -l) archs from the installed ROCm dist"
    say_dim "                         $GPU_TARGETS"
else
    GPU_TARGETS="$GPU_TARGETS_FALLBACK"
    row_opt GpuTargets 'no kernel list found in the dist, using the 7.14.0 snapshot'
fi

row_ok LlamaCppDir "$LLAMA_CPP_DIR"

cuda_dir="$(find_cuda)"
if [[ -n "$cuda_dir" ]]; then row_ok 'CUDA Toolkit' "$cuda_dir"
else row_opt 'CUDA Toolkit' "none under $CUDA_ROOT (needed for CUDA)"; fi
vk_dir="$(find_vulkan)"
if [[ -n "$vk_dir" ]]; then row_ok 'Vulkan SDK' "$vk_dir"
else row_opt 'Vulkan SDK' "none under $VULKAN_ROOT (needed for Vulkan)"; fi

say ''
say_head '  Tools'
say_head '  -----'

for tool in git cmake ninja g++; do
    if p="$(command -v "$tool")"; then row_ok "$tool" "$p"
    else row_miss "$tool" 'run 00-install-prerequisites.sh'; fi
done
for tool in node npm; do
    if p="$(command -v "$tool")"; then row_ok "$tool" "$p"
    else row_opt "$tool" 'not found (the chat UI falls back to the prebuilt download)'; fi
done

# cargo builds llama-cpp-config; rustup puts it in ~/.cargo/bin, which only a
# new login shell has on PATH.
if p="$(command -v cargo)" || { p="$HOME/.cargo/bin/cargo"; [[ -x "$p" ]]; }; then row_ok cargo "$p"
else row_miss cargo 'run 00-install-prerequisites.sh'; fi

# Compiler: the clang shipping in the ROCm dist when there is one, so the host
# half and the HIP device half of the build come from the same LLVM (as on
# Windows), then a system clang, then GCC. Full paths: the dist's clang is not
# on PATH and nothing here puts it there machine-wide.
if [[ -n "$HIP_DIR" && -x "$HIP_DIR/lib/llvm/bin/clang" ]]; then
    CC_PATH="$HIP_DIR/lib/llvm/bin/clang"; CXX_PATH="$HIP_DIR/lib/llvm/bin/clang++"
elif command -v clang >/dev/null && command -v clang++ >/dev/null; then
    CC_PATH="$(command -v clang)"; CXX_PATH="$(command -v clang++)"
else
    CC_PATH="$(command -v gcc || echo gcc)"; CXX_PATH="$(command -v g++ || echo g++)"
fi
row_ok 'C compiler' "$CC_PATH"
row_ok 'C++ compiler' "$CXX_PATH"

# Linker: lld, but only the one shipping WITH the compiler above (looked up
# next to it and nowhere else). It is what makes GGML_LTO work with clang:
# -flto=thin objects are LLVM bitcode, which GNU ld cannot link without a
# plugin, and llama.cpp then turns LTO off silently. Blank is a supported
# state (GCC, or a clang without lld): CMake keeps the default linker.
LINKER_TYPE=''
if [[ "$(basename "$CC_PATH")" == clang* && -x "$(dirname "$CC_PATH")/ld.lld" ]]; then
    LINKER_TYPE='LLD'
    row_ok Linker "lld ($(dirname "$CC_PATH")/ld.lld)"
else
    row_opt Linker 'system default (no lld next to the compiler)'
fi

# nvcc compiles the host half of every .cu with a HOST compiler, and supports
# a narrow range of them: the distro's g++, not the ROCm clang.
CUDA_HOST_CXX="$(command -v g++ || true)"

say ''
if (( ${#GAPS[@]} )); then
    say_err "  Gaps found (${#GAPS[@]}):"
    for g in "${GAPS[@]}"; do say_err "    - $g"; done
else
    say_ok '  All required dependencies found!'
fi
say ''

# ── Write build/linux/config-build.sh ──────────────────────────────
BUILD_JOBS="$(default_build_jobs)"
q() { printf "'%s'" "${1//\'/\'\\\'\'}"; }   # single-quote for sourcing

mkdir -p "$BUILD_ROOT"
cat > "$CONFIG_FILE" <<EOF
# Generated by 01-configure.sh; re-run it to refresh. Sourced by common.sh.

# Paths
CFG_LLAMA_CPP_DIR=$(q "$LLAMA_CPP_DIR")
CFG_HIP_PATH=$(q "$HIP_DIR")

# Build settings
# CFG_GPU_TARGETS: DERIVED from the installed ROCm dist by 01-configure.sh
# (the archs it ships BLAS kernels for, minus the data-center gfx9xx and
# gfx125x, plus the Ryzen
# iGPU archs). Re-run 01-configure.sh after a ROCm dist bump to refresh.
# Hand-edit to prune (~56 MiB of libggml-hip.so per arch) or to add an MI card.
CFG_GPU_TARGETS=$(q "$GPU_TARGETS")
CFG_BUILD_TYPE='Release'
CFG_C_COMPILER=$(q "$CC_PATH")
CFG_CXX_COMPILER=$(q "$CXX_PATH")
# 'LLD' = link with the lld next to the compiler (-fuse-ld=lld); blank =
# the toolchain's default linker.
CFG_LINKER_TYPE=$(q "$LINKER_TYPE")
# Host compiler nvcc drives for the CUDA backend.
CFG_CUDA_HOST_COMPILER=$(q "$CUDA_HOST_CXX")
# Parallel build jobs: 3/4 of logical cores, never all of them, so at least
# one stays free during a long CUDA/HIP compile (0 = use all cores).
CFG_BUILD_JOBS=$BUILD_JOBS
EOF

say_ok '  build/linux/config-build.sh written.'
say ''

# Non-zero exit when required tools are missing, so callers can script on it.
# The config file is still written above to ease fixing the gaps incrementally.
(( ${#GAPS[@]} == 0 ))
