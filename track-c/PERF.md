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


## Step 4: known-length short decimal conversion (2026-10-30)

Hypothesis: short numeric fields currently miss the eight-digit SWAR loop.
Convert one through eight digits using the same pair/four/eight reduction,
masking preceding bytes to leading zeroes. Read the eight bytes ending at the
field's end from the complete immutable mapping, only when end >= 8. This
stays in bounds at the first record, worker boundaries and EOF; it does not
require padding, copying, allocation or reading past the field end. Empty,
early and longer fields retain the previous general conversion. Fractional
cents use two direct digit operations; sign handling remains outside conversion.
Name storage, structural scanning, pipeline depth, workers and flags are unchanged.

The baseline repeat at `aa708b6`, `run-9870cc100939`, passed every gate:
60.416 ms, 13,426,688 bytes, S545, D1, 68.563262 points. Recent unchanged C
runs span 59.995–61.685 ms; the historical best is 68.743505 points at
60.261 ms / 12,963,840 bytes / S545. Do not treat small single-run differences
as a demonstrated gain. Raw baseline evidence (generated 23:00:56 UTC) and
complete baseline/candidate sources are under
[`experiments/2026-10-30-c-short-decimals`](../experiments/2026-10-30-c-short-decimals/).

Estimated S559 and unchanged memory imply about 59.804 ms historical-best
break-even at the historical best's memory; actual official S/M must replace
these estimates. Aim near 57 ms or below for a useful margin. No memory payload
change is intended; measured peak remains unknown.

Formatting, native and x86_64 Linux release compile checks, and the pinned
Docker build passed. A differential regression case covers numeric widths,
negative subunits, mixed terminators and EOF. Local tests and runtime benchmarks
were deliberately skipped. Official correctness and Intel performance are pending.


### Completed Intel measurement

Retain the candidate at `8dd4f2f`. `run-bee64502fbeb` passed check, build,
correctness, timeout and sandbox gates, with measured static axes:

| T (ms) | M (bytes) | S | D | Machine points |
|---:|---:|---:|---:|---:|
| 55.221 | 12,730,368 | 555 | 1 | 69.711510 |

This is **+1.148248 points** against the immediate `aa708b6` repeat and
**+0.968005 points** against C's preceding historical best. Relative to that
best, time fell 8.36%, measured memory fell 1.80%, and source grew by 10 lines.
The actual S555 is lower than the S559 estimate above. The scoring contributions
are approximately +0.948 runtime, +0.079 memory and -0.059 source points.

All four tracks passed every gate in this submission. Unchanged A/B/D ran
slower than their preceding repeats; their timings are not used to adjust C's
result. This is one promising public-data Intel measurement, not proof of a
stable improvement or final holdout correctness. Keep numeric conversion
selected and evaluate country-major membership as the next separate experiment.

Complete raw official evidence, generated 2026-10-30 23:03:59 UTC, is saved in
[intel-8dd4f2f.json](../experiments/2026-10-30-c-short-decimals/intel-8dd4f2f.json).
Results are recorded locally for the next deliberate submission, avoiding a
separate documentation-only board run.


## Step 5: country-major name membership (2026-10-30)

Hypothesis: place membership bits by country, indexed by immutable interned
name slot, to improve update locality and remove per-record first-bit counting
for common names. The second pipeline stage now prefetches and sets
`country * words_per_country + slot / 64`, bit `slot % 64`. After all workers
join and both sixteen-entry stages drain, finish sums popcounts per country.
Overflow names retain exact byte-keyed membership and worker first-bit counts.
The interner, decimal conversion, worker count and pipeline depth are unchanged.

Slot identity remains immutable; Release/Acquire head publication and full
name/tail comparisons are unchanged. Atomic OR unions concurrent updates;
joining workers makes the subsequent relaxed loads observe completed writes.
A name that exhausts bounded probing cannot later enter the main table:
probed slots never clear, busy publications are awaited, and names never move.
Thus overflow and main-table counts remain disjoint. Long names and offsets
outside the packed key range always use overflow. Empty names and bucket zero
(`_UNMATCHED`) follow the same exact membership rules as other values.

