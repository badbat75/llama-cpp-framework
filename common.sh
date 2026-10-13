# Shared bootstrap for the Linux build scripts, which source it:
#   . "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/common.sh"
# The bash scripts are the Linux counterpart of the .ps1 ones and deliberately
# share nothing generated with them: the repo may sit on a drive both systems
# mount, so everything Linux writes lives under build/linux/ (source clone,
# cmake binary dir, sccache cache, config-build.sh), next to the Windows
# artifacts under build/windows/ and never inside them.
#
# - Sets strict mode, REPO_ROOT, BUILD_ROOT and the say_* helpers.
# - Sources installer/dist-sources.sh (roots and sources of the dists) and
#   provides use_dist_root, which moves them to another base.
# - Finders for the newest complete ROCm / CUDA / Vulkan dist.
# - load_build_config: reads build/linux/config-build.sh and sets the ROCm
#   build environment. Only 02-build.sh calls it; 00 and 01 run before the
#   file exists.

set -euo pipefail

REPO_ROOT="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"
BUILD_ROOT="$REPO_ROOT/build/linux"
CONFIG_FILE="$BUILD_ROOT/config-build.sh"

# The dist root a previous 01-configure.sh recorded, read in a subshell so no
# other CFG_* leaks into 00 and 01: dist-sources.sh falls back to it when
# neither --dist-root nor LLAMA_FW_DIST_ROOT says otherwise.
CFG_DIST_ROOT=''
if [[ -f "$CONFIG_FILE" ]]; then
    # shellcheck disable=SC1090
    CFG_DIST_ROOT="$( . "$CONFIG_FILE"; printf '%s' "${CFG_DIST_ROOT:-}" )"
fi

# shellcheck source=installer/dist-sources.sh
. "$REPO_ROOT/installer/dist-sources.sh"

# Switches every dist root (ROCM_ROOT, CUDA_ROOT, VULKAN_ROOT) to another base:
# the --dist-root of 00 and 01, and the recorded root in load_build_config.
use_dist_root() {
    [[ -n "$1" ]] || die '--dist-root needs a path'
    DIST_ROOT_OVERRIDE="$(readlink -m "$1")"
    # shellcheck source=installer/dist-sources.sh
    . "$REPO_ROOT/installer/dist-sources.sh"
}

if [[ -t 1 ]]; then
    _c_red=$'\e[31m'; _c_grn=$'\e[32m'; _c_yel=$'\e[33m'; _c_cyn=$'\e[36m'; _c_dim=$'\e[90m'; _c_off=$'\e[0m'
else
    _c_red=''; _c_grn=''; _c_yel=''; _c_cyn=''; _c_dim=''; _c_off=''
fi
say()      { printf '%s\n' "$*"; }
say_head() { printf '%s%s%s\n' "$_c_cyn" "$*" "$_c_off"; }
say_ok()   { printf '%s%s%s\n' "$_c_grn" "$*" "$_c_off"; }
say_warn() { printf '%s%s%s\n' "$_c_yel" "$*" "$_c_off"; }
say_err()  { printf '%s%s%s\n' "$_c_red" "$*" "$_c_off" >&2; }
say_dim()  { printf '%s%s%s\n' "$_c_dim" "$*" "$_c_off"; }
die()      { say_err "ERROR: $*"; exit 1; }

# Is version $1 strictly newer than $2? Compared as versions (sort -V), so
# 1.4.10 outranks 1.4.9, which a string comparison gets wrong.
version_gt() {
    [[ "$1" != "$2" && "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$1" ]]
}

