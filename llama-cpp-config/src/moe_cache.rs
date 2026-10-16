//! The `--moe-cache-mib` floor: the smallest budget a preset can load with, and
//! the larger one at which every group of layers actually caches. Pure logic
//! (no IO, no Slint state); the Models tab quotes it in the MoE cache hint and
//! refuses a Save below the first number.
//!
//! ## What llama.cpp does with the budget (`src/llama-moe-cache.cpp`, v0.7.0)
//! Only a layer whose expert tensors (`ffn_{gate_up,gate,up,down}_exps`) ALL sit
//! in host memory, on a layer assigned to a GPU, is cached. Such layers form
//! GROUPS: same device, same expert layout (types, shapes, strides). The budget
//! is split across the devices holding one, by `tensor-split` (or by FREE VRAM
//! when that is blank or all zeros), then within a device by each group's share
//! of its host bytes. A group buys slots at `Σ_t nb[2]·(n_slots + 1)` over its
//! tensors (one spare slot: CUDA MMQ can read past the last expert, padded to
//! the buffer alignment), and one that cannot afford `n_expert_used` slots is
//! skipped with a warning. When EVERY group is skipped the context throws "MoE
//! cache is too small to hold the experts of one token" and the model fails to
//! load.
//!
//! ## Two numbers, because the budget is shared
//! A group's need, divided by the fraction of the budget it receives, is the
//! size at which THAT group caches. The smallest of those over the groups is the
//! LOAD floor (below it every group is skipped), the largest is the size at
//! which every group caches; in between, the groups with the smaller share log
//! "budget is too small" and run uncached. On one GPU with a uniform layout the
//! two coincide. Both ignore the alignment padding (a few hundred bytes per
//! tensor), which the round-up to whole MiB absorbs except at the boundary.
//!
//! ## What this cannot see
//! With 2+ GPUs and no explicit `tensor-split`, llama.cpp weighs the devices by
//! the VRAM free at load and places the layers the same way, so neither the
//! groups nor their shares are knowable here. What is left is a floor no split
//! gets under (a group's share never exceeds the whole budget, and its need is
//! at least that of its cheapest layer): [`Floor::Load`] carries that one alone.
//!
//! ## CPU repack takes no layer out
//! The cache only counts a weight in a HOST buffer
//! (`llama_moe_cache_is_host_weight`), and a repacked expert (`CPU_REPACK`,
//! whose `is_host` is null) is not one. But a CPU override (`n-cpu-moe`, an
//! `exps=CPU` rule) takes the FIRST type of the CPU list that fits, and
//! `make_cpu_buft_list` (`src/llama-model.cpp`) puts the first GPU's pinned
//! host type (`ROCm_Host`, `CUDA_Host`) ahead of the repack types, and that
//! pinned type is host memory to the cache (its `is_host` is the CPU type's,
//! `ggml-cuda.cu`). Only `--no-host` (the preset's `no-host`) drops it. Without
//! it every host layer counts, whatever `repack` says (the cache needs a GPU
//! anyway). With it, which layers repack depends on the HOST CPU's kernels and
//! each tensor's type, which a header read here cannot settle, so the floor
//! falls back to the cheapest layer's need ([`Floor::Repack`]).
//! The host type is there only when the backend returns one
//! (`ggml_backend_dev_host_buffer_type`): all three the framework builds do
//! (CUDA, HIP, and Vulkan's `ggml_backend_vk_host_buffer_type`, whose
//! `is_host` is the CPU type's too).
//! Measured on Qwen3.8-Flash-Next IQ3_S (IQ4_NL `ffn_down_exps` in 39 of 48
//! layers, an AVX-512 host, v0.7.0): on ROCm the default and `--no-repack`
//! print the same 48-layer cache, and `--no-host` repacks 17550 MiB and
//! leaves 9 layers; on CUDA and on Vulkan (`Vulkan_Host`, 48284 MiB) the
//! default caches all 48 as well. Those logs are the first place a change of
//! order shows: re-check `make_cpu_buft_list` and each backend's host type on
//! a llama.cpp bump.

use std::collections::BTreeMap;

use regex::Regex;

