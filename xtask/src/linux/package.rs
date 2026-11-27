//! Package phase (cargo run -p xtask -- package): the Linux build as eight .deb
//! and eight .rpm packages in ./dist/ (common/components.rs says which file goes
//! where): the framework's pai-config, pai, pai-bff and core (pai-manager and
//! its services), and the engine, llama.cpp, with its ROCm, CUDA and Vulkan
//! runtimes. `xtask package llama` packages the engine and its runtimes
//! alone, from llama.cpp's build tree; `xtask package framework` the
//! framework's four alone, from cargo's release dir (the llama.cpp clone only
//! for its release and licence).
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
//! No config file is in the package. The server and model profile files
//! (server@<id>.ini, models.ini, models.ini.disabled) live in
//! /etc/llama.cpp, the engine's, which its maintainer scripts fill from the
//! defaults it ships in /usr/share/private-ai/llama.cpp (each only when
//! missing) and systemd creates at the manager's start
//! (ConfigurationDirectory=), beside the framework's /etc/private-ai
//! (bff.ini); only the manager writes them. settings.ini stays
//! per user, written by pai-config in ~/.config/private-ai on first use.

use super::build_llama::load_build_env;
use super::dists;
use super::system_files;
use crate::common::components::{self, Component, Product};
use crate::common::configurator::{self, set_mode};
use crate::common::licenses;
use crate::common::ui;
use crate::common::util::{
    self, cmd, cmd_without_ld_library_path, gfx_arch, output, run as run_cmd, which,
};
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
/// The RUNPATH of every staged llama.cpp ELF file, by absolute path:
/// [`LIB_DIR`], [`RT_DIR`] and every directory of the ROCm dist mirrored
/// under [`RT_DIR`]: the loader searches only what is named here, so the
/// directory [`rocm_stage_path`] stages a runtime reached through a symlink
/// of the dist has to be named too (the dist's OpenMP runtime).
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

/// Where one runtime library of the ROCm dist is staged, relative to the
/// package root: the dist's own directory under [`RT_DIR`] (its libraries
/// find each other and their kernel files by `$ORIGIN`-relative paths), the
/// file under `soname`, the name the loader asked for. `path` is the file the
/// loader resolved, inside `hip_root`'s `lib/`.
///
/// A dist keeps some libraries as a symlink in one of the directories the
/// RUNPATH names, naming the file in a deeper one: `ldd` answers the file it
/// points at, the loader asks the link's name. Such a library is staged where
/// the link sits, the directory the loader searches, and carries the file's
/// bytes: staged where the file sits it would sit off the RUNPATH, and every
/// machine would load it by the system's copy of the same soname or not at
/// all. A link that reaches no such file (a dangling one) is not the loader's
/// way to it: the file keeps its own place.
fn rocm_stage_path(hip_root: &Path, path: &Path, soname: &str) -> PathBuf {
    let lib = hip_root.join("lib");
    // The directory `dir` sits in, mirrored under the runtime dir, with the
    // file named `soname` in it.
    let mirrored = |dir: &Path| {
        let rel = dir
            .strip_prefix(&lib)
            .ok()
            .and_then(|r| r.parent())
            .unwrap_or(Path::new(""));
        PathBuf::from(RT_DIR).join(rel).join(soname)
    };
    // The link is sought in every directory of the dist above the file, the
    // deepest first: the one resolving to this file is the way the loader
    // reached it.
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut dir = path.parent().filter(|d| d.starts_with(&lib));
    while let Some(d) = dir {
        let link = d.join(soname);
        if std::fs::symlink_metadata(&link).is_ok_and(|m| m.is_symlink())
            && std::fs::canonicalize(&link).is_ok_and(|t| t == real)
        {
            return mirrored(&link);
        }
        dir = d.parent().filter(|p| p.starts_with(&lib));
    }
    mirrored(path)
}

/// A read-only data directory (`benchmarks`, `strings`,
/// `licenses/<component>`), relative to the package root.
fn share(sub: &str) -> String {
    format!("usr/share/private-ai/{sub}")
}

