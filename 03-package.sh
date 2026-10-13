#!/usr/bin/env bash
# Package the Linux build (llama.cpp + llama-cpp-config) as a .deb and an
# .rpm in ./dist/, the Linux counterpart of 03-package.ps1. Requires a
# successful 02-build.sh; patchelf, dpkg-dev and rpm come from
# 00-install-prerequisites.sh.
#
# What the packages install (both from the same staged tree):
#   /opt/llama.cpp/bin/   every llama.cpp tool, its shared libraries and the
#                         ggml backends, plus llama-cpp-config: ONE directory,
#                         because ggml looks for its backends next to the
#                         executable and llama-cpp-config looks for
#                         llama-server there too
#   /opt/llama.cpp/lib/   the GPU runtimes the backends need, taken from the
#                         dists this build linked against: the ROCm runtime
#                         with its kernel libraries, the CUDA libraries, the
#                         Vulkan loader. Self-contained: nothing else has to
#                         be installed but the GPU drivers.
#   /usr/bin/             a symlink per executable into /opt/llama.cpp/bin
#   /etc/llama.cpp/       the config files (server.ini, presets.ini,
#                         presets.ini.disabled, settings.ini, the bench
#                         prompts), written by llama-cpp-config on first use
#   /usr/share/           the desktop entry, its icon, the licence
#
# Usage: ./03-package.sh [--deb] [--rpm] [--keep-staging]
#   --deb / --rpm    build only that format (default: both)
#   --keep-staging   leave build/linux/staging/ in place for inspection

. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/common.sh"

WANT_DEB=0; WANT_RPM=0; KEEP_STAGING=0
while (( $# )); do
    case "$1" in
        --deb) WANT_DEB=1; shift ;;
        --rpm) WANT_RPM=1; shift ;;
        --keep-staging) KEEP_STAGING=1; shift ;;
        -h|--help) sed -n '2,/^$/s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
if (( ! WANT_DEB && ! WANT_RPM )); then WANT_DEB=1; WANT_RPM=1; fi

load_build_config

PKG_NAME='llama-cpp-framework'
PKG_GROUP='llama-cpp'           # owns /etc/llama.cpp, see the scriptlets below
OPT_DIR='opt/llama.cpp'         # relative to the package root
BIN_DIR="$OPT_DIR/bin"          # llama.cpp + llama-cpp-config
RT_DIR="$OPT_DIR/lib"           # the bundled GPU runtimes
ETC_DIR='etc/llama.cpp'

BUILD_DIR="$BUILD_ROOT/llama.cpp-cmake"
STAGE_DIR="$BUILD_ROOT/staging"
PKG_WORK="$BUILD_ROOT/package"  # control files, spec, rpmbuild tree: transient
OUT_DIR="$REPO_ROOT/dist"

# ── Resolve versions ────────────────────────────────────────────────
# Framework version = the llama-cpp-config crate version, as in 03-package.ps1.
FW_VERSION="$(sed -n '/^\[package\]/,/^\[/s/^[[:space:]]*version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
              "$REPO_ROOT/llama-cpp-config/Cargo.toml" | head -n1)"
[[ -n "$FW_VERSION" ]] || die "could not read [package] version from llama-cpp-config/Cargo.toml"

# The bundled llama.cpp: its release tag when the checkout sits exactly on one
# (02-build.sh detaches onto it), else the build number shaped bNNNN, by the
# same rule and for the same reasons as 03-package.ps1.
[[ -d "$CFG_LLAMA_CPP_DIR/.git" ]] || die "no llama.cpp clone at $CFG_LLAMA_CPP_DIR. Run 02-build.sh first."
LLAMA_BUILD="$(git -C "$CFG_LLAMA_CPP_DIR" describe --tags 2>/dev/null | head -n1 || true)"
if [[ ! "$LLAMA_BUILD" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    n="$(git -C "$CFG_LLAMA_CPP_DIR" rev-list --count HEAD 2>/dev/null || true)"
    if [[ -n "$n" ]]; then LLAMA_BUILD="b$n"
    else LLAMA_BUILD="b0-$(git -C "$CFG_LLAMA_CPP_DIR" rev-parse --short HEAD)"; fi
    say_warn 'llama.cpp checkout is not on a release tag; naming it by build number'
fi

# Each format names the architecture its own way.
case "$(uname -m)" in
    x86_64)  DEB_ARCH='amd64'; RPM_ARCH='x86_64' ;;
    aarch64) DEB_ARCH='arm64'; RPM_ARCH='aarch64' ;;
    *) die "unsupported architecture: $(uname -m)" ;;