use crate::gguf::TensorInfo;
use crate::gpu_split::{self, Layout};
use crate::presets::Preset;
use crate::tensor_override;

/// A group's key: its device index and its expert layout (each tensor's kind
/// and bytes), llama.cpp's `same_layout` test as far as a header can tell.
type GroupKey = (usize, Vec<(String, u64)>);

/// Block index → index into the preset's device list.
type DeviceOf = Box<dyn Fn(u32) -> usize>;

/// The expert weights the cache copies, as llama.cpp lists them
/// (`llama_moe_cache_layer_experts`).
const EXPERT_KINDS: [&str; 4] = [
    "ffn_gate_up_exps.weight",
    "ffn_gate_exps.weight",
    "ffn_up_exps.weight",
    "ffn_down_exps.weight",
];

/// What the model's header says about its experts: the tensors (every shard's,
/// `gguf::read_tensors`), `<arch>.expert_count`, `<arch>.expert_used_count` and
/// `<arch>.block_count`.
pub struct Model<'a> {
    pub tensors: &'a [TensorInfo],
    pub expert_count: u32,
    pub expert_used: u32,
    pub n_layer: u32,
}

/// The verdict for one preset on one model, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Floor {
    /// No GPU-assigned layer keeps all of its experts in host memory, so
    /// llama.cpp warns and runs without a cache: any size loads, none helps.
    NoHostExperts,
    /// The devices are weighed at load (free VRAM): only the floor no split
    /// gets under is known.
    Load(u64),
    /// Placement fully known: the load floor and the size at which every group
    /// caches.
    Split { load: u64, every: u64 },
    /// `no-host` with repacking on: the layers the CPU repacks leave the cache,
    /// and which they are depends on the host CPU, so only the floor no subset
    /// gets under is known (the cheapest host layer's need).
    Repack(u64),
}

impl Floor {
    /// The size below which the model does not load; 0 when any size does.
    pub fn load_bytes(self) -> u64 {
        match self {
            Floor::NoHostExperts => 0,
            Floor::Load(b) | Floor::Split { load: b, .. } | Floor::Repack(b) => b,
        }
    }

    /// The tail of the MoE cache hint's "below the experts of one token"
    /// sentence. Facts only; the hint around it says what the cache does.
    pub fn hint(self) -> String {
        match self {
            Floor::NoHostExperts => ": with this preset no GPU layer keeps all of its experts in RAM, so llama.cpp would run without a cache.".into(),
            Floor::Load(b) => format!(
                ": for this model at least {} MiB (Save refuses less). With the GPUs weighed by free VRAM, which of them end up caching is decided at load.",
                mib(b)
            ),
            Floor::Repack(b) => format!(
                ": for this model at least {} MiB (Save refuses less). With no-host the CPU repacks the experts it has kernels for, and those layers leave the cache.",
                mib(b)
            ),
            Floor::Split { load, every } if mib(load) == mib(every) => {
                format!(": for this preset at least {} MiB (Save refuses less).", mib(load))
            }
            Floor::Split { load, every } => format!(
                ": for this preset at least {} MiB (Save refuses less), and {} MiB before every group of layers caches; in between, the ones with the smaller share of the budget run uncached.",
                mib(load),
                mib(every)
            ),
        }
    }
}

/// Bytes → whole MiB, rounded UP: a floor rounded down would let through a
/// value that fails.
pub fn mib(bytes: u64) -> u64 {
    bytes.div_ceil(1 << 20)
}

