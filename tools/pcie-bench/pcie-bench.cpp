// pcie-bench: host <-> GPU transfer micro-benchmark mirroring the two paths of
// ggml-cuda's internal AllReduce (ggml/src/ggml-cuda/allreduce.cu), the one the
// tensor split mode uses on Windows, where NCCL/RCCL do not exist:
//
//   - copy-engine path: cudaMemcpyAsync D2H / H2D against pinned host memory;
//   - chunked kernel path: 16-byte vector stores into, and loads from, mapped
//     pinned host memory, run by 8 blocks of 256 threads like the AR kernel
//     (64 blocks alongside, to show whether more SMs help);
//   - the arrival-token handshake: the GPU writes a token to mapped host
//     memory and spins until the host echoes it back.
//
// The crossover between the first two is what GGML_CUDA_AR_COPY_THRESHOLD
// sets, and the copy-engine cost per call is what GGML_CUDA_AR_COPY_CHUNK_BYTES
// trades against. One GPU at a time: the real AllReduce also waits for the
// peer, so these numbers seed the knobs, they do not replace tuning on the pair.
//
// Built twice from this one file: as HIP (clang, pcie-bench-hip.exe) and as
// CUDA (nvcc, pcie-bench-cuda.exe, through pcie-bench.cu). See README.md.
//
// Two HIP-on-Windows traps this file works around, both found writing it:
//   - event timing of DMA copies is bogus there (it reported 270 GB/s over a
//     link that moves 3.2), so DMA is timed wall-clock around a synchronize;
//   - a launched kernel does not start until the stream is flushed by some
//     later API call, so a host thread that spins right after the launch waits
//     for a kernel that never runs; the handshake flushes with a stream query.

#if defined(__HIP_PLATFORM_AMD__) || defined(__HIPCC__)
#include <hip/hip_runtime.h>
#define gpuSetDevice hipSetDevice
#define gpuGetDeviceCount hipGetDeviceCount
#define gpuGetDeviceProperties hipGetDeviceProperties
#define gpuDeviceProp hipDeviceProp_t
#define gpuMalloc hipMalloc
#define gpuHostAlloc hipHostMalloc
#define gpuHostAllocPortable hipHostMallocPortable
#define gpuHostAllocMapped hipHostMallocMapped
#define gpuHostGetDevicePointer hipHostGetDevicePointer
#define gpuMemcpyAsync hipMemcpyAsync
#define gpuMemcpyD2H hipMemcpyDeviceToHost
#define gpuMemcpyH2D hipMemcpyHostToDevice
#define gpuStream_t hipStream_t
#define gpuStreamCreate hipStreamCreate
#define gpuStreamSynchronize hipStreamSynchronize
#define gpuStreamQuery hipStreamQuery
#define gpuEvent_t hipEvent_t
#define gpuEventCreate hipEventCreate
#define gpuEventRecord hipEventRecord
#define gpuEventSynchronize hipEventSynchronize
#define gpuEventElapsedTime hipEventElapsedTime
#define gpuGetLastError hipGetLastError
#define gpuGetErrorString hipGetErrorString
#define gpuSuccess hipSuccess
#define gpuError_t hipError_t
#define GPU_SLEEP() __builtin_amdgcn_s_sleep(4)
#define GPU_BACKEND "HIP"
#else
#include <cuda_runtime.h>
#define gpuSetDevice cudaSetDevice
#define gpuGetDeviceCount cudaGetDeviceCount
#define gpuGetDeviceProperties cudaGetDeviceProperties
#define gpuDeviceProp cudaDeviceProp
#define gpuMalloc cudaMalloc
#define gpuHostAlloc cudaHostAlloc
#define gpuHostAllocPortable cudaHostAllocPortable
#define gpuHostAllocMapped cudaHostAllocMapped
#define gpuHostGetDevicePointer cudaHostGetDevicePointer
#define gpuMemcpyAsync cudaMemcpyAsync
#define gpuMemcpyD2H cudaMemcpyDeviceToHost
#define gpuMemcpyH2D cudaMemcpyHostToDevice
#define gpuStream_t cudaStream_t
#define gpuStreamCreate cudaStreamCreate
#define gpuStreamSynchronize cudaStreamSynchronize
#define gpuStreamQuery cudaStreamQuery
#define gpuEvent_t cudaEvent_t
#define gpuEventCreate cudaEventCreate
#define gpuEventRecord cudaEventRecord
#define gpuEventSynchronize cudaEventSynchronize
#define gpuEventElapsedTime cudaEventElapsedTime
#define gpuGetLastError cudaGetLastError
#define gpuGetErrorString cudaGetErrorString
#define gpuSuccess cudaSuccess
#define gpuError_t cudaError_t
#define GPU_SLEEP() __nanosleep(100)
#define GPU_BACKEND "CUDA"
#endif

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

