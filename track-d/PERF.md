# Track D: performance log

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | Organisers' unchanged Python reference | | 72.69 | 893 | 0 |
| 1 | Parallel parsing, forward/reverse CSR, sparse bidirectional Dijkstra, balanced frontiers | run-c88839436741 | 0.459081 | 170.385408 | 56.211682 |
| 2 | Twelve query workers | run-43460285dd9e | 0.354099 | 171.261952 | 59.008513 |
| 3 | Compact queue entries with full-width fallback | run-b8e527b07f2c | 0.326074 | 171.425792 | 59.490467 |

Steps 4 and 5 have board measurements. Step 5 was reverted after losing points.
Step 6 evaluates graph reduction and mapped text offsets; neither is retained.

## Rust implementation, 2026-10-30

The 59.238-point target is achieved: commit `7e74e404` scores 59.490467 on the
live board with all gates passing, S=305 and D=1. The first Rust
run passed all gates but scored 56.211682 at commit `fbd3f2b`, with S=269 and D=1.
It did not meet the target; local measurements below are separate diagnostics.

The initial Rust implementation used parallel edge parsing, CSR arrays and six
bidirectional search workers with dense distance arrays. One native full-seed-1
run took about 600 ms: parsing completed in 13.5 ms, CSR construction in 49.3 ms,
and queries dominated the remainder.

Changing direction selection from the smallest distance to the smaller heap
reduced total native time to about 236 ms. The termination condition continues
to use the sum of frontier distance minima, so answers remain exact. Replacing
the two graph-sized distance arrays per worker with reusable sparse hash maps
reduced a further local run to about 203 ms and reduced memory requirements.
An integer radix heap with dense distances took about 233 ms, so it was not
retained. These are single local diagnostic runs on Apple Silicon, not official
scoring measurements. The final version also removes the obsolete touched-node
lists from the sparse implementation.

Full seed 1 output matches:
`a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e`.
Sample seed 1 matches:
`a4c76ed030bc7e365db33191f6cf74708fba7f25eb48d64c8c4279f2e57046a0`.
Five differential tests against the unchanged Python reference cover exact
uint64 sums, zeros, directionality, duplicates, parallel edges, disconnected
components, no-edge graphs, no-query files, and eight randomized all-pairs
cases. Formatting and Clippy with warnings denied pass.

## Step 2: use all twelve query workers

The first board measurement showed queries still needed more throughput.
Increase concurrent query workers from six to twelve, while retaining sparse
per-worker state and the two concurrent CSR builders. Three warm native trials
were about 192 ms with six workers; the twelve-worker candidate took about
132 ms median (249 / 132 / 130 ms, with the first run noticeably cold).
These results are from Apple Silicon and do not predict the Intel board's
hyperthreading benefit. The second live-board run, commit `50d3a87`, passed all
gates and scored 59.008513 with S=269 and D=1. This remains below the 59.238 target.

A sequential CSR construction experiment released raw edges before building the
reverse graph. It reduced the theoretical build allocation peak from roughly
170 MB to 114 MB but raised local runtime to about 185 ms. Retain the parallel
builders because that time cost outweighed the expected memory-score gain.
Five differential tests and the full published output hash continue to pass.

## Step 3: compact queue entries for short distances

Distances up to uint32 maximum now use one packed uint64 containing the distance
and node ID. Larger distances keep the original full-width tuple in a separate
heap. Every compact entry sorts before every full-width entry, so selecting from
the compact heap first preserves the same exact ordering and uint64 answers.
This reduces the heap footprint and comparison/movement cost for shorter paths
without assuming a bound on the final distance. A native full-seed-1 comparison
took about 134.1 ms before and 118.4 ms after the change. These are local Apple
Silicon diagnostics. The live-board run `run-b8e527b07f2c` at commit `7e74e404`
measured 0.326074 seconds, 171,425,792 bytes, S=305 and D=1 for 59.490467
points. All gates passed, exceeding the requested 59.238-point target.

The compact-queue candidate passes all five differential suites, including paths
above uint32 and randomized all-pairs queries with uint32 maximum weights. The
full and sample published hashes also match.

## Step 4: shrink temporary graph construction data

Parsed edges now use one uint64 with three 21-bit fields. The high bit marks an
index into a full-width side list when any field is too large, preserving exact
uint32 node IDs and weights. Each CSR builder reuses its offsets as construction
cursors, then shifts the resulting end offsets back to starts. This removes the
two separate node-sized cursor allocations. The final adjacency representation
and search loop are unchanged; temporary edge lists are still freed before
queries start.

