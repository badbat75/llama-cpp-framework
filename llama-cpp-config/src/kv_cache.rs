//! KV-cache types, and which backend can actually run flash attention over them.
//!
//! The Models tab offers every type llama.cpp accepts for `--cache-type-k/-v`
//! (`kv_cache_types` in `common/arg.cpp`, mirrored by `TYPES`), but accepting a
//! type is not the same as having a GPU kernel for it, and the gaps are silent:
//!
//! - **CUDA and HIP have no FlashAttention kernel for `iq4_nl`**
//!   (`ggml_cuda_fattn_kv_type_supported`, `ggml-cuda/fattn.cu`). A quantized V
//!   forces flash attention on (`llama_init_from_model`), so the op then runs on
//!   the CPU for every layer on those cards; with an unquantized V and flash
//!   attention on `auto`, llama.cpp detects the mismatch and turns it off instead
//!   (`llama_context::resolve_fused_ops`).
//! - **Vulkan covers `iq4_nl` too, not `f8_e4m3`, and `bf16` only on K and V
//!   together** (`fa_kv_ok`, `ggml_backend_vk_device_supports_op`,
//!   `GGML_OP_FLASH_ATTN_EXT`).
//! - **`f8_e4m3` is a plain 8-bit float: NOT quantized, block size 1**
//!   (`[GGML_TYPE_F8_E4M3]`'s traits, `ggml.c`). A quantized V does not force
//!   flash attention on for it, it loads with flash attention `off`, and the
//!   per-layer block-size check passes for any head width. `is_quantized` below
//!   mirrors the traits table, not the name.
//! - **On CUDA and HIP a supported pair still needs a compiled VECTOR kernel** for
//!   the decode steps of 1-2 query tokens; without one those steps convert the
//!   whole used cache to f16 first ("no FlashAttention vector kernel compiled",
//!   `ggml_cuda_flash_attn_ext_vec`). Which pairs are compiled is a BUILD setting,
//!   `GGML_CUDA_FA_QUANTS`, and `NATIVE_VEC_PAIRS` mirrors the one this framework
//!   builds with (`xtask/cmake/common.toml`, pinned by a test below). The configurator
//!   ships in the same installer as those binaries, so the mirror describes the
//!   llama-server next to it.
//! - **A quantized V with flash attention `off` does not load at all**
//!   (`quantized V cache requires flash_attn to be enabled`).
//!
//! All of this is a copy of upstream logic (llama.cpp v1.0.0) rather than a probe
//! of the installed DLLs: re-check `fattn.cu`, the Vulkan `supports_op` arm and
//! `kv_cache_types` on every llama.cpp bump. A type that gains a kernel and is
//! missing here only costs a needless warning; a type listed here WITHOUT one
//! hides a CPU fallback.

use crate::strings;
use crate::devices::DeviceOption;

/// Every type llama.cpp accepts for the KV cache, highest precision first (the
/// order the Models tab lists them in, after its own "default" entry). Equal
/// to `Options.cache_types` minus "default": asserted by the e2e test.
pub const TYPES: &[&str] = &[
    "f32", "f16", "bf16", "f8_e4m3", "q8_0", "q5_1", "q5_0", "q4_1", "q4_0", "iq4_nl",
];

