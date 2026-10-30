# Track C: Join

The contract is the organisers' [Track C specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The executable memory-maps `records.csv` read-only and parses it with up to
12 scoped workers. Chunk boundaries follow real record terminators. The
published generator guarantees that quoted names contain no LF and note
continuation lines never start with a numeric id or a BOM; the parser uses
that guarantee when aligning chunks. Parsing fields still handles quoted
commas, doubled quotes, embedded LF, empty values, mixed LF/CRLF, and BOMs
at record boundaries. Eight-byte delimiter searches avoid scanning every
ordinary character individually.

`regions.csv` becomes a dense region-to-country lookup. Unknown ids map to
`_UNMATCHED`. Each worker maintains its own counts, sums, extrema and decoded
byte lengths in integer arithmetic.

For distinct names, workers share an exact atomic interning table. Each entry
holds an offset and length into the immutable input plus four country bitsets.
A successful first setting of a country's bit increments the worker's local
distinct count. This keeps one name entry globally, rather than one per country
or per input occurrence. A sampled hash accelerates lookups, but every potential
match compares the complete payload bytes. Probes are bounded; an ordinary
mutex-protected hash map handles overflow and collisions exactly.

CSV escaping is canonical, so equality of the encoded payloads is equivalent
to equality after unescaping. The two empty spellings both produce an empty
payload. Byte lengths subtract one byte for every doubled quote; no per-record
string allocation or decoding is needed. Offset packing is an optimization:
names whose offsets or lengths exceed 32 bits use the overflow map.

The final reduction combines worker aggregates, formats cents without floating
point, sorts country codes bytewise and writes only `aggregates.txt`. A country
without records is omitted. The original Python files remain unchanged as
correctness oracles.

The only unsafe operations wrap read-only Unix `mmap`/`munmap`. The mapping
outlives all scoped workers and borrowed names. Atomic keys point to immutable
input created before worker startup, and country words are atomics initialized
before any key is installed. No data is published through a key, so relaxed
atomic operations are sufficient; joining workers synchronizes the reduction.

## Dependencies

None. The pinned allowlisted Docker images build with Cargo offline and locked.
`target-cpu=native` requests the organisers' supported build on the scoring CPU.

## Verification

From the repository root:

```sh
python3 -m unittest discover -s tests -p test_track_c.py -v
cargo fmt --manifest-path track-c/Cargo.toml --check
cargo clippy --manifest-path track-c/Cargo.toml --offline --all-targets -- -D warnings
python3 check.py c --data data/c/full-seed1 --expected d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536
```

Native development accepts `--data DIRECTORY --out DIRECTORY`; defaults are
`/data` and `/out`. Five differential tests cover the worked example, quote
escaping, multiline notes, BOM placement and literal BOM bytes, empty values,
trailing spaces, negative cents, 64-bit sums, unknown regions, chunk boundaries,
header-only input, omitted final terminator, and deliberate sampled-hash
collisions that reach the overflow map.

Full seed 1 matches the published output hash. Sample seed 42 matches an
independently executed Python reference. Local measurements and live-board
results are recorded in [PERF.md](PERF.md).

## With another 24 hours

Profile atomic table access and CSV delimiter scanning on the scoring machine.
Compare worker count, table capacity, and cache-line layout against both runtime
and peak memory. Validate changes on independent generator seeds and retain only
measured improvements in the score.
