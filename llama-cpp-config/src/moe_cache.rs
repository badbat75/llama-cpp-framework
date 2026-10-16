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
//! ## CPU repack takes layers out
//! The cache only counts a weight in a HOST buffer
//! (`llama_moe_cache_is_host_weight`). An expert the CPU backend repacks lands
//! in `CPU_REPACK`, whose `is_host` is null (`ggml-cpu/repack.cpp`), so it is
//! not one, and ONE repacked tensor takes its whole layer out of every group.
//! That holds for experts sent to the CPU by `n-cpu-moe` and by an `exps=CPU`
//! rule alike: both are CPU overrides, and those consider the extra buffer types
//! (`select_weight_buft(..., buft_list_cpu)` in `llama-model-loader.cpp`).
//! Imatrix mixes trip it: Qwen3.8-Flash-Next at IQ3_S keeps `ffn_down_exps` at
//! IQ4_NL in 39 of its 48 layers, so on AVX2 only 9 could be cached. Hence
//! [`repacks`], a mirror of `ggml_repack_get_optimal_repack_type` for x86
//! (type x ISA x `ne[1] % 8`), applied unless the preset says `repack = false`.
//! Assumptions, to re-check on a llama.cpp bump like the other upstream
//! mirrors:
//! - the repack rule itself (`ggml_repack_get_optimal_repack_type`);
//! - the ISA is that of THIS machine: the configurator and the server run on
//!   the same one, and `ggml_cpu_has_avx2/avx512` are build flags of the CPU
//!   variant the server loaded, which `cpu-feats.cpp` picks as the best one
//!   this CPU passes, scored as in [`Isa::host`]. That holds only while the
//!   bundle ships EVERY variant (`GGML_BACKEND_DL` + `GGML_CPU_ALL_VARIANTS`,
//!   `cmake-options.psd1`) and nothing builds with `GGML_NATIVE=ON` or a
//!   subset of them: a missing AVX-512 variant would load the AVX2 one and
//!   leave Q2_K unrepacked, which this module would count as repacked;
//! - ARM's own list (NEON, SVE, i8mm: Q5_K, Q6_K, Q8_0 and more) is not
//!   modelled, since the framework builds for x86-64 only.

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
/// `<arch>.block_count`; plus the CPU the server will repack on.
pub struct Model<'a> {
    pub tensors: &'a [TensorInfo],
    pub expert_count: u32,
    pub expert_used: u32,
    pub n_layer: u32,
    pub isa: Isa,
}

/// The two x86 features the CPU repack's x86 branches test
/// (`ggml_cpu_has_avx2` / `ggml_cpu_has_avx512`). Both are COMPILE-time flags of
/// the CPU variant the server loaded, so they are true when that variant was
/// built with them; `Default` (all false) means no repack at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Isa {
    pub avx2: bool,
    pub avx512: bool,
}

impl Isa {
    /// This machine's, as `GGML_CPU_ALL_VARIANTS` would pick: a variant built
    /// with AVX2 (haswell and up) also needs FMA, F16C and BMI2, one with
    /// AVX-512 (skylakex and up) F, CD, VL, DQ and BW
    /// (`ggml-cpu/arch/x86/cpu-feats.cpp`), so the flags require the same.
    pub fn host() -> Isa {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::is_x86_feature_detected as has;
            let avx2 = has!("avx2") && has!("fma") && has!("f16c") && has!("bmi2");
            let avx512 = avx2
                && has!("avx512f")
                && has!("avx512cd")
                && has!("avx512vl")
                && has!("avx512dq")
                && has!("avx512bw");
            Isa { avx2, avx512 }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Isa::default()
        }
    }
}

/// Whether the CPU backend repacks `t` on `isa` (llama.cpp v0.7.0,
/// `ggml_repack_get_optimal_repack_type`, x86 branches only): Q4_0, Q4_K,
/// IQ4_NL and MXFP4 under AVX2, Q2_K under AVX-512, each when `ne[1] % 8 == 0`.
/// Q5_K, Q6_K, Q8_0 and Q1_0 repack on ARM only.
pub fn repacks(t: &TensorInfo, isa: Isa) -> bool {
    const Q4_0: u32 = 2;
    const Q2_K: u32 = 10;
    const Q4_K: u32 = 12;
    const IQ4_NL: u32 = 20;
    const MXFP4: u32 = 39;
    let by_isa = match t.ty {
        Q4_0 | Q4_K | IQ4_NL | MXFP4 => isa.avx2,
        Q2_K => isa.avx512,
        _ => false,
    };
    by_isa && t.ne1.is_multiple_of(8)
}

