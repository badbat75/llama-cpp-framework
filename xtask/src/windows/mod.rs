//! The Windows phases configure, build, package and install (`cargo run -p
//! xtask -- <phase>`). scripts\00-install-prerequisites.ps1 stays a PowerShell script:
//! it is the bootstrap that installs PowerShell 7, the .NET SDK and Rust
//! itself, and it installs the GPU dists through one UAC-elevated batch.
//!
//! The build config stays a PowerShell data file,
//! build\windows\config-build.psd1, with the keys and the layout
//! the configure phase always wrote: scripts\00-install-prerequisites.ps1 and
//! tools\pcie-bench\build.ps1 (through common.ps1) still read it with
//! Import-PowerShellDataFile. This module writes it and reads it back.

pub mod build_llama;
pub mod configure;
pub mod dists;
pub mod install;
pub mod package;

use crate::common::{ui, util};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `build config`: llama-cpp-config, in the VS environment as it always
/// was. Not copied anywhere: the package phase stages it straight from
/// cargo's release dir.
pub fn build_config() -> Result<()> {
    let cfg = WinConfig::load()?;
    vs_env(&cfg)?;
    let target = crate::common::configurator::build(&cfg.llama_cpp_dir, cfg.build_jobs)?;
    ui::ok(format!("llama-cpp-config build complete: {}", target.join("release").join("llama-cpp-config.exe").display()));
    ui::say("");
    Ok(())
}

/// build\windows\config-build.psd1, the machine-derived half of the build.
#[derive(Default)]
pub struct WinConfig {
    pub dist_root: String,
    pub llama_cpp_dir: String,
    pub openssl_dir: String,
    pub hip_path: String,
    pub vs_dev_shell: String,
    pub wix_exe: String,
    pub gpu_targets: String,
    pub build_type: String,
    pub c_compiler: String,
    pub cxx_compiler: String,
    pub linker: String,
    pub build_jobs: u32,
}

pub fn config_path() -> PathBuf {
    util::build_root().join("config-build.psd1")
}

/// A single-quoted PowerShell string: the quotes inside it doubled.
fn psq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The scalar entries of a PowerShell data file, `Key = 'value'` or
/// `Key = [int]N` / `Key = N`, one per line, which is every shape the files
/// this crate reads use. Comments and anything else are skipped.
pub fn psd1_scalars(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let t = line.trim();
        let Some((key, value)) = t.split_once('=') else { continue };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let v = value.trim();
        if let Some(rest) = v.strip_prefix('\'') {
            // Up to the closing quote; '' inside is one quote.
            let mut s = String::new();
            let mut chars = rest.chars().peekable();
            let mut closed = false;
            while let Some(c) = chars.next() {
                if c == '\'' {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                        s.push('\'');
                        continue;
                    }
                    closed = true;
                    break;
                }
                s.push(c);
            }
            if closed {
                out.insert(key.to_string(), s);
            }
        } else {
            let n = v.strip_prefix("[int]").unwrap_or(v).trim();
            if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) {
                out.insert(key.to_string(), n.to_string());
            }
        }
    }
    out
}

/// A one-line `Key = @('a', 'b')` list of a PowerShell data file.
pub fn psd1_list(text: &str, key: &str) -> Option<Vec<String>> {
    text.lines().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let inner = v.trim().strip_prefix("@(")?.strip_suffix(')')?;
        Some(
            inner
                .split(',')
                .filter_map(|s| s.trim().strip_prefix('\'')?.strip_suffix('\'').map(str::to_string))
                .collect(),
        )
    })
}

impl WinConfig {
    /// The config, or None before the configure phase has run.
    pub fn load_optional() -> Result<Option<WinConfig>> {
        let p = config_path();
        if !p.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let m = psd1_scalars(&text);
        let get = |k: &str| m.get(k).cloned().unwrap_or_default();
        Ok(Some(WinConfig {
            dist_root: get("DistRoot"),
            llama_cpp_dir: get("LlamaCppDir"),
            openssl_dir: get("OpenSSLDir"),
            hip_path: get("HipPath"),
            vs_dev_shell: get("VsDevShell"),
            wix_exe: get("WixExe"),
            gpu_targets: get("GpuTargets"),
            build_type: get("BuildType"),
            c_compiler: get("CCompiler"),
            cxx_compiler: get("CxxCompiler"),
            linker: get("Linker"),
            build_jobs: get("BuildJobs").parse().unwrap_or(0),
        }))
    }

    pub fn load() -> Result<WinConfig> {
        Self::load_optional()?.context("build\\windows\\config-build.psd1 not found. Run cargo run -p xtask -- configure first.")
    }

