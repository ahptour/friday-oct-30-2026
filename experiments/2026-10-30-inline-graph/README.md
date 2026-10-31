# Build the graph while parsing; delete CSR assembly

The user requested one unusually bold efficiency experiment. This shot changes
Track D's entire graph construction and adjacency layout. A/B/C are unchanged.
Baseline commit: `e6d43333cd647211fd29aec9e8e380de0500a14e`.

Each parser reserves one of four inline adjacency slots with an atomic degree
increment and writes ordinary packed edges directly to the final graph. Wide
edges and excess degree retain exact full-width storage in private exception
vectors, merged after parser joins. Both forward and reverse graphs are built
this way. There is no full temporary routed edge list, degree pass, prefix sum,
scatter or global concatenation. The existing bidirectional search engine is
retained. Four slots are an optimization, not a correctness limit.

The gamble is random atomic writes and overflow allocation versus removed bulk
construction work. The inline arrays alone use 40*n bytes, 69,620,480 at full
scale; input mappings, exact overflow and search scratch can raise the peak.
This budget does not establish a cgroup measurement. Thread scheduling changes
which exact edges land inline but not the represented graph or answers.

`d-baseline.rs` and `d-candidate.rs` are complete source files outside production.
`intel-baseline.json` is the complete official feed generated
2026-10-31T04:28:37Z. Baseline D: `run-b2bf06e7dc60`, 249.672 ms,
109,588,480 peak bytes, S457, D1, 63.015115 points. Historical best D:
`run-8274c9e7b4bd`, 247.106 ms, 110,682,112 bytes, S457, D1,
63.084153 points. Compare complete measured scores and all gates.

Source review and Rust formatting/whitespace checks are the permitted local
verification. No Docker, local compilation, solver executions, tests or runtime
benchmarks are performed. Official gates and performance are pending. No new
dependency, compiler flag, image, seed detection or precomputed answer is added.
The independent Python oracle is unchanged. See track-d/PERF.md for review cases.


## Official result: correct, but rejected

The full official response generated 2026-10-31T04:33:18Z is preserved in
`intel-addf6d8.json`. Candidate `addf6d8`, D run `run-4d58ed0ef75d`, passed
check, build, correctness, timeout and sandbox, with measured static axes:

| Version | Time ms | Peak bytes | S | D | Machine points |
|---|---:|---:|---:|---:|---:|
| Selected CSR baseline repeat | 249.672 | 109,588,480 | 457 | 1 | 63.015115 |
| Direct inline candidate | 296.114 | 130,162,688 | 351 | 1 | 61.275239 |

The candidate took 18.6% longer and used 18.8% more peak memory than the
baseline repeat. Reducing S by 23.2% did not compensate: it lost 1.739876
machine points against that repeat and 1.808914 against the historical best.
A/B/C were unchanged and passed every gate. Their repeat changes are not
attributed to this D experiment.

Restore the complete `d-baseline.rs` to production and retain both sources and
this measured failure outside the track directory. The direct-build hypothesis
did not produce an efficiency gain. Random atomics, node-array initialization,
overflow merging and query locality are possible costs; no phase profile was
collected, so this result does not isolate a cause. A public-seed gate pass is
not a holdout proof. No local Docker, compilation, solution execution, tests or
benchmarks were run. Formatting/whitespace checks passed; the restored Rust
source is byte-identical to the preceding selected implementation.
