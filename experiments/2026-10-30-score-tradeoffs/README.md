# Measured memory and graph experiments, 2026-10-30

Baseline: `e2db23906110cb7dad9b86a79c5cf80a94a94334`. These experiments
implement the handoff's C/B priorities and two independent D candidates. The
retained production changes are C's one-byte country IDs and smaller exact name
table, plus B's category-sized scratch arrays. A and D production are unchanged.
No PGO, dependencies, seed recognition or cached answers are added. Runtime
solutions are wholly contained in their track directories. This directory holds
research and rejected alternatives, not required runtime/build code.

## Measurements and decisions

Measurements are local Linux ARM64 through OrbStack on Apple Silicon, using the
track Dockerfiles' digest-pinned standard Rust image and native release flags.
They do not establish performance on the board's six physical Intel P-cores.
AMD64 emulation validates correctness only. Input prewarming runs outside the
measured cgroup; `/timer` reports process exec-to-exit time and cgroup
`memory.peak`, including its own small overhead. Timed runs use CPU threads
0–11, 8 GB memory with swap disabled, the board sandbox and no network. Variants
run sequentially in alternating forward/reverse order. No competing builds,
data generation or tests run during a series. Every series retains trial 0;
warm summaries exclude that first trial for each variant and take maximum warm
memory, never average memory. Trial 0 is not necessarily cold input: prewarming
also precedes the first measured run.

| Experiment | Raw samples | Warm median / largest warm peak | Decision |
|---|---|---|---|
| C baseline | `arm64-samples.json` | 50.804 ms / 18,616,320 B | Comparison |
| C u8 lookup | same | 50.094 ms / 14,721,024 B | Useful |
| C u8 + half table | same | 51.313 ms / 9,449,472 B | Retain |
| B 12 / 8 / 6 / 4 workers | same | 239.745 / 278.022 / 354.494 / 498.377 ms | Retain 12 locally |
| D baseline | same | 164.619 ms / 138,682,368 B | Retain |
| D wide degree-two contraction | same | 490.046 ms / 135,430,144 B | Reject |
| D text-offset CSR | same | 480.513 ms / 87,232,512 B | Reject |
| B baseline, repeated | `b-scratch-samples.json` | 253.523 ms / 69,140,480 B | Comparison |
| B compact scratch arrays | same | 256.141 ms / 65,593,344 B | Retain; tiny estimated score gain |
| D baseline, repeated | `d-guarded-samples.json` | 175.366 ms / 137,347,072 B | Comparison |
| D guarded degree-two contraction | same | 414.562 ms / 135,262,208 B | Reject |

The first seven-trial C/B/D series prewarms before each track series. The later
nine-trial B and seven-trial guarded-D series prewarm immediately before every
run. This follow-up was necessary because the first nine-trial B scratch series
(`b-scratch-initial-samples.json`) had a 323 MB phrase-only memory peak,
consistent with input-page charges. Keep that row and the full series; do not
silently trim it. Its runtime/memory results are not the selection evidence.

