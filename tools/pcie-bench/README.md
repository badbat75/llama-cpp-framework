# pcie-bench

A host <-> GPU transfer micro-benchmark that reproduces the transfer paths used by
ggml-cuda's internal AllReduce (`ggml/src/ggml-cuda/allreduce.cu`). `split-mode tensor`
uses that AllReduce on Windows, where NCCL/RCCL do not exist. On a machine without
NVLink/xGMI it stages every reduction through pinned host memory. The slower card's
PCIe link therefore bounds tensor parallelism, and this tool measures that link.

It is a diagnostic, like `tools\report-memory.ps1`: it is not part of the build or the
installer.

## What it measures

One GPU per run, for sizes from 4 KiB to 32 MiB:

| Column | AllReduce path | How it is timed |
|---|---|---|
| DMA D2H / H2D | copy-engine path (large reductions) | wall clock around a synchronize |
| kernel store 8x256 / 64x256 | chunked kernel phase 1: 16-byte stores into mapped host memory | GPU events |
| kernel load 8x256 / 64x256 | chunked kernel phase 3: loads from mapped host memory | GPU events |
| handshake | phase 2: arrival-token round trip GPU -> host -> GPU | host clock over 5000 trips |

These results give starting values for the two AllReduce knobs:

- **`GGML_CUDA_AR_COPY_THRESHOLD`** (default 1 MiB) is the size at which a reduction
  switches from the kernel path to the copy engines. Start from the size where the DMA
  columns overtake the kernel-store column.
- **`GGML_CUDA_AR_COPY_CHUNK_BYTES`** (default: `nbytes/4` clamped to 512 KiB..2 MiB) is
  the copy-engine chunk. A large fixed cost per DMA call argues for the top of that range.

These are only starting values. The real AllReduce also waits for the other card, so
tune the final values on the actual pair, for example by sweeping `llama-bench -sm tensor`
across both variables.

## Build and run

Run `01-configure.ps1` once first, so that `build\config-build.psd1` exists. Then:

```powershell
tools\pcie-bench\build.ps1              # both binaries; -NoHip / -NoCuda to skip one
build\pcie-bench\pcie-bench-hip.exe R9700
build\pcie-bench\pcie-bench-cuda.exe 4070
```

The argument is a substring of the device name.

`build.ps1` configures the build the same way `02-build.ps1` configures llama.cpp:

- ROCm's clang is the CXX compiler, with `hip::device` turning the source into HIP;
- the patched `__clang_hip_runtime_wrapper.h` for the dist's clang major is force-included;
- nvcc builds the CUDA binary from `pcie-bench.cu`, which only includes the `.cpp`.

The HIP build copies the dist's `amdhip64_7.dll`, `amd_comgr.dll` and `rocm_kpack.dll`
next to the exe, as the installer does for `llama-server.exe`. Without the dist's
`amd_comgr.dll`, the driver's System32 HIP runtime enumerates no device while the Ryzen
iGPU is visible.

The HIP build targets `gfx1201;gfx1036` by default. `gfx1036` is included because the HIP
runtime refuses to load a code module on every device when one visible device lacks code
for its architecture. Pass `-DGPU_TARGETS=...` to the configure step for other cards.

The run allocates about 64 MiB of VRAM plus 64 MiB of pinned host memory, so it can run
beside a loaded llama-server. It takes under a minute.

## Two HIP-on-Windows traps (handled in the code)

- **Event timing of DMA copies is wrong.** It reported 270 GB/s over a link that
  measured 3.2 GB/s by wall clock. The DMA columns therefore use wall-clock time. Kernel
  event timings agree with wall-clock time.
- **A launched kernel does not start until the stream is flushed** by some later API
  call. A host thread that spins straight after the launch waits for a kernel that never
  starts. The handshake calls `hipStreamQuery` after the launch. Without that call, every
  trip timed out.

## Reference results (2026-09-24, X870E AORUS ELITE WIFI7)

| | R9700 (chipset slot, behind two Promontory 21) | RTX 4070 Super (CPU Gen5 x16 slot, Gen4 card) |
|---|---|---|
| DMA, 32 MiB | 3.2 GB/s each way | 26 GB/s each way |
| DMA, 4 KiB call | ~60 us | ~5 us |
| kernel store / load | 2.8 / 3.3 GB/s | 26 / 26.7 GB/s |
| handshake round trip | 2.4 us | 1.35 us |
| empty kernel | 8 to 13 us | 5 us |

The R9700 reports Gen5 x16, but that is only its link to the card's own internal switch.
On this board the card's real path to the CPU is the chipset uplink, and it measures about
half of what Gen4 x4 should deliver.

For this link, the numbers suggest `GGML_CUDA_AR_COPY_THRESHOLD` of 1 to 2 MiB and a
fixed 2 MiB `GGML_CUDA_AR_COPY_CHUNK_BYTES`. The DMA and kernel paths have almost the same
bandwidth here, and each DMA call carries tens of microseconds of fixed cost.
