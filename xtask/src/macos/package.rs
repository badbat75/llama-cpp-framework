//! Package phase on macOS (cargo run -p xtask -- package [llama|framework]):
//! the engine's two .pkg files (llama.cpp and its Vulkan runtime) and the
//! framework's three (pai-config, pai, core with the strings every program
//! prints), or one product's (common/macos_pkg.rs and common/components.rs
//! say which file goes where; ROCm and CUDA do not exist here). Stages two
//! trees: the engine's /opt/llama.cpp (llama.cpp's executables, libraries
//! and backends in bin/, the bundled Vulkan loader and MoltenVK in lib/) and
//! the framework's /opt/private-ai (pai-config and pai in bin/, its data in
//! share/), a symlink per executable in /usr/local/bin, the MoltenVK driver
//! manifest in /usr/local/share/vulkan/icd.d, and builds them with pkgbuild
//! into ./dist/. A component package states no dependencies: `xtask install`
//! lays down the engine's first, and without the Vulkan one ggml skips that
//! backend and Metal runs.
//!
//! Self-contained: Metal is part of the system, and what Vulkan needs comes
//! out of the SDK the build linked. WHICH libraries is not a list kept here:
//! it is the `otool -L` closure of the staged files, restricted to what
//! resolves inside the SDK; anything else that is not a system library
//! (a Homebrew dylib, say) fails the phase. Every staged Mach-O gets its
//! rpaths rewritten relative to itself (`@loader_path`), no build path left,
//! and an ad hoc signature (arm64 refuses to run an unsigned, or a modified
//! and not re-signed, binary).
//!
//! The driver manifest goes where the Vulkan loader looks for it (the loader
//! does not search next to itself): /usr/local/share/vulkan/icd.d, under the
//! engine's name, with an absolute library_path into /opt/llama.cpp/lib. A
//! system-wide Vulkan SDK install adds its own MoltenVK manifest there too.

