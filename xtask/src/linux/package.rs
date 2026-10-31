//! Package phase (cargo run -p xtask -- package): the Linux build (llama.cpp +
//! llama-cpp-config) as a .deb and an .rpm in ./dist/, the counterpart of
//! the Windows MSI (windows/package.rs). Requires a successful build; patchelf, dpkg-dev and rpm
//! come from the prerequisites.
//!
//! What the packages install (both from the same staged tree):
//!   /opt/llama.cpp/bin/   every llama.cpp tool, its shared libraries and the
//!                         ggml backends, plus llama-cpp-config: ONE
//!                         directory, because ggml looks for its backends next
//!                         to the executable and llama-cpp-config looks for
//!                         llama-server there too
//!   /opt/llama.cpp/lib/   the GPU runtimes the backends need, taken from the
//!                         dists this build linked against: the ROCm runtime
//!                         with its kernel libraries, the CUDA libraries, the
//!                         Vulkan loader. Self-contained: nothing else has to
//!                         be installed but the GPU drivers.
//!   /opt/llama.cpp/share/benchmarks/
//!                         the benchmark method, read-only: every published
//!                         standard-<X.Y>.ini and the prompts they pin
//!   /usr/bin/             a symlink per executable into /opt/llama.cpp/bin
//!   /usr/share/           the desktop entry, its icon, the licence
//!
//! The config files (server.ini, presets.ini, presets.ini.disabled,
//! settings.ini) are NOT in the package: llama-cpp-config
//! writes them per user, in ~/.config/llama.cpp, on first use.

use crate::common::configurator::set_mode;
use super::build_llama::load_build_env;
use super::dists;
use crate::common::ui;
use crate::common::version;
use crate::common::util::{self, cmd, gfx_arch, output, run as run_cmd, which};
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const PKG_NAME: &str = "llama-cpp-framework";
const OPT_DIR: &str = "opt/llama.cpp"; // relative to the package root
const BIN_DIR: &str = "opt/llama.cpp/bin"; // llama.cpp + llama-cpp-config
const RT_DIR: &str = "opt/llama.cpp/lib"; // the bundled GPU runtimes

fn is_elf(p: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(p) else { return false };
    if !m.is_file() {
        return false;
    }
    let mut magic = [0u8; 4];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && magic == *b"\x7fELF"
}

fn name_of(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// The entries of a directory, sorted by name (a shell glob's order).
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| !name_of(p).starts_with('.')).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Every path under `dir` (`find dir`, the root excluded), not following
/// symlinks.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(p.clone());
            }
            out.push(p);
        }
    }
    out.sort();
    out
}