esac

say_head "Framework version: $FW_VERSION"
say_head "llama.cpp release: $LLAMA_BUILD"
say_head "Architecture:      $RPM_ARCH"

# ── Preconditions ───────────────────────────────────────────────────
[[ -x "$BUILD_DIR/bin/llama-server" ]] || die "llama-server not found in $BUILD_DIR/bin. Run 02-build.sh first."
[[ -x "$BUILD_DIR/bin/llama-cpp-config" ]] || die "llama-cpp-config not found in $BUILD_DIR/bin. Run 02-build.sh first."
# A version bump without a rebuild would ship the OLD configurator in a
# package named after the new version.
built="$("$BUILD_DIR/bin/llama-cpp-config" --version 2>/dev/null | awk '{print $NF}' || true)"
[[ "$built" == "$FW_VERSION" ]] ||
    die "the built llama-cpp-config is $built, Cargo.toml says $FW_VERSION. Run 02-build.sh first."

command -v patchelf >/dev/null || die 'patchelf not found. Run 00-install-prerequisites.sh (or: sudo apt-get install patchelf).'
if (( WANT_DEB )); then
    command -v dpkg-deb >/dev/null || die 'dpkg-deb not found: cannot build the .deb (use --rpm for the rpm alone).'
fi
if (( WANT_RPM )); then
    command -v rpmbuild >/dev/null || die 'rpmbuild not found. Run 00-install-prerequisites.sh (or: sudo apt-get install rpm), or use --deb.'
fi

# ── Stage ───────────────────────────────────────────────────────────
rm -rf "$STAGE_DIR" "$PKG_WORK"
mkdir -p "$STAGE_DIR/$BIN_DIR" "$STAGE_DIR/$RT_DIR" "$STAGE_DIR/usr/bin" "$STAGE_DIR/$ETC_DIR" "$PKG_WORK" "$OUT_DIR"

