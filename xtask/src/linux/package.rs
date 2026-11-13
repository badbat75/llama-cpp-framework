//! Package phase (cargo run -p xtask -- package): the Linux build as five .deb
//! and five .rpm packages in ./dist/ (common/components.rs says which file goes
//! where): pai-config, llama.cpp, and the ROCm, CUDA and Vulkan runtimes.
//! The counterpart of the Windows MSI (windows/package.rs). Requires a successful
//! build; patchelf, dpkg-dev and rpm come from the prerequisites.
//!
//! What the packages install together (all from one staged tree):
//!   /opt/private-ai/bin/  every llama.cpp tool, its shared libraries and the
//!                         ggml backends, plus pai-config: ONE
//!                         directory, because ggml looks for its backends next
//!                         to the executable and pai-config looks for
//!                         llama-server there too
//!   /opt/private-ai/lib/  the GPU runtimes the backends need, taken from the
//!                         dists this build linked against: the ROCm runtime
//!                         with its kernel libraries, the CUDA libraries, the
//!                         Vulkan loader. Self-contained: nothing else has to
//!                         be installed but the GPU drivers.
//!   /opt/private-ai/share/benchmarks/
//!                         the benchmark method, read-only: every published
//!                         standard-<X.Y>.ini and the prompts they pin
//!   /opt/private-ai/share/strings/
//!                         the configurator's language files, read-only
//!   /usr/bin/             a symlink per executable into /opt/private-ai/bin
//!   /usr/share/           the desktop entry, its icon, the licence
//!
//! pai-config and llama.cpp carry <framework>+llama.cpp.<tag> as version
//! and require each other's exact one, in that direction. A runtime carries its
//! SDK's version and is optional: llama.cpp recommends the one it was built
//! against and refuses any other, because a backend that cannot resolve a symbol
//! is skipped at start with nothing but a log line.
//!
//! The config files (server.ini, presets.ini, presets.ini.disabled,
//! settings.ini) are NOT in the package: pai-config
//! writes them per user, in ~/.config/private-ai, on first use.

use super::build_llama::load_build_env;
use super::dists;
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

