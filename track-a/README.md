# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The Rust executable streams `requests.log` through a 256 KiB buffered reader,
reusing one line buffer. The spec guarantees a fixed-width UTC timestamp, so its
first 13 bytes identify the hour directly, including across the month boundary.
No date parser or string conversion is needed.

A sorted map holds the hours; each hour has a hash map keyed by exact endpoint
bytes. Keys are allocated only for new groups. Each group stores its latencies as
`u32` values and counts statuses starting with `5`. Every request is processed,
including duplicate lines, and groups stay available until EOF because the spec
does not bound input disorder.

Three calls to `select_nth_unstable` find the exact nearest-rank p50, p95 and p99,
progressively narrowing the remaining slice instead of sorting every latency.
The pivot stays in that slice to handle equal ranks in small groups. Error rates
use `u64` integer arithmetic. Output walks hours chronologically, sorts endpoints
by byte order, and writes only `summary.txt`, including a final LF per row. Empty
input produces a zero-byte output.

The implementation uses safe Rust and one thread. Memory grows with the number
of input latencies and observed groups; it does not retain the input file.
`a_ingest.py` and `run.py` remain byte-identical to the starter as a correctness
oracle; the runtime image contains only the Rust executable and Debian runtime.

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

The six differential tests build the release executable and compare its complete
output with the Python reference. They cover the spec's worked example, empty
input, percentile rank rounding, repeated ranks and values, uint32 extremes,
every legal status, integer ppm, all 72 hour boundaries in reverse order,
endpoint byte ordering and maximum length, all methods, duplicates, and three
randomized seeds. Tests live outside the track's build context.

See [PERF.md](PERF.md) for local measurements and pending live-board evidence.

## With another 24 hours

Obtain a live-board run ID for this baseline, then profile parsing, map lookups
and output separately on the scoring architecture. Compare endpoint interning
and dense hour storage with the current maps, and benchmark chunk parsing and
parallelism against their extra source size and memory. Validate each candidate
on several generator seeds and retain only measured improvements.
