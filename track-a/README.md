# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The Rust executable maps `requests.log` read-only and parses newline-aligned
chunks with up to twelve workers. Each line is parsed once. On Intel, an AVX2
mask identifies spaces and LF in the 64 bytes after the timestamp; long endpoints
continue through bounded delimiter searches, and short tails use a padded stack
buffer. Endpoint dictionaries are private to each parser and use exact byte
comparison, including hash collisions.

Each parser stores ordinary requests in four-byte records: twenty latency bits,
eleven endpoint-ID bits and one error bit. Latencies outside that range and
larger endpoint IDs go to exact full-width side lists. Packed records live in
16 KB blocks linked by hour, carved from one anonymous mapping per parser.
The reserve follows the minimum legal line length plus one partial block per
hour; allocating it does not touch unused pages. Every record is retained even
when an earlier hour reappears arbitrarily late.

After parsing, endpoint dictionaries are merged and sorted once. Local IDs map
to dense byte-sorted ranks. Workers claim hours and count/scatter their latencies
into contiguous endpoint groups, then compute exact nearest-rank p50/p95/p99.
Duplicate requests count independently; errors and ppm use integer arithmetic.
Completed buffers are written in hour order, omitting empty groups.

Consumed block prefixes are returned with batched `madvise` calls as hours finish,
so record pages can be reclaimed while output pages grow. A Release/Acquire
completion flag proves each block's reader has finished; one reclamation lock
protects the prefix cursors. Heavy input disorder can delay reclamation, but
never permits dropping a record. Linux arena mappings disable huge-page backing
with a hint so sparsely used blocks need not charge whole huge pages.

Unsafe code covers Unix mappings, SIMD loads and exclusive arena writes followed
by read-only block traversal. The mapping guard outlives all workers and borrowed
endpoint bytes; each hour has exactly one consumer. `a_ingest.py` and `run.py`
remain byte-identical correctness oracles.

The compact-record pipeline was informed by
[Cache Me's public scored A implementation](https://github.com/manusingh090/speed-demon-2026-entry/blob/836c19e46966bcbe9e91e79883f148059402f1c9/track-a/main.cpp).

The Docker build enables `target-cpu=native`, which the published board rules
route to a build on the scoring machine. No CPU features are assumed from this
Apple Silicon development host.

## Dependencies

None.

## Run and verify

The current candidate skips local runtime tests and benchmarks as requested by
the user. Formatting and native/x86_64 Linux compilation checks passed. The
commands below are available for verification; they were not run for this step.

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

The preceding implementation's historical best is 75.973214 points, 49.090 ms
and 19,177,472 peak bytes. The compact candidate awaits its Intel measurement;
see [PERF.md](PERF.md).

## With another 24 hours

Measure the compact pipeline on Intel, then profile parsing, dictionary
reconciliation, percentile selection and output. Compare any runtime gain with
its extra source and peak memory, including seeds with highly disordered hours.

## Profile-guided experiment

Build-time PGO trained on generated sample seeds 7, 19 and 103 passed correctness
on ARM64 and AMD64. Intel board scores fell after memory and source costs, so
the ordinary native release build is retained. The reproducible experiment is
preserved at commit `3b74c524ad473105b5bfa934855984fe98336eea`; see [PERF.md](PERF.md).