    pub fn save(&self) -> Result<()> {
        let text = format!(
            r#"@{{
    # Generated by the configure phase (cargo run -p xtask -- configure); re-run it to refresh.

    # Paths
    # DistRoot: where the GPU SDK dists live, <DistRoot>\<Name>\<Version>\windows.
    # HipPath is the newest ROCm dist under it at configure time; the build
    # resolves the CUDA Toolkit and the Vulkan SDK there itself. OpenSSL is a
    # system library, outside the layout.
    # Change it with cargo run -p xtask -- configure --dist-root <path>, not by hand.
    DistRoot     = {dist_root}
    LlamaCppDir  = {llama}
    OpenSSLDir   = {openssl}
    HipPath      = {hip}
    VsDevShell   = {vs}
    # WixExe: the WiX Toolset the package phase builds the MSI with (version pinned
    # in installer\wix-toolset.psd1); empty when not installed yet.
    WixExe       = {wix}

    # Build settings
    # GpuTargets: DERIVED from the installed ROCm dist by the configure phase,
    # by the same rule as on Linux: the archs the dist ships BLAS kernels for
    # (.kpack\blas_lib_gfx*.kpack; legacy fallback: TensileLibrary_lazy_gfx*.dat),
    # minus the data-center gfx9xx and gfx125x, plus the Ryzen iGPU archs even
    # when kernels are absent (the driver HIP runtime fails module load on
    # EVERY visible device when one visible device has no code object).
    # Re-run cargo run -p xtask -- configure after a ROCm dist bump to refresh. Hand-edit
    # only to prune (~56 MiB of ggml-hip.dll per arch), and never prune the
    # iGPU archs (gfx1035/1036/1103/1152/1153).
    GpuTargets  = {targets}
    BuildType   = {build_type}
    CCompiler   = {cc}
    CxxCompiler = {cxx}
    # Linker: the lld-link.exe next to CCompiler; empty lets CMake pick (link.exe).
    Linker      = {linker}
    # Parallel build jobs: 3/4 of logical cores, never all of them, so at
    # least one stays free for interactive use during a long CUDA/HIP
    # compile (0 = use all cores).
    BuildJobs   = [int]{jobs}
}}
"#,
            dist_root = psq(&self.dist_root),
            llama = psq(&self.llama_cpp_dir),
            openssl = psq(&self.openssl_dir),
            hip = psq(&self.hip_path),
            vs = psq(&self.vs_dev_shell),
            wix = psq(&self.wix_exe),
            targets = psq(&self.gpu_targets),
            build_type = psq(&self.build_type),
            cc = psq(&self.c_compiler),
            cxx = psq(&self.cxx_compiler),
            linker = psq(&self.linker),
            jobs = self.build_jobs,
        );
        let p = config_path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, text).with_context(|| format!("writing {}", p.display()))
    }
}

/// The per-process ROCm environment of a build, what common.ps1 sets for
/// tools\pcie-bench\build.ps1: HIP_PATH, the dist's bin and lib\llvm\bin on
/// PATH, and the compile-time HIP vars. TheRock keeps the device bitcode
/// under lib\llvm\amdgcn\bitcode, not the <rocm>\amdgcn\bitcode layout clang
/// derives from --rocm-path, so without HIP_DEVICE_LIB_PATH the device
/// compile dies with "cannot find ROCm device library".
///
/// PROCESS SCOPE ONLY, and never anything else: the Adrenalin driver's own
/// HIP runtime reads LLVM_PATH at run time, and a machine-wide one pointing at
/// TheRock's LLVM makes every llama-server die at model load (0xC0000005).
pub fn hip_env(cfg: &WinConfig) {
    let hip = Path::new(&cfg.hip_path);
    if cfg.hip_path.is_empty() || !hip.is_dir() {
        return;
    }
    std::env::set_var("HIP_PATH", hip);
    util::path_prepend(&hip.join("bin"), true);
    let llvm_bin = hip.join("lib").join("llvm").join("bin");
    if llvm_bin.is_dir() {
        util::path_prepend(&llvm_bin, true);
    }
    let bitcode = hip.join("lib").join("llvm").join("amdgcn").join("bitcode");
    if bitcode.is_dir() {
        std::env::set_var("HIP_DEVICE_LIB_PATH", bitcode);
        std::env::set_var("HIP_PLATFORM", "amd");
        std::env::set_var("LLVM_PATH", hip.join("lib").join("llvm"));
    }
}

