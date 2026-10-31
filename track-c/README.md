# Track C: Join

The contract is the organisers' [Track C specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The executable maps `records.csv` read-only and creates up to six producer and
name-owner pairs. On the scoring machine six CSV producers and six name owners
use twelve worker threads. A single available CPU uses one pair of OS threads;
blocking sends do not occupy a fixed task pool or prevent the owner running.

Producers split input at real record boundaries. The published generator has no
LF inside names and note continuation lines do not begin with a numeric id or
BOM. The 64-byte structural classifier handles quoted commas, doubled quotes,
embedded note LF, mixed LF/CRLF, empty fields and record BOMs. AVX2 and carry-less
multiplication produce quote parity on Intel, with a portable fallback and a
stack-padded final block. Prefix parity and quote counts carry across blocks.
Short numeric fields use bounded known-length decimal conversion.

`regions.csv` becomes a dense one-byte region-to-country lookup. The frozen
contract permits 240 countries plus `_UNMATCHED`; missing IDs map to zero.
Producers keep private integer counts, sums, extrema and decoded byte lengths.

Distinct names take a separate concurrent path. A producer hashes each encoded
payload and routes it to one of 32 shards. It batches up to 512 descriptors per
shard, then transfers that vector to the shard's dedicated owner over a bounded
standard-library channel. Each owner has at most two queued batches. This limits
the backlog; batches hold borrowed input slices rather than copied strings.
Owners alone access their assigned name tables. No mutex hands those tables
between producer threads and no atomic operation updates their contents.

The exact name representation retains offset/length/fingerprint keys, 24 inline
payload bytes, four country words and full input-tail comparisons. Fingerprints
only filter probes. A full byte-keyed overflow map handles crowded probe windows,
long names and oversized mappings. The total compact-table cap remains 131,072
slots across all shards and never limits the accepted distinct-name count.
Oversized mappings consistently use overflow for every occurrence of a name.

Owners count the first insertion of each exact name/country pair. Equal names
always select the same shard and owner, so summing owner counts is exact. CSV
escaping is canonical, making encoded payload equality preserve decoded name
equality; both empty spellings become an empty payload. Quote counts compute
decoded byte lengths without allocating unescaped strings.

Every producer sends its final partial batches and drops its sender clones.
The coordinator drops its own senders before joining producers. Owners consume
the remaining queues until all senders close, then publish counts. No owner
waits on another owner or producer result. The mapping outlives both stages,
all pending descriptors and the exact overflow keys.

Final reduction merges producer aggregates and owner distinct counts, formats
cents with integer arithmetic, sorts country codes bytewise and writes only
`aggregates.txt`. Countries without records are omitted. The original Python
files remain independent correctness oracles.

This experiment trades half the parser threads and bounded queue/allocator work
for concurrent stages and private ownership of hot name tables. It needs an
Intel measurement; ownership alone does not establish a speedup. Unsafe code
remains limited to Unix mapping, bounded SIMD classification and prefetching.

The block scanner and staged lookup design were informed by
[Cache Me's public scored implementation](https://github.com/manusingh090/speed-demon-2026-entry/blob/3b56ff2ff0d5efa3cf22dd8a875e24b672a2bac1/track-c/main.cpp).
The producer/owner pipeline is a new implementation using the existing exact
tables and overflow handling.

## Dependencies

None. The pinned allowlisted Docker images build with Cargo offline and locked.
`target-cpu=native` requests the organisers' supported build on the scoring CPU.

## Verification

For this producer/owner candidate, formatting and source review passed. Docker,
local compilation, solver execution, tests and benchmarks were skipped under the
current repository instruction. Official build/correctness/performance are
pending. The following commands are available for verification and were not run
for this candidate.

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
That is baseline evidence, not validation of this pipeline. Results are recorded
in [PERF.md](PERF.md).

## With another 24 hours

Profile producer throughput, channel waiting and owner-table access on the scoring machine.
Compare worker count, table capacity, and cache-line layout against both runtime
and peak memory. Validate changes on independent generator seeds and retain only
measured improvements in the score.
