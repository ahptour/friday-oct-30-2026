# Track B: performance log

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | Published Python reference | | 56.35 | 2,200 | 0 |
| 1 | Parallel query-aware Rust index, adaptive bitsets, token Aho–Corasick phrase matching | run-4128726e1d3c | 0.721288 | 203.89 | 51.629 |
| 2 | Streaming 64-document Boolean masks, phrase anchors, SSE2 tokenizer | run-3da917fe8828 | 0.455149 | 69.505024 | 61.516786 |

## Step 1 (2026-10-30)

The reference indexes every corpus term and repeatedly retokenizes phrase
candidates. This version indexes only Boolean query terms and matches all phrase
queries during the single corpus pass. Parallel document chunks use local dense
bitsets, compact automaton nodes, and inline normalized token keys. Boolean
operations then run in parallel over a hybrid sparse/dense index. No external
runtime dependencies or precomputed answers are used.

The full public seed-1 output matches
`f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`.
Five differential tests pass against the unchanged Python reference.

Local diagnostic iterations on the Apple Silicon host reduced warm native
runtime from approximately 500 ms to 258 ms. A transition hash initially failed
to mix high state bits into low bucket bits, causing collisions among common
terms; fixing it made a substantial improvement. Adaptive bitsets and compact
nodes then reduced allocation work. Native physical footprint is about 189 MB;
this is not the competition's cgroup memory measurement, and mmap input pages
are accounted differently locally.

A separate rarest-posting phrase experiment was slower locally, about 490 ms,
because common repeated-word phrases require many candidate scans. Local timing
is only diagnostic. The first board run was correct but scored 51.629, below
the requested >=64.636 target. It measured 578 source lines and one dependency.
The next iteration is investigating streaming evaluation with a small index
for frequently matched Boolean queries, to avoid allocating postings for every
query term and repeatedly updating the same common-query summaries.

## Step 2 (2026-10-30)

Document-at-a-time Boolean streaming removed large posting allocations but
performed about 100 million candidate-clause checks and 88 million individual
match updates on the public input. Evaluating 64 documents in each machine word
removes that repeated work without building a full index. Selective phrase
anchors and exact token-ID comparisons replace the automaton. Touched masks are
cleared at block boundaries, so absent terms cannot reuse stale presence bits.

Warm native diagnostics are approximately 185–192 ms, versus 258 ms for step 1.
The block prototype's native physical footprint was approximately 74 MB before
removing presence stamps. An independent x86-64 Docker experiment with the SSE2
tokenizer passed the full hash and all six differential tests; translated local
runs improved by roughly 10–15%, which is not a native scoring-machine result.
The full public hash remains unchanged. The board repeat at `e2db239` passed
all gates: run `run-3da917fe8828`, T=0.455149 s, M=69,505,024 bytes,
S=541, D=1, K=16.995569 and 61.516786 machine points. The best recorded
repeat of this same B implementation is `run-4a9c515be486` at `3b74c52`,
T=0.437009 s, M=70,397,952 bytes, S=541, D=1, K=17.300496 and
61.902928 points. B source did not change between these commits; this
variation is not an A/D PGO benefit. The original >=64.636 target remains unmet.


## Worker-count experiment (2026-10-30; not selected)

Seven alternating trials per variant used offline Docker release builds on local
Linux ARM64, CPUs 0–11, the board sandbox, prewarmed input, process exec-to-exit
time and cgroup `memory.peak`. Exclude the first trial for each warm summary;
report the maximum warm peak, not average memory. All 28 full-seed-1 output
hashes matched. No source/dependency penalty changes apply to these constants.

| Workers | Warm median (s) | Worst warm peak (bytes) | Local point delta vs 12 |
|---|---:|---:|---:|
| 12 | 0.239745368 | 69,066,752 | 0 |
| 8 | 0.278022098 | 58,085,376 | -0.856 |
| 6 | 0.354494351 | 50,802,688 | -2.913 |
| 4 | 0.498377121 | 45,113,344 | -6.096 |

