# Track D: performance log

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | Organisers' unchanged Python reference | | 72.69 | 893 | 0 |
| 1 | Parallel parsing, forward/reverse CSR, sparse bidirectional Dijkstra, balanced frontiers | run-c88839436741 | 0.459081 | 170.385408 | 56.211682 |

## Rust implementation, 2026-10-30

The target for this track is at least 59.238 live-board points. The first Rust
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
hyperthreading benefit. The second live-board measurement remains pending.

A sequential CSR construction experiment released raw edges before building the
reverse graph. It reduced the theoretical build allocation peak from roughly
170 MB to 114 MB but raised local runtime to about 185 ms. Retain the parallel
builders because that time cost outweighed the expected memory-score gain.
Five differential tests and the full published output hash continue to pass.
