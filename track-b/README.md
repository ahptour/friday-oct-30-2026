# Track B: Index

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

Read the queries first and build postings only for the terms they use. Tokenize
each document once with ASCII byte lowercasing and `[a-z0-9]+`. Keep its complete
token sequence as a space-delimited byte string, including tokens that no query
uses. Those intervening tokens are necessary for exact phrase adjacency.

Evaluate Boolean queries with set operations strictly left to right. For phrases,
intersect postings starting with the smallest list, then search each candidate's
normalized bytes with spaces at both ends of the phrase. This preserves token
boundaries, repetitions and punctuation-separated adjacency without retokenizing.
Each document has its own string, so phrases cannot span documents.

Count and sum every matching document id, and use `heapq.nsmallest` for the ten
smallest ids. Cache formatted results for repeated query lines. Empty documents,
absent terms and duplicate queries retain their exact specified behavior. The
CLI remains `--data DIR --out DIR`, and `run.py` calls `main`.

## Dependencies

None beyond Python 3.13's standard library. The existing pinned Python image and
zero runtime dependency declaration are unchanged.

## Validation

From the repository root:

```sh
python3.13 -m unittest discover -s tests -v
```

Tests execute the actual CLI against an independent pinned reference. They cover
all six-term Boolean operator combinations, exact byte tokenization, 32-byte
tokens, phrase boundaries and repetitions, empty input and randomized corpora.

See [PERF.md](PERF.md) for native measurements and
[the experiment archive](../experiments/2026-10-31-tracks-bcd/README.md) for complete
candidate sources, raw reports, oracle provenance and reproduction commands.

## With another 24 hours

Measure on the official Intel runner before claiming a contest score. Profile
full-sized indexing and Boolean query work there. Compare compact postings or
bitmaps against these sets, retaining exact token identities and left-to-right
evaluation. Include construction, result summarization, memory, source size and
dependencies in every comparison.
