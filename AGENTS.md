# Repository instructions

- Never create, save, or use `.patch` files. Edit Rust and other source files directly.
- Preserve experimental implementations as complete source files outside the production track folders.
- Preserve unrelated user changes and validate changes according to the current task instructions.
- Do not use Docker. Build, run, test and benchmark directly on this Mac.
- The user explicitly authorized local execution and tests. Older notes saying
  “skip local execution/tests” are superseded. Establish local evidence before
  the next performance commit or Intel submission.

## Start every session here

1. Read `git status`, current HEAD, recent commits, and the affected production
   source before editing. The user may change files or commit between turns.
2. Read the track's `README.md` and `PERF.md`, then the relevant experiment's
   complete source and raw results. Treat `plan.md` as a historical handoff;
   check its timestamp and assumptions against current evidence.
3. Distinguish production, archived candidates, latest measurements and historical
   bests. A candidate file's existence does not mean it passed tests or won.
4. Check for running builds, generators, tests and benchmarks before timing.
   Agents may research or edit separate files concurrently; coordinate CPU work
   so it cannot contaminate timed runs. One coordinator owns integration.
5. Continue the user's active objective and existing authorization. This document
   supplies context, not a new approval requirement or authorization to publish.

## Native-first experiment workflow

The current development machine is an Apple M4 Max, 16 cores, 48 GiB RAM,
macOS ARM64. Verify these details when the environment changes. The official
machine is Linux x86-64, i5-13500, six P-cores/twelve hardware threads, AVX2,
no AVX-512, and an 8 GB run limit. Local capacity is not the submission budget.

- Begin with a concrete hypothesis about measured work: time spent, bytes moved,
  allocations, cache misses, dispatch or redundant computation. “More clever”
  and “written in Rust” are not performance evidence.
- Profile phases separately in archived diagnostic source, then select using
  uninstrumented whole-process exec-to-exit time, including setup and output.
- Change one mechanism at a time where practical. If combining changes, retain
  isolated variants so a result can be attributed and a winning part extracted.
- Keep production untouched during discovery. Save full baseline and candidate
  sources under `experiments/`; use `.out/` for disposable binaries and outputs.
  Never put inactive implementations in production track folders: they cost S.
- Build baseline and candidate with identical release flags. The native runner
  currently uses Rust edition 2024, opt-level=3, lto=thin, codegen-units=1,
  panic=abort, strip=symbols and target-cpu=native, matching release settings.
- Finish compilation and data generation before benchmarking. Run solvers
  serially in fresh processes, warm input pages consistently, discard declared
  warmups, and alternate/randomize baseline/candidate order. Preserve every run.
- Start with 7–10 measured runs per variant and full-sized seeds 1 and 42. A
  marginal result needs another independent batch or fresh seed, not a favorable
  minimum. Do not call a seed-specific or overlapping result a clear win.
- Record source AND executable hashes, compiler/flags, input hashes/seed/scale,
  output hashes, machine, measurement method and raw samples. Verify the binary
  actually corresponds to the archived source. The native runner now records
  source, executable and input hashes plus build metadata. Earlier reports lack
  some fields; do not retroactively infer identities that were not recorded.
- Measure memory separately with macOS `/usr/bin/time -l`; label maximum RSS and
  peak footprint explicitly. Neither equals Linux cgroup `memory.peak`. Mapped
  input pages can dominate RSS; do not compare unlike metrics as memory savings.
- ARM and Intel execute different architecture branches. A Mac win caused by a
  scalar/NEON change may say nothing about AVX2. Inspect the actual target paths.
  Local results screen hypotheses; official Intel results decide submission value.
- Promote only after correctness checks and a credible full-score benefit. Keep
  losing sources/results and document rejection. Do not silently replace a known
  stronger implementation merely because the live board remembers its old score.
- Do not claim performance, tests, holdout correctness or causal attribution that
  the recorded evidence does not establish. No local breakthrough is preferable
  to an invented one.

## Correctness: verify the verifier

Keep the published Python reference independent and unchanged. Compare exact
output bytes, not approximate values or selected summary fields. Full-data hashes
and original tests complement targeted differential/property tests; neither alone
proves final holdout correctness.

