use super::*;

#[test]
fn a_library_finds_its_licence_under_any_os_s_name() {
    for (file, want) in [
        ("hipblas.dll", Origin::RocmDoc("hipblas")),
        ("libhipblas.so.3", Origin::RocmDoc("hipblas")),
        ("libhipblaslt.dll", Origin::RocmDoc("hipblaslt")),
        ("liborigami.so.1", Origin::Vendored("rocm-origami")),
        ("origami.dll", Origin::Vendored("rocm-origami")),
        (
            "libtensilelite-host.so.1",
            Origin::Vendored("rocm-tensilelite"),
        ),
        ("tensilelite-host.dll", Origin::Vendored("rocm-tensilelite")),
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
        is_llama_cpp("ggml-vulkan.dll") && is_llama_cpp("libllama.so") && is_llama_cpp("mtmd.dll")
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

/// tensilelite-host's text (hipBLASLt's `libtensilelite-host.so.1`, which
/// `libhipblaslt.so.1` needs) is vendored in the repo, not read from the
/// dist's `share/doc`, like origami's and rocroller's.
#[test]
fn the_tensilelite_host_text_is_vendored_in_the_repo() {
    let dir = util::repo_root().join("installer").join("licenses");
    assert!(
        dir.join("rocm-tensilelite.txt").is_file(),
        "rocm-tensilelite.txt"
    );
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
    let staged = stage(&share, &[], &sources, Product::Framework, true);
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
        stage_in(&licenses, &[], &sources, Product::Framework, web_ui).unwrap();
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
    let dir = std::env::temp_dir().join(format!("xtask-licences-missing-{}", std::process::id()));
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
    stage_in(&dir.join("plain"), &[], &sources, Product::Framework, false)
        .expect("a package without the web UI stops over a web UI text");
    let stopped = stage_in(&dir.join("web"), &[], &sources, Product::Framework, true)
        .expect_err("a package shipping the web UI was packaged without its text");
    assert!(
        stopped.to_string().contains(missing),
        "the refusal does not name {missing}: {stopped}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The framework is AGPL-3.0-or-later (llama.cpp stays MIT): `README.txt`
/// names that licence beside the file holding it, says the bundled
/// libraries sit outside it, and never calls the framework's own text MIT.
#[test]
fn the_readme_names_the_framework_s_agpl_licence() {
    let text = readme(&[], Product::Framework, false);
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    assert!(
        paragraphs
            .iter()
            .any(|p| p.contains("GNU Affero General Public License")
                && p.contains("private-ai-framework.txt")
                && p.contains("3")
                && p.contains("later")),
        "README.txt does not name the framework's licence as the GNU Affero\n\
             General Public License, version 3 or later, in private-ai-framework.txt:\n{text}"
    );
    assert!(
        paragraphs.iter().any(|p| p.contains("librar")
            && p.contains("cover")
            && p.contains("not")
            && (p.contains("Affero") || p.contains("AGPL"))),
        "README.txt does not say the bundled libraries are outside the framework's\n\
             licence:\n{text}"
    );
    for paragraph in &paragraphs {
        assert!(
            !paragraph.contains("private-ai-framework.txt") || !paragraph.contains("MIT"),
            "README.txt calls the framework's own text MIT:\n{text}"
        );
    }
    assert!(
        !text.contains("this project's MIT licence"),
        "README.txt still says this project's MIT licence:\n{text}"
    );
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

/// The `.rpm` `License:` field of the package that ships the web UI (the
/// bff, a framework package) starts with the framework's AGPL and keeps MIT
/// beside it, for llama.cpp and its MIT libraries, and adds OFL-1.1 and ISC.
/// The engine's field names MIT alone: the framework is not in that package.
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
    assert_eq!(
        rpm_license(&[], Product::Framework, true),
        "AGPL-3.0-or-later AND MIT AND OFL-1.1 AND ISC"
    );
    assert_eq!(
        rpm_license(&[], Product::Framework, false),
        "AGPL-3.0-or-later AND MIT"
    );
    assert_eq!(
        rpm_license(
            &entries(&["rocm-clr.txt", CUDA_EULA_FILE, "numactl.txt"]),
            Product::Llama,
            false
        ),
        "MIT AND LicenseRef-NVIDIA-CUDA-EULA AND LGPL-2.1-only"
    );
}

/// The arguments of every call to `what` in `src` (which ends with its
/// `(`): each argument up to the `)` that closes the call, split at the
/// commas of the call's own nesting level. Reads every call, not the
/// first.
fn call_args(src: &str, what: &str) -> Vec<Vec<String>> {
    let mut calls = Vec::new();
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
        calls.push(parts);
    }
    calls
}

/// The web UI flag of every call of `what`: its last argument.
fn call_flags(src: &str, what: &str) -> Vec<String> {
    call_args(src, what)
        .into_iter()
        .map(|mut args| args.pop().unwrap())
        .collect()
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
        // Production code: a test that stages licences for a fixture adds
        // call sites of its own, which are not the phase's decision.
        let src = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
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

    let unknown = stage(
        &share,
        &["somethingnew.dll".to_string()],
        &sources,
        Product::Framework,
        false,
    )
    .unwrap_err();
    assert!(
        unknown.to_string().contains("somethingnew.dll"),
        "{unknown}"
    );
    let absent = stage(
        &share,
        &["hipblas.dll".to_string()],
        &sources,
        Product::Framework,
        false,
    )
    .unwrap_err();
    assert!(absent.to_string().contains("hipblas.dll"), "{absent}");

    let libraries = [
        "cublas64_13.dll",
        "cublasLt64_13.dll",
        "llama.dll",
        "amdhip64_7.dll",
    ]
    .map(str::to_string);
    let entries = stage(&share, &libraries, &sources, Product::Framework, false).unwrap();
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
    assert_eq!(rpm_license(&[], Product::Llama, false), "MIT");
    assert_eq!(
        rpm_license(&[], Product::Framework, false),
        "AGPL-3.0-or-later AND MIT"
    );
    assert_eq!(
        rpm_license(
            &entries(&["rocm-clr.txt", CUDA_EULA_FILE, "numactl.txt", "libnl.txt"]),
            Product::Framework,
            false
        ),
        "AGPL-3.0-or-later AND MIT AND LicenseRef-NVIDIA-CUDA-EULA AND LGPL-2.1-only"
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
    let page = installer_page(
        &entries,
        "AGPL text",
        Some("EULA text"),
        Product::Framework,
        true,
    );
    assert!(page.starts_with("{\\rtf1"), "{page}");
    assert!(page.trim_end().ends_with('}'), "{page}");
    assert!(
        page.contains("AGPL text") && page.contains("EULA text") && page.contains("cublas.dll"),
        "{page}"
    );
    assert!(
        !installer_page(&entries, "AGPL text", None, Product::Framework, true,)
            .contains("NVIDIA's terms ----")
    );
}

/// The page heads the framework's own text with its licence, the GNU AGPL
/// v3 or later, and says no MIT about it.
#[test]
fn the_installer_page_heads_the_framework_s_agpl_text() {
    let page = installer_page(&[], "AGPL text", None, Product::Framework, false);
    assert!(
        page.contains("Private AI Framework: GNU AGPL v3 or later"),
        "the page does not head the framework's text with its licence:\n{page}"
    );
    assert!(
        !page.contains("Private AI Framework: MIT"),
        "the page still calls the framework MIT:\n{page}"
    );
}

/// The production comments of this file, joined in order with their `//`
/// markers stripped, so a phrase wrapped across two doc lines reads as one.
fn comments(src: &str) -> String {
    src.lines()
        .filter(|l| l.trim_start().starts_with("//"))
        .map(|l| l.trim_start().trim_start_matches('/').trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The framework is AGPL, not MIT: the note on the MSI page's NVIDIA
/// section no longer measures those terms against "what the MIT does not".
#[test]
fn the_nvidia_note_no_longer_names_the_mit() {
    let src = include_str!("licenses.rs");
    let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
    let notes = comments(code);
    assert!(
        !notes.contains("what the MIT does not"),
        "the NVIDIA note says what the MIT does not:\n{notes}"
    );
}

/// `text` names the framework's licence, in any casing, as a word
/// ("AGPL") or in full ("Affero").
fn names_agpl(text: &str) -> bool {
    text.to_ascii_uppercase().contains("AGPL") || text.to_ascii_lowercase().contains("affero")
}

/// The two products present their licences apart: `README.txt` of a
/// llama.cpp package (the engine, its runtimes, llama.cpp's MSI) says
/// llama.cpp is under MIT in `llama.cpp.txt` and that the bundled
/// libraries keep their own licences, and names the framework's AGPL
/// nowhere, not even as a file it does not ship. The framework's keeps the
/// AGPL text beside the file holding it. The messages stay short, so the
/// red reason reads on one line.
#[test]
fn the_readme_of_the_llama_product_names_mit_and_no_agpl() {
    let llama = readme(&[], Product::Llama, false);
    assert!(!names_agpl(&llama), "a llama.cpp readme names the AGPL");
    assert!(
        !llama.contains("private-ai-framework.txt"),
        "a llama.cpp readme names private-ai-framework.txt"
    );
    let paragraphs: Vec<&str> = llama.split("\n\n").collect();
    assert!(
        paragraphs
            .iter()
            .any(|p| p.contains("llama.cpp.txt") && p.contains("MIT")),
        "the llama.cpp readme does not name MIT in llama.cpp.txt"
    );
    assert!(
        paragraphs
            .iter()
            .any(|p| p.contains("librar") && p.contains("licenc") && p.contains("own")),
        "the llama.cpp readme omits the libraries' own licences"
    );
    let framework = readme(&[], Product::Framework, false);
    assert!(
        names_agpl(&framework)
            && framework.contains("GNU Affero General Public License")
            && framework.contains("private-ai-framework.txt"),
        "the framework readme lost its AGPL text"
    );
}

/// The framework's own licence text is staged for the framework's packages
/// only: a llama.cpp package (the engine, its runtimes, llama.cpp's MSI)
/// carries llama.cpp's text and says nothing about a file it does not
/// ship. The messages stay short.
#[test]
fn the_framework_s_own_text_is_staged_only_for_the_framework() {
    let dir = std::env::temp_dir().join(format!("xtask-licences-product-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let llama = dir.join("llama");
    std::fs::create_dir_all(&llama).unwrap();
    std::fs::write(llama.join("LICENSE"), "MIT").unwrap();
    let sources = Sources {
        llama_cpp: llama,
        ..Default::default()
    };
    for product in [Product::Framework, Product::Llama] {
        let licenses = dir.join(if product == Product::Framework {
            "framework"
        } else {
            "llama"
        });
        stage_in(&licenses, &[], &sources, product, false).unwrap();
        let own = product == Product::Framework;
        assert_eq!(
            licenses.join("private-ai-framework.txt").is_file(),
            own,
            "{product:?}: private-ai-framework.txt staged"
        );
        assert!(
            licenses.join("llama.cpp.txt").is_file(),
            "{product:?}: llama.cpp.txt is not staged"
        );
        let readme = std::fs::read_to_string(licenses.join("README.txt")).unwrap();
        assert_eq!(
            readme.contains("private-ai-framework.txt"),
            own,
            "{product:?}: readme names private-ai-framework.txt it does not ship"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The licence page of the MSI: the llama.cpp MSI heads llama.cpp's own
/// LICENSE as MIT and carries no AGPL text at all, since the framework is
/// not in that package. The framework's MSI keeps the AGPL heading. The messages stay short.
#[test]
fn the_installer_page_of_the_llama_msi_heads_llama_cpp_as_mit() {
    let page = installer_page(&[], "MIT licence text", None, Product::Llama, false);
    assert!(!names_agpl(&page), "the llama.cpp MSI page names the AGPL");
    assert!(
        page.contains("MIT licence text"),
        "the llama.cpp MSI page lacks llama.cpp's own text"
    );
    assert!(
        page.lines()
            .any(|l| l.contains("----") && l.contains("llama.cpp") && l.contains("MIT")),
        "the llama.cpp MSI page has no MIT heading"
    );
    let framework = installer_page(&[], "AGPL text", None, Product::Framework, false);
    assert!(
        framework.contains("Private AI Framework: GNU AGPL v3 or later"),
        "the framework MSI page lost its AGPL heading"
    );
}

/// The MSI's own text: the Windows package phase reads llama.cpp's LICENSE
/// for llama.cpp's MSI and the repo's for the framework's, so the page
/// heads the text the MSI actually ships. Read from the code, since the
/// Windows phase compiles on Windows only.
#[test]
fn the_msi_s_own_text_is_picked_by_the_product() {
    let code = include_str!("../windows/package.rs")
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    let from = code
        .find("let own =")
        .expect("the MSI page's own text is never read");
    let to = code[from..]
        .find("licenses::installer_page(")
        .expect("installer_page is not called after the own text");
    let picks = &code[from..from + to];
    assert!(
        picks.contains("llama_cpp"),
        "the MSI page never reads llama.cpp's LICENSE:\n{picks}"
    );
    assert!(
        picks.contains("product"),
        "the MSI page's own text is not picked by product:\n{picks}"
    );
}

/// What behaviour cannot reach: every licence seam of the three package
/// phases is handed the product of the package being built, as its own
/// argument, never a literal: the answer is the component's / the MSI's,
/// not the caller's.
#[test]
fn every_licence_call_passes_the_product() {
    for (src, what, want) in [
        (
            include_str!("../linux/package.rs"),
            "licenses::stage_in(",
            ".product()",
        ),
        (
            include_str!("../macos/package.rs"),
            "licenses::stage_in(",
            ".product()",
        ),
        (
            include_str!("../windows/package.rs"),
            "licenses::stage(",
            "product",
        ),
        (
            include_str!("../windows/package.rs"),
            "licenses::installer_page(",
            "product",
        ),
    ] {
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let calls = call_args(code, what);
        assert!(!calls.is_empty(), "{what} is not called");
        for args in &calls {
            assert!(
                args.iter().any(|a| a.contains(want)),
                "{what} is called with no product: {args:?}"
            );
            assert!(
                !args.iter().any(|a| a.contains("Product::")),
                "{what} is called with a literal product: {args:?}"
            );
        }
    }
}

/// The RPM field's product: every `rpm_license` call of the Linux package
/// phase hands it the component's own product, never a literal: the answer
/// is the component's, not the caller's.
#[test]
fn every_rpm_licence_call_passes_the_component_product() {
    let src = include_str!("../linux/package.rs");
    let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
    let calls = call_args(code, "licenses::rpm_license(");
    assert!(!calls.is_empty(), "licenses::rpm_license is not called");
    for args in &calls {
        assert!(
            args.iter().any(|a| a.contains(".product()")),
            "no component product passed: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a.contains("Product::")),
            "a literal product: {args:?}"
        );
    }
}
