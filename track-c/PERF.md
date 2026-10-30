# Track C: performance log

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | Published Python reference | | 14.44 | 367 | 0 |
| 1 | Rust parallel mapped CSV parsing, dense region lookup, exact atomic name/country interning | run-628c737f586e | 0.074043 | 16.949248 | 66.648923 |
| 2 | One-byte country IDs and half-sized exact name table | run-50c660d01afb | 0.080884 | 10.768384 | 67.659457 |
| 3 | 64-byte block parser and staged inline-name lookups | run-d637090437df | 0.059995 | 13.512704 | 68.611451 |

Step 0 is the organisers' published calibration. Step 1 is the live-board
measurement for commit `b5076d5` on 2026-10-30: S=365 and D=1. The score
**66.648923** exceeds the requested Track C target of **65.403**. Local
diagnostics below are separate from the board figures.

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


## Step 2: narrower country IDs and a smaller exact name table (2026-10-30)

The frozen specification permits 240 countries plus `_UNMATCHED`, so country
IDs 0–240 fit in `u8`. Narrow the million-entry region lookup and country-code
lookup from `u16` to `u8`; retain checked region access for unknown IDs. Reduce
the name-table cap from 2^18 to 2^17 entries. This removes 6,260,456 bytes of
allocated table payload at full scale. The cap is a capacity choice, not a
name-count assumption: full name equality, bounded probing and the exact mutex
map remain intact when more distinct names or collisions reach overflow.

Seven alternating trials per variant used offline Docker release builds on
local Linux ARM64, CPUs 0–11, prewarmed input, the board sandbox, exec-to-exit
time and cgroup peak memory. Each warm summary excludes that variant's first
trial. All 21 full-seed-1 output hashes matched.

| Variant | Warm median (s) | Worst warm peak (bytes) | Local point delta vs baseline |
|---|---:|---:|---:|
| Retained baseline (`u16`, 2^18) | 0.050803877 | 18,616,320 | 0 |
| `u8`, 2^18 | 0.050094316 | 14,721,024 | +1.172 |
| `u8`, 2^17 | 0.051313210 | 9,449,472 | +2.837 |

Payload savings and peak-memory differences are different measurements; allocator
behavior also affects the peak. These local estimates use the actual log formula,
with unchanged counted source and dependencies, and do not establish Intel gains.
Select the smaller exact table for a board measurement. No PGO is enabled.

Six differential suites pass, including all 240 country IDs across every word
of the country bitset, unknown regions above the dense lookup, deliberately
colliding names, and exact overflow counting. Formatting and Clippy with warnings
denied pass. Offline ARM64 Docker and the board sandbox match the published
full-seed-1 output. Full seed 42 also matches the unchanged Python reference:
`54ed4d6bd196c17dda213cfcc7b79ec6732f06d80b1b3cb444e7468dd438e517`.
Offline AMD64 builds also match full seeds 1 and 42 under the board sandbox.
Emulated execution is used only for correctness.
Raw samples and reproducible candidates are in
`experiments/2026-10-30-score-tradeoffs/` at the repository root.

The immediately preceding unchanged-code board repeat was `run-c342f5384aee`
at `e2db239`: T=0.078485 s, M=17,092,608 bytes, S=365, D=1,
K=20.873513, machine points=65.979778; all gates passed. That repeat is not
an optimization step.

### Completed Intel board measurements

| Commit | Run | T (s) | M (bytes) | S | D | K | Machine points |
|---|---|---:|---:|---:|---:|---:|---:|
| `93b8a1b` | `run-50c660d01afb` | 0.080884 | 10,768,384 | 365 | 1 | 22.552211 | 67.659457 |
| `a7abacb` | `run-b2f445994d95` | 0.080566 | 10,825,728 | 365 | 1 | 22.572685 | 67.679161 |

All check, build, correctness, timeout and sandbox gates passed in both runs.
The first result improves on the preceding `e2db239` run by **1.679679 points**:
37.00% less peak memory compensates for 3.06% more runtime, with S and D unchanged.
It also exceeds the previous best C score, 66.946447, by 0.713010 points.
C's production source and Dockerfile are identical at `93b8a1b` and `a7abacb`;
the second result is a repeat of the same optimization, not another code gain.
These are individual public-data runs; final holdout scoring remains separate.

The first row is transcribed from the review's October 30, 14:25 PDT snapshot of
the [official board feed](https://s3.amazonaws.com/raptors.dev/speeddemon.dev/2026/board/board.json).
The second row was independently retrieved from that feed, generated at
2026-10-30 21:30:21 UTC (14:30:21 PDT), and is preserved in
[`board-a7abacb.json`](../experiments/2026-10-30-score-tradeoffs/board-a7abacb.json).


## Step 3: 64-byte structural masks and staged name lookups (2026-10-30)

Replace per-field delimiter searches with one structural pass over each block.
AVX2 identifies quotes, commas and LF; carry-less multiplication derives quote
parity. State crosses block boundaries, and a padded stack tail avoids loads
past EOF. Quote-count deltas retain exact decoded name/note byte lengths.
Known-length numeric fields use eight-digit arithmetic plus a scalar remainder.

Split name keys from inline 24-byte payload heads and membership words. Keys
include a fingerprint; full equality remains mandatory, including tails of
longer names. A busy-key publication protocol uses Release/Acquire so readers
never see partially initialized inline bytes. Two sixteen-record rings prefetch
name slots, resolve names, prefetch country words, then perform exact first-bit
updates. Both rings drain at every worker boundary. Bounded probing and the
exact overflow map preserve long names, collisions and capacity overflow.

This architecture was informed by
[Cache Me's public C source](https://github.com/manusingh090/speed-demon-2026-entry/blob/3b56ff2ff0d5efa3cf22dd8a875e24b672a2bac1/track-c/main.cpp).
No dependency, worker-count or compiler-flag changes accompany the candidate.
The inline heads add 24 bytes per common-table slot, so runtime gains must pay
for their additional memory and the larger source count.

Submitted production code at `dbcb3af`. Local runtime tests and benchmarks were
explicitly skipped at the user's request to obtain real Intel feedback first.
Formatting and `cargo check` for native and x86_64 Linux passed.

### Completed Intel measurement

`run-d637090437df`, at `dbcb3af`, passed every check/build/correctness/timeout/
sandbox gate. The October 30 22:30:43 UTC board snapshot reports:

| T (s) | M (bytes) | S | D | K | Machine points |
|---:|---:|---:|---:|---:|---:|
| 0.059995 | 13,512,704 | 545 | 1 | 23.562915 | 68.611451 |

The preceding historical best C result was `run-c3f9d13c494b` at `00f82d78`:
T=0.080296 s, M=10,719,232 bytes, S=365, D=1, points=67.758543.
The block-parser candidate is 25.28% faster, uses 26.06% more peak memory,
and adds 180 counted lines. Its net improvement is **0.852908 points**.
This is one public-data Intel run, not final holdout evidence. The raw result
is saved in [intel-dbcb3af.json](../experiments/2026-10-30-c-blocks/intel-dbcb3af.json)
from the [official board feed](https://s3.amazonaws.com/raptors.dev/speeddemon.dev/2026/board/board.json).
