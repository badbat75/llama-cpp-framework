//! Package phase (cargo run -p xtask -- package): the Linux build as eight .deb
//! and eight .rpm packages in ./dist/ (common/components.rs says which file goes
//! where): the framework's pai-config, pai, pai-bff and core (pai-manager and
//! its services), and the engine, llama.cpp, with its ROCm, CUDA and Vulkan
//! runtimes.
//! The counterpart of the Windows MSI (windows/package.rs). Requires a successful
//! build; patchelf, dpkg-dev and rpm come from the prerequisites.
//!
//! What the packages install together (all from one staged tree), in the
//! standard layout under /usr:
//!   /usr/bin/             every llama.cpp executable, pai-config and pai, the real
//!                         files (the distributions' llama.cpp packages are
//!                         refused by Conflicts, so no file is shared)
//!   /usr/libexec/private-ai/
//!                         pai-manager and pai-bff, internal: the units call
//!                         them by full path
//!   /usr/lib/private-ai/  llama.cpp's shared libraries and the ggml backends,
//!                         a private directory (never a system library
//!                         directory): the executables find it by RUNPATH and
//!                         ggml its backends by GGML_BACKEND_DIR
//!                         (xtask/cmake/linux.toml)
//!   /usr/lib/private-ai/lib/
//!                         the GPU runtimes the backends need, taken from the
//!                         dists this build linked against: the ROCm runtime
//!                         with its kernel libraries (its kpack archives in
//!                         /usr/lib/private-ai/.kpack), the CUDA libraries, the
//!                         Vulkan loader. Self-contained: nothing else has to
//!                         be installed but the GPU drivers.
//!   /usr/share/private-ai/benchmarks/
//!                         the benchmark method, read-only: every published
//!                         standard-<X.Y>.ini and the prompts they pin
//!   /usr/share/private-ai/strings/
//!                         the front ends' language files, read-only
//!   /usr/share/private-ai/licenses/<component>/
//!                         the licence texts of each package
//!   /usr/share/           the desktop entry, its icon, the copyright file
//!   /usr/lib/systemd/system/, /usr/lib/sysusers.d/, /usr/share/dbus-1/,
//!   /usr/share/polkit-1/  the system files of the services, in the core
//!                         package (linux/system_files.rs), with the
//!                         maintainer scripts that create the service user and
//!                         enable, start and stop the manager; the web
//!                         service's unit in the bff's package, with its own
//!                         maintainer scripts
//!
//! The framework's packages carry the framework's version alone: each requires
//! the core at its own version (pai-config also pai's), and those that run the
//! engine (pai-config, pai, the core) llama.cpp at least at the release they
//! were built against (`components::llama_cpp_min`). The engine carries its
//! own version, `1:<release>+pai<N>` (`components::llama_cpp_version`), and
//! requires nothing of the framework. A runtime carries its
//! SDK's version and is optional: llama.cpp recommends the one it was built
//! against and refuses any other, because a backend that cannot resolve a symbol
//! is skipped at start with nothing but a log line.
//!
//! No config file is in the package. The server and preset files
//! (server@<id>.ini, presets.ini, presets.ini.disabled) live in
//! /etc/private-ai, which systemd creates at the manager's start
//! (ConfigurationDirectory=) and only the manager writes; settings.ini stays
//! per user, written by pai-config in ~/.config/private-ai on first use.

use super::build_llama::load_build_env;
use super::dists;
use super::system_files;
use crate::common::components::{self, Component};
use crate::common::configurator::set_mode;
use crate::common::licenses;
use crate::common::ui;
use crate::common::util::{self, cmd, gfx_arch, output, run as run_cmd, which};
use crate::common::version;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The single package every release up to `MONOLITH_LAST` shipped, which the
/// split packages replace.
const MONOLITH: &str = components::OLD_BASE_NAME;
/// The last release that shipped it: the relation against `MONOLITH` stops there, an
/// unversioned one matches every version of the name, future ones included (rpmbuild
/// warns about it). See "The `.deb` and `.rpm` (Linux)" in docs/build-packaging.md.
const MONOLITH_LAST: &str = "2.2.0";
/// The bound of the `.rpm` Obsoletes against the 2.4 configurator and llama.cpp
/// packages (`<MONOLITH>-<component>`): the first version after the last release
/// under the old name. Their versions read `2.4.0+llama.cpp.<tag>`, which rpm
/// orders above 2.4.0 and below 2.4.1, so `< 2.4.1` catches every such build.
const OLD_SPLIT_BEFORE: &str = "2.4.1";
/// The `.rpm` Release of every package, which a versioned Provides spells too.
const RPM_RELEASE: &str = "1";
// Relative to the package root.
const LIB_DIR: &str = "usr/lib/private-ai"; // llama.cpp's libraries, the ggml backends
const RT_DIR: &str = "usr/lib/private-ai/lib"; // the bundled GPU runtimes
const LIBEXEC_DIR: &str = "usr/libexec/private-ai"; // pai-manager, pai-bff
/// The RUNPATH of every staged llama.cpp ELF file: [`LIB_DIR`], [`RT_DIR`] and
/// libomp's directory in it, by absolute path.
const RUNPATH: &str =
    "/usr/lib/private-ai:/usr/lib/private-ai/lib:/usr/lib/private-ai/lib/llvm/lib";

/// Where a file the build produces is installed, relative to the package
/// root: an executable of the build's `bin/` (or `pai-config`, `pai`) in
/// `usr/bin`, a library or ggml backend in the private `usr/lib/private-ai`,
/// `pai-manager` and `pai-bff` (from cargo's target dir) in
/// `usr/libexec/private-ai`.
fn place(name: &str) -> String {
    if name == "pai-manager" || name == "pai-bff" {
        format!("{LIBEXEC_DIR}/{name}")
    } else if name.starts_with("lib") && name.contains(".so") {
        format!("{LIB_DIR}/{name}")
    } else {
        format!("usr/bin/{name}")
    }
}

/// The RUNPATH every staged llama.cpp ELF file carries.
fn runpath() -> &'static str {
    RUNPATH
}

/// A read-only data directory (`benchmarks`, `strings`,
/// `licenses/<component>`), relative to the package root.
fn share(sub: &str) -> String {
    format!("usr/share/private-ai/{sub}")
}

/// The `.spec` of one package, whose `%files` lists only what `root` holds.
fn rpm_spec(f: &Fields, license: &str, libraries: &str, root: &Path) -> String {
    let package = f.component.package();
    // rpm's automatic dependency scan is kept (it yields soname requirements
    // any rpm distro resolves), minus what it must not see: every library the
    // packages carry (not "provided" to the system, so not "required" from it
    // either) and libcuda, which is the NVIDIA driver's.
    let url = if f.homepage.is_empty() {
        String::new()
    } else {
        format!("URL:            {}\n", f.homepage)
    };
    // Only what this package holds (a %files line without a match stops
    // rpmbuild), and never a directory of the system, which the filesystem
    // package owns: /usr/bin by a glob of its files, the private directories
    // whole.
    let doc = format!("usr/share/doc/{package}");
    let mut files = String::new();
    for (dir, line) in [
        (doc.as_str(), format!("/{doc}")),
        ("usr/bin", "/usr/bin/*".to_string()),
        (LIBEXEC_DIR, format!("/{LIBEXEC_DIR}")),
        (LIB_DIR, format!("/{LIB_DIR}")),
        ("usr/share/private-ai", "/usr/share/private-ai".to_string()),
        (
            "usr/share/applications",
            "/usr/share/applications/org.privateai.Config.desktop".to_string(),
        ),
        (
            "usr/share/icons",
            "/usr/share/icons/hicolor/*/apps/org.privateai.Config.png".to_string(),
        ),
    ] {
        if root.join(dir).is_dir() {
            files.push_str(&line);
            files.push('\n');
        }
    }
    // The system files by name: systemd, dbus and polkit own their directories.
    for (rel, _) in system_files::files() {
        if root.join(rel).is_file() {
            files.push_str(&format!("/{rel}\n"));
        }
    }
    let (epoch, version, release) = rpm_evr(f);
    let epoch = epoch
        .map(|e| format!("Epoch:          {e}\n"))
        .unwrap_or_default();
    format!(
        r#"Name:           {package}
{epoch}Version:        {version}
Release:        {release}
Summary:        {summary}
License:        {license}
{url}{relations}

# The staged files are final: no stripping, no debuginfo, no build-id links,
# no brp post-processing of a tree this phase already prepared.
%global debug_package %{{nil}}
%global __strip /bin/true
%global __os_install_post %{{nil}}
%global _build_id_links none
%global _binary_payload w3T0.zstdio
%global __provides_exclude_from ^/{LIB_DIR}/.*$
# The groups of the sysusers.d m lines (render, video) may be provided by no
# package; the m line creates a missing one anyway.
%global _use_weak_usergroup_deps 1
%global __requires_exclude ^({libraries}|libcuda)\\.so

%description
{description}
%install
cp -a "{root}/." "%{{buildroot}}/"

{scriptlets}%files
%defattr(-,root,root,-)
{files}"#,
        summary = f.summary,
        description = f.description,
        relations = rpm_relations(f),
        root = root.display(),
        scriptlets = system_files::rpm_scriptlets(f.component),
    )
}

/// The ELF files of `root` dpkg-shlibdeps scans for the Depends.
fn shlibdeps_inputs(root: &Path) -> Vec<PathBuf> {
    // libggml-cuda.so is left out of the scan: it links libcuda.so.1, which
    // is the NVIDIA DRIVER's and no Debian package's (dpkg-shlibdeps fails on
    // a library it cannot find), and the package must install on a machine
    // without an NVIDIA card; the CUDA libraries link it too.
    walk(root)
        .into_iter()
        .filter(|p| is_file(p) && (name_of(p).contains(".so") || user_executable(p)))
        .filter(|p| is_elf(p))
        .filter(|p| {
            let n = name_of(p);
            n != "libggml-cuda.so" && !n.starts_with("libcu")
        })
        .collect()
}

