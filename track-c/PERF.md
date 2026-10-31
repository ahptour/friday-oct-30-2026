# Track C: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 14.44 | 367 | 0 |

## Native Python evidence

These are local Apple M4 Max measurements using Python 3.13.7, not official
Intel runs. There is no live-board run ID for the optimized solver.
T is whole-process wall time, averaged after dropping the fastest and slowest
of seven runs. M is the highest maximum RSS from two separate memory runs;
decimal MB are shown. Each variant also receives one discarded warmup.

| Full seed | Variant | T (s) | M (MB RSS) | Estimated S | D |
|---|---|---:|---:|---:|---:|
| 1 | Pinned reference | 5.080 | 422.986 | 108 (published) | 1 |
| 1 | Inline aggregation | 3.390 | 395.821 | 48 | 1 |
| 1 | Shared exact names (selected) | 3.520 | 248.938 | 50 | 1 |
| 42 | Pinned reference | 4.962 | 408.797 | 108 (published) | 1 |
| 42 | Inline aggregation | 3.327 | 416.449 | 48 | 1 |
| 42 | Shared exact names (selected) | 3.422 | 218.186 | 50 | 1 |

Using each batch's measured local reference:

| Full seed | Local E (s) | Local K | Local machine points |
|---|---:|---:|---:|
| 1 | 2.847 | 1.336 | 6.285 |
| 42 | 2.662 | 1.365 | 6.762 |

Inline aggregation removes redundant validation and function calls while
retaining Python's standard CSV parser. Sharing exact name strings is measured
as a separate change: it adds a lookup and is slightly slower on both seeds,
but reduces retained name copies enough to improve the combined coefficient.
The inline candidate is retained with its sources and results.

Every oracle, warmup, timing and memory output matches a fresh reference
process from git commit `54a4eb2af132d507bd4f2e2e12f388f7e7d02142`.
Seed 1 additionally matches the frozen spec's published output hash.
The CLI differential tests cover hostile CSV, BOMs, exact name identity, wide
integer sums and signed cents. Source/input hashes, raw samples, local E/K/points,
per-axis contributions and runtime budgets are preserved in
[the experiment archive](../experiments/2026-10-31-tracks-bcd/README.md).

Local scores use the measured local reference, with estimated Python/Dockerfile
source size and zero runtime dependencies (D=1). RSS is not cgroup `memory.peak`.
No official Intel score, sandbox result or unseen-seed correctness is claimed.