say_head 'Staging binaries...'
# Straight from the build's bin/, where 02-build.sh leaves executables,
# libraries, backends and llama-cpp-config side by side: the layout the
# package keeps. `cmake --install` would split them into bin/ and lib/.
# llama.cpp's own test-* harnesses stay out: they are its unit tests, and
# every executable staged here gets a command in /usr/bin.
pruned=0
for f in "$BUILD_DIR/bin"/*; do
    case "$(basename "$f")" in
        test-*) pruned=$(( pruned + 1 )); continue ;;
    esac
    cp -a "$f" "$STAGE_DIR/$BIN_DIR/"
done
(( pruned )) && say_dim "Pruned $pruned llama.cpp test binaries"

is_elf() { [[ -f "$1" && ! -L "$1" && "$(head -c4 "$1" | od -An -c | tr -d ' ')" == '177ELF' ]]; }

# ── Bundle the GPU runtimes ─────────────────────────────────────────
# What the backends load is copied out of the dists this build linked
# against, so the package runs on a machine that has the GPU drivers and
# nothing else: no ROCm, no CUDA Toolkit, no Vulkan SDK.
#
# WHICH libraries is not a list kept here: it is the ldd closure of the staged
# files, restricted to what resolves inside a dist. Each library lands under
# the name it is asked for (its soname), symlinks dereferenced.
#
# The ROCm dist keeps its layout under lib/ (lib/, lib/llvm/lib/,
# lib/rocm_sysdeps/lib/, lib/rocblas/library/, ...): its libraries find each
# other and their kernel files by $ORIGIN-relative paths, so a tree that
# mirrors the dist needs no patching of anything AMD shipped. That is also
# where the compiler's own runtime comes from: the CPU backends are built with
# the dist's clang and link ITS OpenMP runtime (lib/llvm/lib/libomp.so).
# CUDA and Vulkan libraries have no such structure and go flat into lib/.
HIP_ROOT="${CFG_HIP_PATH:-}"
CUDA_DIR="$(find_cuda)"
VULKAN_DIR="$(find_vulkan)"
VULKAN_LIB_DIR=''
if [[ -n "$VULKAN_DIR" ]]; then VULKAN_LIB_DIR="$(dirname "$(readlink -f "$(vulkan_loader "$VULKAN_DIR")")")"; fi

say_head 'Bundling the GPU runtimes...'
bundled=0
while read -r soname path; do
    real="$(readlink -f "$path")"
    if [[ -n "$HIP_ROOT" && "$path" == "$HIP_ROOT"/lib/* ]]; then
        rel="${path#"$HIP_ROOT"/lib/}"
        dest="$STAGE_DIR/$RT_DIR/$(dirname "$rel")/$soname"
    elif [[ ( -n "$CUDA_DIR" && "$real" == "$(readlink -f "$CUDA_DIR")"/* ) ||
            ( -n "$VULKAN_LIB_DIR" && "$real" == "$VULKAN_LIB_DIR"/* ) ]]; then
        dest="$STAGE_DIR/$RT_DIR/$soname"
    else
        continue
    fi
    [[ -e "$dest" ]] && continue
    mkdir -p "$(dirname "$dest")"
    cp -L "$path" "$dest"
    bundled=$(( bundled + 1 ))
done < <(for f in "$BUILD_DIR/bin"/*; do
             is_elf "$f" || continue
             case "$(basename "$f")" in test-*) continue ;; esac
             ldd "$f" 2>/dev/null
         done | awk '$2 == "=>" && $3 ~ /^\// { print $1, $3 }' | sort -u)
say_dim "$bundled runtime libraries"

# The Vulkan backend links the loader of the SDK it was built with only if
# ldd resolved it there; when the system loader answered instead, take the
# SDK's by name so the package still carries one.
if [[ -n "$VULKAN_LIB_DIR" && ! -e "$STAGE_DIR/$RT_DIR/libvulkan.so.1" && -e "$VULKAN_LIB_DIR/libvulkan.so.1" ]]; then
    cp -L "$VULKAN_LIB_DIR/libvulkan.so.1" "$STAGE_DIR/$RT_DIR/libvulkan.so.1"
    say_dim 'libvulkan.so.1 (Vulkan loader, from the SDK)'
fi

# ROCm kernel data, which no ldd shows: rocBLAS and hipBLASLt load their
# precompiled kernels per GPU architecture from <lib>/rocblas/library and
# <lib>/hipblaslt/library, and the kpack archives sit in <dist>/.kpack, which
# is <lib>/../.kpack here as there. Only the architectures this build targets
# (CFG_GPU_TARGETS) are taken: the rest could never be used by a
# libggml-hip.so that has no code for them, and they are most of the size.
# Files that name no architecture are shared and always copied.
if [[ -n "$HIP_ROOT" && -e "$STAGE_DIR/$RT_DIR/libamdhip64.so.7" ]]; then
    targets="|${CFG_GPU_TARGETS//;/|}|"
    wanted() {   # does this file belong to no architecture, or to a targeted one?
        local arch
        arch="$(grep -oE 'gfx[0-9a-zA-Z]+' <<<"$1" | head -n1 || true)"
        [[ -z "$arch" || "$targets" == *"|$arch|"* ]]
    }
    for sub in rocblas/library hipblaslt/library; do
        [[ -d "$HIP_ROOT/lib/$sub" ]] || continue
        mkdir -p "$STAGE_DIR/$RT_DIR/$sub"
        # Entries are files named after their architecture, or (rocBLAS, for
        # the newer ones) a directory per architecture.
        for f in "$HIP_ROOT/lib/$sub"/*; do
            wanted "$(basename "$f")" || continue
            cp -rL "$f" "$STAGE_DIR/$RT_DIR/$sub/"
        done
        say_dim "$sub: $(du -sm "$STAGE_DIR/$RT_DIR/$sub" | cut -f1) MiB"
    done
    if [[ -d "$HIP_ROOT/.kpack" ]]; then
        mkdir -p "$STAGE_DIR/$OPT_DIR/.kpack"
        for f in "$HIP_ROOT/.kpack"/blas_lib_*.kpack; do
            wanted "$(basename "$f")" || continue
            cp -L "$f" "$STAGE_DIR/$OPT_DIR/.kpack/"
        done
        say_dim ".kpack: $(du -sm "$STAGE_DIR/$OPT_DIR/.kpack" | cut -f1) MiB"
    fi
fi

# RUNPATH. The build tree's RUNPATH names the build tree itself and the dists
# under /mnt/data; a packaged file must find its siblings and the bundled
# runtimes wherever the tree lands. So every llama.cpp ELF gets $ORIGIN (the
# other llama.cpp libraries), $ORIGIN/../lib (the runtimes) and
# $ORIGIN/../lib/llvm/lib (libomp). The bundled runtime libraries are left
# exactly as their vendor built them. LD_LIBRARY_PATH still wins over RUNPATH.
RUNPATH='$ORIGIN:$ORIGIN/../lib:$ORIGIN/../lib/llvm/lib'

say_head 'Stripping and setting RUNPATH...'
before="$(du -sm "$STAGE_DIR/$BIN_DIR" | cut -f1)"
executables=()
for f in "$STAGE_DIR/$BIN_DIR"/*; do
    is_elf "$f" || continue
    # strip BEFORE patchelf: stripping a patched file can drop the section
    # patchelf appended. --strip-unneeded keeps the dynamic symbols, which is
    # all a shared library needs.
    strip --strip-unneeded "$f"
    patchelf --set-rpath "$RUNPATH" "$f"
    chmod 755 "$f"
    case "$(basename "$f")" in
        lib*.so*) ;;
        *) executables+=("$(basename "$f")") ;;
    esac
done
after="$(du -sm "$STAGE_DIR/$BIN_DIR" | cut -f1)"
say_dim "${#executables[@]} executables, $before MiB -> $after MiB stripped"
say_dim "Bundled runtimes: $(du -sm "$STAGE_DIR/$RT_DIR" | cut -f1) MiB"

for exe in "${executables[@]}"; do
    ln -s "../../$BIN_DIR/$exe" "$STAGE_DIR/usr/bin/$exe"   # relative: rpm warns on absolute ones
done

# ── Desktop entry, icon, licence ────────────────────────────────────
mkdir -p "$STAGE_DIR/usr/share/applications" "$STAGE_DIR/usr/share/doc/$PKG_NAME"
cat > "$STAGE_DIR/usr/share/applications/llama-cpp-config.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=llama.cpp Config
Comment=Configure and run llama-server and its model presets
Exec=llama-cpp-config
Icon=llama-cpp-config
Terminal=false
Categories=Utility;
StartupWMClass=llama-cpp-config
EOF

# The icon is the largest PNG frame of resources/llama.ico (the same image the
# Windows build uses), cut out of the container byte by byte: an .ico is a
# directory of 16-byte entries (size at +8, offset at +12, little endian)
# followed by the frames, and its large frames are plain PNG files.
ico="$REPO_ROOT/resources/llama.ico"
icon_png=''
if [[ -f "$ico" ]]; then
    u32() { od -An -tu4 -j "$2" -N4 "$1" | tr -d ' '; }
    count="$(od -An -tu2 -j4 -N2 "$ico" | tr -d ' ')"
    best_size=0; best_off=0
    for (( i = 0; i < count; i++ )); do
        size="$(u32 "$ico" $(( 6 + i * 16 + 8 )))"; off="$(u32 "$ico" $(( 6 + i * 16 + 12 )))"
        if (( size > best_size )) &&
           [[ "$(od -An -tx1 -j "$off" -N4 "$ico" | tr -d ' ')" == '89504e47' ]]; then
            best_size="$size"; best_off="$off"
        fi
    done
    if (( best_size )); then
        # A PNG's IHDR holds width and height big endian at bytes 16 and 20.
        px=$(( 0x$(od -An -tx1 -j $(( best_off + 16 )) -N4 "$ico" | tr -d ' ') ))
        icon_png="$STAGE_DIR/usr/share/icons/hicolor/${px}x${px}/apps/llama-cpp-config.png"
        mkdir -p "$(dirname "$icon_png")"
        tail -c "+$(( best_off + 1 ))" "$ico" | head -c "$best_size" > "$icon_png"
        say_dim "Icon: ${px}x${px} from resources/llama.ico"
    fi
fi
[[ -n "$icon_png" ]] || say_warn 'No PNG frame in resources/llama.ico: the desktop entry ships without an icon'

{
    printf '%s %s bundles llama.cpp %s, whose licence follows.\n' "$PKG_NAME" "$FW_VERSION" "$LLAMA_BUILD"
    printf 'The libraries under /%s are redistributed unmodified from the AMD ROCm\n' "$RT_DIR"
    printf '(TheRock) distribution, the NVIDIA CUDA redistributable archives and the\n'
    printf 'LunarG Vulkan SDK, each under its own licence.\n\n'
    cat "$CFG_LLAMA_CPP_DIR/LICENSE"
    printf '\nllama-cpp-config embeds the Fluent UI System Icons font, whose licence follows.\n\n'
    cat "$REPO_ROOT/llama-cpp-config/ui/fonts/LICENSE"
} > "$STAGE_DIR/usr/share/doc/$PKG_NAME/copyright"

# The repo may sit on a filesystem that hands out group-writable modes; a
# package carries 755 / 644 whatever the build drive says.
find "$STAGE_DIR" -type d -exec chmod 755 {} +
find "$STAGE_DIR/$RT_DIR" "$STAGE_DIR/$OPT_DIR/.kpack" -type f -name '*.so*' -exec chmod 755 {} + 2>/dev/null || true
find "$STAGE_DIR/$RT_DIR" "$STAGE_DIR/$OPT_DIR/.kpack" -type f ! -name '*.so*' -exec chmod 644 {} + 2>/dev/null || true
find "$STAGE_DIR/usr/share" -type f -exec chmod 644 {} +

# ── Maintainer scripts ──────────────────────────────────────────────
# /etc/llama.cpp is edited by llama-cpp-config, which runs as the desktop
# user, not as root. So the directory belongs to a system group and is
# group-writable + setgid (files created in it inherit the group), and the
# user who installs the package is added to that group. Membership takes
# effect at the next login; other users are added with
#   sudo usermod -aG llama-cpp <user>
# The same script serves both formats: neither depends on its arguments.
POST_SCRIPT="$(cat <<EOF
getent group $PKG_GROUP >/dev/null || groupadd --system $PKG_GROUP
chgrp $PKG_GROUP /$ETC_DIR
chmod 2775 /$ETC_DIR
user="\${SUDO_USER:-}"
if [ -z "\$user" ] && [ -n "\${PKEXEC_UID:-}" ]; then
    user="\$(getent passwd "\$PKEXEC_UID" | cut -d: -f1)"
fi
if [ -n "\$user" ] && [ "\$user" != root ] && ! id -nG "\$user" | tr ' ' '\n' | grep -qx $PKG_GROUP; then
    usermod -aG $PKG_GROUP "\$user"
    echo "$PKG_NAME: added \$user to the $PKG_GROUP group (owner of /$ETC_DIR); log out and back in for it to take effect."
fi
EOF
)"

DESCRIPTION_SHORT='llama.cpp with CUDA, Vulkan and ROCm backends, plus its configurator'
HOMEPAGE="$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null | sed -E 's/\.git$//' || true)"
MAINTAINER="${LLAMA_FW_MAINTAINER:-$(git -C "$REPO_ROOT" config user.name 2>/dev/null || true) <$(git -C "$REPO_ROOT" config user.email 2>/dev/null || true)>}"
[[ "$MAINTAINER" == ' <>' ]] && MAINTAINER="$PKG_NAME <nobody@localhost>"

# e.g. llama-cpp-framework-v1.17.0-v0.6.0-amd64.deb: the Windows installer's
# name with each format's own architecture token (framework version first,
# bundled llama.cpp release second).
DEB_FILE="$OUT_DIR/$PKG_NAME-v$FW_VERSION-$LLAMA_BUILD-$DEB_ARCH.deb"
RPM_FILE="$OUT_DIR/$PKG_NAME-v$FW_VERSION-$LLAMA_BUILD-$RPM_ARCH.rpm"

# ── .deb ────────────────────────────────────────────────────────────
if (( WANT_DEB )); then
    say_head 'Building the .deb...'
    # Depends, computed from what the staged ELF files actually link (minus
    # libvulkan1, which the scan names although the loader is bundled), the
    # bundled runtimes included (their own directories are given as private
    # library paths). libggml-cuda.so is left out of the scan: it links
    # libcuda.so.1, which is the NVIDIA DRIVER's and no Debian package's
    # (dpkg-shlibdeps fails on a library it cannot find), and the package
    # must install on a machine without an NVIDIA card.
    depends=''
    if command -v dpkg-shlibdeps >/dev/null; then
        mkdir -p "$PKG_WORK/shlibdeps/debian"
        printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' "$PKG_NAME" "$PKG_NAME" > "$PKG_WORK/shlibdeps/debian/control"
        scan=()
        private=()
        while read -r d; do private+=("-l$d"); done < <(find "$STAGE_DIR/$OPT_DIR" -type f -name '*.so*' -printf '%h\n' | sort -u)
        while read -r f; do
            is_elf "$f" || continue
            case "$(basename "$f")" in libggml-cuda.so|libcu*) continue ;; esac
            scan+=("-e$f")
        done < <(find "$STAGE_DIR/$OPT_DIR" -type f \( -name '*.so*' -o -perm -u+x \) | sort)
        depends="$(cd "$PKG_WORK/shlibdeps" &&
                   dpkg-shlibdeps -O --ignore-missing-info "${private[@]}" "${scan[@]}" 2>/dev/null |
                   sed -n 's/^shlibs:Depends=//p' | sed -E 's/(^|, )libvulkan1[^,]*//; s/^, //' || true)"
    fi
    if [[ -z "$depends" ]]; then
        depends='libc6, libstdc++6, libgcc-s1, libssl3t64 | libssl3, libfontconfig1, libfreetype6'
        say_warn "dpkg-shlibdeps gave no answer; using the fixed Depends list"
    fi

    DEB_ROOT="$PKG_WORK/deb"
    mkdir -p "$DEB_ROOT"
    cp -a "$STAGE_DIR/." "$DEB_ROOT/"
    mkdir -p "$DEB_ROOT/DEBIAN"
    # Recommends: what llama-cpp-config and the Vulkan backend load at run
    # time by dlopen, which no link-time scan sees.
    cat > "$DEB_ROOT/DEBIAN/control" <<EOF
