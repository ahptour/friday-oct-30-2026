# Bold four-track Intel experiment

The user requested another large combined change and authorized Intel evaluation
with rollback available. Baselines are complete production files from d900c19;
candidates are complete proposed sources. All archives stay outside track folders.

- A: single-pass compact records with the selected SIMD parser; 4 KiB blocks,
  immediate hour-owned page discard after scatter, unique-only endpoint sorting,
  reusable worker output buffers and ordered positional writes.
- B: per-worker adaptive sparse/bitmap inverted indexes, actual-frequency
  Boolean anchors and source-text KMP phrase verification after presence filtering.
- C: 32 mutex-owned name shards, committing batches of 64 records per worker;
  exact compact keys/tails and overflow maps; all oversized-mapping names use
  overflow consistently to avoid cross-representation double counting.
- D: reusable generation-tagged flat distance tables and full-u64 monotone
  radix queues, retaining twelve workers and packed CSR.

Immediate selected baseline, with all official gates passed:

| Track | Run | Time ms | Peak bytes | S | Points |
|---|---|---:|---:|---:|---:|
| A | run-fdc7f970b0d2 | 44.913 | 21,602,304 | 329 | 75.801948 |
| B | run-d615b350ea77 | 478.898 | 67,141,632 | 548 | 61.072919 |
| C | run-d4ed0d0a93de | 55.667 | 13,316,096 | 555 | 69.428812 |
| D | run-d93663499dcd | 327.403 | 97,128,448 | 361 | 61.364546 |

The full raw official response is `intel-baseline.json`, generated
2026-10-30T23:47:57Z. Track PERF.md logs record historical bests, hypotheses,
estimated source budgets and important allocation tradeoffs. Baseline repeats
remain distinct from historical bests.

Formatting and source reviews completed; A and B additionally received independent
reviews of their ownership/ordering and matching semantics. No compilation,
Docker, solver execution, local tests or runtime benchmarks were performed, as
instructed. Official build, correctness and performance remain pending. Passing
public-data checks will not establish final holdout correctness.

Compare complete measured scores per track. This batch combines several mechanisms
within each track, so a result does not isolate their individual contributions.
Preserve this complete experiment and restore clear losers after the Intel run.


## Completed official run and selection

All four tracks passed every official gate at f20c4a1, with measured static axes.
The complete feed generated 2026-10-30 23:55:20 UTC is `intel-f20c4a1.json`.

| Track | Time ms | Peak bytes | S | Points | Delta vs latest baseline | Delta vs former best |
|---|---:|---:|---:|---:|---:|---:|
| A | 31.596 | 23,285,760 | 488 | 78.010364 | +2.208416 | +0.720043 |
| B | 497.701 | 177,471,488 | 564 | 56.339675 | -4.733244 | -5.563253 |
| C | 54.888 | 13,746,176 | 540 | 69.533017 | +0.104205 | -0.178493 |
| D | 378.197 | 99,250,176 | 437 | 59.082506 | -2.282040 | -2.886951 |

Keep A: a 23.20% runtime reduction against the former best repays its memory
and source costs, gaining 0.720043 points. Restore B and D from their exact
baseline source files; both clearly lose. Keep C provisionally for an unchanged
repeat in that restoration push: it beats the immediate repeat slightly but
falls short of the historical best. The selected-combination repeat is pending;
no isolated-mechanism or final holdout claim is made.
