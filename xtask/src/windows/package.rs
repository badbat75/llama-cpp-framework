//! Package phase on Windows (cargo run -p xtask -- package): stages the llama.cpp build and
//! llama-cpp-config into build\windows\staging\ (the GPU runtimes the
//! backends load included, copied out of the dists), builds the MSI into
//! .\dist\ with the WiX Toolset pinned in installer\wix-toolset.psd1 and
//! validates it. installer\AGENTS.md says what the MSI does with the tree.

use super::build_llama::load_build_env;
use crate::common::ui;
use crate::common::version;
use crate::common::util::{self, cmd, gfx_arch, run as run_cmd};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

const GIB: u64 = 1 << 30;

fn name_of(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Every file under `dir`, recursively, hidden ones included.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// The files directly in `dir`.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
        .unwrap_or_default()
}

fn is_pe(p: &Path) -> bool {
    p.extension()
        .map(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("dll"))
        .unwrap_or(false)
}

/// Is there a file in `dir` named `<prefix>*<suffix>` (case ignored, as
/// Windows does)?
fn any_like(dir: &Path, prefix: &str, suffix: &str) -> bool {
    let (p, s) = (prefix.to_ascii_lowercase(), suffix.to_ascii_lowercase());
    files_in(dir).iter().any(|f| {
        let n = name_of(f).to_ascii_lowercase();
        n.len() >= p.len() + s.len() && n.starts_with(&p) && n.ends_with(&s)
    })
}

/// Copies a file or a directory tree into `dest_dir`, under its own name.
fn copy_into(src: &Path, dest_dir: &Path) -> Result<()> {
    let dest = dest_dir.join(src.file_name().unwrap_or_default());
    if src.is_dir() {
        std::fs::create_dir_all(&dest)?;
        for e in std::fs::read_dir(src)?.flatten() {
            copy_into(&e.path(), &dest)?;
        }
    } else {
        std::fs::copy(src, &dest).with_context(|| format!("copying {} to {}", src.display(), dest.display()))?;
    }
    Ok(())
}

fn size_mib(dir: &Path) -> u64 {
    walk(dir).iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum::<u64>() / (1 << 20)
}

/// The OpenSSL DLL names a binary mentions (`libcrypto-N-x64.dll`,
/// `libssl-N-x64.dll`), read from its raw bytes in chunks: ggml-hip.dll alone
/// is ~1 GB. The search for the prefix gates the match, so only the few chunks
/// that could hold a name are looked at; the overlap carried between chunks is
/// longer than any name, so one straddling a boundary is still seen.
fn openssl_imports(path: &Path) -> Result<BTreeSet<String>> {
    const OVERLAP: usize = 64;
    let mut found = BTreeSet::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 4 << 20];
    let mut chunk: Vec<u8> = Vec::new();
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        chunk.extend_from_slice(&buf[..n]);
        for prefix in [&b"libcrypto-"[..], &b"libssl-"[..]] {
            let mut i = 0;
            while let Some(pos) = find(&chunk[i..], prefix) {
                let start = i + pos;
                if let Some(name) = openssl_name_at(&chunk[start..], prefix.len()) {
                    found.insert(name);
                }
                i = start + 1;
            }
        }
        let keep = chunk.len().min(OVERLAP);
        chunk.drain(..chunk.len() - keep);
    }
    Ok(found)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// `(?:libcrypto|libssl)-\d+-x64\.dll` at the start of `s`.
