# Allocator-abort attribution and analyzer fix

The historical exit250 was from the standalone Luau analyzer, not the Bead UI
process. On exact B6 source7ee79580, repeated workflow stages used the same
170-module generated-SDK mirror. Runs1–11 passed both stages. Run12 printed
CHECK_OK and exited0 from `nacre-shell --check`; the subsequent analyzer stage
reported `malloc(): unaligned tcache chunk detected` and exited250.
`tools/check-luau.py` captured the analyzer output and propagated its
SIGABRT(-6) through Python SystemExit, yielding250.

Commands ran remotely at nice10 with MALLOC_CHECK_=3 and MALLOC_PERTURB_=173.
That first series did not preload glibc's optional debug library. A separate
series explicitly loaded `libc_malloc_debug.so` on glibc2.44; a same-PID
pre-exec map records `libc_malloc_debug.so.0`. Runs1–27 passed, and run28
crashed in the old analyzer with SIGSEGV, propagated as245 by the old checker.
Thus the analyzer fault is reproduced both with the original tcache message
and under the real debug allocator. The fixed0.742 analyzer completed100/100 exact project checks under the same
real debug-allocator preload. The preload map and environment are recorded
separately from the earlier100 runs without that library.

Upstream [issue3020](https://github.com/luau-lang/luau/issues/3020) identifies
parallel checks racing on a shared `InternalErrorReporter` module-name string.
The [0.742 release](https://github.com/luau-lang/luau/releases/tag/0.742) removes
that shared reporter. The existing0.740 pin is replaced with0.742; embedded
runtime Luau dependencies and generated SDK are unchanged. The downloaded zip
hash matches the official release API digest:
`152058b98c60ff1f5b5ec9bca0b8eb37793cae161dd3a05608cc3d8738ba1a4d`.

The fixed analyzer completed100/100 exact project checks with the same allocator
environment. The patched runner also passes a fake-analyzer SIGABRT regression:
it prints `LUAU_ANALYZE_FAILED signal=SIGABRT` and returns1, while the actual
0.742 checker passes all170 modules. Full final CI and scroll-soak results are
recorded separately once complete.

Fresh Rust/std plus Luau-C++ ASan checks also completed20 teardowns without a
finding; those tests did not reproduce the historical analyzer fault. ASan
interception rejects the mmap-threshold mallopt operation, which is reported as
an instrumentation limitation. Valgrind reached the existing50ms Luau execution
limit and is inconclusive. The old analyzer TSan binary built successfully, but
Docker's default personality restrictions prevented its runtime from starting;
no separate container or host setting change was needed for the debug-allocator reproduction.
