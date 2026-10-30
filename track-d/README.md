# Track D: Traverse

The [frozen specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md)
defines directed shortest-path queries with nonnegative uint32 edge weights.

## Approach

Map the graph read-only and split its edge lines among up to twelve parsing
workers. Temporary edges use eight bytes when both node IDs and the weight fit
in 21 bits; a tagged index preserves larger values in a full-width side list.
Build compact forward and reverse CSR adjacency arrays concurrently, reusing
their offsets as write cursors and restoring the starts after construction.
Each ordinary adjacency entry occupies four bytes: 21 node bits, 10 weight
bits, and a tag bit. Weights up to 1023 and nodes below 2^21 fit directly. A
tagged entry indexes that direction's full-width exception vector, so larger
weights and node IDs remain exact. Narrowing individual entries does not
require every edge in the graph to fit.
Both preserve parallel edges, duplicates, zero weights and self-loops. No node
ordering, geometry, or input seed is assumed.

Up to twelve query workers share the graph and take queries from an atomic work
counter. Each query uses bidirectional Dijkstra with uint64 distances and binary
heaps. Queue entries with distances up to uint32 maximum pack the distance and
node into one uint64; larger distances use a separate full-width heap. All
compact entries precede full-width entries, preserving the exact minimum.
Expand the smaller frontier, then stop when the sum of the two minimum
queue distances cannot improve the best complete path found. An empty forward
adjacency at the source or reverse adjacency at the target immediately proves
unreachability (except a same-node query, which always answers zero).

Reusable sparse hash maps hold tentative distances, so each query worker pays
for the nodes it visits instead of allocating two full graph-sized arrays.
The hash is multiplicative on node IDs; full integer equality handles collisions.
Results are stored by query index and written in input order. The graph mapping
and temporary edge lists are released after the CSR arrays are built.

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
The existing integer-radix-heap experiment did not improve local runtime enough
to justify its extra code. Keep profiling before adding another data structure.

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
are preserved outside the production folders; see [PERF.md](PERF.md). Production
uses the preceding implementation.
