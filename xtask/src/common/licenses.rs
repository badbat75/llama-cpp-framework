//! The licence texts a package carries in `<root>/share/licenses/`: this
//! project's, llama.cpp's, and one for every library bundled beside them, with
//! `README.txt` saying which text covers which library.
//!
//! Which licence is not guessed: a bundled library this table does not name
//! stops the package, so a new library in a dist gets read before it ships. The
//! texts come from the dist when it has them (they then match the version
//! bundled) and otherwise from `installer/licenses/`, whose README says where
//! each one was taken from.

use super::{ui, util};
#[cfg(target_os = "linux")]
use crate::common::components::Component;
use crate::Product;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Where the text of a library's licence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `share/doc/<name>/LICENSE*` of the ROCm dist.
    RocmDoc(&'static str),
    /// `installer/licenses/<name>.txt` of this repo.
    Vendored(&'static str),
    /// `LICENSE` of the CUDA dist: NVIDIA's terms, not an open-source licence.
    CudaEula,
    /// `license.txt` of the OpenSSL install.
    OpenSsl,
}

/// The file NVIDIA's terms are installed as.
pub const CUDA_EULA_FILE: &str = "NVIDIA-CUDA-EULA.txt";

/// The web UI's own texts, vendored beside the ones the table names: its two
/// fonts, its icon set, the React it is built on. Staged only for the package
/// that ships the web UI, and named by `README.txt` only there.
const WEB_UI_TEXTS: &[&str] = &[
    "atkinson-hyperlegible-next.txt",
    "atkinson-hyperlegible-mono.txt",
    "lucide.txt",
    "react.txt",
];

/// Does this Linux component's package ship the web UI, and so carry its
/// licence texts: the bff's package, no other. Every call of [`stage_in`] and
/// [`rpm_license`] passes this, never an expression of its own.
#[cfg(target_os = "linux")]
pub fn component_ships_web_ui(component: Component) -> bool {
    component == Component::Bff
}

/// Does this Windows product's MSI ship the web UI, and so carry its licence
/// texts: the framework's MSI holds pai-bff.exe, llama.cpp's does not. Every
/// call of [`stage`] and [`installer_page`] passes this, never an expression
/// of its own.
#[cfg(any(windows, test))]
pub fn product_ships_web_ui(product: Product) -> bool {
    product == Product::Framework
}

/// Library name prefixes, without `lib` and the version; the first match wins,
/// so a longer name goes before the one it starts with.
const TABLE: &[(&str, Origin)] = &[
    ("hipblaslt", Origin::RocmDoc("hipblaslt")),
    ("hipblas", Origin::RocmDoc("hipblas")),
    ("rocblas", Origin::RocmDoc("rocblas")),
    ("rocsolver", Origin::RocmDoc("rocsolver")),
    ("origami", Origin::Vendored("rocm-origami")),
    ("tensilelite-host", Origin::Vendored("rocm-tensilelite")),
    ("amd_comgr", Origin::RocmDoc("amd_comgr")),
    ("rocm-core", Origin::RocmDoc("rocm-core")),
    ("amdhip64", Origin::Vendored("rocm-clr")),
    ("rocm_kpack", Origin::Vendored("rocm-kpack")),
    ("hsa-runtime64", Origin::Vendored("rocm-rocr-runtime")),
    ("amd_smi", Origin::Vendored("rocm-amdsmi")),
    (
        "rocprofiler-register",
        Origin::Vendored("rocm-rocprofiler-register"),
    ),
    ("roctx64", Origin::Vendored("rocm-roctracer")),
    ("rccl", Origin::Vendored("rocm-rccl")),
    ("rocroller", Origin::Vendored("rocm-rocroller")),
    ("rocm_sysdeps_bz2", Origin::Vendored("bzip2")),
    ("rocm_sysdeps_drm", Origin::Vendored("libdrm")),
    ("rocm_sysdeps_elf", Origin::Vendored("elfutils")),
    ("rocm_sysdeps_liblzma", Origin::Vendored("xz")),
    ("rocm_sysdeps_mnl", Origin::Vendored("libmnl")),
    ("rocm_sysdeps_nl", Origin::Vendored("libnl")),
    ("rocm_sysdeps_numa", Origin::Vendored("numactl")),
    ("rocm_sysdeps_zstd", Origin::Vendored("zstd")),
    ("rocm_sysdeps_z", Origin::Vendored("zlib")),
    ("LLVM", Origin::Vendored("llvm")),
    ("clang-cpp", Origin::Vendored("llvm")),
    ("omp", Origin::Vendored("llvm")),
    // ggml-cuda links NVIDIA's static runtime, so it needs the terms too.
    ("ggml-cuda", Origin::CudaEula),
    ("cublas", Origin::CudaEula),
    ("cudart", Origin::CudaEula),
    ("nccl", Origin::Vendored("nccl")),
    ("vulkan", Origin::Vendored("vulkan-loader")),
    ("MoltenVK", Origin::Vendored("moltenvk")),
    ("crypto", Origin::OpenSsl),
    ("ssl", Origin::OpenSsl),
];

/// `libcublas.so.13` -> `cublas`, `amdhip64_7.dll` -> `amdhip64_7`.
fn stem(file: &str) -> &str {
    let name = file.strip_prefix("lib").unwrap_or(file);
    name.split('.').next().unwrap_or(name)
}

pub fn origin(file: &str) -> Option<Origin> {
    let stem = stem(file);
    TABLE
        .iter()
        .find(|(prefix, _)| stem.starts_with(prefix))
        .map(|(_, origin)| *origin)
}

/// llama.cpp's own libraries: its licence, staged for every package, covers them.
fn is_llama_cpp(file: &str) -> bool {
    let stem = stem(file);
    ["ggml", "llama", "mtmd"]
        .iter()
        .any(|own| stem.starts_with(own))
}

/// Where a package's texts come from. What a package does not bundle stays None.
#[derive(Debug, Default)]
pub struct Sources {
    pub llama_cpp: PathBuf,
    pub rocm: Option<PathBuf>,
    pub cuda: Option<PathBuf>,
    pub openssl: Option<PathBuf>,
    /// The web UI's own texts; None is `installer/licenses/`.
    pub web_ui_licenses: Option<PathBuf>,
}

impl Sources {
    /// The source file of a licence and the name it is installed under.
    fn locate(&self, origin: Origin) -> Option<(PathBuf, String)> {
        match origin {
            Origin::RocmDoc(name) => {
                let dir = self.rocm.as_ref()?.join("share").join("doc").join(name);
                let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
                    .ok()?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("LICENSE"))
                    })
                    .collect();
                found.sort();
                Some((found.into_iter().next()?, format!("rocm-{name}.txt")))
            }
            Origin::Vendored(name) => {
                let file = format!("{name}.txt");
                Some((
                    util::repo_root()
                        .join("installer")
                        .join("licenses")
                        .join(&file),
                    file,
                ))
            }
            Origin::CudaEula => Some((
                self.cuda.as_ref()?.join("LICENSE"),
                CUDA_EULA_FILE.to_string(),
            )),
            Origin::OpenSsl => Some((
                self.openssl.as_ref()?.join("license.txt"),
                "openssl.txt".to_string(),
            )),
        }
    }

    /// The web UI's text `file`: `web_ui_licenses` when it names a directory,
    /// the repo's `installer/licenses/` otherwise.
    fn web_ui_text(&self, file: &str) -> PathBuf {
        self.web_ui_licenses
            .clone()
            .unwrap_or_else(|| util::repo_root().join("installer").join("licenses"))
            .join(file)
    }
}

