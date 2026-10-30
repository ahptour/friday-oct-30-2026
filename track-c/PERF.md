# Track C: performance log

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | Published Python reference | | 14.44 | 367 | 0 |
| 1 | Rust parallel mapped CSV parsing, dense region lookup, exact atomic name/country interning | Pending | | | |

Step 0 is the organisers' published calibration. Step 1 awaits a live-board
measurement; local diagnostics below are not board figures. The requested
Track C score target is at least 65.403.

## Step 1: exact names without per-record allocation (2026-10-30)

First tried a parallel parser that retained packed name offsets per country,
then built country name sets. It produced the correct full output but took
roughly 70 ms locally with about 69 MB native physical footprint. Replacing
those vectors with sharded name-to-country-bitset maps reduced that to about
60 ms and 22 MB, but mutex contention remained visible.

The retained implementation uses a bounded-probe atomic table. Each name is
interned once, and its country membership occupies four atomic words. The first
thread to set a country bit records one distinct name in its local aggregate.
Every hash collision is resolved by full name-byte comparison; overflow goes
to an exact mutex-protected map. Scalar aggregation remains per worker. Parsing
uses eight-byte delimiter tests and preserves CSV escaping, BOM and byte-count
semantics without allocating decoded text.

Six sequential warm native runs on the Apple Silicon development host took
42.14, 40.62, 40.15, 38.78, 40.35 and 40.51 ms; median **40.43 ms**.
A separate `/usr/bin/time -l` run reported about **16.7 MB physical footprint**.
This excludes mapped input in that native footprint counter and is not the
competition's cgroup `memory.peak`.

Verification:

- Five differential tests against the unchanged Python reference pass, including
  more than 32 colliding sampled hashes to exercise overflow and concurrent
  repeated names spread across chunks.
- Full seed 1: `d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536`.
- Sample seed 42 reference match: `0b2db0295de7e26e2b9547f1ce823087b8b6a1988ec5353e2ad44b6416c3f690`.
- `cargo fmt --check` and Clippy with warnings denied pass.
- Offline ARM64 Docker build and sandboxed full-data `check.py` pass; 0.24 s
  including container startup. Runtime image is approximately 30 MB locally.

No external runtime dependencies or precomputed answers are used. The final
holdout seed is separate from these correctness checks.