Package: $PKG_NAME
Version: $FW_VERSION
Architecture: $DEB_ARCH
Maintainer: $MAINTAINER
Installed-Size: $(du -sk --exclude=DEBIAN "$DEB_ROOT" | cut -f1)
Depends: $depends
Recommends: libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libxcursor1, libxi6, libegl1
Conflicts: llama.cpp
Section: utils
Priority: optional
${HOMEPAGE:+Homepage: $HOMEPAGE
}Description: $DESCRIPTION_SHORT
 llama.cpp $LLAMA_BUILD built with the CUDA, Vulkan and ROCm (HIP) backends,
 and llama-cpp-config, the GUI + CLI configurator for server.ini and the
 per-model presets.ini (kept in /etc/llama.cpp).
 .
 The ROCm, CUDA and Vulkan runtime libraries are bundled in /opt/llama.cpp/lib:
 only the GPU drivers have to be installed. A backend whose GPU or driver is
 missing is skipped when llama-server starts.
EOF
    printf '#!/bin/sh\nset -e\n%s\n' "$POST_SCRIPT" > "$DEB_ROOT/DEBIAN/postinst"
    # purge removes the config files llama-cpp-config wrote: dpkg does not
    # know them (they are not shipped), so nothing else would. A plain remove
    # keeps them, which is what survives an upgrade or a reinstall.
    cat > "$DEB_ROOT/DEBIAN/postrm" <<EOF