/// The floor of `p` on `m`; `None` when the model has no experts to cache
/// (not MoE, or no `_exps` tensor in the list, e.g. an unread GGUF).
pub fn floor(m: &Model, p: &Preset) -> Option<Floor> {
    if m.expert_count == 0 || m.expert_used == 0 {
        return None;
    }
    let layers = expert_layers(m.tensors);
    if layers.is_empty() {
        return None;
    }
    let slots = u64::from(m.expert_used) + 1;
    let n_expert = u64::from(m.expert_count);

    let Some(layout) = Layout::new(m.n_layer as i32, p.n_gpu_layers.unwrap_or(-1)) else {
        return Some(Floor::NoHostExperts);
    };
    let rules = host_rules(&p.override_tensor);
    let host = |il: u32, names: &[&TensorInfo]| {
        p.n_cpu_moe.is_some_and(|n| (il as i32) < n)
            || names.iter().all(|t| first_rule_is_cpu(&rules, &t.name))
    };
    // Layers the cache takes: on a GPU position, every expert in RAM.
    let cached: Vec<(u32, &Vec<&TensorInfo>)> = layers
        .iter()
        .filter(|(il, ts)| (**il as i32) >= layout.start && host(**il, ts))
        .map(|(il, ts)| (*il, ts))
        .collect();
    if cached.is_empty() {
        return Some(Floor::NoHostExperts);
    }
    // One expert's bytes per layer: nb[2] summed over its tensors.
    let one_expert = |ts: &[&TensorInfo]| ts.iter().map(|t| t.bytes / n_expert).sum::<u64>();
    let cheapest = || {
        cached
            .iter()
            .map(|(_, ts)| one_expert(ts))
            .min()
            .unwrap_or(0)
    };
    if p.no_host && p.repack != Some(false) {
        return Some(Floor::Repack(cheapest() * slots));
    }

    let Some((weights, device_of)) = device_map(p, layout) else {
        return Some(Floor::Load(cheapest() * slots));
    };

    // Groups: (device, layout signature) → (one expert's bytes, host bytes).
    let mut groups: BTreeMap<GroupKey, (u64, u64)> = BTreeMap::new();
    let mut device_host: BTreeMap<usize, u64> = BTreeMap::new();
    for (il, ts) in &cached {
        let dev = device_of(*il);
        let sig = ts
            .iter()
            .map(|t| (kind(&t.name).to_string(), t.bytes))
            .collect();
        let host_bytes = ts.iter().map(|t| t.bytes).sum::<u64>();
        let g = groups.entry((dev, sig)).or_insert((one_expert(ts), 0));
        g.1 += host_bytes;
        *device_host.entry(dev).or_default() += host_bytes;
    }
    let split_sum: f64 = device_host
        .keys()
        .map(|&d| f64::from(weights.get(d).copied().unwrap_or(0)))
        .sum();
    let needs: Vec<u64> = groups
        .iter()
        .map(|((dev, _), (expert, host_bytes))| {
            let share = if split_sum > 0.0 {
                f64::from(weights.get(*dev).copied().unwrap_or(0)) / split_sum
            } else {
                1.0 / device_host.len() as f64
            };
            let fraction = share * (*host_bytes as f64) / (device_host[dev] as f64);
            let cost = expert * slots;
            if fraction > 0.0 {
                (cost as f64 / fraction).ceil() as u64
            } else {
                u64::MAX
            }
        })
        .collect();
    let load = needs.iter().copied().min().unwrap_or(0);
    let every = needs.iter().copied().max().unwrap_or(0);
    Some(Floor::Split { load, every })
}

/// Every block holding expert tensors, with them, in layer order.
fn expert_layers(tensors: &[TensorInfo]) -> BTreeMap<u32, Vec<&TensorInfo>> {
    let mut out: BTreeMap<u32, Vec<&TensorInfo>> = BTreeMap::new();
    for t in tensors {
        let Some((layer, k)) = t.name.strip_prefix("blk.").and_then(|r| r.split_once('.')) else {
            continue;
        };
        if let (true, Ok(il)) = (EXPERT_KINDS.contains(&k), layer.parse::<u32>()) {
            out.entry(il).or_default().push(t);
        }
    }
    out
}

fn kind(name: &str) -> &str {
    name.split_once('.')
        .and_then(|(_, r)| r.split_once('.'))
        .map_or(name, |(_, k)| k)
}

/// The preset's `override-tensor` rules, compiled in order, each with whether
/// it sends to the CPU. A rule the `regex` crate cannot read is kept as one
/// that matches nothing: guessing would be worse than a missed host layer.
fn host_rules(override_tensor: &str) -> Vec<(Option<Regex>, bool)> {
    tensor_override::parse(override_tensor)
        .into_iter()
        .map(|r| {
            (
                Regex::new(&r.pattern).ok(),
                r.device.eq_ignore_ascii_case("CPU"),
            )
        })
        .collect()
}

