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


## Step 5: single-pass compact records (2026-10-30; measured and reverted)

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
The Intel trial `run-4932f1d71ccd` at `330dbc3` passed every gate, measuring
T=0.045323 s, M=25,317,376 bytes, S=465, D=1, K=30.043006,
points=73.887169. Relative to the historical best, time adds 0.866860 points,
memory costs 1.206273 and source costs 1.746630: net **-2.086045 points**.
At this memory/source cost it needed T below 0.037400 s to break even.

The immediately preceding unchanged-production run at `a30d6d1`,
`run-5320357f613d`, measured 0.056568 s, 20,905,984 bytes, S=272 and
74.058976 points. The candidate also loses 0.171807 points against that run.
The variation between baseline runs cautions against treating a single
comparison as a precise causal estimate. No run demonstrated a score gain.

Restore the previous production source and preserve the candidate as
[`a-compact.rs`](../experiments/2026-10-30-rejected-pipelines/a-compact.rs).
[Raw Intel evidence](../experiments/2026-10-30-rejected-pipelines/intel-330dbc3.json)
records the successful gates and measured axes. The architecture did not reach
the hoped-for 30 ms target in this implementation.


## Step 6: isolate the Intel parser (2026-10-30; selected on Intel)

Retain the pre-step-5 hour ownership, endpoint maps, latency vectors and ordered
writer. Change only parsing: classify spaces/LF in a bounded 64-byte window,
decode the known-length latency with SIMD decimal arithmetic, and copy the hour
label once per range. Reuse the existing scalar delimiter scanner for the index,
long endpoints and short tails. There is no global packed-record buffer, block
allocator, dictionary reconciliation or extra output queue. The Intel-only
intrinsic path relies on the hardware contract; other targets use scalar parsing.

This isolates the parsing hypothesis from step 5's memory-lifetime and source
costs. Canonical local formatting estimates S=305 versus 272. At the historical
best baseline's unchanged memory, the candidate must run below **47.432 ms** to
break even; any memory increase tightens that threshold. The threshold is a
selection rule, not a speedup prediction. Native and x86_64 compilation checks
pass; no local runtime tests or benchmarks were run per user instruction.
Intel run `run-95a10bfac7c3` at `9e2af17` passed all gates:

| Runtime | Peak memory | Counted S | D | K | Machine points |
|---:|---:|---:|---:|---:|---:|
| 0.042205 s | 19,369,984 bytes | 305 | 1 | 34.990618 | 77.197581 |

The measured S matches the estimate. Relative to the historical best,
runtime falls 14.03%, memory rises 1.00%, and the net score improves
**1.224367 points**. Compared with the discarded compact-record candidate,
it is also faster (42.205 versus 45.323 ms), smaller (19.37 versus 25.32 MB),
and shorter (305 versus 465 counted lines). Select this implementation.

This is one public-data Intel measurement, not holdout validation.
[Raw result and restored-D repeat](../experiments/2026-10-30-rejected-pipelines/intel-9e2af17.json)
come from the official feed generated at 2026-10-30 22:50:48 UTC.


## Combined remaining-plan candidate (2026-10-30)

The user requested the remaining concrete candidates together in one push,
superseding the plan's separate-submission workflow.

Replace generic integer formatting in `format_hour` with a twenty-byte
stack decimal converter, emitting zero explicitly through the same loop and
preserving all u64/u32 values. Reserve at most 64 KiB initially per hour, based
on group count; the output grows normally as needed. Keep endpoint byte ordering,
percentile ranks, error-ppm arithmetic, hour indexing, parser and twelve-worker
hour ownership unchanged. This tests output work without reviving compact records.

Immediate official baseline at `d34c3a4`, `run-feb3a00d8e4c`:
43.964 ms, 21,929,984 bytes, S305, D1,
76.215158 points. Historical best: 77.197581 points
at 42.205 ms / 19,369,984 bytes / S305.
Estimated S329 (local formatted source-line delta, not official S) requires
below 42.976 ms at the recent baseline's memory, or 41.257 ms
at the historical best's memory. Actual peak memory is unmeasured; evaluate
the full measured score after the official run.

Formatted and source-reviewed. Per user instruction, no local compilation,
solver execution, tests, benchmarks or Docker runs were performed. Official
compilation, correctness and performance remain pending. Complete pre-change
and candidate files and the raw baseline board response are in
[`experiments/2026-10-30-batched-plan`](../experiments/2026-10-30-batched-plan/).


### Completed combined Intel result and selection

`run-433a18fd0c5d` at `2ab1dd9` passed every check, build, correctness,
timeout and sandbox gate; static axes are measured. Official response generated
2026-10-30 23:42:24 UTC:

