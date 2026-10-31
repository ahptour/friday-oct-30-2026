# Track B: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 56.35 | 2,200 | 0 |

## Native Python evidence

These are local Apple M4 Max measurements using Python 3.13.7, not official
Intel runs. There is no live-board run ID for the optimized solver.
T is whole-process wall time, averaged after dropping the fastest and slowest
of seven runs. M is the highest maximum RSS from two separate memory runs;
decimal MB are shown. Each variant also receives one discarded warmup.

| Full seed | Variant | T (s) | M (MB RSS) | Estimated S | D |
|---|---|---:|---:|---:|---:|
| 1 | Pinned reference | 22.100 | 2338.243 | 91 (published) | 1 |
| 1 | Query-driven index only | 19.916 | 1787.085 | 93 | 1 |
| 1 | Normalized token phrases (selected) | 11.949 | 1684.849 | 64 | 1 |
| 42 | Pinned reference | 22.025 | 2354.889 | 91 (published) | 1 |
| 42 | Query-driven index only | 20.196 | 1697.841 | 93 | 1 |
| 42 | Normalized token phrases (selected) | 12.101 | 1738.899 | 64 | 1 |

Using each batch's measured local reference:

| Full seed | Local E (s) | Local K | Local machine points |
|---|---:|---:|---:|
| 1 | 10.481 | 1.452 | 8.100 |
| 42 | 10.718 | 1.433 | 7.820 |

The selected solver avoids indexing unused terms, repeated phrase tokenization
and sorting complete result sets. These changes are bundled; the comparison
does not attribute the whole difference to one mechanism. Repeated input queries
also reuse their formatted answers.

Every oracle, warmup, timing and memory output must match a fresh reference
process from git commit `54a4eb2af132d507bd4f2e2e12f388f7e7d02142`.
Seed 1 additionally matches the frozen spec's published output hash.
Source and input hashes, raw samples, local E/K/points, per-axis contributions
and runtime budgets are preserved in
[the experiment archive](../experiments/2026-10-31-tracks-bcd/README.md).

Local scores use the measured local reference, with estimated Python/Dockerfile
source size and zero runtime dependencies (D=1). RSS is not cgroup `memory.peak`.
No official Intel score, sandbox result or unseen-seed correctness is claimed.
