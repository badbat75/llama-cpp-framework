//! Headless CLI dispatcher for llama-cpp-config.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::{paths, presets, server_cfg};

// ── Command definitions (clap) ───────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "llama-cpp-config",
    version,
    about = "Configure llama.cpp-framework: llama-server and model presets. Run with no args for the GUI."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Force-launch the GUI (default when no subcommand is given).
    Gui {
        /// Start hidden in the system tray instead of opening the window.
        /// This is what the "Start with Windows" logon entry launches
        /// (see src/startup.rs).
        #[arg(long)]
        minimized: bool,
    },
    /// Server-wide settings (server.ini).
    #[command(subcommand)]
    Server(ServerCmd),
    /// Per-model presets (presets.ini).
    #[command(subcommand)]
    Preset(PresetCmd),
    /// Control the llama-server process.
    #[command(subcommand)]
    Control(ControlCmd),
}

#[derive(Subcommand, Debug)]
pub enum ControlCmd {
    /// Start llama-server.
    Start,
    /// Stop llama-server.
    Stop,
    /// Restart llama-server.
    Restart,
    /// Stop llama-server and close the config GUI if running.
    StopAndClose,
}

#[derive(Subcommand, Debug)]
pub enum ServerCmd {
    /// Print the current server.ini values.
    Show,
    /// Update one or more server.ini fields.
    // Boxed: `ServerSet` carries one Option per server.ini field, so it grows
    // with the schema and has outgrown clippy's enum-variant size threshold
    // against the payload-less `Show`. clap derives `Args` for the inner struct
    // and unwraps the Box itself, so the CLI surface is unchanged.
    Set(Box<ServerSet>),
}

#[derive(Args, Debug, Default)]
pub struct ServerSet {
    #[arg(long)]
    pub port: Option<i32>,
    #[arg(long)]
    pub hostname: Option<String>,
    #[arg(long)]
    pub mlock: Option<bool>,
    /// Read the weights into RAM instead of mmapping the GGUF (--no-mmap). true = on.
    #[arg(long)]
    pub no_mmap: Option<bool>,
    /// CPU threads for generation. 0 or negative clears the override (auto).
    #[arg(long)]
    pub threads: Option<i32>,
    /// Minimum prompt-cache reuse chunk. 0 or negative clears the override.
    #[arg(long)]
    pub cache_reuse: Option<i32>,
    /// CPU threads for prompt processing. 0 or negative clears the override (auto).
    #[arg(long)]
    pub threads_batch: Option<i32>,
    /// Models kept resident at once. Stored as-is (0 = unlimited); NOT cleared by 0.
    #[arg(long)]
    pub models_max: Option<i32>,
    #[arg(long)]
    pub models_dir: Option<String>,
    /// GPUs models run on, in split order, e.g. "ROCm1,CUDA0" (empty = all detected).
    #[arg(long)]
    pub device: Option<String>,
    /// Multi-GPU split mode (--split-mode): none|layer|row (empty = default/layer).
    #[arg(long)]
    pub split_mode: Option<String>,
    /// How much each --device holds (--tensor-split), e.g. "3,1" (empty = by free VRAM).
    #[arg(long)]
    pub tensor_split: Option<String>,
    /// Pin tensors to a device (--override-tensor), e.g. "token_embd\.weight=ROCm0"
    /// (empty = none). REPLACES every preset's own rules; an unknown device stops the server.
    #[arg(long)]
    pub override_tensor: Option<String>,
    /// GPU for the image encoder, e.g. "ROCm1" (empty = the first GPU llama.cpp finds).
    #[arg(long)]
    pub mmproj_device: Option<String>,
    /// Enable the web UI's MCP CORS proxy (--webui-mcp-proxy). true = on.
    #[arg(long)]
    pub webui_mcp_proxy: Option<bool>,
    /// Auto-fit unset args to device memory (-fit): true = on, false = off.
    #[arg(long)]
    pub fit: Option<bool>,
    /// Continue a trailing assistant message instead of answering it
    /// (--prefill-assistant / --no-prefill-assistant). true = llama.cpp's default.
    #[arg(long)]
    pub prefill_assistant: Option<bool>,
    /// llama-server log verbosity threshold (-lv / --log-verbosity): 0 output,
    /// 1 error, 2 warning, 3 info, 4 trace, 5 debug.
    #[arg(long)]
    pub log_verbosity: Option<i32>,
    /// Override the integration base URL (opencode.json + Claude Code). Empty = auto.
    #[arg(long)]
    pub opencode_base_url: Option<String>,
    /// API key for the integration provider. Empty = none.
    #[arg(long)]
    pub opencode_api_key: Option<String>,
}