| T (ms) | M (bytes) | S | D | Machine points |
|---:|---:|---:|---:|---:|
| 41.140 | 19,095,552 | 329 | 1 | 77.290321 |

Score change: **+1.075163** against the immediate
`d34c3a4` repeat and **+0.092740** against the preceding
historical best.

Retain direct integer appends and the capped initial reservation. This is
a new historical best, but the gain is only 0.092740 points; one run does not
establish a stable improvement. Keep the candidate pending repeat evidence.

Complete candidate sources remain archived with
[intel-2ab1dd9.json](../experiments/2026-10-30-batched-plan/intel-2ab1dd9.json).
The selected combination keeps A and restores B/C/D as described. No local
builds, solver executions, tests, benchmarks or Docker runs were performed.
Restored-combination official results remain pending; public-data gate success
is not evidence of final holdout correctness.


## Bold candidate: compact records with immediate page release and positioned output

The user requested a substantial new experiment in one combined Intel push.
Revisit the rejected `330dbc3` representation with concrete changes to its
known excess work and allocation overlap. Preserve the prior selected source
in `experiments/2026-10-30-bold/a-baseline.rs` and the complete new candidate in
`a-candidate.rs` there.

Parsing now uses the selected bounded SIMD field/latency routines in one pass,
retaining four-byte packed records when latency < 2^20 and local endpoint ID <
2048; wide pairs preserve every other latency/ID. Per-hour chains preserve
arbitrary disorder. Blocks shrink from 16 KiB to 4 KiB with eight header bytes
(1,022 packed records), cutting partial-block slack. Unique endpoints are
hash-deduplicated before sorting; local-to-global translation uses exact hash
lookups instead of sorting all duplicated worker name lists and binary searches.

Each hour has one consumer. After its second/scatter read of each part, it
immediately discards that part's whole hour-owned pages, coalescing contiguous
blocks. Other hours never read those blocks; no shared completion flags or
prefix reclaimer are needed. Capture links before discarding. Linux x86_64 uses
its 4 KiB base-page granularity; other targets retain the mapping until join.
Formatting uses only copied latencies, prefix sums, error counts, labels and
input-backed endpoint strings. Wide exception vectors remain live until join.

Each worker owns one reusable output buffer. The preceding hour publishes its
ending offset with Release; the successor acquires that offset and writes with
FileExt::write_all_at. Publish the successor offset before I/O so an I/O error
cannot leave another worker waiting forever; join propagates write failures.
A buffer is reused only after its write finishes. Missing hours publish the
same offset, including chains of empty hours. This removes queued output
backlogs and preserves exact chronological/bytewise output order.

The immediate selected repeat at `d900c19`, `run-fdc7f970b0d2`, passed all gates:
44.913 ms, 21,602,304 bytes, S329, D1, 75.801948 points. Historical best is
41.140 ms, 19,095,552 bytes, S329, 77.290321 points (`run-433a18fd0c5d`).
Estimated S488 from formatted source delta requires approximately
35.880 ms at an assumed 20 MB peak to beat
that best. Neither memory nor speed is measured yet. This is a sizable source
cost, justified only if the reduced passes and overlap buy enough throughput.

Source review covers minimum-line arena sizing, record bit layouts, scatter
counts, independent block ownership, page alignment, captures before discard,
empty hours, buffer ownership and write-error propagation. No local compilation,
solver execution, tests or Docker runs were performed, per user instruction.
Formatting is checked; official build/correctness/performance are pending.


### Completed bold Intel result

`run-d942851a76aa` at `f20c4a1` passed every check, build, correctness,
timeout and sandbox gate; static axes are measured. The full official feed
generated 2026-10-30 23:55:20 UTC is archived in
[intel-f20c4a1.json](../experiments/2026-10-30-bold/intel-f20c4a1.json).

| T (ms) | Peak bytes | S | D | Machine points |
|---:|---:|---:|---:|---:|
| 31.596 | 23,285,760 | 488 | 1 | 78.010364 |

Score delta against the immediate d900c19 repeat: **+2.208416**.
Against the previous historical best: **+0.720043**.

Keep the new compact-record architecture. Runtime is 23.20% below the
preceding historical best (41.140 ms), while peak is 21.94% higher and source
grows from S329 to S488. The complete score still gains 0.720043 points:
+2.865805 runtime -0.861582 memory -1.284179 source. At the measured memory and
source, break-even was 33.762 ms; 31.596 ms clears that budget. This validates
the combined candidate on public Intel input, not any single mechanism in it
or final holdout behavior. The next selected-combination push repeats this A.

All complete candidate and baseline sources remain under
`experiments/2026-10-30-bold/`. No local builds, Docker, solver runs, tests or
benchmarks were used; the official Intel run supplied execution evidence.