# The newest COMPLETE dist under a root: the highest <Root>/<X.Y...>/<DIST_OS>
# that holds every file named after the root (paths relative to the dist dir).
# Prints the dist dir, or nothing. A tree that is missing one of the files is
# a failed or partial install and is skipped, not reported as present.
newest_dist() {
    local root="$1"; shift
    local d ver rel best='' best_ver=''
    [[ -d "$root" ]] || return 0
    for d in "$root"/*/"$DIST_OS"; do
        [[ -d "$d" ]] || continue
        ver="$(basename "$(dirname "$d")")"
        [[ "$ver" =~ ^[0-9]+(\.[0-9]+)+$ ]] || continue
        for rel in "$@"; do
            [[ -e "$d/$rel" ]] || continue 2
        done
        if [[ -z "$best_ver" ]] || version_gt "$ver" "$best_ver"; then
            best_ver="$ver"; best="$d"
        fi
    done
    [[ -n "$best" ]] && printf '%s\n' "$best"
    return 0
}

# What each build leg needs from its dist; also the completeness test
# 00-install-prerequisites.sh applies before calling a version installed.
find_rocm()   { newest_dist "$ROCM_ROOT" "$ROCM_MARKER" lib/llvm/bin/clang++ lib/cmake/hip/hip-config.cmake; }
find_cuda()   { newest_dist "$CUDA_ROOT" "$CUDA_MARKER" bin/nvcc lib64/libcudart.so lib64/libcublas.so; }
# Prints the SDK proper (the x86_64 dir), which is what VULKAN_SDK names.
find_vulkan() {
    local d
    d="$(newest_dist "$VULKAN_ROOT" "$VULKAN_MARKER" x86_64/include/vulkan/vulkan.h x86_64/bin/glslc)"
    [[ -n "$d" && -n "$(vulkan_loader "$d/x86_64")" ]] && printf '%s\n' "$d/x86_64"
    return 0
}
# The loader library of an SDK. Since 1.4.3xx LunarG keeps it out of lib/, in
# lib/VulkanLoader/lib, so that putting the SDK on a library path does not
# replace the system loader by accident; older SDKs have it in lib/.
vulkan_loader() {
    local f
    for f in "$1/lib/libvulkan.so" "$1/lib/VulkanLoader/lib/libvulkan.so"; do
        if [[ -e "$f" ]]; then printf '%s\n' "$f"; return 0; fi
    done
    return 0
}
# <Root>/<Version>/linux[/...] -> <Version>
dist_version() {
    local p="${1%/x86_64}"
    basename "$(dirname "$p")"
}

# 3/4 of the logical cores, and never all of them (same policy as
# 01-configure.ps1): a core stays free during the long CUDA/HIP compiles.
default_build_jobs() {
    local n j
    n="$(nproc)"
    j=$(( n * 3 / 4 ))
    (( j > n - 1 )) && j=$(( n - 1 ))
    (( j < 1 )) && j=1
    printf '%s\n' "$j"
}

# Loads CFG_* from build/linux/config-build.sh and sets the per-process ROCm
# environment. ROCM_PATH is what llama.cpp's ggml-hip CMakeLists reads to find
# the hip / hipblas / rocblas packages (it falls back to /opt/rocm, then /usr).
# HIP_DEVICE_LIB_PATH points clang at the device bitcode, which TheRock keeps
# under lib/llvm/amdgcn/bitcode rather than the <rocm>/amdgcn/bitcode layout
# clang derives from --rocm-path. Process scope only, as on Windows.
load_build_config() {
    [[ -f "$CONFIG_FILE" ]] || die "build/linux/config-build.sh not found. Run 01-configure.sh first."
    # shellcheck disable=SC1090
    . "$CONFIG_FILE"
    # The root 01-configure.sh recorded, so the CUDA Toolkit and the Vulkan
    # SDK resolved at build time come from the same root as CFG_HIP_PATH.
    if [[ -n "${CFG_DIST_ROOT:-}" ]]; then use_dist_root "$CFG_DIST_ROOT"; fi
    if [[ -n "${CFG_HIP_PATH:-}" && -d "$CFG_HIP_PATH" ]]; then
        export HIP_PATH="$CFG_HIP_PATH" ROCM_PATH="$CFG_HIP_PATH"
        local llvm_bin="$CFG_HIP_PATH/lib/llvm/bin"
        [[ -d "$llvm_bin" && ":$PATH:" != *":$llvm_bin:"* ]] && PATH="$llvm_bin:$PATH"
        [[ ":$PATH:" != *":$CFG_HIP_PATH/bin:"* ]] && PATH="$CFG_HIP_PATH/bin:$PATH"
        export PATH
        if [[ -d "$CFG_HIP_PATH/lib/llvm/amdgcn/bitcode" ]]; then
            export HIP_DEVICE_LIB_PATH="$CFG_HIP_PATH/lib/llvm/amdgcn/bitcode"
            export HIP_PLATFORM='amd'
        fi
    fi
    return 0
}
