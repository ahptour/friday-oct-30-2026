# One-shot scan and ownership rewrite

This is a new experiment after `2cea13fe47c6d2b690256e336d8b0ed405e95506`,
not a claim that its phrase automaton or repeated graph scans worked. The old
attempt and completed official results remain under `../2026-10-30-query-machine/`.
The user authorized either reverting that attempt or continuing with a new one.

Three independent track changes are submitted together for one Intel measurement:

- B: return to selective streaming Boolean/phrase evaluation, but replace
  per-token padded keys with a frozen exact token dictionary. Cached 64-byte
  masks enumerate tokens; bounded lowercase word loads and a fingerprint table
  resolve IDs without copying a 32-byte key for every token. Length and complete
  word comparisons resolve collisions, including interiors of long tokens.
- C: six CSV producers route batches to six dedicated name owners through
  bounded channels. Each name table has one writer and no mutex handoffs.
  Producers aggregate numeric fields; owners count exact name/country insertions.
- D: parser threads route edges once into forward and reverse owner buckets.
  Owners count/scatter only their own edges directly into disjoint slices of the
  final contiguous CSR. No repeated full-graph owner scan or final concatenation.
  The earlier binary heaps and sparse distance maps replace the losing radix
  search experiment. Wide edges retain exact full-width exceptions.

A is unchanged. No new dependency, build flag, image, oracle modification,
seed recognition or precomputed answer was introduced. All runtime data
structures are created from the actual inputs inside the measured process.

## Baseline and selection

`intel-baseline.json` is the full official board response generated
2026-10-31T04:15:08Z. Every gate passed for the preceding attempt. The current
and historical-best numbers must be distinguished: current B and D are already
slower than earlier implementations. A gain over current alone is insufficient.

| Track | Current ms | Current bytes | S | Current points | Historical-best points |
|---|---:|---:|---:|---:|---:|
| A | 40.469 | 23,322,624 | 488 | 75.316235 | 78.010364 |
| B | 510.215 | 70,705,152 | 642 | 59.644913 | 61.902928 |
| C | 52.975 | 14,024,704 | 540 | 69.831060 | 70.122320 |
| D | 459.309 | 100,663,296 | 495 | 56.505507 | 61.969457 |

Baseline files contain the complete preceding sources, candidate files contain
the complete new implementations, and reference files preserve the sources to
compare/select if the experiment loses: B `3b74c52`, C `2cea13f` (identical to
its earlier best source), D `aa708b6`. These reference builds use the same
manifests/native compiler flags as the current tracks. Source and dependency
costs count along with time and memory. Each track's PERF.md records its
hypothesis, run IDs, source-budget estimates and remaining risks.

## Review and verification

B source review covered exact ASCII semantics, 1–32 byte tokens, equal
fingerprints and equal heads/tails with different interiors, unknown token
positions, UTF-8 separators, LF/EOF handling, tokens crossing 64-byte boundaries,
empty documents, Boolean associativity and output ordering. C review covered
unique owner routing, borrowed mapping lifetimes, bounded backpressure,
producer sender drop before owner joins, single-CPU scheduling, exact overflow
maps, escaped names and per-country distinct reduction. D review covered
source/reverse routing, global CSR sentinel and every owner boundary, span one,
short final ranges, empty adjacency, global exception indices, wide arithmetic,
and safe mutable slice partitioning. Independent source reviews were performed
for the new mechanisms; no extra unsafe graph writes were introduced.

Rust formatting and whitespace checks are the local verification permitted by
the task. No Docker, local compilation, solver execution, tests or benchmarks
are run. Official build/correctness/performance remain pending. A public-seed
pass will not prove final holdout correctness. This batch changes multiple
mechanisms within tracks, so measured gains cannot isolate each contribution.

C may lose from six producers, queue traffic and batch allocation. D spends a
second temporary edge copy to buy parallel construction; allocated payload is
not a cgroup peak measurement. B spends source and transition-mask bookkeeping
to reduce corpus scan work. The complete measured score decides selection.