/// Which text covers which library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub file: String,
    pub libraries: Vec<String>,
}

/// Group `(library, text)` pairs by text, in the order of the file names.
fn group(pairs: Vec<(String, String)>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for (library, file) in pairs {
        match entries.iter_mut().find(|e| e.file == file) {
            Some(e) => e.libraries.push(library),
            None => entries.push(Entry {
                file,
                libraries: vec![library],
            }),
        }
    }
    entries.sort_by(|a, b| a.file.cmp(&b.file));
    entries.iter_mut().for_each(|e| e.libraries.sort());
    entries
}

/// The libraries staged under `root`, at any depth, by file name. A symlink is
/// another name for a library already listed.
pub fn libraries_under(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                dirs.push(e.path());
            } else if kind.is_file() && is_library(&name) {
                found.push(name);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn is_library(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with(".dll")
        || lower.ends_with(".dylib")
        || lower.ends_with(".so")
        || lower.contains(".so.")
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    std::fs::copy(from, to).with_context(|| format!("copying the licence {}", from.display()))?;
    Ok(())
}

/// Stage every text into `<share>/licenses`. `libraries` is what the package
/// bundles, by file name; llama.cpp's own files in it are accepted as they are.
/// `product` says which product's package this is: only the framework's carries
/// the framework's own text. `web_ui` says the package ships the web UI, and so
/// carries its texts.
#[cfg(any(windows, test))]
pub fn stage(
    share: &Path,
    libraries: &[String],
    sources: &Sources,
    product: Product,
    web_ui: bool,
) -> Result<Vec<Entry>> {
    stage_in(&share.join("licenses"), libraries, sources, product, web_ui)
}

/// [`stage`] into `dir` itself: split packages each keep their own directory,
/// since no two packages can own the same file.
pub fn stage_in(
    dir: &Path,
    libraries: &[String],
    sources: &Sources,
    product: Product,
    web_ui: bool,
) -> Result<Vec<Entry>> {
    std::fs::create_dir_all(dir)?;
    let repo = util::repo_root();
    if product == Product::Framework {
        copy(&repo.join("LICENSE"), &dir.join("private-ai-framework.txt"))?;
    }
    copy(
        &repo
            .join("crates")
            .join("config")
            .join("ui")
            .join("fonts")
            .join("LICENSE"),
        &dir.join("fluent-system-icons.txt"),
    )?;
    if web_ui {
        for file in WEB_UI_TEXTS {
            copy(&sources.web_ui_text(file), &dir.join(file))?;
        }
    }
    copy(
        &sources.llama_cpp.join("LICENSE"),
        &dir.join("llama.cpp.txt"),
    )?;
    // What llama.cpp itself embeds (LICENSE-jsonhpp and the like).
    if let Ok(rd) = std::fs::read_dir(sources.llama_cpp.join("licenses")) {
        for e in rd.flatten().filter(|e| e.path().is_file()) {
            let name = e.file_name().to_string_lossy().replace("LICENSE-", "");
            copy(
                &e.path(),
                &dir.join(format!("llama.cpp-{}.txt", name.trim_end_matches(".txt"))),
            )?;
        }
    }

    let mut pairs = Vec::new();
    for library in libraries {
        let Some(origin) = origin(library) else {
            if is_llama_cpp(library) {
                continue;
            }
            bail!(
                "{library} is bundled but xtask/src/common/licenses.rs names no licence for it: read its licence, add it to the table, then package again"
            );
        };
        let Some((from, file)) = sources.locate(origin).filter(|(from, _)| from.is_file()) else {
            bail!("{library} is bundled but its licence was not found ({origin:?}); refusing to ship it without");
        };
        copy(&from, &dir.join(&file))?;
        pairs.push((library.clone(), file));
    }
    let entries = group(pairs);
    std::fs::write(dir.join("README.txt"), readme(&entries, product, web_ui))?;
    ui::dim(format!(
        "Licences of {} libraries in share/licenses",
        libraries.len()
    ));
    Ok(entries)
}

fn listing(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|e| format!("{}: {}\n", e.file, e.libraries.join(", ")))
        .collect()
}

