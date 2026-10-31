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


### Completed combined Intel result and selection

`run-a89a3b3bae3e` at `2ab1dd9` passed every check, build, correctness,
timeout and sandbox gate; static axes are measured. Official response generated
2026-10-30 23:42:24 UTC:

| T (ms) | M (bytes) | S | D | Machine points |
|---:|---:|---:|---:|---:|
| 587.515 | 58,904,576 | 566 | 1 | 59.316680 |

Score change: **-2.078095** against the immediate
`d34c3a4` repeat and **-2.586248** against the preceding
historical best.

Reject the compact-query/u32-token batch. Its 12.85% lower memory than
the immediate repeat does not compensate for 26.71% more runtime and higher S.
Restore only B production source from `d34c3a4` (twelve workers, Vec-based queries
and category-sized scratch). The bundled changes do not identify whether clause
encoding, token width or another layout detail caused the regression. No new
claim about the separate category-sized scratch experiment is made.

Complete candidate sources remain archived with
[intel-2ab1dd9.json](../experiments/2026-10-30-batched-plan/intel-2ab1dd9.json).
The selected combination keeps A and restores B/C/D as described. No local
builds, solver executions, tests, benchmarks or Docker runs were performed.
Restored-combination official results remain pending; public-data gate success
is not evidence of final holdout correctness.


## Bold candidate: local adaptive inverted indexes (2026-10-30)

Replace the streaming query checks at every 64-document block with a build then
query pipeline inside each of twelve document workers. Each worker records text
ranges and builds postings only for query vocabulary. A posting begins as an
ascending, deduplicated vector of local document IDs; when that payload exceeds
a local bitmap it promotes to the bitmap. Both forms share a Vec<u32> allocation
and carry an exact document frequency. The cutoff is derived from actual chunk
document count rather than a sampled-frequency correctness assumption.

Boolean expressions retain the existing left-to-right DNF expansion and exact
contradiction removal. Each worker now selects the actually rarest positive
posting for every clause. Sparse anchors enumerate candidate documents and test
other terms by binary search or bitmap membership. Dense anchors combine entire
64-document words, with monotone cursors through negative sparse postings. A
reusable touched-word union combines all clauses without double counting, then
computes counts, ID sums and the first ten results in ascending order.

Phrase queries enumerate their actually rarest posting and filter by all term
memberships before reading source text. Exact ASCII tokenization and KMP verify
the ordered phrase, including repeated terms and overlapping prefixes. No full
positional index is retained. Each posting contains a document at most once,
so repeated occurrences cannot duplicate a phrase result. Text ranges never
cross document boundaries. Chunk results are merged in file order; the frozen
contract's docs.txt section 2.1 guarantees dense ascending IDs, making local ID
plus chunk base exact; this is a contract property, not a sampled distribution.

This is a new implementation using the existing tokenizer, clause semantics and
summary arithmetic; no external code was adapted and no dependencies were added.
The hypothesis is fewer repeated query/clause probes during tokenization and
actual-frequency anchors. The risks are allocation overhead for sparse postings,
retokenization of phrase candidates and higher peak memory from retaining indexes
until each worker finishes its query phase. Earlier hybrid-index approaches had
losing timings; this candidate differs in using independent adaptive local indexes,
no positional/automaton construction and word-level dense-clause evaluation.

Immediate official baseline `d900c19`, `run-d615b350ea77`:
478.898 ms, 67,141,632 bytes, S548, D1, 61.072919 points. Historical best:
437.009 ms, 70,397,952 bytes, S541, 61.902928 points. Runtime and actual peak
are unmeasured; this is a deliberately substantial Intel experiment, not a
claimed improvement. Complete baseline and candidate source files are preserved
at `experiments/2026-10-30-bold/b-baseline.rs` and `b-candidate.rs`.

Rust formatting and whitespace/source review passed. Source review covers
sparse-to-dense promotion, duplicate stamps, padding bits, monotone sparse-word
cursors, contradictory clauses, KMP failure links, empty inputs and sorted result
merging. Per user instruction, no local compilation, solver execution, tests or
Docker runs were performed. Official build/correctness/performance are pending.

The formatted nonblank/noncomment Rust delta is +16 lines, suggesting S564
against official baseline S548; this remains an estimate until the official
static-axis pass. At unchanged baseline memory that requires approximately
474.8 ms versus the latest repeat, or 431.6 ms at historical-best memory, to
break even. Actual memory is a material unknown because local inverted indexes
survive through query evaluation.


## Compiled phrase machine experiment (2026-10-30)