fn is_file(p: &Path) -> bool {
    std::fs::symlink_metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

/// `cp <flags> <srcs...> <dest>`, in chunks so no argument list overflows.
fn cp(flags: &str, srcs: &[PathBuf], dest: &Path) -> Result<()> {
    for chunk in srcs.chunks(500) {
        run_cmd(cmd("cp").arg(flags).args(chunk).arg(dest))?;
    }
    Ok(())
}

pub fn run(want_deb: bool, want_rpm: bool, keep_staging: bool) -> Result<()> {
    let (cfg, dists) = load_build_env()?;
    let repo = util::repo_root();
    let build_root = util::build_root();
    let build_dir = build_root.join("llama.cpp-cmake");
    let stage = build_root.join("staging");
    let pkg_work = build_root.join("package"); // control files, spec, rpmbuild tree: transient
    let out_dir = repo.join("dist");
    let llama = PathBuf::from(&cfg.llama_cpp_dir);

    // ── Resolve versions ────────────────────────────────────────────────
    let fw_version = version::framework()?;
    let llama_build = version::llama_build(&llama)?;

    // Each format names the architecture its own way.
    let (deb_arch, rpm_arch) = match std::env::consts::ARCH {
        "x86_64" => ("amd64", "x86_64"),
        "aarch64" => ("arm64", "aarch64"),
        a => bail!("unsupported architecture: {a}"),
    };

    // The package version carries the bundled llama.cpp too, as a "+"
    // suffix: a rebuild on a newer llama.cpp is then an UPGRADE to the package
    // manager (dpkg and rpm both order 2.0.0 < 2.0.0+llama.cpp.v0.7.0 <
    // ...v0.10.0, and the next framework version above all of them), and
    // `apt policy`/`rpm -q` name what is installed. "-" is not allowed in an
    // rpm Version: the b0-<sha> fallback gets "." instead.
    let pkg_version = format!("{fw_version}+llama.cpp.{}", llama_build.replace('-', "."));

    ui::head(format!("Framework version: {fw_version}"));
    ui::head(format!("llama.cpp release: {llama_build}"));
    ui::head(format!("Package version:   {pkg_version}"));
    ui::head(format!("Architecture:      {rpm_arch}"));

    // ── Preconditions ───────────────────────────────────────────────────
    let bin = build_dir.join("bin");
    if !util::is_executable(&bin.join("llama-server")) {
        bail!("llama-server not found in {}. Run cargo run -p xtask -- build first.", bin.display());
    }
    if !util::is_executable(&bin.join("llama-cpp-config")) {
        bail!("llama-cpp-config not found in {}. Run cargo run -p xtask -- build first.", bin.display());
    }
    // A version bump without a rebuild would ship the OLD configurator in a
    // package named after the new version.
    let built = output(cmd(bin.join("llama-cpp-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();
    if built != fw_version {
        bail!("the built llama-cpp-config is {built}, Cargo.toml says {fw_version}. Run cargo run -p xtask -- build first.");
    }
    if which("patchelf").is_none() {
        bail!("patchelf not found. Run scripts/00-install-prerequisites.sh (or: sudo apt-get install patchelf).");
    }
    if want_deb && which("dpkg-deb").is_none() {
        bail!("dpkg-deb not found: cannot build the .deb (use --rpm for the rpm alone).");
    }
    if want_rpm && which("rpmbuild").is_none() {
        bail!("rpmbuild not found. Run scripts/00-install-prerequisites.sh (or: sudo apt-get install rpm), or use --deb.");
    }

    // ── Stage ───────────────────────────────────────────────────────────
    util::remove_all(&stage)?;
    util::remove_all(&pkg_work)?;
    for d in [stage.join(BIN_DIR), stage.join(RT_DIR), stage.join("usr/bin"), pkg_work.clone(), out_dir.clone()] {
        std::fs::create_dir_all(&d)?;
    }

    ui::head("Staging binaries...");
    // Straight from the build's bin/, where the build leaves executables,
    // libraries, backends and llama-cpp-config side by side: the layout the
    // package keeps. `cmake --install` would split them into bin/ and lib/.
    // llama.cpp's own test-* harnesses stay out: they are its unit tests, and
    // every executable staged here gets a command in /usr/bin.
    let (tests, keep): (Vec<PathBuf>, Vec<PathBuf>) =
        entries(&bin).into_iter().partition(|p| name_of(p).starts_with("test-"));
    cp("-a", &keep, &stage.join(BIN_DIR))?;
    crate::common::bench_share::stage(&stage.join(OPT_DIR))?;
    if !tests.is_empty() {
        ui::dim(format!("Pruned {} llama.cpp test binaries", tests.len()));
    }

    // ── Bundle the GPU runtimes ─────────────────────────────────────────
    // What the backends load is copied out of the dists this build linked
    // against, so the package runs on a machine that has the GPU drivers and
    // nothing else: no ROCm, no CUDA Toolkit, no Vulkan SDK.
    //
    // WHICH libraries is not a list kept here: it is the ldd closure of the
    // staged files, restricted to what resolves inside a dist. Each library
    // lands under the name it is asked for (its soname), symlinks
    // dereferenced.
    //
    // The ROCm dist keeps its layout under lib/ (lib/, lib/llvm/lib/,
    // lib/rocm_sysdeps/lib/, lib/rocblas/library/, ...): its libraries find
    // each other and their kernel files by $ORIGIN-relative paths, so a tree
    // that mirrors the dist needs no patching of anything AMD shipped. That is
    // also where the compiler's own runtime comes from: the CPU backends are
    // built with the dist's clang and link ITS OpenMP runtime
    // (lib/llvm/lib/libomp.so). CUDA, NCCL and Vulkan libraries have no such
    // structure and go flat into lib/.
    let hip_root = cfg.hip_path.clone();
    let canon = |p: &Path| p.canonicalize().map(|c| c.display().to_string()).unwrap_or_default();
    let cuda_dir = dists.find_cuda().map(|d| canon(&d));
    let nccl_dir = dists.find_nccl().map(|d| canon(&d));
    let vulkan_dir = dists.find_vulkan();
    let vulkan_lib_dir = vulkan_dir
        .as_deref()
        .and_then(dists::vulkan_loader)
        .and_then(|l| l.canonicalize().ok())
        .and_then(|l| l.parent().map(|d| d.display().to_string()));

    ui::head("Bundling the GPU runtimes...");
    let mut closure = BTreeSet::new();
    for f in entries(&bin) {
        if !is_elf(&f) || name_of(&f).starts_with("test-") {
            continue;
        }
        let Some(ldd) = output(cmd("ldd").arg(&f)) else { continue };
        for line in ldd.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 3 && cols[1] == "=>" && cols[2].starts_with('/') {
                closure.insert((cols[0].to_string(), cols[2].to_string()));
            }
        }
    }
    let under = |real: &str, dir: &Option<String>| dir.as_ref().map(|d| real.starts_with(&format!("{d}/"))).unwrap_or(false);
    let rt = stage.join(RT_DIR);
    let mut bundled = 0;
    for (soname, path) in &closure {
        let real = canon(Path::new(path));
        let hip_lib = format!("{hip_root}/lib/");
        let dest = if !hip_root.is_empty() && path.starts_with(&hip_lib) {
            let rel = Path::new(&path[hip_lib.len()..]);
            rt.join(rel.parent().unwrap_or(Path::new(""))).join(soname)
        } else if under(&real, &cuda_dir) || under(&real, &nccl_dir) || under(&real, &vulkan_lib_dir) {
            rt.join(soname)
        } else {
            continue;
        };
        if dest.exists() {
            continue;
        }
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::copy(path, &dest).with_context(|| format!("copying {path}"))?;
        bundled += 1;
    }
    ui::dim(format!("{bundled} runtime libraries"));

    // A backend whose runtime did not resolve inside a dist (ldd answered
    // from a system ROCm, or the dist is gone from under the dist root) would
    // be dropped silently by ggml on every machine but this one.
    let rt_names: Vec<String> = entries(&rt).iter().map(|p| name_of(p)).collect();
    for (backend, runtime) in [("libggml-hip.so", "libhipblas.so."), ("libggml-cuda.so", "libcublas.so.")] {
        if stage.join(BIN_DIR).join(backend).exists() && !rt_names.iter().any(|n| n.starts_with(runtime)) {
            bail!(
                "{backend} is staged but {runtime}* was not found in its dist under {}; refusing to package a backend that cannot load",
                cfg.dist_root
            );
        }
    }

    // The Vulkan backend links the loader of the SDK it was built with only
    // if ldd resolved it there; when the system loader answered instead, take
    // the SDK's by name so the package still carries one.
    if let Some(dir) = &vulkan_lib_dir {
        let src = Path::new(dir).join("libvulkan.so.1");
        if !rt.join("libvulkan.so.1").exists() && src.exists() {
            std::fs::copy(&src, rt.join("libvulkan.so.1"))?;
            ui::dim("libvulkan.so.1 (Vulkan loader, from the SDK)");
        }
    }

    // ROCm kernel data, which no ldd shows: rocBLAS and hipBLASLt load their
    // precompiled kernels per GPU architecture from <lib>/rocblas/library and
    // <lib>/hipblaslt/library, and the kpack archives sit in <dist>/.kpack,
    // which is <lib>/../.kpack here as there: hipBLASLt's (blas_lib) and
    // RCCL's (rccl_lib, the collective kernels --split-mode tensor reduces
    // through). Only the architectures this build targets (gpu_targets) are
    // taken: the rest could never be used by a libggml-hip.so that has no
    // code for them, and they are most of the size. Files that name no
    // architecture are shared and always copied.
    if !hip_root.is_empty() && rt.join("libamdhip64.so.7").exists() {
        let targets: BTreeSet<&str> = cfg.gpu_targets.split(';').collect();
        let wanted = |name: &str| gfx_arch(name).map(|a| targets.contains(a)).unwrap_or(true);
        let hip = Path::new(&hip_root);
        for sub in ["rocblas/library", "hipblaslt/library"] {
            let src = hip.join("lib").join(sub);
            if !src.is_dir() {
                continue;
            }
            let dest = rt.join(sub);
            std::fs::create_dir_all(&dest)?;
            // Entries are files named after their architecture, or (rocBLAS,
            // for the newer ones) a directory per architecture.
            let picked: Vec<PathBuf> = entries(&src).into_iter().filter(|p| wanted(&name_of(p))).collect();
            cp("-rL", &picked, &dest)?;
            ui::dim(format!("{sub}: {} MiB", util::du("-sm", &dest)));
        }
        let kpack = hip.join(".kpack");
        if kpack.is_dir() {
            let dest = stage.join(OPT_DIR).join(".kpack");
            std::fs::create_dir_all(&dest)?;
            let mut picked = Vec::new();
            for prefix in ["blas_lib_", "rccl_lib_"] {
                picked.extend(entries(&kpack).into_iter().filter(|p| {
                    let n = name_of(p);
                    n.starts_with(prefix) && n.ends_with(".kpack") && wanted(&n)
                }));
            }
            cp("-L", &picked, &dest)?;
            ui::dim(format!(".kpack: {} MiB", util::du("-sm", &dest)));
        }
    }

    // RUNPATH. The build tree's RUNPATH names the build tree itself and the
    // dists under /mnt/data; a packaged file must find its siblings and the
    // bundled runtimes wherever the tree lands. So every llama.cpp ELF gets
    // $ORIGIN (the other llama.cpp libraries), $ORIGIN/../lib (the runtimes)
    // and $ORIGIN/../lib/llvm/lib (libomp). The bundled runtime libraries are
    // left exactly as their vendor built them. LD_LIBRARY_PATH still wins
    // over RUNPATH.
    const RUNPATH: &str = "$ORIGIN:$ORIGIN/../lib:$ORIGIN/../lib/llvm/lib";

    ui::head("Stripping and setting RUNPATH...");
    let staged_bin = stage.join(BIN_DIR);
    let before = util::du("-sm", &staged_bin);
    let mut executables = Vec::new();
    for f in entries(&staged_bin) {
        if !is_elf(&f) {
            continue;
        }
        // strip BEFORE patchelf: stripping a patched file can drop the
        // section patchelf appended. --strip-unneeded keeps the dynamic
        // symbols, which is all a shared library needs.
        run_cmd(cmd("strip").arg("--strip-unneeded").arg(&f))?;
        run_cmd(cmd("patchelf").args(["--set-rpath", RUNPATH]).arg(&f))?;
        set_mode(&f, 0o755)?;
        let n = name_of(&f);
        if !(n.starts_with("lib") && n.contains(".so")) {
            executables.push(n);
        }
    }
    let after = util::du("-sm", &staged_bin);
    ui::dim(format!("{} executables, {before} MiB -> {after} MiB stripped", executables.len()));
    ui::dim(format!("Bundled runtimes: {} MiB", util::du("-sm", &rt)));

    for exe in &executables {
        // relative: rpm warns on absolute ones
        std::os::unix::fs::symlink(format!("../../{BIN_DIR}/{exe}"), stage.join("usr/bin").join(exe))?;
    }

    // ── Desktop entry, icon, licence ────────────────────────────────────
    let apps = stage.join("usr/share/applications");
    let doc = stage.join("usr/share/doc").join(PKG_NAME);
    std::fs::create_dir_all(&apps)?;
    std::fs::create_dir_all(&doc)?;
    std::fs::write(
        apps.join("llama-cpp-config.desktop"),
        "[Desktop Entry]\nType=Application\nName=llama.cpp Config\nComment=Configure and run llama-server and its model presets\nExec=llama-cpp-config\nIcon=llama-cpp-config\nTerminal=false\nCategories=Utility;\nStartupWMClass=llama-cpp-config\n",
    )?;

    let icon = stage_icon(&repo.join("resources/llama.ico"), &stage)?;
    if icon.is_none() {
        ui::warn("No PNG frame in resources/llama.ico: the desktop entry ships without an icon");
    }

    let mut copyright = format!("{PKG_NAME} {fw_version} is distributed under the MIT licence:\n\n");
    copyright.push_str(&std::fs::read_to_string(repo.join("LICENSE"))?);
    copyright.push_str(&format!("\nIt bundles llama.cpp {llama_build}, whose licence follows.\n"));
    copyright.push_str(&format!(
        "The libraries under /{RT_DIR} are redistributed unmodified from the AMD ROCm\n"
    ));
    copyright.push_str("(TheRock) distribution, the NVIDIA CUDA redistributable archives and the\n");
    copyright.push_str("LunarG Vulkan SDK, each under its own licence.\n\n");
    copyright.push_str(&std::fs::read_to_string(llama.join("LICENSE"))?);
    copyright.push_str("\nllama-cpp-config embeds the Fluent UI System Icons font, whose licence follows.\n\n");
    copyright.push_str(&std::fs::read_to_string(repo.join("llama-cpp-config/ui/fonts/LICENSE"))?);
    std::fs::write(doc.join("copyright"), copyright)?;

    // The repo may sit on a filesystem that hands out group-writable modes; a
    // package carries 755 / 644 whatever the build drive says.
    for p in walk(&stage) {
        let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
        if m.is_dir() {
            set_mode(&p, 0o755)?;
        }
    }
    for dir in [rt.clone(), stage.join(OPT_DIR).join(".kpack")] {
        for p in walk(&dir).into_iter().filter(|p| is_file(p)) {
            set_mode(&p, if name_of(&p).contains(".so") { 0o755 } else { 0o644 })?;
        }
    }
    for p in walk(&stage.join("usr/share")).into_iter().filter(|p| is_file(p)) {
        set_mode(&p, 0o644)?;
    }

    // No maintainer scripts: the package ships no config directory,
    // llama-cpp-config writes ~/.config/llama.cpp as whoever runs it.

    let description_short = "llama.cpp with CUDA, Vulkan and ROCm backends, plus its configurator";
    let repo_git = |args: &[&str]| output(cmd("git").arg("-C").arg(&repo).args(args)).unwrap_or_default();
    let homepage = {
        let url = repo_git(&["remote", "get-url", "origin"]);
        url.strip_suffix(".git").map(str::to_string).unwrap_or(url)
    };
    let maintainer = match std::env::var("LLAMA_FW_MAINTAINER") {
        Ok(m) if !m.is_empty() => m,
        _ => {
            let m = format!("{} <{}>", repo_git(&["config", "user.name"]), repo_git(&["config", "user.email"]));
            if m == " <>" {
                format!("{PKG_NAME} <nobody@localhost>")
            } else {
                m
            }
        }
    };

    // e.g. llama-cpp-framework-v1.17.0-v0.6.0-amd64.deb: the Windows
    // installer's name with each format's own architecture token (framework
    // version first, bundled llama.cpp release second).
    let deb_file = out_dir.join(format!("{PKG_NAME}-v{fw_version}-{llama_build}-{deb_arch}.deb"));
    let rpm_file = out_dir.join(format!("{PKG_NAME}-v{fw_version}-{llama_build}-{rpm_arch}.rpm"));

    let long_description = format!(
        "llama.cpp {llama_build} built with the CUDA, Vulkan and ROCm (HIP) backends,\n\
         and llama-cpp-config, the GUI + CLI configurator for server.ini and the\n\
         per-model presets.ini (kept per user, in ~/.config/llama.cpp).\n\
         \n\
         The ROCm, CUDA and Vulkan runtime libraries are bundled in /opt/llama.cpp/lib:\n\
         only the GPU drivers have to be installed. A backend whose GPU or driver is\n\
         missing is skipped when llama-server starts.\n"
    );

    if want_deb {
        build_deb(&stage, &pkg_work, &deb_file, &DebFields {
            version: &pkg_version,
            arch: deb_arch,
            maintainer: &maintainer,
            homepage: &homepage,
            summary: description_short,
            description: &long_description,
        })?;
    }
    if want_rpm {
        build_rpm(&stage, &pkg_work, &out_dir, &rpm_file, icon.is_some(), &RpmFields {
            version: &pkg_version,
            arch: rpm_arch,
            homepage: &homepage,
            summary: description_short,
            description: &long_description,
        })?;
    }

    // ── Cleanup ─────────────────────────────────────────────────────────
    util::remove_all(&pkg_work)?;
    if !keep_staging {
        util::remove_all(&stage)?;
    }

    ui::say("");
    if want_deb {
        ui::ok(format!("Package created: {} ({} MiB)", deb_file.display(), util::du("-m", &deb_file)));
    }
    if want_rpm {
        ui::ok(format!("Package created: {} ({} MiB)", rpm_file.display(), util::du("-m", &rpm_file)));
    }
    ui::say("");
    Ok(())
}

/// The icon is the largest PNG frame of resources/llama.ico (the same image
/// the Windows build uses), cut out of the container: an .ico is a directory
/// of 16-byte entries (size at +8, offset at +12, little endian) after a
/// 6-byte header, followed by the frames, and its large frames are plain PNG
/// files. Returns the staged PNG, or None when there is none.
fn stage_icon(ico: &Path, stage: &Path) -> Result<Option<PathBuf>> {
    let Ok(data) = std::fs::read(ico) else { return Ok(None) };
    let u16le = |o: usize| data.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32le = |o: usize| data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let count = u16le(4).unwrap_or(0);
    let (mut best_size, mut best_off) = (0, 0);
    for i in 0..count {
        let (Some(size), Some(off)) = (u32le(6 + i * 16 + 8), u32le(6 + i * 16 + 12)) else { continue };
        if size > best_size && data.get(off..off + 4) == Some(b"\x89PNG") {
            best_size = size;
            best_off = off;
        }
    }
    if best_size == 0 {
        return Ok(None);
    }
    // A PNG's IHDR holds width and height big endian at bytes 16 and 20.
    let px = data
        .get(best_off + 16..best_off + 20)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0);
    let png = stage.join(format!("usr/share/icons/hicolor/{px}x{px}/apps/llama-cpp-config.png"));
    std::fs::create_dir_all(png.parent().unwrap())?;
    let end = (best_off + best_size).min(data.len());
    std::fs::write(&png, &data[best_off..end])?;
    ui::dim(format!("Icon: {px}x{px} from resources/llama.ico"));
    Ok(Some(png))
}

struct DebFields<'a> {
    version: &'a str,
    arch: &'a str,
    maintainer: &'a str,
    homepage: &'a str,
    summary: &'a str,
    description: &'a str,
}

fn build_deb(stage: &Path, pkg_work: &Path, deb_file: &Path, f: &DebFields) -> Result<()> {
    ui::head("Building the .deb...");
    // Depends, computed from what the staged ELF files actually link (minus
    // libvulkan1, which the scan names although the loader is bundled), the
    // bundled runtimes included (their own directories are given as private
    // library paths). libggml-cuda.so is left out of the scan: it links
    // libcuda.so.1, which is the NVIDIA DRIVER's and no Debian package's
    // (dpkg-shlibdeps fails on a library it cannot find), and the package
    // must install on a machine without an NVIDIA card.
    let mut depends = String::new();
    if which("dpkg-shlibdeps").is_some() {
        let work = pkg_work.join("shlibdeps");
        std::fs::create_dir_all(work.join("debian"))?;
        std::fs::write(
            work.join("debian/control"),
            format!("Source: {PKG_NAME}\n\nPackage: {PKG_NAME}\nArchitecture: any\n"),
        )?;
        let opt = stage.join(OPT_DIR);
        let all = walk(&opt);
        let private: BTreeSet<String> = all
            .iter()
            .filter(|p| is_file(p) && name_of(p).contains(".so"))
            .filter_map(|p| p.parent().map(|d| format!("-l{}", d.display())))
            .collect();
        let scan: Vec<String> = all
            .iter()
            .filter(|p| is_file(p) && (name_of(p).contains(".so") || user_executable(p)))
            .filter(|p| is_elf(p))
            .filter(|p| {
                let n = name_of(p);
                n != "libggml-cuda.so" && !n.starts_with("libcu")
            })
            .map(|p| format!("-e{}", p.display()))
            .collect();
        if let Some(out) = output(
            cmd("dpkg-shlibdeps")
                .current_dir(&work)
                .args(["-O", "--ignore-missing-info"])
                .args(&private)
                .args(&scan),
        ) {
            if let Some(line) = out.lines().find_map(|l| l.strip_prefix("shlibs:Depends=")) {
                depends = drop_libvulkan(line);
            }
        }
    }
    if depends.is_empty() {
        depends = "libc6, libstdc++6, libgcc-s1, libssl3t64 | libssl3, libfontconfig1, libfreetype6".into();
        ui::warn("dpkg-shlibdeps gave no answer; using the fixed Depends list");
    }

    let root = pkg_work.join("deb");
    std::fs::create_dir_all(&root)?;
    run_cmd(cmd("cp").arg("-a").arg(format!("{}/.", stage.display())).arg(format!("{}/", root.display())))?;
    std::fs::create_dir_all(root.join("DEBIAN"))?;
    let installed_size = output(cmd("du").args(["-sk", "--exclude=DEBIAN"]).arg(&root))
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .unwrap_or_default();
    let homepage = if f.homepage.is_empty() { String::new() } else { format!("Homepage: {}\n", f.homepage) };
    // Debian folds the long description one space in, a blank line as " .".
    let body: String = f
        .description
        .lines()
        .map(|l| if l.is_empty() { " .\n".to_string() } else { format!(" {l}\n") })
        .collect();
    // Recommends: what llama-cpp-config and the Vulkan backend load at run
    // time by dlopen, which no link-time scan sees.
    std::fs::write(
        root.join("DEBIAN/control"),
        format!(
            "Package: {PKG_NAME}\nVersion: {}\nArchitecture: {}\nMaintainer: {}\nInstalled-Size: {installed_size}\nDepends: {depends}\nRecommends: libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libxcursor1, libxi6, libegl1\nConflicts: llama.cpp\nSection: utils\nPriority: optional\n{homepage}Description: {}\n{body}",
            f.version, f.arch, f.maintainer, f.summary
        ),
    )?;
    // zstd: the tree is several GiB of GPU kernels and xz would take many
    // minutes.
    let status = cmd("dpkg-deb")
        .args(["--root-owner-group", "-Zzstd", "--build"])
        .arg(&root)
        .arg(deb_file)
        .stdout(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        bail!("dpkg-deb failed ({status})");
    }
    util::remove_all(&root)
}

/// `-perm -u+x`
fn user_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::symlink_metadata(p).map(|m| m.permissions().mode() & 0o100 != 0).unwrap_or(false)
}