The tested prototype was `.out/d-new/d-rawpacked-cursor.rs`; its measurements are
in `.out/d-new/docker-results.json` at the repository root. On local Linux ARM64,
full seed 1 warm median time fell from 174.5 to 170.2 ms. The largest warm cgroup
memory peak fell from 168.9 to 136.5 MB. These compare four warm runs of each
version, excluding the first run. Every output matched the published hash.
The estimated gain after source-size cost is about 0.9 D points, not an official
score, and is insufficient alone to return D to the team's best three tracks.
Custom search maps and compressed adjacency were slower locally and are not
included.

Seven differential suites include new coverage of packed-field boundaries,
node IDs above 21 bits, mixed compact and full-width edges, adjacency gaps and
the last node. The existing uint32 maximum, long-distance, empty-graph and
randomized all-pairs cases continue to pass, as do formatting and Clippy with
warnings denied.

The board measured step 4 at commit `8528bde3cb3715b078d61524bc02d1ceb0cbe166`
in `run-d9856975d2f3`: all gates passed, T=0.333852 s, M=136,790,016 bytes,
S=336, D=1, points=59.899443. The memory improvement transferred to Intel;
the local runtime improvement did not.

## Step 5: build-time PGO (2026-10-30; measured and reverted)

The offline build generates sample-scale seeds 7, 19 and 103 with a byte-identical
copy of the official generator. The allowlisted Rust image supplies matching
LLVM tools: compile with instrumentation, run all three seeds, merge profiles,
then rebuild with profile-use and missing-profile warnings. Both compilations
retain native CPU targeting and the release settings. Only the final executable
enters the runtime image; seed 1 is excluded from training.

Seven alternating trials per variant on local ARM64 Linux, pinned to CPUs 0–11,
excluded the first trial from warm summaries. Median process time was 162.713 ms
before and 158.691 ms after PGO, about 2.5% less. Worst warm cgroup peak was
137,560,064 versus 136,097,792 bytes. Added source cost may absorb this gain.
All outputs matched the public full-seed-1 hash. These measurements do not
establish Intel performance.

All existing differential suites pass against the actual PGO runtime container.
Offline ARM64 and AMD64 builds succeed without missing-profile warnings; the
AMD64 sandbox run also matches the public full-seed-1 hash. Build scripts and
Dockerfiles count toward source size; unchanged official generators are exempt.
Raw local results are in `.out/pgo-experiment/bench.json` at the repo root.

Board run `run-073e327db1c6` at commit
`3b74c524ad473105b5bfa934855984fe98336eea` passed every gate:
T=0.330415 s, M=137,994,240 bytes, S=360, D=1, points=59.749009.
Runtime improved by about 1%, but memory and source costs outweighed it:
PGO lost 0.150434 points. Restore the previous Docker build.
The measured commit preserves the complete reproducible PGO experiment,
including the unchanged generators and training scripts.


## Step 6: query-aware contraction and text offsets (2026-10-30; rejected locally)

Two independent candidates start from retained `e2db239`, without PGO.
Production D remains unchanged. Keep A unchanged as well; a lower D score would
not enter the team's best three, while C's table reduction benefits a counting
track immediately.

The contraction prototype reads queries first and protects every endpoint.
It drops nonnegative self-loops, min-deduplicates parallel arcs, and serially
eliminates unprotected vertices with at most two unique neighbors in the union
of incoming and outgoing adjacency. Each incoming/outgoing pair creates its
exact directed shortcut, excluding a return to the same neighbor. The arc slot
to the eliminated vertex supplies space for the sole possible replacement;
there is no separate heap-allocated map per vertex. Tombstones are compacted
in place. Original edge entries remain eight bytes, with a sparse exact `u64`
side map for shortcut weights above `u32::MAX`; distance additions saturate
safely. Every shortcut represents a real directed walk of equal weight, and
every path through an eliminated vertex has a replacement shortcut. Nonnegative
cycles can be removed. Protected endpoint distances are therefore preserved.

On full seed 1 this removes 615,583 vertices and leaves 3,711,163 directed arcs.
One native diagnostic run recorded parse 49.5 ms, build 28.0 ms, contraction plus
compaction 355.5 ms, and queries 58.3 ms. These phase measurements are a separate
macOS diagnostic, not Linux or Intel evidence. They identify the serial
preprocessing as a larger cost than the query work it removes.

The independent text-offset candidate retains the read-only mapping through
all queries and stores original line positions in forward/reverse CSR. Checked
file size selects `u32` positions or an exact `u64` fallback. Two concurrent
builders each count then scatter, reusing offsets as cursors, so no raw edge
list remains allocated. Input lines are decoded again on traversal. Its index
payload is 53,924,104 bytes at full seed 1; this excludes search/scratch,
page tables and allocator slack and is not total memory.

Seven alternating trials per variant used local Linux ARM64, CPUs 0–11, offline
Docker native release builds, prewarmed input and the board sandbox. Exec-to-exit
time includes all preprocessing; peak is cgroup `memory.peak`. Exclude each
variant's first trial for these warm summaries. Every full-seed-1 hash matched.