fn is_elf(p: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(p) else {
        return false;
    };
    if !m.is_file() {
        return false;
    }
    let mut magic = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == *b"\x7fELF"
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// The entries of a directory, sorted by name (a shell glob's order).
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| !name_of(p).starts_with('.'))
                .collect()
        })
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
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
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
    std::fs::symlink_metadata(p)
        .map(|m| m.is_file())
        .unwrap_or(false)
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
    if !util::is_release_tag(&llama_build) {
        bail!("llama.cpp {llama_build} is not a release (vX.Y.Z): packages are built from release tags only. Run cargo run -p xtask -- build to check out the latest release.");
    }

    // Each format names the architecture its own way.
    let (deb_arch, rpm_arch) = match std::env::consts::ARCH {
        "x86_64" => ("amd64", "x86_64"),
        "aarch64" => ("arm64", "aarch64"),
        a => bail!("unsupported architecture: {a}"),
    };

    // The framework's packages carry the framework's version, the engine its
    // own; the framework accepts any engine from the release it was built
    // against on.
    let engine_version = components::llama_cpp_version(&llama_build);
    let engine_min = components::llama_cpp_min(&llama_build);

    ui::head(format!("Framework version: {fw_version}"));
    ui::head(format!("llama.cpp release: {llama_build}"));
    ui::head(format!("llama.cpp package: {engine_version}"));
    ui::head(format!("Architecture:      {rpm_arch}"));

    // ── Preconditions ───────────────────────────────────────────────────
    let bin = build_dir.join("bin");
    if !util::is_executable(&bin.join("llama-server")) {
        bail!(
            "llama-server not found in {}. Run cargo run -p xtask -- build first.",
            bin.display()
        );
    }
    for exe in ["pai-config", "pai"] {
        if !util::is_executable(&bin.join(exe)) {
            bail!(
                "{exe} not found in {}. Run cargo run -p xtask -- build first.",
                bin.display()
            );
        }
    }
    // A version bump without a rebuild would ship the OLD configurator in a
    // package named after the new version.
    let built = output(cmd(bin.join("pai-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();
    if built != fw_version {
        bail!("the built pai-config is {built}, Cargo.toml says {fw_version}. Run cargo run -p xtask -- build first.");
    }
    // pai-manager and pai-bff, built beside pai-config by the build phase.
    let manager = build_root.join("cargo-target/release/pai-manager");
    if !util::is_executable(&manager) {
        bail!(
            "pai-manager not found at {}. Run cargo run -p xtask -- build first.",
            manager.display()
        );
    }
    let bff = build_root.join("cargo-target/release/pai-bff");
    if !util::is_executable(&bff) {
        bail!(
            "pai-bff not found at {}. Run cargo run -p xtask -- build first.",
            bff.display()
        );
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
    for d in [
        stage.join("usr/bin"),
        stage.join(LIB_DIR),
        stage.join(RT_DIR),
        stage.join(LIBEXEC_DIR),
        pkg_work.clone(),
        out_dir.clone(),
    ] {
        std::fs::create_dir_all(&d)?;
    }

    ui::head("Staging binaries...");
    // Straight from the build's bin/, where the build leaves executables,
    // libraries, backends and pai-config side by side: each goes where
    // `place` says (the executables to /usr/bin, the libraries and backends to
    // /usr/lib/private-ai), the library symlinks kept as symlinks.
    // llama.cpp's own test-* harnesses stay out: they are its unit tests, and
    // every executable staged here is a command in /usr/bin.
    let (tests, keep): (Vec<PathBuf>, Vec<PathBuf>) = entries(&bin)
        .into_iter()
        .partition(|p| name_of(p).starts_with("test-"));
    let mut by_dir: std::collections::BTreeMap<PathBuf, Vec<PathBuf>> = Default::default();
    for p in keep {
        let dest = stage.join(place(&name_of(&p)));
        by_dir
            .entry(dest.parent().unwrap().to_path_buf())
            .or_default()
            .push(p);
    }
    for (dir, files) in &by_dir {
        cp("-a", files, dir)?;
    }
    std::fs::copy(&manager, stage.join(place("pai-manager")))
        .with_context(|| format!("copying {}", manager.display()))?;
    std::fs::copy(&bff, stage.join(place("pai-bff")))
        .with_context(|| format!("copying {}", bff.display()))?;
    crate::common::bench_share::stage(&stage.join(share("benchmarks")))?;
    crate::common::strings_share::stage(&stage.join(share("strings")))?;
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
    let canon = |p: &Path| {
        p.canonicalize()
            .map(|c| c.display().to_string())
            .unwrap_or_default()
    };
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
        let Some(ldd) = output(cmd("ldd").arg(&f)) else {
            continue;
        };
        for line in ldd.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 3 && cols[1] == "=>" && cols[2].starts_with('/') {
                closure.insert((cols[0].to_string(), cols[2].to_string()));
            }
        }
    }
    let under = |real: &str, dir: &Option<String>| {
        dir.as_ref()
            .map(|d| real.starts_with(&format!("{d}/")))
            .unwrap_or(false)
    };
    let rt = stage.join(RT_DIR);
    let mut bundled = 0;
    for (soname, path) in &closure {
        let real = canon(Path::new(path));
        let hip_lib = format!("{hip_root}/lib/");
        let (dest, from) = if !hip_root.is_empty() && path.starts_with(&hip_lib) {
            let rel = Path::new(&path[hip_lib.len()..]);
            (
                rt.join(rel.parent().unwrap_or(Path::new(""))).join(soname),
                Component::Rocm,
            )
        } else if under(&real, &cuda_dir) || under(&real, &nccl_dir) {
            (rt.join(soname), Component::Cuda)
        } else if under(&real, &vulkan_lib_dir) {
            (rt.join(soname), Component::Vulkan)
        } else {
            continue;
        };
        // components::of goes by name; a library it would hand to another SDK's
        // package has to be taught to it, not shipped there. libomp comes out of
        // the ROCm dist and belongs to llama.cpp on purpose.
        let owner = components::of(&dest.strip_prefix(&stage)?.to_string_lossy());
        if owner != from && owner != Component::Llama {
            bail!(
                "{soname} comes from the {} dist but common/components.rs gives it to {}",
                from.suffix(),
                owner.suffix()
            );
        }
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
    for (backend, runtime) in [
        ("libggml-hip.so", "libhipblas.so."),
        ("libggml-cuda.so", "libcublas.so."),
    ] {
        if stage.join(place(backend)).exists() && !rt_names.iter().any(|n| n.starts_with(runtime)) {
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
    // which is <lib>/../.kpack here as there (/usr/lib/private-ai/.kpack
    // beside the runtimes' lib/): hipBLASLt's (blas_lib) and
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
            let picked: Vec<PathBuf> = entries(&src)
                .into_iter()
                .filter(|p| wanted(&name_of(p)))
                .collect();
            cp("-rL", &picked, &dest)?;
            ui::dim(format!("{sub}: {} MiB", util::du("-sm", &dest)));
        }
        let kpack = hip.join(".kpack");
        if kpack.is_dir() {
            let dest = stage.join(LIB_DIR).join(".kpack");
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
    // dists under /mnt/data; a packaged file must find the other llama.cpp
    // libraries and the bundled runtimes. The executables in /usr/bin are not
    // beside them, so $ORIGIN cannot reach them: every llama.cpp ELF gets
    // RUNPATH (/usr/lib/private-ai, its lib/ with the runtimes and lib/llvm/lib
    // with libomp, by absolute path). The bundled runtime libraries are left
    // exactly as their vendor built them, and pai-manager and pai-bff link
    // nothing of llama.cpp and get none. LD_LIBRARY_PATH still wins over RUNPATH.
    ui::head("Stripping and setting RUNPATH...");
    let staged = [stage.join("usr/bin"), stage.join(LIB_DIR)];
    // llama.cpp's own files: the runtimes in lib/ and .kpack are not stripped.
    let size = || {
        output(
            cmd("du")
                .args(["-scm", "--exclude=lib", "--exclude=.kpack"])
                .args(&staged),
        )
        .and_then(|s| {
            s.lines()
                .last()?
                .split_whitespace()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| "?".into())
    };
    let before = size();
    let mut executables = 0;
    for f in staged.iter().flat_map(|d| entries(d)) {
        if !is_elf(&f) {
            continue;
        }
        // strip BEFORE patchelf: stripping a patched file can drop the
        // section patchelf appended. --strip-unneeded keeps the dynamic
        // symbols, which is all a shared library needs.
        run_cmd(cmd("strip").arg("--strip-unneeded").arg(&f))?;
        run_cmd(cmd("patchelf").args(["--set-rpath", runpath()]).arg(&f))?;
        set_mode(&f, 0o755)?;
        if f.parent() == Some(stage.join("usr/bin").as_path()) {
            executables += 1;
        }
    }
    for name in ["pai-manager", "pai-bff"] {
        let staged = stage.join(place(name));
        run_cmd(cmd("strip").arg("--strip-unneeded").arg(&staged))?;
        set_mode(&staged, 0o755)?;
    }
    let after = size();
    ui::dim(format!(
        "{executables} executables, {before} MiB -> {after} MiB stripped"
    ));
    ui::dim(format!("Bundled runtimes: {} MiB", util::du("-sm", &rt)));

    // ── Desktop entry, icon ─────────────────────────────────────────────
    let apps = stage.join("usr/share/applications");
    std::fs::create_dir_all(&apps)?;
    std::fs::write(apps.join("org.privateai.Config.desktop"), desktop_entry())?;

    let icon = stage_icon(&repo.join("resources/llama.ico"), &stage)?;
    if icon.is_none() {
        ui::warn("No PNG frame in resources/llama.ico: the desktop entry ships without an icon");
    }

    // ── The services' system files ──────────────────────────────────────
    system_files::stage(&stage)?;

    // ── One tree per package ────────────────────────────────────────────
    let roots = pkg_work.join("roots");
    let present = components::split(&stage, &roots)?;

    // A runtime's version is its SDK's, so a new llama.cpp alone leaves it as it
    // is. NCCL has a version of its own and travels in the CUDA package.
    let sdk_version = |dist: Option<PathBuf>| {
        dist.as_deref()
            .and_then(components::dist_version)
            .unwrap_or_else(|| fw_version.clone())
    };
    let cuda_version = match dists
        .find_nccl()
        .as_deref()
        .and_then(components::dist_version)
    {
        Some(nccl) => format!("{}+nccl.{nccl}", sdk_version(dists.find_cuda())),
        None => sdk_version(dists.find_cuda()),
    };
    let version_of = |c: Component| match c {
        Component::Config | Component::Cli | Component::Bff | Component::Core => fw_version.clone(),
        Component::Llama => engine_version.clone(),
        Component::Rocm => sdk_version(Some(PathBuf::from(&hip_root))),
        Component::Cuda => cuda_version.clone(),
        Component::Vulkan => sdk_version(dists.find_vulkan()),
    };
    let runtimes: Vec<(Component, String)> = present
        .iter()
        .filter(|c| c.is_runtime())
        .map(|c| (*c, version_of(*c)))
        .collect();

    // ── Licences ────────────────────────────────────────────────────────
    let mut licence_fields = Vec::new();
    for c in &present {
        let root = components::root(&roots, *c);
        let dir = root.join(share(&format!("licenses/{}", c.suffix())));
        let entries = licenses::stage_in(
            &dir,
            &licenses::libraries_under(&root.join(LIB_DIR)),
            &licenses::Sources {
                llama_cpp: llama.clone(),
                rocm: (!hip_root.is_empty()).then(|| PathBuf::from(&hip_root)),
                cuda: dists.find_cuda(),
                ..Default::default()
            },
        )?;
        licence_fields.push((*c, licenses::rpm_license(&entries)));

        let doc = root.join("usr/share/doc").join(c.package());
        std::fs::create_dir_all(&doc)?;
        let licences_dir = share(&format!("licenses/{}", c.suffix()));
        if *c == Component::Llama {
            // The engine is llama.cpp, under its own licence.
            let mut copyright = format!(
                "{} {} is llama.cpp {llama_build}, distributed under its MIT licence:\n\n",
                c.package(),
                version_of(*c),
            );
            copyright.push_str(&std::fs::read_to_string(llama.join("LICENSE"))?);
            copyright.push_str(&format!(
                "\nThe libraries under /{RT_DIR} are redistributed unmodified, each under its own licence.\n\
                 The full texts are in /{licences_dir}:\n\n"
            ));
            copyright.push_str(&std::fs::read_to_string(dir.join("README.txt"))?);
            std::fs::write(doc.join("copyright"), copyright)?;
        } else if c.is_runtime() {
            let readme = std::fs::read_to_string(dir.join("README.txt"))?;
            std::fs::write(
                doc.join("copyright"),
                runtime_copyright(*c, &version_of(*c), &readme),
            )?;
        } else {
            let mut copyright = format!(
                "{} {} is part of {} {fw_version}, distributed under the MIT licence:\n\n",
                c.package(),
                version_of(*c),
                components::BASE_NAME
            );
            copyright.push_str(&std::fs::read_to_string(repo.join("LICENSE"))?);
            copyright.push_str(&format!(
                "\nIt was built with llama.cpp {llama_build}, whose licence follows.\n"
            ));
            copyright.push_str(&format!(
                "The libraries under /{RT_DIR} are redistributed unmodified, each under its own licence.\n"
            ));
            copyright.push_str(&format!("The full texts are in /{licences_dir}:\n\n"));
            copyright.push_str(&std::fs::read_to_string(dir.join("README.txt"))?);
            copyright.push_str("\nllama.cpp's licence:\n\n");
            copyright.push_str(&std::fs::read_to_string(llama.join("LICENSE"))?);
            copyright.push_str(
                "\npai-config embeds the Fluent UI System Icons font, whose licence follows.\n\n",
            );
            copyright.push_str(&std::fs::read_to_string(
                repo.join("crates/config/ui/fonts/LICENSE"),
            )?);
            std::fs::write(doc.join("copyright"), copyright)?;
        }

        // The repo may sit on a filesystem that hands out group-writable modes; a
        // package carries 755 / 644 whatever the build drive says.
        for p in walk(&root) {
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if m.is_dir() {
                set_mode(&p, 0o755)?;
            }
        }
        for dir in [root.join(RT_DIR), root.join(LIB_DIR).join(".kpack")] {
            for p in walk(&dir).into_iter().filter(|p| is_file(p)) {
                set_mode(
                    &p,
                    if name_of(&p).contains(".so") {
                        0o755
                    } else {
                        0o644
                    },
                )?;
            }
        }
        for p in walk(&root.join("usr/share"))
            .into_iter()
            .filter(|p| is_file(p))
        {
            set_mode(&p, 0o644)?;
        }
    }

    // The maintainer scripts (the core's and the bff's only) are written by
    // build_deb and rpm_spec.

    let repo_git =
        |args: &[&str]| output(cmd("git").arg("-C").arg(&repo).args(args)).unwrap_or_default();
    let repo_url = {
        let url = repo_git(&["remote", "get-url", "origin"]);
        url.strip_suffix(".git").map(str::to_string).unwrap_or(url)
    };
    let maintainer = match std::env::var("PRIVATE_AI_MAINTAINER") {
        Ok(m) if !m.is_empty() => m,
        _ => {
            let m = format!(
                "{} <{}>",
                repo_git(&["config", "user.name"]),
                repo_git(&["config", "user.email"])
            );
            if m == " <>" {
                format!("{} <nobody@localhost>", components::BASE_NAME)
            } else {
                m
            }
        }
    };

    // The libraries of all the packages, which no package provides to the
    // system or requires from it.
    let library_dirs = private_library_dirs(&roots, &present);
    let libraries = library_stems(&roots, &present);

    let mut created = Vec::new();
    for c in &present {
        let root = components::root(&roots, *c);
        let version = version_of(*c);
        let (summary, description) = texts(*c, &llama_build);
        let fields = Fields {
            component: *c,
            version: &version,
            summary,
            description: &description,
            homepage: homepage(*c, &repo_url),
            depends_on_llama: matches!(*c, Component::Config | Component::Cli | Component::Core)
                .then_some(engine_min.as_str()),
            runtimes: if *c == Component::Llama {
                &runtimes
            } else {
                &[]
            },
        };
        let stem = components::file_stem(*c, &fw_version, &llama_build, &version);
        if want_deb {
            let file = out_dir.join(format!("{stem}-{deb_arch}.deb"));
            build_deb(
                &root,
                &pkg_work,
                &file,
                &fields,
                deb_arch,
                &maintainer,
                &library_dirs,
            )?;
            created.push(file);
        }
        if want_rpm {
            let file = out_dir.join(format!("{stem}-{rpm_arch}.rpm"));
            let license = &licence_fields
                .iter()
                .find(|(l, _)| l == c)
                .expect("every component has its licences")
                .1;
            build_rpm(
                &root, &pkg_work, &out_dir, &file, &fields, rpm_arch, license, &libraries,
            )?;
            created.push(file);
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────────────
    util::remove_all(&pkg_work)?;
    if !keep_staging {
        util::remove_all(&stage)?;
    }

    ui::say("");
    for file in &created {
        ui::ok(format!(
            "Package created: {} ({} MiB)",
            file.display(),
            util::du("-m", file)
        ));
    }
    ui::say("");
    Ok(())
}

/// llama.cpp's upstream home, the engine's and its runtimes' Homepage (deb)
/// and URL (rpm).
const LLAMA_CPP_URL: &str = "https://github.com/ggml-org/llama.cpp";

/// The Homepage of package `c`: the framework's repository (`framework`)
/// for the framework's packages, llama.cpp's upstream for the engine and
/// its runtimes.
fn homepage(c: Component, framework: &str) -> &str {
    if c == Component::Llama || c.is_runtime() {
        LLAMA_CPP_URL
    } else {
        framework
    }
}

/// The `/usr/share/doc/<package>/copyright` of a runtime package: what it
/// is (the SDK's libraries the llama.cpp engine loads, at the SDK's
/// `version`), where the licence texts are, and `readme`, the licence
/// directory's README.txt.
fn runtime_copyright(c: Component, version: &str, readme: &str) -> String {
    let sdk = match c {
        Component::Rocm => "ROCm",
        Component::Cuda if version.contains("+nccl.") => "CUDA and NCCL",
        Component::Cuda => "CUDA",
        _ => "Vulkan SDK",
    };
    format!(
        "{} {version} holds the {sdk} libraries the llama.cpp engine loads,\n\
         redistributed unmodified, each under its own licence.\n\
         The full texts are in /{}:\n\n{readme}",
        c.package(),
        share(&format!("licenses/{}", c.suffix())),
    )
}

/// The short and the long description of a package.
fn texts(c: Component, llama_build: &str) -> (&'static str, String) {
    match c {
        Component::Config => (
            "pai-config: graphical configurator for llama-server",
            "pai-config, the graphical configurator for server.ini and the per-model\n\
             presets.ini, which the manager keeps in /etc/private-ai.\n"
                .to_string(),
        ),
        Component::Cli => (
            "pai: command line for llama-server, its presets and benchmarks",
            "pai, the command line of the framework: server.ini and the per-model presets.ini\n\
             through the manager, the server's control and the benchmarks, with the\n\
             benchmark method. It needs no graphics library.\n"
                .to_string(),
        ),
        Component::Llama => (
            "llama.cpp with its CPU, CUDA, Vulkan and ROCm backends",
            format!(
                "llama.cpp {llama_build} with every ggml backend. A GPU backend also needs its\n\
                 runtime package and the GPU driver: without either, llama-server skips it at\n\
                 start.\n"
            ),
        ),
        Component::Core => (
            "pai-manager: the service that runs llama-server for the framework",
            "pai-manager, the framework's service: it keeps server.ini and presets.ini in\n\
             /etc/private-ai and runs llama-server as a systemd service, for pai-config,\n\
             pai and pai-bff. With the systemd units, the D-Bus and polkit policies, the\n\
             service user and the texts the framework's programs print.\n"
                .to_string(),
        ),
        Component::Rocm => (
            "ROCm/HIP runtime and kernels for llama.cpp",
            "The ROCm runtime libraries and kernel files the HIP backend of llama.cpp loads.\n\
             Only the amdgpu driver has to be installed besides it.\n"
                .to_string(),
        ),
        Component::Cuda => (
            "CUDA runtime libraries for llama.cpp",
            "The CUDA and NCCL libraries the CUDA backend of llama.cpp loads. Only the NVIDIA\n\
             driver has to be installed besides it.\n"
                .to_string(),
        ),
        Component::Bff => (
            "pai-bff: the web service of the framework",
            "pai-bff, the web service of the framework: the web UI and the API it calls,\n\
             on this machine only. A systemd service, started at boot.\n"
                .to_string(),
        ),
        Component::Vulkan => (
            "Vulkan loader for llama.cpp",
            "The Vulkan loader the Vulkan backend of llama.cpp loads. Only a Vulkan driver has\n\
             to be installed besides it.\n"
                .to_string(),
        ),
    }
}

/// `-l<dir>` for every directory holding a library of the packages, so
/// dpkg-shlibdeps resolves a library another package carries.
fn private_library_dirs(roots: &Path, present: &[Component]) -> BTreeSet<String> {
    present
        .iter()
        .flat_map(|c| walk(&components::root(roots, *c).join(LIB_DIR)))
        .filter(|p| is_file(p) && name_of(p).contains(".so"))
        .filter_map(|p| p.parent().map(|d| format!("-l{}", d.display())))
        .collect()
}

/// The names of every library of the packages, as a regular expression's alternatives.
fn library_stems(roots: &Path, present: &[Component]) -> String {
    let stems: BTreeSet<String> = present
        .iter()
        .flat_map(|c| walk(&components::root(roots, *c).join(LIB_DIR)))
        .map(|p| name_of(&p))
        .filter(|n| n.starts_with("lib") && n.contains(".so"))
        .map(|n| {
            let stem = &n[..n.find(".so").unwrap()];
            stem.chars()
                .map(|c| {
                    if c == '+' || c == '.' {
                        format!("\\\\{c}")
                    } else {
                        c.to_string()
                    }
                })
                .collect()
        })
        .collect();
    stems.into_iter().collect::<Vec<_>>().join("|")
}

/// What one package says about itself and the others, in both formats.
struct Fields<'a> {
    component: Component,
    version: &'a str,
    summary: &'a str,
    description: &'a str,
    homepage: &'a str,
    /// The lowest llama.cpp package version this one requires (`1:1.0.0`),
    /// None when it does not run the engine.
    depends_on_llama: Option<&'a str>,
    /// The runtimes llama.cpp was built against, with their versions.
    runtimes: &'a [(Component, String)],
}

/// The `.deb` relations beyond the scanned Depends: (Recommends, Breaks, Conflicts).
fn deb_relations(f: &Fields) -> (String, String, String) {
    let mut recommends: Vec<String> = Vec::new();
    let mut breaks: Vec<String> = Vec::new();
    for (c, v) in f.runtimes {
        recommends.push(format!("{} (= {v})", c.package()));
        breaks.push(format!("{} (<< {v})", c.package()));
        breaks.push(format!("{} (>> {v})", c.package()));
    }
    // What pai-config loads at run time by dlopen, which no link-time
    // scan sees.
    if f.component == Component::Config {
        recommends.push("libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libxcursor1, libxi6, libegl1".into());
    }
    let mut conflicts = Vec::new();
    if let Some(old) = old_package(f.component) {
        conflicts = vec![MONOLITH.to_string(), old];
    }
    // The distributions' packages that carry the same commands in /usr/bin.
    if f.component == Component::Llama {
        conflicts.push("llama.cpp-tools".into());
    }
    (
        recommends.join(", "),
        breaks.join(", "),
        conflicts.join(", "),
    )
}

/// The `.deb` control file of one package: its identity, the scanned
/// `depends`, [`deb_relations`], and the takeover of the packages it
/// replaces (`Replaces`, `Provides`).
fn deb_control(
    f: &Fields,
    arch: &str,
    maintainer: &str,
    installed_size: &str,
    depends: &str,
) -> String {
    let line = |tag: &str, value: &str| {
        if value.is_empty() {
            String::new()
        } else {
            format!("{tag}: {value}\n")
        }
    };
    // Debian folds the long description one space in, a blank line as " .".
    let body: String = f
        .description
        .lines()
        .map(|l| {
            if l.is_empty() {
                " .\n".to_string()
            } else {
                format!(" {l}\n")
            }
        })
        .collect();
    let (recommends, breaks, conflicts) = deb_relations(f);
    let takeover = match old_package(f.component) {
        Some(old) => format!(
            "Replaces: {MONOLITH}, {old}\nProvides: {old} (= {})\n",
            f.version
        ),
        None => String::new(),
    };
    format!(
        "Package: {}\nVersion: {}\nArchitecture: {arch}\nMaintainer: {maintainer}\nInstalled-Size: {installed_size}\n{}{}{}{}{takeover}Section: utils\nPriority: optional\n{}Description: {}\n{body}",
        f.component.package(),
        f.version,
        line("Depends", depends),
        line("Recommends", &recommends),
        line("Breaks", &breaks),
        line("Conflicts", &conflicts),
        line("Homepage", f.homepage),
        f.summary,
    )
}

/// The 2.3 and 2.4 package of a component, which this one takes over; None
/// for the command line, the bff and the core, which those releases did not
/// have.
fn old_package(c: Component) -> Option<String> {
    (!matches!(c, Component::Cli | Component::Bff | Component::Core))
        .then(|| format!("{MONOLITH}-{}", c.suffix()))
}

/// A package's version as rpm spells it: (Epoch, Version, Release). The
/// engine's `1:1.0.0+pai1` is epoch 1, version 1.0.0 and our build revision
/// `pai1` as the release; every other package has no epoch and Release
/// [`RPM_RELEASE`].
fn rpm_evr<'a>(f: &Fields<'a>) -> (Option<&'a str>, &'a str, &'a str) {
    if f.component != Component::Llama {
        return (None, f.version, RPM_RELEASE);
    }
    let (epoch, rest) = match f.version.split_once(':') {
        Some((e, rest)) => (Some(e), rest),
        None => (None, f.version),
    };
    match rest.split_once('+') {
        Some((version, release)) => (epoch, version, release),
        None => (epoch, rest, RPM_RELEASE),
    }
}

/// The `.rpm` relations, one tag per line.
fn rpm_relations(f: &Fields) -> String {
    let mut lines = Vec::new();
    // The old package of this component: a runtime is versioned by its SDK and
    // built locally, so no last version under the old name is known; its own
    // version, inclusive, takes the old package of the same SDK or an older one.
    if let Some(old) = old_package(f.component) {
        lines.push(format!("Obsoletes:      {MONOLITH} <= {MONOLITH_LAST}"));
        if f.component.is_runtime() {
            lines.push(format!("Obsoletes:      {old} <= {}", f.version));
        } else {
            lines.push(format!("Obsoletes:      {old} < {OLD_SPLIT_BEFORE}"));
        }
        let (epoch, version, release) = rpm_evr(f);
        let epoch = epoch.map(|e| format!("{e}:")).unwrap_or_default();
        lines.push(format!(
            "Provides:       {old} = {epoch}{version}-{release}"
        ));
    }
    for c in exact_requires(f) {
        lines.push(format!("Requires:       {} = {}", c.package(), f.version));
    }
    if let Some(min) = f.depends_on_llama {
        lines.push(format!(
            "Requires:       {} >= {min}",
            Component::Llama.package()
        ));
    }
    for (c, v) in f.runtimes {
        lines.push(format!("Recommends:     {} = {v}", c.package()));
        lines.push(format!("Conflicts:      {} < {v}", c.package()));
        lines.push(format!("Conflicts:      {} > {v}", c.package()));
    }
    if f.component == Component::Llama {
        lines.push("Conflicts:      llama-cpp".into());
    }
    if matches!(f.component, Component::Core | Component::Bff) {
        // What the scriptlets run (Fedora's %systemd_requires, plus %posttrans).
        for tag in ["post", "preun", "postun", "posttrans"] {
            lines.push(format!("Requires({tag}): systemd"));
        }
    }
    lines.join("\n")
}

/// The configurator's desktop entry, installed as
/// `usr/share/applications/<app id>.desktop`: the display name, the
/// executable, and the app id as the icon and the window class.
fn desktop_entry() -> String {
    "[Desktop Entry]\n\
     Type=Application\n\
     Name=Private AI Framework\n\
     Comment=Configure and run llama-server and its model presets\n\
     Exec=pai-config\n\
     Icon=org.privateai.Config\n\
     Terminal=false\n\
     Categories=Utility;\n\
     StartupWMClass=org.privateai.Config\n"
        .to_string()
}

/// The icon is the largest PNG frame of resources/llama.ico (the same image
/// the Windows build uses), cut out of the container: an .ico is a directory
/// of 16-byte entries (size at +8, offset at +12, little endian) after a
/// 6-byte header, followed by the frames, and its large frames are plain PNG
/// files. Returns the staged PNG, or None when there is none.
fn stage_icon(ico: &Path, stage: &Path) -> Result<Option<PathBuf>> {
    let Ok(data) = std::fs::read(ico) else {
        return Ok(None);
    };
    let u16le = |o: usize| {
        data.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32le = |o: usize| {
        data.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let count = u16le(4).unwrap_or(0);
    let (mut best_size, mut best_off) = (0, 0);
    for i in 0..count {
        let (Some(size), Some(off)) = (u32le(6 + i * 16 + 8), u32le(6 + i * 16 + 12)) else {
            continue;
        };
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
    let png = stage.join(format!(
        "usr/share/icons/hicolor/{px}x{px}/apps/org.privateai.Config.png"
    ));
    std::fs::create_dir_all(png.parent().unwrap())?;
    let end = (best_off + best_size).min(data.len());
    std::fs::write(&png, &data[best_off..end])?;
    ui::dim(format!("Icon: {px}x{px} from resources/llama.ico"));
    Ok(Some(png))
}

fn build_deb(
    root: &Path,
    pkg_work: &Path,
    deb_file: &Path,
    f: &Fields,
    arch: &str,
    maintainer: &str,
    library_dirs: &BTreeSet<String>,
) -> Result<()> {
    let package = f.component.package();
    ui::head(format!("Building the .deb of {package}..."));
    // Depends, computed from what the staged ELF files actually link (minus
    // libvulkan1, which the scan names although the loader is bundled), the
    // bundled runtimes included (their own directories are given as private
    // library paths).
    let scan: Vec<String> = shlibdeps_inputs(root)
        .iter()
        .map(|p| format!("-e{}", p.display()))
        .collect();
    let mut scanned = String::new();
    if which("dpkg-shlibdeps").is_some() && !scan.is_empty() {
        let work = pkg_work.join("shlibdeps");
        std::fs::create_dir_all(work.join("debian"))?;
        std::fs::write(
            work.join("debian/control"),
            format!("Source: {package}\n\nPackage: {package}\nArchitecture: any\n"),
        )?;
        if let Some(out) = output(
            cmd("dpkg-shlibdeps")
                .current_dir(&work)
                .args(["-O", "--ignore-missing-info"])
                .args(library_dirs)
                .args(&scan),
        ) {
            if let Some(line) = out.lines().find_map(|l| l.strip_prefix("shlibs:Depends=")) {
                scanned = drop_libvulkan(line);
            }
        }
    }
    if scanned.is_empty() && !f.component.is_runtime() {
        ui::warn("dpkg-shlibdeps gave no answer; using the fixed Depends list");
    }
    let depends = depends_line(f, &scanned);

    std::fs::create_dir_all(root.join("DEBIAN"))?;
    let installed_size = output(cmd("du").args(["-sk", "--exclude=DEBIAN"]).arg(root))
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .unwrap_or_default();
    std::fs::write(
        root.join("DEBIAN/control"),
        deb_control(f, arch, maintainer, &installed_size, &depends),
    )?;
    system_files::write_deb_scripts(root, f.component)?;
    // zstd: the tree is several GiB of GPU kernels and xz would take many
    // minutes.
    let status = cmd("dpkg-deb")
        .args(["--root-owner-group", "-Zzstd", "--build"])
        .arg(root)
        .arg(deb_file)
        .stdout(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        bail!("dpkg-deb failed ({status})");
    }
    util::remove_all(&root.join("DEBIAN"))
}

/// The whole `.deb` Depends of a package from what dpkg-shlibdeps found
/// (`scanned`, empty when it gave no answer): the scan or a fixed list, the
/// framework packages it requires exactly, llama.cpp at its lowest version,
/// what its maintainer scripts run.
fn depends_line(f: &Fields, scanned: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !scanned.is_empty() {
        parts.push(scanned.to_string());
    } else if !f.component.is_runtime() {
        // A runtime asks for nothing it does not bring along; the packages that
        // hold our programs need the system's C and C++ libraries at the least,
        // and the GUI its font libraries.
        parts.push("libc6, libstdc++6, libgcc-s1, libssl3t64 | libssl3".into());
        if f.component == Component::Config {
            parts.push("libfontconfig1, libfreetype6".into());
        }
    }
    for c in exact_requires(f) {
        parts.push(format!("{} (= {})", c.package(), f.version));
    }
    if let Some(min) = f.depends_on_llama {
        parts.push(format!("{} (>= {min})", Component::Llama.package()));
    }
    let scripts = system_files::deb_depends(f.component);
    if !scripts.is_empty() {
        parts.push(scripts.to_string());
    }
    parts.join(", ")
}

/// The framework packages this one requires at exactly its own version: the
/// core's, and for the configurator also the command line's, which holds the
/// data the GUI reads.
fn exact_requires(f: &Fields) -> Vec<Component> {
    match f.component {
        Component::Config => vec![Component::Cli, Component::Core],
        Component::Cli | Component::Bff => vec![Component::Core],
        _ => Vec::new(),
    }
}

/// `-perm -u+x`
fn user_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::symlink_metadata(p)
        .map(|m| m.permissions().mode() & 0o100 != 0)
        .unwrap_or(false)
}

/// Drops the first libvulkan1 entry of a Depends list (the loader is
/// bundled).
fn drop_libvulkan(depends: &str) -> String {
    let parts: Vec<&str> = depends.split(", ").collect();
    match parts.iter().position(|p| p.starts_with("libvulkan1")) {
        Some(i) => parts
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, p)| *p)
            .collect::<Vec<_>>()
            .join(", "),
        None => depends.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_rpm(
    root: &Path,
    pkg_work: &Path,
    out_dir: &Path,
    rpm_file: &Path,
    f: &Fields,
    arch: &str,
    license: &str,
    libraries: &str,
) -> Result<()> {
    let package = f.component.package();
    ui::head(format!("Building the .rpm of {package}..."));
    let spec = pkg_work.join(format!("{package}.spec"));
    std::fs::write(&spec, rpm_spec(f, license, libraries, root))?;
    let file_name = rpm_file.file_name().unwrap().to_string_lossy().into_owned();
    run_cmd(
        cmd("rpmbuild")
            .args(["-bb", "--quiet", "--target", arch])
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `depends_on_llama`: the lowest llama.cpp package version this one
    /// requires (`1:1.0.0`), None when it does not run the engine.
    fn fields<'a>(
        component: Component,
        runtimes: &'a [(Component, String)],
        depends_on_llama: Option<&'a str>,
    ) -> Fields<'a> {
        Fields {
            component,
            version: "1",
            summary: "",
            description: "",
            homepage: "",
            depends_on_llama,
            runtimes,
        }
    }

    /// The desktop entry names the product by its display name, runs
    /// `pai-config`, and carries the app id the window sets (the Wayland app
    /// id and X11 class), so the dock matches the window to the entry.
    #[test]
    fn the_desktop_entry_names_the_product_and_the_app_id() {
        let text = desktop_entry();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "[Desktop Entry]");
        for line in [
            "Type=Application",
            "Name=Private AI Framework",
            "Exec=pai-config",
            "Icon=org.privateai.Config",
            "StartupWMClass=org.privateai.Config",
        ] {
            assert!(lines.contains(&line), "{line} missing:\n{text}");
        }
        assert!(!text.contains("llama-cpp"), "{text}");
    }

    /// The icon is installed under the app id, the name the desktop entry's
    /// `Icon=` gives.
    #[test]
    fn the_icon_is_staged_under_the_app_id() {
        let dir = std::env::temp_dir().join(format!("xtask-icon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A PNG whose IHDR says 256x256, in a one-frame ICO.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(256u32.to_be_bytes());
        png.extend(256u32.to_be_bytes());
        png.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        let mut ico = vec![0, 0, 1, 0, 1, 0];
        ico.extend([0, 0, 0, 0, 1, 0, 32, 0]);
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend(22u32.to_le_bytes());
        ico.extend(&png);
        let ico_path = dir.join("llama.ico");
        std::fs::write(&ico_path, &ico).unwrap();
        let stage = dir.join("stage");

        let staged = stage_icon(&ico_path, &stage).unwrap();
        let want = stage.join("usr/share/icons/hicolor/256x256/apps/org.privateai.Config.png");
        assert_eq!(staged.as_deref(), Some(want.as_path()));
        assert!(want.is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The packages of the 2.4 releases and earlier, spelled split so the
    /// names lint stays clean: the monolith of 2.2 and earlier, and the
    /// split packages `<OLD>-<component>` of 2.3 and 2.4.
    const OLD: &str = concat!("llama-cpp", "-framework");

    /// The engine's version and the lowest one the framework accepts, as
    /// `components::llama_cpp_version` and `llama_cpp_min` give them for
    /// llama.cpp v1.0.0.
    const ENGINE: &str = "1:1.0.0+pai1";
    const ENGINE_MIN: &str = "1:1.0.0";

    fn fields_v<'a>(
        component: Component,
        version: &'a str,
        runtimes: &'a [(Component, String)],
        depends_on_llama: Option<&'a str>,
    ) -> Fields<'a> {
        Fields {
            version,
            ..fields(component, runtimes, depends_on_llama)
        }
    }

    /// The engine recommends its runtimes at the SDK versions it was built
    /// against and refuses any other (a backend that cannot resolve a symbol
    /// is skipped at start with nothing but a log line). It takes over 2.4's
    /// llama.cpp package, and refuses to sit beside the distributions'
    /// packages that carry the same commands in /usr/bin: Ubuntu's
    /// `llama.cpp-tools` (llama-server, llama-bench, llama-cli...; 26.04's
    /// `8681+dfsg-1`, read with `dpkg -c`; its `-extra` sibling requires it,
    /// and Ubuntu's `llama.cpp` itself is only a metapackage our package
    /// supersedes by name and epoch) and Fedora's `llama-cpp` (its spec,
    /// src.fedoraproject.org/rpms/llama-cpp: `%{_bindir}/llama-server`...;
    /// `llama-cpp-examples` requires it). It no longer conflicts with
    /// `llama.cpp`, its own name. It names nothing of the framework.
    #[test]
    fn llama_cpp_recommends_its_runtimes_and_refuses_any_other_version() {
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = fields_v(Component::Llama, ENGINE, &runtimes, None);
        let (recommends, breaks, conflicts) = deb_relations(&f);
        assert_eq!(recommends, "llama.cpp-rocm (= 10.1.0)");
        assert_eq!(
            breaks,
            "llama.cpp-rocm (<< 10.1.0), llama.cpp-rocm (>> 10.1.0)"
        );
        assert_eq!(conflicts, format!("{OLD}, {OLD}-llama, llama.cpp-tools"));
        let rpm = rpm_relations(&f);
        for want in [
            "Recommends:     llama.cpp-rocm = 10.1.0".to_string(),
            "Conflicts:      llama.cpp-rocm < 10.1.0".to_string(),
            "Conflicts:      llama.cpp-rocm > 10.1.0".to_string(),
            "Conflicts:      llama-cpp".to_string(),
            format!("Obsoletes:      {OLD} <= 2.2.0"),
        ] {
            assert!(rpm.lines().any(|l| l == want), "{want} missing:\n{rpm}");
        }
        assert!(!rpm.contains(&format!("{OLD}-rocm")), "{rpm}");
        for text in [&rpm, &recommends, &breaks, &conflicts] {
            assert!(!text.contains("private-ai-framework"), "{text}");
        }
    }

    /// The configurator requires the command line and the core at its own
    /// version (the framework's packages move together), and llama.cpp at
    /// least at the release it was built against (`1:1.0.0`, any build
    /// revision of it or a later release): a newer llama.cpp installs
    /// without a framework rebuild. rpm's `>= 1:1.0.0` names no release, so
    /// every `1:1.0.0-pai<N>` matches (rpm-version(7)).
    #[test]
    fn the_configurator_requires_cli_and_core_exactly_and_llama_cpp_at_least() {
        let f = fields_v(Component::Config, FW, &[], Some(ENGINE_MIN));
        let rpm = rpm_relations(&f);
        for want in [
            format!("Requires:       private-ai-framework-cli = {FW}"),
            format!("Requires:       private-ai-framework-core = {FW}"),
            format!("Requires:       llama.cpp >= {ENGINE_MIN}"),
        ] {
            assert!(rpm.lines().any(|l| l == want), "{want} missing:\n{rpm}");
        }
        assert!(!rpm.contains("private-ai-framework-llama"), "{rpm}");
        assert!(!rpm.contains("llama.cpp ="), "{rpm}");
        let (recommends, breaks, conflicts) = deb_relations(&f);
        assert!(recommends.contains("libxkbcommon0"), "{recommends}");
        assert!(breaks.is_empty());
        assert_eq!(conflicts, format!("{OLD}, {OLD}-config"));
    }

    #[test]
    fn a_runtime_requires_nothing_of_the_others() {
        let f = fields_v(Component::Cuda, "13.0.2", &[], None);
        assert_eq!(
            deb_relations(&f),
            (String::new(), String::new(), format!("{OLD}, {OLD}-cuda"))
        );
        assert_eq!(
            rpm_relations(&f),
            format!(
                "Obsoletes:      {OLD} <= 2.2.0\n\
                 Obsoletes:      {OLD}-cuda <= 13.0.2\n\
                 Provides:       {OLD}-cuda = 13.0.2-1"
            )
        );
    }

    /// The upgrade from 2.4 on a .deb system: each package replaces the old
    /// package of its component as a rename (Debian Policy 7.6.2: Replaces
    /// with Conflicts forces the old one out; 7.5: a versioned Provides keeps
    /// a versioned dependency on the old name satisfied; 7.4: conflicting
    /// with a name it provides does not stop its own installation).
    #[test]
    fn a_deb_takes_over_the_old_package_of_its_component() {
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let config_depends = format!(
            "libc6, private-ai-framework-cli (= {FW}), private-ai-framework-core (= {FW}), llama.cpp (>= {ENGINE_MIN})"
        );
        for (component, package, version, runtimes, depends) in [
            (
                Component::Config,
                "private-ai-framework-config",
                FW,
                &[][..],
                config_depends.as_str(),
            ),
            (
                Component::Llama,
                "llama.cpp",
                ENGINE,
                &runtimes[..],
                "libc6",
            ),
            (Component::Rocm, "llama.cpp-rocm", "10.1.0", &[][..], ""),
            (
                Component::Vulkan,
                "llama.cpp-vulkan",
                "1.4.328.1",
                &[][..],
                "",
            ),
        ] {
            let f = fields_v(component, version, runtimes, None);
            let old = format!("{OLD}-{}", component.suffix());
            let text = deb_control(&f, "amd64", "Someone <a@b.c>", "1234", depends);
            let lines: Vec<&str> = text.lines().collect();
            let (recommends, breaks, conflicts) = deb_relations(&f);
            let mut want = vec![
                format!("Package: {package}"),
                format!("Version: {version}"),
                "Architecture: amd64".to_string(),
                "Maintainer: Someone <a@b.c>".to_string(),
                "Installed-Size: 1234".to_string(),
                format!("Conflicts: {conflicts}"),
                format!("Replaces: {OLD}, {old}"),
                format!("Provides: {old} (= {version})"),
            ];
            for (tag, value) in [
                ("Depends", depends),
                ("Recommends", recommends.as_str()),
                ("Breaks", breaks.as_str()),
            ] {
                if value.is_empty() {
                    assert!(
                        !lines.iter().any(|l| l.starts_with(&format!("{tag}:"))),
                        "an empty {tag} is left out:\n{text}"
                    );
                } else {
                    want.push(format!("{tag}: {value}"));
                }
            }
            for line in &want {
                assert!(lines.contains(&line.as_str()), "{line} missing:\n{text}");
            }
            assert_eq!(
                lines.iter().filter(|l| l.starts_with("Provides:")).count(),
                1,
                "{text}"
            );
        }
    }

    /// The upgrade from 2.4 on an .rpm system, as Fedora's packaging
    /// guidelines rename a package ("Renaming/Replacing or Removing Existing
    /// Packages"): `Provides: old = %{version}-%{release}` and `Obsoletes:
    /// old < $obsEVR`, a fixed bound just beyond the last release under the
    /// old name. That is 2.4.0 for the configurator and llama.cpp, whose
    /// versions read 2.4.0+llama.cpp.<tag> (rpm orders that above 2.4.0 and
    /// below 2.4.1, so `<= 2.4.0` would miss it). A runtime is versioned by
    /// its SDK and built locally, so no last version is known: its bound is
    /// its own version, inclusive, which takes the old package of the same
    /// SDK and any older one (rpm matches Obsoletes against package names
    /// only, rpm-dependencies(7), so the package does not obsolete its own
    /// Provides).
    #[test]
    fn an_rpm_obsoletes_and_provides_the_old_package_of_its_component() {
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = fields_v(Component::Config, FW, &[], Some(ENGINE_MIN));
        let rpm = rpm_relations(&f);
        for want in [
            format!("Obsoletes:      {OLD} <= 2.2.0"),
            format!("Obsoletes:      {OLD}-config < 2.4.1"),
            format!("Provides:       {OLD}-config = {FW}-1"),
        ] {
            assert!(rpm.lines().any(|l| l == want), "{want} missing:\n{rpm}");
        }
        // The engine's Provides spells its EVR as rpm does: epoch, version,
        // release (`1:1.0.0-pai1`).
        let f = fields_v(Component::Llama, ENGINE, &runtimes, None);
        let rpm = rpm_relations(&f);
        for want in [
            format!("Obsoletes:      {OLD}-llama < 2.4.1"),
            format!("Provides:       {OLD}-llama = 1:1.0.0-pai1"),
            "Conflicts:      llama-cpp".to_string(),
        ] {
            assert!(rpm.lines().any(|l| l == want), "{want} missing:\n{rpm}");
        }
        for (c, v) in [
            (Component::Rocm, "10.1.0"),
            (Component::Vulkan, "1.4.328.1"),
        ] {
            let f = fields_v(c, v, &[], None);
            let rpm = rpm_relations(&f);
            let old = format!("{OLD}-{}", c.suffix());
            for want in [
                format!("Obsoletes:      {old} <= {v}"),
                format!("Provides:       {old} = {v}-1"),
            ] {
                assert!(rpm.lines().any(|l| l == want), "{want} missing:\n{rpm}");
            }
        }
    }

    // ── The standard layout under /usr ──────────────────────────────────

    /// A fresh directory of this test's own under the temp dir.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `bytes` at `root/rel`, the directories made, with `mode`.
    fn put(root: &Path, rel: &str, bytes: &[u8], mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    const ELF: &[u8] = b"\x7fELF\x02\x01\x01\0";

    /// Every executable the build leaves in its `bin/`, and `pai-config`, is a
    /// command in /usr/bin as the real file (the per-executable links into
    /// /opt are gone); the libraries and the ggml backends go to the private
    /// /usr/lib/private-ai, never a system library directory; `pai-manager`
    /// is internal and goes to /usr/libexec/private-ai, which the units call
    /// by full path. Each lands in the package that owns it.
    #[test]
    fn executables_go_to_usr_bin_and_libraries_to_the_private_dir() {
        for exe in ["llama-server", "llama-bench", "llama", "llama-cli"] {
            assert_eq!(place(exe), format!("usr/bin/{exe}"));
            assert_eq!(components::of(&place(exe)), Component::Llama, "{exe}");
        }
        assert_eq!(place("pai-config"), "usr/bin/pai-config");
        assert_eq!(components::of(&place("pai-config")), Component::Config);
        for lib in [
            "libggml-base.so.0",
            "libggml-base.so.0.9.0",
            "libggml.so.0",
            "libggml-cpu-zen4.so",
            "libggml-hip.so",
            "libggml-cuda.so",
            "libggml-vulkan.so",
            "libllama.so.0",
            "libmtmd.so.0",
            "libllama-server-impl.so",
        ] {
            assert_eq!(place(lib), format!("usr/lib/private-ai/{lib}"));
            assert_eq!(components::of(&place(lib)), Component::Llama, "{lib}");
        }
        assert_eq!(place("pai-manager"), "usr/libexec/private-ai/pai-manager");
        assert_eq!(components::of(&place("pai-manager")), Component::Core);
    }

    /// The executables in /usr/bin find libllama, the ggml backends and the
    /// bundled runtimes through their RUNPATH, by absolute path: /usr/bin is
    /// not beside them, so `$ORIGIN` no longer reaches them. The runtimes'
    /// directory (`lib/`, where the ROCm tree keeps its layout and `.kpack`
    /// sits beside it, as `<lib>/../.kpack` asks) and libomp's are named too.
    /// No build path, dist path or /opt is left in it.
    #[test]
    fn the_runpath_names_the_private_library_dirs() {
        assert_eq!(
            runpath(),
            "/usr/lib/private-ai:/usr/lib/private-ai/lib:/usr/lib/private-ai/lib/llvm/lib"
        );
        let first = runpath().split(':').next().unwrap();
        let lib = place("libggml-base.so.0");
        assert_eq!(
            Some(first.trim_start_matches('/')),
            Path::new(&lib).parent().and_then(|p| p.to_str()),
            "the first entry is where the libraries are placed"
        );
    }

    /// The read-only data (the benchmark method, the language files, the
    /// licences) under /usr/share/private-ai: the benchmark method in the
    /// command line's package, `cli.toml` (what every program prints) in
    /// the core's, the GUI's language files in the configurator's, each
    /// package's own licences in that package.
    #[test]
    fn the_data_goes_under_usr_share_private_ai() {
        assert_eq!(share("benchmarks"), "usr/share/private-ai/benchmarks");
        assert_eq!(share("strings"), "usr/share/private-ai/strings");
        assert_eq!(
            share("licenses/llama"),
            "usr/share/private-ai/licenses/llama"
        );
        for (rel, want) in [
            (
                format!("{}/specs/standard-1.1.ini", share("benchmarks")),
                Component::Cli,
            ),
            (format!("{}/cli.toml", share("strings")), Component::Core),
            (
                format!("{}/en_US.toml", share("strings")),
                Component::Config,
            ),
        ] {
            assert_eq!(components::of(&rel), want, "{rel}");
        }
    }

    // ── pai, the command line, in a package of its own ──────────────────

    /// The version of every framework package in these tests: the
    /// framework's alone, no `+llama.cpp...` any more.
    const FW: &str = "2.99.1";

    /// `pai` is a command in /usr/bin, in its own package.
    #[test]
    fn pai_goes_to_usr_bin_in_the_cli_package() {
        assert_eq!(place("pai"), "usr/bin/pai");
        assert_eq!(components::of(&place("pai")), Component::Cli);
    }

    /// The command line talks to the manager of the core package, so it
    /// requires the core at its own version, and runs llama-bench and
    /// llama-server from /usr/bin, so it requires llama.cpp at least at the
    /// release it was built against; it requires nothing of the GUI and
    /// recommends no graphics library. It has no predecessor in 2.4 (pai did
    /// not exist), so it names no package of 2.4 or earlier: no Obsoletes,
    /// no Provides.
    #[test]
    fn the_command_line_requires_core_exactly_llama_cpp_at_least_and_nothing_of_the_gui() {
        let f = fields_v(Component::Cli, FW, &[], Some(ENGINE_MIN));
        let mut lines: Vec<String> = rpm_relations(&f).lines().map(str::to_string).collect();
        lines.sort();
        assert_eq!(
            lines,
            [
                format!("Requires:       llama.cpp >= {ENGINE_MIN}"),
                format!("Requires:       private-ai-framework-core = {FW}"),
            ]
        );
        assert_eq!(
            deb_relations(&f),
            (String::new(), String::new(), String::new())
        );
    }

    /// The GUI package's `.deb` Depends: the command line's at its own
    /// version (the benchmark method, which the GUI reads, is there), the
    /// core's at its own version (`cli.toml`, the manager), and llama.cpp at
    /// least at the release it was built against.
    #[test]
    fn the_configurator_deb_requires_cli_and_core_exactly_and_llama_cpp_at_least() {
        let f = fields_v(Component::Config, FW, &[], Some(ENGINE_MIN));
        let depends = depends_line(&f, "libc6 (>= 2.34), libfontconfig1 (>= 2.12.6)");
        let mut parts: Vec<&str> = depends.split(", ").collect();
        parts.sort();
        let cli = format!("private-ai-framework-cli (= {FW})");
        let core = format!("private-ai-framework-core (= {FW})");
        let llama = format!("llama.cpp (>= {ENGINE_MIN})");
        let mut want = vec![
            "libc6 (>= 2.34)",
            "libfontconfig1 (>= 2.12.6)",
            cli.as_str(),
            core.as_str(),
            llama.as_str(),
        ];
        want.sort();
        assert_eq!(parts, want, "{depends}");
    }

    /// The point of the split: the command line's `.deb` installs on a
    /// headless server. Whatever dpkg-shlibdeps answers for `pai` (or the
    /// fixed list when it gives none), the core's package and llama.cpp's,
    /// and nothing of the GUI: no configurator, no font or windowing library.
    #[test]
    fn the_cli_deb_depends_on_core_and_llama_cpp_and_no_graphics_library() {
        let f = fields_v(Component::Cli, FW, &[], Some(ENGINE_MIN));
        for scanned in ["", "libc6 (>= 2.34), libgcc-s1 (>= 4.2)"] {
            let depends = depends_line(&f, scanned);
            let parts: Vec<&str> = depends.split(", ").collect();
            for want in [
                format!("private-ai-framework-core (= {FW})"),
                format!("llama.cpp (>= {ENGINE_MIN})"),
            ] {
                assert!(
                    parts.contains(&want.as_str()),
                    "{scanned:?}: {want} missing in {depends}"
                );
            }
            assert!(
                parts.iter().any(|p| p.starts_with("libc6")),
                "{scanned:?}: {depends}"
            );
            for gui in [
                "private-ai-framework-config",
                "libfontconfig",
                "libfreetype",
                "libxkbcommon",
                "libwayland",
                "libx11",
                "libxcursor",
                "libxi6",
                "libegl",
                "libgl",
            ] {
                assert!(!depends.contains(gui), "{scanned:?}: {gui} in {depends}");
            }
        }
    }

    /// The engine's Depends is its scanned list and nothing else: no
    /// maintainer script any more (they went to the core), nothing of the
    /// framework. The core's: its scanned list, llama.cpp at least at the
    /// release it was built against, and what its maintainer scripts need.
    /// A runtime asks for nothing it does not bring along, the configurator
    /// for the fixed list when the scan gives none.
    #[test]
    fn the_deb_depends_of_the_engine_the_core_and_the_others() {
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = fields_v(Component::Llama, ENGINE, &runtimes, None);
        assert_eq!(depends_line(&f, "libc6 (>= 2.34)"), "libc6 (>= 2.34)");
        let f = fields_v(Component::Core, FW, &[], Some(ENGINE_MIN));
        let depends = depends_line(&f, "libc6 (>= 2.34)");
        let mut parts: Vec<&str> = depends.split(", ").collect();
        parts.sort();
        let llama = format!("llama.cpp (>= {ENGINE_MIN})");
        let mut want = vec![
            "libc6 (>= 2.34)",
            llama.as_str(),
            "systemd | systemd-standalone-sysusers | systemd-sysusers",
        ];
        want.sort();
        assert_eq!(parts, want, "{depends}");
        assert_eq!(
            system_files::deb_depends(Component::Core),
            "systemd | systemd-standalone-sysusers | systemd-sysusers"
        );
        let f = fields_v(Component::Rocm, "10.1.0", &[], None);
        assert_eq!(depends_line(&f, ""), "");
        let f = fields_v(Component::Config, FW, &[], Some(ENGINE_MIN));
        let depends = depends_line(&f, "");
        assert!(depends.starts_with("libc6, "), "{depends}");
        assert!(depends.contains("libfontconfig1"), "{depends}");
    }

    /// The control file of the command line's `.deb`: its identity and its
    /// Depends, and no Conflicts, Replaces or Provides, since no package of
    /// 2.4 or earlier held pai.
    #[test]
    fn the_cli_deb_names_no_package_of_2_4() {
        let f = fields_v(Component::Cli, FW, &[], Some(ENGINE_MIN));
        let depends =
            format!("libc6, private-ai-framework-core (= {FW}), llama.cpp (>= {ENGINE_MIN})");
        let text = deb_control(&f, "amd64", "Someone <a@b.c>", "1234", &depends);
        let lines: Vec<&str> = text.lines().collect();
        for want in [
            "Package: private-ai-framework-cli".to_string(),
            format!("Version: {FW}"),
            format!("Depends: {depends}"),
        ] {
            assert!(lines.contains(&want.as_str()), "{want} missing:\n{text}");
        }
        for tag in [
            "Conflicts:",
            "Replaces:",
            "Provides:",
            "Breaks:",
            "Recommends:",
        ] {
            assert!(
                !lines.iter().any(|l| l.starts_with(tag)),
                "{tag} in:\n{text}"
            );
        }
        assert!(!text.contains(OLD), "{text}");
    }

    /// The command line's `.rpm` lists its files (`pai`, the benchmark
    /// method, its licences and copyright) and nothing of the
    /// GUI's, and names no package of 2.4 or earlier.
    #[test]
    fn the_cli_rpm_spec_lists_its_files() {
        let dir = scratch("rpm-spec-cli");
        let cli = dir.join("cli");
        for (rel, mode) in [
            ("usr/bin/pai", 0o755),
            (
                "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
                0o644,
            ),
            (
                "usr/share/private-ai/benchmarks/prompts/bench-prompt-43k-1.0.txt",
                0o644,
            ),
            ("usr/share/private-ai/licenses/cli/README.txt", 0o644),
            ("usr/share/doc/private-ai-framework-cli/copyright", 0o644),
        ] {
            put(&cli, rel, ELF, mode);
        }
        let f = fields_v(Component::Cli, FW, &[], Some(ENGINE_MIN));
        let spec = rpm_spec(&f, "MIT", "libggml", &cli);
        check_files(&spec, &cli);
        std::fs::remove_dir_all(&dir).unwrap();
        let lines: Vec<&str> = spec.lines().collect();
        for want in [
            "Name:           private-ai-framework-cli".to_string(),
            format!("Version:        {FW}"),
            "Release:        1".to_string(),
            format!("Requires:       private-ai-framework-core = {FW}"),
            format!("Requires:       llama.cpp >= {ENGINE_MIN}"),
        ] {
            assert!(lines.contains(&want.as_str()), "{want} missing:\n{spec}");
        }
        assert!(!spec.contains("Epoch:"), "{spec}");
        assert!(!spec.contains(OLD), "{spec}");
        assert!(!spec.contains("Obsoletes:"), "{spec}");
        assert!(!spec.contains("org.privateai.Config"), "{spec}");
        let files = files_section(&spec);
        assert_eq!(
            files,
            [
                "/usr/share/doc/private-ai-framework-cli",
                "/usr/bin/*",
                "/usr/share/private-ai",
            ],
            "{spec}"
        );
    }

    /// The descriptions say which program each package holds: the
    /// configurator's is the GUI alone now, the command line's names `pai`.
    #[test]
    fn the_descriptions_tell_the_gui_from_the_command_line() {
        let (summary, description) = texts(Component::Cli, "v0.9.0");
        assert!(summary.starts_with("pai:"), "{summary}");
        assert!(!summary.contains("GUI"), "{summary}");
        assert!(description.contains("pai"), "{description}");
        let (summary, _) = texts(Component::Config, "v0.9.0");
        assert!(!summary.contains("CLI"), "{summary}");
    }

    /// The `%files` lines of a spec.
    fn files_section(spec: &str) -> Vec<String> {
        spec.split_once("\n%files\n")
            .map(|(_, rest)| rest)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('%'))
            .map(str::to_string)
            .collect()
    }

    /// Whether a `%files` line takes `path`: the path itself, a directory
    /// above it, or a `<dir>/*` glob of its directory.
    fn covers(line: &str, path: &str) -> bool {
        if let Some(dir) = line.strip_suffix("/*") {
            return Path::new(path).parent() == Some(Path::new(dir));
        }
        if line.contains('*') {
            let (head, tail) = line.split_once('*').unwrap();
            return path.starts_with(head) && path.ends_with(tail);
        }
        path == line || path.starts_with(&format!("{line}/"))
    }

    /// Checks `%files` against `root` as rpmbuild does: each line must take
    /// a file of the tree (a line without a match stops rpmbuild), every file
    /// must be taken, and no line may claim a directory of the system.
    fn check_files(spec: &str, root: &Path) {
        let files = files_section(spec);
        let tree: Vec<String> = walk(root)
            .into_iter()
            .filter(|p| !p.is_dir())
            .map(|p| format!("/{}", p.strip_prefix(root).unwrap().display()))
            .collect();
        for line in &files {
            assert!(
                !line.contains("opt/private-ai"),
                "nothing under /opt: {line}\n{spec}"
            );
            assert!(
                ![
                    "/usr",
                    "/usr/bin",
                    "/usr/lib",
                    "/usr/libexec",
                    "/usr/share",
                    "/usr/share/doc",
                    "/usr/share/applications",
                    "/usr/share/icons",
                    "/usr/lib/systemd",
                    "/usr/lib/systemd/system",
                    "/usr/lib/sysusers.d",
                    "/usr/share/dbus-1",
                    "/usr/share/dbus-1/system.d",
                    "/usr/share/polkit-1",
                    "/usr/share/polkit-1/actions",
                    "/etc",
                ]
                .contains(&line.as_str()),
                "{line} would own a directory of the system:\n{spec}"
            );
            assert!(
                tree.iter().any(|f| covers(line, f)),
                "{line} takes no file of the package (rpmbuild stops):\n{spec}"
            );
        }
        for f in &tree {
            assert!(
                files.iter().any(|l| covers(l, f)),
                "{f} is in no %files line:\n{spec}"
            );
        }
    }

    /// The `.rpm` of each package lists the /usr layout in `%files`, nothing
    /// more (a line without a match stops rpmbuild) and no directory of the
    /// system; the libraries under /usr/lib/private-ai are provided to no one
    /// (a system package asking for `libvulkan.so.1` must not get ours); the
    /// relations stay.
    #[test]
    fn the_rpm_spec_lists_the_usr_layout() {
        let dir = scratch("rpm-spec");
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];

        let llama = dir.join("llama");
        for (rel, mode) in [
            ("usr/bin/llama-server", 0o755),
            ("usr/bin/llama-bench", 0o755),
            ("usr/lib/private-ai/libggml-base.so.0", 0o755),
            ("usr/lib/private-ai/libggml-hip.so", 0o755),
            ("usr/lib/private-ai/lib/llvm/lib/libomp.so", 0o755),
            ("usr/share/doc/llama.cpp/copyright", 0o644),
            ("usr/share/private-ai/licenses/llama/README.txt", 0o644),
        ] {
            put(&llama, rel, ELF, mode);
        }
        let f = fields_v(Component::Llama, ENGINE, &runtimes, None);
        let spec = rpm_spec(&f, "MIT", "libggml\\\\-base|libomp", &llama);
        check_files(&spec, &llama);
        let lines: Vec<&str> = spec.lines().collect();
        assert!(
            lines.contains(&"%global __provides_exclude_from ^/usr/lib/private-ai/.*$"),
            "{spec}"
        );
        assert!(
            lines.contains(&"%global __requires_exclude ^(libggml\\\\-base|libomp|libcuda)\\\\.so"),
            "{spec}"
        );
        for want in [
            "Name:           llama.cpp",
            "Epoch:          1",
            "Version:        1.0.0",
            "Release:        pai1",
            "License:        MIT",
            "Conflicts:      llama-cpp",
        ] {
            assert!(lines.contains(&want), "{want} missing:\n{spec}");
        }
        for line in rpm_relations(&f).lines() {
            assert!(lines.contains(&line), "{line} missing:\n{spec}");
        }
        assert!(!spec.contains("opt/private-ai"), "{spec}");

        let cuda = dir.join("cuda");
        for rel in [
            "usr/lib/private-ai/lib/libcublas.so.13",
            "usr/share/doc/llama.cpp-cuda/copyright",
            "usr/share/private-ai/licenses/cuda/README.txt",
        ] {
            put(&cuda, rel, ELF, 0o644);
        }
        let f = fields_v(Component::Cuda, "13.0.2", &[], None);
        let spec = rpm_spec(&f, "LicenseRef-NVIDIA", "libcublas", &cuda);
        check_files(&spec, &cuda);

        let config = dir.join("config");
        for rel in [
            "usr/bin/pai-config",
            "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
            "usr/share/private-ai/strings/en_US.toml",
            "usr/share/private-ai/licenses/config/README.txt",
            "usr/share/applications/org.privateai.Config.desktop",
            "usr/share/icons/hicolor/256x256/apps/org.privateai.Config.png",
            "usr/share/doc/private-ai-framework-config/copyright",
        ] {
            put(&config, rel, ELF, 0o644);
        }
        let f = fields_v(Component::Config, FW, &[], Some(ENGINE_MIN));
        let spec = rpm_spec(&f, "MIT", "libggml", &config);
        check_files(&spec, &config);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// dpkg-shlibdeps scans every ELF program and library the package
    /// installs, in /usr/bin, /usr/libexec/private-ai and /usr/lib/private-ai,
    /// but libggml-cuda.so and the CUDA libraries (they link the NVIDIA
    /// driver's libcuda.so.1, which no Debian package provides) and no data
    /// file.
    #[test]
    fn dpkg_shlibdeps_scans_the_programs_and_libraries_of_the_usr_layout() {
        let root = scratch("shlibdeps");
        for (rel, bytes, mode) in [
            ("usr/bin/llama-server", ELF, 0o755),
            ("usr/libexec/private-ai/pai-manager", ELF, 0o755),
            ("usr/lib/private-ai/libggml-base.so.0", ELF, 0o755),
            ("usr/lib/private-ai/libggml-cuda.so", ELF, 0o755),
            ("usr/lib/private-ai/lib/libcublas.so.13", ELF, 0o755),
            (
                "usr/lib/private-ai/lib/rocblas/library/Kernels.so-000-gfx1201.dat",
                b"data".as_slice(),
                0o644,
            ),
            (
                "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
                b"[x]".as_slice(),
                0o644,
            ),
            (
                "usr/share/doc/private-ai-framework-llama/copyright",
                b"MIT".as_slice(),
                0o644,
            ),
        ] {
            put(&root, rel, bytes, mode);
        }
        let mut got: Vec<String> = shlibdeps_inputs(&root)
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().display().to_string())
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                "usr/bin/llama-server",
                "usr/lib/private-ai/libggml-base.so.0",
                "usr/libexec/private-ai/pai-manager",
            ]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The libraries of the five packages, which no package provides to the
    /// system or requires from it, are found under /usr/lib/private-ai: the
    /// rpm's requires filter and dpkg-shlibdeps' private library dirs. Missed,
    /// the llama.cpp package would require a `libggml-base.so.0` from the
    /// system and could not install.
    #[test]
    fn the_private_libraries_are_found_in_the_usr_layout() {
        let roots = scratch("private-libs");
        for rel in [
            "llama/usr/bin/llama-server",
            "llama/usr/lib/private-ai/libggml-base.so.0",
            "cuda/usr/lib/private-ai/lib/libcublas.so.13",
            "rocm/usr/lib/private-ai/lib/llvm/lib/libLLVM.so.24.0git",
        ] {
            put(&roots, rel, ELF, 0o755);
        }
        let present = [Component::Llama, Component::Rocm, Component::Cuda];
        assert_eq!(
            library_stems(&roots, &present),
            "libLLVM|libcublas|libggml-base"
        );
        let want: BTreeSet<String> = [
            "llama/usr/lib/private-ai",
            "cuda/usr/lib/private-ai/lib",
            "rocm/usr/lib/private-ai/lib/llvm/lib",
        ]
        .iter()
        .map(|d| format!("-l{}", roots.join(d).display()))
        .collect();
        assert_eq!(private_library_dirs(&roots, &present), want);
        std::fs::remove_dir_all(&roots).unwrap();
    }

    // ── The system files of the services ────────────────────────────────

    /// The units call pai-manager by full path, where `place` puts it.
    #[test]
    fn the_units_run_pai_manager_where_the_package_places_it() {
        use super::super::system_files::{instance_unit, manager_unit};
        let exe = format!("/{}", place("pai-manager"));
        let manager = manager_unit();
        assert!(
            manager.contains(&format!("\nExecStart={exe} serve\n")),
            "{manager}"
        );
        let instance = instance_unit();
        assert!(
            instance.contains(&format!("\nExecStart={exe} run-instance %i\n")),
            "{instance}"
        );
    }

    /// The core's `.rpm` lists each of its system files by name in `%files`
    /// (never their directories, which systemd, dbus and polkit own), runs
    /// its scriptlets (before `%files`, whose lines run to the end of the
    /// spec), and needs systemd for them (Fedora's `%systemd_requires`:
    /// Requires(post), (preun), (postun); %posttrans too, since the start
    /// is there). rpm (4.19 and later) turns each `m` line of the sysusers.d
    /// file into a `Requires: group(<group>)`; render and video may be
    /// provided by no package of an older distribution, and the `m` line
    /// creates a missing group anyway, so they are weak
    /// (`_use_weak_usergroup_deps`, /usr/lib/rpm/fileattrs/sysusers.attr).
    /// The configurator's and the engine's `.rpm` have none of it.
    #[test]
    fn the_core_rpm_ships_the_system_files_and_their_scriptlets() {
        use super::super::system_files::{files, rpm_scriptlets};
        let dir = scratch("rpm-system-files");
        let core = dir.join("core");
        for (rel, mode) in [
            ("usr/libexec/private-ai/pai-manager", 0o755),
            ("usr/share/private-ai/strings/cli.toml", 0o644),
            ("usr/share/doc/private-ai-framework-core/copyright", 0o644),
            ("usr/share/private-ai/licenses/core/README.txt", 0o644),
        ] {
            put(&core, rel, ELF, mode);
        }
        let ours: Vec<(&str, String)> = files()
            .into_iter()
            .filter(|(rel, _)| components::of(rel) == Component::Core)
            .collect();
        assert_eq!(ours.len(), 5, "{ours:?}");
        for (rel, text) in &ours {
            put(&core, rel, text.as_bytes(), 0o644);
        }
        let f = fields_v(Component::Core, FW, &[], Some(ENGINE_MIN));
        let spec = rpm_spec(&f, "MIT", "libggml\\\\-base", &core);
        check_files(&spec, &core);
        let listed = files_section(&spec);
        for (rel, _) in &ours {
            let line = format!("/{rel}");
            assert!(listed.contains(&line), "{line} missing:\n{spec}");
        }
        let scriptlets = rpm_scriptlets(Component::Core);
        assert!(!scriptlets.is_empty());
        let at = spec
            .find(&scriptlets)
            .unwrap_or_else(|| panic!("the scriptlets are missing:\n{spec}"));
        assert!(at < spec.find("\n%files\n").unwrap(), "{spec}");
        let lines: Vec<&str> = spec.lines().collect();
        assert!(
            lines.contains(&"%global _use_weak_usergroup_deps 1"),
            "{spec}"
        );
        for tag in ["post", "preun", "postun", "posttrans"] {
            let want = format!("Requires({tag}): systemd");
            assert!(
                rpm_relations(&f)
                    .lines()
                    .any(|l| l.split_whitespace().collect::<Vec<_>>()
                        == want.split_whitespace().collect::<Vec<_>>()),
                "{want} missing:\n{}",
                rpm_relations(&f)
            );
        }

        let config = dir.join("config");
        for rel in [
            "usr/bin/pai-config",
            "usr/share/doc/private-ai-framework-config/copyright",
        ] {
            put(&config, rel, ELF, 0o644);
        }
        let llama = dir.join("llama");
        for rel in [
            "usr/bin/llama-server",
            "usr/lib/private-ai/libggml-base.so.0",
            "usr/share/doc/llama.cpp/copyright",
        ] {
            put(&llama, rel, ELF, 0o755);
        }
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        for (f, root) in [
            (
                fields_v(Component::Config, FW, &[], Some(ENGINE_MIN)),
                &config,
            ),
            (fields_v(Component::Llama, ENGINE, &runtimes, None), &llama),
        ] {
            let spec = rpm_spec(&f, "MIT", "libggml", root);
            check_files(&spec, root);
            for scriptlet in ["%post", "%preun", "%postun", "%posttrans"] {
                assert!(
                    !spec.lines().any(|l| l.trim() == scriptlet),
                    "{scriptlet} in {:?}:\n{spec}",
                    f.component
                );
            }
            assert!(!rpm_relations(&f).contains("Requires(post)"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ── pai-bff, the web service, in a package of its own ───────────────

    /// pai-bff is a service, not a user command: beside pai-manager in
    /// /usr/libexec/private-ai, in the bff's package.
    #[test]
    fn pai_bff_goes_to_libexec_in_the_bff_package() {
        assert_eq!(place("pai-bff"), "usr/libexec/private-ai/pai-bff");
        assert_eq!(components::of(&place("pai-bff")), Component::Bff);
        assert_eq!(place("pai-manager"), "usr/libexec/private-ai/pai-manager");
    }

    /// The bff's unit calls pai-bff by full path, where `place` puts it.
    #[test]
    fn the_bff_unit_runs_pai_bff_where_the_package_places_it() {
        let exe = format!("/{}", place("pai-bff"));
        let unit = super::super::system_files::bff_unit();
        assert!(unit.contains(&format!("\nExecStart={exe}\n")), "{unit}");
    }

    /// The bff talks to the manager of the core package, so it requires
    /// that package at its own version, like the command line; it never runs
    /// the engine, so it does not require llama.cpp; nothing of the GUI or
    /// the command line. It has no predecessor in 2.4, so it names no
    /// package of 2.4 or earlier. Its scriptlets run systemctl, so the
    /// `.rpm` needs systemd for them, as the core package does.
    #[test]
    fn the_bff_requires_core_exactly_and_systemd_for_its_scriptlets() {
        let f = fields_v(Component::Bff, FW, &[], None);
        let rpm = rpm_relations(&f);
        let mut lines: Vec<String> = rpm
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        lines.sort();
        let mut want = vec![
            format!("Requires: private-ai-framework-core = {FW}"),
            "Requires(post): systemd".to_string(),
            "Requires(preun): systemd".to_string(),
            "Requires(postun): systemd".to_string(),
            "Requires(posttrans): systemd".to_string(),
        ];
        want.sort();
        assert_eq!(lines, want, "{rpm}");
        assert_eq!(
            deb_relations(&f),
            (String::new(), String::new(), String::new())
        );
    }

    /// The bff's `.deb` Depends: what dpkg-shlibdeps found for pai-bff (or
    /// the fixed C libraries when it gives none) and the core's package at
    /// its version; not the engine, no GUI, no command line, no graphics
    /// library, nothing for its scripts (system_files::deb_depends is empty
    /// for it).
    #[test]
    fn the_bff_deb_depends_on_core_and_nothing_of_the_front_ends() {
        let f = fields_v(Component::Bff, FW, &[], None);
        for scanned in ["", "libc6 (>= 2.34), libgcc-s1 (>= 4.2)"] {
            let depends = depends_line(&f, scanned);
            let parts: Vec<&str> = depends.split(", ").collect();
            assert!(
                parts.contains(&format!("private-ai-framework-core (= {FW})").as_str()),
                "{scanned:?}: {depends}"
            );
            assert!(
                parts.iter().any(|p| p.starts_with("libc6")),
                "{scanned:?}: {depends}"
            );
            for other in [
                "private-ai-framework-config",
                "private-ai-framework-cli",
                "libfontconfig",
                "libfreetype",
                "libxkbcommon",
                "libwayland",
                "libx11",
                "libegl",
                "sysusers",
                "llama.cpp",
            ] {
                assert!(
                    !depends.contains(other),
                    "{scanned:?}: {other} in {depends}"
                );
            }
        }
    }

    /// The bff's control file: its identity and Depends, and no
    /// Conflicts, Replaces or Provides (no package of 2.4 held it).
    #[test]
    fn the_bff_deb_names_no_package_of_2_4() {
        let f = fields_v(Component::Bff, FW, &[], None);
        let depends = format!("libc6, private-ai-framework-core (= {FW})");
        let text = deb_control(&f, "amd64", "Someone <a@b.c>", "1234", &depends);
        let lines: Vec<&str> = text.lines().collect();
        for want in [
            "Package: private-ai-framework-bff".to_string(),
            format!("Version: {FW}"),
            format!("Depends: {depends}"),
        ] {
            assert!(lines.contains(&want.as_str()), "{want} missing:\n{text}");
        }
        for tag in ["Conflicts:", "Replaces:", "Provides:", "Breaks:"] {
            assert!(
                !lines.iter().any(|l| l.starts_with(tag)),
                "{tag} in:\n{text}"
            );
        }
        assert!(!text.contains(OLD), "{text}");
    }

    /// The bff's `.rpm` lists its files as rpmbuild checks them (pai-bff,
    /// its unit by name, its licences and copyright), runs its scriptlets
    /// before `%files`, and names no package of 2.4 or earlier.
    #[test]
    fn the_bff_rpm_spec_lists_its_files_and_scriptlets() {
        use super::super::system_files::rpm_scriptlets;
        let dir = scratch("rpm-spec-bff");
        let bff = dir.join("bff");
        for (rel, mode) in [
            ("usr/libexec/private-ai/pai-bff", 0o755),
            ("usr/lib/systemd/system/private-ai-bff.service", 0o644),
            ("usr/share/private-ai/licenses/bff/README.txt", 0o644),
            ("usr/share/doc/private-ai-framework-bff/copyright", 0o644),
        ] {
            put(&bff, rel, ELF, mode);
        }
        let f = fields_v(Component::Bff, FW, &[], None);
        let spec = rpm_spec(&f, "MIT", "libggml", &bff);
        check_files(&spec, &bff);
        std::fs::remove_dir_all(&dir).unwrap();
        let lines: Vec<&str> = spec.lines().collect();
        for want in [
            "Name:           private-ai-framework-bff".to_string(),
            format!("Version:        {FW}"),
        ] {
            assert!(lines.contains(&want.as_str()), "{want} missing:\n{spec}");
        }
        for line in rpm_relations(&f).lines() {
            assert!(lines.contains(&line), "{line} missing:\n{spec}");
        }
        let listed = files_section(&spec);
        assert!(
            listed.contains(&"/usr/lib/systemd/system/private-ai-bff.service".to_string()),
            "{spec}"
        );
        let scriptlets = rpm_scriptlets(Component::Bff);
        assert!(!scriptlets.is_empty());
        let at = spec
            .find(&scriptlets)
            .unwrap_or_else(|| panic!("the scriptlets are missing:\n{spec}"));
        assert!(at < spec.find("\n%files\n").unwrap(), "{spec}");
        assert!(!spec.contains(OLD), "{spec}");
        assert!(!spec.contains("Obsoletes:"), "{spec}");
        assert!(!spec.contains("org.privateai.Config"), "{spec}");
    }

    /// The description says what the package holds: pai-bff, the web
    /// service.
    #[test]
    fn the_bff_description_names_the_web_service() {
        let (summary, description) = texts(Component::Bff, "v0.9.0");
        assert!(summary.starts_with("pai-bff:"), "{summary}");
        assert!(description.contains("pai-bff"), "{description}");
        assert!(description.to_lowercase().contains("web"), "{description}");
        assert!(!description.contains('\u{2014}'), "{description}");
    }

    /// The package phase itself (`run`, which needs a build and is not run
    /// here): pai-bff is copied from cargo's release dir, as pai-manager is,
    /// to where `place` says. The packages that run the engine (the
    /// configurator, the command line, the core) require llama.cpp at least
    /// at `llama_cpp_min`; the bff does not. The engine's version comes from
    /// `llama_cpp_version`, and no package carries `+llama.cpp.<tag>` any
    /// more. Read from the code before the tests.
    #[test]
    fn the_package_phase_stages_pai_bff_and_versions_the_engine_apart() {
        let src = include_str!("package.rs");
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        assert!(
            code.contains(r#""cargo-target/release/pai-bff""#),
            "run() does not take pai-bff from cargo's release dir"
        );
        assert!(
            code.contains(r#"place("pai-bff")"#),
            "run() does not stage pai-bff where place() says"
        );
        let field = code
            .split("depends_on_llama: ")
            .nth(1)
            .and_then(|rest| rest.split(".then_some").next())
            .expect("the depends_on_llama field in run()");
        for c in ["Component::Config", "Component::Cli", "Component::Core"] {
            assert!(field.contains(c), "{c} does not require llama.cpp: {field}");
        }
        assert!(
            !field.contains("Component::Bff"),
            "the bff requires llama.cpp: {field}"
        );
        for call in [
            "components::llama_cpp_version(",
            "components::llama_cpp_min(",
        ] {
            assert!(code.contains(call), "run() does not call {call}");
        }
        assert!(
            !code.contains("+llama.cpp.{"),
            "a package version still carries the llama.cpp tag"
        );
    }

    // ── The framework's core and the engine ─────────────────────────────

    /// The core requires llama.cpp at least at the release it was built
    /// against (`llama.cpp@.service` runs `pai-manager run-instance`, which
    /// runs llama-server) and systemd for its scriptlets. It holds nothing
    /// 2.4 shipped (2.4's `-llama` held llama.cpp alone, read with `dpkg -c`
    /// on the 2.4.0 package), so it names no package of 2.4 or earlier, and
    /// no 2.99.x name, which was never published.
    #[test]
    fn the_core_requires_llama_cpp_at_least_and_names_no_older_package() {
        let f = fields_v(Component::Core, FW, &[], Some(ENGINE_MIN));
        let rpm = rpm_relations(&f);
        let mut lines: Vec<String> = rpm
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        lines.sort();
        let mut want = vec![
            format!("Requires: llama.cpp >= {ENGINE_MIN}"),
            "Requires(post): systemd".to_string(),
            "Requires(preun): systemd".to_string(),
            "Requires(postun): systemd".to_string(),
            "Requires(posttrans): systemd".to_string(),
        ];
        want.sort();
        assert_eq!(lines, want, "{rpm}");
        assert_eq!(
            deb_relations(&f),
            (String::new(), String::new(), String::new())
        );
        let depends = depends_line(&f, "libc6");
        let text = deb_control(&f, "amd64", "Someone <a@b.c>", "1234", &depends);
        let lines: Vec<&str> = text.lines().collect();
        for want in [
            "Package: private-ai-framework-core".to_string(),
            format!("Version: {FW}"),
            format!("Depends: {depends}"),
        ] {
            assert!(lines.contains(&want.as_str()), "{want} missing:\n{text}");
        }
        for tag in ["Conflicts:", "Replaces:", "Provides:", "Breaks:"] {
            assert!(
                !lines.iter().any(|l| l.starts_with(tag)),
                "{tag} in:\n{text}"
            );
        }
        assert!(!text.contains(OLD), "{text}");
        assert!(!rpm.contains(OLD), "{rpm}");
        assert!(!text.contains("private-ai-framework-llama"), "{text}");
    }

    /// The engine's `.deb` carries its version whole, epoch and build
    /// revision (Debian Policy 5.6.12: `[epoch:]upstream_version`), and
    /// names nothing of the framework: it installs and runs alone.
    #[test]
    fn the_engine_deb_carries_its_epoch_and_requires_nothing_of_the_framework() {
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = fields_v(Component::Llama, ENGINE, &runtimes, None);
        let depends = depends_line(&f, "libc6 (>= 2.34)");
        let text = deb_control(&f, "amd64", "Someone <a@b.c>", "1234", &depends);
        let lines: Vec<&str> = text.lines().collect();
        for want in [
            "Package: llama.cpp".to_string(),
            "Version: 1:1.0.0+pai1".to_string(),
            format!("Provides: {OLD}-llama (= 1:1.0.0+pai1)"),
            format!("Replaces: {OLD}, {OLD}-llama"),
            format!("Conflicts: {OLD}, {OLD}-llama, llama.cpp-tools"),
        ] {
            assert!(lines.contains(&want.as_str()), "{want} missing:\n{text}");
        }
        assert!(!text.contains("private-ai-framework"), "{text}");
        assert!(!rpm_relations(&f).contains("private-ai-framework"));
    }

    /// The engine's `.rpm` spells its version as rpm wants it: `Epoch: 1`,
    /// the release as `Version`, and our build revision as `Release`
    /// (rpm-spec(5): Release is "the downstream packaging revision of a
    /// software version", which `+pai<N>` is; rpm-version(7): a release may
    /// hold letters, and `-` is the separator, so `pai1` and not `+pai1`).
    /// A framework package keeps no epoch and Release 1.
    #[test]
    fn the_rpm_spec_carries_the_engine_epoch_and_build_revision() {
        let dir = scratch("rpm-spec-evr");
        let llama = dir.join("llama");
        put(&llama, "usr/bin/llama-server", ELF, 0o755);
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = fields_v(Component::Llama, ENGINE, &runtimes, None);
        let spec = rpm_spec(&f, "MIT", "libggml", &llama);
        let core = dir.join("core");
        put(&core, "usr/libexec/private-ai/pai-manager", ELF, 0o755);
        let g = fields_v(Component::Core, FW, &[], Some(ENGINE_MIN));
        let core_spec = rpm_spec(&g, "MIT", "libggml", &core);
        std::fs::remove_dir_all(&dir).unwrap();
        let header = |spec: &str| -> Vec<String> {
            spec.lines()
                .take_while(|l| !l.is_empty())
                .filter(|l| {
                    ["Name:", "Epoch:", "Version:", "Release:"]
                        .iter()
                        .any(|t| l.starts_with(t))
                })
                .map(str::to_string)
                .collect()
        };
        assert_eq!(
            header(&spec),
            [
                "Name:           llama.cpp",
                "Epoch:          1",
                "Version:        1.0.0",
                "Release:        pai1",
            ],
            "{spec}"
        );
        assert_eq!(
            header(&core_spec),
            [
                "Name:           private-ai-framework-core",
                format!("Version:        {FW}").as_str(),
                "Release:        1",
            ],
            "{core_spec}"
        );
    }

    /// The core's description names pai-manager and the services; the
    /// engine's names llama.cpp and no program of ours.
    #[test]
    fn the_descriptions_tell_the_core_from_the_engine() {
        let (summary, description) = texts(Component::Core, "v1.0.0");
        assert!(summary.starts_with("pai-manager:"), "{summary}");
        assert!(description.contains("pai-manager"), "{description}");
        assert!(!description.contains('\u{2014}'), "{description}");
        let (summary, description) = texts(Component::Llama, "v1.0.0");
        assert!(summary.contains("llama.cpp"), "{summary}");
        assert!(!summary.contains("pai"), "{summary}");
        assert!(!description.contains("pai"), "{description}");
    }

    // ── The engine's and the runtimes' own identity ─────────────────────

    /// The engine and its runtimes point at llama.cpp's upstream (the
    /// `ggml-org` organisation, as llama.cpp's README links it; the clone's
    /// `ggerganov` remote redirects there), not at the framework's
    /// repository; the framework's packages keep the framework's.
    #[test]
    fn the_engine_and_its_runtimes_carry_llama_cpps_homepage() {
        let ours = "https://github.com/example/private-ai-framework";
        for c in [
            Component::Llama,
            Component::Rocm,
            Component::Cuda,
            Component::Vulkan,
        ] {
            assert_eq!(
                homepage(c, ours),
                "https://github.com/ggml-org/llama.cpp",
                "{c:?}"
            );
        }
        for c in [
            Component::Config,
            Component::Cli,
            Component::Bff,
            Component::Core,
        ] {
            assert_eq!(homepage(c, ours), ours, "{c:?}");
        }
        // As the control file and the spec write it.
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = Fields {
            homepage: homepage(Component::Llama, ours),
            ..fields_v(Component::Llama, ENGINE, &runtimes, None)
        };
        let text = deb_control(&f, "amd64", "Someone <a@b.c>", "1234", "libc6");
        assert!(
            text.lines()
                .any(|l| l == "Homepage: https://github.com/ggml-org/llama.cpp"),
            "{text}"
        );
        let dir = scratch("rpm-spec-url");
        put(&dir, "usr/bin/llama-server", ELF, 0o755);
        let spec = rpm_spec(&f, "MIT", "libggml", &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            spec.lines()
                .any(|l| l == "URL:            https://github.com/ggml-org/llama.cpp"),
            "{spec}"
        );
    }

    /// A runtime's copyright file says what the package is: its SDK's
    /// libraries, at the SDK's version, for the llama.cpp engine, where the
    /// licence texts are (its own directory under /usr/share/private-ai/
    /// licenses), and the README of that directory. It is not part of the
    /// framework: it neither says so nor carries the framework's MIT text.
    #[test]
    fn a_runtimes_copyright_describes_the_runtime_not_the_framework() {
        let readme = "libamdhip64.so.7: MIT, see rocm.txt\n";
        for (c, version, sdk) in [
            (Component::Rocm, "10.1.0", "ROCm"),
            (Component::Cuda, "13.5.0+nccl.2.32.3", "CUDA"),
            (Component::Vulkan, "1.4.363.1", "Vulkan"),
        ] {
            let text = runtime_copyright(c, version, readme);
            let first = text.lines().next().unwrap_or_default();
            assert!(
                first.starts_with(&format!("{} {version} ", c.package())),
                "{c:?}: {text}"
            );
            for want in [
                sdk,
                "llama.cpp",
                &format!("/usr/share/private-ai/licenses/{}", c.suffix()),
                readme,
            ] {
                assert!(text.contains(want), "{c:?}: {want} missing:\n{text}");
            }
            assert!(!text.contains("private-ai-framework"), "{c:?}:\n{text}");
            assert!(!text.contains("part of"), "{c:?}:\n{text}");
            assert!(
                !text.contains("Permission is hereby granted"),
                "{c:?}: the framework's MIT text:\n{text}"
            );
            assert!(!text.contains('\u{2014}'), "{c:?}:\n{text}");
        }
    }

    /// The package phase writes each runtime's copyright with
    /// [`runtime_copyright`] and each package's Homepage with [`homepage`].
    /// Read from the code before the tests.
    #[test]
    fn the_package_phase_uses_the_runtime_copyright_and_homepage() {
        let src = include_str!("package.rs");
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let run = code
            .split("pub fn run(")
            .nth(1)
            .and_then(|r| r.split("\nfn ").next())
            .expect("run()");
        for call in ["runtime_copyright(", "homepage("] {
            assert!(run.contains(call), "run() does not call {call}");
        }
    }

    /// The engine's version is built from a llama.cpp release
    /// (`1:<release>+pai<N>`), so `run()` refuses a build that is not on a
    /// release tag (the `b0-<sha>` fallback of `version::llama_build`, a
    /// `bNNNN` nightly, an `-rc`) before it stages or builds any package:
    /// the check, a `bail!` on `util::is_release_tag(&llama_build)`, comes
    /// right after the release is read and before the staging, `build_deb`
    /// and `build_rpm`. Read from the code before the tests.
    #[test]
    fn a_build_that_is_not_a_release_is_refused_before_any_package() {
        for tag in ["b0-1a2b3c4", "b10566", "v1.0.0-rc1", "v1.0", ""] {
            assert!(!util::is_release_tag(tag), "{tag:?}");
        }
        assert!(util::is_release_tag("v1.0.0"));
        let src = include_str!("package.rs");
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let run = code
            .split("pub fn run(")
            .nth(1)
            .and_then(|r| r.split("\nfn ").next())
            .expect("run()");
        let check = run
            .find("if !util::is_release_tag(&llama_build) {\n        bail!(")
            .expect("run() does not bail on a llama.cpp build that is not a release");
        assert!(
            run.find("version::llama_build(") < Some(check),
            "the release is checked before it is read"
        );
        for later in ["util::remove_all(&stage)", "build_deb(", "build_rpm("] {
            let at = run
                .find(later)
                .unwrap_or_else(|| panic!("{later} not in run()"));
            assert!(check < at, "{later} comes before the release check");
        }
    }
}
