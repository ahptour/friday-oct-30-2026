# Track B: performance log

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | Published Python reference | | 56.35 | 2,200 | 0 |
| 1 | Parallel query-aware Rust index, adaptive bitsets, token Aho–Corasick phrase matching | Pending | | | |

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
is only diagnostic; the board run and >=64.636 target remain pending.
