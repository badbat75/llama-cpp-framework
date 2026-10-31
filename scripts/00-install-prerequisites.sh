#!/usr/bin/env bash
# Idempotent toolchain bootstrapper for Linux (Debian/Ubuntu) and macOS
# (Apple Silicon), the counterpart of scripts\00-install-prerequisites.ps1. Safe to
# re-run any time.
#
# This script does the legs xtask cannot, since it needs them to build:
#   - Linux: apt packages (git, g++, cmake, ninja, OpenSSL headers, node +
#     npm, ...): the one leg that needs root, through sudo, and only when one
#     is missing.
#   - macOS: the Xcode Command Line Tools (checked: their installer is a GUI
#     dialog) and Homebrew packages (cmake, ninja, node; Homebrew itself is
#     checked, not installed).
#   - Rust through rustup, when missing (for xtask and llama-cpp-config).
# Then it runs xtask's prerequisites phase (xtask/src/<os>/prereq.rs):
# sccache and the newest stable dists (Linux: ROCm, CUDA Toolkit, NCCL and
# Vulkan SDK; macOS: the Vulkan SDK), the llama.cpp release check and the
# report.
#
# Usage: scripts/00-install-prerequisites.sh [--dist-root <path>]
#          [--skip apt,brew,sccache,rust,rocm,cuda,nccl,vulkan]
set -euo pipefail
repo="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"

if [[ -t 1 ]]; then c_red=$'\e[31m'; c_grn=$'\e[32m'; c_cyn=$'\e[36m'; c_off=$'\e[0m'
else c_red=''; c_grn=''; c_cyn=''; c_off=''; fi

# Only --skip matters here; every argument goes on to xtask unchanged.
skip=','; help=0; args=("$@")
while (( $# )); do
    case "$1" in
        --skip)   skip=",${2:-},"; shift $(( $# > 1 ? 2 : 1 )) ;;
        --skip=*) skip=",${1#--skip=},"; shift ;;
        -h|--help) help=1; shift ;;
        *) shift ;;
    esac
done
skipped() { [[ "$skip" == *",$1,"* ]]; }
gaps=()

# build/linux or build/macos: everything generated stays per OS.
case "$(uname -s)" in Darwin) os=macos ;; *) os=linux ;; esac