| Variant | Warm median (s) | Worst warm peak (bytes) | Local delta before source cost |
|---|---:|---:|---:|
| Baseline | 0.164619466 | 138,682,368 | 0 |
| Degree-two contraction | 0.490045842 | 135,430,144 | -11.741 |
| Text-offset CSR | 0.480513191 | 87,232,512 | -9.617 |

Reject both: neither memory saving repays roughly tripled end-to-end time,
even before extra formatted-source cost. The official static-tools image was
not available locally, so no official candidate S or Intel score is claimed.
Both sources visibly add code; source savings cannot rescue the result. There
is no reason to submit these losing variants, try degree three with this
mutation design, or combine their costs. A future contraction design needs to
avoid the serial normalization/mutation pass or amortize it across many more
queries. Text-backed random decimal parsing needs a substantially cheaper query
path. The immediate next priority is C's small measured table improvement,
then Intel B worker/scratch-layout experiments.

Ten differential suites run against each candidate, explicitly selecting its
own binary. New sparse-endpoint chains, wide shortcuts, one-way forks, zero
cycles, components without endpoints, parallel minima in either input order,
protected leaves, same-node queries, shortcut-vs-direct minima and randomized
edge orders exercise elimination rather than protecting every vertex with
all-pairs queries. Text-offset Rust unit tests cover width selection at the
u32 boundary and matching narrow/wide adjacency decoding. See versioned sources,
reproduction tooling and raw samples in
`experiments/2026-10-30-score-tradeoffs/` at the repository root.

Both candidates also match the unchanged Python oracle on full seed 42:
`0069046cc261b9825d90583ea3e6417d9728b8e7356b3fbde986f65bdeed7945`.

The latest unchanged-baseline board repeat before this experiment was
`run-8e2549a44195` at `e2db239`: all gates passed, T=0.357854 s,
M=136,007,680 bytes, S=336, D=1, K=15.254961, points=59.170556.
The best retained baseline remains the earlier 59.899443-point run. Single-run
variation must not be mistaken for an algorithm change.


### Revised compact-weight guard (also rejected)

The revised handoff asks to skip an entire contraction if any directed shortcut
would exceed `u32::MAX`. A separate candidate implements that check over all
incoming/outgoing pairs **before changing either adjacency**, removes the wide
side map entirely, and retains the eight-byte original edge representation.
It preserves arbitrary full-width answers by leaving wide chains in the graph.
This is separate from the exact wide-shortcut prototype above.

Seven further alternating trials per variant prewarmed input immediately before
every run and otherwise used the same Linux ARM64 sandbox and timer. Excluding
the first trial, baseline median was 0.175365553 s with worst peak 137,347,072
bytes; guarded contraction was 0.414561838 s and 135,262,208 bytes. Every output
matched. Even without a source penalty this loses about 9.275 local machine
points, so the revised guard does not rescue serial preprocessing. It passes
all ten differential suites and full seeds 1 and 42. Keep D production unchanged.
The guarded source and raw series are preserved with the other experiments.

## Step 7: four-byte adjacency with exact exceptions (2026-10-30)

Retain parallel parsing, concurrent CSR builds and twelve query workers. Each
ordinary final edge uses 21 node bits and 10 weight bits. The high bit tags an
index into that direction's full-width exception vector. Larger nodes and all
uint32 weights remain exact; distances remain uint64. No input recognition,
dependencies, PGO or compiler flags are added.

Full seeds 1/42 each have five million edges and 1,740,512 nodes, with
52,012/52,384 weights above 1023. The two final edge arrays plus live exceptions
need 40,832,192/40,838,144 bytes instead of 80,000,000, excluding capacity.
This approximately 39.2 MB payload saving is separate from measured peak.

Nine alternating trials per variant and seed used offline Rust 1.99.1 Docker
builds on local ARM64 Linux, CPUs 0–11, identical limits and no competing work.
Warm summaries exclude each variant's first trial, use median exec-to-exit time
and maximum cgroup peak, and retain every row. These are local diagnostics,
not Intel results or the final five-run scoring estimator.

| Seed | Variant | Warm median (ms) | Worst warm peak (bytes) | Estimated point delta with source |
|---|---|---:|---:|---:|
| 1 | Baseline | 167.087 | 138,117,120 | 0 |
| 1 | Tagged vector, weights 0–1023 | 159.384 | 101,183,488 | +1.630 |
| 1 | Slot-keyed exceptions, weights 0–2046 | 163.581 | 96,751,616 | +1.498 |
| 1 | 0–2046, forward then derive reverse | 242.717 | 96,821,248 | -2.903 |
| 42 | Baseline | 172.764 | 137,486,336 | 0 |
| 42 | Tagged vector, weights 0–1023 | 161.542 | 96,493,568 | +2.033 |
| 42 | Slot-keyed exceptions, weights 0–2046 | 168.472 | 96,100,352 | +1.550 |
| 42 | 0–2046, forward then derive reverse | 251.192 | 95,584,256 | -2.877 |