fn readme(entries: &[Entry], product: Product, web_ui: bool) -> String {
    // The two products speak for themselves: the framework names its AGPL
    // and the file holding it; a llama.cpp package names llama.cpp's MIT,
    // and says nothing of a licence it does not ship.
    let mut text = if product == Product::Framework {
        String::from(
            "The Private AI Framework is under the GNU Affero General Public License,\n\
             version 3 or later (private-ai-framework.txt).\n\n\
             llama.cpp is under llama.cpp.txt, the icon font of the configurator under\n\
             fluent-system-icons.txt. The bundled libraries are redistributed under\n\
             their own licences: the GNU Affero General Public License does not cover\n\
             them, and neither does the MIT licence of llama.cpp:\n\n",
        )
    } else {
        String::from(
            "llama.cpp is under the MIT licence (llama.cpp.txt).\n\n\
             The bundled libraries are redistributed under their own licences, which\n\
             the MIT licence of llama.cpp does not cover:\n\n",
        )
    };
    text.push_str(&listing(entries));
    if web_ui {
        text.push_str(
            "\nThe web UI is shipped with its own files under their own licences: its Atkinson\n\
             Hyperlegible Next font under atkinson-hyperlegible-next.txt, its Atkinson\n\
             Hyperlegible Mono font under atkinson-hyperlegible-mono.txt, its Lucide icon set\n\
             under lucide.txt, and the React it is built on under react.txt.\n",
        );
    }
    if entries.iter().any(|e| e.file == CUDA_EULA_FILE) {
        text.push_str(
            "\nThe NVIDIA libraries are distributed with this application under NVIDIA's terms:\n\
             they may be used only through it.\n",
        );
    }
    if entries
        .iter()
        .any(|e| e.libraries.iter().any(|l| l.contains("rocm_sysdeps")))
    {
        text.push_str(
            "\nThe rocm_sysdeps libraries are AMD's builds of the projects named above, some under\n\
             the LGPL, unmodified by this project. Their sources: https://github.com/ROCm/TheRock\n\
             (third-party/sysdeps).\n",
        );
    }
    text
}