#define CHECK(x) do { gpuError_t e_ = (x); if (e_ != gpuSuccess) { \
    fprintf(stderr, "%s:%d %s -> %s\n", __FILE__, __LINE__, #x, gpuGetErrorString(e_)); exit(1); } } while (0)

// Phase 1 of the AR kernel: vector stores from the GPU into mapped host memory.
__global__ void k_store(const int4 * src, int4 * host, int n) {
    for (int i = blockIdx.x * blockDim.x + threadIdx.x; i < n; i += gridDim.x * blockDim.x) {
        host[i] = src[i];
    }
    __threadfence_system();
}

// Phase 3 of the AR kernel: vector loads from mapped host memory, summed locally.
__global__ void k_load(const int4 * host, int4 * dst, int n) {
    for (int i = blockIdx.x * blockDim.x + threadIdx.x; i < n; i += gridDim.x * blockDim.x) {
        const int4 v = host[i];
        int4 d = dst[i];
        d.x += v.x; d.y += v.y; d.z += v.z; d.w += v.w;
        dst[i] = d;
    }
}

// Phase 2 of the AR kernel: signal a token, spin until the host echoes it.
// Bounded, so a handshake that never completes reports instead of hanging.
__global__ void k_pingpong(int * out, const int * in, int iters, int * timeouts) {
    for (int t = 1; t <= iters; ++t) {
        *(volatile int *) out = t;
        __threadfence_system();
        long long guard = 0;
        while (*(const volatile int *) in != t) {
            GPU_SLEEP();
            if (++guard > 20000000LL) { atomicAdd(timeouts, 1); return; }
        }
    }
}

static double median(std::vector<double> v) {
    std::sort(v.begin(), v.end());
    return v[v.size() / 2];
}

int main(int argc, char ** argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <device-name-substring>   (e.g. R9700, 4070)\n", argv[0]);
    }
    const char * want = argc > 1 ? argv[1] : "";
    int n_dev = 0;
    CHECK(gpuGetDeviceCount(&n_dev));
    int dev = -1;
    for (int i = 0; i < n_dev; ++i) {
        gpuDeviceProp p;
        CHECK(gpuGetDeviceProperties(&p, i));
        printf("# %s device %d: %s (pci %02x:%02x)\n", GPU_BACKEND, i, p.name, p.pciBusID, p.pciDeviceID);
        if (dev < 0 && strstr(p.name, want)) dev = i;
    }
    if (dev < 0) { fprintf(stderr, "no device matching '%s'\n", want); return 1; }
    CHECK(gpuSetDevice(dev));
    gpuDeviceProp prop;
    CHECK(gpuGetDeviceProperties(&prop, dev));
    printf("# using device %d: %s\n", dev, prop.name);

    const size_t MAXB = 32u << 20;
    void * d_a = nullptr;
    void * d_b = nullptr;
    CHECK(gpuMalloc(&d_a, MAXB));
    CHECK(gpuMalloc(&d_b, MAXB));
    // Same flags as ggml_cuda_ar_host_mapping: portable + mapped.
    void * h_pin = nullptr;
    CHECK(gpuHostAlloc(&h_pin, MAXB, gpuHostAllocPortable | gpuHostAllocMapped));
    void * h_map = nullptr;
    CHECK(gpuHostAlloc(&h_map, MAXB, gpuHostAllocPortable | gpuHostAllocMapped));
    void * h_map_dev = nullptr;
    CHECK(gpuHostGetDevicePointer(&h_map_dev, h_map, 0));
    memset(h_pin, 1, MAXB);
    memset(h_map, 1, MAXB);

    gpuStream_t s;
    CHECK(gpuStreamCreate(&s));
    gpuEvent_t e0, e1;
    CHECK(gpuEventCreate(&e0));
    CHECK(gpuEventCreate(&e1));

    const size_t sizes[] = { 4u << 10, 16u << 10, 64u << 10, 256u << 10, 512u << 10, 1u << 20,
                             2u << 20, 4u << 20, 8u << 20, 16u << 20, 32u << 20 };

    // Kernels: event-timed, one launch per sample, median.
    auto time_kernel = [&](auto && launch, int reps) {
        std::vector<double> us;
        for (int w = 0; w < 3; ++w) launch();
        CHECK(gpuStreamSynchronize(s));
        for (int r = 0; r < reps; ++r) {
            CHECK(gpuEventRecord(e0, s));
            launch();
            CHECK(gpuEventRecord(e1, s));
            CHECK(gpuEventSynchronize(e1));
            float ms = 0;
            CHECK(gpuEventElapsedTime(&ms, e0, e1));
            us.push_back(ms * 1000.0);
        }
        CHECK(gpuGetLastError());
        return median(us);
    };
    // DMA copies: wall-clock around a synchronize (see the header), so the
    // figure includes the per-call cost a synchronized copy really pays.
    auto time_copy = [&](auto && launch, int reps) {
        std::vector<double> us;
        launch();
        CHECK(gpuStreamSynchronize(s));
        for (int r = 0; r < reps; ++r) {
            const auto t0 = std::chrono::steady_clock::now();
            launch();
            CHECK(gpuStreamSynchronize(s));
            us.push_back(std::chrono::duration<double, std::micro>(std::chrono::steady_clock::now() - t0).count());
        }
        return median(us);
    };

    printf("\n%8s  %21s  %21s  %21s  %21s  %21s  %21s\n", "",
           "DMA D2H (wall)", "DMA H2D (wall)", "kernel store 8x256", "kernel store 64x256",
           "kernel load 8x256", "kernel load 64x256");
    printf("%8s", "KiB");
    for (int c = 0; c < 6; ++c) printf("  %10s %10s", "us", "GB/s");
    printf("\n");
    for (size_t b : sizes) {
        const int reps = (int) std::max<size_t>(15, std::min<size_t>(200, (256u << 20) / b));
        const int n4 = (int) (b / sizeof(int4));
        const double r[6] = {
            time_copy([&] { CHECK(gpuMemcpyAsync(h_pin, d_a, b, gpuMemcpyD2H, s)); }, reps),
            time_copy([&] { CHECK(gpuMemcpyAsync(d_a, h_pin, b, gpuMemcpyH2D, s)); }, reps),
            time_kernel([&] { k_store<<<8, 256, 0, s>>>((const int4 *) d_a, (int4 *) h_map_dev, n4); }, reps),
            time_kernel([&] { k_store<<<64, 256, 0, s>>>((const int4 *) d_a, (int4 *) h_map_dev, n4); }, reps),
            time_kernel([&] { k_load<<<8, 256, 0, s>>>((const int4 *) h_map_dev, (int4 *) d_b, n4); }, reps),
            time_kernel([&] { k_load<<<64, 256, 0, s>>>((const int4 *) h_map_dev, (int4 *) d_b, n4); }, reps),
        };
        printf("%8zu", b >> 10);
        for (double us : r) printf("  %10.1f %10.2f", us, b / (us * 1e-6) / 1e9);
        printf("\n");
        fflush(stdout);
    }

    // Arrival-token round trip GPU -> host -> GPU through mapped memory.
    int * flags = nullptr;
    CHECK(gpuHostAlloc((void **) &flags, 4096, gpuHostAllocPortable | gpuHostAllocMapped));
    int * flags_dev = nullptr;
    CHECK(gpuHostGetDevicePointer((void **) &flags_dev, flags, 0));
    int * d_timeouts = nullptr;
    CHECK(gpuMalloc((void **) &d_timeouts, sizeof(int)));
    printf("\n");
    for (int round = 0; round < 3; ++round) {
        memset(flags, 0, 4096);
        CHECK(gpuMemcpyAsync(d_timeouts, flags + 512, sizeof(int), gpuMemcpyH2D, s));
        CHECK(gpuStreamSynchronize(s));
        const int iters = 5000;
        volatile int * out = flags;       // written by the GPU
        volatile int * in  = flags + 16;  // written by the host, on its own cache line
        bool host_timeout = false;
        const auto t0 = std::chrono::steady_clock::now();
        k_pingpong<<<1, 1, 0, s>>>(flags_dev, flags_dev + 16, iters, d_timeouts);
        (void) gpuStreamQuery(s);  // flush: see the header
        for (int t = 1; t <= iters && !host_timeout; ++t) {
            const auto ts = std::chrono::steady_clock::now();
            while (*out != t) {
                if (std::chrono::steady_clock::now() - ts > std::chrono::seconds(2)) { host_timeout = true; break; }
            }
            if (!host_timeout) *in = t;
        }
        const auto t1 = std::chrono::steady_clock::now();
        CHECK(gpuStreamSynchronize(s));
        int timeouts = 0;
        CHECK(gpuMemcpyAsync(&timeouts, d_timeouts, sizeof(int), gpuMemcpyD2H, s));
        CHECK(gpuStreamSynchronize(s));
        const double us = std::chrono::duration<double, std::micro>(t1 - t0).count();
        printf("handshake round %d: %d iters, %.2f us per round trip%s%s\n", round, iters, us / iters,
               host_timeout ? " (HOST TIMEOUT)" : "", timeouts ? " (GPU TIMEOUT)" : "");
    }

    const double kempty = time_kernel([&] { k_store<<<1, 32, 0, s>>>((const int4 *) d_a, (int4 *) h_map_dev, 0); }, 200);
    printf("empty kernel (event-timed): %.1f us\n", kempty);
    return 0;
}