Source estimates are 336/361/366/379 respectively, after local canonical Rust
formatting plus the unchanged Dockerfile; D=1 throughout. Only baseline S is
official. Select the tagged vector for an Intel trial: local full-scale time
falls 4.61%/6.50%, and worst warm peak falls 26.74%/29.82%. No new Intel score
or official candidate S is claimed.

The first series retained a 227,766,272-byte baseline peak despite per-run
prewarming; it is not selection evidence. The table uses a subsequent diagnostic
with read-only input pages pinned by an idle helper outside measured cgroups.
Prewarming still precedes every run. Warm D rows record 12,288 file bytes after
exit. This pinning is benchmark machinery, absent from solvers and production
Dockerfiles. The timer reads container `memory.peak` before teardown; the
official harness reads a dedicated parent's peak after teardown.

Separate phase diagnostics explain the sequential build's remaining peak:
forward construction reaches 67,690,496 bytes; freeing raw edges drops current
memory to 27,582,464; the finished graph pair holds about 54.6 MB. Query execution
then reaches 94,806,016 bytes. Twelve workers retain two maps and two queues
each; search scratch and allocator memory now dominate. Instrumented phase
runs are excluded from timing selection.

All eleven differential suites pass for each packed candidate, including
1023/1024 and 2046/2047 boundaries, maximal packed node fields, node IDs above
21 bits, maximum weights and wide distances. Every compared full-seed-1/42 hash
matches the unchanged reference's published or previously verified output.
Production formatting, Clippy with warnings denied, and offline Docker sandbox
checks on both full seeds pass. Complete Rust sources, raw rows and reproduction are in
[`experiments/2026-10-30-compact-graphs/`](../experiments/2026-10-30-compact-graphs/).


## Step 8: predecessor-aware queue (2026-10-30; measured and reverted)

Carry each discovered node's predecessor in its heap entry. Ordinary entries
pack distance22/node21/parent21 into the existing eight-byte common heap word;
full-width tuples preserve all larger values. Compare both heap minima because
wide node IDs can carry distances smaller than a compact entry. Stale-entry
checks and the bidirectional stopping condition remain unchanged.

An immediate return to the predecessor cannot improve its known distance under
nonnegative weights, so skip that arc before probing the distance map. Prior
instrumentation of full seeds 1/42 found 6,963,254/7,314,109 such arcs, about
33.2% of all relaxations. This count is an opportunity estimate, not a runtime
speedup claim. No reachability preprocessing or graph mutation is introduced.

Baseline: `run-6082ca272e2a` at `c17406b`, T=0.316613 s,
M=97,452,032 bytes, S=361, D=1, points=61.713950. Local runtime tests and
benchmarks are skipped per user instruction. Formatting and native/x86_64 Linux
compilation checks pass.

The Intel trial `run-45eac4be08b8` at `330dbc3` passed every gate:
T=0.326405 s, M=97,144,832 bytes, S=384, D=1, K=16.746174,
points=61.195780. The immediately preceding unchanged-production repeat,
`run-335888525673` at `a30d6d1`, measured 0.309189 s, 98,488,320 bytes,
S=361 and 61.925629 points. The candidate loses **0.729849 points**:
-0.588319 time, +0.059650 memory, -0.201180 source. Its measured memory and
source would require time below 0.305185 s to break even against that repeat.

Restore the previous production queue. Fewer distance-map probes did not
translate into lower end-to-end time; predecessor packing and merging heap
minima add work, but its exact contribution was not profiled on Intel.
Preserve [`d-predecessor.rs`](../experiments/2026-10-30-rejected-pipelines/d-predecessor.rs)
and [raw Intel evidence](../experiments/2026-10-30-rejected-pipelines/intel-330dbc3.json)
for subsequent investigation.


## Combined remaining-plan candidate (2026-10-30)

The user requested the remaining concrete candidates together in one push,
superseding the plan's separate-submission workflow.

Separate parsing-worker and query-worker limits. Keep up to twelve edge
parsing workers and two concurrent CSR builds; cap only the dynamically claimed
query work at six workers (or available parallelism/query count when smaller).
Keep packed adjacency and the selected distance/node queues. This can reduce
live per-query maps and heaps, but construction overlaps may still determine
peak, and reduced concurrency can lose runtime. No guaranteed memory target
or speedup is claimed.

Immediate official baseline at `d34c3a4`, `run-75c0e7050728`:
318.682 ms, 96,813,056 bytes, S361, D1,
61.671800 points. Historical best: 61.969457 points
at 309.870 ms / 96,964,608 bytes / S361.
Estimated S362 (local formatted source-line delta, not official S) requires
below 318.418 ms at the recent baseline's memory, or 309.613 ms
at the historical best's memory. Actual peak memory is unmeasured; evaluate
the full measured score after the official run.