At maximum capacity and 241 countries the bitset payload falls from 4,194,304
to 3,948,544 bytes. Locality is the hypothesis; false sharing and the final
popcount pass may offset it. Baseline is `8dd4f2f`, `run-bee64502fbeb`:
55.221 ms, 12,730,368 bytes, S555, D1, 69.711510 points. Estimated S565 at
unchanged measured memory requires roughly 54.926 ms to break even; replace
estimates with official axes. This is a separate experiment from decimal parsing.
Complete baseline and candidate files are archived under
[`experiments/2026-10-30-c-country-major`](../experiments/2026-10-30-c-country-major/).

Formatted and reviewed source indexing, publication, pipeline drain and counting.
Per the user's latest instruction, no Docker build or local execution was run.
Official compilation, correctness gates and Intel score are pending.


### Completed Intel membership measurement

`run-7e3699d8615a` at `d34c3a4` passed every official gate with measured
static axes: 54.450 ms, 13,246,464 bytes, S561, D1, 69.656556 points.
Compared with decimal-only `8dd4f2f`, runtime improved by 1.40%, measured peak
increased 4.05%, and score fell **0.054954 points**. This close single-run
comparison establishes no score gain. Retain the layout provisionally for the
user-requested combined concurrency/pipeline experiment, rather than attributing
its faster runtime to a proven overall improvement. Raw evidence is saved in
[intel-d34c3a4.json](../experiments/2026-10-30-c-country-major/intel-d34c3a4.json).


## Combined remaining-plan candidate (2026-10-30)

The user requested the remaining concrete candidates together in one push,
superseding the plan's separate-submission workflow.

Cap parsing at six workers and increase each pipeline stage from 16 to
32 entries. One PIPELINE_DEPTH constant controls both arrays, initialization,
ring indexing and the two-stage drain. Total staged capacity at the full worker
limit is unchanged (12 * 16 per stage becomes 6 * 32). The hypothesis is fewer
competing hardware threads with more per-thread lookahead. This combines two
knobs at the user's request; their separate effects cannot be inferred. Keep
short decimal conversion, country-major membership and the exact interner.

Immediate official baseline at `d34c3a4`, `run-7e3699d8615a`:
54.450 ms, 13,246,464 bytes, S561, D1,
69.656556 points. Historical best: 69.711510 points
at 55.221 ms / 12,730,368 bytes / S555.
Estimated S562 (local formatted source-line delta, not official S) requires
below 54.421 ms at the recent baseline's memory, or 55.014 ms
at the historical best's memory. Actual peak memory is unmeasured; evaluate
the full measured score after the official run.

Formatted and source-reviewed. Per user instruction, no local compilation,
solver execution, tests, benchmarks or Docker runs were performed. Official
compilation, correctness and performance remain pending. Complete pre-change
and candidate files and the raw baseline board response are in
[`experiments/2026-10-30-batched-plan`](../experiments/2026-10-30-batched-plan/).


### Completed combined Intel result and selection

`run-571db117cc7f` at `2ab1dd9` passed every check, build, correctness,
timeout and sandbox gate; static axes are measured. Official response generated
2026-10-30 23:42:24 UTC:

| T (ms) | M (bytes) | S | D | Machine points |
|---:|---:|---:|---:|---:|
| 67.667 | 12,152,832 | 562 | 1 | 67.665504 |

Score change: **-1.991052** against the immediate
`d34c3a4` repeat and **-2.046006** against the preceding
historical best.

Reject six workers plus pipeline depth 32. The batch is 24.27% slower
than the country-major-only repeat despite an 8.26% lower measured peak. The
combined experiment does not isolate the worker and depth effects. Restore
production source from `8dd4f2f`: selected short decimal conversion, twelve
workers, sixteen-entry stages and the earlier per-name country membership.
The country-major-only trial had not established a full-score benefit either.

