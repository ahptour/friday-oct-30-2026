# Track D: Traverse

The [frozen specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md)
defines directed shortest-path queries with nonnegative uint32 edge weights.

## Approach

Map the graph read-only and split its edge lines among up to twelve parsing
workers. During parsing, route each edge into a forward bucket owned by its
source node and a reverse bucket owned by its target node. Owners cover disjoint
power-of-two node ranges; twelve available threads yield seven owners at full
scale. Reverse buckets store the endpoints reversed, so both directions use the
same construction code. Each temporary copy packs three 21-bit fields into a
u64 when possible and uses an exact full-width exception otherwise. Initial
bucket reservations are proportional to their owner's node count, with 10%
headroom; a short final range therefore does not cause undersizing of every
full range. Buckets still grow without a fixed-capacity correctness assumption.

Bucket lengths and wide-entry counts determine the final global CSR allocation.
Safe mutable slice splitting gives each owner its own node-offset range, edge
range and exception range in those final arrays. Each owner counts and scatters
only its own routed edges; it consumes and frees each parser bucket immediately
after scatter. No owner rescans the whole graph, and no fragment concatenation,
atomic scatter or raw-pointer write is required. Offset cursors are restored
within each owner range; the global final sentinel is set before workers start.
The resulting graph is ordinary contiguous CSR, with the same query lookup as
the earlier implementation. Each ordinary adjacency entry occupies four bytes:
21 global node bits, ten weight bits and an exception tag. Exceptions retain full
uint32 neighbors and weights. Zero weights, duplicate/parallel edges, self-loops
and arbitrary input order remain exact.

This deliberately spends two compact temporary edge copies to enable parallel
construction. The common temporary payload is 16*m bytes, plus bucket capacity
slack and wide exceptions. Final ordinary CSR payload is 8*m + 8*(n+1) bytes for
both directions. Capacity growth is unrestricted, so skew may raise the peak;
these are allocation budgets, not measured cgroup memory.

Up to twelve query workers share the graph and consume queries from an atomic
counter. The search engine returns to the earlier binary heaps and sparse
HashMap distances. Each query uses bidirectional Dijkstra with uint64 distances.
Small queue entries pack distance and node into a u64; larger distances use a
separate full-width heap. Expand the smaller frontier and stop when the sum of
minimum distances cannot improve the best complete path. Empty source/target
adjacency immediately proves unreachability, except same-node queries return 0.
Results are stored by query index and written in input order. All temporary construction
storage is released before search workers start.

The only unsafe operations are Unix memory mapping and creating a read-only
slice from the successful mapping. Scoped parsing workers finish before the
mapping is released. The algorithm and graph accesses use checked indexing.

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


## Measured selection

`run-8274c9e7b4bd` at `196b0c0` passed every official gate: 247.106 ms,
110,682,112 peak bytes, S457, D1, 63.084153 machine points. This is 20.3%
faster and 1.114696 points higher than the preceding historical best after
accounting for memory and source cost. This candidate is retained. See PERF.md
for complete evidence and limits; the public-seed result is not a holdout proof.
No local compilation or runtime verification was performed.


## Direct inline graph experiment (rejected)

At `addf6d8`, parsers installed edges directly into four atomic inline slots per
node, with exact high-degree/wide-edge overflow. This removed CSR assembly and
reduced measured S457 to S351. Official run `run-4d58ed0ef75d` passed every
gate, but took 296.114 ms at 130,162,688 peak bytes, scoring 61.275239 points.
The preceding selected CSR repeat took 249.672 ms at 109,588,480 bytes and
scored 63.015115. The complete score lost 1.739876 points, so the preceding
CSR source is restored byte-for-byte. Full candidate, baseline and official
responses remain in `experiments/2026-10-30-inline-graph/`; see PERF.md.
No local compilation, execution, Docker, tests or benchmarks were run.
