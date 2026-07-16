# Patched HIP runtime wrappers, versioned per clang resource major

`<clang-major>\__clang_hip_runtime_wrapper.h` is a **modified copy of that
toolchain's own stock header** (`<HIP_PATH>\lib\llvm\lib\clang\<major>\include\`
on the TheRock dist). `02-build.ps1` detects the installed toolchain's clang
resource major and force-includes the matching copy — and **fails fast when no
matching copy exists**, because force-including a wrapper generated from a
different clang's headers means silently drifting from the toolchain (clang 23
added `__cluster_dims__`/`__no_cluster__` that the 7.1-era copy lacked).

## Why the patch exists

MSVC's `<cmath>` (`_CLANG_BUILTIN2`, first seen in MSVC 14.51 / VS 18) declares
constexpr math classification functions that are implicitly
`__host__ __device__` under clang. The stock wrapper includes `<cmath>` before
the HIP device math headers, so the `__device__` declarations of
`isgreater`/`isless`/etc. in `__clang_cuda_math_forward_declares.h` /
`__clang_hip_cmath.h` then fail with "cannot overload __host__ __device__
function". First hit on ROCm 7.1; still present on TheRock ROCm 7.14
(AMD clang 23) + MSVC 14.51.36231 (verified 2026-07-16).

## Regeneration recipe (on a new clang major)

1. Copy the stock header:
   `<HIP_PATH>\lib\llvm\lib\clang\<major>\include\__clang_hip_runtime_wrapper.h`
   → `patches\hip\<major>\__clang_hip_runtime_wrapper.h`.
2. Replace the include guard with the PATCHED double-guard (copy the block from
   the previous version verbatim): a `__CLANG_HIP_RUNTIME_WRAPPER_PATCHED_H__`
   guard, plus defining `__CLANG_HIP_RUNTIME_WRAPPER_H__` so the stock wrapper
   (which the HIP driver -include's unconditionally) no-ops. The build passes
   `-D__CLANG_HIP_RUNTIME_WRAPPER_H__ -include <this file>`.
3. Move the HIP device math includes (`__clang_hip_libdevice_declares.h`,
   `__clang_hip_math.h`, `__clang_hip_stdlib.h`, and the
   `__clang_cuda_math_forward_declares.h`/`__clang_hip_cmath.h`/
   `__clang_cuda_complex_builtins.h` block) BEFORE the `#include <cmath>`
   block, prefixed with `#include <math.h>` (provides FP_NAN etc. that
   `__clang_hip_cmath.h` needs). Keep `<algorithm>`/`<complex>`/`<new>` in the
   std-lib section after `<cmath>`. Keep everything else byte-identical to
   stock — the patch is the include ORDER, nothing else.
4. Validate both directions with a small `-x hip` TU that includes `<cmath>` +
   `hip/hip_runtime.h` and calls `std::sqrt`/`std::isgreater`/`sinf` in a
   kernel (inside a VS dev shell, with the TheRock env set):
   - stock (no flags): expected to FAIL with the overload clash — if it
     compiles clean, MSVC/ROCm fixed it upstream: delete this machinery
     instead of regenerating.
   - patched (`-D__CLANG_HIP_RUNTIME_WRAPPER_H__ -include <patched>`): must
     compile clean.
5. Commit the new directory. Old majors can be deleted once no supported dist
   ships them (git history keeps them; the ROCm 7.1-era unversioned copy lives
   in history as `patches\__clang_hip_runtime_wrapper.h`).