fn openssl_name_at(s: &[u8], prefix_len: usize) -> Option<String> {
    let digits = s[prefix_len..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let end = prefix_len + digits;
    let tail = b"-x64.dll";
    (s.len() >= end + tail.len() && &s[end..end + tail.len()] == tail)
        .then(|| String::from_utf8_lossy(&s[..end + tail.len()]).into_owned())
}

/// The DLL names a PE32+ image imports, normal and delay-load, read from its
/// import directories. Headers only: ggml-hip.dll is ~1 GB.
fn pe_imports(path: &Path) -> Result<Vec<String>> {
    let mut f = std::fs::File::open(path)?;
    let mut rd = |off: u64, n: usize| -> Result<Vec<u8>> {
        f.seek(SeekFrom::Start(off))?;
        let mut b = vec![0u8; n];
        f.read_exact(&mut b)?;
        Ok(b)
    };
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

    let pe = u32_at(&rd(0x3C, 4)?, 0) as u64;
    let coff = rd(pe, 24)?;
    let section_count = u16_at(&coff, 6) as u64;
    let opt_size = u16_at(&coff, 20) as u64;
    let opt = pe + 24;
    let opt_hdr = rd(opt, opt_size as usize)?;
    if u16_at(&opt_hdr, 0) != 0x20B {
        return Ok(Vec::new()); // PE32+ only
    }
    // (VirtualSize, VirtualAddress, SizeOfRawData, PointerToRawData)
    let mut sections = Vec::new();
    for i in 0..section_count {
        let s = rd(opt + opt_size + 40 * i + 8, 16)?;
        sections.push((u32_at(&s, 0) as u64, u32_at(&s, 4) as u64, u32_at(&s, 8) as u64, u32_at(&s, 12) as u64));
    }
    let to_offset = |rva: u64| {
        sections
            .iter()
            .find(|(vsize, vaddr, raw, _)| rva >= *vaddr && rva < vaddr + (*vsize).max(*raw))
            .map(|(_, vaddr, _, ptr)| rva - vaddr + ptr)
    };

    let mut names = Vec::new();
    // Data directory 1 = imports (20-byte descriptors, name RVA at +12),
    // 13 = delay imports (32-byte descriptors, name RVA at +4); each table
    // ends at a descriptor with no name.
    for (index, size, name_at) in [(1u64, 20u64, 12u64), (13, 32, 4)] {
        let dir_off = 112 + 8 * index as usize;
        if dir_off + 4 > opt_hdr.len() {
            continue;
        }
        let rva = u32_at(&opt_hdr, dir_off) as u64;
        if rva == 0 {
            continue;
        }
        let Some(table) = to_offset(rva) else { continue };
        for i in 0.. {
            let name_rva = u32_at(&rd(table + size * i + name_at, 4)?, 0) as u64;
            if name_rva == 0 {
                break;
            }
            let Some(off) = to_offset(name_rva) else { break };
            let mut name = Vec::new();
            let mut at = off;
            loop {
                let b = rd(at, 1)?[0];
                if b == 0 {
                    break;
                }
                name.push(b);
                at += 1;
            }
            names.push(String::from_utf8_lossy(&name).into_owned());
        }
    }
    Ok(names)
}

/// Base64 of the UTF-16LE text, what powershell.exe -EncodedCommand decodes.
fn encoded_command(text: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The custom actions' script as an -EncodedCommand value: no script file to
/// extract at install time, no quoting of a script body. Comment-only and
/// blank lines are dropped to keep the command line short.
fn script_command(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let body: Vec<&str> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.is_empty() || t.starts_with('#'))
        })
        .collect();
    Ok(encoded_command(&body.join("\n")))
}