/// Drops the first libvulkan1 entry of a Depends list (the loader is
/// bundled).
fn drop_libvulkan(depends: &str) -> String {
    let parts: Vec<&str> = depends.split(", ").collect();
    match parts.iter().position(|p| p.starts_with("libvulkan1")) {
        Some(i) => parts.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect::<Vec<_>>().join(", "),
        None => depends.to_string(),
    }
}

struct RpmFields<'a> {
    version: &'a str,
    arch: &'a str,
    homepage: &'a str,
    summary: &'a str,
    description: &'a str,
}

fn build_rpm(stage: &Path, pkg_work: &Path, out_dir: &Path, rpm_file: &Path, has_icon: bool, f: &RpmFields) -> Result<()> {
    ui::head("Building the .rpm...");
    // rpm's automatic dependency scan is kept (it yields soname requirements
    // any rpm distro resolves), minus what it must not see: every library the
    // package carries itself (not "provided" to the system, so not "required"
    // from it either) and libcuda, which is the NVIDIA driver's.
    let private: BTreeSet<String> = walk(&stage.join(OPT_DIR))
        .iter()
        .map(|p| name_of(p))
        .filter(|n| n.starts_with("lib") && n.contains(".so"))
        .map(|n| {
            let stem = &n[..n.find(".so").unwrap()];
            stem.chars()
                .map(|c| if c == '+' || c == '.' { format!("\\\\{c}") } else { c.to_string() })
                .collect()
        })
        .collect();
    let private = private.into_iter().collect::<Vec<_>>().join("|");
    let url = if f.homepage.is_empty() { String::new() } else { format!("URL:            {}\n", f.homepage) };
    let icon_line = if has_icon { "/usr/share/icons/hicolor/*/apps/llama-cpp-config.png\n" } else { "" };
    let spec = pkg_work.join(format!("{PKG_NAME}.spec"));
    std::fs::write(
        &spec,
        format!(
            r#"Name:           {PKG_NAME}
Version:        {version}
Release:        1
Summary:        {summary}
License:        MIT
{url}Conflicts:      llama-cpp

# The staged files are final: no stripping, no debuginfo, no build-id links,
# no brp post-processing of a tree this phase already prepared.
%global debug_package %{{nil}}
%global __strip /bin/true
%global __os_install_post %{{nil}}
%global _build_id_links none
%global _binary_payload w3T0.zstdio
%global __provides_exclude_from ^/{OPT_DIR}/.*$
%global __requires_exclude ^({private}|libcuda)\\.so

%description
{description}
%install
cp -a "{stage}/." "%{{buildroot}}/"

%files
%defattr(-,root,root,-)
/{OPT_DIR}
/usr/bin/*
/usr/share/applications/llama-cpp-config.desktop
{icon_line}/usr/share/doc/{PKG_NAME}
"#,
            version = f.version,
            summary = f.summary,
            description = f.description,
            stage = stage.display(),
        ),
    )?;
    let file_name = rpm_file.file_name().unwrap().to_string_lossy().into_owned();
    run_cmd(
        cmd("rpmbuild")
            .args(["-bb", "--quiet", "--target", f.arch])
            .arg("--define")
            .arg(format!("_topdir {}", pkg_work.join("rpmbuild").display()))
            .arg("--define")
            .arg(format!("_dbpath {}", pkg_work.join("rpmdb").display()))
            .arg("--define")
            .arg(format!("_rpmdir {}", out_dir.display()))
            .arg("--define")
            .arg(format!("_rpmfilename {file_name}"))
            .arg(&spec),
    )
    .context("rpmbuild")
}