#!/bin/sh
set -e
if [ "\$1" = purge ]; then
    rm -rf /$ETC_DIR
fi
EOF
    chmod 755 "$DEB_ROOT/DEBIAN/postinst" "$DEB_ROOT/DEBIAN/postrm"
    # zstd: the tree is several GiB of GPU kernels and xz would take many minutes.
    dpkg-deb --root-owner-group -Zzstd --build "$DEB_ROOT" "$DEB_FILE" >/dev/null
    rm -rf "$DEB_ROOT"
fi

# ── .rpm ────────────────────────────────────────────────────────────
if (( WANT_RPM )); then
    say_head 'Building the .rpm...'
    # rpm's automatic dependency scan is kept (it yields soname requirements
    # any rpm distro resolves), minus what it must not see: every library the
    # package carries itself (not "provided" to the system, so not "required"
    # from it either) and libcuda, which is the NVIDIA driver's.
    private="$(find "$STAGE_DIR/$OPT_DIR" -name 'lib*.so*' -printf '%f\n' | sed -E 's/\.so.*$//; s/[+.]/\\\\&/g' | sort -u | paste -sd'|')"
    spec="$PKG_WORK/$PKG_NAME.spec"
    cat > "$spec" <<EOF
Name:           $PKG_NAME
Version:        $FW_VERSION
Release:        1
Summary:        $DESCRIPTION_SHORT
License:        MIT (bundled llama.cpp)
${HOMEPAGE:+URL:            $HOMEPAGE
}Requires(post): shadow-utils
Conflicts:      llama-cpp

