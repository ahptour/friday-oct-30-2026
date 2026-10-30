# Track B: Index

The exact contract is in the [Track B spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

Read the queries first and intern only the requested terms. Tokens are normalized
into fixed 32-byte keys using ASCII lowercase only. Non-ASCII bytes always
separate tokens, exactly as the specification requires.

Sample the first 1,000 documents to choose selective query anchors. Sampling
only chooses an execution order; every document and query is still evaluated
exactly, including terms absent from the sample. Compile left-to-right Boolean
expressions into a small disjunction of conjunctions, removing contradictory
clauses and selecting a positive anchor for each remaining clause.

Map the corpus read-only and partition it at document boundaries among up to
12 workers. Boolean evaluation operates on blocks of 64 document IDs. Each
requested term holds a 64-bit presence mask; an anchor schedules its clauses
once per block. Bitwise intersections and differences evaluate clauses, and
unions combine them into query masks. Counts use population counts, ID sums use
weighted population counts, and only the first ten IDs need enumeration. Clear
only masks touched in the completed block; no full corpus index is retained.

For phrases, choose the sampled rarest term and record its occurrences in the
current document's token-ID sequence. Compare the complete phrase around every
candidate occurrence. Unknown tokens retain a sentinel position, repeated terms
must match at distinct consecutive positions, and document boundaries cannot be
crossed. Document stamps prevent repeated matches from counting twice.

Each result records its exact count, 64-bit ID sum, and ten smallest IDs. Worker
summaries merge in document order. On x86-64, SSE2 classifies and lowercases up to
16 bytes per step, with a scalar fallback near the input boundary and on other
architectures. Unsafe code is limited to bounded SIMD loads/stores and the Unix
read-only mapping, whose lifetime exceeds all scoped threads.

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

The six differential tests cover the worked example, operator associativity,
ASCII/Unicode distinctions, 32-byte tokens, phrase positions, repeated terms,
duplicate queries, document boundaries, empty inputs, all six-term operator sequences, terms absent from the sampling
prefix, and randomized documents spanning worker boundaries.

For native development the binary accepts `--data DIRECTORY --out DIRECTORY`;
defaults are `/data` and `/out`. See [PERF.md](PERF.md) for measurements.

## With another 24 hours

Profile the Boolean block size, phrase anchor dispatch, and SIMD tokenizer on
the scoring CPU. Compare compact token dictionaries and frequency-ordered IDs
only when the measured gain pays for their extra source and memory. Retain
improvements only after independent-seed and full-output verification.