use super::build_llama::load_build_env;
use crate::common::components::{self, Component, Product};
use crate::common::licenses;
use crate::common::macos_pkg::{self, ENGINE_DIR, FRAMEWORK_DIR};
use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd, which};
use crate::common::version;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn is_macho(p: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && matches!(magic, [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe])
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// A path under the payload root.
fn under(stage: &Path, abs: &str) -> PathBuf {
    stage.join(abs.trim_start_matches('/'))
}

/// The LC_RPATH entries of a Mach-O, as written (with @loader_path).
fn rpaths(p: &Path) -> Vec<String> {
    let text = output(cmd("otool").arg("-l").arg(p)).unwrap_or_default();
    let mut out = Vec::new();
    let mut in_rpath = false;
    for l in text.lines().map(str::trim) {
        if l.starts_with("cmd ") {
            in_rpath = l == "cmd LC_RPATH";
        } else if in_rpath {
            if let Some(rest) = l.strip_prefix("path ") {
                out.push(
                    rest.rsplit_once(" (offset")
                        .map(|(p, _)| p)
                        .unwrap_or(rest)
                        .to_string(),
                );
                in_rpath = false;
            }
        }
    }
    out
}

/// The libraries a Mach-O links (otool -L, its own install name left out).
fn deps(p: &Path) -> Vec<String> {
    let text = output(cmd("otool").args(["-L"]).arg(p)).unwrap_or_default();
    let id = output(cmd("otool").args(["-D"]).arg(p))
        .and_then(|s| s.lines().nth(1).map(str::trim).map(str::to_string))
        .unwrap_or_default();
    text.lines()
        .skip(1)
        .filter_map(|l| l.trim().split(" (compatibility").next().map(str::to_string))
        .filter(|d| !d.is_empty() && *d != id)
        .collect()
}

fn is_system(dep: &str) -> bool {
    dep.starts_with("/usr/lib/") || dep.starts_with("/System/")
}

/// Where `dep` resolves for the ORIGINAL file at `orig` (its rpaths and its
/// directory, as dyld would), or None.
fn resolve(dep: &str, orig: &Path, orig_rpaths: &[String]) -> Option<PathBuf> {
    let dir = orig.parent()?;
    let loader = |s: &str| {
        s.replace("@loader_path", &dir.display().to_string())
            .replace("@executable_path", &dir.display().to_string())
    };
    if let Some(rest) = dep.strip_prefix("@rpath/") {
        return orig_rpaths
            .iter()
            .map(|r| PathBuf::from(loader(r)).join(rest))
            .find(|p| p.is_file());
    }
    let p = PathBuf::from(loader(dep));
    p.is_file().then_some(p)
}

pub fn run(product: Option<Product>, keep_staging: bool) -> Result<()> {
    let (cfg, dists) = load_build_env()?;
    let repo = util::repo_root();
    let build_root = util::build_root();
    let bin = build_root.join("llama.cpp-cmake/bin");
    let stage = build_root.join("staging");
    let out_dir = repo.join("dist");
    let llama = PathBuf::from(&cfg.llama_cpp_dir);

    let fw_version = version::framework()?;
    let llama_build = version::llama_build(&llama)?;
    if !util::is_release_tag(&llama_build) {
        bail!("llama.cpp {llama_build} is not a release (vX.Y.Z): packages are built from release tags only. Run cargo run -p xtask -- build to check out the latest release.");
    }
    ui::head(format!("Framework version: {fw_version}"));
    ui::head(format!("llama.cpp release: {llama_build}"));
    ui::head("Architecture:      arm64");

    let wanted = macos_pkg::of_product(product, cfg.minimal);
    let engine = wanted.contains(&Component::Llama);
    let framework = wanted.iter().any(|c| c.product() == Product::Framework);

    // ── Preconditions ───────────────────────────────────────────────────
    let mut needed = Vec::new();
    if engine {
        needed.push("llama-server");
    }
    if framework {
        needed.extend(["pai-config", "pai"]);
    }
    for exe in needed {
        if !util::is_executable(&bin.join(exe)) {
            bail!(
                "{exe} not found in {}. Run cargo run -p xtask -- build first.",
                bin.display()
            );
        }
    }
    if framework {
        let built = output(cmd(bin.join("pai-config")).arg("--version"))
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_default();
        if built != fw_version {
            bail!("the built pai-config is {built}, Cargo.toml says {fw_version}. Run cargo run -p xtask -- build first.");
        }
    }
    for tool in ["pkgbuild", "otool", "install_name_tool", "codesign"] {
        if which(tool).is_none() {
            bail!(
                "{tool} not found: install the Xcode Command Line Tools (xcode-select --install)"
            );
        }
    }

    // ── Stage ───────────────────────────────────────────────────────────
    let engine_tree = under(&stage, ENGINE_DIR);
    let framework_tree = under(&stage, FRAMEWORK_DIR);
    let engine_bin = engine_tree.join("bin");
    let engine_lib = engine_tree.join("lib");
    let links = under(&stage, "/usr/local/bin");
    util::remove_all(&stage)?;
    for d in [&links, &out_dir] {
        std::fs::create_dir_all(d)?;
    }
    if framework {
        crate::common::bench_share::stage(&framework_tree.join("share/benchmarks"))?;
        crate::common::strings_share::stage(&framework_tree.join("share/strings"))?;
    }

    ui::head("Staging binaries...");
    // The build's bin/ (executables, the backend dylibs and pai-config and
    // pai side by side), each file into its own tree's bin/ and only the
    // packaged product's, llama.cpp's own test-* harnesses left out. `orig`
    // remembers where each staged Mach-O came from, to resolve its @rpath
    // dependencies as the build tree does.
    let mut orig: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&bin)?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for src in entries {
        let n = name_of(&src);
        if n.starts_with("test-") || !src.is_file() {
            continue;
        }
        let at = macos_pkg::staged_path(&n);
        if !wanted.contains(&components::of(&at)) {
            continue;
        }
        let dest = under(&stage, &at);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::copy(&src, &dest).with_context(|| format!("copying {}", src.display()))?;
        if is_macho(&dest) {
            orig.push((dest, src));
        }
    }

    // ── Vulkan: MoltenVK and its manifest ──────────────────────────────
    let sdk = dists.find_vulkan();
    let sdk_canon = sdk.as_deref().and_then(|d| d.canonicalize().ok());
    let vulkan = wanted.contains(&Component::Vulkan);
    if let Some(sdk) = sdk.as_ref().filter(|_| vulkan) {
        let src_icd = super::dists::moltenvk_icd(sdk);
        let text = std::fs::read_to_string(&src_icd)
            .with_context(|| format!("reading {}", src_icd.display()))?;
        let mut json: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("parsing {}", src_icd.display()))?;
        let lib = json["ICD"]["library_path"]
            .as_str()
            .context("MoltenVK_icd.json has no ICD.library_path")?
            .to_string();
        let lib_path = src_icd.parent().unwrap().join(&lib);
        let dest = under(&stage, &macos_pkg::moltenvk_path());
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::copy(&lib_path, &dest)
            .with_context(|| format!("copying {}", lib_path.display()))?;
        orig.push((dest, lib_path));
        json["ICD"]["library_path"] = macos_pkg::moltenvk_path().into();
        let manifest = under(&stage, &macos_pkg::icd_manifest());
        std::fs::create_dir_all(manifest.parent().unwrap())?;
        std::fs::write(&manifest, serde_json::to_string_pretty(&json)? + "\n")?;
        ui::dim(format!("MoltenVK and its manifest from {}", sdk.display()));
    } else if vulkan {
        ui::warn("No Vulkan SDK in the dists: the package has no MoltenVK (Vulkan backend unusable, Metal unaffected)");
    }

    // ── Dependency closure ─────────────────────────────────────────────
    // Per tree: every non-system dependency must be staged in the file's
    // own tree or, for the engine's, come from the SDK (it is then copied
    // into the engine's lib/ under the name it is asked for and given an
    // @rpath install name). The closure grows as libraries are added.
    ui::head("Resolving dependencies...");
    let mut i = 0;
    let mut bundled = BTreeSet::new();
    while i < orig.len() {
        let (staged, src) = orig[i].clone();
        i += 1;
        let src_rpaths = rpaths(&src);
        let in_engine = staged.starts_with(&engine_tree);
        let tree = if in_engine {
            &engine_tree
        } else {
            &framework_tree
        };
        for dep in deps(&staged) {
            if is_system(&dep) {
                continue;
            }
            let base = util::basename(&dep).to_string();
            if tree.join("bin").join(&base).is_file() || tree.join("lib").join(&base).is_file() {
                if !dep.starts_with('@') {
                    run_cmd(
                        cmd("install_name_tool")
                            .args(["-change", &dep, &format!("@rpath/{base}")])
                            .arg(&staged),
                    )?;
                }
                continue;
            }
            let found = resolve(&dep, &src, &src_rpaths).and_then(|p| p.canonicalize().ok());
            let from_sdk = match (&found, &sdk_canon) {
                (Some(f), Some(s)) => in_engine && f.starts_with(s),
                _ => false,
            };
            if !from_sdk {
                bail!(
                    "{} links {dep} ({}), which is neither a system library nor from the Vulkan SDK: the package would not run elsewhere",
                    name_of(&staged),
                    found.map(|f| f.display().to_string()).unwrap_or_else(|| "not found".into())
                );
            }
            let found = found.unwrap();
            std::fs::create_dir_all(&engine_lib)?;
            let dest = engine_lib.join(&base);
            std::fs::copy(&found, &dest).with_context(|| format!("copying {}", found.display()))?;
            run_cmd(
                cmd("install_name_tool")
                    .args(["-id", &format!("@rpath/{base}")])
                    .arg(&dest),
            )?;
            if !dep.starts_with('@') {
                run_cmd(
                    cmd("install_name_tool")
                        .args(["-change", &dep, &format!("@rpath/{base}")])
                        .arg(&staged),
                )?;
            }
            bundled.insert(base);
            orig.push((dest, found));
        }
    }
    if !bundled.is_empty() {
        ui::dim(format!(
            "Bundled from the SDK: {}",
            bundled.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    // ── Rpaths and signatures ──────────────────────────────────────────
    // Build paths out, paths relative to the file in: the engine's bin/
    // finds its neighbours and lib/, lib/ finds itself, the framework's
    // bin/ its neighbours.
    ui::head("Rewriting rpaths and signing (ad hoc)...");
    for (staged, _) in &orig {
        let mut c = cmd("install_name_tool");
        for r in rpaths(staged) {
            c.args(["-delete_rpath", &r]);
        }
        c.args(["-add_rpath", "@loader_path"]);
        if staged.parent() == Some(engine_bin.as_path()) {
            c.args(["-add_rpath", "@loader_path/../lib"]);
        }
        run_cmd(c.arg(staged))?;
        run_cmd(cmd("codesign").args(["--force", "--sign", "-"]).arg(staged))?;
    }

    // ── Commands on PATH ───────────────────────────────────────────────
    let mut n_links = 0;
    for (staged, _) in &orig {
        let n = name_of(staged);
        let in_bin = staged.parent() == Some(engine_bin.as_path())
            || staged.parent() == Some(framework_tree.join("bin").as_path());
        if in_bin && !n.ends_with(".dylib") && !n.ends_with(".so") {
            std::os::unix::fs::symlink(macos_pkg::staged_path(&n), links.join(&n))?;
            n_links += 1;
        }
    }
    ui::dim(format!("{n_links} commands linked into /usr/local/bin"));

    // ── One .pkg per component ─────────────────────────────────────────
    let roots = build_root.join("package");
    util::remove_all(&roots)?;
    let present = components::split(&stage, &roots)?;
    let vulkan_version = sdk
        .as_deref()
        .and_then(components::dist_version)
        .unwrap_or_else(|| fw_version.clone());
    let mut created = Vec::new();
    for c in present.into_iter().filter(|c| wanted.contains(c)) {
        let root = components::root(&roots, c);
        let tree = if c.product() == Product::Llama {
            under(&root, ENGINE_DIR)
        } else {
            under(&root, FRAMEWORK_DIR)
        };
        licenses::stage_in(
            &tree.join("share/licenses").join(c.suffix()),
            &licenses::libraries_under(&tree),
            &licenses::Sources {
                llama_cpp: llama.clone(),
                ..Default::default()
            },
        )?;
        let version = macos_pkg::version(c, &fw_version, &llama_build, &vulkan_version);
        let pkg = out_dir.join(macos_pkg::file_name(
            c,
            &fw_version,
            &llama_build,
            &vulkan_version,
            cfg.minimal,
        ));
        let _ = std::fs::remove_file(&pkg);
        ui::head(format!("Building {}...", name_of(&pkg)));
        run_cmd(
            cmd("pkgbuild")
                .arg("--root")
                .arg(&root)
                .args([
                    "--install-location",
                    "/",
                    "--identifier",
                    &macos_pkg::identifier(c),
                    "--version",
                    &version,
                ])
                .arg(&pkg),
        )?;
        created.push(pkg);
    }
    util::remove_all(&roots)?;
    if !keep_staging {
        util::remove_all(&stage)?;
    }

    ui::say("");
    for pkg in &created {
        ui::ok(format!(
            "Package: {} ({} MiB)",
            pkg.display(),
            std::fs::metadata(pkg)?.len() / (1024 * 1024)
        ));
    }
    ui::say("");
    Ok(())
}
