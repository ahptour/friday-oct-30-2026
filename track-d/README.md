# Track D: Traverse

The [frozen specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md)
defines directed shortest-path queries with nonnegative uint32 edge weights.

## Approach

The current experiment removes CSR construction. Twelve parsers write directly
into the final graph while reading their disjoint, newline-aligned chunks.
Each direction has four compact adjacency slots per vertex, next to an atomic
degree counter. An atomic reservation assigns each edge a unique slot. Ordinary
neighbors below 2^21 with weights below 1024 occupy one uint32. Everything else,
including every edge after the fourth slot, goes into exact per-parser overflow
vectors. After parsers join, only those exceptions are merged into node-keyed
adjacency lists. There is no full edge-routing buffer, prefix sum, count/scatter
pass, final graph concatenation or second copy of ordinary parsed edges.

Four slots are a storage choice, never a degree bound. A wide edge reserves its
place but leaves an inline tag; traversal skips that tag and reads the complete
overflow list. Degree greater than four also selects overflow. Thus scheduling
can change which edges are inline without changing graph semantics. Parallel
edges, duplicate lines, self-loops, zero weights, arbitrary node IDs and full
uint32 weights are retained. Slot atomics use relaxed ordering because no reader
traverses the graph before all producer threads join. No new unsafe code is used.

At full scale the two inline arrays have a 40*n byte payload, about 69.6 MB,
plus exact overflow maps, transient overflow vectors and query scratch. This is
an allocation budget, not a measured cgroup peak. Parsing still touches mapped
input pages. Random atomic writes, cache-line contention, overflow allocation
and inline-slot scans could outweigh the removed construction phases.

The query engine is unchanged: up to twelve workers share the completed graph
and use bidirectional Dijkstra, sparse HashMap distances and compact/wide binary
heaps. Distances remain uint64; queries are answered in input order. Same-node
and empty-adjacency cases retain their exact fast paths. All input-derived
structures are built inside the timed run. No seed recognition, cached answers,
new dependency, image or compiler flag is introduced.

The complete preceding CSR implementation and this candidate are preserved
outside production in `experiments/2026-10-30-inline-graph/`. See PERF.md for
source-review details and the official build/correctness/performance result.

## Dependencies

None. The Rust standard library and the platform's mmap/munmap interface suffice.
The Dockerfile uses digest-pinned allowlisted images, builds offline, and enables
`target-cpu=native`, which routes the board build onto its scoring CPU.

## Verification

From the repository root:

```sh
cargo build --manifest-path track-d/Cargo.toml --release --offline --locked
python3 -m unittest discover -s tests -p test_track_d.py -v
cargo clippy --manifest-path track-d/Cargo.toml --offline --all-targets -- -D warnings
python3 check.py d --data data/d/full-seed1 --expected a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e
```

This candidate has not been compiled or run locally, following the current
repository instruction. See PERF.md for official build and correctness status.

The executable defaults to `/data` and `/out`, with `--data` and `--out` overrides
for native development. The unchanged Python reference remains the independent
oracle. Differential tests cover the worked example, arbitrary small graphs,
all-pairs queries, repeated queries, asymmetric reachability, disconnected
components, zero cycles, duplicate and parallel edges, uint32 maximum weights,
paths above uint32, packed-field boundaries, node IDs above 21 bits, adjacency
gaps, isolated nodes, and empty query files.

## With another 24 hours

Measure query-worker counts and CSR construction on the board. Consider graph
relabeling to improve cache locality only if its preprocessing cost is repaid.
Separate parsing, construction and query timings before attributing a combined
result to any one component. Compare the full memory/source-adjusted score.

## Profile-guided experiment

Build-time PGO trained on generated sample seeds 7, 19 and 103 passed correctness
on ARM64 and AMD64. Intel board scores fell after memory and source costs, so
the ordinary native release build is retained. The reproducible experiment is
preserved at commit `3b74c524ad473105b5bfa934855984fe98336eea`; see [PERF.md](PERF.md).


## Query-aware graph experiments

Exact degree-two elimination and input-text-offset CSR were separately tested
with sparse protected endpoints and full-width arithmetic. Both preserved
answers but their preprocessing/traversal cost outweighed memory savings in
local ARM64 trials. Production retains CSR with packed adjacency. See
[PERF.md](PERF.md) and the reproducible sources under
`experiments/2026-10-30-score-tradeoffs/` at the repository root.

Packed adjacency and sequential reverse construction are compared separately in
`experiments/2026-10-30-compact-graphs/`. Sequential construction lowers its
payload, but query scratch becomes the observed peak and serial traversal costs
too much in the recorded full-scale local trials.


The compact-record A and predecessor-aware D trials at `330dbc3` passed the
Intel correctness gates but lost machine points. Their sources and measurements
are preserved outside the production folders; see [PERF.md](PERF.md).


## Preceding measured selection

`run-8274c9e7b4bd` at `196b0c0` passed every official gate: 247.106 ms,
110,682,112 peak bytes, S457, D1, 63.084153 machine points. This is 20.3%
faster and 1.114696 points higher than the preceding historical best after
accounting for memory and source cost. This was the preceding selected baseline. See PERF.md
for complete evidence and limits; the public-seed result is not a holdout proof.
No local compilation or runtime verification was performed.
