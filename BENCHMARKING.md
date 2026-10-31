# Native Python benchmarks

Run the Python tracks directly on this Mac. The runner uses only Python 3.11+,
git, and `/usr/bin/time`; it does not use Docker or install packages.

```sh
# Quick correctness/tooling check using existing tiny data:
python3 bench.py d --data data/d/tiny-seed1 --seed 1 --scale tiny

# Full-sized selection evidence (run serially):
python3 bench.py d --data data/d/full-seed1 --seed 1 --scale full --runs 10 --output-dir .out/python-d-before
python3 bench.py d --data data/d/full-seed42 --seed 42 --scale full --runs 10
```

Substitute `a`, `b` or `c` and its data directory for other tracks. Data is not
tracked. If it is absent, use the organizer's public generators, for example:

```sh
python3 /path/to/speed-demon-2026/generators/d_traverse.py --seed 1 --scale tiny --out data/d/tiny-seed1
```

The local organizer checkout, when present, is `.out/setup/harness/`. No part of
the runner depends on that ignored checkout. The oracle comes from initial git
commit `54a4eb2af132d507bd4f2e2e12f388f7e7d02142`; keep that history available.

For each batch the runner:

1. Archives the candidate folder and untouched reference from the pinned commit,
   excluding `.git`, `__pycache__`, `.pyc` files and stale Rust `target/` artifacts.
2. Hashes every input and archived source file, and the Python executable.
3. Executes the reference once to establish the expected output. `--expected HASH`
   additionally checks it against an independently trusted hash for these inputs.
4. Runs both variants in fresh processes, warming input pages before each run and
   alternating order each round. Every output must hash identically, including
   warmups and separate memory runs. A timeout, nonzero exit or mismatch fails
   the batch and preserves a failed report without a passing score.
5. Drops declared warmups and, with at least five timings, the single fastest and
   slowest samples. T is the mean of the remainder. At five runs this is the
   official time aggregation rule. Fewer runs use a plainly labeled mean.
6. Measures memory separately with `/usr/bin/time -l` on macOS (`-v` on Linux).
   M is the highest maximum RSS across `--memory-runs` (default 1). macOS peak
   footprint remains a separate field in each raw sample; it is not used as M.
7. Checks input/snapshot/interpreter hashes again, computes E/K/clamped points
   against the **measured local reference**, and reports per-axis point changes
   and break-even/+5-point runtime budgets (`--gain` changes the target).

Every batch gets a fresh `.out/python-bench/<track>-<timestamp>/` directory by
default. An explicit `--output-dir` must not already exist. It contains
`report.json`, complete `reference/` and `candidate/` snapshots, and per-process
outputs, stdout/stderr and raw memory reports. Failed batches retain evidence.
Archive important reports/snapshots under `experiments/` before cleaning `.out/`.

To try an archived candidate, copy the full production track into an experiment
folder outside `track-*`, edit that copy, and pass `--track-dir`:

```sh
python3 bench.py d --track-dir experiments/d-query-candidate --data data/d/full-seed1 --seed 1 --scale full --runs 10 --output-dir .out/python-d-after --baseline .out/python-d-before/report.json
```

The candidate must retain the track module's CLI (`--data DIR --out DIR`) and
its `.speed-demon.toml` declaration, or pass `--dependencies N`. The runner calls
that CLI directly, with imports and all input-derived preprocessing inside T.
It does not invoke the container's `/data`/`/out` entrypoint. A prior report must
have matching input hashes, reference source, machine, interpreter and measurement
method. Its old candidate metrics are rescored against the current local
reference. This is a **separate-batch comparison**, so repeat marginal results;
it does not establish an isolated causal effect.

`PYTHONHASHSEED=0`, bytecode writing disabled, `TRACK` and `LC_ALL=C` are fixed
for both variants. Stop competing benchmarks/builds before starting. Run the
command with a particular interpreter (for example `python3.13`) to select it;
`python3` here currently means Python 3.15.1, whereas the board uses 3.13.

## What local K means

The arithmetic in `score.py` implements the published caps and point clamp
exactly. The runner's **inputs** are local estimates: native spawn-to-exit wall
time includes process launch overhead; RSS is not Linux cgroup `memory.peak`;
S is estimated or supplied; D is a declaration, not a manifest audit. There is
no sandbox/official 8 GB gate validation. Neither an unchanged reference's noisy
K near 1 nor a native improvement predicts a specific Intel score.

Sref is 56/91/108/43 for A/B/C/D. Candidate S defaults to unformatted Python token
code lines plus noncomment Dockerfile lines; exact copies of the same track's
published Python reference are exempt. This estimate excludes docstrings,
comments and blank lines but does not reproduce `ruff format`/`scc`, registry
exemptions or arbitrary other languages. Unsupported file types require an
explicit `--source-lines N`. Pass verified official S with `--source-lines N`
and direct runtime dependency counts with `--dependencies N` when available.
S=0 is allowed; D=1 means no runtime dependencies. Adding one sets D=2.

Use full seeds 1 and 42 and at least 7–10 runs for decisions. A tiny run establishes
that the path works, not a full-scale speedup. Preserve exact fallbacks and use
independent differential/edge-case tests before promoting a rewrite. Every timed
batch checks the dataset used; it is not a proof about the unseen scoring seed.

## Calculate budgets or overall scenarios from recorded axes

`score.py` also works without executing a solver. Each metrics JSON is a plain
object with `T` in seconds, `M` in bytes, `S` lines and `D` dependency weight:

```json
{"T": 0.246987, "M": 108370000, "S": 457, "D": 1}
```

```sh
python3 score.py --reference ref.json --baseline old.json --candidate candidate.json --gain 5
python3 score.py --reference ref.json --candidate candidate.json --track d --machine-points points.json --judges judges.json
```

`points.json` and `judges.json` map track letters to scores between 0 and 100,
for example `{"a": 77.892, "b": 61.527, "c": 69.780, "d": 63.181}`. Provide a
judge value for every included track. With judges, the calculator uses
Q=0.65P+0.35J and reselects the best three before/after. Without judges it labels
the aggregate **machine-only**. Below three tracks the aggregate is unavailable.
The supplied points are the before-scenario snapshot; candidate points replace
`--track` for the after scenario. Keep those measurements coherent; do not mix
local axes with official reference axes and present the result as measured K.

The native runner leaves the aggregate unavailable because one track's local
batch does not supply coherent other-track scores or judge assumptions. Official
`PERF.md` claims still require actual live-board run IDs.

Run the arithmetic and harness regression checks with:

```sh
python3 -m unittest discover -s tests -v
```
