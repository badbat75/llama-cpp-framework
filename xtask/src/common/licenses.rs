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
#[cfg(any(windows, test))]
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
/// `web_ui` says the package ships the web UI, and so carries its texts.
#[cfg(any(windows, test))]
pub fn stage(
    share: &Path,
    libraries: &[String],
    sources: &Sources,
    web_ui: bool,
) -> Result<Vec<Entry>> {
    stage_in(&share.join("licenses"), libraries, sources, web_ui)
}

/// [`stage`] into `dir` itself: split packages each keep their own directory,
/// since no two packages can own the same file.
pub fn stage_in(
    dir: &Path,
    libraries: &[String],
    sources: &Sources,
    web_ui: bool,
) -> Result<Vec<Entry>> {
    std::fs::create_dir_all(dir)?;
    let repo = util::repo_root();
    copy(&repo.join("LICENSE"), &dir.join("private-ai-framework.txt"))?;
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
    std::fs::write(dir.join("README.txt"), readme(&entries, web_ui))?;
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

fn readme(entries: &[Entry], web_ui: bool) -> String {
    let mut text = String::from(
        "The Private AI Framework is distributed under the MIT licence (private-ai-framework.txt).\n\
         llama.cpp is under llama.cpp.txt, the icon font of the configurator under\n\
         fluent-system-icons.txt. The bundled libraries are redistributed under their own\n\
         licences, and this project's MIT licence does not cover them:\n\n",
    );
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

/// The RPM `License:` value: this project and llama.cpp (both MIT), then what
/// the package bundles, and the web UI's texts when the package ships it.
#[cfg(any(target_os = "linux", test))]
pub fn rpm_license(entries: &[Entry], web_ui: bool) -> String {
    let mut parts: Vec<&'static str> = vec!["MIT"];
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
/// licence, and NVIDIA's terms in full since those restrict what the MIT does
/// not. The other texts are installed. `web_ui` says this MSI ships the web UI,
/// and so names its texts.
#[cfg(any(windows, test))]
pub fn installer_page(entries: &[Entry], own: &str, eula: Option<&str>, web_ui: bool) -> String {
    let mut text = readme(entries, web_ui);
    text.push_str("\nThe full texts are installed in share\\licenses, beside bin.\n\n");
    text.push_str("---- Private AI Framework: MIT licence ----\n\n");
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
mod tests {
    use super::*;

    #[test]
    fn a_library_finds_its_licence_under_any_os_s_name() {
        for (file, want) in [
            ("hipblas.dll", Origin::RocmDoc("hipblas")),
            ("libhipblas.so.3", Origin::RocmDoc("hipblas")),
            ("libhipblaslt.dll", Origin::RocmDoc("hipblaslt")),
            ("liborigami.so.1", Origin::Vendored("rocm-origami")),
            ("origami.dll", Origin::Vendored("rocm-origami")),
            ("amdhip64_7.dll", Origin::Vendored("rocm-clr")),
            ("libamdhip64.so.7", Origin::Vendored("rocm-clr")),
            ("rocm_kpack.dll", Origin::Vendored("rocm-kpack")),
            ("librocm-core.so.1", Origin::RocmDoc("rocm-core")),
            ("cublas64_13.dll", Origin::CudaEula),
            ("cublasLt64_13.dll", Origin::CudaEula),
            ("libcudart.so.13", Origin::CudaEula),
            ("ggml-cuda.dll", Origin::CudaEula),
            ("libggml-cuda.so", Origin::CudaEula),
            ("vulkan-1.dll", Origin::Vendored("vulkan-loader")),
            ("libvulkan.so.1", Origin::Vendored("vulkan-loader")),
            ("libcrypto-4-x64.dll", Origin::OpenSsl),
            ("libssl-4-x64.dll", Origin::OpenSsl),
            ("librocm_sysdeps_z.so.1", Origin::Vendored("zlib")),
            ("librocm_sysdeps_zstd.so.1", Origin::Vendored("zstd")),
            (
                "librocm_sysdeps_drm_amdgpu.so.1",
                Origin::Vendored("libdrm"),
            ),
            (
                "librocm_sysdeps_nl_genl_3.so.200",
                Origin::Vendored("libnl"),
            ),
            ("libLLVM.so.24.0git", Origin::Vendored("llvm")),
            ("libomp.so", Origin::Vendored("llvm")),
        ] {
            assert_eq!(origin(file), Some(want), "{file}");
        }
        assert_eq!(origin("ggml-vulkan.dll"), None, "llama.cpp's own");
        assert!(
            is_llama_cpp("ggml-vulkan.dll")
                && is_llama_cpp("libllama.so")
                && is_llama_cpp("mtmd.dll")
        );
        assert_eq!(origin("somethingnew.dll"), None);
        assert!(!is_llama_cpp("somethingnew.dll"));
    }

    #[test]
    fn every_vendored_text_named_by_the_table_is_in_the_repo() {
        let dir = util::repo_root().join("installer").join("licenses");
        for (_, origin) in TABLE {
            if let Origin::Vendored(name) = origin {
                assert!(dir.join(format!("{name}.txt")).is_file(), "{name}");
            }
        }
    }

    /// origami's text is vendored in the repo, not read from the dist's
    /// `share/doc`, like rocroller's.
    #[test]
    fn the_origami_text_is_vendored_in_the_repo() {
        let dir = util::repo_root().join("installer").join("licenses");
        assert!(dir.join("rocm-origami.txt").is_file(), "rocm-origami.txt");
    }

    /// The web UI's own texts, vendored from `installer/licenses/` and staged
    /// by `stage_in` beside `fluent-system-icons.txt`: its two fonts, its
    /// icons, React. `README.txt` says what each covers.
    #[test]
    fn the_web_ui_texts_are_staged_beside_the_icon_font() {
        let dir = std::env::temp_dir().join(format!("xtask-licences-webui-{}", std::process::id()));
        let llama = dir.join("llama");
        std::fs::create_dir_all(&llama).unwrap();
        std::fs::write(llama.join("LICENSE"), "MIT").unwrap();
        let share = dir.join("share");
        let sources = Sources {
            llama_cpp: llama,
            ..Default::default()
        };
        let files = [
            "atkinson-hyperlegible-next.txt",
            "atkinson-hyperlegible-mono.txt",
            "lucide.txt",
            "react.txt",
        ];
        // A missing text stops the package naming it, as the other vendored
        // texts do: an Ok that skipped them and an Err that named none are
        // both refusals here.
        let staged = stage(&share, &[], &sources, true);
        let licenses = share.join("licenses");
        let entries = match staged {
            Ok(entries) => entries,
            Err(e) => {
                let text = e.to_string();
                for file in files {
                    assert!(
                        text.contains(file),
                        "staging stopped without naming {file}: {text}"
                    );
                }
                panic!("staging stopped: {text}");
            }
        };
        assert!(
            entries.is_empty(),
            "the fixed texts join no library entry: {entries:?}"
        );
        for file in files {
            assert!(licenses.join(file).is_file(), "{file} is not staged");
        }
        let readme = std::fs::read_to_string(licenses.join("README.txt")).unwrap();
        assert!(
            readme.contains("web UI"),
            "README.txt does not name the web UI:\n{readme}"
        );
        for (file, covers) in [
            ("atkinson-hyperlegible-next.txt", "font"),
            ("atkinson-hyperlegible-mono.txt", "font"),
            ("lucide.txt", "icon"),
            ("react.txt", "React"),
        ] {
            assert!(
                covers_in_a_paragraph(&readme, file, covers),
                "README.txt does not say {file} covers {covers}:\n{readme}"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `file` and `covers` sit in the same paragraph of `readme`: the text
    /// is named beside what it covers, not somewhere and the subject
    /// somewhere.
    fn covers_in_a_paragraph(readme: &str, file: &str, covers: &str) -> bool {
        readme
            .split("\n\n")
            .any(|p| p.contains(file) && p.contains(covers))
    }

    /// The four texts are vendored in the repo like every one the table
    /// names: a missing text stops the package.
    #[test]
    fn the_web_ui_vendored_texts_are_in_the_repo() {
        let dir = util::repo_root().join("installer").join("licenses");
        for name in [
            "atkinson-hyperlegible-next",
            "atkinson-hyperlegible-mono",
            "lucide",
            "react",
        ] {
            assert!(dir.join(format!("{name}.txt")).is_file(), "{name}");
        }
    }

    /// The web UI's texts belong to the package that ships the web UI, and to
    /// no other: a package without it carries neither the texts nor the
    /// `README.txt` paragraph that names them.
    #[test]
    fn the_web_ui_texts_are_staged_only_for_a_package_that_ships_the_web_ui() {
        let dir =
            std::env::temp_dir().join(format!("xtask-licences-webui-only-{}", std::process::id()));
        let llama = dir.join("llama");
        std::fs::create_dir_all(&llama).unwrap();
        std::fs::write(llama.join("LICENSE"), "MIT").unwrap();
        let sources = Sources {
            llama_cpp: llama,
            ..Default::default()
        };
        let files = [
            "atkinson-hyperlegible-next.txt",
            "atkinson-hyperlegible-mono.txt",
            "lucide.txt",
            "react.txt",
        ];
        for (package, web_ui) in [
            ("the package that ships the web UI", true),
            ("the package that does not", false),
        ] {
            let licenses = dir.join(if web_ui { "web" } else { "plain" });
            stage_in(&licenses, &[], &sources, web_ui).unwrap();
            let readme = std::fs::read_to_string(licenses.join("README.txt")).unwrap();
            for file in files {
                assert_eq!(
                    licenses.join(file).is_file(),
                    web_ui,
                    "{file} staged for {package}"
                );
            }
            assert_eq!(readme.contains("web UI"), web_ui, "{package}: {readme}");
            for file in files {
                assert_eq!(
                    readme.contains(file),
                    web_ui,
                    "{package}: README.txt names {file}\n{readme}"
                );
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A package that ships the web UI and is missing one of its texts stops,
    /// naming it, like a bundled library with no known licence does. A package
    /// that does not ship the web UI is not stopped by a text it never carries.
    #[test]
    fn a_package_that_ships_the_web_ui_stops_when_its_text_is_missing() {
        let dir =
            std::env::temp_dir().join(format!("xtask-licences-missing-{}", std::process::id()));
        let llama = dir.join("llama");
        std::fs::create_dir_all(&llama).unwrap();
        std::fs::write(llama.join("LICENSE"), "MIT").unwrap();
        // The texts of this package, one of them gone.
        let texts = dir.join("texts");
        std::fs::create_dir_all(&texts).unwrap();
        for file in [
            "atkinson-hyperlegible-next.txt",
            "atkinson-hyperlegible-mono.txt",
            "lucide.txt",
        ] {
            std::fs::write(texts.join(file), "text").unwrap();
        }
        let missing = "react.txt";
        let sources = Sources {
            llama_cpp: llama,
            web_ui_licenses: Some(texts),
            ..Default::default()
        };
        stage_in(&dir.join("plain"), &[], &sources, false)
            .expect("a package without the web UI stops over a web UI text");
        let stopped = stage_in(&dir.join("web"), &[], &sources, true)
            .expect_err("a package shipping the web UI was packaged without its text");
        assert!(
            stopped.to_string().contains(missing),
            "the refusal does not name {missing}: {stopped}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The SPDX identifier of each web UI text, as its own vendored file says:
    /// the fonts are SIL OFL 1.1, Lucide is ISC, React is MIT.
    #[test]
    fn the_web_ui_texts_map_to_their_spdx_licence() {
        for (file, spdx_id) in [
            ("atkinson-hyperlegible-next.txt", "OFL-1.1"),
            ("atkinson-hyperlegible-mono.txt", "OFL-1.1"),
            ("lucide.txt", "ISC"),
            ("react.txt", "MIT"),
        ] {
            assert_eq!(spdx(file), Some(spdx_id), "{file}");
        }
    }

    /// The `.rpm` `License:` field of the package that ships the web UI declares
    /// OFL-1.1 and ISC beside MIT; every other component's field is byte for
    /// byte what it is today.
    #[test]
    fn the_rpm_field_of_the_web_ui_package_adds_the_font_and_icon_licences() {
        let entries = |files: &[&str]| -> Vec<Entry> {
            files
                .iter()
                .map(|f| Entry {
                    file: f.to_string(),
                    libraries: Vec::new(),
                })
                .collect()
        };
        assert_eq!(rpm_license(&[], true), "MIT AND OFL-1.1 AND ISC");
        assert_eq!(rpm_license(&[], false), "MIT");
        assert_eq!(
            rpm_license(
                &entries(&["rocm-clr.txt", CUDA_EULA_FILE, "numactl.txt"]),
                false
            ),
            "MIT AND LicenseRef-NVIDIA-CUDA-EULA AND LGPL-2.1-only"
        );
    }

    /// The web UI flag of every call to `what` in `src` (which ends with its
    /// `(`): the last argument, up to the `)` that closes the call, split at
    /// the commas of the call's own nesting level. Reads every call, not the
    /// first.
    fn call_flags(src: &str, what: &str) -> Vec<String> {
        let mut flags = Vec::new();
        let mut from = 0;
        while let Some(at) = src[from..].find(what) {
            let args = &src[from + at + what.len()..];
            let mut parens = 1usize;
            let mut opened = 1usize;
            let mut parts: Vec<String> = vec![String::new()];
            let mut closed = false;
            for c in args.chars() {
                match c {
                    '(' => {
                        parens += 1;
                        opened += 1;
                        parts.last_mut().unwrap().push(c);
                    }
                    ')' => {
                        parens -= 1;
                        if parens == 0 {
                            closed = true;
                            break;
                        }
                        opened -= 1;
                        parts.last_mut().unwrap().push(c);
                    }
                    '[' | '{' => {
                        opened += 1;
                        parts.last_mut().unwrap().push(c);
                    }
                    ']' | '}' => {
                        opened -= 1;
                        parts.last_mut().unwrap().push(c);
                    }
                    ',' if opened == 1 => parts.push(String::new()),
                    _ => parts.last_mut().unwrap().push(c),
                }
            }
            assert!(closed, "{what} is not closed");
            from += at + what.len();
            if parts.last().is_some_and(|p| p.trim().is_empty()) {
                parts.pop();
            }
            assert!(parts.len() > 1, "{what} is called with no arguments");
            flags.push(parts.pop().unwrap());
        }
        flags
    }

    /// The decision for a Linux component, one assert per variant: a new
    /// component must be decided here, as true only for the bff. The match
    /// lists every variant and has no wildcard, so a new one is a compile
    /// error. The names it uses exist only on Linux, so the test is built
    /// only there.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_component_that_ships_the_web_ui_is_the_bff() {
        for component in Component::ALL {
            let expected = match component {
                Component::Config => false,
                Component::Cli => false,
                Component::Bff => true,
                Component::Core => false,
                Component::Llama => false,
                Component::Rocm => false,
                Component::Cuda => false,
                Component::Vulkan => false,
            };
            assert_eq!(component_ships_web_ui(component), expected, "{component:?}");
        }
    }

    /// The decision for a Windows product, one assert per variant: a new
    /// product must be decided here, as true only for the framework, the MSI
    /// that holds pai-bff.exe. The match lists every variant and has no
    /// wildcard, so a new one is a compile error.
    #[test]
    fn the_windows_product_that_ships_the_web_ui_is_the_framework() {
        for product in [Product::Llama, Product::Framework] {
            let expected = match product {
                Product::Llama => false,
                Product::Framework => true,
            };
            assert_eq!(product_ships_web_ui(product), expected, "{product:?}");
        }
    }

    /// What behaviour cannot reach: every licence call hands its web UI flag
    /// to the function that decides it, as the call's last argument. Reads
    /// every call, `installer_page` included, and refuses `||` and a literal
    /// flag: the answer is the function's, not the caller's. macOS has no
    /// bff, so no macOS package ships the web UI texts: there the flag is a
    /// plain `false`.
    #[test]
    fn every_licence_call_passes_the_web_ui_function() {
        for (src, what, decide) in [
            (
                include_str!("../linux/package.rs"),
                "licenses::stage_in(",
                "component_ships_web_ui(",
            ),
            (
                include_str!("../linux/package.rs"),
                "licenses::rpm_license(",
                "component_ships_web_ui(",
            ),
            (
                include_str!("../windows/package.rs"),
                "licenses::stage(",
                "product_ships_web_ui(",
            ),
            (
                include_str!("../windows/package.rs"),
                "licenses::installer_page(",
                "product_ships_web_ui(",
            ),
        ] {
            let flags = call_flags(src, what);
            assert!(!flags.is_empty(), "{what} is not called");
            for flag in &flags {
                assert!(flag.contains(decide), "{what} flag is not {decide}: {flag}");
                assert!(!flag.contains("||"), "{what} flag decides with ||: {flag}");
                assert!(
                    !flag.contains("true") && !flag.contains("false"),
                    "{what} flag is a literal: {flag}"
                );
            }
        }
        let flags = call_flags(include_str!("../macos/package.rs"), "licenses::stage_in(");
        assert!(
            !flags.is_empty(),
            "licenses::stage_in is not called on macOS"
        );
        for flag in &flags {
            assert!(
                flag.contains("false") && !flag.contains("true"),
                "a macOS package ships the web UI texts: {flag}"
            );
        }
    }

    #[test]
    fn a_library_with_no_known_licence_stops_the_package() {
        let dir = std::env::temp_dir().join(format!("xtask-licences-{}", std::process::id()));
        let llama = dir.join("llama");
        std::fs::create_dir_all(llama.join("licenses")).unwrap();
        std::fs::write(llama.join("LICENSE"), "MIT").unwrap();
        std::fs::write(llama.join("licenses").join("LICENSE-jsonhpp"), "MIT").unwrap();
        let cuda = dir.join("cuda");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join("LICENSE"), "EULA").unwrap();
        let share = dir.join("share");
        let sources = Sources {
            llama_cpp: llama,
            cuda: Some(cuda),
            ..Default::default()
        };

        let unknown =
            stage(&share, &["somethingnew.dll".to_string()], &sources, false).unwrap_err();
        assert!(
            unknown.to_string().contains("somethingnew.dll"),
            "{unknown}"
        );
        let absent = stage(&share, &["hipblas.dll".to_string()], &sources, false).unwrap_err();
        assert!(absent.to_string().contains("hipblas.dll"), "{absent}");

        let libraries = [
            "cublas64_13.dll",
            "cublasLt64_13.dll",
            "llama.dll",
            "amdhip64_7.dll",
        ]
        .map(str::to_string);
        let entries = stage(&share, &libraries, &sources, false).unwrap();
        assert_eq!(
            entries,
            [
                Entry {
                    file: CUDA_EULA_FILE.to_string(),
                    libraries: vec![
                        "cublas64_13.dll".to_string(),
                        "cublasLt64_13.dll".to_string()
                    ],
                },
                Entry {
                    file: "rocm-clr.txt".to_string(),
                    libraries: vec!["amdhip64_7.dll".to_string()]
                },
            ]
        );
        let licenses = share.join("licenses");
        for file in [
            "private-ai-framework.txt",
            "llama.cpp.txt",
            "llama.cpp-jsonhpp.txt",
            CUDA_EULA_FILE,
            "rocm-clr.txt",
        ] {
            assert!(licenses.join(file).is_file(), "{file}");
        }
        let readme = std::fs::read_to_string(licenses.join("README.txt")).unwrap();
        assert!(
            readme.contains("NVIDIA-CUDA-EULA.txt: cublas64_13.dll, cublasLt64_13.dll"),
            "{readme}"
        );
        assert!(readme.contains("used only through it"), "{readme}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_rpm_field_names_every_text_a_linux_package_can_carry() {
        for (_, origin) in TABLE {
            let file = match origin {
                Origin::RocmDoc(name) => format!("rocm-{name}.txt"),
                Origin::Vendored("moltenvk") => continue,
                Origin::Vendored(name) => format!("{name}.txt"),
                Origin::CudaEula => CUDA_EULA_FILE.to_string(),
                Origin::OpenSsl => continue,
            };
            assert!(spdx(&file).is_some(), "{file}");
        }
        let entries = |files: &[&str]| -> Vec<Entry> {
            files
                .iter()
                .map(|f| Entry {
                    file: f.to_string(),
                    libraries: Vec::new(),
                })
                .collect()
        };
        assert_eq!(rpm_license(&[], false), "MIT");
        assert_eq!(
            rpm_license(
                &entries(&["rocm-clr.txt", CUDA_EULA_FILE, "numactl.txt", "libnl.txt"]),
                false
            ),
            "MIT AND LicenseRef-NVIDIA-CUDA-EULA AND LGPL-2.1-only"
        );
    }

    #[test]
    fn the_libraries_are_found_at_any_depth_and_data_files_are_not() {
        let dir = std::env::temp_dir().join(format!("xtask-libraries-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib").join("llvm")).unwrap();
        for file in [
            "lib/llvm/libomp.so",
            "lib/libamdhip64.so.7",
            "a.dll",
            "b.dylib",
            "lib/kernels.dat",
            "soname.txt",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        assert_eq!(
            libraries_under(&dir),
            ["a.dll", "b.dylib", "libamdhip64.so.7", "libomp.so"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_installer_page_is_valid_rtf_with_the_terms_in_it() {
        assert_eq!(rtf_paragraphs("a\\b{c}\n"), "a\\\\b\\{c\\}\\par\n");
        assert_eq!(rtf_paragraphs("\u{2019}"), "\\u8217?\\par\n");
        assert_eq!(rtf_paragraphs("\u{1F600}"), "\\u-10179?\\u-8704?\\par\n");
        let entries = [Entry {
            file: CUDA_EULA_FILE.to_string(),
            libraries: vec!["cublas.dll".to_string()],
        }];
        let page = installer_page(&entries, "MIT text", Some("EULA text"), true);
        assert!(page.starts_with("{\\rtf1"), "{page}");
        assert!(page.trim_end().ends_with('}'), "{page}");
        assert!(
            page.contains("MIT text") && page.contains("EULA text") && page.contains("cublas.dll"),
            "{page}"
        );
        assert!(!installer_page(&entries, "MIT text", None, true).contains("NVIDIA's terms ----"));
    }
}