pub fn run(keep_staging: bool, wix_arg: Option<PathBuf>) -> Result<()> {
    let (cfg, dists) = load_build_env()?; // cmake --install needs the VS env
    let repo = util::repo_root();

    // ── Versions ────────────────────────────────────────────────────────
    let framework_version = version::framework()?;
    let llama_build = version::llama_build(Path::new(&cfg.llama_cpp_dir))?;

    // Architecture token for the package name (native 64-bit build).
    let is_arm = ["PROCESSOR_ARCHITEW6432", "PROCESSOR_ARCHITECTURE"]
        .iter()
        .find_map(|v| std::env::var(v).ok())
        .map(|a| a.eq_ignore_ascii_case("ARM64"))
        .unwrap_or(false);
    let arch = if is_arm { "arm64" } else { "x64" };

    ui::head(format!("Framework version: {framework_version}"));
    ui::head(format!("llama.cpp release: {llama_build}"));
    ui::head(format!("Architecture:      {arch}"));

    // ── WiX, checked BEFORE staging ─────────────────────────────────────
    // A missing tool or an unaccepted EULA costs seconds, not a staging run.
    // Resolution: --wix, then the configured one, then PATH and the dotnet
    // tools directory.
    let pin = super::wix_pin()?;
    let wix = [wix_arg, Some(PathBuf::from(&cfg.wix_exe)).filter(|p| !p.as_os_str().is_empty()), super::find_wix()]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
        .with_context(|| {
            format!(
                "WiX Toolset not found. Run scripts\\00-install-prerequisites.ps1 (it installs WiX {} as a dotnet tool), then cargo run -p xtask -- configure.",
                pin.version
            )
        })?;
    let wix_version = super::wix_version(&wix).with_context(|| format!("{} --version failed", wix.display()))?;
    if wix_version != pin.version {
        ui::warn(format!(
            "WiX {wix_version} at {} is not the pinned {} (installer\\wix-toolset.psd1)",
            wix.display(),
            pin.version
        ));
    }
    // Any command but --version doubles as the EULA check (v7+: WIX7015 until
    // `wix eula accept wix7`); this never accepts it on the user's behalf.
    let list = cmd(&wix).args(["extension", "list", "-g"]).output()?;
    let list_text = format!("{}{}", String::from_utf8_lossy(&list.stdout), String::from_utf8_lossy(&list.stderr));
    if !list.status.success() {
        if list_text.contains("WIX7015") {
            bail!(
                "WiX {wix_version} needs its EULA accepted once per user before it builds anything. Read it (https://opensourcemaintenancefee.org), then run: wix eula accept {}",
                pin.eula_id
            );
        }
        bail!("wix extension list failed: {}", list_text.trim());
    }
    for ext in &pin.extensions {
        let want = format!("{ext} {wix_version}");
        let present = list_text.lines().any(|l| {
            l.starts_with(&want)
                && !l[want.len()..].chars().next().map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false)
        });
        if !present {
            ui::warn(format!("Adding WiX extension {ext}/{wix_version}..."));
            run_cmd(cmd(&wix).args(["extension", "add", "-g", &format!("{ext}/{wix_version}")]))?;
        }
    }
    ui::head(format!("WiX:               {wix_version} ({})", wix.display()));

    // ── Stage llama.cpp with cmake --install ────────────────────────────
    let build_root = util::build_root();
    let build_dir = build_root.join("llama.cpp-cmake");
    let stage = build_root.join("staging");
    let stage_bin = stage.join("bin");
    let out_dir = repo.join("dist");
    util::remove_all(&stage)?;
    std::fs::create_dir_all(&stage)?;
    std::fs::create_dir_all(&out_dir)?;

    ui::head("Staging llama.cpp binaries...");
    run_cmd(cmd("cmake").arg("--install").arg(&build_dir).arg("--prefix").arg(&stage))?;

    // llama.cpp's own test harnesses (test-*.exe, ~11 MB): unit tests, and
    // unwanted here because the installer offers to put bin\ on the SYSTEM
    // PATH. The C headers go too: the installer does not ship them, and the
    // MSI harvests the staging tree whole.
    let tests: Vec<PathBuf> = files_in(&stage_bin)
        .into_iter()
        .filter(|p| {
            let n = name_of(p).to_ascii_lowercase();
            n.starts_with("test") && n.ends_with(".exe")
        })
        .collect();
    if !tests.is_empty() {
        let bytes: u64 = tests.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
        for t in &tests {
            std::fs::remove_file(t)?;
        }
        ui::dim(format!("Pruned {} llama.cpp test binaries ({:.1} MB)", tests.len(), bytes as f64 / (1 << 20) as f64));
    }
    util::remove_all(&stage.join("include"))?;

    // ── The OpenSSL runtime next to llama-server.exe ────────────────────
    // llama-server links OpenSSL dynamically; on a machine without OpenSSL
    // it fails at load, so those DLLs ship in bin\ (the application dir wins
    // the DLL search order). The ABI-versioned names are read from EVERY
    // staged .exe and .dll, not a name pattern: llama.cpp moves its imports
    // between DLLs (llama-server.exe is a thin launcher of
    // llama-server-impl.dll since v0.2.0).
    let mut ssl_sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for bin in walk(&stage_bin).into_iter().filter(|p| is_pe(p)) {
        for name in openssl_imports(&bin)? {
            ssl_sources.entry(name).or_default().push(name_of(&bin));
        }
    }
    if ssl_sources.is_empty() {
        bail!("No OpenSSL import names found in ANY staged binary: import scan broken or llama.cpp stopped linking OpenSSL; refusing to package a server that may not start on clean machines");
    }
    for (name, users) in &ssl_sources {
        let src = Path::new(&cfg.openssl_dir).join("bin").join(name);
        if !src.is_file() {
            bail!(
                "OpenSSL runtime {name} not found in {}\\bin, but {} imports it and would fail to load on machines without OpenSSL",
                cfg.openssl_dir,
                users.join(", ")
            );
        }
        std::fs::copy(&src, stage_bin.join(name))?;
        ui::dim(format!("Staged {name} (OpenSSL runtime, imported by {})", users.join(", ")));
    }

    // ── Bundle the GPU runtimes ─────────────────────────────────────────
    // The import closure of the staged binaries, restricted to what
    // resolves inside a dist (ROCm, CUDA, Vulkan SDK): everything else is a
    // system library and stays one (Windows' DLLs, the VC++ runtime, the
    // driver's vulkan-1.dll). Flat in bin\, next to llama-server.exe, which
    // precedes System32 in the search order, so the bundled amdhip64_*.dll
    // also wins over the driver's copy and the dist's rocBLAS never runs
    // against the DRIVER's HIP runtime (the mix that aborts BF16/F16 models on
    // gfx1201 at the first prefill batch).
    let hip_root = Some(PathBuf::from(&cfg.hip_path)).filter(|p| !cfg.hip_path.is_empty() && p.is_dir());
    let mut runtime_dirs: Vec<(&str, Vec<PathBuf>)> = Vec::new();
    if let Some(h) = &hip_root {
        runtime_dirs.push(("ROCm", vec![h.join("bin")]));
    }
    if let Some(c) = dists.find_cuda() {
        runtime_dirs.push(("CUDA", vec![c.join(r"bin\x64"), c.join("bin")]));
    }
    if let Some(v) = dists.find_vulkan() {
        runtime_dirs.push(("Vulkan", vec![v.join("Bin")]));
    }

    ui::head("Bundling the GPU runtimes...");
    let mut bundled: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut pending: VecDeque<PathBuf> = files_in(&stage_bin).into_iter().filter(|p| is_pe(p)).collect();
    while let Some(bin) = pending.pop_front() {
        for name in pe_imports(&bin)? {
            let dest = stage_bin.join(&name);
            if dest.exists() {
                continue;
            }
            for (dist, dirs) in &runtime_dirs {
                let Some(src) = dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file()) else { continue };
                std::fs::copy(&src, &dest)?;
                pending.push_back(dest);
                bundled.entry(dist).or_default().push(name.clone());
                break;
            }
        }
    }

    // A backend whose runtime did not resolve would be dropped silently by
    // ggml on every machine that lacks the SDK, which is every machine but
    // this one.
    for (backend, prefix, suffix) in [("ggml-hip.dll", "hipblas", ".dll"), ("ggml-cuda.dll", "cublas64_", ".dll")] {
        let runtime_present = if prefix == "hipblas" {
            stage_bin.join("hipblas.dll").is_file()
        } else {
            any_like(&stage_bin, prefix, suffix)
        };
        if stage_bin.join(backend).is_file() && !runtime_present {
            bail!(
                "{backend} is staged but {prefix}{} was not found in its dist under {}; refusing to package a backend that cannot load",
                if prefix == "hipblas" { ".dll" } else { "*.dll" },
                dists.root.display()
            );
        }
    }

    // What no import table shows, ROCm only:
    //   - amd_comgr*.dll: the HIP runtime asks for it by bare name at run
    //     time, and System32 holds the DRIVER's build (with a Ryzen iGPU
    //     visible that mix makes hipGetDeviceCount fail).
    //   - the kernel stores: rocBLAS and hipBLASLt load their precompiled
    //     kernels per GPU architecture from rocblas\library and
    //     hipblaslt\library beside the DLLs, and the kpack archives sit in
    //     <dist>\.kpack, which is <bin>\..\.kpack here as there. Only the BLAS
    //     archives, and only the architectures this build targets; entries
    //     that name no architecture are shared and always copied.
    if let Some(hip) = hip_root.as_ref().filter(|_| any_like(&stage_bin, "amdhip64_", ".dll")) {
        let hip_bin = hip.join("bin");
        for f in files_in(&hip_bin) {
            let n = name_of(&f);
            let l = n.to_ascii_lowercase();
            if l.starts_with("amd_comgr") && l.ends_with(".dll") {
                std::fs::copy(&f, stage_bin.join(&n))?;
                bundled.entry("ROCm").or_default().push(n);
            }
        }
        let targets: BTreeSet<String> =
            cfg.gpu_targets.split(';').filter(|t| !t.is_empty()).map(|t| t.to_ascii_lowercase()).collect();
        let wanted = |name: &str| gfx_arch(name).map(|a| targets.contains(&a.to_ascii_lowercase())).unwrap_or(true);
        for sub in [r"rocblas\library", r"hipblaslt\library"] {
            let from = hip_bin.join(sub);
            let Ok(rd) = std::fs::read_dir(&from) else { continue };
            let to = stage_bin.join(sub);
            std::fs::create_dir_all(&to)?;
            for e in rd.flatten() {
                if wanted(&e.file_name().to_string_lossy()) {
                    copy_into(&e.path(), &to)?;
                }
            }
            ui::dim(format!("  {sub}: {} MiB", size_mib(&to)));
        }
        let kpack = hip.join(".kpack");
        if kpack.is_dir() {
            let to = stage.join(".kpack");
            std::fs::create_dir_all(&to)?;
            for f in files_in(&kpack) {
                let n = name_of(&f);
                if n.starts_with("blas_lib_") && n.ends_with(".kpack") && wanted(&n) {
                    std::fs::copy(&f, to.join(&n))?;
                }
            }
            ui::dim(format!("  .kpack: {} MiB", size_mib(&to)));
        }
    }
    for (dist, _) in &runtime_dirs {
        let mut names = bundled.get(dist).cloned().unwrap_or_default();
        names.sort();
        let what = if names.is_empty() {
            "nothing to bundle (its runtime is a system library)".to_string()
        } else {
            names.join(", ")
        };
        ui::dim(format!("  {dist}: {what}"));
    }

    // ── llama-cpp-config and the icon ───────────────────────────────────
    // Straight from cargo's release output, into bin\ where it is installed:
    // the staging tree is the installed tree, file for file.
    let config_exe = build_root.join(r"cargo-target\release\llama-cpp-config.exe");
    if !config_exe.is_file() {
        bail!("llama-cpp-config.exe not found at {}. Run cargo run -p xtask -- build first.", config_exe.display());
    }
    std::fs::copy(&config_exe, stage_bin.join("llama-cpp-config.exe"))?;
    ui::dim("Staged llama-cpp-config.exe");

    // The icon (the Apps list entry, the Start Menu shortcut, the MSI's
    // own): generated, not checked in; regenerated here if it went missing.
    let resources = repo.join("resources");
    let ico = resources.join("llama.ico");
    if !ico.is_file() {
        ui::head("llama.ico missing: regenerating from the llama.cpp webui logo...");
        if !resources.join(r"node_modules\sharp-ico").is_dir() {
            let npm = util::which("npm").context("npm not found: node + npm generate resources\\llama.ico")?;
            run_cmd(cmd(npm).current_dir(&resources).args(["install", "--no-save", "sharp", "sharp-ico"]))?;
        }
        run_cmd(cmd("node").current_dir(&resources).arg("generate-llama-ico.mjs").arg(&cfg.llama_cpp_dir))?;
    }
    std::fs::copy(&ico, stage_bin.join("llama.ico"))?;

    // ── The MSI ─────────────────────────────────────────────────────────
    // e.g. llama-cpp-framework-v2.2.0-v0.7.0-x64.msi: the framework's own
    // version first, the bundled llama.cpp release second.
    let installer = format!("llama-cpp-framework-v{framework_version}-{llama_build}-{arch}.msi");
    let out_file = out_dir.join(&installer);
    let wxs = repo.join(r"installer\llama-cpp-framework.wxs");
    let obj = build_root.join("wix-obj");

    let files = walk(&stage);
    let payload: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
    let defines: Vec<(&str, String)> = vec![
        ("Version", framework_version.clone()),
        ("LlamaBuild", llama_build.clone()),
        ("StagingDir", stage.display().to_string()),
        ("HasLib", (stage.join("lib").exists() as u8).to_string()),
        ("HasKpack", (stage.join(".kpack").exists() as u8).to_string()),
        ("EstimatedSizeKb", payload.div_ceil(1024).to_string()),
        ("StopProcessesCmd", script_command(&repo.join(r"installer\msi-stop-processes.ps1"))?),
    ];
    // The .wixpdb (debug symbols of the package) goes with the
    // intermediates, not beside the .msi in dist\.
    let mut c = cmd(&wix);
    c.args(["build", "-arch", "x64", "-intermediatefolder"])
        .arg(&obj)
        .arg("-pdb")
        .arg(obj.join("package.wixpdb"))
        .arg("-o")
        .arg(&out_file);
    for ext in &pin.extensions {
        c.arg("-ext").arg(format!("{ext}/{wix_version}"));
    }
    for (k, v) in &defines {
        c.arg("-d").arg(format!("{k}={v}"));
    }
    c.arg(&wxs);

    ui::head(format!(
        "Building the MSI ({:.2} GB of payload, {} files)...",
        payload as f64 / GIB as f64,
        files.len()
    ));
    util::remove_all(&out_file)?;
    let t = Instant::now();
    run_cmd(&mut c)?;
    let secs = t.elapsed().as_secs();
    ui::dim(format!("wix build: {:02}:{:02}", secs / 60, secs % 60));

    // GitHub refuses release assets of 2 GiB or more, and the package must
    // stay ONE file (embedded cabinets): fail rather than publish what
    // cannot upload.
    let msi_bytes = std::fs::metadata(&out_file)?.len();
    if msi_bytes >= 2 * GIB {
        bail!(
            "{installer} is {:.2} GiB, over GitHub's 2 GiB release-asset limit. Options: fewer HIP targets (GpuTargets), a higher compression level, or splitting the ROCm kernel stores into a separate package.",
            msi_bytes as f64 / GIB as f64
        );
    }

    // ICE validation; the suppressed ICEs, and why, are in installer\AGENTS.md.
    ui::head("Validating the MSI (ICE)...");
    run_cmd(cmd(&wix).args(["msi", "validate", "-sice", "ICE61"]).arg(&out_file))?;

    let _ = util::remove_all(&obj);
    if !keep_staging {
        util::remove_all(&stage)?;
    }
    let secs = t.elapsed().as_secs();
    ui::say("");
    ui::ok(format!(
        "Installer created: {} ({:.1} MB, {:02}:{:02})",
        out_file.display(),
        msi_bytes as f64 / (1 << 20) as f64,
        secs / 60,
        secs % 60
    ));
    ui::say("");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_powershell() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('ab'))
        assert_eq!(encoded_command("ab"), "YQBiAA==");
        assert_eq!(encoded_command("abc"), "YQBiAGMA");
    }

    #[test]
    fn finds_openssl_names() {
        let s = b"libssl-3-x64.dll";
        assert_eq!(openssl_name_at(s, 7).as_deref(), Some("libssl-3-x64.dll"));
        assert_eq!(openssl_name_at(b"libssl--x64.dll", 7), None);
    }
}
