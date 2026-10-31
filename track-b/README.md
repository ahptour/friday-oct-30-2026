# Track B: Index

The exact contract is in the [Track B spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

Read the queries first and intern only the requested terms. Freeze that
vocabulary into a compact open-addressed table: eight-byte slots carry a 32-bit
fingerprint and a one-based term ID, while exact keys and lengths are stored
separately. At most half the slots are occupied. Hash collisions always fall
through to complete token equality, including interior words of long tokens.
The construction hash map is released before document workers start.

Each worker classifies a 64-byte input block once into alphanumeric and newline
masks. SSE2 handles full blocks on x86-64; the final partial block uses bounded
scalar reads. Bit scans enumerate runs, joining tokens across block boundaries
and stopping at document newlines. Eight-byte loads lowercase token words in
registers: no padded token buffer is cleared, filled or hashed on the corpus
path. Short-token loads mask any bytes past the token; every load stays inside
the supplied input. ASCII digits are unchanged, and all non-ASCII bytes are
separators. Token fingerprints use the first and last words plus length;
acceptance verifies the length and all words, never the fingerprint alone.

Sample the first 1,000 documents to choose selective query anchors. Sampling
only chooses an execution order; every document and query is still evaluated
exactly, including terms absent from the sample. Compile left-to-right Boolean
expressions into a small disjunction of conjunctions, removing contradictory
clauses and selecting a positive anchor for each remaining clause.

Map the corpus read-only and partition it at document boundaries among up to
12 workers. Boolean evaluation uses 64-document presence masks and exact bitwise
clause evaluation. Phrase evaluation uses the sampled rarest term as an anchor
and compares consecutive token IDs around candidate occurrences. Unknown tokens
keep their positions, repeated terms require repeated positions, and document
stamps prevent double counting. This reuses the earlier streaming execution
plan while replacing the vocabulary representation and mandatory corpus scan.

Each result stores an exact count, 64-bit ID sum and ten smallest IDs. Worker
summaries merge in document order. No full corpus index or phrase automaton is
retained. Unsafe code is limited to bounded SIMD loads and the Unix read-only
mapping, whose lifetime exceeds all scoped threads.

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

The existing differential suites are available to check the worked example, operator associativity,
ASCII/Unicode distinctions, 32-byte tokens, phrase positions, repeated terms,
duplicate queries, document boundaries, empty inputs, all six-term operator sequences, terms absent from the sampling
prefix, randomized documents spanning worker boundaries, and category-only or
interleaved queries with repeated clauses and output-order checks.

This candidate has not been run locally; current verification and official
board evidence are recorded in PERF.md.

For native development the binary accepts `--data DIRECTORY --out DIRECTORY`;
defaults are `/data` and `/out`. See [PERF.md](PERF.md) for measurements.

## With another 24 hours

Profile the Boolean block size, compact dictionary probes and structural scanner on
the scoring CPU. Compare compact token dictionaries and frequency-ordered IDs
only when the measured gain pays for their extra source and memory. Retain
improvements only after independent-seed and full-output verification.