Formatted and source-reviewed. Per user instruction, no local compilation,
solver execution, tests, benchmarks or Docker runs were performed. Official
compilation, correctness and performance remain pending. Complete pre-change
and candidate files and the raw baseline board response are in
[`experiments/2026-10-30-batched-plan`](../experiments/2026-10-30-batched-plan/).


### Completed combined Intel result and selection

`run-48c0c51098d1` at `2ab1dd9` passed every check, build, correctness,
timeout and sandbox gate; static axes are measured. Official response generated
2026-10-30 23:42:24 UTC:

| T (ms) | M (bytes) | S | D | Machine points |
|---:|---:|---:|---:|---:|
| 357.599 | 97,329,152 | 362 | 1 | 60.388730 |

Score change: **-1.283070** against the immediate
`d34c3a4` repeat and **-1.580727** against the preceding
historical best.

Reject the six-query-worker candidate: runtime rose 12.21% relative to
the immediate repeat while measured peak rose 0.53%. Restore production source
from `d34c3a4`, retaining twelve parsing/query workers, concurrent CSR builds and
packed adjacency. Reducing query scratch did not lower end-to-end measured peak
in this run; no specific phase is claimed as a profiled explanation.

Complete candidate sources remain archived with
[intel-2ab1dd9.json](../experiments/2026-10-30-batched-plan/intel-2ab1dd9.json).
The selected combination keeps A and restores B/C/D as described. No local
builds, solver executions, tests, benchmarks or Docker runs were performed.
Restored-combination official results remain pending; public-data gate success
is not evidence of final holdout correctness.


## Bold search-state rewrite: generation tables and radix queues (2026-10-30)

Replace both search engines' HashMap distance stores and binary heaps together.
The distance table uses two flat vectors: an exact u32 node plus u32 generation
in each tag, and a full u64 distance. Linear probing compares complete tags,
grows above 75% occupancy, and rehashes only the current generation. Starting a
query increments the generation in O(1), avoiding a capacity-sized table clear
for every short query after a long search. Generation wrap explicitly clears
tags before restarting at one; generation zero is always vacant. No node ID or
distance is narrowed or used as an empty-key sentinel.

Each frontier is a 65-bucket monotone radix heap. Buckets use the highest
set bit of distance XOR the most recently exposed minimum; zero holds equal
minimum distances. A u64 bitset locates the first nonempty higher bucket.
Redistribution chooses that bucket's exact minimum and moves all entries to
strictly lower buckets, retaining allocations. Queue keys contain only distance,
so equal-distance nodes may be popped in any order and zero-weight arcs remain
valid. Full u64 keys cover distances above u32 without a second heap or a
compact/wide ordering assumption. Both directions retain their own monotone
state. Stale-entry checks and the frontier-minimum stopping rule remain intact.

Packed CSR adjacency, twelve parsing workers, concurrent graph builds and twelve
query workers are retained. This changes core search state rather than graph
contraction or predecessor suppression. No external implementation was copied
and no new runtime dependency or compiler flag is introduced.

Immediate baseline: `d900c19`, `run-d93663499dcd`, 327.403 ms,
97,128,448 bytes, S361, D1, 61.364546 points; every official gate passed.
Historical best: 309.870 ms, 96,964,608 bytes, S361, 61.969457 points.
Estimated source is S437 from the formatted noncomment delta, not an official
measurement. At unchanged historical-best memory this needs about 292.6 ms to
break even. Flat-table slack and retained radix buckets may increase peak:
tags plus values require 16 bytes per allocated slot, and radix entries occupy
16 bytes on x86_64 versus eight for the old compact queue. The intended gain is
less per-query clearing and cheaper frontier operations; actual time and peak
must come from Intel. This experiment does not isolate the two changes.

Source review checked vacant-tag termination, exact key matching, growth
rehashing, generation rollover, monotone keys, strictly decreasing redistribution
bucket indexes, wide distances, zero-weight ties and empty-frontier termination.
Formatting and diff whitespace checks passed. No compilation, local execution,
tests, benchmarks or Docker runs were performed, following the user's request.
Official build/correctness/performance results remain pending. Complete baseline
and candidate sources are in `experiments/2026-10-30-bold/d-baseline.rs` and
`experiments/2026-10-30-bold/d-candidate.rs` outside the production track.


## Parallel node-owned graph assembly experiment (2026-10-30)

Replace the two whole-graph CSR builders with power-of-two vertex owners.
Every owner streams the compact input edges twice, counting then scattering
both graph directions into its own CSR fragments. At full scale twelve threads
produce seven ranges of at most 262,144 vertices. Global node IDs and exact
per-fragment exception entries retain every legal edge. Queries select a
fragment by shift/mask. There is no atomic scatter, global concatenation or
additional edge-sized partition buffer. The current generation distance tables,
radix queues, parser and twelve query workers remain unchanged.