impl ServerSet {
    /// Copy every provided flag into `cfg`, applying each field's clearing rule
    /// (see the per-field docs above): a `None` flag leaves the field untouched;
    /// non-positive thread/reuse values clear the override; a blank string
    /// unsets any optional string field (`opt_nonblank`, matching `load()`).
    /// The single, unit-tested home for `server set`'s field mapping — keep it
    /// in lockstep with the `ServerConfig` schema.
    fn apply(&self, cfg: &mut server_cfg::ServerConfig) {
        if let Some(p) = self.port {
            cfg.port = Some(p);
        }
        if let Some(h) = &self.hostname {
            cfg.hostname = server_cfg::opt_nonblank(Some(h.clone()));
        }
        if let Some(m) = self.mlock {
            cfg.mlock = Some(m);
        }
        if let Some(nm) = self.no_mmap {
            cfg.no_mmap = Some(nm);
        }
        if let Some(t) = self.threads {
            cfg.threads = if t > 0 { Some(t) } else { None };
        }
        if let Some(cr) = self.cache_reuse {
            cfg.cache_reuse = if cr > 0 { Some(cr) } else { None };
        }
        if let Some(tb) = self.threads_batch {
            cfg.threads_batch = if tb > 0 { Some(tb) } else { None };
        }
        if let Some(m) = self.models_max {
            cfg.models_max = Some(m);
        }
        if let Some(d) = &self.models_dir {
            cfg.models_dir = server_cfg::opt_nonblank(Some(d.clone()));
        }
        if let Some(dev) = &self.device {
            cfg.device = server_cfg::opt_nonblank(Some(dev.clone()));
        }
        if let Some(sm) = &self.split_mode {
            cfg.split_mode = server_cfg::opt_nonblank(Some(sm.clone()));
        }
        if let Some(ts) = &self.tensor_split {
            cfg.tensor_split = server_cfg::opt_nonblank(Some(ts.clone()));
        }
        if let Some(ot) = &self.override_tensor {
            cfg.override_tensor = server_cfg::opt_nonblank(Some(ot.clone()));
        }
        if let Some(md) = &self.mmproj_device {
            cfg.mmproj_device = server_cfg::opt_nonblank(Some(md.clone()));
        }
        if let Some(w) = self.webui_mcp_proxy {
            cfg.webui_mcp_proxy = Some(w);
        }
        if let Some(f) = self.fit {
            cfg.fit = Some(f);
        }
        if let Some(p) = self.prefill_assistant {
            cfg.prefill_assistant = Some(p);
        }
        if let Some(lv) = self.log_verbosity {
            cfg.log_verbosity = Some(lv);
        }
        if let Some(url) = &self.opencode_base_url {
            cfg.opencode_base_url = server_cfg::opt_nonblank(Some(url.clone()));
        }
        if let Some(key) = &self.opencode_api_key {
            cfg.opencode_api_key = server_cfg::opt_nonblank(Some(key.clone()));
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum PresetCmd {
    /// List preset ids and the resolved model path for each.
    List,
    /// Dump one preset as INI.
    Show { id: String },
    /// Delete a preset section.
    Delete { id: String },
}

// ── Dispatch & rendering ─────────────────────────────────────────────────

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Gui { minimized } => crate::gui::run(minimized),
        Command::Server(c) => run_server(c),
        Command::Preset(c) => run_preset(c),
        Command::Control(c) => run_control(c),
    }
}

/// The aligned body of `server show`, one `  Label        value` row per field
/// (the label column fits the longest key, "PrefillAssistant:"). Pure so the test
/// below can pin that every `ServerConfig` field is printed — a field added to
/// the schema but forgotten here would otherwise be a silent omission.
fn show_lines(cfg: &server_cfg::ServerConfig) -> String {
    let mut out = String::new();
    let mut row = |label: &str, value: String| {
        out.push_str(&format!("  {label:<18} {value}\n"));
    };
    row("Port:", cfg.port.map_or("-".into(), |v| v.to_string()));
    row(
        "Hostname:",
        cfg.hostname.clone().unwrap_or_else(|| "-".into()),
    );
    row("Mlock:", cfg.mlock.map_or("-".into(), |v| v.to_string()));
    row(
        "NoMmap:",
        cfg.no_mmap
            .map_or_else(|| "false (default)".into(), |v| v.to_string()),
    );
    row(
        "Threads:",
        cfg.threads.map_or_else(|| "auto".into(), |v| v.to_string()),
    );
    row(
        "CacheReuse:",
        cfg.cache_reuse.map_or("-".into(), |v| v.to_string()),
    );
    row(
        "ThreadsBatch:",
        cfg.threads_batch
            .map_or_else(|| "auto".into(), |v| v.to_string()),
    );
    row(
        "ModelsMax:",
        cfg.models_max
            .map_or_else(|| "auto (default: 4)".into(), |v| v.to_string()),
    );
    row(
        "ModelsDir:",
        cfg.models_dir.clone().unwrap_or_else(|| "-".into()),
    );
    row(
        "Device:",
        cfg.device.clone().unwrap_or_else(|| "auto (all)".into()),
    );
    row(
        "SplitMode:",
        cfg.split_mode
            .clone()
            .unwrap_or_else(|| "layer (default)".into()),
    );
    row(
        "TensorSplit:",
        cfg.tensor_split
            .clone()
            .unwrap_or_else(|| "auto (by free VRAM)".into()),
    );
    row(
        "OverrideTensor:",
        cfg.override_tensor.clone().unwrap_or_else(|| "none".into()),
    );
    row(
        "MmprojDevice:",
        cfg.mmproj_device
            .clone()
            .unwrap_or_else(|| "auto (first GPU)".into()),
    );
    row(
        "WebuiMcpProxy:",
        cfg.webui_mcp_proxy
            .map_or_else(|| "true (default)".into(), |v| v.to_string()),
    );
    row(
        "Fit:",
        cfg.fit
            .map_or_else(|| "false (default)".into(), |v| v.to_string()),
    );
    row(
        "PrefillAssistant:",
        cfg.prefill_assistant
            .map_or_else(|| "true (default)".into(), |v| v.to_string()),
    );
    row(
        "LogVerbosity:",
        cfg.log_verbosity
            .map_or_else(|| "4 (default)".into(), |v| v.to_string()),
    );
    row(
        "OpencodeBaseUrl:",
        cfg.opencode_base_url
            .clone()
            .unwrap_or_else(|| "auto (host:port, /v1 appended)".into()),
    );
    row(
        "OpencodeApiKey:",
        cfg.opencode_api_key
            .clone()
            .unwrap_or_else(|| "(none)".into()),
    );
    out
}

fn run_server(c: ServerCmd) -> Result<()> {
    match c {
        ServerCmd::Show => {
            let cfg = server_cfg::load();
            println!("server.ini: {}", paths::server_ini().display());
            print!("{}", show_lines(&cfg));
            Ok(())
        }
        ServerCmd::Set(s) => {
            let mut cfg = server_cfg::load();
            s.apply(&mut cfg);
            server_cfg::save(&cfg).context("save server.ini")?;
            println!("Wrote {}", paths::server_ini().display());
            Ok(())
        }
    }
}

fn run_preset(c: PresetCmd) -> Result<()> {
    match c {
        PresetCmd::List => {
            let presets = presets::load_all();
            println!("presets.ini: {}", paths::presets_ini().display());
            if presets.is_empty() {
                println!("  (no presets defined)");
            }
            for p in presets {
                println!("  [{}]  model={}", p.id, p.model);
            }
            Ok(())
        }
        PresetCmd::Show { id } => {
            let presets = presets::load_all();
            // Case-insensitive, like the whole INI section layer (read_section,
            // rename_section, delete_section all use eq_ignore_ascii_case).
            let Some(p) = presets.iter().find(|p| p.id.eq_ignore_ascii_case(&id)) else {
                anyhow::bail!("No preset named `{id}`. Run `llama-cpp-config preset list`.");
            };
            println!("{}", presets::render_section(p));
            Ok(())
        }
        PresetCmd::Delete { id } => {
            // ini::delete_section is a documented no-op for a missing section,
            // so look the id up first — mirroring Show — or a typo'd id gets a
            // "Removed" message for a preset that never existed. Match
            // case-insensitively (as the INI layer does) and delete by the
            // STORED id so the header is hit whatever case the user typed.
            let presets = presets::load_all();
            let Some(p) = presets.iter().find(|p| p.id.eq_ignore_ascii_case(&id)) else {
                anyhow::bail!("No preset named `{id}`. Run `llama-cpp-config preset list`.");
            };
            let real_id = p.id.clone();
            presets::delete(&real_id).context("delete preset")?;
            println!(
                "Removed [{real_id}] from {}",
                paths::presets_ini().display()
            );
            Ok(())
        }
    }
}

// ── Control commands ────────────────────────────────────────────────────

fn run_control(c: ControlCmd) -> Result<()> {
    use crate::runstate;
    match c {
        ControlCmd::Start => {
            match runstate::start() {
                Ok(Some(_)) => println!("llama-server started."),
                Ok(None) => println!("llama-server is already running."),
                Err(e) => anyhow::bail!("Failed to start llama-server: {}", e),
            }
            Ok(())
        }
        ControlCmd::Stop => {
            runstate::stop();
            if runstate::is_running() {
                anyhow::bail!("Failed to stop llama-server.");
            } else {
                println!("llama-server stopped.");
            }
            Ok(())
        }
        ControlCmd::Restart => {
            runstate::stop();
            std::thread::sleep(std::time::Duration::from_millis(1000));
            match runstate::start() {
                Ok(Some(_)) => println!("llama-server restarted."),
                Ok(None) => println!("llama-server is already running."),
                Err(e) => anyhow::bail!("Failed to restart llama-server: {}", e),
            }
            Ok(())
        }
        ControlCmd::StopAndClose => {
            runstate::stop();
            std::thread::sleep(std::time::Duration::from_millis(500));
            #[cfg(windows)]
            crate::single_instance::signal_close();
            println!("llama-server stopped and config GUI closed.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;

    // The only guard on `server set`'s schema mirror: every other server-field
    // spot (server_cfg load/save, the form conversions) has a round-trip test,
    // but `apply` is the CLI-only copy — an omitted field here is silent.

    #[test]
    fn server_set_apply_copies_every_field() {
        let set = ServerSet {
            port: Some(9000),
            hostname: Some("0.0.0.0".into()),
            mlock: Some(true),
            no_mmap: Some(true),
            threads: Some(8),
            cache_reuse: Some(256),
            threads_batch: Some(16),
            models_max: Some(3),
            models_dir: Some("D:\\models".into()),
            device: Some("ROCm1,CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            override_tensor: Some(r"token_embd\.weight=ROCm1".into()),
            mmproj_device: Some("ROCm1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            prefill_assistant: Some(false),
            log_verbosity: Some(2),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        let mut cfg = ServerConfig::default();
        set.apply(&mut cfg);
        // Whole-struct equality against a second exhaustive literal: the
        // compiler forces a value for a new field in BOTH literals, and the
        // equality fails until `apply` actually copies it — an initialized-
        // but-never-copied field can't slip through.
        let expected = ServerConfig {
            port: Some(9000),
            hostname: Some("0.0.0.0".into()),
            mlock: Some(true),
            no_mmap: Some(true),
            threads: Some(8),
            cache_reuse: Some(256),
            threads_batch: Some(16),
            models_max: Some(3),
            models_dir: Some("D:\\models".into()),
            device: Some("ROCm1,CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            override_tensor: Some(r"token_embd\.weight=ROCm1".into()),
            mmproj_device: Some("ROCm1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            prefill_assistant: Some(false),
            log_verbosity: Some(2),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        assert_eq!(cfg, expected);
    }

    // The Show leg of the 3-spot CLI fan-out: every field set in a rich config
    // must surface in `server show`'s output. Complements the `apply` test
    // above so neither CLI leg can silently drop a new server field.
    #[test]
    fn show_lines_prints_every_field() {
        let cfg = ServerConfig {
            port: Some(9000),
            hostname: Some("0.0.0.0".into()),
            mlock: Some(false),
            no_mmap: Some(true),
            threads: Some(8),
            cache_reuse: Some(256),
            threads_batch: Some(16),
            models_max: Some(3),
            models_dir: Some(r"D:\models".into()),
            device: Some("ROCm1,CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            override_tensor: Some(r"token_embd\.weight=ROCm1".into()),
            mmproj_device: Some("ROCm1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            prefill_assistant: Some(false),
            log_verbosity: Some(2),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        // The exhaustive destructure breaks compilation the moment a field is
        // added, until this test decides what to do with it — but the
        // assertions come from the hand-maintained `needles` array below:
        // bind the new field AND add its needle, or its Show row goes
        // unguarded (binding alone, or `field: _`, compiles fine).
        let ServerConfig {
            port,
            hostname,
            mlock,
            no_mmap,
            threads,
            cache_reuse,
            threads_batch,
            models_max,
            models_dir,
            device,
            split_mode,
            tensor_split,
            override_tensor,
            mmproj_device,
            webui_mcp_proxy,
            fit,
            prefill_assistant,
            log_verbosity,
            opencode_base_url,
            opencode_api_key,
        } = cfg.clone();
        let needles = [
            ("Port:", port.unwrap().to_string()),
            ("Hostname:", hostname.unwrap()),
            ("Mlock:", mlock.unwrap().to_string()),
            ("NoMmap:", no_mmap.unwrap().to_string()),
            ("Threads:", threads.unwrap().to_string()),
            ("CacheReuse:", cache_reuse.unwrap().to_string()),
            ("ThreadsBatch:", threads_batch.unwrap().to_string()),
            ("ModelsMax:", models_max.unwrap().to_string()),
            ("ModelsDir:", models_dir.unwrap()),
            ("Device:", device.unwrap()),
            ("SplitMode:", split_mode.unwrap()),
            ("TensorSplit:", tensor_split.unwrap()),
            ("OverrideTensor:", override_tensor.unwrap()),
            ("MmprojDevice:", mmproj_device.unwrap()),
            ("WebuiMcpProxy:", webui_mcp_proxy.unwrap().to_string()),
            ("Fit:", fit.unwrap().to_string()),
            ("PrefillAssistant:", prefill_assistant.unwrap().to_string()),
            ("LogVerbosity:", log_verbosity.unwrap().to_string()),
            ("OpencodeBaseUrl:", opencode_base_url.unwrap()),
            ("OpencodeApiKey:", opencode_api_key.unwrap()),
        ];
        let out = show_lines(&cfg);
        for (label, value) in needles {
            // Label and value must share a LINE: two separate contains() could
            // both pass off other fields (e.g. ModelsMax "3" matching inside
            // TensorSplit's "3,1") while this field's row prints a placeholder.
            assert!(
                out.lines().any(|l| l.contains(label) && l.contains(&value)),
                "no line pairs {label:?} with {value:?} in:\n{out}"
            );
        }
    }

    #[test]
    fn server_set_apply_leaves_unset_flags_untouched() {
        let before = ServerConfig {
            port: Some(1234),
            hostname: Some("localhost".into()),
            threads: Some(6),
            ..Default::default()
        };
        let mut cfg = before.clone();
        ServerSet::default().apply(&mut cfg); // all flags None
        assert_eq!(cfg, before);
    }

    #[test]
    fn server_set_apply_clears_overrides_on_nonpositive_and_blank() {
        let mut cfg = ServerConfig {
            hostname: Some("0.0.0.0".into()),
            threads: Some(8),
            cache_reuse: Some(64),
            threads_batch: Some(4),
            models_dir: Some(r"D:\models".into()),
            device: Some("CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            override_tensor: Some(r"token_embd\.weight=CUDA0".into()),
            ..Default::default()
        };
        let set = ServerSet {
            hostname: Some("  ".into()),
            threads: Some(0),
            cache_reuse: Some(-1),
            threads_batch: Some(0),
            models_dir: Some(String::new()),
            device: Some(String::new()),
            split_mode: Some("  ".into()),
            tensor_split: Some(String::new()),
            override_tensor: Some(String::new()),
            ..Default::default()
        };
        set.apply(&mut cfg);
        assert_eq!(cfg.hostname, None);
        assert_eq!(cfg.threads, None);
        assert_eq!(cfg.cache_reuse, None);
        assert_eq!(cfg.threads_batch, None);
        assert_eq!(cfg.models_dir, None);
        assert_eq!(cfg.device, None);
        assert_eq!(cfg.split_mode, None);
        assert_eq!(cfg.tensor_split, None);
        assert_eq!(cfg.override_tensor, None);
    }
}
