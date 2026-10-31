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
  actually corresponds to the archived source. The current native runner records
  source hashes but still needs executable/input hashes added to its reports.
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

K weights time 0.50, memory 0.20, formatted source 0.15 and dependencies 0.15.
Memory improvement is capped at 50×; source/dependency ratios cannot reward going
below their floors. Away from caps and point clamps:

```text
Delta machine points = 25 log10(Told/Tnew) + 10 log10(Mold/Mnew)
                     + 7.5 log10(Sold/Snew) + 7.5 log10(Dold/Dnew)
```

Use the actual scoring function near caps. Mac memory is not a substitute for M
in an official score claim. Include source/dependency growth in the decision.
The final frozen commit is scored on an unseen seed; historical-best board
entries do not assemble the final submission for us. PERF.md should cite official
run IDs and preserve the raw feed used to make each selection.

## Current handoff — 2026-10-31, refresh before acting

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