The intended gain is parallel construction and smaller random-write ranges.
The deliberate cost is extra streaming/decoding: seven owners each read the
compact edges twice instead of two builders each reading twice. This is an
experiment, not a profiled or measured claim. Ordinary final edge payload
remains four bytes per direction; each fragment adds one offset sentinel.

Immediate official baseline at `b90a4a4`, `run-9ead5837f230`: 379.135 ms,
100,716,544 bytes, S437, D1, 58.991915 points. Historical best is
`run-e4950b1f6c67`: 309.870 ms, 96,964,608 bytes, S361, 61.969457 points.

Formatted nonblank/noncomment source delta estimates S495, pending the
official canonical formatter/static-axis pass. At unchanged historical-best
memory, break-even against that best is approximately 281.9 ms. Actual
runtime and peak are unknown. Compare the complete measured score, including
source and memory, before selecting this candidate.

Complete baseline/candidate source and raw baseline evidence are preserved in
[`experiments/2026-10-30-query-machine`](../experiments/2026-10-30-query-machine/).
Source review and Rust formatting/whitespace checks passed. No Docker, local
compilation, solution execution, tests or benchmarks were performed, following
the current repository instruction. Official Intel build, correctness and
performance are pending; public-data correctness will not prove holdout safety.


### Completed Intel result for `2cea13f`

`run-33b028b22671` passed every official gate with measured static axes:
459.309 ms, 100,663,296 peak bytes, S495, D1,
56.505507 machine points. Historical-best delta: -5.463950 points.
Raw evidence is archived in
[`intel-2cea13f.json`](../experiments/2026-10-30-query-machine/intel-2cea13f.json).
The complete candidate remains in that experiment directory. The next shot
uses the earlier better-scoring streaming/search structures as its base; no
speedup over the historical best is claimed for this attempt. All verification
was on the official Intel board; no local execution or tests were performed.


## Parse-time radix routing into final CSR slices (2026-10-30)

The user requested another unusually bold one-shot experiment after `2cea13f`.
Replace the seven-owner repeated-whole-graph scans with parse-time source/target
routing. Each parsing worker emits one compact forward copy into its source
owner's bucket and one reverse copy into its target owner's bucket. Reverse
copies reverse endpoints at parse time. Temporary values retain the previous
three-21-bit packed representation with exact full-width exceptions; vector
capacities grow when needed. Reserve proportional to each owner's actual node
count with 10% headroom: uniform division by seven would predictably undersize
the six full ranges because the final range is shorter, causing geometric
capacity doubling. This capacity heuristic affects allocation only; arbitrary
skew remains exact. Neither input ordering nor any seed is recognized.

Bucket lengths and counts of wide final adjacency entries give exact global
allocation sizes. Allocate the two final CSR graphs once. Safe `split_at_mut`
views assign every owner disjoint offset, edge and exception slices, plus their
global base indices. Each owner counts and scatters only the records routed to
it; it consumes parser buckets and releases their capacity during scatter.
No global rescans, atomic scatter, unsafe mutation, intermediate CSR fragments
or concatenation copy remain. The finished query representation is ordinary
contiguous CSR, with no owner lookup on the search path.

Offset ownership excludes the final global sentinel: `offsets[n]` is set to the
actual direction edge count before work starts. Owners turn degrees into global
starts, use those entries as cursors, then rotate their own offset slice and set
its first entry to their edge base. The next owner's first offset supplies the
previous owner's end. This covers empty degree ranges, span-one owners and the
last partial range. Wide counts use the same neighbor/weight predicate as final
packing. The total exception length is checked below 2^31, so all preassigned
exception indices fit below the tag bit. Both endpoints stay global and exact.

Restore the previously selected sparse HashMap distance store and compact/wide
binary heaps from the complete `d900c19` baseline preserved in
`experiments/2026-10-30-bold/d-baseline.rs`. The generation-table/radix-queue
combination had not beaten this engine. Consequently this submission combines
construction and search selection; its result cannot isolate their individual
contributions. Twelve parsing/query workers remain; construction spawns no more
owners than available workers, since the owner span rounds up from n/workers.

The common temporary payload is 16*m bytes, versus 8*m before routing. At full
scale this is 80 MB of used entries; proportional reservations with headroom
budget roughly 90 MB before further growth. Final CSR contributes about 54 MB plus
wide exceptions. All final arrays are allocated while buckets still exist.
Geometric bucket growth, parser imbalance, allocation metadata and exceptions
can raise this peak substantially; 145–180 MB is an illustrative budget, not a
measurement or bound. Demand paging and per-owner release can make actual
resident overlap lower than total vector capacity; allocator retention can keep
freed pages charged. Input mappings are released after parsing, before CSR
construction. Exact bucket sizing would need another scan and is not added.