Complete candidate sources remain archived with
[intel-2ab1dd9.json](../experiments/2026-10-30-batched-plan/intel-2ab1dd9.json).
The selected combination keeps A and restores B/C/D as described. No local
builds, solver executions, tests, benchmarks or Docker runs were performed.
Restored-combination official results remain pending; public-data gate success
is not evidence of final holdout correctness.


## Batched shard ownership candidate (2026-10-30)

Replace the shared atomic name table with 32 mutex-owned shards. Every parser
collects up to 64 records per shard, acquires that shard once for the batch,
and performs ordinary key/head/membership reads and writes. Full batches flush
during parsing; all partial batches flush before the worker returns. Preserve
twelve parsing workers, the selected SIMD structural scanner and short-decimal
conversion. This is a synchronization and cache-locality redesign, rather than
the rejected country-major/six-worker candidate.

Each shard retains compact offset/length/fingerprint keys, inline 24-byte heads,
full tail comparison and four ordinary country words. Shard selection uses hash
bits 48–52, fingerprints use bits 32–47, and initial slots use low bits. Thus a
shard does not sacrifice fingerprint entropy. While holding the lock, processing
prefetches keys, heads and country words eight records ahead. There is no busy
key, unsafe head publication, per-record CAS or atomic membership operation.

A row's name routes deterministically to one shard. Exact equality still checks
all name bytes; valid CSV escaping is injective after removing outer quotes,
and both empty encodings map to an empty slice. Oversized names, mappings at
least u32::MAX bytes long, and crowded 32-probe windows use that shard's exact
byte-keyed overflow map. Mapping eligibility is shared by every occurrence:
large mappings route all names to overflow, preventing an early compact entry
and a late full-width offset from counting the same name twice. Slots never
clear or move, so a name falling through a crowded window
cannot later enter the main table and be counted twice. All accesses to each
shard's state, including prefetch references, occur under its mutex. No worker
holds more than one shard lock, preventing a lock-order cycle. Country-bit
first insertion is credited to the flushing worker's groups, and those credits
sum exactly once after all workers join. Mapping lifetime covers all pending
slices, table tails and overflow keys.

The fixed common-table payload remains 8,388,608 bytes at full capacity. Twelve
workers' maximum retained batches add 786,432 bytes (32 shards * 64 records *
32 bytes * 12 workers), plus vector/mutex metadata, replacing the small original
pipeline rings. Per-shard load imbalance may increase overflow-map allocations.
At most 64 records amortize each lock; skewed names or slow overflow comparisons
can serialize workers. These are risks to measure, not promised speedups.

Immediate baseline `d900c19`, `run-d4ed0d0a93de`: 55.667 ms, 13,316,096 bytes,
S555, D1, 69.428812 points. Historical best `8dd4f2f`, `run-bee64502fbeb`:
55.221 ms, 12,730,368 bytes, S555, D1, 69.711510 points. Estimated source S540
uses the local formatted noncomment-line delta, not the official formatter.
With an illustrative 14.2 MB measured peak, full-score break-even against the
historical best is about 53.296 ms.
Actual source count, memory and runtime remain unmeasured.

Complete baseline/candidate source files are in
[`experiments/2026-10-30-bold`](../experiments/2026-10-30-bold/). Formatting and
source review completed. Per user instruction, no local compilation, Docker,
solver execution, tests or runtime benchmarks were performed. Official Intel
build/correctness/performance gates remain pending.


## Dedicated name owners and bounded producer pipeline (2026-10-30)

Replace mutex-shard handoff with two concurrent stages: six CSV producers and
six dedicated name owners on the twelve scoring threads. Each producer retains
numeric and decoded-length aggregates, hashes each name and routes descriptors
into 32 shard-specific batches of 512 records. Shard `s` belongs to owner
`s % owners`; within that owner its table index is `s / owners`. The table never
moves between threads. Standard-library bounded channels transfer owned vectors
without copying their name payloads. Each channel admits at most two waiting
batches. Owners retain the selected exact compact-key/head/membership tables,
32-probe bound and byte-keyed overflow maps, with the same total table capacity.

