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

All measurements remain above the reference-memory/50 cap. Savings did not
repay the extra time in the actual scoring formula; retain twelve workers.
Apple Silicon has different cores from the scoring server, so this rejects a
local selection, not the possibility of an Intel-specific win. An Intel worker
sweep and query-category-sized scratch arrays remain useful next experiments.
Six unchanged B differential suites pass. Reproduction and raw samples are in
`experiments/2026-10-30-score-tradeoffs/` at the repository root.
