# Track C: Join

The contract is the organisers' [Track C specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The executable memory-maps `records.csv` read-only and parses it with up to
12 scoped workers. Chunk boundaries follow real record terminators. The
published generator guarantees that quoted names contain no LF and note
continuation lines never start with a numeric id or a BOM; the parser uses
that guarantee when aligning chunks. Parsing fields still handles quoted
commas, doubled quotes, embedded LF, empty values, mixed LF/CRLF, and BOMs
at record boundaries. The record parser classifies 64 bytes at a time into
quote/comma/LF masks. On Intel it uses AVX2 and carry-less multiplication for
quote parity; a portable classifier handles other CPUs. A stack-padded final
block prevents reading beyond the mapping. Quote state and quote counts carry
across blocks, including escapes split at a boundary. Each structural separator
is consumed once; quote-count differences give decoded field lengths.

`regions.csv` becomes a dense region-to-country lookup with one-byte IDs: the
frozen contract allows 240 countries plus `_UNMATCHED`. Unknown ids map to
`_UNMATCHED`. Each worker maintains its own counts, sums, extrema and decoded
byte lengths in integer arithmetic.

For distinct names, workers share an exact atomic interning table. Separate
keys carry input offset, 16-bit fingerprint and 16-bit length. Each name has
24 inline payload bytes and four atomic country words; longer payloads compare
the remaining immutable input bytes. A fingerprint only filters probes: matches
still require complete payload equality. Probes stop after 32 slots, with an
exact mutex-protected map for crowded tables, names over 65,535 encoded bytes,
and offsets outside the packed range. The 131,072-slot cap never limits the
number of accepted distinct names.

A two-stage ring overlaps parsing with memory access: prefetch a candidate key
and inline name, resolve it sixteen records later and prefetch its country word,
then update that word after another sixteen records. The atomic operation
credits exactly one worker per name/country pair. Both rings are drained at the
end of every chunk, including chunks with fewer than sixteen records.

CSV escaping is canonical, so encoded payload equality preserves decoded name
equality; both empty spellings produce an empty payload. Quote counts remove
delimiting quotes and one byte per doubled quote without allocating strings.
Numeric fields use known-length slices and eight-digit arithmetic.

The final reduction combines worker aggregates, formats cents without floating
point, sorts country codes bytewise and writes only `aggregates.txt`. A country
without records is omitted. The original Python files remain unchanged as
correctness oracles.

Unsafe operations cover Unix mappings, bounded SIMD loads, prefetching and
inline name storage. A successful empty-to-busy CAS gives one writer exclusive
ownership of an inline name. Publishing the completed key uses Release;
readers use Acquire before touching that immutable head. Country words are
initialized atomics before worker startup. The mapping outlives all workers,
pending names and overflow keys; joining workers synchronizes the reduction.

The block scanner and staged lookup design were informed by
[Cache Me's public scored implementation](https://github.com/manusingh090/speed-demon-2026-entry/blob/3b56ff2ff0d5efa3cf22dd8a875e24b672a2bac1/track-c/main.cpp).
This Rust implementation retains bounded probing and exact overflow handling.

## Dependencies

None. The pinned allowlisted Docker images build with Cargo offline and locked.
`target-cpu=native` requests the organisers' supported build on the scoring CPU.

## Verification

For the current block-parser candidate, local runtime tests and benchmarks were
skipped at the user's request in favor of an immediate Intel board submission.
Formatting and native/Intel Linux compilation checks passed. The commands below
are available for differential and sandbox verification; they were not run for
this candidate.

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

The preceding implementation matched full seeds 1/42 and sample seed 42 against
the reference. The new candidate passed every official Intel board gate at `dbcb3af`, taking
59.995 ms with 13,512,704 peak bytes and scoring 68.611451. This is one public
seed measurement; final holdout performance remains unmeasured. Results are
recorded in [PERF.md](PERF.md).

## With another 24 hours

Profile atomic table access and CSV delimiter scanning on the scoring machine.
Compare worker count, table capacity, and cache-line layout against both runtime
and peak memory. Validate changes on independent generator seeds and retain only
measured improvements in the score.