/// K-V pairs with a compiled FlashAttention vector kernel on CUDA and HIP in
/// this framework's build: `GGML_CUDA_FA_QUANTS` in `xtask/cmake/common.toml`, plus
/// `f16-f16`, which ggml's CMake adds whatever the list says.
pub const NATIVE_VEC_PAIRS: &[(&str, &str)] = &[
    ("q4_0", "q4_0"),
    ("q8_0", "q8_0"),
    ("q5_0", "q5_0"),
    ("q5_1", "q5_1"),
    ("bf16", "bf16"),
    ("f8_e4m3", "f8_e4m3"),
    ("f16", "f16"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    /// CUDA and HIP (ROCm) share one flash-attention implementation.
    CudaHip,
    Vulkan,
}

/// The form carries "default" (or an empty string) for "omit the flag", and
/// llama.cpp's default for both the model and the draft context is f16.
fn effective(t: &str) -> &str {
    if t.is_empty() || t == "default" {
        "f16"
    } else {
        t
    }
}

/// Mirrors `ggml_is_quantized` for the cache types (`is_quantized` in the type's
/// traits, `ggml.c`): `f8_e4m3` is a plain float, a quantized V does not force
/// flash attention on for it and it loads with flash attention `off`.
fn is_quantized(t: &str) -> bool {
    !matches!(t, "f32" | "f16" | "bf16" | "f8_e4m3")
}

fn backend_of(device_id: &str) -> Option<Backend> {
    let id = device_id.trim();
    if id.starts_with("CUDA") || id.starts_with("ROCm") {
        Some(Backend::CudaHip)
    } else if id.starts_with("Vulkan") {
        Some(Backend::Vulkan)
    } else {
        None
    }
}

/// `true` when the backend has ANY FlashAttention kernel for this K-V pair.
fn cuda_hip_fa_type(t: &str) -> bool {
    matches!(
        t,
        "f32" | "f16" | "bf16" | "f8_e4m3" | "q4_0" | "q4_1" | "q5_0" | "q5_1" | "q8_0"
    )
}

/// Vulkan's `fa_kv_ok`: CUDA/HIP's set plus iq4_nl, minus f8_e4m3.
fn vulkan_fa_type(t: &str) -> bool {
    (cuda_hip_fa_type(t) && t != "f8_e4m3") || t == "iq4_nl"
}

fn has_fa_kernel(backend: Backend, k: &str, v: &str) -> bool {
    match backend {
        Backend::CudaHip => cuda_hip_fa_type(k) && cuda_hip_fa_type(v),
        Backend::Vulkan => {
            vulkan_fa_type(k) && vulkan_fa_type(v) && ((k == "bf16") == (v == "bf16"))
        }
    }
}

fn has_native_vec_kernel(k: &str, v: &str) -> bool {
    NATIVE_VEC_PAIRS.iter().any(|(pk, pv)| *pk == k && *pv == v)
}

/// What the warning looks at, gathered by the GUI from the form.
pub struct Input<'a> {
    /// The preset's `device` list (comma-separated ids, empty = not pinned).
    pub device: &'a str,
    /// `--list-devices`, standing in for "every device" when nothing is pinned.
    pub probed: &'a [DeviceOption],
    pub cache_type_k: &'a str,
    pub cache_type_v: &'a str,
    /// "", "default", "on" or "off".
    pub flash_attn: &'a str,
    /// The draft context's types, when the preset has one (a draft file or
    /// embedded MTP heads); `None` otherwise.
    pub draft: Option<(&'a str, &'a str)>,
}