/// The SPDX identifier of a staged text.
#[cfg(any(target_os = "linux", test))]
fn spdx(file: &str) -> Option<&'static str> {
    Some(match file {
        "rocm-rocblas.txt"
        | "rocm-hipblas.txt"
        | "rocm-hipblaslt.txt"
        | "rocm-origami.txt"
        | "rocm-tensilelite.txt"
        | "rocm-rocm-core.txt"
        | "rocm-clr.txt"
        | "rocm-kpack.txt"
        | "rocm-amdsmi.txt"
        | "rocm-rocprofiler-register.txt"
        | "rocm-roctracer.txt"
        | "rocm-rocroller.txt"
        | "libdrm.txt" => "MIT",
        "rocm-rocsolver.txt" => "BSD-2-Clause AND BSD-3-Clause",
        "rocm-amd_comgr.txt" | "llvm.txt" => "Apache-2.0 WITH LLVM-exception",
        "rocm-rocr-runtime.txt" => "NCSA",
        "rocm-rccl.txt" | "zstd.txt" => "BSD-3-Clause",
        "nccl.txt" => "Apache-2.0 AND BSD-3-Clause",
        "vulkan-loader.txt" => "Apache-2.0",
        "bzip2.txt" => "bzip2-1.0.6",
        "elfutils.txt" => "(LGPL-3.0-or-later OR GPL-2.0-or-later)",
        "xz.txt" => "0BSD",
        "libmnl.txt" => "LGPL-2.1-or-later",
        "libnl.txt" | "numactl.txt" => "LGPL-2.1-only",
        "zlib.txt" => "Zlib",
        CUDA_EULA_FILE => "LicenseRef-NVIDIA-CUDA-EULA",
        // The web UI's own texts, as each vendored file says.
        "atkinson-hyperlegible-next.txt" | "atkinson-hyperlegible-mono.txt" => "OFL-1.1",
        "lucide.txt" => "ISC",
        "react.txt" => "MIT",
        _ => return None,
    })
}

/// The RPM `License:` value: the framework's own licence, AGPL-3.0-or-later,
/// when the package is one of the framework's; llama.cpp's MIT, which every
/// package carries; then what the package bundles, and the web UI's texts when
/// the package ships it.
#[cfg(any(target_os = "linux", test))]
pub fn rpm_license(entries: &[Entry], product: Product, web_ui: bool) -> String {
    let mut parts: Vec<&'static str> = Vec::new();
    if product == Product::Framework {
        parts.push("AGPL-3.0-or-later");
    }
    parts.push("MIT");
    let mut add = |part: &'static str| {
        if !parts.contains(&part) {
            parts.push(part);
        }
    };
    for part in entries.iter().filter_map(|e| spdx(&e.file)) {
        add(part);
    }
    if web_ui {
        for part in WEB_UI_TEXTS.iter().copied().filter_map(spdx) {
            add(part);
        }
    }
    parts.join(" AND ")
}

/// The Rich Text form of plain text: one paragraph per line.
#[cfg(any(windows, test))]
fn rtf_paragraphs(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for line in text.lines() {
        for c in line.chars() {
            match c {
                '\\' | '{' | '}' => {
                    out.push('\\');
                    out.push(c);
                }
                ' '..='~' => out.push(c),
                '\t' => out.push(' '),
                // RTF wants a signed 16-bit code unit, with a fallback after it.
                c => {
                    let mut units = [0u16; 2];
                    for unit in c.encode_utf16(&mut units) {
                        out.push_str(&format!("\\u{}?", *unit as i16));
                    }
                }
            }
        }
        out.push_str("\\par\n");
    }
    out
}

/// The license page of the MSI: what is under which terms, this project's
/// licence text, and NVIDIA's terms in full since those carry obligations of
/// their own. The other texts are installed. `product` is the MSI's product,
/// whose own licence heads the page. `web_ui` says this MSI ships the
/// web UI, and so names its texts.
#[cfg(any(windows, test))]
pub fn installer_page(
    entries: &[Entry],
    own: &str,
    eula: Option<&str>,
    product: Product,
    web_ui: bool,
) -> String {
    let mut text = readme(entries, product, web_ui);
    text.push_str("\nThe full texts are installed in share\\licenses, beside bin.\n\n");
    if product == Product::Llama {
        text.push_str("---- llama.cpp: MIT ----\n\n");
    } else {
        text.push_str("---- Private AI Framework: GNU AGPL v3 or later ----\n\n");
    }
    text.push_str(own);
    if let Some(eula) = eula {
        text.push_str("\n\n---- The NVIDIA libraries: NVIDIA's terms ----\n\n");
        text.push_str(eula);
    }
    format!(
        "{{\\rtf1\\ansi\\ansicpg1252\\deff0{{\\fonttbl{{\\f0\\fnil\\fcharset0 Tahoma;}}}}\\uc1\\pard\\fs16\n{}}}\n",
        rtf_paragraphs(&text)
    )
}

#[cfg(test)]
#[path = "licenses_tests.rs"]
mod tests;
