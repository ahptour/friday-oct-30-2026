# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Rust std-only streaming parser, byte-prefix hours, buffered output, exact percentile selection | run-5ae87f32d8ff | 0.705724 | 41.951232 | 46.671522 |
| 2 | Parallel mmap parsing, borrowed keys, sampled hash, word delimiter scanning and parallel output | run-595acb9bbcc2 | 0.048674 | 41.299968 | 73.210288 |

Step 0 contains the organisers' published scoring-box reference figures. Step 1
was measured at commit `0b08b305a17153c8831acca9f29a8c0595dbc7a0`; the board
reported S=107, D=1, K=8.578877, and all gates passed. Local timings below are
separate from board results.

## Step 1: establish a correct Rust baseline (2026-10-30)

The reference spends much of its time constructing strings and parsing dates.
This version reuses a byte buffer, takes the UTC hour directly from the timestamp,
allocates endpoint keys only for new groups, and stores four-byte latencies.
It selects the three exact percentile ranks in progressively smaller slices
instead of sorting the full latency lists. It uses no external dependencies,
unsafe code, worker threads, scratch files or precomputed results.

Local checks on the Apple Silicon host through OrbStack, Linux ARM64 containers,
seed 1, full scale (3,100,000 lines / 238,836,234 input bytes):

| Version | `check.py` elapsed (s) | Output SHA-256 |
|---|---|---|
| Unchanged Python reference | 11.63 | `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3` |
| Rust baseline | 0.52 | `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3` |

These are single local runs, including container startup, about 22x apart. The
Rust runtime image is approximately 30 MB. A separate native macOS release run
took 0.29 s with 40,534,016 bytes maximum RSS (`/usr/bin/time -l`). RSS is not the
competition's cgroup `memory.peak`, and these figures do not predict its score.

The offline Docker build and the unprivileged, read-only, network-disabled run
pass `check.py`. A separate `docker build --platform linux/amd64 --network=none`
also succeeds; running that image under OrbStack's x86-64 emulation with the same
sandbox limits matches the full output hash. No emulated timing is used above.
Rust container outputs also match the published seed-1 tiny and sample hashes:

| Scale | Output SHA-256 |
|---|---|
| tiny | `c24fa507c963eaeac9224fd8435019b4f4e8c002ece7e32f316681a3957eb92f` |
| sample | `ca6311bbeb536fa55d85265bf6fd3d24cc9586f13de1ac73bd2a23ac3893aac3` |

All six differential tests pass against the unchanged Python reference, with
specific coverage for percentile rounding, equal ranks, zero and uint32-max
latencies, duplicate requests, hour and month boundaries, endpoint byte order,
all legal statuses and three randomized seeds. `cargo fmt --check` and Clippy
with warnings denied pass.

## Step 2: parallel mapped parsing (2026-10-30)

Replace the serial line reader and BTreeMap lookup with read-only mmap,
newline-aligned chunks, 72 direct hour buckets per worker, borrowed endpoint
keys, eight-byte delimiter scanning, and a sampled hash with full-key equality.
Merge groups and select/format percentiles in parallel by hour. The container
build now targets the scoring CPU using the organisers' supported native build
route. There are still zero runtime dependencies.

In sequential warm native comparisons on this host, the original baseline took
about 306 ms and the parallel map version about 29 ms end to end. A separate
compact-record/count-and-scatter experiment reached about 25 ms, but used about
53 MB native physical footprint versus 47 MB for maps, plus more source lines;
retain maps for the first live measurement. These are local diagnostics, not
cgroup memory measurements or board timings.

Nine differential tests pass, including deliberate sampled-hash collisions.
Full seed 1 matches the published hash and full
seed 42 matches the independent Python reference:
`b148ea775ca3fe99edb1329adfc5e85f75f9a67c9dffc37a2caff92d3bbdf9b1`.
Offline ARM64 and x86-64 container checks pass.

Live-board run `run-595acb9bbcc2`, commit
`d13d98c971a3754b5838168497e78340723ca0db`, completed at 2026-10-30 15:59:01 UTC:
all gates passed, T=0.048674 s, M=41,299,968 bytes, S=235, D=1,
K=29.120964, points=73.210288. This exceeds the requested 69.413 target,
improves runtime by 14.50x over step 1, and slightly reduces peak memory.
Build time was 7.894 s. The final holdout scoring run remains separate.

The additional collision test uses distinct 100-byte endpoints with identical
sampled first/middle/last words. Partition tests include inputs around the
64 KiB worker threshold and large globally shuffled groups. They were added
after the measured commit without changing its production implementation.

## Step 3: hour ownership (2026-10-30; board measurement pending)

Replace overlapping worker-local groups and their merge with a parallel range
index followed by exclusive ownership of each hour. The index coalesces only
adjacent same-hour lines and retains separate ranges for arbitrarily late
returns. Workers claim hours dynamically, parse all ranges for one hour,
select percentiles, format output, and release latency groups before moving on.
The writer drains completed buffers in hour order. There are no new runtime
dependencies or unsafe operations.

The investigation's four interleaved full-seed-1 comparisons in local ARM64
Linux containers measured process time separately from container startup and
read peak memory from the cgroup:

| Local metric | Previous version | Hour-owned prototype |
|---|---:|---:|
| Median process runtime | 33.108 ms | 26.310 ms |
| Worst observed cgroup `memory.peak` | 39,763,968 bytes | 24,096,768 bytes |

That is about 20.5% less runtime and 39.4% less peak memory on this dataset.
These measurements are local diagnostics, not live-board results. The
investigation estimated A at about 77.61 points after the extra-code penalty;
that projection still needs a board run. Highly disordered inputs can require
one range per line, so the memory reduction is not a universal bound.

The prototype passed all nine existing differential tests, full seeds 1 and
42, and offline Docker sandbox checks. Integration adds a differential
regression for separated runs of the same hour, uneven hour sizes and missing
hours. The Python correctness oracle and container build settings are unchanged.
