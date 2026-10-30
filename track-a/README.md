# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The Rust executable maps `requests.log` read-only and splits it at newline
boundaries among up to 12 workers. A first pass indexes byte ranges by hour,
coalescing adjacent lines for the same hour. Separate ranges are retained when
an hour reappears, so there is no assumed bound on input disorder. The fixed
72-hour date span is part of the specification for every seed; endpoint names
and results are always discovered from the input.

The Intel parser classifies spaces and LF in one bounded 64-byte window after
each timestamp, with scalar delimiter handling for long endpoints and tails.
It decodes the known-length latency with SIMD decimal arithmetic and copies the
hour label once per range. The index pass still uses eight-byte newline tests.
The production data structures retain the preceding hour-ownership layout. Each hour has
a hash map from borrowed endpoint bytes to a latency vector and
5xx count. A small sampled hash accelerates lookups; the map still compares the
entire endpoint for equality, so collisions cannot combine different endpoints.
No per-line strings or endpoint copies are allocated.

In the second pass, workers claim hours dynamically. One worker parses every
indexed range for its hour, selects the exact nearest-rank p50/p95/p99, sorts
endpoints bytewise, formats output, and frees the groups before claiming another
hour. Latencies never need to be merged across workers. Completed byte buffers
are written as soon as preceding hours are ready, preserving output order.
Duplicate requests all count. Error ppm uses integer arithmetic; empty input
produces an empty file.

Unsafe operations wrap Unix mapping and bounded Intel SIMD loads. A guard owns
the mapping and outlives all scoped threads and borrowed keys. SIMD parsing
uses the scoring contract's AVX2-capable CPU; loads require a full input window,
and latency loads remain inside the current complete record. Other targets use
the scalar parser.
`a_ingest.py` and `run.py` remain byte-identical correctness oracles.

The Docker build enables `target-cpu=native`, which the published board rules
route to a build on the scoring machine. No CPU features are assumed from this
Apple Silicon development host.

## Dependencies

None.

## Run and verify

From the repository root, generate the public dataset using the organisers'
[generator](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/generators/a_ingest.py):

```sh
mkdir -p .out
curl -fsSL https://raw.githubusercontent.com/hackathon-raptors/speed-demon-2026/main/generators/a_ingest.py -o .out/a_generator.py
python3 .out/a_generator.py --seed 1 --scale full --out data/a/full-seed1
```

Build and run with the board's sandbox settings and published output hash:

```sh
python3 check.py a --data data/a/full-seed1 --expected 495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3
```

Both Docker base images are pinned to the published allowlist. The build uses
`cargo build --release --offline --locked`; there are no crates to vendor.
The executable defaults to `/data` and `/out`. For native development:

```sh
cargo run --manifest-path track-a/Cargo.toml --release --offline --locked -- --data data/a/full-seed1 --out .out/native-a
python3 -m unittest discover -s tests -v
cargo fmt --manifest-path track-a/Cargo.toml --check
cargo clippy --manifest-path track-a/Cargo.toml --offline --all-targets -- -D warnings
```

The ten differential tests build the release executable and compare its complete
output with the Python reference. They cover the spec's worked example, empty
input, percentile rank rounding, repeated ranks and values, uint32 extremes,
every legal status, integer ppm, all 72 hour boundaries in reverse order,
endpoint byte ordering and maximum length, all methods, duplicates, and three
randomized seeds, partition boundaries, groups spanning every worker, and
deliberate sampled-hash collisions. A regression test covers long runs that
revisit hours, uneven hour sizes, and empty hours between output groups. Tests
live outside the track's build context.

Last measured live-board score: **75.758840**, runtime **0.048427 s**,
peak memory **20.84 MB**, with all gates passed. See [PERF.md](PERF.md).

## With another 24 hours

Measure hour ownership on the scoring architecture, then profile indexing,
map lookups and output separately. Compare endpoint interning and denser range
storage with their extra source size and memory, including heavily disordered
inputs where the range index grows. Validate each candidate on several
generator seeds and retain only measured improvements.

## Profile-guided experiment

Build-time PGO trained on generated sample seeds 7, 19 and 103 passed correctness
on ARM64 and AMD64. Intel board scores fell after memory and source costs, so
the ordinary native release build is retained. The reproducible experiment is
preserved at commit `3b74c524ad473105b5bfa934855984fe98336eea`; see [PERF.md](PERF.md).


The compact-record A and predecessor-aware D trials at `330dbc3` passed the
Intel correctness gates but lost machine points. Their sources and measurements
are preserved outside the production folders; see [PERF.md](PERF.md). Production D uses the preceding implementation; production A isolates the
SIMD parser inside its preceding hour-ownership layout. Local runtime tests
are skipped at the user's request; Intel results determine the selection.
