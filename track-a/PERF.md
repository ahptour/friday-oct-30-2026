# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Rust std-only streaming parser, byte-prefix hours, buffered output, exact percentile selection | Pending | | | |

Step 0 contains the organisers' published scoring-box reference figures. Step 1
has no live-board run yet; its scoring columns remain blank until an actual run
ID is available. No local timing is a board result.

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
