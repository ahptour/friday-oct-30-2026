# Compact graph and persistent-state experiments, 2026-10-30

Baseline: `00f82d78`, whose production code matches `420d664`. Select only D's
four-byte tagged adjacency for the next Intel trial. A remains unchanged.
Exact alternatives are Rust sources outside runtime folders; no dependencies, PGO,
worker counts, compiler flags or seed-specific answers are added.

The intended scoring tracks remain A/C/D, with B as insurance. This local D
selection does not establish that D has overtaken B on Intel; that requires a
board measurement. The next memory target is query scratch, not another change
to construction lifetimes alone.

| Candidate | Seed 1 estimated point delta | Seed 42 estimated point delta | Decision |
|---|---:|---:|---|
| D tag bit + full-width vector, weights 0–1023 inline | +1.630 | +2.033 | Select |
| D 0–2046 inline + slot-keyed exception map | +1.498 | +1.550 | Preserve alternative |
| D forward → release raw edges → derive reverse | -2.903 | -2.877 | Reject locally |
| A persistent dictionary/groups/order/capacities | -0.429 | -0.808 | Reject locally |

These are local ARM64 Linux Docker estimates, not official points. Source
estimates include canonical local Rust plus the unchanged Dockerfiles:
A 272→301, D 336→361/366/379; D=1 throughout. Ratios are below memory's cap
and above source's floor. Deltas use `25*log10(T_old/T_new) +
10*log10(M_old/M_new) + 7.5*log10(S_old/S_new)`. Candidate S and Intel gains
remain unmeasured. Full tables are in the A/D PERF logs.

Both D representations preserve full uint32 nodes/weights and uint64 distances.
The selected tag bit indexes a full-width vector; the other design reserves
weight 2047 and keys exceptions by CSR slot. A wide node also takes the exact
fallback. `graph-stats.json` records the full counts and payload calculations.

## Evidence and procedure

- `d-initial-samples.json`: complete original D series, including a 227,766,272 B
  baseline peak. Its excess is consistent with input charging, but no category
  breakdown was collected, so the cause is unproven. Not selection evidence.
- `arm64-seed{1,42}-samples.json`: all 108 subsequent Docker runs. Containers
  run sequentially on CPUs 0–11 with 8 GB/no swap, no network, a read-only root,
  1 GB tmpfs, uid 65534 and no capabilities. Pinned offline builds use Rust
  1.99.1. No builds, generators, tests or competing benchmark agents run during
  timing. Nine trials alternate forward/reverse variant order. Trial 0 is
  retained but excluded from each warm median/maximum; no other rows are cut.
- Input mappings are mlocked by a separate idle helper throughout each track's
  second series, preventing cache eviction; another container prewarms before
  every run. Only the helper gets IPC_LOCK/unlimited memlock. Solver images and
  measured cgroups are unaffected. `file_after`/`anon_after` are memory.stat
  counters after exit, not a decomposition of peak. Warm D file bytes are
  12,288; A's approximately 10.1 MB are its output.
- The local timer reads container `memory.peak` before teardown, including its
  small overhead. The official harness reads a parent's peak after teardown;
  final scoring uses the middle-three mean of five times and all-five maximum
  memory, rather than these warm medians. ARM64 does not establish Intel gains.
- `d-phase-memory.json`: separate instrumented runs, excluded from timing.
  Sequential construction peaks at 67.69 MB, drops to 27.58 MB after releasing
  raw edges, then holds 54.63 MB for both graphs. Query scratch/allocator memory
  raises peak to 94.81 MB. This explains why lower construction payload alone
  does not achieve the predicted final peak.
  `variants/d-phase-{base,escape11,reverse}/main.rs` preserve instrumented Rust
  sources separately from solver candidates. Copy a source into its context.
- `a-native-samples.json`: earlier macOS/Rust 1.98.1 experiment. Native footprint
  and RSS are not cgroup memory; this series is not selection evidence.
  `a-correctness.json` records A's Docker seed checks.

## Reproduce

Use the unchanged [official generators](https://github.com/hackathon-raptors/speed-demon-2026/tree/main/generators)
to generate full seeds 1 and 42 in `data/{a,d}/full-seed{1,42}`. Data is untracked.
Python, Cargo, Docker and the baseline revision
are required.

```sh
python3 experiments/2026-10-30-compact-graphs/run.py prepare --track all
python3 experiments/2026-10-30-compact-graphs/run.py test --track all
python3 experiments/2026-10-30-compact-graphs/run.py build --track all
python3 experiments/2026-10-30-compact-graphs/run.py bench --track all --seed 1 --pin-inputs
python3 experiments/2026-10-30-compact-graphs/run.py bench --track all --seed 42 --pin-inputs
```

Preparation reconstructs sources from git and saved Rust files without switching
the worktree. Tests load the unchanged Python oracle before selecting each
experimental binary. Builds are offline; the timer/warmer use the pinned Rust
image. Every benchmark checks the output filename and hash, saving all rows in
`.out/compact-graphs/`. Omit `--pin-inputs` for ordinary per-run prewarming and
keep that series separate from the pinned-input evidence.

All packed D candidates pass eleven differential suites, including packed-node
and weight boundaries, direction, parallel edges, zero cycles and wide answers.
All compared full seeds 1/42 match the unchanged reference's published or
previously verified hashes. Production D also passes formatting, Clippy with
warnings denied and offline Docker sandbox checks on both full seeds. A passes
ten differential suites and the seed/persistence checks described in its log.
No new Intel result is claimed.