/// Visual Studio's developer environment (INCLUDE, LIB, the Windows SDK, the
/// MSVC tools on PATH), from the installation the config names. The config
/// records Launch-VsDevShell.ps1, which only works inside a PowerShell
/// session; its batch sibling VsDevCmd.bat in the same directory sets the same
/// environment, so it is run with the same -arch=amd64 (host arch left at its
/// default, as before) and its result is copied into this process.
pub fn vs_env(cfg: &WinConfig) -> Result<()> {
    if cfg.vs_dev_shell.is_empty() {
        bail!("VsDevShell not configured. Install Visual Studio with the C++ workload and re-run cargo run -p xtask -- configure.");
    }
    let shell = Path::new(&cfg.vs_dev_shell);
    if !shell.is_file() {
        bail!("VsDevShell not found at '{}'. Re-run cargo run -p xtask -- configure to fix the path.", shell.display());
    }
    activate_vs(&shell.with_file_name("VsDevCmd.bat"))
}

/// Runs VsDevCmd.bat and takes over the environment it leaves.
pub fn activate_vs(bat: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;

    ui::dim("Activating the VS developer environment...");
    // vswhere.exe lives in the VS Installer dir, which is not on PATH by
    // default; VsDevCmd calls it by bare name and prints "'vswhere.exe' is
    // not recognized" without it.
    let pf86 = std::env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let installer = pf86.join("Microsoft Visual Studio").join("Installer");
    if installer.join("vswhere.exe").is_file() {
        util::path_prepend(&installer, true);
    }
    // `set` goes to a file of its own, in UTF-16 (cmd /u), so neither the
    // console code page nor VsDevCmd's own output can garble a value.
    let dump = std::env::temp_dir().join(format!("llama-cpp-framework-vsenv-{}.txt", std::process::id()));
    let status = std::process::Command::new("cmd.exe")
        .raw_arg(format!(
            "/d /u /s /c \"\"{}\" -arch=amd64 -no_logo && set > \"{}\"\"",
            bat.display(),
            dump.display()
        ))
        .stdin(std::process::Stdio::null())
        .status()
        .with_context(|| format!("cannot run {}", bat.display()))?;
    let bytes = std::fs::read(&dump);
    let _ = std::fs::remove_file(&dump);
    if !status.success() {
        bail!("{} failed ({status})", bat.display());
    }
    let bytes = bytes.with_context(|| format!("{} left no environment", bat.display()))?;
    let units: Vec<u16> = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    let text = String::from_utf16_lossy(&units);
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}');
        if let Some((k, v)) = line.split_once('=') {
            // `=C:=C:\...`-style entries are cmd's per-drive cwd, not variables.
            if !k.is_empty() {
                std::env::set_var(k, v);
            }
        }
    }
    Ok(())
}

/// installer\wix-toolset.psd1: the WiX version the .wxs is written against,
/// its extensions and the EULA id.
pub struct WixPin {
    pub version: String,
    pub extensions: Vec<String>,
    pub eula_id: String,
}

pub fn wix_pin() -> Result<WixPin> {
    let path = util::repo_root().join("installer").join("wix-toolset.psd1");
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let m = psd1_scalars(&text);
    let field = |k: &str| m.get(k).cloned().with_context(|| format!("{}: {k} is missing", path.display()));
    Ok(WixPin {
        version: field("Version")?,
        eula_id: field("EulaId")?,
        extensions: psd1_list(&text, "Extensions").with_context(|| format!("{}: Extensions is missing", path.display()))?,
    })
}

/// `wix --version` prints e.g. 7.0.1+b3f3403: the part before '+' is the
/// version the extensions must match.
pub fn wix_version(exe: &Path) -> Option<String> {
    let out = util::output(util::cmd(exe).arg("--version"))?;
    Some(out.split('+').next().unwrap_or("").trim().to_string()).filter(|v| !v.is_empty())
}

/// The dotnet global tool on PATH, else in its default directory (a console
/// opened before the .NET SDK install has neither on PATH).
pub fn find_wix() -> Option<PathBuf> {
    util::which("wix").or_else(|| {
        let p = util::home().join(".dotnet").join("tools").join("wix.exe");
        p.is_file().then_some(p)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_psd1() {
        let text = "@{\n    # c = 'x'\n    DistRoot     = 'E:\\'\n    Name = 'it''s'\n    BuildJobs   = [int]18\n    Extensions = @('WixToolset.UI.wixext', 'WixToolset.Util.wixext')\n}\n";
        let m = psd1_scalars(text);
        assert_eq!(m["DistRoot"], "E:\\");
        assert_eq!(m["Name"], "it's");
        assert_eq!(m["BuildJobs"], "18");
        assert!(!m.contains_key("c"));
        assert_eq!(psd1_list(text, "Extensions").unwrap(), ["WixToolset.UI.wixext", "WixToolset.Util.wixext"]);
        assert_eq!(psq("it's"), "'it''s'");
    }
}