- Build an oracle before a rewrite. Record whether expected outputs came from a
  fresh reference run, published hashes, or previously verified reference output.
  Replaying historical hashes is not a newly executed reference run.
- Confirm tests invoke the candidate binary, exercise the intended code path,
  compare outputs, and fail on mismatches. Track attempted, executed, skipped,
  errored and mismatching cases separately; exit 0 alone is insufficient.
- For a new harness or high-risk transformation, deliberately break an isolated
  copy and confirm the expected test fails. Never mutate production or the oracle
  to do this. Prefer targeted mutations over indiscriminate test proliferation.
- Bias generated cases toward the mechanism's boundaries, not only ordinary data:
  - A: coincident percentile ranks, ties, empty/small groups, optimization threshold
    ±1, full uint32 latencies and error-rate integer arithmetic.
  - B: ASCII-only tokenization, unknown intervening words, 32-byte tokens, every
    phrase-anchor position, repeated matches, document boundaries and Boolean order.
  - C: quotes split across 64-byte blocks/worker boundaries, escaped quotes,
    embedded newlines, CRLF/EOF, BOM rules, empty names, exact name equality,
    country-bit boundaries, hash collisions and overflow paths.
  - D: directed/disconnected graphs, zero cycles, parallel/self-loop edges,
    distances at and beyond compact-field limits, wide-to-small improvements,
    table growth/reset/epoch wrap, arbitrary node IDs and chunk tails.
- Include state in the model: worker count, repeated runs, reset/reuse, environment
  and architecture-specific paths. A generator only explores assumptions encoded
  in that generator. Fresh seeds do not substitute for missing input categories.
- Preserve exact fallbacks for wide values, crowded tables and unusual inputs.
  Observed distributions are optimization opportunities, not guaranteed bounds.

## Lessons from Port Mortem 2026

