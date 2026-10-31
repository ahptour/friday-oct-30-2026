# Native performance experiments

Production at the start of this session: `1bc5fb6`. Native host: Apple M4 Max,
16 cores, 48 GiB RAM. No Docker was used. Local results screen mechanisms;
official Linux Intel measurements determine contest value.

The `*-base.rs` files preserve complete production baselines. Other complete
sources are isolated candidates or diagnostics. File existence is not a test
result. The earlier screen JSON files were committed at `1bc5fb6`; detailed
historical status remains in AGENTS.md and each production PERF.md.

## Latest selection

| Mechanism | Outcome |
|---|---|
| C static classifier | Reject: native compiler already specializes baseline; flat timings, more source |
| D radix queue with identical tie ordering | Reject: about 9% slower on both seeds, greater footprint and source |
| B converted 16-byte dictionary | Reject: marginal timing and no footprint improvement |
| B directly built 8-byte dictionary | Archive: slower than 16-byte variant on both seeds in the shared screen |
| B directly built 16-byte dictionary | Selected locally for Intel trial after two batches; official benefit pending |

The selected complete file is `b-shortkeys-direct.rs`. Production Track B matches
it exactly. Native screen medians were 171.297 -> 165.386 ms / 169.304 ->
165.792 ms on full seeds 1 / 42. Independent repeat medians were 169.935 ->
165.673 ms / 173.914 -> 167.021 ms. The candidate was faster in 39/44 timing
pairs. Raw variability and separate RSS/footprint samples are preserved, not
collapsed into an Intel score. Source estimate S541 -> S566 requires 1.35%
less time at unchanged official memory. See track-b/PERF.md for full limits.

Correctness: B passed eight existing suites and two new boundary suites against
the unchanged Python reference, then passed those suites again from production.
The long-token prefix mutation failed both new suites. C passed 12 existing
suites and one scanner-oracle test covering 16,384 offset/tail combinations;
an isolated missing-newline mutation failed that oracle. D passed 11 existing
suites and one monotone-queue oracle with 160,000 mixed operations plus drains;
an isolated node-ID mutation failed that oracle. Full seeds 1 and 42 matched
previously verified reference outputs on every benchmark execution. This is
neither a fresh full-reference run nor holdout evidence.

## Reproduce

```sh
python3 experiments/2026-10-31-native-first/run.py build b-base b-shortkeys-direct
python3 experiments/2026-10-31-native-first/run.py test b-shortkeys-direct
python3 experiments/2026-10-31-native-first/test_b_shortkeys.py .out/native-first/b-shortkeys-direct/target/release/solution
python3 experiments/2026-10-31-native-first/run.py bench b-base b-shortkeys-direct --seeds 1 42 --trials 10 --report another-batch.json
```

Finish builds/generation before benchmarking. Timing order is randomized and
serial, with two excluded warmups and three separate Mac time -l memory runs.
Inputs are full-sized, and the existing oracle files must be independently
verified. `expected-hashes.json` records published seed-1 hashes and previously
verified seed-42 hashes. The current runner records source/executable/input
hashes, exact build command and compiler, and reference-output provenance.
Earlier reports lack some of those fields; do not retroactively infer them.

`b-phase-results.json` is instrumented diagnostic evidence, not selection timing.
`c-codegen.json` records assembly generation flags, source/assembly hashes and
excerpts; disposable assembly files and objects are under `.out/native-first/`.

`score-budget.json` was calculated using the actual frozen harness scoring.py,
published reference calibration and the complete `intel-1bc5fb6.json` board
snapshot. It separates latest runs from historical bests, records memory-cap
floors and hypothetical score targets, and does not substitute Mac footprint
for official memory. Source counts for unsubmitted candidates remain estimates.
No commit or official submission was made in this session.