/// The backends the model will run on: the preset's pin, else every probed GPU
/// (llama.cpp's own default).
fn backends(input: &Input) -> Vec<Backend> {
    let pinned = input.device;
    let ids: Vec<String> = if pinned.trim().is_empty() {
        input.probed.iter().map(|d| d.id.clone()).collect()
    } else {
        pinned.split(',').map(|s| s.trim().to_string()).collect()
    };
    let mut out = Vec::new();
    for b in ids.iter().filter_map(|id| backend_of(id)) {
        if !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

/// What happens to a layer whose backend has no FlashAttention kernel for the
/// pair: forced flash attention moves the op to the CPU, `auto` turns it off.
fn fallback(fa_forced: bool) -> String {
    if fa_forced {
        strings::t("fa-forced-fallback")
    } else {
        strings::t("fa-default-fallback")
    }
}

/// One cache's verdict (the model's or the draft's), or `None` when it is fine.
fn check(label: &str, backends: &[Backend], k: &str, v: &str, flash_attn: &str) -> Option<String> {
    let (k, v) = (effective(k), effective(v));
    // Only a hand-edited presets.ini gets here (the combos offer TYPES), and the
    // combo then shows "default" for a value it does not list, so the strip is
    // the one place that can say the launch will fail (`Unsupported cache type`).
    if let Some(bad) = [k, v].into_iter().find(|t| !TYPES.contains(t)) {
        return Some(strings::fill("kv-type-unknown", &[&label, &bad, &TYPES.join(", ")]));
    }
    if flash_attn == "off" {
        // No flash attention at all: no kernel to miss, one refusal to report.
        return is_quantized(v).then(|| {
            strings::fill("kv-quantized-needs-fa", &[&label, &v])
        });
    }
    // A quantized V forces flash attention on, whatever `auto` would pick.
    let fa_forced = flash_attn == "on" || is_quantized(v);
    if backends.contains(&Backend::CudaHip) && !has_fa_kernel(Backend::CudaHip, k, v) {
        return Some(strings::fill("kv-no-kernel-cuda", &[&label, &k, &v, &fallback(fa_forced)]));
    }
    if backends.contains(&Backend::Vulkan) && !has_fa_kernel(Backend::Vulkan, k, v) {
        // Either a type with no kernel at all, or a bf16 on one side only.
        let fb = fallback(fa_forced);
        return Some(if vulkan_fa_type(k) && vulkan_fa_type(v) {
            strings::fill("kv-no-kernel-vulkan", &[&label, &k, &v, &fb])
        } else {
            strings::fill("kv-no-kernel-vulkan-type", &[&label, &k, &v, &fb])
        });
    }
    if backends.contains(&Backend::CudaHip) && !has_native_vec_kernel(k, v) {
        return Some(strings::fill("kv-no-decode-kernel", &[&label, &k, &v]));
    }
    None
}

/// The Models tab's KV-cache strip: empty when every cache the preset builds has
/// a kernel where it runs. The model's own cache is checked before the draft's,
/// and each reports at most its most serious problem.
pub fn warning(input: &Input) -> String {
    let backends = backends(input);
    let mut lines = Vec::new();
    if let Some(w) = check(
        &strings::t("kv-cache"),
        &backends,
        input.cache_type_k,
        input.cache_type_v,
        input.flash_attn,
    ) {
        lines.push(w);
    }
    if let Some((dk, dv)) = input.draft {
        if let Some(w) = check(&strings::t("draft-kv-cache-label"), &backends, dk, dv, input.flash_attn) {
            lines.push(w);
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str) -> DeviceOption {
        DeviceOption {
            id: id.into(),
            label: id.into(),
            name: String::new(),
            total_mib: 0,
            free_mib: 0,
        }
    }

    fn input<'a>(device: &'a str, k: &'a str, v: &'a str) -> Input<'a> {
        Input {
            device,
            probed: &[],
            cache_type_k: k,
            cache_type_v: v,
            flash_attn: "",
            draft: None,
        }
    }

    #[test]
    fn matched_pairs_with_native_kernels_are_silent_everywhere() {
        for t in ["default", "f16", "bf16", "q8_0", "q5_1", "q5_0", "q4_0"] {
            for dev in ["ROCm1", "CUDA0", "Vulkan1", "ROCm1,CUDA0"] {
                assert_eq!(warning(&input(dev, t, t)), "", "{dev} {t}");
            }
        }
    }

    #[test]
    fn iq4_nl_is_fine_on_vulkan_and_falls_to_the_cpu_on_cuda_and_rocm() {
        assert_eq!(warning(&input("Vulkan1", "iq4_nl", "iq4_nl")), "");
        for dev in ["ROCm1", "CUDA0", "Vulkan1,CUDA0"] {
            let w = warning(&input(dev, "iq4_nl", "iq4_nl"));
            assert!(w.contains("on the CPU"), "{dev}: {w}");
        }
        // An unquantized V leaves flash attention on auto, which turns it off.
        let w = warning(&input("CUDA0", "iq4_nl", "f16"));
        assert!(w.contains("without flash attention"), "{w}");
    }

    #[test]
    fn f8_e4m3_is_fine_on_cuda_and_rocm_and_falls_back_on_vulkan() {
        // The compiled f8_e4m3-f8_e4m3 pair: silent, like q8_0.
        for dev in ["ROCm1", "CUDA0", "ROCm1,CUDA0"] {
            assert_eq!(warning(&input(dev, "f8_e4m3", "f8_e4m3")), "", "{dev}");
        }
        // Vulkan's fa_kv_ok does not list it: the pair warns there, like iq4_nl
        // warns on CUDA/HIP.
        let w = warning(&input("Vulkan1", "f8_e4m3", "f8_e4m3"));
        assert!(w.contains("without flash attention"), "{w}");
        assert!(w.contains("Vulkan has no flash attention kernel"), "{w}");
        assert!(!w.contains("bf16"), "{w}");
    }

    #[test]
    fn mixed_pairs_take_the_slow_decode_path_on_cuda_and_rocm_only() {
        let w = warning(&input("ROCm1", "q8_0", "q5_0"));
        assert!(w.contains("no native flash attention decode kernel"), "{w}");
        assert!(w.contains("K q8_0 / V q5_0"), "{w}");
        assert_eq!(warning(&input("Vulkan1", "q8_0", "q5_0")), "");
        // q4_1 is a supported type with no compiled pair of its own.
        assert!(warning(&input("CUDA0", "q4_1", "q4_1")).contains("no native"));
        // With flash attention off there is no FA kernel to miss (an f16 V loads).
        let mut off = input("CUDA0", "q8_0", "f16");
        off.flash_attn = "off";
        assert_eq!(warning(&off), "");
        // f8_e4m3 is not quantized (traits), so an FA-off load with an F8 V passes.
        let mut off = input("ROCm1", "f8_e4m3", "f8_e4m3");
        off.flash_attn = "off";
        assert_eq!(warning(&off), "");
    }

    #[test]
    fn a_quantized_v_with_flash_attention_off_does_not_load() {
        let mut i = input("Vulkan1", "q8_0", "q8_0");
        i.flash_attn = "off";
        assert!(warning(&i).contains("does not load"));
    }

    #[test]
    fn a_type_llama_cpp_does_not_accept_is_reported_before_any_kernel_check() {
        let w = warning(&input("Vulkan1", "q6_K", "q8_0"));
        assert!(w.contains("q6_K is not a KV cache type"), "{w}");
        assert!(
            w.contains("iq4_nl"),
            "the accepted list is spelled out: {w}"
        );
    }

    #[test]
    fn vulkan_needs_bf16_on_both_or_neither() {
        assert!(warning(&input("Vulkan1", "bf16", "f16")).contains("bf16 on both"));
        assert_eq!(warning(&input("Vulkan1", "bf16", "bf16")), "");
    }

    #[test]
    fn nothing_pinned_means_every_probed_gpu() {
        let probed = [dev("CUDA0"), dev("Vulkan1"), dev("CPU")];
        let mut none = input("", "iq4_nl", "iq4_nl");
        none.probed = &probed;
        assert!(warning(&none).contains("on the CPU"));
        // No pin and no probe yet: nothing to judge the kernels against.
        assert_eq!(warning(&input("", "iq4_nl", "iq4_nl")), "");
    }

    #[test]
    fn the_draft_cache_is_checked_on_its_own_line() {
        let mut i = input("ROCm1", "q8_0", "q8_0");
        i.draft = Some(("iq4_nl", "iq4_nl"));
        let w = warning(&i);
        assert!(w.starts_with("Draft KV cache:"), "{w}");
        assert_eq!(w.lines().count(), 1);
        // "default" for the draft is f16, which is always fine.
        i.draft = Some(("default", "default"));
        assert_eq!(warning(&i), "");
    }

    /// `NATIVE_VEC_PAIRS` must describe the build the installer ships:
    /// `GGML_CUDA_FA_QUANTS` in the repo's `xtask/cmake/common.toml`, plus f16-f16.
    #[test]
    fn native_pairs_mirror_the_build_setting() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../xtask/cmake/common.toml");
        let text = std::fs::read_to_string(path).expect("read xtask/cmake/common.toml");
        let value = text
            .lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix("\"GGML_CUDA_FA_QUANTS="))
            .and_then(|l| l.strip_suffix("\","))
            .expect("GGML_CUDA_FA_QUANTS entry");
        let mut built: Vec<(String, String)> = value
            .split([',', ';'])
            .map(|pair| {
                let (k, v) = pair.trim().split_once('-').expect("K-V pair");
                (k.to_string(), v.to_string())
            })
            .collect();
        built.push(("f16".into(), "f16".into()));
        built.sort();
        built.dedup();
        let mut mirrored: Vec<(String, String)> = NATIVE_VEC_PAIRS
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        mirrored.sort();
        assert_eq!(mirrored, built);
    }
}