/// llama.cpp takes the FIRST rule matching a tensor (searched anywhere in the
/// name), so a GPU rule ahead of a CPU one keeps the tensor on the card.
fn first_rule_is_cpu(rules: &[(Option<Regex>, bool)], name: &str) -> bool {
    rules
        .iter()
        .find(|(re, _)| re.as_ref().is_some_and(|re| re.is_match(name)))
        .is_some_and(|(_, cpu)| *cpu)
}

/// The per-device weights and the block → device index map, when the preset
/// pins them; `None` when llama.cpp weighs the devices by free VRAM at load.
fn device_map(p: &Preset, layout: Layout) -> Option<(Vec<i32>, DeviceOf)> {
    let devices = gpu_split::parse_device_list(&p.device);
    if p.split_mode.trim() == "none" || devices.len() == 1 {
        return Some((vec![1], Box::new(|_| 0)));
    }
    let weights = gpu_split::parse_weights(&p.tensor_split);
    let counts = gpu_split::block_counts(&weights, layout)?;
    let start = layout.start;
    Some((
        weights,
        Box::new(move |il| {
            let mut end = start;
            for (d, c) in counts.iter().enumerate() {
                end += c;
                if (il as i32) < end {
                    return d;
                }
            }
            counts.len() - 1
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1 << 20;

    fn t(name: String, bytes: u64) -> TensorInfo {
        TensorInfo { name, bytes }
    }

    /// `n` uniform MoE layers of 128 experts, 1 MiB per expert per tensor over
    /// three tensors (3 MiB per expert), behind one dense leading block.
    fn tensors(n: u32) -> Vec<TensorInfo> {
        let mut out = vec![t("blk.0.ffn_up.weight".into(), 512 * MIB)];
        for il in 1..=n {
            for k in ["ffn_gate_exps", "ffn_up_exps", "ffn_down_exps"] {
                out.push(t(format!("blk.{il}.{k}.weight"), 128 * MIB));
            }
            out.push(t(format!("blk.{il}.ffn_up_shexp.weight"), 64 * MIB));
        }
        out
    }

    fn model(ts: &[TensorInfo], n_layer: u32) -> Model<'_> {
        Model {
            tensors: ts,
            expert_count: 128,
            expert_used: 8,
            n_layer,
        }
    }

    // One GPU, every expert layer in RAM: 8 used + 1 spare = 9 slots of 3 MiB.
    #[test]
    fn one_gpu_floor_is_one_tokens_experts_plus_the_spare_slot() {
        let ts = tensors(8);
        let p = Preset {
            device: "ROCm0".into(),
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        let f = floor(&model(&ts, 9), &p).unwrap();
        assert_eq!(
            f,
            Floor::Split {
                load: 27 * MIB,
                every: 27 * MIB
            }
        );
        assert_eq!(mib(f.load_bytes()), 27);
    }

    // Two GPUs at 3:1, experts of every layer in RAM: GPU1's group gets a
    // quarter of the budget, so the load floor is GPU0's need over 3/4 and every
    // group caches only at GPU1's need over 1/4.
    #[test]
    fn a_pinned_split_gives_a_load_floor_and_an_every_group_floor() {
        let ts = tensors(8);
        let p = Preset {
            device: "ROCm0,ROCm1".into(),
            tensor_split: "3,1".into(),
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        let Some(Floor::Split { load, every }) = floor(&model(&ts, 9), &p) else {
            panic!("expected a split floor");
        };
        assert_eq!(load, (27.0 * MIB as f64 / 0.75).ceil() as u64);
        assert_eq!(every, 27 * MIB * 4);
    }

    // Blank tensor-split over 2+ GPUs: llama.cpp weighs by free VRAM at load,
    // so only the existence floor is known.
    #[test]
    fn a_free_vram_split_only_knows_the_load_floor() {
        let ts = tensors(8);
        let p = Preset {
            device: "ROCm0,ROCm1".into(),
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        assert_eq!(floor(&model(&ts, 9), &p), Some(Floor::Load(27 * MIB)));
    }

    // no-host lets the CPU repack host experts out of the cache, by kernels this
    // cannot see: only the cheapest layer's need is left, and `repack = false`
    // restores the full answer.
    #[test]
    fn no_host_with_repack_falls_back_to_the_cheapest_layer() {
        let ts = tensors(8);
        let p = Preset {
            device: "ROCm0".into(),
            n_cpu_moe: Some(99),
            no_host: true,
            ..Default::default()
        };
        let full = floor(&model(&ts, 9), &Preset { no_host: false, ..p.clone() }).unwrap();
        let repack = floor(&model(&ts, 9), &p).unwrap();
        assert!(matches!(repack, Floor::Repack(_)), "{repack:?}");
        assert_eq!(repack.load_bytes(), full.load_bytes());
        let kept = Preset {
            repack: Some(false),
            ..p
        };
        assert_eq!(floor(&model(&ts, 9), &kept), Some(full));
    }

    // Experts all on the GPU (no n-cpu-moe, no CPU rule), or a GPU rule ahead of
    // the CPU one: nothing is cached and any size loads.
    #[test]
    fn no_host_experts_means_no_floor() {
        let ts = tensors(8);
        let gpu = Preset {
            device: "ROCm0".into(),
            ..Default::default()
        };
        assert_eq!(floor(&model(&ts, 9), &gpu), Some(Floor::NoHostExperts));
        let shadowed = Preset {
            override_tensor: r"exps=ROCm0,\.ffn_(up|down|gate)_exps=CPU".into(),
            ..gpu.clone()
        };
        assert_eq!(floor(&model(&ts, 9), &shadowed), Some(Floor::NoHostExperts));
        let cpu_rule = Preset {
            override_tensor: r"\.ffn_(up|down|gate)_exps=CPU".into(),
            ..gpu
        };
        assert_eq!(
            floor(&model(&ts, 9), &cpu_rule).map(Floor::load_bytes),
            Some(27 * MIB)
        );
    }

    // Only the four `_exps` weights count (not the shared experts, the router or
    // a dense block), and without a known split the floor is the CHEAPEST layer:
    // a fused gate_up layer at 4 MiB per expert loses to a 3 MiB one.
    #[test]
    fn the_unknown_split_floor_is_the_cheapest_layer() {
        let mut ts = tensors(1);
        ts.push(t("blk.2.ffn_gate_up_exps.weight".into(), 256 * MIB));
        ts.push(t("blk.2.ffn_down_exps.weight".into(), 256 * MIB));
        ts.push(t("blk.2.ffn_gate_inp.weight".into(), MIB));
        let p = Preset {
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        assert_eq!(floor(&model(&ts, 3), &p), Some(Floor::Load(27 * MIB)));
    }

    // Two expert layouts on one GPU (an imatrix mix keeping some layers'
    // `ffn_down_exps` at a wider type) share the budget by host bytes. `repack`
    // changes nothing here: with a GPU, host experts never repack.
    #[test]
    fn mixed_layouts_share_the_budget_and_repack_is_inert() {
        let mut ts = tensors(8);
        // Layers 1-4 cost 3.5 MiB per expert: 1792 and 1536 of 3328 MiB of
        // host bytes, 31.5 MiB over 0.538 and 27 over 0.462, both 58.5 MiB,
        // since with four layers each a group's need and share scale alike.
        for il in 1..=4 {
            let name = format!("blk.{il}.ffn_down_exps.weight");
            ts.iter_mut().find(|t| t.name == name).unwrap().bytes = 192 * MIB;
        }
        let p = Preset {
            device: "ROCm0".into(),
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        let f = floor(&model(&ts, 9), &p);
        let Some(Floor::Split { load, every }) = f else {
            panic!("expected a split floor");
        };
        assert_eq!((mib(load), mib(every)), (59, 59));
        for repack in [Some(true), Some(false)] {
            let r = Preset {
                repack,
                ..p.clone()
            };
            assert_eq!(floor(&model(&ts, 9), &r), f);
        }
    }

    // A dense model, or a list without expert tensors, has no floor at all.
    #[test]
    fn a_dense_model_has_no_floor() {
        let ts = tensors(8);
        let m = Model {
            expert_count: 0,
            ..model(&ts, 9)
        };
        assert_eq!(floor(&m, &Preset::default()), None);
        assert_eq!(floor(&model(&ts[..1], 9), &Preset::default()), None);
    }
}