Immediate official baseline: `2cea13f`, `run-33b028b22671`, 459.309 ms,
100,663,296 bytes, S495, D1, 56.505507 points. Historical best:
`aa708b6`, `run-e4950b1f6c67`, 309.870 ms, 96,964,608 bytes, S361,
61.969457 points. The formatted nonblank/noncomment delta estimates S457;
canonical static measurement remains pending. At a hypothetical 145 MB peak,
full-score break-even against the historical best is approximately 245.8 ms;
at 180 MB it is approximately 225.4 ms. Aim for a large runtime reduction rather
than treating memory growth as free.

Complete current baseline and candidate sources are preserved outside production
in `experiments/2026-10-30-next-shot/d-baseline.rs` and `d-candidate.rs`.
Source review covered routing coverage, source reversal, owner/thread bounds,
zero/duplicate/self-loop/wide edges, exact counts and tag indices, global CSR
boundaries, private-slice lifetimes and mapping release. Rust formatting and
whitespace checks passed. Per repository instruction, no Docker, local
compilation, solver execution, tests or benchmarks were run. Official build,
correctness and performance remain pending; source review cannot establish
holdout correctness.


### Completed Intel result and selection at `196b0c0`

`run-8274c9e7b4bd` passed check, build, correctness, timeout and sandbox,
with measured static axes: **247.106 ms**, **110,682,112 peak bytes**,
**S457**, D1, **63.084153 machine points**.
Delta versus preceding historical best: **+1.114696 points**.

Keep the new routed builder and historical binary-heap search engine. Time
fell 20.3% from the historical best while memory rose 14.1% and S rose 361 to
457; the complete score improved 1.114696 points. Relative to `2cea13f`, time
fell 46.2% and score rose 6.578646 points. This is one public-seed result,
not evidence that each individual mechanism helped or that the gain is repeatable.

Raw official response generated 2026-10-31T04:23:43Z and complete candidate
source are preserved under
[`experiments/2026-10-30-next-shot`](../experiments/2026-10-30-next-shot/).
No local compilation, execution, tests or Docker were used. Public-data gate
success does not establish correctness on the final holdout seed. The selected
combination retains A, restores B/C, and keeps D; its next board run is a repeat
of these selected implementations, not another algorithm experiment.


## Direct inline graph: remove the CSR build (2026-10-30)

Hypothesis: the fastest graph assembly is no separate graph assembly. Instead
of routing every edge twice, counting degrees, prefix-summing and scattering
into CSR, parsers reserve slots directly in the final node records. Each
vertex/direction has a uint32 atomic degree and four uint32 atomic slots.
Ordinary entries keep the existing exact 21-node-bit/10-weight-bit packing.
A unique `fetch_add(Relaxed)` reservation assigns each directed insertion one
slot. An edge that is wide or beyond four slots is appended, at full uint32
width, to that parser's private exception vector. Only exceptions are merged
after parsing; all ordinary edges are already in their final location.

Four is not a degree limit. A wide edge within the first four reservations
leaves a high-bit marker, so traversal detects overflow even at small degree.
Degree above four also requires overflow. Traversal emits every ordinary
inline edge and every full-width overflow edge exactly once. The second graph
receives the exact reversed insertion. Producer scheduling changes storage
placement but cannot remove an edge. Queries begin after every producer joins;
join establishes publication, so reservation/store operations need no release
ordering. The overflow maps are then immutable during queries. Search state,
uint64 arithmetic, queue ordering, early exits and output format are unchanged.

This replaces the whole construction/layout mechanism, not an isolated atomic
operation. It removes two ordinary temporary edge copies and both CSR passes,
but spends ten million random atomic reservations at full scale and increases
inline adjacency payload. The arrays use 40*n = 69,620,480 bytes at full scale,
plus overflow maps/vectors, allocator overhead and query scratch. Input pages
can overlap those arrays during parsing. No peak-memory reduction or speedup
is established by these payload numbers. High degrees and wide weights grow
exactly; they may be expensive and are not treated as distribution guarantees.

The official baseline repeat at `e6d4333` is `run-b2bf06e7dc60`:
249.672 ms, 109,588,480 peak bytes, S457, D1, 63.015115 machine points.
Historical best is `run-8274c9e7b4bd` at `196b0c0`: 247.106 ms,
110,682,112 bytes, S457, D1, 63.084153 points. Both passed every gate.
The complete response generated 2026-10-31T04:28:37Z is archived with full
baseline/candidate source under `experiments/2026-10-30-inline-graph/`.
A/B/C remain unchanged so the experiment isolates this track.