Reviewed the [organizer's previous-contest results](https://www.raptors.dev/project/code-resurrection-2026-port-mortem)
and primary repositories/write-ups. Its emphasis was behavioral equivalence;
Speed Demon's scoring is different. Transfer the engineering methods, not the old
scoring weights or another project's benchmark numbers.

- **Specialize a measured hot path once.** First-place 0xicbor moved byte-source
  selection into generic types. Its recorded mean Rust/C parsing-time ratio fell
  from 1.492 to 1.033 (lower is better), at a code-size cost. This is their reported
  measurement, not a promised improvement here.
  [Specialization commit](https://github.com/keirsalterego/0xicbor/commit/39e5be5)
- **Avoid computing discarded results.** Its structural skip scanner avoided
  constructing decoded values just to advance a cursor, retaining an exact
  fallback. The initial commit reported a mean ratio change from 0.995 to 0.399.
  [Scanner commit](https://github.com/keirsalterego/0xicbor/commit/8b71557)
- **Representation and call boundaries can reverse a win.** Its 24-byte parser
  state return regressed a deep-nesting benchmark versus filling an existing slot.
  Inspect assembly/call costs before assuming a prettier interface is cheaper.
  [Decision log](https://github.com/keirsalterego/0xicbor/blob/main/decisions.md)
- **Bind results to the actual executable.** Its runner checks linked artifact
  identity, checks equivalence and alternates timing blocks to spread drift.
  [Benchmark runner](https://github.com/keirsalterego/0xicbor/blob/main/bench/run.py)
- **A passing assertion may observe nothing.** cron-parser-go found four negative
  spy assertions passing because the Go engine bypassed the observed JavaScript
  methods. It connected tests to actual engine operations and mutation-tested the
  path. Its fuzzing also missed broken year arithmetic until operation coverage
  and leap-day cases were deliberately added.
  [Author's account](https://dev.to/aniket_3001/41088-divergences-and-four-tests-that-were-lying-to-me-169h),
  [fuzzing methodology](https://github.com/aniket-3001/cron-parser-go/blob/main/fuzz/README.md)
- **Large replay counts have a denominator.** The bignumber port's 139,907 matched
  calls do not mean every upstream assertion/API/error path was covered. Source
  inspection found capture after successful returns, replay skips, and printed
  divergences without a failing process exit. Audit coverage and failure signaling;
  do not imply that these limitations invalidate the reported matching calls.
  [Write-up](https://dev.to/prince_panchani_f971a20ec/139907-calls-later-what-it-actually-takes-to-port-a-numerical-library-from-javascript-to-rust-5c07),
  [capture code](https://github.com/PrinceXDev/port-mortem-bignumber-rust/blob/main/fuzz/capture_tester.js),
  [replay code](https://github.com/PrinceXDev/port-mortem-bignumber-rust/blob/main/src/bin/replay_captured.rs)
- **Ambient state can be absent from a fuzzer.** Another cron port always supplied
  an explicit timezone, missing default-timezone behavior; source inspection also
  exposed wall-clock-dependent DST resolution. Model omitted state explicitly.
  [First-person analysis](https://dev.to/sanjaysah/the-bug-that-differential-fuzzing-cannot-find-1gi4)
- **Repeatability outranks a flattering run.** JSBI's unchanged addition comparison
  moved from 1.75× faster to 1.09× to 0.95× across runs. The authors correctly
  called the comparison inconclusive. Fresh processes and interleaved repetitions
  help; report variability and remaining host confounders.
  [Benchmark account](https://dev.to/devanshkant/we-ported-jsbi-to-c-and-the-benchmark-taught-us-more-than-the-port-did-14c)
- **Measure the execution engine, not the language's reputation.** Picomatch's
  reported 18.2× slowdown was its direct native matching API with precompiled
  patterns, not merely IPC or compilation. The author suspected the regex engines;
  this was not an isolated causal measurement. Profile the actual hot work.
  [Benchmark report](https://github.com/LubuSeb/picomatch-mortis/blob/main/BENCHMARK.md)
- **Publish losses and their scope.** The sortedcontainers port reported faster
  indexing but slower membership, with cross-runtime comparisons as its explanation.
  Keep both wins and regressions in PERF.md; explain what was measured and what is
  inferred. Specific, reproducible failure accounts were valued by the judges.
  [Write-up](https://dev.to/yash_vardhanshukla_82115/the-test-suite-never-knew-it-was-running-on-rust-1ljj)

## Speed Demon selection rules

Check the [current rules](https://speeddemon.dev/#rules), frozen track specs and
scoring implementation when needed. No cached answers, seed recognition or oracle
changes. All input-derived preprocessing belongs inside the measured run.

### Optimize the score before implementing the idea

Turn K into a precise optimization target. Before a change, ask whether its
expected score gain justifies its runtime, memory, formatted-source and dependency
costs. The formula prices tradeoffs; profiling and experiments identify which
algorithms can achieve them. The active objective is winning the current contest.
A public article is a separate task and should not displace performance work.

Use the [published coefficient](https://speeddemon.dev/#coefficient), frozen specs
and actual `harness/scoring.py`. Reference measurements are fixed scoring anchors,
not the current leader's results. Refresh the official feed when choosing work,
preserve its timestamp/raw response, and keep latest runs separate from historical
bests. The final frozen commit is scored on an unseen seed; historical-best board
entries do not assemble the final submission for us.

### Exact adjusted runtime, including caps

K weights time 0.50, memory 0.20, formatted source 0.15 and dependencies 0.15.
For a fixed track and reference, define normalized costs:

```text
m = max(M / Mref, 1/50)
s = max(S / Sref, 1)
d = max(D / Dref, 1)

E = T * m^0.4 * s^0.3 * d^0.3
K = sqrt(Tref / E)
P = clamp(50 * log10(K), 0, 100)
```

Minimizing adjusted runtime E is exactly equivalent to maximizing K within that
track. The dependency floor is consistent with valid contest inputs: Dref=1 and
D=1 plus direct non-stdlib runtime dependencies. E has time units; keep units
consistent. Do not compare E across tracks with different references as though
it were the overall score. K can still improve above the 100-point clamp and
matters to the highest-K prize, even when machine points cannot increase.

For positive costs, equivalently minimize:

```text
T * max(M, Mref/50)^0.4 * max(S, Sref)^0.3 * D^0.3
```

Memory reduction earns nothing below Mref/50; source reduction earns nothing
below Sref; dependency weight cannot fall below 1. Published seed-1 reference
calibration gives these memory floors (decimal MB, not MiB):

| Track | Reference source lines | Memory reward stops below MB |
|---|---:|---:|
| A | 56 | 4.559667 |
| B | 91 | 43.999232 |
| C | 108 | 7.348716 |
| D | 43 | 17.858314 |

Recompute against the reference used for final scoring if its holdout calibration
changes. All four official peaks in the saved `1bc5fb6` snapshot exceed their
floors. Mac RSS and peak footprint are not official M: local reports must label
E/K/points as unavailable or explicitly hypothetical when official axes are
missing. Never mix Mac runtime/footprint with Intel reference axes and call it a
measured contest score. Estimated S must remain labeled until the official pass.

### Point exchange rates and break-even budgets

Before the point clamp, the exact change using normalized costs is:

```text
Delta P = 25 log10(Told/Tnew) + 10 log10(mold/mnew)
        + 7.5 log10(sold/snew) + 7.5 log10(dold/dnew)
```

P is the machine score. Apply the actual scoring function and clamp to both
implementations when reporting points. Final track deltas below assume unchanged
judge score and no point-clamp interference.

| Change, all other axes unchanged | Machine points gained | Final track points gained (65%) |
|---|---:|---:|
| Halve runtime | 7.526 | 4.892 |
| Halve memory without crossing its reward cap | 3.010 | 1.957 |
| Halve counted source, remaining at/above the reference | 2.258 | 1.468 |
| Remove the only runtime dependency, D2 -> D1 | 2.258 | 1.468 |
| Reduce runtime by 10% | 1.144 | 0.744 |
| Reduce counted source by 10%, above its floor | 0.343 | 0.223 |

Percentage changes determine these gains: 400 -> 200 ms and 40 -> 20 ms earn the
same time contribution. A 10% slower implementation that halves memory gains
about 1.975 machine points when the memory cap does not interfere.

Set the candidate's maximum break-even runtime BEFORE a substantial rewrite:

```text
B = Told * (mold/mnew)^0.4 * (sold/snew)^0.3 * (dold/dnew)^0.3
Improve K only if Tnew < B.

For a desired gain g in pre-clamp machine points:
Ttarget <= B * 10^(-g/25)
```

Use estimates to screen an idea, then replace them with measured axes. Include
all input-derived preprocessing and output in T. Seek a margin larger than the
observed measurement variability, not a barely positive point estimate. With
other axes unchanged, +5 machine points requires Tnew <= 0.630957 * Told.

| Proposed cost increase, above the relevant floors | Runtime reduction needed just to break even |
|---|---:|
| 10% more source | 2.82% |
| 50% more source | 11.45% |
| Twice the source | 18.77% |
| First runtime dependency, D1 -> D2 | 18.77% |
| Twice the memory | 24.21% |
| Twice the memory and twice the source | 38.44% |
| S541 -> estimated S566 | 1.35% |

A 20% runtime reduction cannot repay twice the memory by itself. Conversely,
20% less memory can tolerate 9.34% more runtime; 20% less source can tolerate
6.92% more runtime while above its floor. Dependencies are a priced choice:
an allowed dependency reducing counted source from 600 to 200 lines (both above
Sref), while changing D1 -> D2, gains about 1.321 machine points at unchanged
T/M. Verify library eligibility and track restrictions before applying that case.

### Recorded example: faster Track A implementation lost points

The compact-record experiment in `track-a/PERF.md` compares historical baseline
`run-e3bf174864d4` at `420d664` with `run-4932f1d71ccd` at `330dbc3`:

| Axis | Historical baseline | Compact-record candidate |
|---|---:|---:|
| T, ms | 49.090 | 45.323 |
| M, bytes | 19,177,472 | 25,317,376 |
| S | 272 | 465 |
| D | 1 | 1 |
| Machine points | 75.973214 | 73.887169 |

Time contributed +0.866860, memory -1.206273 and source -1.746630 machine
points, totaling -2.086045. At those M/S costs the candidate needed less than
37.400 ms to break even. The immediate baseline repeat differed materially;
this historical comparison does not isolate a precise causal effect. No recorded
comparison established a score win. Preserve the rejected implementation and
raw evidence rather than treating its faster runtime as sufficient.

### Choose work by its effect on the overall score

Final track total Q = 0.65 * P + 0.35 * J. The overall score is the geometric
mean of the best three FINAL track totals, not machine points alone. For a fixed
counting set of three positive totals:

```text
G = (Q1 * Q2 * Q3)^(1/3)
dG/dQi = G / (3 * Qi)
```

An extra point on the weakest counting track has greatest marginal value when
effort, risk and attainable gain are comparable. D is a preferred target under
those conditions while A/C/D are the counting tracks. Refresh that assumption;
an easier reliable A/C win can be better, and judge scores can change the set.
B remains a possible replacement: compare improving D with improving B enough
to enter the top three. A small gain on a fourth-place track may have no overall
effect. Report both fixed-judge scenarios and uncertainty rather than presenting
the top-three machine aggregate as the final result.

Use a coherent latest-commit snapshot to assess the current selection. A later
preserved feed, generated 2026-10-31T04:55:28Z (October 30, 21:55 PDT), records
all four latest runs at `1bc5fb6`; these supersede the earlier 21:42 PDT example
supplied in conversation:

| Track | T, ms | M, bytes | S | D | Machine points |
|---|---:|---:|---:|---:|---:|
| A | 37.351 | 23,330,816 | 488 | 1 | 76.185216 |
| B | 446.538 | 70,979,584 | 541 | 1 | 61.632993 |
| C | 54.787 | 13,881,344 | 540 | 1 | 69.510518 |
| D | 269.936 | 111,017,984 | 457 | 1 | 62.111556 |

The full feed and calculation are preserved in
`experiments/2026-10-31-native-first/intel-1bc5fb6.json` and `score-budget.json`.
These are dated observations, not permanent targets. At unchanged axes in that
snapshot, B needs about 427.283 ms to reach D's machine score. Separately, their
historical-best machine points were B61.902928 and D63.181152: the 1.278224-point
gap requires about 11.11% less B runtime at unchanged B axes. Do not mix the
historical-best scores with latest-run timings to calculate a budget. Final
holdout scoring uses the recorded frozen source, not either board-best mosaic.

For the earlier hypothetical D baseline 246.987 ms, a five-machine-point target
at unchanged axes is 155.838 ms. This is an illustrative budget, not a prediction.
Reducing A source from 488 to 350 lines above its floor gains about 1.083 machine
points at unchanged T/M/D. Source cleanup has value, but weigh it against changes
to time, correctness, clarity and verification effort.

A 10-point improvement in judge score adds 3.5 final track points. Matching that
through time alone requires about a 1.64x speedup at fixed other axes, away from
clamps. Preserve code quality, technique explanations and an honest `PERF.md`
with official run IDs/raw evidence. Comments and blank lines do not count toward
S; removing explanations does not reduce counted code. Do not trade correctness
or clarity for an unsupported tiny K gain.

### Required experiment report and selection process

Every candidate report should include, when the axes are available:

- Correctness results, case/seed counts, exact outputs and oracle provenance.
- T/M/S/D with units, machine, source/executable/input hashes, compiler/flags,
  raw samples and variability; distinguish measured, estimated and unknown axes.
- E, K and clamped machine points using the actual scoring function, plus each
  axis's pre-clamp point contribution and the break-even/target runtime.
- Change to the best-three aggregate under explicit judge-score assumptions,
  including whether the candidate actually enters the counting set.
- Evidence supporting selection, remaining uncertainty and official run IDs
  when available. Native evidence can select an Intel trial, not certify its K.

Use the formula to reject implausible expensive ideas before implementation.
Profile the intended architecture; inspect generated code before paying to
remove a source-level abstraction that the compiler already eliminates. Apply
Amdahl's law to phase targets: eliminating a phase consuming 10% of whole-process
time can save at most 10%, and accelerating it less saves less. Profile setup,
preprocessing, parsing, construction, queries and output as relevant.

Start with small independent mechanisms: worker count, buffer capacity,
representation, dispatch and redundant work. Retain competing sources until
measurements establish a tradeoff. A candidate is safely dominated on machine
axes only when another is no worse on every axis and better on at least one,
with equivalent correctness and credible measurements. Rank experiments by
expected improvement to final overall score per engineering effort, including
validation and unseen-input failure risk; a theoretical speedup is insufficient.

### Latest repository-ideas follow-up, refresh before acting

- Baseline was clean `1b85287`. Production B now matches archived
  `b-inline-clauses.rs`, retaining the short-key dictionary and inlining up to
  five Boolean operands with exact term-ID/polarity encoding. Screen medians
  on full seeds 1 / 42: 159.986 -> 156.343 ms / 159.327 -> 155.605 ms;
  independent repeat: 159.934 -> 155.570 / 159.945 -> 155.525 ms. Candidate
  won 41/44 pairs. Estimated S566 -> S572 needs 0.316% less time at equal
  official memory. Mac RSS/footprint are not official M; no inline-clause
  Intel score or overall-score gain is established.
- Selected candidate and production passed eight existing differential methods
  and two boundary methods. An isolated polarity mutation failed the exhaustive
  six-term operator-order test. Full benchmark/production bytes matched saved
  reference outputs for seeds 1/42, not fresh full-reference runs. Formatting,
  Clippy and Haswell code generation passed.
- B masking out previously matched OR documents was flat/slower. D pruning
  relaxations at the complete-route bound was about 2% slower; its diagnostic
  skipped only about 0.02% of relaxations. Both are rejected. D passed all eleven
  existing differential methods; production A/C/D remain unchanged.
- Refreshed board generated 2026-10-31T05:12:30Z has all latest runs at
  `1b85287`. Short-key B passed all Intel gates: `run-d5c09f62298a`, 441.470 ms,
  67,166,208 bytes, S566, D1, 61.849606 points. Latest A/C/D are 75.809715 /
  69.694784 / 62.910395. Equal judges still select A/C/D. Latest and historical
  bests remain separate. Complete raw feed, archived sources, every sample,
  mutation evidence and hypothetical score scenarios are under the native-first
  experiment folder; start with `REPO-TRICKS.md`. No commit/push/submission was
  made during this follow-up.

### Earlier native selection (before the inline-clause follow-up)

- Production B then matched archived `b-shortkeys-direct.rs`: exact 16-byte keys
  for short tokens, full 32-byte keys for longer tokens, built directly from the
  input queries. Two full-seed native batches matched verified reference outputs;
  eight existing differential suites and two boundary suites passed, including
  again from production. A long-token prefix mutation failed both boundary suites.
- Independent repeat medians on seeds 1 / 42 were 169.935 -> 165.673 ms and
  173.914 -> 167.021 ms. The candidate was faster in 39/44 pairs across both
  batches. Timing spread is material; Mac footprint is not official M. Estimated
  S541 -> S566 needs 1.35% less runtime at equal official memory. This is a local
  selection for an Intel trial, with no official score gain established.
- C classifier specialization has now been implemented and rejected. Native ARM
  and Haswell code generation already specializes the baseline; the source-level
  function pointer does not establish native indirect-call overhead. Timings were
  flat and source grew. Do not restart this as an untested next experiment.
- D's radix queue with preserved tie ordering passed differential/queue checks
  but lost about 9% on both native full seeds and used more footprint/source.
  Production A/C/D remain unchanged. Prefer a new measured hypothesis over
  repeating the same losing mechanisms.
- Complete sources, raw samples, mutation evidence and scope are in the native
  experiment README and track PERF.md files. No commit or Intel submission was
  made for these working-tree changes. Preserve them and refresh git status/HEAD.

## Historical handoff — 2026-10-31 at `322b9bb`

The following records the earlier handoff. The latest native selection above
supersedes its production status, runner limitations and proposed C experiment.

- HEAD at this handoff: `322b9bb`. The direct-inline D graph experiment passed
  every official gate but lost: 249.672 → 296.114 ms, 109,588,480 → 130,162,688
  peak bytes, 63.015115 → 61.275239 machine points. Production CSR was restored.
  Full evidence: `experiments/2026-10-30-inline-graph/`.
- The next local batch is under `experiments/2026-10-31-native-first/`; sources,
  tests and JSON reports are preserved but were still untracked at this handoff.
  No production source was changed, committed or submitted from this batch.
  Preserve these files and the user's `plan.md`/AGENTS changes.
- All screened variants matched the recorded full-size reference hashes for seeds
  1 and 42. This is not a holdout proof. Read each report's source hash and samples.
- Local results below are paired median milliseconds from their respective report;
  do not compare baselines across separate batches as if host conditions were fixed.

| Experiment | Seed 1: baseline → candidate | Seed 42: baseline → candidate | Status |
|---|---:|---:|---|
| D portable short decimals | 72.11 → 70.72 | 72.66 → 70.57 | Small 2–3% lead; no promotion yet |
| D custom labels / 4-ary heap | 72.11 → 95.39 / 78.58 | 72.66 → 96.84 / 80.06 | Both lose; combined also loses |
| D dense labels, 6/12 workers | 76.24 → 101–176 | 77.28 → 103–181 | Lose; much larger memory |
| D compact standard HashMap | 74.51 → 75.01 | 75.98 → 76.62 | No time win; footprint modestly lower |
| B direct phrase checks | 166.88 → 166.37 | 164.53 → 165.85 | Flat/inconsistent; more memory |
| B short-token cache | 169.96 → 170.77 | 166.78 → 164.71 | Inconsistent; no established win |
| C 64-byte name entries | 28.42 → 31.07 | 30.64 → 33.41 | Loses about 9% |
| C quadratic probing | 28.06 → 28.08 | 28.39 → 28.09 | Marginal/inconsistent |
| A exact histogram quantiles | 16.77 → 15.55 | 16.67 → 16.83 | Seed-specific; no robust win |

Diagnostics on this Mac put D at approximately 8–10 ms parsing, 6–9 ms
construction and 54 ms querying. These two diagnostic observations are not an
Intel profile. They favor investigating query work over another builder rewrite.

Completed targeted checks: D decimal candidate passed all 11 existing Python
reference differential tests and 2 new decimal tests; custom sparse labels/heap
passed 2 internal reference-comparison tests; compact standard HashMap passed
2 tests; A quantiles passed 3 sort-oracle tests. Dense-label and B-anchor test
files also exist but have not been run in this batch. Do not equate files with runs.

**Next grounded experiment from the contest review:** C's `Structure` stores a
`Classify` function pointer and calls it for every 64-byte block. Investigate
specializing the classifier and dispatching once per worker, preserving the
scalar and AVX2/PCLMUL paths. Inspect the indirect call and 24-byte tuple return
in generated code; the 0xicbor analogy is a hypothesis, not evidence of a win.
This experiment has not yet been implemented or measured.

## Reusing the native harness

`experiments/2026-10-31-native-first/run.py` builds complete source variants into
`.out/native-first/`, invokes existing differential suites against those binaries,
and benchmarks variants sequentially. Example:

```sh
python3 experiments/2026-10-31-native-first/run.py build d-base d-digits
python3 experiments/2026-10-31-native-first/run.py test d-digits
python3 experiments/2026-10-31-native-first/run.py bench d-base d-digits --seeds 1 42 --trials 10 --report d-repeat.json
```

Benchmarking requires `.out/native-first/oracle-<track>-seed<seed>/` outputs.
These ignored files may be absent in a new checkout: recreate them with the
unchanged reference or verify against `expected-hashes.json`, whose seed-1 values
are published references and seed-42 values come from earlier reference checks.
Do not silently generate a new expected answer from the candidate under test.
The runner performs native execution only; never substitute the older Docker
experiment runners. Finish documentation and evidence before any next submission.