/// The version file of one package: its path under the staging root and its
/// text, `versions/<package>` under the engine's share folder, one line, the
/// version this package carries. The engine's and each runtime's own, each
/// owned by the package its name names (`components::of`), in the folder the
/// web service reads (`pai_core::bff_cfg::versions_dir`).
fn versions_file(c: Component, version: &str) -> (String, String) {
    (
        format!("{}{}", components::ENGINE_VERSIONS_DIR, c.package()),
        format!("{version}\n"),
    )
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
    // Those under /usr/share/private-ai (the engine's defaults) are already
    // taken by its line above (rpmbuild warns of a file listed twice).
    for (rel, _) in system_files::files() {
        if root.join(rel).is_file() && !rel.starts_with("usr/share/private-ai/") {
            files.push_str(&format!("/{rel}\n"));
        }
    }
    // The target's `multi-user.target.wants/` link is a symlink, so it is not
    // one of [`system_files::files`]: `stage` links it. rpm packs the path as
    // the symlink it is.
    if root.join(system_files::TARGET_LINK).is_symlink() {
        files.push_str(&format!("/{}\n", system_files::TARGET_LINK));
    }
    // The PAM service is the administrator's to edit: an update keeps a local
    // edit (rpm-spec(5) `%config(noreplace)`).
    if root.join(system_files::PAM_SERVICE).is_file() {
        files.push_str(&format!(
            "%config(noreplace) /{}\n",
            system_files::PAM_SERVICE
        ));
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

/// The first build output a package run of `product` (both when None) needs
/// and does not find under `build_root` as an executable: the engine needs
/// llama.cpp's build tree (`llama.cpp-cmake/bin/llama-server`), the framework
/// cargo's release build (`cargo-target/release/`: pai-config, pai,
/// pai-manager, pai-bff) and nothing of llama.cpp's build tree.
pub(crate) fn missing_input(build_root: &Path, product: Option<Product>) -> Option<PathBuf> {
    let mut needed = Vec::new();
    if product != Some(Product::Framework) {
        needed.push(build_root.join("llama.cpp-cmake/bin/llama-server"));
    }
    if product != Some(Product::Llama) {
        let release = build_root.join("cargo-target/release");
        needed.extend(FRAMEWORK_EXES.map(|exe| release.join(exe)));
    }
    needed.into_iter().find(|p| !util::is_executable(p))
}

/// The framework's executables, staged from cargo's release dir.
const FRAMEWORK_EXES: [&str; 4] = ["pai-config", "pai", "pai-manager", "pai-bff"];

/// Packages `product` (both when None) into ./dist/; the other product's
/// packages there are left as they are.
pub fn run(
    product: Option<Product>,
    want_deb: bool,
    want_rpm: bool,
    keep_staging: bool,
) -> Result<()> {
    let build_root = util::build_root();
    if let Some(missing) = missing_input(&build_root, product) {
        bail!(
            "{} not found. Run cargo run -p xtask -- build first.",
            missing.display()
        );
    }
    let wanted = components::of_product(product);
    let engine = wanted.contains(&Component::Llama);
    let framework = wanted.contains(&Component::Core);
    let (cfg, dists) = load_build_env()?;
    let repo = util::repo_root();
    let build_dir = build_root.join("llama.cpp-cmake");
    let release = build_root.join("cargo-target/release");
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
    // The build outputs are there (`missing_input`, above).
    let bin = build_dir.join("bin");
    // A version bump without a rebuild would ship the OLD configurator in a
    // package named after the new version.
    if framework {
        let built = output(cmd(release.join("pai-config")).arg("--version"))
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_default();
        if built != fw_version {
            bail!("the built pai-config is {built}, Cargo.toml says {fw_version}. Run cargo run -p xtask -- build first.");
        }
    }
    if engine && which("patchelf").is_none() {
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
    // The engine straight from llama.cpp's bin/, where the build leaves
    // executables, libraries and backends side by side: each goes where
    // `place` says (the executables to /usr/bin, the libraries and backends to
    // /usr/lib/private-ai), the library symlinks kept as symlinks.
    // llama.cpp's own test-* harnesses stay out: they are its unit tests, and
    // every executable staged here is a command in /usr/bin. The copies of
    // pai-config and pai the build leaves there stay out too: the framework
    // comes from cargo's release dir.
    let llama_files = |p: &PathBuf| {
        let name = name_of(p);
        !configurator::NEXT_TO_SERVER.contains(&name.as_str())
    };
    if engine {
        let (tests, keep): (Vec<PathBuf>, Vec<PathBuf>) = entries(&bin)
            .into_iter()
            .filter(llama_files)
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
        if !tests.is_empty() {
            ui::dim(format!("Pruned {} llama.cpp test binaries", tests.len()));
        }
    }
    if framework {
        // pai-config, pai, pai-manager and pai-bff, from cargo's release dir.
        let bff = build_root.join("cargo-target/release/pai-bff");
        std::fs::copy(&bff, stage.join(place("pai-bff")))
            .with_context(|| format!("copying {}", bff.display()))?;
        for exe in ["pai-config", "pai", "pai-manager"] {
            let from = release.join(exe);
            std::fs::copy(&from, stage.join(place(exe)))
                .with_context(|| format!("copying {}", from.display()))?;
        }
        crate::common::bench_share::stage(&stage.join(share("benchmarks")))?;
        crate::common::strings_share::stage(&stage.join(share("strings")))?;
    }

    let hip_root = cfg.hip_path.clone();
    if engine {
        // ── Bundle the GPU runtimes ─────────────────────────────────────────
        // What the backends load is copied out of the dists this build linked
        // against, so the package runs on a machine that has the GPU drivers and
        // nothing else: no ROCm, no CUDA Toolkit, no Vulkan SDK.
        //
        // WHICH libraries is not a list kept here: it is the ldd closure of the
        // staged files, restricted to what resolves inside a dist. Each library
        // lands under the name it is asked for (its soname), with the bytes of
        // the file behind that name: no dist symlink is copied, and a library
        // the loader reaches through one is staged in the directory of the link
        // it sits in, which the RUNPATH names (rocm_stage_path).
        //
        // The ROCm dist keeps its layout under lib/ (lib/, lib/llvm/lib/,
        // lib/rocm_sysdeps/lib/, lib/rocblas/library/, ...): its libraries find
        // each other and their kernel files by $ORIGIN-relative paths, so a tree
        // that mirrors the dist needs no patching of anything AMD shipped. That is
        // also where the compiler's own runtime comes from: the CPU backends are
        // built with the dist's clang and link ITS OpenMP runtime
        // (lib/llvm/lib/libomp.so). CUDA, NCCL and Vulkan libraries have no such
        // structure and go flat into lib/.
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
        // Each file's dependencies are asked of the loader with LD_LIBRARY_PATH
        // removed: the caller's LD_LIBRARY_PATH can point at another dist, and
        // the closure must resolve inside the dists this build is configured
        // with, by the RUNPATH the binaries carry.
        let mut closure = BTreeSet::new();
        for f in entries(&bin).into_iter().filter(llama_files) {
            if !is_elf(&f) || name_of(&f).starts_with("test-") {
                continue;
            }
            let Some(ldd) = output(cmd_without_ld_library_path("ldd").arg(&f)) else {
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
                (
                    stage.join(rocm_stage_path(
                        Path::new(&hip_root),
                        Path::new(path),
                        soname,
                    )),
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
            if stage.join(place(backend)).exists()
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
        // exactly as their vendor built them, and the framework's executables
        // link nothing of llama.cpp and get none (below). LD_LIBRARY_PATH still
        // wins over RUNPATH.
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
            if !is_elf(&f) || FRAMEWORK_EXES.contains(&name_of(&f).as_str()) {
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
        let after = size();
        ui::dim(format!(
            "{executables} executables, {before} MiB -> {after} MiB stripped"
        ));
        ui::dim(format!("Bundled runtimes: {} MiB", util::du("-sm", &rt)));
    }
    // The framework's executables link nothing of llama.cpp: stripped, no
    // RUNPATH.
    if framework {
        for name in FRAMEWORK_EXES {
            let staged = stage.join(place(name));
            run_cmd(cmd("strip").arg("--strip-unneeded").arg(&staged))?;
            set_mode(&staged, 0o755)?;
        }
    }

    // ── Desktop entry, icon ─────────────────────────────────────────────
    if framework {
        let apps = stage.join("usr/share/applications");
        std::fs::create_dir_all(&apps)?;
        std::fs::write(apps.join("org.privateai.Config.desktop"), desktop_entry())?;

        let icon = stage_icon(&repo.join("resources/llama.ico"), &stage)?;
        if icon.is_none() {
            ui::warn(
                "No PNG frame in resources/llama.ico: the desktop entry ships without an icon",
            );
        }
    }

    // ── The services' system files ──────────────────────────────────────
    system_files::stage(&stage)?;

    // ── One tree per package ────────────────────────────────────────────
    let roots = pkg_work.join("roots");
    // Only the product asked for is packaged.
    let present: Vec<Component> = components::split(&stage, &roots)?
        .into_iter()
        .filter(|c| wanted.contains(c))
        .collect();

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

    // The version of the engine and of each runtime it ships, one file per
    // package in the folder the web service reads, named after the package and
    // holding the version this package carries (slice 13). Each file goes to
    // the package it names (`components::of`), so an engine upgrade rewrites
    // only `llama.cpp` and a new SDK only its runtime's.
    for c in present
        .iter()
        .filter(|c| **c == Component::Llama || c.is_runtime())
    {
        let (rel, text) = versions_file(*c, &version_of(*c));
        let dest = components::root(&roots, *c).join(&rel);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::write(&dest, text)?;
    }

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
            c.product(),
            licenses::component_ships_web_ui(*c),
        )?;
        licence_fields.push((
            *c,
            licenses::rpm_license(&entries, c.product(), licenses::component_ships_web_ui(*c)),
        ));

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
                "{} {} is part of {} {fw_version}, distributed under the GNU Affero General \
                 Public License, version 3 or later:\n\n",
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
             models.ini, which the manager keeps in /etc/llama.cpp.\n"
                .to_string(),
        ),
        Component::Cli => (
            "pai: command line for llama-server, its profiles and benchmarks",
            "pai, the command line of the framework: server.ini and the per-model models.ini\n\
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
            "pai-manager, the framework's service: it keeps server.ini and models.ini in\n\
             /etc/llama.cpp and runs llama-server as a systemd service, for pai-config,\n\
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

/// The framework's version, the bound of the engine's Breaks and Conflicts on
/// the core: read from the workspace, the same read `run` does before staging
/// (which stops the phase when it fails).
fn framework_version() -> String {
    version::framework().expect("the package phase reads the framework version first")
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
    // RS2: the instance unit's `ExecStartPost= ... wait-ready` carries no `-`
    // prefix, so a unit of this package beside a manager with no `wait-ready`
    // would fail every start: the engine breaks a framework core below the
    // framework version (Debian Policy 7.4: Breaks is the reverse of Depends).
    // It requires nothing of the framework: the unit's
    // ConditionFileIsExecutable= skips the command where there is no manager,
    // so the engine installs and runs alone.
    if f.component == Component::Llama {
        breaks.push(format!(
            "{} (<< {})",
            Component::Core.package(),
            framework_version()
        ));
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

/// A version as rpm spells it: (Epoch, Version, Release). The engine's
/// `1:1.0.0+pai2` is epoch 1, version 1.0.0 and our build revision `pai2` as
/// the release; a version with no epoch and no `+` build revision has Release
/// [`RPM_RELEASE`].
fn evr(version: &str) -> (Option<&str>, &str, &str) {
    let (epoch, rest) = match version.split_once(':') {
        Some((e, rest)) => (Some(e), rest),
        None => (None, version),
    };
    match rest.split_once('+') {
        Some((version, release)) => (epoch, version, release),
        None => (epoch, rest, RPM_RELEASE),
    }
}

/// The three parts as rpm writes one version: `1:1.0.0-pai2`. rpm reads a
/// relation's value as an epoch-version-release, so a `+` never appears in one.
fn evr_text((epoch, version, release): (Option<&str>, &str, &str)) -> String {
    format!(
        "{}{version}-{release}",
        epoch.map(|e| format!("{e}:")).unwrap_or_default()
    )
}

/// A package's version as rpm spells it: (Epoch, Version, Release). Only the
/// engine carries an epoch and a build revision; every other package has no
/// epoch and Release [`RPM_RELEASE`].
fn rpm_evr<'a>(f: &Fields<'a>) -> (Option<&'a str>, &'a str, &'a str) {
    if f.component != Component::Llama {
        return (None, f.version, RPM_RELEASE);
    }
    evr(f.version)
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
        lines.push(format!("Provides:       {old} = {}", evr_text(rpm_evr(f))));
    }
    for c in exact_requires(f) {
        lines.push(format!("Requires:       {} = {}", c.package(), f.version));
    }
    if let Some(min) = f.depends_on_llama {
        lines.push(format!(
            "Requires:       {} >= {}",
            Component::Llama.package(),
            evr_text(evr(min))
        ));
    }
    for (c, v) in f.runtimes {
        lines.push(format!("Recommends:     {} = {v}", c.package()));
        lines.push(format!("Conflicts:      {} < {v}", c.package()));
        lines.push(format!("Conflicts:      {} > {v}", c.package()));
    }
    if f.component == Component::Llama {
        lines.push("Conflicts:      llama-cpp".into());
        // RS2: the same relation as rpm spells it (rpm-dependencies(7):
        // Conflicts is the pair relation, and rpm has no Breaks), keeping a
        // new unit off a manager with no `wait-ready`.
        lines.push(format!(
            "Conflicts:      {} < {}",
            Component::Core.package(),
            framework_version()
        ));
    }
    if matches!(f.component, Component::Core | Component::Bff) {
        // What the scriptlets run (Fedora's %systemd_requires, plus %pre and
        // %posttrans).
        for tag in ["pre", "post", "preun", "postun", "posttrans"] {
            lines.push(format!("Requires({tag}): systemd"));
        }
    }
    if f.component == Component::Llama {
        // The engine's scriptlets run systemctl too; it has no %post.
        for tag in ["pre", "preun", "postun", "posttrans"] {
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
     Comment=Configure and run llama-server and its model profiles\n\
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
    // The .deb text of the PAM service goes with DEBIAN/: an .rpm of the same
    // tree writes its own.
    util::remove_all(&root.join(system_files::PAM_SERVICE))?;
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
    system_files::write_rpm_files(root, f.component)?;
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
#[path = "package_tests.rs"]
mod tests;