The retained raw series also includes the twelve-worker first trial's
**286,601,216-byte** peak, versus **69,066,752 bytes** for the worst retained
warm trial. Every variant's first trial is excluded by the same warm-summary
rule. Prewarming once per track did not ensure comparable
memory accounting throughout the series. Input-page charging or VM cache
behavior is a plausible explanation, not a confirmed diagnosis: these samples
contain no `memory.stat` breakdown. Do not use this series to resolve close
memory or score differences. The later scratch-array comparison below prewarms
outside the measured cgroup immediately before every run and retains both full
sample series, including anomalous rows.

All measurements remain above the reference-memory/50 cap. Savings did not
repay the extra time in the actual scoring formula; retain twelve workers.
Apple Silicon has different cores from the scoring server, so this rejects a
local selection, not the possibility of an Intel-specific win. An Intel worker
sweep remains useful; the scratch-array experiment follows below.
Six unchanged B differential suites pass. Reproduction and raw samples are in
`experiments/2026-10-30-score-tradeoffs/` at the repository root.


## Step 3: scratch arrays sized by query category (2026-10-30)

Retain twelve workers. Phrase stamps now contain only phrase queries; Boolean
masks contain only Boolean queries. Each clause carries a dense Boolean mask ID
alongside its original result ID, so interleaved queries and multiple clauses
still produce exactly one ordered answer per input query. Dense phrase IDs use
`usize::MAX` for Boolean queries, reducing `Query` by one word compared with
`Option<usize>`. Full token equality and all matching semantics remain intact.

The original twelve-worker payload saving from narrower arrays is 3.312 MB for
the published query mix, plus smaller shared query records, minus additional
clause mask IDs. Measure total peak rather than reporting this payload as peak.

An initial nine-trial three-way series prewarmed once before the series. One
phrase-only run charged 323,395,584 bytes, far beyond the solver's scratch
allocation; this is consistent with input pages being faulted into its cgroup.
Do not discard that row as a favorable outlier. Keep all initial samples and
repeat the comparison with input prewarming immediately before **every** measured
run, matching the official procedure more closely. No competing builds or data
generation ran during either timed series.

| Variant | Warm median (s) | Worst warm peak (bytes) |
|---|---:|---:|
| Baseline, twelve workers | 0.253522720 | 69,140,480 |
| Category-sized arrays, twelve workers | 0.256140674 | 65,593,344 |

These nine alternating trials per variant are Linux ARM64 diagnostics; the
first trial is excluded from each warm summary and every hash matches full
seed 1. The time/memory delta is +0.117 local machine points before source cost.
Canonical local Rust formatting adds seven nonblank, noncomment lines; using
541 -> approximately 548 scored lines leaves roughly +0.075 points. This is an
estimate, not official S or an Intel score. Retain the clear, smaller exact
state layout, but do not describe the small estimated score gain as established
on Intel. No worker-count change or PGO is included.

Seven differential suites include phrase-only, Boolean-only, interleaved and
reversed query orders, repeated phrase hits, unsatisfiable clauses, multiple
Boolean clauses and transitions across 64-document blocks. Formatting, Clippy
with warnings denied, and the offline ARM64 sandbox/full public hash pass.
Reproduction and both complete sample series are in
`experiments/2026-10-30-score-tradeoffs/` at the repository root. The completed
Intel result is recorded below.


The unchanged Python oracle also matches sample seed 42
(`8afc643bfca75356178541927d54654067b8f4c288ec9c16862e20d8d459cb63`)
and full seed 42
(`f853a72cd18c2535183170a22482f363a251ac88fec1ed419791e1572dbfdffd`).
The offline AMD64 build matches full seeds 1 and 42 under the board sandbox;
emulated runs validate correctness and are not used for timing decisions.

### Completed Intel board measurements; score benefit unproven

| Metric | Previous B, `93b8a1b` | Category-sized arrays, `a7abacb` |
|---|---:|---:|
| T (s) | 0.451293 | 0.460989 |
| M (bytes) | 70,459,392 | 67,555,328 |
| S | 541 | 548 |
| D | 1 | 1 |
| Machine points | 61.549934 | 61.460053 |

Candidate run `run-5e2e58cfab73` has K=16.951224. All check, build, correctness,
timeout and sandbox gates passed. Compared with the immediately preceding
`93b8a1b` measurement, peak memory is **4.12% lower**, runtime is **2.15% higher**,
and the score is **0.089881 points lower**. The older `e2db239` comparison is
also negative (-0.056733 points), but is not the immediately preceding result.