Replace the adaptive local inverted indexes and per-phrase KMP rescans with one
shared Aho-Corasick automaton over exact query token IDs, fed by the same corpus
pass as streaming 64-document Boolean evaluation. Boolean compilation, sampled
anchor scheduling, tokenization and summary arithmetic reuse the preceding
streaming implementation. Root transitions are direct; nonroot arcs use exact
integer keys. Failure/terminal links share suffix work, duplicate query lines
keep separate emissions, and terminal document stamps avoid repeated counting
and redundant suffix walks. Unknown tokens and document boundaries reset state;
separator bytes preserve token adjacency. No corpus index or token sequence is
retained. Actual input queries compile at runtime; no answers are precomputed.

Immediate official baseline at `b90a4a4`, `run-6f1d594cc1cc`: 505.661 ms,
177,463,296 bytes, S564, D1, 56.167602 points. Historical best is
`run-4a9c515be486`: 437.009 ms, 70,397,952 bytes, S541, 61.902928 points.

Formatted nonblank/noncomment source delta estimates S642, pending the
official canonical formatter/static-axis pass. At unchanged historical-best
memory, break-even against that best is approximately 415.1 ms. Actual
runtime and peak are unknown. Compare the complete measured score, including
source and memory, before selecting this candidate.

Complete baseline/candidate source and raw baseline evidence are preserved in
[`experiments/2026-10-30-query-machine`](../experiments/2026-10-30-query-machine/).
Source review and Rust formatting/whitespace checks passed. No Docker, local
compilation, solution execution, tests or benchmarks were performed, following
the current repository instruction. Official Intel build, correctness and
performance are pending; public-data correctness will not prove holdout safety.


### Completed Intel result for `2cea13f`

`run-26af339e8a1a` passed every official gate with measured static axes:
510.215 ms, 70,705,152 peak bytes, S642, D1,
59.644913 machine points. Historical-best delta: -2.258015 points.
Raw evidence is archived in
[`intel-2cea13f.json`](../experiments/2026-10-30-query-machine/intel-2cea13f.json).
The complete candidate remains in that experiment directory. The next shot
uses the earlier better-scoring streaming/search structures as its base; no
speedup over the historical best is claimed for this attempt. All verification
was on the official Intel board; no local execution or tests were performed.


## Compact lexical scan experiment (2026-10-30)

The previous runtime phrase automaton passed all official gates but reached
510.215 ms / 70,705,152 bytes / S642 / 59.644913 points at `2cea13f`
(`run-26af339e8a1a`). That is below the historical streaming best of
437.009 ms / 70,397,952 bytes / S541 / 61.902928 points
(`run-4a9c515be486`). Return to the earlier 64-document Boolean and rare-anchor
phrase execution plan, then replace its common lexical path rather than
building another corpus index or phrase automaton.

Freeze runtime query vocabulary into a compact table with eight-byte
fingerprint/ID slots, separate exact keys and lengths, and at most 50% occupancy.
The query-building hash map is dropped before parsing workers start. Corpus
lookup loads the lowercase first word and, for long tokens, last word directly
from bounded input bytes. Fingerprints reject mismatches cheaply; acceptance
compares token length and every word, including interiors of 17–32-byte tokens.
Collisions cannot produce false matches. Unknown terms retain a sentinel token
position during phrase matching. No dataset-specific words or IDs are embedded.

A worker caches 64-byte alphanumeric/newline masks from four bounded SSE2 loads.
Bit scans enumerate tokens without repeatedly classifying 16 bytes per short
word. Token runs span mask boundaries; newline masks stop at the exact document
boundary. The final partial block has a scalar path, as do other architectures.
ASCII lowercase is performed on token words with OR 0x20; digits are unchanged,
all high bytes are separators, and masked suffix bytes never enter equality.
There is no per-token padded buffer write or generic full-key hash on the corpus
path. The table, scanner and exact token comparisons are new; Boolean
associativity, clause deduplication, phrase anchors, output arithmetic and worker
merging reuse the preceding streaming implementation.

This combines a return to the better historical execution plan with a new
lexical representation and block scanner. A measured gain would not isolate
those contributions. The expected benefit is fewer repeated byte classifications,
key writes and dictionary cache misses. Costs include a second query-time table
construction, linear-probe chains and extra source. Neither throughput nor peak
memory has been established locally or on the official board yet.