# The staged files are final: no stripping, no debuginfo, no build-id links,
# no brp post-processing of a tree this script already prepared.
%global debug_package %{nil}
%global __strip /bin/true
%global __os_install_post %{nil}
%global _build_id_links none
%global _binary_payload w3T0.zstdio
%global __provides_exclude_from ^/$OPT_DIR/.*\$
%global __requires_exclude ^($private|libcuda)\\\\.so

%description
llama.cpp $LLAMA_BUILD built with the CUDA, Vulkan and ROCm (HIP) backends,
and llama-cpp-config, the GUI + CLI configurator for server.ini and the
per-model presets.ini (kept in /etc/llama.cpp).

The ROCm, CUDA and Vulkan runtime libraries are bundled in /opt/llama.cpp/lib:
only the GPU drivers have to be installed. A backend whose GPU or driver is
missing is skipped when llama-server starts.

%install
cp -a "$STAGE_DIR/." "%{buildroot}/"

%post
$POST_SCRIPT

%files
%defattr(-,root,root,-)
/$OPT_DIR
/usr/bin/*
/usr/share/applications/llama-cpp-config.desktop
/usr/share/icons/hicolor/*/apps/llama-cpp-config.png
/usr/share/doc/$PKG_NAME
%dir /$ETC_DIR
EOF
    [[ -n "$icon_png" ]] || sed -i '\|/usr/share/icons/|d' "$spec"
    rpmbuild -bb --quiet --target "$RPM_ARCH" \
        --define "_topdir $PKG_WORK/rpmbuild" \
        --define "_dbpath $PKG_WORK/rpmdb" \
        --define "_rpmdir $OUT_DIR" \
        --define "_rpmfilename $(basename "$RPM_FILE")" \
        "$spec"
fi

# ── Cleanup ─────────────────────────────────────────────────────────
rm -rf "$PKG_WORK"
(( KEEP_STAGING )) || rm -rf "$STAGE_DIR"

say ''
(( WANT_DEB )) && say_ok "Package created: $DEB_FILE ($(du -m "$DEB_FILE" | cut -f1) MiB)"
(( WANT_RPM )) && say_ok "Package created: $RPM_FILE ($(du -m "$RPM_FILE" | cut -f1) MiB)"
say ''