Distinct credits now belong to owner-local country counters and are reduced
separately from numeric aggregates. Identical encoded names deterministically
select the same shard; full equality still resolves every fingerprint/hash
collision. Canonical CSV escaping preserves decoded equality and normalizes
both empty forms. Table overflow, arbitrarily long names and mappings outside
the compact range retain the preceding exact fallbacks.

All owners start before producers. Each producer owns a complete sender set;
its final partial batches are sent before that set drops. The coordinator drops
its original senders before joining producers, and joins owners only after
producers. Receivers drain queued batches before reporting disconnection. There
is no channel cycle or consumer waiting for a producer result. With one available
CPU a single producer/owner pair still progresses through OS scheduling. At full
scale twelve active workers includes both stages, not twelve parsers plus owners.

On x86_64 a descriptor is 32 bytes. Six producers' 32 batches of capacity 512
occupy 3,145,728 payload bytes. Twelve queued batches add 196,608 bytes; six
currently processed batches add 98,304; up to six sends can hold another 98,304
while blocked. This is a bounded descriptor payload of 3,538,944 bytes, excluding
metadata, allocator retention and thread stacks. It replaces 786,432 bytes of
pending records in the preceding design. The common-table payload remains
8,388,608 bytes; exact overflow allocations remain data-dependent. Batches are
allocated by producers and released by owners, so cross-thread allocator cost
and retained allocator capacity are risks alongside reducing parser workers.

Baseline: `2cea13f`, `run-8184b81202bd`, all gates passed,
52.975 ms, 14,024,704 bytes, S540, D1, 69.831060 points. Historical best from the
same preceding C implementation: `92dcd11`, `run-eeb4537bf4a6`, 52.836 ms,
13,201,408 bytes, S540, 70.122320 points. The formatted noncomment source delta is
+19 lines, estimating S559 before the official static-axis pass. At unchanged
historical-best memory this needs about 52.3 ms to break even; greater peak memory
requires a further time reduction. No improvement is claimed before measurement.

Complete pre-change and candidate source files are
`experiments/2026-10-30-next-shot/c-baseline.rs` and `c-candidate.rs`, outside the
production track. Formatting and whitespace checks passed. Source review covered
owner/shard indexing, exact identity and overflow, distinct-credit reduction,
all partial-batch paths, bounded queues, sender lifetimes, normal disconnection,
header-only and single-CPU behavior, and mapping lifetime. No Docker, local
compilation, solver execution, tests or benchmarks were run. Official Intel
build, correctness and performance remain pending; public-data success would
not establish final holdout correctness.


### Completed Intel result and selection at `196b0c0`

`run-b030da9573bb` passed check, build, correctness, timeout and sandbox,
with measured static axes: **59.713 ms**, **16,965,632 peak bytes**,
**S559**, D1, **67.591713 machine points**.
Delta versus preceding historical best: **-2.530607 points**.

Restore the preceding batched-mutex source (`c-reference.rs`, source from
`2cea13f`, identical to the historical best). The new pipeline was 12.7% slower
than the immediately preceding run, used 21.0% more peak memory, and lost
2.239347 points. Exclusive ownership did not repay queue and producer costs
in this run; no phase attribution is claimed.

Raw official response generated 2026-10-31T04:23:43Z and complete candidate
source are preserved under
[`experiments/2026-10-30-next-shot`](../experiments/2026-10-30-next-shot/).
No local compilation, execution, tests or Docker were used. Public-data gate
success does not establish correctness on the final holdout seed. The selected
combination retains A, restores B/C, and keeps D; its next board run is a repeat
of these selected implementations, not another algorithm experiment.
