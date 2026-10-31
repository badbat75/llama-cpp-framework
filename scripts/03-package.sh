#!/usr/bin/env bash
# Package the build (llama.cpp + llama-cpp-config) in ./dist/: a .deb and an
# .rpm on Linux, a .pkg on macOS; the counterpart of 03-package.ps1. Requires
# a successful build. The logic, and what the packages install, is in
# xtask/src/<os>/package.rs; this script only runs it.
#
# Usage: scripts/03-package.sh [--deb] [--rpm] [--keep-staging]   (--deb/--rpm: Linux)
# The same as: cargo run -p xtask -- package [options]
set -euo pipefail
repo="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"
# build/linux or build/macos: everything generated stays per OS.
case "$(uname -s)" in Darwin) os=macos ;; *) os=linux ;; esac
export CARGO_TARGET_DIR="$repo/build/$os/cargo-target"
PATH="$PATH:$HOME/.cargo/bin"
exec cargo run --quiet --manifest-path "$repo/Cargo.toml" -p xtask -- package "$@"
