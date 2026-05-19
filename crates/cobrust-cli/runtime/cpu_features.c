/*
 * cpu_features.c — Tier 1 runtime CPU feature detection helpers.
 *
 * Provides the two symbols used by the Cobrust Tier-1 runtime-dispatch
 * dispatcher (numerical-compute-hardware-tiering.md §Tier1):
 *
 *   __cobrust_cpu_avx512_supported()  → 1 if AVX-512F available, else 0
 *   __cobrust_cpu_avx2_supported()    → 1 if AVX2 available, else 0
 *
 * Uses __builtin_cpu_supports (GCC / Clang ≥ 4.9, available on all
 * x86_64 targets we ship). On non-x86_64 targets (aarch64) both
 * functions return 0; the Cobrust codegen skips dispatch entirely on
 * those targets (NEON is mandatory in armv8-a), so the 0 path is
 * purely a guard-against-link-error fallback.
 *
 * No inline assembly. No Rust unsafe. `#![forbid(unsafe_code)]` on the
 * Rust side is unaffected.
 *
 * Compiled into the runtime object via `cobrust-cli/build.rs` or the
 * same C-compile path used for cobrust_main.c.
 */

int __cobrust_cpu_avx512_supported(void) {
#if defined(__x86_64__) || defined(_M_X64)
    return __builtin_cpu_supports("avx512f") ? 1 : 0;
#else
    return 0;
#endif
}

int __cobrust_cpu_avx2_supported(void) {
#if defined(__x86_64__) || defined(_M_X64)
    return __builtin_cpu_supports("avx2") ? 1 : 0;
#else
    return 0;
#endif
}
