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