/// The single package every release up to `MONOLITH_LAST` shipped, which the five replace.
const MONOLITH: &str = components::BASE_NAME;
/// The last release that shipped it: the relation against `MONOLITH` stops there, an
/// unversioned one matches every version of the name, future ones included (rpmbuild
/// warns about it). See "The `.deb` and `.rpm` (Linux)" in docs/build-packaging.md.
const MONOLITH_LAST: &str = "2.2.0";
const OPT_DIR: &str = "opt/private-ai"; // relative to the package root
const BIN_DIR: &str = "opt/private-ai/bin"; // llama.cpp + pai-config
const RT_DIR: &str = "opt/private-ai/lib"; // the bundled GPU runtimes

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
        bail!(
            "llama-server not found in {}. Run cargo run -p xtask -- build first.",
            bin.display()
        );
    }
    if !util::is_executable(&bin.join("pai-config")) {
        bail!(
            "pai-config not found in {}. Run cargo run -p xtask -- build first.",
            bin.display()
        );
    }
    // A version bump without a rebuild would ship the OLD configurator in a
    // package named after the new version.
    let built = output(cmd(bin.join("pai-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();
    if built != fw_version {
        bail!("the built pai-config is {built}, Cargo.toml says {fw_version}. Run cargo run -p xtask -- build first.");
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
        stage.join(BIN_DIR),
        stage.join(RT_DIR),
        stage.join("usr/bin"),
        pkg_work.clone(),
        out_dir.clone(),
    ] {
        std::fs::create_dir_all(&d)?;
    }

    ui::head("Staging binaries...");
    // Straight from the build's bin/, where the build leaves executables,
    // libraries, backends and pai-config side by side: the layout the
    // package keeps. `cmake --install` would split them into bin/ and lib/.
    // llama.cpp's own test-* harnesses stay out: they are its unit tests, and
    // every executable staged here gets a command in /usr/bin.
    let (tests, keep): (Vec<PathBuf>, Vec<PathBuf>) = entries(&bin)
        .into_iter()
        .partition(|p| name_of(p).starts_with("test-"));
    cp("-a", &keep, &stage.join(BIN_DIR))?;
    crate::common::bench_share::stage(&stage.join(OPT_DIR))?;
    crate::common::strings_share::stage(&stage.join(OPT_DIR))?;
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
        if stage.join(BIN_DIR).join(backend).exists()
            && !rt_names.iter().any(|n| n.starts_with(runtime))
        {
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
            let picked: Vec<PathBuf> = entries(&src)
                .into_iter()
                .filter(|p| wanted(&name_of(p)))
                .collect();
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
    ui::dim(format!(
        "{} executables, {before} MiB -> {after} MiB stripped",
        executables.len()
    ));
    ui::dim(format!("Bundled runtimes: {} MiB", util::du("-sm", &rt)));

    for exe in &executables {
        // relative: rpm warns on absolute ones
        std::os::unix::fs::symlink(
            format!("../../{BIN_DIR}/{exe}"),
            stage.join("usr/bin").join(exe),
        )?;
    }

    // ── Desktop entry, icon ─────────────────────────────────────────────
    let apps = stage.join("usr/share/applications");
    std::fs::create_dir_all(&apps)?;
    std::fs::write(apps.join("org.privateai.Config.desktop"), desktop_entry())?;

    let icon = stage_icon(&repo.join("resources/llama.ico"), &stage)?;
    if icon.is_none() {
        ui::warn("No PNG frame in resources/llama.ico: the desktop entry ships without an icon");
    }

    // ── One tree per package ────────────────────────────────────────────
    let roots = pkg_work.join("roots");
    let present = components::split(&stage, &roots)?;

    // A runtime's version is its SDK's, so a new llama.cpp alone leaves it as it
    // is. NCCL has a version of its own and travels in the CUDA package.
    let sdk_version = |dist: Option<PathBuf>| {
        dist.as_deref()
            .and_then(components::dist_version)
            .unwrap_or_else(|| pkg_version.clone())
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
        Component::Config | Component::Llama => pkg_version.clone(),
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
        let opt = root.join(OPT_DIR);
        let dir = opt.join("share/licenses").join(c.suffix());
        let entries = licenses::stage_in(
            &dir,
            &licenses::libraries_under(&opt),
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
        let mut copyright = format!(
            "{} {} is part of {MONOLITH} {fw_version}, distributed under the MIT licence:\n\n",
            c.package(),
            version_of(*c)
        );
        copyright.push_str(&std::fs::read_to_string(repo.join("LICENSE"))?);
        copyright.push_str(&format!(
            "\nIt was built with llama.cpp {llama_build}, whose licence follows.\n"
        ));
        copyright.push_str(&format!(
            "The libraries under /{RT_DIR} are redistributed unmodified, each under its own licence.\n"
        ));
        copyright.push_str(&format!(
            "The full texts are in /{OPT_DIR}/share/licenses/{}:\n\n",
            c.suffix()
        ));
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
        for dir in [opt.join("lib"), opt.join(".kpack")] {
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

    // No maintainer scripts: the package ships no config directory,
    // pai-config writes ~/.config/private-ai as whoever runs it.

    let repo_git =
        |args: &[&str]| output(cmd("git").arg("-C").arg(&repo).args(args)).unwrap_or_default();
    let homepage = {
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
                format!("{MONOLITH} <nobody@localhost>")
            } else {
                m
            }
        }
    };

    // The libraries of all five, which no package provides to the system or
    // requires from it.
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
            homepage: &homepage,
            depends_on_llama: (*c == Component::Config).then_some(pkg_version.as_str()),
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

/// The short and the long description of a package.
fn texts(c: Component, llama_build: &str) -> (&'static str, String) {
    match c {
        Component::Config => (
            "pai-config: GUI and CLI configurator for llama-server",
            "pai-config, the GUI + CLI configurator for server.ini and the per-model\n\
             presets.ini (kept per user, in ~/.config/private-ai), with the benchmark method.\n"
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
        Component::Vulkan => (
            "Vulkan loader for llama.cpp",
            "The Vulkan loader the Vulkan backend of llama.cpp loads. Only a Vulkan driver has\n\
             to be installed besides it.\n"
                .to_string(),
        ),
    }
}

/// `-l<dir>` for every directory holding a library of the five, so
/// dpkg-shlibdeps resolves a library another package carries.
fn private_library_dirs(roots: &Path, present: &[Component]) -> BTreeSet<String> {
    present
        .iter()
        .flat_map(|c| walk(&components::root(roots, *c).join(OPT_DIR)))
        .filter(|p| is_file(p) && name_of(p).contains(".so"))
        .filter_map(|p| p.parent().map(|d| format!("-l{}", d.display())))
        .collect()
}

/// The names of every library of the five, as a regular expression's alternatives.
fn library_stems(roots: &Path, present: &[Component]) -> String {
    let stems: BTreeSet<String> = present
        .iter()
        .flat_map(|c| walk(&components::root(roots, *c).join(OPT_DIR)))
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
    /// The llama.cpp package version this one requires exactly.
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
    let mut conflicts = vec![MONOLITH.to_string()];
    if f.component == Component::Llama {
        conflicts.push("llama.cpp".into());
    }
    (
        recommends.join(", "),
        breaks.join(", "),
        conflicts.join(", "),
    )
}

/// The `.rpm` relations, one tag per line.
fn rpm_relations(f: &Fields) -> String {
    let mut lines = vec![format!("Obsoletes:      {MONOLITH} <= {MONOLITH_LAST}")];
    if let Some(v) = f.depends_on_llama {
        lines.push(format!(
            "Requires:       {} = {v}",
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
    // library paths). libggml-cuda.so is left out of the scan: it links
    // libcuda.so.1, which is the NVIDIA DRIVER's and no Debian package's
    // (dpkg-shlibdeps fails on a library it cannot find), and the package
    // must install on a machine without an NVIDIA card.
    let scan: Vec<String> = walk(&root.join(OPT_DIR))
        .iter()
        .filter(|p| is_file(p) && (name_of(p).contains(".so") || user_executable(p)))
        .filter(|p| is_elf(p))
        .filter(|p| {
            let n = name_of(p);
            n != "libggml-cuda.so" && !n.starts_with("libcu")
        })
        .map(|p| format!("-e{}", p.display()))
        .collect();
    let mut depends = String::new();
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
                depends = drop_libvulkan(line);
            }
        }
    }
    // A runtime asks for nothing it does not bring along; the two that hold our
    // programs need the system's C and C++ libraries at the least.
    if depends.is_empty() && !f.component.is_runtime() {
        depends =
            "libc6, libstdc++6, libgcc-s1, libssl3t64 | libssl3, libfontconfig1, libfreetype6"
                .into();
        ui::warn("dpkg-shlibdeps gave no answer; using the fixed Depends list");
    }
    if let Some(v) = f.depends_on_llama {
        let exact = format!("{} (= {v})", Component::Llama.package());
        depends = if depends.is_empty() {
            exact
        } else {
            format!("{depends}, {exact}")
        };
    }

    std::fs::create_dir_all(root.join("DEBIAN"))?;
    let installed_size = output(cmd("du").args(["-sk", "--exclude=DEBIAN"]).arg(root))
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .unwrap_or_default();
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
    std::fs::write(
        root.join("DEBIAN/control"),
        format!(
            "Package: {package}\nVersion: {}\nArchitecture: {arch}\nMaintainer: {maintainer}\nInstalled-Size: {installed_size}\n{}{}{}Conflicts: {conflicts}\nReplaces: {MONOLITH}\nSection: utils\nPriority: optional\n{}Description: {}\n{body}",
            f.version,
            line("Depends", &depends),
            line("Recommends", &recommends),
            line("Breaks", &breaks),
            line("Homepage", f.homepage),
            f.summary,
        ),
    )?;
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
    // rpm's automatic dependency scan is kept (it yields soname requirements
    // any rpm distro resolves), minus what it must not see: every library the
    // five packages carry (not "provided" to the system, so not "required"
    // from it either) and libcuda, which is the NVIDIA driver's.
    let url = if f.homepage.is_empty() {
        String::new()
    } else {
        format!("URL:            {}\n", f.homepage)
    };
    // Only what this package holds: a %files line without a match stops rpmbuild.
    let mut files = format!("/{OPT_DIR}\n/usr/share/doc/{package}\n");
    if root.join("usr/bin").is_dir() {
        files.push_str("/usr/bin/*\n");
    }
    if root.join("usr/share/applications").is_dir() {
        files.push_str("/usr/share/applications/org.privateai.Config.desktop\n");
    }
    if root.join("usr/share/icons").is_dir() {
        files.push_str("/usr/share/icons/hicolor/*/apps/org.privateai.Config.png\n");
    }
    let spec = pkg_work.join(format!("{package}.spec"));
    std::fs::write(
        &spec,
        format!(
            r#"Name:           {package}
Version:        {version}
Release:        1
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
%global __provides_exclude_from ^/{OPT_DIR}/.*$
%global __requires_exclude ^({libraries}|libcuda)\\.so

%description
{description}
%install
cp -a "{root}/." "%{{buildroot}}/"

%files
%defattr(-,root,root,-)
{files}"#,
            version = f.version,
            summary = f.summary,
            description = f.description,
            relations = rpm_relations(f),
            root = root.display(),
        ),
    )?;
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

    #[test]
    fn llama_cpp_recommends_its_runtimes_and_refuses_any_other_version() {
        let runtimes = [(Component::Rocm, "10.1.0".to_string())];
        let f = fields(Component::Llama, &runtimes, None);
        let (recommends, breaks, conflicts) = deb_relations(&f);
        assert_eq!(recommends, "llama-cpp-framework-rocm (= 10.1.0)");
        assert_eq!(
            breaks,
            "llama-cpp-framework-rocm (<< 10.1.0), llama-cpp-framework-rocm (>> 10.1.0)"
        );
        assert_eq!(conflicts, "llama-cpp-framework, llama.cpp");
        let rpm = rpm_relations(&f);
        assert!(
            rpm.contains("Recommends:     llama-cpp-framework-rocm = 10.1.0"),
            "{rpm}"
        );
        assert!(
            rpm.contains("Conflicts:      llama-cpp-framework-rocm < 10.1.0"),
            "{rpm}"
        );
        assert!(
            rpm.contains("Conflicts:      llama-cpp-framework-rocm > 10.1.0"),
            "{rpm}"
        );
        assert!(
            rpm.contains("Obsoletes:      llama-cpp-framework <= 2.2.0\n"),
            "{rpm}"
        );
    }

    #[test]
    fn the_configurator_requires_its_llama_cpp_exactly() {
        let f = fields(Component::Config, &[], Some("2.3.0+llama.cpp.v0.9.0"));
        assert!(rpm_relations(&f)
            .contains("Requires:       llama-cpp-framework-llama = 2.3.0+llama.cpp.v0.9.0"));
        let (recommends, breaks, _) = deb_relations(&f);
        assert!(recommends.contains("libxkbcommon0"), "{recommends}");
        assert!(breaks.is_empty());
    }

    #[test]
    fn a_runtime_requires_nothing_of_the_others() {
        let f = fields(Component::Cuda, &[], None);
        assert_eq!(
            deb_relations(&f),
            (
                String::new(),
                String::new(),
                "llama-cpp-framework".to_string()
            )
        );
        assert_eq!(
            rpm_relations(&f),
            "Obsoletes:      llama-cpp-framework <= 2.2.0"
        );
    }
}
