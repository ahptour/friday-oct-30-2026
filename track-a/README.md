# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The Rust executable maps `requests.log` read-only and splits it at newline
boundaries among up to 12 workers. Each worker parses bytes directly into 72
hour buckets. The fixed 72-hour date span is part of the specification for every
seed; endpoint names and results are always discovered from the input.

Space delimiters are found eight bytes at a time using integer byte tests.
Each hour has a hash map from borrowed endpoint bytes to a latency vector and
5xx count. A small sampled hash accelerates lookups; the map still compares the
entire endpoint for equality, so collisions cannot combine different endpoints.
No per-line strings or endpoint copies are allocated.

After parsing, a second parallel phase merges all contributions for each hour,
selects the exact nearest-rank p50/p95/p99, sorts endpoints bytewise, and formats
output. Keeping all groups until parsing completes supports arbitrarily
disordered input and duplicate requests. Output buffers are written in hour
order. Error ppm uses integer arithmetic; empty input produces an empty file.

The only unsafe operations are the Unix `mmap`/`munmap` calls and creating a
read-only slice from a successful mapping. A guard owns the mapping and outlives
all scoped threads and borrowed keys. Parsing itself uses checked slices.
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

The eight differential tests build the release executable and compare its complete
output with the Python reference. They cover the spec's worked example, empty
input, percentile rank rounding, repeated ranks and values, uint32 extremes,
every legal status, integer ppm, all 72 hour boundaries in reverse order,
endpoint byte ordering and maximum length, all methods, duplicates, and three
randomized seeds, partition boundaries, and groups spanning every worker. Tests live outside the track's build context.

See [PERF.md](PERF.md) for local measurements and pending live-board evidence.

## With another 24 hours

Obtain a live-board run ID for this baseline, then profile parsing, map lookups
and output separately on the scoring architecture. Compare endpoint interning
and dense hour storage with the current maps, and benchmark chunk parsing and
parallelism against their extra source size and memory. Validate each candidate
on several generator seeds and retain only measured improvements.
