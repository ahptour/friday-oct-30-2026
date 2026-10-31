# Applying ideas from the Port Mortem winners

Session baseline: clean `1b8528789fb6cf7f6c847e8fabc566cc4923ad56`.
Native machine verified as Apple M4 Max, 16 cores, 48 GiB, macOS ARM64.
No other build, test, generator or benchmark was running before timed batches.
No Docker, commit, push or official submission was performed.

## What the winning repositories actually did

The [organizer's results](https://www.raptors.dev/project/code-resurrection-2026-port-mortem)
place 0xicbor first, cron-parser-go second and Mnemonist third. That contest
weighted reliability/equivalence at 70%; these techniques are useful here, but
those placements do not establish that a particular optimization caused a win.

- **0xicbor:** choose a byte-source implementation once through a Rust generic
  type instead of testing the source kind repeatedly. Its decision log reports
  mean parsing-time ratio against C falling from 1.492 to 1.033. A separate flat
  subtree scanner skips values without constructing parser state that will be
  discarded, retaining the original recursive error/fallback path. Its initial
  specialization commit and scanner commit are separate measurements. The log
  also records a rejected 24-byte return-by-value change that slowed recursion.
  [Decisions 13, 14 and 16](https://github.com/keirsalterego/0xicbor/blob/main/decisions.md).
- **Mnemonist:** preserve observable ordering and use real data structures with
  explicit invariants. Its Fibonacci heap links nodes by stable vector indices;
  its sparse set validates a sparse index against a dense array and clears by
  resetting its live length. KD-tree search prunes branches using distance bounds.
  These are representation and algorithm choices, not evidence that copying its
  heap would speed our Dijkstra implementation. Its decision log makes no general
  speedup claim for these choices. [Decisions](https://github.com/bhavyakeerthi3/mnemonist-rs/blob/main/DECISIONS.md),
  [heap](https://github.com/bhavyakeerthi3/mnemonist-rs/blob/main/src/fibonacci_heap.rs),
  [sparse set](https://github.com/bhavyakeerthi3/mnemonist-rs/blob/main/src/sparse_set.rs).
- **cron-parser-go:** keep the native library separate from a WASM adapter so
  the unchanged TypeScript tests can exercise Go. Record actual date operations
  so JavaScript spies see real engine behavior; deliberately regress the engine
  to prove those assertions fail. Its fuzzer targets semantic discontinuities
  such as DST gaps, leap days, errors and repeated operations, rather than relying
  on ordinary examples. The current decision log supersedes its older plan to
  offer a corrected default mode. [Decisions D13–D14](https://github.com/aniket-3001/cron-parser-go/blob/main/DECISIONS.md).

`repo-tricks-sources.json` pins the fetched file blob SHAs and URLs. Full fetched
text is disposable research material in `.out/repo-tricks/source-snapshot.json`.
No external implementation was copied into production. C's prior classifier
specialization and D's prior radix queue were already rejected; neither was
repeated here. Dense per-node search arrays also have prior losing evidence.

## Hypotheses and budgets

B's earlier diagnostic evidence put Boolean flushing at about 20% of worker
parsing time. Reduce its pointer chasing and retained operand storage by keeping
clause operands inline, following the indexed/contiguous representation idea.
A clause needs at most five operands after its positive anchor is removed:
the frozen grammar permits six Boolean terms. A uint32 stores the dense term ID
and its polarity bit. Even 50,000 eight-term queries have at most 400,000 distinct
terms, so this encoding is exact. Deduplication, contradictions, operand order,
anchor selection, masks, phrase handling and output stay on the existing paths.
Planning still uses temporary vectors before packing; this removes retained
operand buffers, not every temporary allocation. Allocation counts were not
profiled.

The second hypothesis is 0xicbor's elimination of discarded work: B can mask out
documents already included by another OR clause, and D can avoid storing or
queuing a relaxation at least as costly as its known complete route. D's proof
uses nonnegative weights; zero-weight and full-width paths remain valid.

The source deltas were screened before builds: +6 Rust code lines for inline
clauses, +3 for D pruning; the follow-up B mask adds +3. Using the refreshed
board's confirmed S566 for B and S457 for D gives these **estimated** new counts:
S572, S460, S569 respectively, with D1 unchanged. At equal official memory,
inline B needs 0.316% less time, D pruning needs 0.196% less, and B masking needs
0.158% less merely to break even. These are screening budgets, not measured gains.
The local Rust delta is not the official formatter/scc pass across the track.

## Results and selection

All times below are uninstrumented whole-process exec-to-exit milliseconds,
including setup and output. Inputs are full-sized seeds 1 and 42. Builds finish
before timing, input pages are warmed once per seed consistently, processes run
serially in randomized order, and two warmups are excluded. Each screen has ten
measured trials per variant/seed; the independent repeat has twelve and reverses
the variant-list order. Every run is retained, including warmups and separate
memory measurements. `run.py` now compares exact reference output bytes as well
as recording hashes, outside the timed interval.

| Mechanism | Seed 1 baseline → candidate ms | Seed 42 baseline → candidate ms | Decision |
|---|---:|---:|---|
| Inline B clauses, screen | 159.986 → 156.343 | 159.327 → 155.605 | Repeat |
| Inline B clauses, repeat | 159.934 → 155.570 | 159.945 → 155.525 | Select locally |
| Skip already matched B documents | 159.986 → 160.240 | 159.327 → 159.944 | Reject: flat/slower |
| Prune D relaxations | 76.765 → 78.212 | 77.465 → 78.982 | Reject: about 2% slower |

The selected B candidate wins 41/44 paired trials over both batches. The repeat
has 24/24 wins, with paired median runtime ratios 0.97246 and 0.97303. Repeat
ranges are 154.833–156.556 / 154.943–159.758 ms for the candidate versus
159.118–161.355 / 158.911–171.259 ms for the baseline. The observations support
a small native improvement, not a large breakthrough or an Intel prediction.

Three separate macOS `/usr/bin/time -l` memory measurements per variant/seed:

| Batch / seed | Baseline max RSS bytes | Candidate max RSS bytes | Baseline max footprint bytes | Candidate max footprint bytes |
|---|---:|---:|---:|---:|
| Screen / 1 | 325,566,464 | 322,715,648 | 71,189,104 | 68,321,880 |
| Screen / 42 | 322,322,432 | 320,208,896 | 70,173,272 | 68,059,736 |
| Repeat / 1 | 324,091,904 | 322,273,280 | 69,714,544 | 67,895,920 |
| Repeat / 42 | 321,404,928 | 320,454,656 | 69,272,176 | 68,321,904 |

RSS includes mapped input pages; Mac footprint is a different metric and neither
is Linux cgroup `memory.peak`. Actual candidate official M, E, K and machine
points are unavailable. Raw JSON reports bind every timing variant to complete
source and executable SHA-256, compiler/version/flags, seed, input hashes and
output hashes. Compiler: rustc 1.98.1; native release flags: edition 2024,
opt-level=3, lto=thin, codegen-units=1, panic=abort, strip=symbols,
target-cpu=native. The machine/platform and every raw sample are in each report.

A further complete `b-inline8-clauses.rs` narrows only the operand count to u8.
Its one-batch medians were 155.444 / 155.198 ms, versus the selected candidate's
155.570 / 155.525 ms in that same batch. This incremental time difference is
small relative to observed variability and has no independent repeat. Keep the
usize-length version selected on its two batches; retain the alternative.

Two instrumented observations per seed, in separately archived sources, put
median worker flush time at about 27.1–27.4 → 23.9–24.5 ms for seed 1 and
26.9 → 24.1–24.3 ms for seed 42. This supports the intended phase target; these
are diagnostic runs, not selection timings or a hardware cache-miss profile.
D's diagnostic counted only 4,153/20,972,155 and 4,206/22,006,890 relaxations
pruned (about 0.02%). That small opportunity is consistent with the extra branch
losing overall; it does not isolate the hardware cost of that branch.

Production B is byte-identical to `b-inline-clauses.rs`. Production A/C/D are
unchanged. Losing and exploratory complete sources remain outside track folders.

## Correctness and verification

- Selected B: eight existing unittest methods passed, including all 3^5 operator
  sequences over distinct and repeated six-term inputs, both before and after
  the 1,000-document sampling prefix. Two existing additional boundary methods
  passed for 8/16/17/32-byte keys, phrases and parallel inputs.
- Production B: the same eight methods and two boundary methods passed again
  against the Cargo release executable. Full seeds 1 and 42 matched the saved
  reference bytes exactly from production too.
- Inline u8 alternative: eight existing methods plus both boundary methods passed.
  B masking: eight existing methods passed. Rejected D pruning: all eleven
  existing Python-reference methods passed, including zero cycles, disconnected
  graphs, repeated queries, parallel/self-loop edges and uint32/wide boundaries.
- Mutation check: invert only the selected candidate's packed polarity bit in a
  separate full source. The six-term operator-order test executed and failed on
  output bytes: one attempted/executed method, one failure, zero errors/skips.
  Its source/build identity and complete failure output are preserved. The
  unmodified Python reference and production source were never mutated.
- All native benchmark and diagnostic outputs matched the saved full-reference
  bytes. Seed 1 hashes are published; seed 42 bytes were previously verified
  against the unchanged reference. These runs are not fresh full-size reference
  executions. The small differential methods execute fresh reference calls.
  No observed mismatch exists outside the deliberately broken mutant. This is
  not final holdout evidence.
- Production rustfmt and Clippy with warnings denied passed. Haswell x86-64
  code generation passed for baseline and candidate; this is compilation, not
  execution on Intel. No source-level classifier overhead is inferred from it.

## Contest impact and remaining uncertainty

The refreshed raw board is `repo-tricks-board.json`, generated
**2026-10-31T05:12:30Z**, with all four latest runs at `1b85287`:
A75.809715, B61.849606, C69.694784, D62.910395 machine points. B's run is
`run-d5c09f62298a`: 441.470 ms, 67,166,208 bytes, S566, D1, every gate passed.
This measures the earlier short-key production source, not inline clauses.
It improves on the preceding latest B61.632993, but remains below the historical
best B61.902928. Keep these comparisons distinct.

`repo_tricks_report.py` uses the actual frozen `harness/scoring.py` and saved
reference calibration to produce `repo-tricks-analysis.json`. It includes
latest official E/K/points, native distributions and source estimates, each
axis's pre-clamp contribution, break-even and +5-point target times, and
explicit best-three judge-score scenarios for every experimental row.

**Hypothetical only:** if the repeat's native runtime ratios transferred to
Intel, official memory stayed unchanged, and S572 were confirmed, B would gain
about 0.266 / 0.270 machine points, of which source costs about 0.034 points.
Equal-M break-even is 440.076 ms against the 441.470 ms official baseline;
+5 machine points would require 277.669 ms. These are not candidate Intel scores.
With equal judge scores of 80, A/C/D still count and overall G is unchanged.
With B's judge score 84 and the others 80, A/C/B count and projected G rises
about 0.061. Actual judge scores and holdout performance are unknown. The local
selection is useful evidence for an Intel trial, not an established overall win.

## Reproduce

```sh
python3 experiments/2026-10-31-native-first/run.py build b-clause-base b-inline-clauses b-unmatched d-prune-base d-prune
python3 experiments/2026-10-31-native-first/run.py test b-inline-clauses b-unmatched d-prune
python3 experiments/2026-10-31-native-first/test_b_shortkeys.py .out/native-first/b-inline-clauses/target/release/solution
python3 experiments/2026-10-31-native-first/run.py bench b-clause-base b-inline-clauses b-unmatched --seeds 1 42 --trials 10 --report another-b-screen.json
python3 experiments/2026-10-31-native-first/run.py bench d-prune-base d-prune --seeds 1 42 --trials 10 --report another-d-screen.json
python3 experiments/2026-10-31-native-first/repo_tricks_report.py
```

Existing independently verified oracle outputs are required by the native
runner. Finish every build/generator before benchmarking and execute batches
serially. Do not build or time the intentionally broken mutant as a candidate.
