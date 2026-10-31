#!/usr/bin/env bash
# Detect paths and tools and write build/<os>/config-build.toml (Linux and
# macOS), the counterpart of 01-configure.ps1. Run it after
# 00-install-prerequisites.sh and again whenever a dist changes. The logic is
# xtask's configure phase (xtask/src/{linux,macos}/configure.rs); this script
# only runs it.
#
# Usage: scripts/01-configure.sh [--llama-cpp-dir <path>] [--dist-root <path>]
# The same as: cargo run -p xtask -- configure [options]
set -euo pipefail
repo="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"
# build/linux or build/macos: everything generated stays per OS.
case "$(uname -s)" in Darwin) os=macos ;; *) os=linux ;; esac
export CARGO_TARGET_DIR="$repo/build/$os/cargo-target"
PATH="$PATH:$HOME/.cargo/bin"
exec cargo run --quiet --manifest-path "$repo/Cargo.toml" -p xtask -- configure "$@"
