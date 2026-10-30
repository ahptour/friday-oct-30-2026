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

## Step 3: hour ownership (2026-10-30)

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

The board measured step 3 at commit `8528bde3cb3715b078d61524bc02d1ceb0cbe166`
in `run-1e5f8f64ce0c`: all gates passed, T=0.048427 s, M=20,844,544 bytes,
S=272, D=1, points=75.758840. The memory improvement transferred to Intel;
the local runtime improvement did not.

## Step 4: build-time PGO (2026-10-30; measured and reverted)

The offline build generates sample-scale seeds 7, 19 and 103 with a byte-identical
copy of the official generator. The allowlisted Rust image supplies matching
LLVM tools: compile with instrumentation, run all three seeds, merge profiles,
then rebuild with profile-use and missing-profile warnings. Both compilations
retain native CPU targeting and the release settings. Only the final executable
enters the runtime image; seed 1 is excluded from training.

Seven alternating trials per variant on local ARM64 Linux, pinned to CPUs 0–11,
excluded the first trial from warm summaries. Median process time was 29.240 ms
before and 29.530 ms after PGO; worst warm cgroup peak was 23,572,480 versus
25,944,064 bytes. This shows no local benefit.
All outputs matched the public full-seed-1 hash. These measurements do not
establish Intel performance.

All existing differential suites pass against the actual PGO runtime container.
Offline ARM64 and AMD64 builds succeed without missing-profile warnings; the
AMD64 sandbox run also matches the public full-seed-1 hash. Build scripts and
Dockerfiles count toward source size; unchanged official generators are exempt.
Raw local results are in `.out/pgo-experiment/bench.json` at the repo root.

Board run `run-7aa15ae36e57` at commit
`3b74c524ad473105b5bfa934855984fe98336eea` passed every gate:
T=0.050670 s, M=22,491,136 bytes, S=296, D=1, points=74.661645.
PGO lost 1.097195 points relative to hour ownership without PGO: runtime,
memory and source size all increased. Restore the previous Docker build.
The measured commit preserves the complete reproducible PGO experiment,
including the unchanged generators and training scripts.

## Persistent worker state (2026-10-30; not selected)

An isolated candidate retains each worker's exact endpoint dictionary, dense
groups, sorted endpoint IDs and latency capacities across hours, while clearing
counts and lengths after output. The hour label is read once per hour. It keeps
all indexed ranges and updates endpoint order when new keys arrive.

Twenty alternating warm native ARM64 macOS trials suggested 9.29% less time
(27.754 to 25.177 ms), but worst native footprint rose 17.1%. That footprint is
not cgroup memory. Subsequent Linux ARM64 Docker comparisons used nine runs per
variant and seed, identical limits and externally pinned input pages. Each warm
summary excludes trial 0 and retains all raw samples.

| Seed | Variant | Warm median (ms) | Worst warm peak (bytes) | Estimated point delta with source |
|---|---|---:|---:|---:|
| 1 | Baseline | 24.132 | 23,187,456 | 0 |
| 1 | Persistent state | 23.518 | 25,300,992 | -0.429 |
| 42 | Baseline | 25.310 | 23,306,240 | 0 |
| 42 | Persistent state | 25.226 | 26,230,784 | -0.808 |

The estimate includes S approximately 272 to 301 and unchanged D=1. Retained
endpoint unions and high-water capacities can grow on skewed/disjoint hours.
The local Linux runtime benefit does not repay memory and source costs; leave
production A unchanged. Ten differential suites, a supplemental single-worker
persistence fixture, tiny/sample/full seed 1, sample 42's unchanged Python
oracle, and Docker full seed 42 all pass. Reproduction and evidence are in
[`experiments/2026-10-30-compact-graphs/`](../experiments/2026-10-30-compact-graphs/).


## Step 5: single-pass compact records (2026-10-30; Intel trial)

Replace the full-file hour index and second parsing pass with one parse per
line into private endpoint dictionaries and per-hour chains of 16 KB blocks.
Ordinary records use four bytes (20 latency bits, 11 endpoint bits, one error
bit); exact side lists preserve larger latencies and IDs. AVX2 classifies the
64-byte window after each timestamp; long endpoints and the final tail remain
bounded. Merge/sort endpoint dictionaries once, map local IDs to global ranks,
and count/scatter each hour's latencies into contiguous percentile groups.

Release consumed block prefixes in batches after acquiring hour-completion
flags. One anonymous mapping per parser avoids allocation per block; unused
reserve pages remain untouched. No assumption bounds the disorder of hours:
late records remain in their hour chain and can postpone page reclamation.
The source design was informed by
[Cache Me's scored A implementation](https://github.com/manusingh090/speed-demon-2026-entry/blob/836c19e46966bcbe9e91e79883f148059402f1c9/track-a/main.cpp).

Baseline for the Intel comparison: historical best `run-e3bf174864d4` at
`420d664`, T=0.049090 s, M=19,177,472 bytes, S=272, D=1,
points=75.973214. Local runtime tests and benchmarks are explicitly skipped per
user instruction. Formatting and native/x86_64 Linux compilation checks pass.
The candidate's correctness gates, runtime, memory, counted source and score
await the official board. No gain is claimed in advance.
