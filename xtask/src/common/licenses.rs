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

/// Library name prefixes, without `lib` and the version; the first match wins,
/// so a longer name goes before the one it starts with.
const TABLE: &[(&str, Origin)] = &[
    ("hipblaslt", Origin::RocmDoc("hipblaslt")),
    ("hipblas", Origin::RocmDoc("hipblas")),
    ("rocblas", Origin::RocmDoc("rocblas")),
    ("rocsolver", Origin::RocmDoc("rocsolver")),
    ("origami", Origin::RocmDoc("origami")),
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
#[cfg(any(windows, test))]
pub fn stage(share: &Path, libraries: &[String], sources: &Sources) -> Result<Vec<Entry>> {
    stage_in(&share.join("licenses"), libraries, sources)
}

/// [`stage`] into `dir` itself: split packages each keep their own directory,
/// since no two packages can own the same file.
pub fn stage_in(dir: &Path, libraries: &[String], sources: &Sources) -> Result<Vec<Entry>> {
    std::fs::create_dir_all(dir)?;
    let repo = util::repo_root();
    copy(&repo.join("LICENSE"), &dir.join("llama.cpp-framework.txt"))?;
    copy(
        &repo
            .join("crates")
            .join("config")
            .join("ui")
            .join("fonts")
            .join("LICENSE"),
        &dir.join("fluent-system-icons.txt"),
    )?;
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
    std::fs::write(dir.join("README.txt"), readme(&entries))?;
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

fn readme(entries: &[Entry]) -> String {
    let mut text = String::from(
        "llama.cpp-framework is distributed under the MIT licence (llama.cpp-framework.txt).\n\
         llama.cpp is under llama.cpp.txt, the icon font of the configurator under\n\
         fluent-system-icons.txt. The bundled libraries are redistributed under their own\n\
         licences, and this project's MIT licence does not cover them:\n\n",
    );
    text.push_str(&listing(entries));
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
        _ => return None,
    })
}

/// The RPM `License:` value: this project and llama.cpp (both MIT), then what
/// the package bundles.
#[cfg(any(target_os = "linux", test))]
pub fn rpm_license(entries: &[Entry]) -> String {
    let mut parts = vec!["MIT"];
    for part in entries.iter().filter_map(|e| spdx(&e.file)) {
        if !parts.contains(&part) {
            parts.push(part);
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
/// not. The other texts are installed.
#[cfg(any(windows, test))]
pub fn installer_page(entries: &[Entry], own: &str, eula: Option<&str>) -> String {
    let mut text = readme(entries);
    text.push_str("\nThe full texts are installed in share\\licenses, beside bin.\n\n");
    text.push_str("---- llama.cpp-framework: MIT licence ----\n\n");
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

        let unknown = stage(&share, &["somethingnew.dll".to_string()], &sources).unwrap_err();
        assert!(
            unknown.to_string().contains("somethingnew.dll"),
            "{unknown}"
        );
        let absent = stage(&share, &["hipblas.dll".to_string()], &sources).unwrap_err();
        assert!(absent.to_string().contains("hipblas.dll"), "{absent}");

        let libraries = [
            "cublas64_13.dll",
            "cublasLt64_13.dll",
            "llama.dll",
            "amdhip64_7.dll",
        ]
        .map(str::to_string);
        let entries = stage(&share, &libraries, &sources).unwrap();
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
            "llama.cpp-framework.txt",
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
        assert_eq!(rpm_license(&[]), "MIT");
        assert_eq!(
            rpm_license(&entries(&[
                "rocm-clr.txt",
                CUDA_EULA_FILE,
                "numactl.txt",
                "libnl.txt"
            ])),
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
        let page = installer_page(&entries, "MIT text", Some("EULA text"));
        assert!(page.starts_with("{\\rtf1"), "{page}");
        assert!(page.trim_end().ends_with('}'), "{page}");
        assert!(
            page.contains("MIT text") && page.contains("EULA text") && page.contains("cublas.dll"),
            "{page}"
        );
        assert!(!installer_page(&entries, "MIT text", None).contains("NVIDIA's terms ----"));
    }
}