With the memory cap inactive and D unchanged, the score change decomposes as
`25*log10(T_old/T_new) + 10*log10(M_old/M_new) + 7.5*log10(S_old/S_new)`:
memory contributes +0.182793 points, runtime -0.230799, and source -0.041875.
At the candidate's measured memory and source count, its break-even runtime
against `93b8a1b` is **457.188527 ms**; its 460.989 ms run is about 3.8 ms slower.

This single public-data comparison records a smaller peak, but establishes
neither an Intel score gain nor a repeatable regression. Unchanged A and D also
ran slower in this batch; that is not a correction factor for B. Keep B as a
candidate pending repeated Intel comparison with the previous implementation.

Source: the [official board feed](https://s3.amazonaws.com/raptors.dev/speeddemon.dev/2026/board/board.json),
generated at 2026-10-30 21:30:21 UTC (14:30:21 PDT); our team's entry is preserved
in [`board-a7abacb.json`](../experiments/2026-10-30-score-tradeoffs/board-a7abacb.json).
The `93b8a1b` row is transcribed from the review's October 30, 14:29 PDT board
snapshot; it is not contained in the saved `a7abacb` feed entry.

An unchanged-code candidate repeat has also completed at `32c9946`:
`run-06a4bd447b20`, T=0.461655 s, M=68,030,464 bytes, S=548, D=1,
K=16.915265 and **61.413940 machine points**. All five gates passed.
B's source, manifest, lockfile and Dockerfile are identical to `a7abacb`.
This is a second candidate measurement, not a fresh paired baseline comparison.
Its official feed entry, generated at 2026-10-30 21:35:19 UTC (14:35:19 PDT), is
preserved in [`board-32c9946.json`](../experiments/2026-10-30-score-tradeoffs/board-32c9946.json).

For the repeat, use isolated copies of `track-b` at `93b8a1b` and `a7abacb`,
both with twelve workers and no PGO, on the same idle Intel host. Alternate
baseline/candidate order across multiple paired batches. Use the official
harness's prewarming, parent-cgroup memory measurement and five-run aggregation;
retain every run, all gates and hashes, and both official source counts.
Compare score deltas per pair before making a final selection. ARM64 timings
and emulated AMD64 runs cannot complete that comparison.


## Combined remaining-plan candidate (2026-10-30)

The user requested the remaining concrete candidates together in one push,
superseding the plan's separate-submission workflow.

Inline eight u32 term IDs and seven operator bytes per query. Retain
explicit length and dense phrase IDs, preserving mixed output order. Store each
Boolean clause's remaining tests in six u32 slots with the low bit indicating
positive membership; query, Boolean mask and anchor IDs use u32. Query and term
ID conversions are checked. The frozen maximum of 50,000 queries with eight
terms bounds unique query terms by 400,000, safely below the 31-bit clause ID
payload and the u32::MAX unknown-token sentinel. Document token buffers now
also use u32. Boolean expansion, contradiction removal and rarest anchors are
unchanged; operators still apply left to right. Phrase comparisons retain every
term and repeated occurrence; per-document stamps prevent duplicate hits.
Keep twelve workers and the current category-sized scratch arrays. This combined
candidate does not independently resolve that older scratch-array experiment.
The larger hybrid-postings B rewrite remains a conditional architectural option,
not part of the concrete bounded-query candidate.

Immediate official baseline at `d34c3a4`, `run-16eafca1daa5`:
463.679 ms, 67,588,096 bytes, S548, D1,
61.394775 points. Historical best: 61.902928 points
at 437.009 ms / 70,397,952 bytes / S541.
Estimated S566 (local formatted source-line delta, not official S) requires
below 459.205 ms at the recent baseline's memory, or 431.126 ms
at the historical best's memory. Actual peak memory is unmeasured; evaluate
the full measured score after the official run.

Formatted and source-reviewed. Per user instruction, no local compilation,
solver execution, tests, benchmarks or Docker runs were performed. Official
compilation, correctness and performance remain pending. Complete pre-change
and candidate files and the raw baseline board response are in
[`experiments/2026-10-30-batched-plan`](../experiments/2026-10-30-batched-plan/).