Source review covers degree 0/1/4/5, wide edges in each inline reservation and
after the inline range, all-wide neighborhoods, duplicate/parallel/self-loop
edges, zero weights, node/weight packing boundaries, exact reversal, parser
chunk boundaries and empty chunks, publication after joins, uint64 distances,
empty query files and stable output order. No unsafe code was introduced.
Rust formatting and whitespace checks are the permitted local verification;
no Docker, local compilation, solver execution, tests or benchmarks are run.
Official build, correctness and full measured score are pending. A public-data
pass cannot establish correctness or timing on the final holdout seed.


### Completed Intel result at `addf6d8`: reject and restore

The full official response generated 2026-10-31T04:33:18Z is preserved in
`experiments/2026-10-30-inline-graph/intel-addf6d8.json`. Candidate `addf6d8`, D run `run-4d58ed0ef75d`, passed
check, build, correctness, timeout and sandbox, with measured static axes:

| Version | Time ms | Peak bytes | S | D | Machine points |
|---|---:|---:|---:|---:|---:|
| Selected CSR baseline repeat | 249.672 | 109,588,480 | 457 | 1 | 63.015115 |
| Direct inline candidate | 296.114 | 130,162,688 | 351 | 1 | 61.275239 |

The candidate took 18.6% longer and used 18.8% more peak memory than the
baseline repeat. Reducing S by 23.2% did not compensate: it lost 1.739876
machine points against that repeat and 1.808914 against the historical best.
A/B/C were unchanged and passed every gate. Their repeat changes are not
attributed to this D experiment.

Restore the complete `d-baseline.rs` to production and retain both sources and
this measured failure outside the track directory. The direct-build hypothesis
did not produce an efficiency gain. Random atomics, node-array initialization,
overflow merging and query locality are possible costs; no phase profile was
collected, so this result does not isolate a cause. A public-seed gate pass is
not a holdout proof. No local Docker, compilation, solution execution, tests or
benchmarks were run. Formatting/whitespace checks passed; the restored Rust
source is byte-identical to the preceding selected implementation.


## Native radix-queue screen (2026-10-31; rejected)

Replace only the selected search queue with 64 radix distance buckets and a
binary heap of nodes at the current distance. Preserve full uint64 distances,
nonnegative-edge monotonicity, exact node ordering for equal distances, sparse
labels, graph construction and worker count. The hypothesis was fewer heap
comparisons in the locally dominant query phase.

Ten timings per variant on full seeds 1 and 42: baseline -> candidate medians
75.663 -> 82.826 ms and 77.789 -> 84.947 ms. Candidate peak Mac footprint was
152,764,920 / 155,484,664 bytes versus 146,866,680 / 150,962,680 bytes. Both
runtime (about 9% slower) and footprint lose, while local source grows by 34
nonblank/noncomment lines. Reject the candidate; production is unchanged.

All 11 existing Python-reference differential suites passed. A targeted queue
oracle compared 160,000 mixed operations plus complete drains against the
standard binary heap, including ties, width boundaries and repeated clear/reuse.
An isolated node-ID mutation made that oracle fail. Complete sources and every
run are `d-radix-*` under
[`2026-10-31-native-first`](../experiments/2026-10-31-native-first/).
These are native Apple M4 Max observations, not Intel or holdout evidence.


## Bound-based relaxation pruning (2026-10-31; rejected)

Apply the skip-discarded-work idea from the 0xicbor review: before a distance
map lookup or heap insertion, skip a relaxation whose candidate distance is at
least the known complete path cost. Nonnegative weights make this exact, but
its value depends on how often that condition occurs.

Ten serial randomized native exec-to-exit timings per variant on full seeds
1 / 42 gave baseline -> candidate medians **76.765 -> 78.212 ms** and
**77.465 -> 78.982 ms**. Candidate was faster in only 3/20 pairs. Separate Mac
peak-footprint maxima were 146,326,008 / 148,013,560 bytes versus baseline
145,654,264 / 148,242,936 bytes; these are not official cgroup memory. Estimated
S457 -> S460 needs about 0.196% less time at equal official memory; instead
time rose about 2%. Reject and leave production unchanged.

Two instrumented runs per seed counted identical totals: only 4,153/20,972,155
and 4,206/22,006,890 relaxations were pruned, about 0.02%. This limited
opportunity is consistent with the added check losing; no hardware cost was
isolated. All eleven existing Python-reference methods passed against the
candidate. Every benchmark/diagnostic matched full saved reference bytes on
seeds 1 and 42, not newly executed full-size reference runs or holdout proof.

Complete baseline/candidate/diagnostic source, every timing, separate RSS and
footprint measurements, executable/source/input hashes, compiler/flags and
hypothetical scoring are in `d-prune-screen.json`, `repo-tricks-diagnostics.json`
and the [experiment report](../experiments/2026-10-31-native-first/REPO-TRICKS.md).
No commit or official submission was made.
