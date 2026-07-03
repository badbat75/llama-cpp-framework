// Headless CLI dispatcher for llama-cpp-config.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::{paths, presets, server_cfg};

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
    Gui,
    /// Server-wide settings (server.ini).
    #[command(subcommand)]
    Server(ServerCmd),
    /// Per-model presets (presets.ini).
    #[command(subcommand)]
    Preset(PresetCmd),
}

#[derive(Subcommand, Debug)]
pub enum ServerCmd {
    /// Print the current server.ini values.
    Show,
    /// Update one or more server.ini fields.
    Set(ServerSet),
}

#[derive(Args, Debug)]
pub struct ServerSet {
    #[arg(long)]
    pub port: Option<i32>,
    #[arg(long)]
    pub hostname: Option<String>,
    #[arg(long)]
    pub mlock: Option<bool>,
    #[arg(long)]
    pub threads: Option<i32>,
    #[arg(long)]
    pub cache_reuse: Option<i32>,
    #[arg(long)]
    pub threads_batch: Option<i32>,
    #[arg(long)]
    pub models_max: Option<i32>,
    #[arg(long)]
    pub models_dir: Option<String>,
    /// GPU device for the main model, e.g. "CUDA0" (empty string = all devices).
    #[arg(long)]
    pub device: Option<String>,
    /// Multi-GPU split mode (--split-mode): none|layer|row (empty = default/layer).
    #[arg(long)]
    pub split_mode: Option<String>,
    /// Per-GPU weight proportions (--tensor-split), e.g. "3,1" (empty = even).
    #[arg(long)]
    pub tensor_split: Option<String>,
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

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Gui => crate::gui::run(),
        Command::Server(c) => run_server(c),
        Command::Preset(c) => run_preset(c),
    }
}

fn run_server(c: ServerCmd) -> Result<()> {
    match c {
        ServerCmd::Show => {
            let cfg = server_cfg::load();
            println!("server.ini: {}", paths::server_ini().display());
            println!(
                "  Port:         {}",
                cfg.port.map_or("-".into(), |v| v.to_string())
            );
            println!(
                "  Hostname:     {}",
                cfg.hostname.unwrap_or_else(|| "-".into())
            );
            println!(
                "  Mlock:        {}",
                cfg.mlock.map_or("-".into(), |v| v.to_string())
            );
            println!(
                "  Threads:      {}",
                cfg.threads.map_or_else(|| "auto".into(), |v| v.to_string()),
            );
            println!(
                "  CacheReuse:   {}",
                cfg.cache_reuse.map_or("-".into(), |v| v.to_string())
            );
            println!(
                "  ThreadsBatch: {}",
                cfg.threads_batch
                    .map_or_else(|| "auto".into(), |v| v.to_string()),
            );
            println!(
                "  ModelsMax:    {}",
                cfg.models_max
                    .map_or_else(|| "auto (default: 1)".into(), |v| v.to_string()),
            );
            println!(
                "  ModelsDir:    {}",
                cfg.models_dir.unwrap_or_else(|| "-".into())
            );
            println!(
                "  Device:       {}",
                cfg.device.unwrap_or_else(|| "auto (all)".into())
            );
            println!(
                "  SplitMode:    {}",
                cfg.split_mode.unwrap_or_else(|| "layer (default)".into())
            );
            println!(
                "  TensorSplit:  {}",
                cfg.tensor_split.unwrap_or_else(|| "even".into())
            );
            Ok(())
        }
        ServerCmd::Set(s) => {
            let mut cfg = server_cfg::load();
            if let Some(p) = s.port {
                cfg.port = Some(p);
            }
            if let Some(h) = s.hostname {
                cfg.hostname = Some(h);
            }
            if let Some(m) = s.mlock {
                cfg.mlock = Some(m);
            }
            if let Some(t) = s.threads {
                cfg.threads = if t > 0 { Some(t) } else { None };
            }
            if let Some(cr) = s.cache_reuse {
                cfg.cache_reuse = if cr > 0 { Some(cr) } else { None };
            }
            if let Some(tb) = s.threads_batch {
                cfg.threads_batch = if tb > 0 { Some(tb) } else { None };
            }
            if let Some(m) = s.models_max {
                cfg.models_max = Some(m);
            }
            if let Some(d) = s.models_dir {
                cfg.models_dir = Some(d);
            }
            // Passing an empty string clears the field (blank = unset).
            if let Some(dev) = s.device {
                cfg.device = server_cfg::opt_nonblank(Some(dev));
            }
            if let Some(sm) = s.split_mode {
                cfg.split_mode = server_cfg::opt_nonblank(Some(sm));
            }
            if let Some(ts) = s.tensor_split {
                cfg.tensor_split = server_cfg::opt_nonblank(Some(ts));
            }
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
            let Some(p) = presets.iter().find(|p| p.id == id) else {
                anyhow::bail!("No preset named `{id}`. Run `llama-cpp-config preset list`.");
            };
            println!("{}", presets::render_section(p));
            Ok(())
        }
        PresetCmd::Delete { id } => {
            presets::delete(&id).context("delete preset")?;
            println!("Removed [{id}] from {}", paths::presets_ini().display());
            Ok(())
        }
    }
}
