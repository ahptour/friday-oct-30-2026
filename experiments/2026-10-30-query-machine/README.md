# Query machine and parallel graph ownership experiment

Baseline source: `b90a4a4216b5c05eb0b9c3b3eea3ab989d6725e6`.
The user requested one unusually bold experiment intended to improve efficiency.
This batch changes the two weaker tracks structurally; A and C remain as found.
No external implementation was copied and no dependencies or build flags changed.

## B: compile the workload; stream the corpus once

Replace independent local inverted indexes and per-query source-text KMP with
one shared token-level Aho-Corasick automaton for every supplied phrase, fused
with streaming 64-document Boolean masks. The Boolean compiler, selective
sampled anchors, SIMD tokenizer and summary arithmetic reuse the earlier
complete streaming implementation in `../2026-10-30-bold/b-baseline.rs`.
The automaton itself is new. All compilation happens inside the timed process
from the actual queries; nothing recognizes a seed or embeds derived answers.

Root transitions use a vocabulary-sized direct array. Other transitions hash
an exact `(state, token ID)` pair, with full integer equality. Failure links
follow proper suffixes; terminal suffix links avoid copying suffix outputs.
Identical phrases share a terminal and retain an emission for each output line.
Per-worker document stamps use the first terminal emission as their index.
When a terminal was already reported in a document, all its terminal suffixes
were reported too, so the entire remaining suffix walk can stop. State resets
at document boundaries and unknown tokens, but survives any separator bytes.
Unknown tokens cannot be silently removed, since they break phrase adjacency.
There is no retained corpus index, per-document token vector or phrase rescan.

The automaton and Boolean compiler have construction cost. Common phrase
prefixes can cause frequent transition probes; the corpus hash lookup still
runs once per token. Twelve workers retain private result summaries, masks and
terminal stamps. This is a hypothesis, not an established speedup.

## D: give graph construction disjoint node owners

Replace two whole-graph CSR builders with disjoint power-of-two node ranges.
Each owner counts both directions in one stream of all compact input edges,
allocates only its final adjacency vectors, then scatters both directions in a
second stream. At full scale, twelve threads yield seven ranges of at most
262,144 vertices. Each direction retains per-fragment offsets and exact tagged
wide-edge exceptions. Shift/mask lookup reaches the appropriate CSR fragment.
No global concatenation, atomics, unsafe scatter, per-worker full-node counters
or edge-sized partition buffers are required. The original parser, generation
distance tables, radix queues, search logic and twelve query workers remain.

The ordinary final payload stays `8*m + 8*(n + fragments)` bytes across both
directions, plus wide exceptions, vector metadata and retained capacities.
The source list remains live during construction as before. Seven owners read
it twice, rather than two builders reading it twice: extra bandwidth, decoding
and filtering are the deliberate cost of greater parallelism and smaller random
write ranges. This may lose if repeated scans dominate; official time and peak
memory decide. No claim about an already-profiled serial bottleneck is made.

## Baseline and verification

`intel-baseline.json` is the complete official response generated
2026-10-31T04:08:27Z. Baseline files are the complete sources found at HEAD;
candidate files are complete experiment implementations. All archives are
outside production folders so inactive experiments do not increase track S.

| Track | Latest run | Time ms | Peak bytes | S | Points |
|---|---|---:|---:|---:|---:|
| B | run-6f1d594cc1cc | 505.661 | 177,463,296 | 564 | 56.167602 |
| D | run-9ead5837f230 | 379.135 | 100,716,544 | 437 | 58.991915 |

Historical bests are separate: B `run-4a9c515be486`, 437.009 ms,
70,397,952 bytes, S541, 61.902928 points; D `run-e4950b1f6c67`,
309.870 ms, 96,964,608 bytes, S361, 61.969457 points. Current B/D were restored
by a user revert before this experiment, so beating their latest repeat alone
would not establish an improvement over the best earlier implementations.

Source review covered Boolean associativity/contradictions and cross-worker
64-document blocks; phrase suffixes, overlapping prefixes, duplicate queries,
repeated matches, absent terms, 32-byte tokens, punctuation/UTF-8 separators,
empty and category-only input; graph owner boundaries including the last short
range and span one, directed arcs, parallel/duplicate edges, self-loops, zero
weights, wide node/weight exceptions, per-fragment CSR cursor restoration,
thread/mapping lifetimes and stable output order. No new unsafe code was added.

Rust formatting and `git diff --check` passed. Per repository instructions,
no Docker, local compilation, solution execution, tests or runtime benchmarks
were performed. Official build, correctness and performance are pending; source
review cannot establish holdout correctness. Compare full measured scores,
not time alone, and retain the complete failed candidate if either track loses.


## Completed official result

The official response generated 2026-10-31T04:15:08Z is preserved in
`intel-2cea13f.json`. All four tracks passed check, build, correctness, timeout
and sandbox gates, with measured static axes. A and C source were unchanged.

| Track | Run | Time ms | Peak bytes | S | Points | Delta vs historical best |
|---|---|---:|---:|---:|---:|---:|
| B | run-26af339e8a1a | 510.215 | 70,705,152 | 642 | 59.644913 | -2.258015 |
| D | run-33b028b22671 | 459.309 | 100,663,296 | 495 | 56.505507 | -5.463950 |

B reduced measured memory from 177,463,296 to 70,705,152 bytes and gained
3.477311 points over the immediately preceding adaptive-index implementation.
Its 510.215 ms time did not improve, and it remains 2.258015 points behind the
earlier streaming implementation. D became 21.1% slower than its preceding
379.135 ms repeat and lost 2.486408 points; it is 5.463950 points behind its
historical best. One public-data run is not a holdout proof or a repeatability
study. These data do not establish a dramatic efficiency improvement.

The next experiment starts B and D from the earlier streaming/search structures
with better measured scores, retaining this complete attempt for inspection.
Its implementation and comparison are under `../2026-10-30-next-shot/`.