The original B worker series likewise retains a 286,601,216-byte first peak for
twelve workers, versus a maximum of 69,066,752 bytes in its retained warm trials.
Linux cgroup memory accounting includes page cache, and memory is charged to the
cgroup that instantiates it ([kernel documentation](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html#memory-ownership)).
Input-page charging or VM cache behavior could explain these peaks; the samples
do not record `memory.stat`, so that cause is unproven. Prewarming once per track
is insufficient evidence of comparable accounting for a close decision. Keep
these historical rows intact; current scripts prewarm before every measured run.

These are local diagnostics, not the final scoring estimator. The
[official harness](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/harness/run_submission.py)
prewarms input outside the measured run cgroups and reads a dedicated parent
cgroup's `memory.peak` after container teardown. Its final five-run batch uses
the mean of the middle three times and the maximum of all five memory peaks;
it does not discard the
first memory peak. Our local timer reads its container cgroup before teardown,
and these tables use warm medians. Use the official procedure where practical
for future close decisions, retaining all samples and this distinction.

Use the capped scoring formula, not time alone. All selection comparisons are
above the memory-benefit threshold. With unchanged source, C's selected variant
estimates +2.837 local machine points. The B scratch time/memory estimate is
+0.117 before about seven added formatted code lines (estimated net +0.075).
B worker variants lose 0.856 / 2.913 / 6.096 local points against twelve workers.
The D alternatives lose at least 9–12 points before their added source cost.
Local source-cost estimates were approximate because the static-tools image
was unavailable; the completed board results supply official S values.
See track PERF files for the full tradeoffs and actual board run IDs.
`baseline-board.json` preserves only our team's public baseline feed entry.
`board-a7abacb.json` preserves our team's public feed entry generated at
2026-10-30 21:30:21 UTC. C's selected layout scored 67.659457 at `93b8a1b` and
67.679161 in the unchanged-code repeat at `a7abacb`. B's scratch-array change
scored 61.460053 at `a7abacb`, 0.056733 below the preceding `e2db239` baseline
result despite a smaller peak. All gates passed. C's Intel score improvement is
confirmed; B's small local estimated gain has not been established on Intel.

## Reproduction

Run from the repository root. Docker, Rust and Python are required. Input data
is intentionally untracked. Obtain the unchanged official generators from
<https://github.com/hackathon-raptors/speed-demon-2026/tree/main/generators> and
generate full seed 1 for B/C/D under `data/{b,c,d}/full-seed1`. The scripts obtain
baseline files from git without switching or modifying the active worktree.

```sh
python3 experiments/2026-10-30-score-tradeoffs/run.py prepare
python3 experiments/2026-10-30-score-tradeoffs/run.py test --variant contract
python3 experiments/2026-10-30-score-tradeoffs/run.py test --variant guarded
python3 experiments/2026-10-30-score-tradeoffs/run.py test --variant offset
python3 experiments/2026-10-30-score-tradeoffs/run.py build
python3 experiments/2026-10-30-score-tradeoffs/run.py bench
```

`prepare` writes isolated build contexts under `.out/score-tradeoffs/`; `build`
uses offline Docker and builds the timer using the same pinned Linux Rust image.
`bench` now prewarms immediately before every run; expect environmental timing
variation from the historical samples. It checks every output hash and exactly
one output file. Benchmarks must run without competing CPU-heavy activity.

The additional B scratch experiment has its own source transformation and
benchmark. After the timer build above:

```sh
python3 experiments/2026-10-30-score-tradeoffs/b-scratch-prepare.py
for variant in base phrase both; do
  docker build --network=none -t speeddemon-b-next/$variant .out/b-next/$variant
done
python3 experiments/2026-10-30-score-tradeoffs/b-scratch-bench.py
```

`phrase` narrows only the phrase stamps/query representation. `both` also narrows
Boolean masks, while retaining original query IDs in aggregate results. The
selected production B is `both` plus comments and canonical formatting. The
selected production C is `c-half` plus explanatory comments. The official
Python correctness references are unchanged.

## Exactness and verification

The D prototype protects every supplied query endpoint, removes nonnegative
self-loops, coalesces parallel directed arcs by their minimum, and eliminates
only vertices whose incoming/outgoing **union** has at most two unique live
neighbors. It replaces each incoming/outgoing pair with an exact directed
shortcut, retaining direction and minimum weights. Existing adjacency slots
suffice because each neighbor can gain at most one replacement for the removed
vertex. Every shortcut expands to a real equal-weight walk; every original path
through a removed vertex has its replacement. Nonnegative cycles can be removed,
so protected endpoint shortest distances are invariant.

`d-contraction.rs` keeps rare wide weights in a sparse side map, with saturating
u64 distance arithmetic. `d-guarded-contraction.rs` checks every shortcut sum
first and skips the whole vertex if any exceeds u32, before changing either
adjacency. `d-text-offsets.rs` independently stores u32 byte positions in CSR,
selects a correct u64 variant for larger files, and borrows the mapping until
all query workers join. Its Rust tests cover offset-width boundaries and
identical narrow/wide decoding.

All three D candidates pass ten differential suites, including sparse endpoint
sets that actually permit contraction, reversed input orders, wide asymmetric
paths, zeros/cycles, parallel minima, protected leaves and disconnected cases.
They match the Python oracle on full seeds 1 and 42. C passes six differential
suites including all 240 country IDs, collision overflow and unknown regions,
and matches full seeds 1 and 42 on native ARM64 and sandboxed AMD64. B passes
seven suites including category-only and interleaved queries. Full holdout
hashes are in `holdout-hashes.json` and `b-holdout-hashes.json`.