Complete baseline/candidate source is archived in
`experiments/2026-10-30-next-shot/` outside the production track folder. Source
review covered complete exact comparison for lengths 1–32, same-head/tail
collisions, digit tokens, uppercase, UTF-8 separators, absent terms, empty query
vocabulary, tokens and line ends at every block boundary, partial final blocks,
partial worker blocks, phrase sentinel positions, duplicate matches and stable
output ordering. Formatting and whitespace checks passed. Per instruction no
Docker, compilation, solver execution, local tests or benchmarks were run.
Official build/correctness/performance are pending; source review does not
establish holdout correctness.

The formatted nonblank/noncomment source delta is +25 lines versus `2cea13f`,
estimating S667 until the official static-axis pass. At the historical best's
unchanged memory, beating its full score requires approximately 410.4 ms.
This is the measurement threshold, not a predicted runtime.


### Completed Intel result and selection at `196b0c0`

`run-3aed697fe27b` passed check, build, correctness, timeout and sandbox,
with measured static axes: **502.058 ms**, **67,682,304 peak bytes**,
**S667**, D1, **59.885225 machine points**.
Delta versus preceding historical best: **-2.017703 points**.

Restore the earlier streaming source from `3b74c52` (`b-reference.rs`). The candidate
gained 0.240312 points over `2cea13f` but remains 2.017703 points below the
historical best. Its smaller peak did not pay for extra source and runtime.

Raw official response generated 2026-10-31T04:23:43Z and complete candidate
source are preserved under
[`experiments/2026-10-30-next-shot`](../experiments/2026-10-30-next-shot/).
No local compilation, execution, tests or Docker were used. Public-data gate
success does not establish correctness on the final holdout seed. The selected
combination retains A, restores B/C, and keeps D; its next board run is a repeat
of these selected implementations, not another algorithm experiment.


## Native short-key dictionary selection (2026-10-31; Intel pending)

Profile before changing the execution plan. Two archived diagnostic executions
per full seed put Boolean block flushes at roughly 20% of each worker's elapsed
parse time. The remainder includes tokenization, dictionary lookup, phrase work
and bookkeeping; this does not isolate dictionary time. Keep streaming Boolean
masks and phrase anchors, and reduce exact dictionary key width instead.

Build two dictionaries directly while reading queries: zero-padded 16-byte keys
for tokens up to 16 bytes and full zero-padded 32-byte keys for longer tokens.
Both assign IDs from their combined length. Corpus lookups use the actual token
length. No prefix accepts a long token, no cached answer is used, and ASCII-only
normalization, unknown-token phrase positions and complete long-token equality
are preserved. Direct construction avoids retaining an initial 32-byte table.

An initial convert-after-building variant led by only 0.8% / 2.1% on seeds 1 / 42
and did not lower measured footprint; reject it. An isolated 8-byte-key variant
was slower than the direct 16-byte candidate on both seeds despite a smaller
footprint; keep it archived and prefer the repeated 16-byte candidate.

| Batch | Full seed | Baseline median ms | Direct 16-byte median ms | Baseline max footprint bytes | Candidate max footprint bytes |
|---|---:|---:|---:|---:|---:|
| Screen, 10 timings each | 1 | 171.297 | 165.386 | 72,942,192 | 69,517,936 |
| Screen, 10 timings each | 42 | 169.304 | 165.792 | 72,974,960 | 69,927,536 |
| Independent repeat, 12 timings each | 1 | 169.935 | 165.673 | 72,385,112 | 70,550,128 |
| Independent repeat, 12 timings each | 42 | 173.914 | 167.021 | 72,106,608 | 69,534,320 |

These are Apple M4 Max native exec-to-exit measurements, two discarded warmups
and randomized serial order; the repeat reverses the variant list. Three
separate `/usr/bin/time -l` runs per variant/seed record both maximum RSS and
peak footprint. The table reports Mac peak footprint, not Linux cgroup
memory.peak. The candidate was faster in 19/20 screen pairs and 20/24 repeat
pairs. Per-pair median time/source point estimates (assuming S566) are +0.225 /
+0.220 for the screen and +0.162 / +0.022 for the repeat, before any memory
benefit. The repeat's seed-42 timing spread is material; do not infer a stable
4% Intel gain from the median ratio alone.

Official baseline source count is S541. Canonical local nonblank/noncomment
Rust grows by 25 lines, estimating S566, with D1 unchanged. At equal measured
memory, the score break-even is T_new/T_old < (541/566)^0.3 = 0.986539,
requiring 1.35% less runtime. This local result is sufficient to select the
candidate for an Intel trial, not to claim official K or final holdout benefit.
Production now matches the complete archived `b-shortkeys-direct.rs`.

