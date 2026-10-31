# Track C: Join

The contract is the organisers' [Track C specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

Map `records.csv` read-only and split complete records among up to twelve scoped
workers. A 64-byte structural scanner carries quote parity/counts across blocks
and handles quoted commas, escaped quotes, embedded newlines, mixed LF/CRLF and
BOMs. Published generator guarantees allow safe chunk alignment. Numeric fields
use bounded decimal conversion; all amount arithmetic is exact integer cents.
A one-byte region lookup maps unknown regions to `_UNMATCHED`.

Each parser accumulates numeric aggregates locally and batches exact borrowed
name descriptors into 32 hash shards, 64 records at a time. A mutex protects each
shard for a complete batch. Its owner compares full name bytes through compact
keys, inline 24-byte heads and input-backed tails, then marks one of four country
words. First insertions credit that worker's distinct count. Empty quoted and
unquoted names share the same payload. Crowded probe windows, oversized names
and oversized mappings use exact overflow maps, so capacity never limits results.
Prefetching looks ahead within the locked batch. All partial batches drain before
workers publish results; no thread holds more than one shard lock.

The final reduction combines counts, sums, extrema, decoded lengths and distinct
credits, sorts country codes bytewise and writes `aggregates.txt`. Input mappings
outlive workers and borrowed names. Unsafe operations are limited to mappings,
bounded SIMD loads and prefetching. No shared atomic name publication is needed.

The dedicated producer/owner pipeline at `196b0c0` passed every official gate
but lost 2.530607 points against the historical best. This restores the preceding
batched mutex implementation. See PERF.md for measured evidence and the complete
archived candidate. The scanner's original design was informed by
[Cache Me's implementation](https://github.com/manusingh090/speed-demon-2026-entry/blob/3b56ff2ff0d5efa3cf22dd8a875e24b672a2bac1/track-c/main.cpp).

## Dependencies

None. The pinned allowlisted Docker images build with Cargo offline and locked.
`target-cpu=native` requests the organisers' supported build on the scoring CPU.

## Verification

The restored batched mutex source passed the official Intel gates at `2cea13f`.
The rejected producer/owner candidate also passed those gates at `196b0c0`.
Formatting and source review passed; Docker, local compilation, solver execution,
tests and benchmarks were skipped under the current repository instruction.
The following commands remain available and were not run for this experiment.

From the repository root:

```sh
python3 -m unittest discover -s tests -p test_track_c.py -v
cargo fmt --manifest-path track-c/Cargo.toml --check
cargo clippy --manifest-path track-c/Cargo.toml --offline --all-targets -- -D warnings
python3 check.py c --data data/c/full-seed1 --expected d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536
```

Native development accepts `--data DIRECTORY --out DIRECTORY`; defaults are
`/data` and `/out`. Existing differential suites cover the worked example, quote
escaping, multiline notes, BOM placement and literal BOM bytes, empty values,
trailing spaces, negative cents, 64-bit sums, unknown regions, chunk boundaries,
header-only input, omitted final terminator, and deliberate sampled-hash
collisions that reach the overflow map, and all 240 country IDs.

The preceding batched mutex implementation passed the official Intel gates at
`2cea13f`, taking 52.975 ms with 14,024,704 peak bytes and scoring 69.831060.
Results for both implementations are recorded
in [PERF.md](PERF.md).

## With another 24 hours

Profile CSV parsing, shard waiting and name-table access on the scoring machine.
Compare worker count, table capacity, and cache-line layout against both runtime
and peak memory. Validate changes on independent generator seeds and retain only
measured improvements in the score.
