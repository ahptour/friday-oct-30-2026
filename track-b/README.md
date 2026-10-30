# Track B: Index

The exact contract is in the [Track B spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

Read the queries first and intern only the requested terms. Tokens are normalized
into fixed 32-byte keys using ASCII lowercase only. Non-ASCII bytes always
separate tokens, exactly as the specification requires.

Build a token-level Aho–Corasick automaton for all phrase queries. Compact nodes
store failure links and linked terminal query IDs, including duplicate queries.
During a document scan, unknown tokens reset the automaton and document
boundaries reset its state. Per-query document stamps prevent repeated phrase
occurrences from counting a document twice.

Map the corpus read-only and partition it at document boundaries among up to
12 workers. Each worker simultaneously matches phrases and indexes terms used
by Boolean queries. Posting lists begin as sorted document IDs and switch to a
bitset when dense enough; worker bitsets cover only that worker's document range.
Merge worker indexes in document order, then answer Boolean queries in parallel.
Sparse and dense set operations obey the written left-to-right operator order.

Each result records its exact count, 64-bit ID sum, and ten smallest IDs. Worker
phrase summaries and output chunks are merged in document/query order. The only
unsafe operations are the Unix mapping calls and forming a borrowed read-only
slice from the mapping. The mapping outlives all scoped threads.

## Dependencies

None. The Dockerfile uses allowlisted digest-pinned images and builds with
`cargo build --release --offline --locked`. `target-cpu=native` uses the board's
supported build-on-the-scoring-machine route. The Python reference files remain
unchanged as test oracles and are not included in the runtime image.

## Verification

From the repository root:

```sh
python3 -m unittest discover -s tests -p test_track_b.py -v
cargo fmt --manifest-path track-b/Cargo.toml --check
cargo clippy --manifest-path track-b/Cargo.toml --offline --all-targets -- -D warnings
python3 check.py b --data data/b/full-seed1 --expected f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927
```

The five differential tests cover the worked example, operator associativity,
ASCII/Unicode distinctions, 32-byte tokens, phrase failure links, repeated terms,
duplicate queries, document boundaries, empty inputs, mixed sparse/dense sets,
and randomized documents spanning worker boundaries.

For native development the binary accepts `--data DIRECTORY --out DIRECTORY`;
defaults are `/data` and `/out`. See [PERF.md](PERF.md) for measurements.

## With another 24 hours

Measure document-at-a-time Boolean evaluation using selective query anchors to
avoid retaining posting lists. Compare that trade with the current hybrid index,
then profile the automaton's transition lookup and ASCII tokenizer on the board.
Retain only improvements that survive independent seeds and the full output hash.