if (( ! help )) && [[ $os == macos ]]; then
    # ── Xcode Command Line Tools ────────────────────────────────────────
    # Apple's clang, git, otool, install_name_tool, codesign and pkgbuild.
    printf '%s\n' "${c_cyn}  Xcode Command Line Tools${c_off}"
    if xcode-select -p >/dev/null 2>&1; then
        printf '%s\n' "${c_grn}    $(xcode-select -p)${c_off}"
    else
        gaps+=('Xcode Command Line Tools: run xcode-select --install, then this script again')
    fi
    echo

    # ── Homebrew packages ───────────────────────────────────────────────
    # node brings npm (the chat UI llama-server embeds, LLAMA_BUILD_UI=ON).
    brew_packages=(cmake ninja node)
    if skipped brew; then
        :
    elif ! command -v brew >/dev/null; then
        gaps+=("install Homebrew (https://brew.sh), then: brew install ${brew_packages[*]}")
    else
        printf '%s\n' "${c_cyn}  Homebrew packages${c_off}"
        missing=()
        for p in "${brew_packages[@]}"; do
            brew list --formula "$p" >/dev/null 2>&1 || missing+=("$p")
        done
        if (( ${#missing[@]} == 0 )); then
            printf '%s\n' "${c_grn}    all ${#brew_packages[@]} present${c_off}"
        else
            echo "    missing: ${missing[*]}"
            brew install "${missing[@]}" || gaps+=("brew install ${missing[*]}")
        fi
        echo
    fi
fi

if (( ! help )) && [[ $os == linux ]]; then
    # ── apt packages ────────────────────────────────────────────────────
    # g++ is needed even though the host compiler is the ROCm dist's clang:
    # clang takes libstdc++, the C runtime start files and libgcc from the
    # distro's GCC install, and nvcc drives g++ as its host compiler. nodejs +
    # npm build the chat UI that llama-server embeds (LLAMA_BUILD_UI=ON).
    # unzip unpacks the NCCL wheel.
    # libfontconfig-dev is for llama-cpp-config: Slint's Skia renderer links
    # the system fontconfig (and freetype, which it pulls in).
    # patchelf, dpkg-dev and rpm are the package phase's: the RUNPATH rewrite,
    # the Depends computation (dpkg-shlibdeps) and rpmbuild.
    apt_packages=(git g++ make cmake ninja-build pkg-config libssl-dev nodejs npm
                  curl jq xz-utils unzip ca-certificates libfontconfig-dev
                  patchelf dpkg-dev rpm)
    if skipped apt; then
        :
    elif ! command -v apt-get >/dev/null; then
        gaps+=("install by hand the equivalents of: ${apt_packages[*]}")
    else
        printf '%s\n' "${c_cyn}  System packages (apt)${c_off}"
        missing=()
        for p in "${apt_packages[@]}"; do
            dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q 'install ok installed' || missing+=("$p")
        done
        if (( ${#missing[@]} == 0 )); then
            printf '%s\n' "${c_grn}    all ${#apt_packages[@]} present${c_off}"
        else
            echo "    missing: ${missing[*]}"
            # Non-interactive when there is no terminal to type a password
            # into, so a scripted run fails here in one line rather than
            # hanging on a prompt.
            sudo_flags=(); [[ -t 0 ]] || sudo_flags=(-n)
            sudo ${sudo_flags[@]+"${sudo_flags[@]}"} apt-get update -qq &&
            sudo ${sudo_flags[@]+"${sudo_flags[@]}"} env DEBIAN_FRONTEND=noninteractive apt-get install -y "${missing[@]}" ||
                gaps+=("sudo apt-get install -y ${missing[*]}")
        fi
        echo
    fi

fi

if (( ! help )); then
    # ── Rust (rustup) ───────────────────────────────────────────────────
    # Installed when missing, user scope, without touching the shell profile
    # (the scripts find ~/.cargo/bin themselves). An existing toolchain is
    # left alone: a rustc upgrade recompiles the whole dependency tree, so
    # `rustup update` stays the user's call.
    if ! skipped rust; then
        printf '%s\n' "${c_cyn}  Rust${c_off}"
        cargo_bin="$(command -v cargo || true)"
        [[ -z "$cargo_bin" && -x "$HOME/.cargo/bin/cargo" ]] && cargo_bin="$HOME/.cargo/bin/cargo"
        if [[ -n "$cargo_bin" ]]; then
            printf '%s\n' "${c_grn}    $("$cargo_bin" --version) ($cargo_bin)${c_off}"
        else
            tmp="$(mktemp -d)"
            if curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$tmp/rustup-init.sh" &&
               sh "$tmp/rustup-init.sh" -y --profile minimal --default-toolchain stable --no-modify-path >/dev/null 2>&1 &&
               [[ -x "$HOME/.cargo/bin/cargo" ]]; then
                printf '%s\n' "${c_grn}    installed $("$HOME/.cargo/bin/cargo" --version) ($HOME/.cargo/bin)${c_off}"
            else
                gaps+=('Rust: install it from https://rustup.rs')
            fi
            rm -rf "$tmp"
        fi
        echo
    fi
fi

# ── Everything else: xtask ──────────────────────────────────────────────
rc=0
if command -v cargo >/dev/null || [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    export CARGO_TARGET_DIR="$repo/build/$os/cargo-target"
    PATH="$PATH:$HOME/.cargo/bin"
    # The ${a[@]+...} form: macOS's bash 3.2 calls an empty array unbound
    # under set -u.
    cargo run --quiet --manifest-path "$repo/Cargo.toml" -p xtask -- prereq ${args[@]+"${args[@]}"} || rc=$?
else
    gaps+=('xtask: no cargo, so the dists, sccache and the report were skipped')
fi

if (( ${#gaps[@]} )); then
    printf '%s\n' "${c_red}  To fix by hand (${#gaps[@]}):${c_off}" >&2
    for g in "${gaps[@]}"; do printf '%s\n' "${c_red}    - $g${c_off}" >&2; done
    exit 1
fi
exit "$rc"