Verification: all eight existing Python-reference differential suites passed
against the candidate binary, as did two additional boundary suites covering
8/16/17/32-byte widths, common prefixes, all phrase positions, short-only,
long-only and mixed parallel inputs. Deliberately routing long lookups to the
16-byte prefix table made both boundary suites fail. Full-sized output bytes
matched previously verified reference outputs for seeds 1 and 42 on every
benchmark execution; these were not fresh full-reference runs. Formatting,
Clippy with warnings denied and x86_64 Linux Haswell code generation passed.
After selection, all eight existing suites passed against the production Cargo
release binary too. No Docker or official submission was run in this session.

Complete sources, mutation output, diagnostics and every sample are in
[`2026-10-31-native-first`](../experiments/2026-10-31-native-first/), especially
`b-shortkeys-direct-screen.json` and `b-shortkeys-direct-repeat.json`. The native
runner now binds measurements to source, executable and input SHA-256 hashes,
compiler/command metadata and existing reference-output provenance. Current
source has no official run ID yet. The last official baseline at `1bc5fb6` was
`run-c0aa86c4d47d`: 446.538 ms, 70,979,584 bytes, S541, D1, 61.632993 points.
Its complete official feed snapshot is preserved as `intel-1bc5fb6.json`.


## Inline Boolean clauses from the repository review (2026-10-31; Intel pending)

Refresh the short-key result first: the feed generated 2026-10-31T05:12:30Z
records `run-d5c09f62298a` at `1b85287`, all gates passed, **441.470 ms**,
**67,166,208 bytes**, **S566**, D1, **61.849606 machine points**. This exceeds
the preceding latest B61.632993 at `1bc5fb6`, but remains below historical best
B61.902928. That official run measures short keys, not the following change.

Inspired by Mnemonist's contiguous indexed structures, replace each Boolean
clause's separately allocated operand vector with five inline uint32 entries,
encoding term ID and polarity together. Six-term Boolean grammar bounds the
remaining operands after removing the anchor; the full query vocabulary fits
this encoding. Keep existing anchors, ordering, contradictions, Boolean masks,
phrases and summaries. No dependency is added.

| Batch | Full seed | Baseline median ms | Candidate median ms | Baseline max Mac footprint bytes | Candidate max Mac footprint bytes |
|---|---:|---:|---:|---:|---:|
| Screen, 10 timings each | 1 | 159.986 | 156.343 | 71,189,104 | 68,321,880 |
| Screen, 10 timings each | 42 | 159.327 | 155.605 | 70,173,272 | 68,059,736 |
| Independent repeat, 12 timings each | 1 | 159.934 | 155.570 | 69,714,544 | 67,895,920 |
| Independent repeat, 12 timings each | 42 | 159.945 | 155.525 | 69,272,176 | 68,321,904 |

The candidate won 41/44 paired trials, including all 24 repeat pairs. Repeat
paired median runtime ratios were 0.97246 / 0.97303. Separate instrumented
sources support the intended flush-phase improvement; no hardware cache profile
was collected. An isolated already-matched-OR mask was flat/slower and is
rejected. Narrowing only the inline length to u8 gave a marginal incremental
result in one batch; retain that source without selecting it.

These are serial randomized native Apple M4 Max exec-to-exit measurements,
including setup/output, two excluded warmups and three separate memory runs
per variant/seed. Mac footprint and RSS are not official cgroup memory. Estimated
S566 -> S572 requires 0.316% less runtime at unchanged official memory. Actual
candidate E/K/points are unavailable. If native repeat ratios transferred to
Intel at equal official memory and estimated source, the hypothetical gain
would be about 0.266 / 0.270 machine points. B would still be fourth with equal
judge scores, so no overall-score gain is established.

Eight existing differential methods and two additional boundary methods passed
against both candidate and production release executables. An isolated polarity
mutation failed the exhaustive six-term operator-order method (one output
failure, no error/skip). Every benchmark and diagnostic compared full output
bytes with saved independently verified reference bytes for seeds 1 and 42;
these are not fresh full-size reference runs. Production full outputs matched
too. Formatting, Clippy with warnings denied and Haswell code generation passed.
Public/native correctness is not a holdout proof.

Production matches `b-inline-clauses.rs`; no commit, push or Intel submission was
made. Full sources, every raw sample, hashes/compiler/input identities, mutation
failure, score budgets and explicit judge-score scenarios are in the
[experiment report](../experiments/2026-10-31-native-first/REPO-TRICKS.md),
`b-repo-tricks-screen.json`, `b-inline-clauses-repeat.json` and
`repo-tricks-analysis.json`. The native runner now checks exact output bytes
in addition to recording hashes, outside the timed interval.
