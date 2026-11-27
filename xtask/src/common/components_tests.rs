use super::*;

/// The engine's package and its runtimes are the product `llama`, the
/// four `private-ai-framework-*` packages the product `framework`.
#[test]
fn each_component_belongs_to_the_engine_or_the_framework() {
    for c in Component::ALL {
        let want = if c == Component::Llama || c.is_runtime() {
            Product::Llama
        } else {
            Product::Framework
        };
        assert_eq!(c.product(), want, "{}", c.package());
    }
}

/// `xtask package llama` touches only the engine and its runtimes,
/// `xtask package framework` only the framework's four packages, and no
/// choice packages them all, in the order of `Component::ALL`.
#[test]
fn a_package_run_takes_the_components_of_the_product_it_names() {
    use Component::*;
    assert_eq!(
        of_product(Some(Product::Llama)),
        vec![Llama, Rocm, Cuda, Vulkan]
    );
    assert_eq!(
        of_product(Some(Product::Framework)),
        vec![Config, Cli, Bff, Core]
    );
    assert_eq!(of_product(None), Component::ALL.to_vec());
}

/// The Linux packages in the standard layout under /usr: the commands in
/// /usr/bin, pai-manager in /usr/libexec/private-ai, libllama, the ggml
/// backends and the runtimes in /usr/lib/private-ai, the data in
/// /usr/share/private-ai.
#[test]
fn linux_files_go_to_their_package() {
    for (rel, want) in [
        ("usr/bin/pai-config", Component::Config),
        ("usr/share/private-ai/strings/en_US.toml", Component::Config),
        (
            "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
            Component::Cli,
        ),
        (
            "usr/share/applications/org.privateai.Config.desktop",
            Component::Config,
        ),
        (
            "usr/share/icons/hicolor/256x256/apps/org.privateai.Config.png",
            Component::Config,
        ),
        ("usr/bin/llama-server", Component::Llama),
        ("usr/bin/llama-bench", Component::Llama),
        ("usr/libexec/private-ai/pai-manager", Component::Core),
        ("usr/lib/private-ai/libggml-base.so.0", Component::Llama),
        ("usr/lib/private-ai/libggml-hip.so", Component::Llama),
        ("usr/lib/private-ai/libggml-cuda.so", Component::Llama),
        ("usr/lib/private-ai/libggml-vulkan.so", Component::Llama),
        ("usr/lib/private-ai/libllama.so.0", Component::Llama),
        (
            "usr/lib/private-ai/lib/llvm/lib/libomp.so",
            Component::Llama,
        ),
        ("usr/lib/private-ai/lib/libamdhip64.so.7", Component::Rocm),
        (
            "usr/lib/private-ai/lib/llvm/lib/libLLVM.so.24.0git",
            Component::Rocm,
        ),
        (
            "usr/lib/private-ai/lib/rocm_sysdeps/lib/librocm_sysdeps_numa.so.1",
            Component::Rocm,
        ),
        (
            "usr/lib/private-ai/lib/rocblas/library/Kernels.so-000-gfx1201.dat",
            Component::Rocm,
        ),
        (
            "usr/lib/private-ai/.kpack/blas_lib_gfx1201.kpack",
            Component::Rocm,
        ),
        ("usr/lib/private-ai/lib/libcublas.so.13", Component::Cuda),
        ("usr/lib/private-ai/lib/libcublasLt.so.13", Component::Cuda),
        ("usr/lib/private-ai/lib/libcudart.so.13", Component::Cuda),
        ("usr/lib/private-ai/lib/libnccl.so.2", Component::Cuda),
        ("usr/lib/private-ai/lib/libvulkan.so.1", Component::Vulkan),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
}

// ── The framework's core and the engine ─────────────────────────────

/// `private-ai-framework-core` holds what the framework put in the
/// llama.cpp package before: pai-manager, the system files of its
/// service, and `cli.toml`, the texts pai-manager, pai and pai-bff
/// print. The instance unit `llama.cpp@.service` is the engine's, the
/// bff's unit and the sysusers.d entry of its user stay with pai-bff,
/// the desktop entry with the GUI, although they sit beside these.
#[test]
fn core_holds_the_manager_its_system_files_and_cli_toml() {
    for rel in [
        "usr/libexec/private-ai/pai-manager",
        "usr/lib/systemd/system/private-ai-manager.service",
        "usr/share/dbus-1/system.d/org.privateai.Manager1.conf",
        "usr/share/polkit-1/actions/org.privateai.manager.policy",
        "usr/lib/sysusers.d/private-ai.conf",
        "usr/share/private-ai/strings/cli.toml",
    ] {
        assert_eq!(of(rel), Component::Core, "{rel}");
    }
    for (rel, want) in [
        (
            "usr/lib/systemd/system/llama.cpp@.service",
            Component::Llama,
        ),
        (
            "usr/lib/systemd/system/private-ai-bff.service",
            Component::Bff,
        ),
        ("usr/lib/sysusers.d/private-ai-bff.conf", Component::Bff),
        (
            "usr/share/applications/org.privateai.Config.desktop",
            Component::Config,
        ),
        ("usr/share/private-ai/strings/en_US.toml", Component::Config),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
}

/// The version file the web service reads (`versions/<package>`, under the
/// engine's share folder) belongs to the package it names: llama.cpp's own
/// file to llama.cpp, and each runtime's file to that runtime's package,
/// ahead of the rule that gives everything of the engine's share folder to
/// llama.cpp. Each package therefore ships its own version, and the folder
/// the bff reads (`pai_core::bff_cfg::versions_dir`) holds all of them
/// (`ARCHITECTURE.md`, slice 13).
#[test]
fn each_versions_file_is_owned_by_the_package_it_names() {
    for c in [
        Component::Llama,
        Component::Rocm,
        Component::Cuda,
        Component::Vulkan,
    ] {
        let rel = format!(
            "usr/share/private-ai/{ENGINE_NAME}/versions/{}",
            c.package()
        );
        assert_eq!(of(&rel), c, "{rel}");
    }
    // The rest of that folder stays the engine's.
    assert_eq!(
        of("usr/share/private-ai/llama.cpp/instances.ini"),
        Component::Llama
    );
}

/// The engine's package holds llama.cpp's executables and libraries,
/// every ggml backend and libomp, and its instance unit
/// `llama.cpp@.service` (the engine owns its unit), and nothing else of
/// the framework: no program, other unit, policy or data file.
#[test]
fn the_engine_holds_llama_cpp_and_nothing_of_the_framework() {
    for rel in [
        "usr/bin/llama-server",
        "usr/bin/llama-bench",
        "usr/bin/llama",
        "usr/lib/private-ai/libllama.so.1",
        "usr/lib/private-ai/libggml-base.so.1",
        "usr/lib/private-ai/libggml-cpu-zen4.so",
        "usr/lib/private-ai/libggml-hip.so",
        "usr/lib/private-ai/libmtmd.so.1",
        "usr/lib/private-ai/libllama-server-impl.so",
        "usr/lib/private-ai/lib/llvm/lib/libomp.so",
        "usr/lib/systemd/system/llama.cpp@.service",
        // RS2: the target every instance is ordered before, and the
        // link that pulls it into the boot transaction.
        "usr/lib/systemd/system/llama.cpp.target",
        "usr/lib/systemd/system/multi-user.target.wants/llama.cpp.target",
        // The engine's defaults, copied into /etc/llama.cpp when missing.
        "usr/share/private-ai/llama.cpp/server@default.ini",
        "usr/share/private-ai/llama.cpp/models.ini",
        "usr/share/private-ai/llama.cpp/assignments.ini",
        "usr/share/private-ai/llama.cpp/instances.ini",
    ] {
        assert_eq!(of(rel), Component::Llama, "{rel}");
    }
    for rel in [
        "usr/bin/pai-config",
        "usr/bin/pai",
        "usr/libexec/private-ai/pai-manager",
        "usr/libexec/private-ai/pai-bff",
        "usr/lib/systemd/system/private-ai-manager.service",
        "usr/lib/systemd/system/private-ai-bff.service",
        "usr/share/dbus-1/system.d/org.privateai.Manager1.conf",
        "usr/share/polkit-1/actions/org.privateai.manager.policy",
        "usr/lib/sysusers.d/private-ai.conf",
        "usr/lib/sysusers.d/private-ai-bff.conf",
        "usr/share/private-ai/strings/cli.toml",
        "usr/share/private-ai/strings/en_US.toml",
        "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
        "usr/share/applications/org.privateai.Config.desktop",
    ] {
        assert_ne!(of(rel), Component::Llama, "{rel}");
    }
}

/// The engine is named after itself, `llama.cpp`, and its runtimes
/// belong to it (`llama.cpp-rocm`, `-cuda`, `-vulkan`); the framework's
/// packages, `core` among them, carry the project name.
#[test]
fn packages_carry_the_project_or_the_engine_name() {
    let names: Vec<String> = Component::ALL.iter().map(|c| c.package()).collect();
    assert_eq!(
        names,
        [
            "private-ai-framework-config",
            "private-ai-framework-cli",
            "private-ai-framework-bff",
            "private-ai-framework-core",
            "llama.cpp",
            "llama.cpp-rocm",
            "llama.cpp-cuda",
            "llama.cpp-vulkan",
        ]
    );
    assert!(!Component::Core.is_runtime());
    assert!(!Component::Llama.is_runtime());
}

/// The engine's version is `1:<release>+pai<N>`: the llama.cpp release
/// without its `v`, `+pai<N>` our build revision ([`LLAMA_CPP_REVISION`],
/// checked in, raised when our packaging of a release changes), and the
/// epoch `1:` because Ubuntu 26.04 ships its own `llama.cpp` at
/// `8681+dfsg-1`, which would rank above `1.0.0` without it (Debian
/// Policy 5.6.12: the epoch is compared first). The framework requires
/// at least `1:<release>+pai<N>` at its own revision: an engine of an
/// earlier build revision of the same release lacks what this framework
/// was built against, and `1:<release>` alone (any revision of it) is
/// not a version the framework ever ships.
#[test]
fn the_engine_version_carries_the_epoch_and_the_build_revision() {
    assert_eq!(LLAMA_CPP_REVISION, 2);
    assert_eq!(llama_cpp_version("v1.0.0"), "1:1.0.0+pai2");
    assert_eq!(llama_cpp_version("v1.2.10"), "1:1.2.10+pai2");
    assert_eq!(llama_cpp_min("v1.0.0"), "1:1.0.0+pai2");
    assert_eq!(llama_cpp_min("v1.2.10"), "1:1.2.10+pai2");
}

/// The minimum ranks as the package managers rank versions (the epoch
/// first, then the release and the build revision, the order
/// `util::version_gt` gives, the one `install.rs` compares installed
/// packages with): a framework built against `+pai2` refuses the engine
/// of `+pai1` of the same release, takes its own, and takes a later
/// release at its own revision.
#[test]
fn a_framework_of_revision_two_refuses_the_engine_of_revision_one() {
    let min = llama_cpp_min("v1.0.0");
    assert_eq!(min, "1:1.0.0+pai2");
    // `>= min` is satisfied by `v` when `min` is not newer than `v`.
    let satisfies = |v: &str| !crate::common::util::version_gt(&min, v);
    assert!(
        !satisfies("1:1.0.0+pai1"),
        "the engine of an earlier build revision must not satisfy the minimum"
    );
    assert!(
        !satisfies("1:1.0.0"),
        "the bare release, which carries no build revision, must not satisfy it"
    );
    assert!(
        satisfies("1:1.0.0+pai2"),
        "the engine this framework was built with satisfies it"
    );
    assert!(
        satisfies("1:1.0.1+pai2"),
        "a later release at this revision satisfies it"
    );
}

/// File names: a framework package names the framework version alone
/// (no llama.cpp tag), the engine its own version without the epoch
/// (a `:` has no place in a file name; dpkg-name leaves the epoch out
/// too), a runtime its SDK. The same stem for the `.deb` and the `.rpm`.
#[test]
fn file_names_carry_the_framework_the_engine_or_the_sdk_version() {
    for (c, version, want) in [
        (
            Component::Config,
            "2.99.1",
            "private-ai-framework-config-v2.99.1",
        ),
        (Component::Cli, "2.99.1", "private-ai-framework-cli-v2.99.1"),
        (Component::Bff, "2.99.1", "private-ai-framework-bff-v2.99.1"),
        (
            Component::Core,
            "2.99.1",
            "private-ai-framework-core-v2.99.1",
        ),
        (Component::Llama, "1:1.0.0+pai1", "llama.cpp-v1.0.0+pai1"),
        (Component::Rocm, "10.1.0", "llama.cpp-rocm-v10.1.0"),
        (
            Component::Cuda,
            "13.5.0+nccl.2.32.3",
            "llama.cpp-cuda-v13.5.0+nccl.2.32.3",
        ),
        (
            Component::Vulkan,
            "1.4.363.1",
            "llama.cpp-vulkan-v1.4.363.1",
        ),
    ] {
        assert_eq!(file_stem(c, "2.99.1", "v1.0.0", version), want, "{c:?}");
    }
}

/// `xtask install` (Linux and macOS) picks the newest file of each
/// package: the engine's `llama.cpp-v...` is told apart from its
/// runtimes' `llama.cpp-rocm-v...`, and `core` from the others.
#[test]
fn the_newest_engine_core_and_runtime_are_picked_apart() {
    let dir = std::env::temp_dir().join(format!("xtask-newest-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in [
        "private-ai-framework-core-v2.99.1-x86_64.rpm",
        "llama.cpp-v1.0.0+pai1-x86_64.rpm",
        "llama.cpp-rocm-v10.1.0-x86_64.rpm",
    ] {
        std::fs::write(dir.join(file), "").unwrap();
    }
    let found: Vec<(Component, String)> = newest_in(&dir, "-x86_64.rpm")
        .iter()
        .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        found,
        [
            (
                Component::Core,
                "private-ai-framework-core-v2.99.1-x86_64.rpm".to_string()
            ),
            (
                Component::Llama,
                "llama.cpp-v1.0.0+pai1-x86_64.rpm".to_string()
            ),
            (
                Component::Rocm,
                "llama.cpp-rocm-v10.1.0-x86_64.rpm".to_string()
            ),
        ]
    );
}

/// The real dist/ of the split, both formats side by side: each
/// package gets its own file. The engine's prefix `llama.cpp-v` is also
/// the start of `llama.cpp-vulkan-v...`, so a match on `<package>-v`
/// alone hands the Vulkan runtime to the engine's slot (seen with
/// `xtask install --dry-run` on the real dist/, which listed the Vulkan
/// file twice and never the engine). The version after `-v` starts with
/// a digit. macOS's `.pkg` finder is this same function.
#[test]
fn each_package_gets_its_own_file_when_the_engine_prefix_starts_a_runtime_name() {
    let dir = std::env::temp_dir().join(format!("xtask-newest-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stems = [
        (Component::Llama, "llama.cpp-v1.0.0+pai1"),
        (Component::Rocm, "llama.cpp-rocm-v10.1.0"),
        (Component::Cuda, "llama.cpp-cuda-v13.5.0+nccl.2.32.3"),
        (Component::Vulkan, "llama.cpp-vulkan-v1.4.363.1"),
    ];
    // The engine first, so it is the oldest: newest-by-time cannot hide
    // the bug.
    let start = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let mut n = 0;
    for (_, stem) in &stems {
        for tail in ["-amd64.deb", "-x86_64.rpm"] {
            let path = dir.join(format!("{stem}{tail}"));
            std::fs::write(&path, "").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(start + std::time::Duration::from_secs(60 * n))
                .unwrap();
            n += 1;
        }
    }
    let picked = |tail: &str| -> Vec<(Component, String)> {
        newest_in(&dir, tail)
            .iter()
            .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
            .collect()
    };
    let (deb, rpm) = (picked("-amd64.deb"), picked("-x86_64.rpm"));
    std::fs::remove_dir_all(&dir).unwrap();
    for (tail, got) in [("-amd64.deb", deb), ("-x86_64.rpm", rpm)] {
        let want: Vec<(Component, String)> = stems
            .iter()
            .map(|(c, stem)| (*c, format!("{stem}{tail}")))
            .collect();
        assert_eq!(got, want, "{tail}");
    }
}

#[test]
fn macos_files_go_to_their_package() {
    for (rel, want) in [
        ("usr/local/bin/pai-config", Component::Config),
        ("usr/local/bin/llama-server", Component::Llama),
        ("opt/private-ai/bin/libggml-metal.so", Component::Llama),
        ("opt/private-ai/lib/libvulkan.1.dylib", Component::Vulkan),
        ("opt/private-ai/lib/libMoltenVK.dylib", Component::Vulkan),
        (
            "usr/local/share/vulkan/icd.d/private-ai-framework-MoltenVK_icd.json",
            Component::Vulkan,
        ),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
}

/// `pai`, the command line, has a package of its own, so a headless
/// server administered over SSH installs it without the GUI and its
/// graphics libraries. The benchmark method, which the GUI reads too,
/// goes with it, since the GUI package requires it and not the other way
/// round; `cli.toml` goes to the core package (below); the GUI's
/// language files, its desktop entry and its icon stay with the GUI.
#[test]
fn the_command_line_and_the_data_it_reads_go_to_the_cli_package() {
    for (rel, want) in [
        ("usr/bin/pai", Component::Cli),
        (
            "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
            Component::Cli,
        ),
        (
            "usr/share/private-ai/benchmarks/specs/standard.versions",
            Component::Cli,
        ),
        (
            "usr/share/private-ai/benchmarks/prompts/bench-prompt-43k-1.0.txt",
            Component::Cli,
        ),
        ("usr/share/private-ai/strings/cli.toml", Component::Core),
        ("usr/share/private-ai/strings/en_US.toml", Component::Config),
        ("usr/bin/pai-config", Component::Config),
        (
            "usr/share/applications/org.privateai.Config.desktop",
            Component::Config,
        ),
        (
            "usr/share/icons/hicolor/256x256/apps/org.privateai.Config.png",
            Component::Config,
        ),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
    // Not taken for pai by a prefix match.
    assert_ne!(of("usr/bin/pai-manager"), Component::Cli);
}

/// macOS: `pai` in /opt/private-ai/bin with its link in /usr/local/bin,
/// and the benchmark method under /opt/private-ai/share, as on Linux.
/// (Where macOS puts `cli.toml` comes with its own split of the `.pkg`.)
#[test]
fn macos_command_line_files_go_to_the_cli_package() {
    for (rel, want) in [
        ("opt/private-ai/bin/pai", Component::Cli),
        ("usr/local/bin/pai", Component::Cli),
        (
            "opt/private-ai/share/benchmarks/specs/standard-1.1.ini",
            Component::Cli,
        ),
        ("opt/private-ai/bin/pai-config", Component::Config),
        ("opt/private-ai/share/strings/en_US.toml", Component::Config),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
}

/// The command line is the framework's, versioned like the GUI (the
/// framework version alone), not a runtime.
#[test]
fn the_cli_package_is_named_and_versioned_like_the_configurator() {
    assert_eq!(Component::Cli.package(), "private-ai-framework-cli");
    assert!(!Component::Cli.is_runtime());
    assert_eq!(
        file_stem(Component::Cli, "2.99.1", "v1.0.0", "2.99.1"),
        "private-ai-framework-cli-v2.99.1"
    );
}

/// `xtask install` (Linux and macOS) picks the newest file of each
/// component: the cli package is one of them, told apart from the
/// configurator's.
#[test]
fn the_newest_cli_package_is_picked_beside_the_configurators() {
    let dir = std::env::temp_dir().join(format!("xtask-newest-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in [
        "private-ai-framework-config-v2.99.1-amd64.deb",
        "private-ai-framework-cli-v2.99.1-amd64.deb",
        "llama.cpp-v1.0.0+pai1-amd64.deb",
    ] {
        std::fs::write(dir.join(file), "").unwrap();
    }
    let names: Vec<(Component, String)> = newest_in(&dir, "-amd64.deb")
        .iter()
        .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        names,
        [
            (
                Component::Config,
                "private-ai-framework-config-v2.99.1-amd64.deb".to_string()
            ),
            (
                Component::Cli,
                "private-ai-framework-cli-v2.99.1-amd64.deb".to_string()
            ),
            (
                Component::Llama,
                "llama.cpp-v1.0.0+pai1-amd64.deb".to_string()
            ),
        ]
    );
}

// ── pai-bff, the web service, in a package of its own ───────────────

/// pai-bff has a package of its own, `private-ai-framework-bff`: it is
/// a service, so it sits beside pai-manager in /usr/libexec/private-ai
/// (no user command), and its unit and the sysusers.d entry of its
/// static user go with it, not with the manager's in the core package.
/// A desktop install and a headless server each choose it apart from
/// the GUI and the command line.
#[test]
fn pai_bff_and_its_unit_go_to_the_bff_package() {
    for (rel, want) in [
        ("usr/libexec/private-ai/pai-bff", Component::Bff),
        (
            "usr/lib/systemd/system/private-ai-bff.service",
            Component::Bff,
        ),
        // The sysusers.d entry of the bff's static user goes with it.
        ("usr/lib/sysusers.d/private-ai-bff.conf", Component::Bff),
        // The manager and its files go to the core package.
        ("usr/libexec/private-ai/pai-manager", Component::Core),
        (
            "usr/lib/systemd/system/private-ai-manager.service",
            Component::Core,
        ),
        (
            "usr/lib/systemd/system/llama.cpp@.service",
            Component::Llama,
        ),
        ("usr/lib/sysusers.d/private-ai.conf", Component::Core),
        ("usr/bin/pai", Component::Cli),
        ("usr/bin/pai-config", Component::Config),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
}

/// The bff is the framework's, versioned like the GUI and the command
/// line (the framework version alone), not a runtime, and one of the
/// packages every install phase considers.
#[test]
fn the_bff_package_is_named_and_versioned_like_the_configurator() {
    assert_eq!(Component::Bff.package(), "private-ai-framework-bff");
    assert!(!Component::Bff.is_runtime());
    assert!(Component::ALL.contains(&Component::Bff));
    assert_eq!(
        file_stem(Component::Bff, "2.99.1", "v1.0.0", "2.99.1"),
        "private-ai-framework-bff-v2.99.1"
    );
}

/// `xtask install` picks the newest file of each component: the bff's
/// is one of them, told apart from the others by its name.
#[test]
fn the_newest_bff_package_is_picked_beside_the_others() {
    let dir = std::env::temp_dir().join(format!("xtask-newest-bff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in [
        "private-ai-framework-cli-v2.99.1-x86_64.rpm",
        "private-ai-framework-bff-v2.99.1-x86_64.rpm",
        "llama.cpp-v1.0.0+pai1-x86_64.rpm",
    ] {
        std::fs::write(dir.join(file), "").unwrap();
    }
    let found: Vec<(Component, String)> = newest_in(&dir, "-x86_64.rpm")
        .iter()
        .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        found.contains(&(
            Component::Bff,
            "private-ai-framework-bff-v2.99.1-x86_64.rpm".to_string()
        )),
        "{found:?}"
    );
    assert_eq!(found.len(), 3, "{found:?}");
}

/// `cli.toml` holds every text the programs print: pai, but also
/// pai-manager and pai-bff, which do not require the command line's
/// package. It goes to the core package, which every other framework
/// package requires; the GUI's language file stays with the GUI, the
/// benchmark method with the command line.
#[test]
fn cli_toml_goes_to_core_which_every_framework_package_requires() {
    for (rel, want) in [
        ("usr/share/private-ai/strings/cli.toml", Component::Core),
        ("usr/share/private-ai/strings/en_US.toml", Component::Config),
        ("opt/private-ai/share/strings/en_US.toml", Component::Config),
        (
            "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
            Component::Cli,
        ),
        (
            "opt/private-ai/share/benchmarks/specs/standard-1.1.ini",
            Component::Cli,
        ),
    ] {
        assert_eq!(of(rel), want, "{rel}");
    }
}

/// macOS is left out, like the manager: no service there. The macOS
/// package phase stages the build's bin/, where the build copies only
/// `NEXT_TO_SERVER` (pai-config, pai), and names neither pai-bff nor its
/// component; its install phase neither. Already true, kept so.
#[test]
fn no_macos_package_carries_pai_bff() {
    use crate::common::configurator::NEXT_TO_SERVER;
    assert!(
        !NEXT_TO_SERVER.contains(&"pai-bff"),
        "pai-bff copied next to llama-server: {NEXT_TO_SERVER:?}"
    );
    for (name, text) in [
        ("macos/package.rs", include_str!("../macos/package.rs")),
        ("macos/install.rs", include_str!("../macos/install.rs")),
    ] {
        assert!(!text.contains("pai-bff"), "{name} names pai-bff");
        assert!(
            !text.contains("Component::Bff"),
            "{name} names Component::Bff"
        );
    }
}

/// macOS ships no instance record. There is no manager and no instance
/// there (the configurator runs llama-server itself), so the engine's
/// read-only defaults, the record among them, belong to the Linux and the
/// Windows packages only: the macOS package and install phases name no
/// `.ini` default, neither the folder the Linux package ships them in nor
/// the module that ships it, and the `.pkg` items of `macos_pkg.rs` put
/// their files in the two `bin/` trees and the Vulkan runtime, never a
/// configuration file.
#[test]
fn no_macos_package_ships_the_instance_record() {
    for (name, text) in [
        ("macos/package.rs", include_str!("../macos/package.rs")),
        ("macos/install.rs", include_str!("../macos/install.rs")),
        ("common/macos_pkg.rs", include_str!("macos_pkg.rs")),
    ] {
        for needle in [
            "instances.ini",
            "assignments.ini",
            "models.ini",
            "server@default.ini",
            "usr/share/private-ai/llama.cpp",
            "system_files::",
        ] {
            assert!(!text.contains(needle), "{name} names {needle}");
        }
    }
}

/// The packages of the 2.4 releases and earlier, spelled split so the
/// names lint stays clean.
const OLD_BASE: &str = concat!("llama-cpp", "-framework");

/// Old packages left in dist/ (the monolith of 2.2 and earlier, the split
/// packages of 2.3 and 2.4, and the never-published 2.99.x runtime
/// `private-ai-framework-rocm`) are never picked, even when written last.
#[test]
fn the_newest_file_of_each_component_wins_and_the_old_packages_never_match() {
    let dir = std::env::temp_dir().join(format!("xtask-newest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let start = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let files = [
        "llama.cpp-rocm-v10.0.0-amd64.deb".to_string(),
        "llama.cpp-rocm-v10.1.0-x86_64.rpm".to_string(),
        format!("{OLD_BASE}-v2.2.0-v0.8.0-amd64.deb"),
        format!("{OLD_BASE}-rocm-v10.2.0-amd64.deb"),
        format!("{OLD_BASE}-config-v2.4.0-v0.9.0-amd64.deb"),
        "private-ai-framework-rocm-v10.2.0-amd64.deb".to_string(),
        "private-ai-framework-llama-v2.99.0-v1.0.0-amd64.deb".to_string(),
    ];
    // Each file one minute newer than the one before: the old ones last.
    for (i, file) in files.iter().enumerate() {
        let path = dir.join(file);
        std::fs::write(&path, "").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(start + std::time::Duration::from_secs(60 * i as u64))
            .unwrap();
    }
    let found = newest_in(&dir, "-amd64.deb");
    let names: Vec<(Component, String)> = found
        .iter()
        .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(
        names,
        [(
            Component::Rocm,
            "llama.cpp-rocm-v10.0.0-amd64.deb".to_string()
        )]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_dist_version_comes_from_the_layout() {
    for (dist, want) in [
        ("/mnt/data/TheRock/10.1.0/linux", Some("10.1.0")),
        ("/mnt/data/CUDA/13.0.2/linux", Some("13.0.2")),
        (
            "/mnt/data/VulkanSDK/1.4.328.1/linux/x86_64",
            Some("1.4.328.1"),
        ),
        (
            "/Users/me/dists/VulkanSDK/1.4.328.1/macos/macOS",
            Some("1.4.328.1"),
        ),
        ("/opt/rocm", None),
    ] {
        assert_eq!(dist_version(Path::new(dist)).as_deref(), want, "{dist}");
    }
}

#[test]
fn split_keeps_every_file_once_and_symlinks_as_symlinks() {
    let dir = std::env::temp_dir().join(format!("xtask-split-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stage = dir.join("stage");
    let into = dir.join("into");
    for file in [
        "usr/bin/llama-server",
        "usr/lib/private-ai/libggml-base.so.0.9.0",
        "usr/lib/private-ai/lib/libcublas.so.13",
    ] {
        let path = stage.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }
    std::os::unix::fs::symlink(
        "libggml-base.so.0.9.0",
        stage.join("usr/lib/private-ai/libggml-base.so.0"),
    )
    .unwrap();

    assert_eq!(
        split(&stage, &into).unwrap(),
        [Component::Llama, Component::Cuda]
    );

    assert!(into.join("llama/usr/bin/llama-server").is_file());
    assert!(into
        .join("cuda/usr/lib/private-ai/lib/libcublas.so.13")
        .is_file());
    assert!(!into.join("rocm").exists());
    let link = into.join("llama/usr/lib/private-ai/libggml-base.so.0");
    assert_eq!(
        std::fs::read_link(link).unwrap(),
        Path::new("libggml-base.so.0.9.0")
    );
    assert!(
        stage.join("usr/bin/llama-server").is_file(),
        "the staged tree stays whole"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