/// The verdict for one preset on one model: the floor, plus how many of the
/// layers the cache could take (on a GPU position, every expert in RAM) the CPU
/// repack takes out of it again. `floor` counts only the remaining ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub floor: Floor,
    pub repacked: u32,
    pub host_layers: u32,
}

impl Verdict {
    pub fn load_bytes(self) -> u64 {
        self.floor.load_bytes()
    }

    /// The floor's hint, with what the repack costs when it costs anything.
    pub fn hint(self) -> String {
        match (self.floor, self.repacked) {
            (_, 0) => self.floor.hint(),
            (Floor::NoHostExperts, r) => format!(
                ": every GPU layer of this preset with its experts in RAM ({r}) holds a type this CPU repacks, which the cache cannot read, so llama.cpp would run without one. Set Repack below to off."
            ),
            (f, r) => format!(
                "{} {r} of the {} GPU layers with their experts in RAM hold a type this CPU repacks and stay uncached (the numbers count the rest); Repack off adds them.",
                f.hint(),
                self.host_layers
            ),
        }
    }
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
}

impl Floor {
    /// The size below which the model does not load; 0 when any size does.
    pub fn load_bytes(self) -> u64 {
        match self {
            Floor::NoHostExperts => 0,
            Floor::Load(b) | Floor::Split { load: b, .. } => b,
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

/// The verdict of `p` on `m`; `None` when the model has no experts to cache
/// (not MoE, or no `_exps` tensor in the list, e.g. an unread GGUF).
pub fn floor(m: &Model, p: &Preset) -> Option<Verdict> {
    if m.expert_count == 0 || m.expert_used == 0 {
        return None;
    }
    let layers = expert_layers(m.tensors);
    if layers.is_empty() {
        return None;
    }
    let slots = u64::from(m.expert_used) + 1;
    let n_expert = u64::from(m.expert_count);

    let none = |repacked: u32, host_layers: u32| {
        Some(Verdict {
            floor: Floor::NoHostExperts,
            repacked,
            host_layers,
        })
    };
    let Some(layout) = Layout::new(m.n_layer as i32, p.n_gpu_layers.unwrap_or(-1)) else {
        return none(0, 0);
    };
    let rules = host_rules(&p.override_tensor);
    let host = |il: u32, names: &[&TensorInfo]| {
        p.n_cpu_moe.is_some_and(|n| (il as i32) < n)
            || names.iter().all(|t| first_rule_is_cpu(&rules, &t.name))
    };
    // Layers the cache could take: on a GPU position, every expert in RAM...
    let host_layers: Vec<(u32, &Vec<&TensorInfo>)> = layers
        .iter()
        .filter(|(il, ts)| (**il as i32) >= layout.start && host(**il, ts))
        .map(|(il, ts)| (*il, ts))
        .collect();
    // ...and of those, the ones whose experts all stay plain host weights.
    let repack_on = p.repack != Some(false);
    let (repacked, cached): (Vec<_>, Vec<_>) = host_layers
        .iter()
        .partition(|(_, ts)| repack_on && ts.iter().any(|t| repacks(t, m.isa)));
    let repacked = repacked.len() as u32;
    let n_host = host_layers.len() as u32;
    if cached.is_empty() {
        return none(repacked, n_host);
    }
    let verdict = |floor| {
        Some(Verdict {
            floor,
            repacked,
            host_layers: n_host,
        })
    };
    // One expert's bytes per layer: nb[2] summed over its tensors.
    let one_expert = |ts: &[&TensorInfo]| ts.iter().map(|t| t.bytes / n_expert).sum::<u64>();

    let Some((weights, device_of)) = device_map(p, layout) else {
        let cheapest = cached
            .iter()
            .map(|(_, ts)| one_expert(ts))
            .min()
            .unwrap_or(0);
        return verdict(Floor::Load(cheapest * slots));
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
    verdict(Floor::Split { load, every })
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

    const IQ3_XXS: u32 = 18;
    const IQ4_NL: u32 = 20;
    const Q2_K: u32 = 10;
    const AVX2: Isa = Isa {
        avx2: true,
        avx512: false,
    };

    /// A tensor of a type no CPU repacks, 4096 rows.
    fn t(name: String, bytes: u64) -> TensorInfo {
        TensorInfo {
            name,
            bytes,
            ty: IQ3_XXS,
            ne1: 4096,
        }
    }

    /// Re-type the `ffn_down_exps` of layers `from..=to`, as imatrix mixes do.
    fn retype_down(ts: &mut [TensorInfo], from: u32, to: u32, ty: u32, ne1: u64) {
        for il in from..=to {
            let name = format!("blk.{il}.ffn_down_exps.weight");
            let t = ts.iter_mut().find(|t| t.name == name).unwrap();
            t.ty = ty;
            t.ne1 = ne1;
        }
    }

    fn split(v: Option<Verdict>) -> Option<Floor> {
        v.map(|v| v.floor)
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
            isa: AVX2,
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
            f.floor,
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
        let Some(Floor::Split { load, every }) = split(floor(&model(&ts, 9), &p)) else {
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
        assert_eq!(
            split(floor(&model(&ts, 9), &p)),
            Some(Floor::Load(27 * MIB))
        );
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
        assert_eq!(
            split(floor(&model(&ts, 9), &gpu)),
            Some(Floor::NoHostExperts)
        );
        let shadowed = Preset {
            override_tensor: r"exps=ROCm0,\.ffn_(up|down|gate)_exps=CPU".into(),
            ..gpu.clone()
        };
        assert_eq!(
            split(floor(&model(&ts, 9), &shadowed)),
            Some(Floor::NoHostExperts)
        );
        let cpu_rule = Preset {
            override_tensor: r"\.ffn_(up|down|gate)_exps=CPU".into(),
            ..gpu
        };
        assert_eq!(
            floor(&model(&ts, 9), &cpu_rule).map(Verdict::load_bytes),
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
        assert_eq!(
            split(floor(&model(&ts, 3), &p)),
            Some(Floor::Load(27 * MIB))
        );
    }

    // The Qwen3.8-Flash-Next IQ3_S shape: `ffn_down_exps` at IQ4_NL in some
    // layers. On AVX2 those repack and leave the cache, so the floors count the
    // rest; `repack = false` brings them back.
    #[test]
    fn repacked_layers_leave_the_cache_unless_repack_is_off() {
        let mut ts = tensors(8);
        // Layers 1-4 cost 3.5 MiB per expert at IQ4_NL: the cheapest left is 3.
        retype_down(&mut ts, 1, 4, IQ4_NL, 4096);
        for il in 1..=4 {
            let name = format!("blk.{il}.ffn_down_exps.weight");
            ts.iter_mut().find(|t| t.name == name).unwrap().bytes = 192 * MIB;
        }
        let p = Preset {
            device: "ROCm0".into(),
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        let v = floor(&model(&ts, 9), &p).unwrap();
        assert_eq!((v.repacked, v.host_layers), (4, 8));
        assert_eq!(
            v.floor,
            Floor::Split {
                load: 27 * MIB,
                every: 27 * MIB
            }
        );
        assert!(v.hint().contains("4 of the 8 GPU layers"), "{}", v.hint());

        let off = Preset {
            repack: Some(false),
            ..p.clone()
        };
        let v = floor(&model(&ts, 9), &off).unwrap();
        assert_eq!(v.repacked, 0);
        // Two layouts now, sharing the budget by host bytes (1792 and 1536 of
        // 3328 MiB): 31.5 MiB over 0.538 and 27 over 0.462, both 58.5 MiB, since
        // with four layers each a group's need and share scale alike.
        let Floor::Split { load, every } = v.floor else {
            panic!("expected a split floor");
        };
        assert_eq!((mib(load), mib(every)), (59, 59));
    }

    // Every host layer repacked: llama.cpp runs without a cache, and the hint
    // says why. A CPU without AVX2 repacks none of it, and neither does a type
    // whose ISA is missing (Q2_K wants AVX-512) or a row count not divisible
    // by 8.
    #[test]
    fn repack_depends_on_type_isa_and_rows() {
        let mut ts = tensors(2);
        retype_down(&mut ts, 1, 2, IQ4_NL, 4096);
        let p = Preset {
            n_cpu_moe: Some(99),
            ..Default::default()
        };
        let v = floor(&model(&ts, 3), &p).unwrap();
        assert_eq!((v.floor, v.repacked), (Floor::NoHostExperts, 2));
        assert!(v.hint().contains("Set Repack below to off"), "{}", v.hint());

        let old_cpu = Model {
            isa: Isa::default(),
            ..model(&ts, 3)
        };
        assert_eq!(floor(&old_cpu, &p).unwrap().repacked, 0);

        retype_down(&mut ts, 1, 2, Q2_K, 4096);
        assert_eq!(floor(&model(&ts, 3), &p).unwrap().repacked, 0);
        let avx512 = Model {
            isa: Isa {
                avx2: true,
                avx512: true,
            },
            ..model(&ts, 3)
        };
        assert_eq!(floor(&avx512, &p).unwrap().repacked, 2);

        retype_down(&mut ts, 1, 2, IQ4_NL, 4100);
        assert_eq!(floor(&model(&ts, 3), &p).unwrap().repacked, 0);
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
