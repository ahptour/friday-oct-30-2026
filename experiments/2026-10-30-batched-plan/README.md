# Combined remaining-plan candidate

The user requested all remaining concrete implementation candidates in one shot
and one push. This supersedes `plan.md`'s isolated-submission preference. The
untracked user plan and AGENTS.md changes are preserved separately.

- A: bounded initial output reservation and direct integer appends.
- B: inline query terms/operators, compact Boolean tests and u32 token IDs.
  Retain category-sized scratch and twelve workers.
- C: six parsing workers, pipeline depth 32 derived from one constant; retain
  short decimal conversion and country-major membership provisionally.
- D: six query workers, twelve parsing workers and concurrent CSR builds.

Each `*-baseline.rs` is the complete production source from `d34c3a4`;
each `*-candidate.rs` is the complete source submitted in this batch. No patch
files are used. `intel-baseline-d34c3a4.json` is the complete official response
generated 2026-10-30 23:36:21 UTC. Per-track PERF.md files record hypotheses,
recent and historical baselines, estimated source costs and break-even times.

The country-major-only C trial passed every official gate but lost 0.054954
points against the decimal-only historical best. It is a close unreplicated
result, not an established score gain. The combined C trial will not isolate
country layout, worker-count and pipeline-depth contributions.

Source review covers numeric output bounds, the frozen Boolean/phrase grammar,
ID ranges, exact phrase comparisons and duplicate handling, complete two-stage
pipeline drain, immutable membership slots and query result ordering. New,
unrun differential cases cover eight-term phrases, repeated matches, six-term
Boolean queries, equal inline name prefixes with colliding tails, 23/24/25 and
65,535/65,536 name lengths, short pipeline tails and all 64 quote alignments.
No local compile, execution, tests, benchmarks or Docker runs were performed,
following the user's explicit instruction. Official build and correctness are
pending; source review does not establish holdout correctness.

This completes implementation of the remaining concrete candidates. Selecting
winners, pairing the old B scratch baseline, and proving repeatability require
official evidence after submission. Parameter sweeps and the conditional larger
B hybrid-index rewrite are not represented as completed measurements or required
parts of this bounded candidate. A's optional phase-worker retuning is deferred:
its selected parser and hour ownership remain unchanged.
