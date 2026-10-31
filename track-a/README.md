# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

Stream the input as bytes. The timestamp grammar fixes the UTC hour in its first
13 bytes, so grouping needs no datetime parsing. One dictionary holds each
`(hour, endpoint)` group's latencies and 5xx count. Statuses are exactly three
digits from 100 to 599, so their first digit identifies errors.

Store latencies in standard-library `array('I')` buffers, preserving the complete
uint32 range without retaining a Python integer object for every request. At the
end, sort the keys by byte order and each group's values numerically. Read the
three nearest-rank percentiles and compute error parts per million with integer
arithmetic. Write each summary directly to the buffered output stream.

No assumptions about arrival order, endpoint popularity, latency distribution or
seed are needed. Duplicate requests count separately; empty input produces an
empty output. The CLI remains `--data DIR --out DIR`, and `run.py` calls `main`.

## Dependencies

None beyond Python 3.13's standard library. The submission uses the existing
pinned Python image and declares zero runtime dependencies.

## Validation

Run the independent reference comparisons and tooling tests from the repo root:

```sh
python3.13 -m unittest discover -s tests -v
```

See [PERF.md](PERF.md) for native measurements and
[the experiment archive](../experiments/2026-10-31-track-a/README.md) for complete
candidate sources, raw reports, oracle provenance and reproduction commands.

## With another 24 hours

Measure this version on the official Intel runner before claiming a contest
score. Then profile parsing and per-group sorting on that machine. Test any
selection algorithm against exact sorting, especially tied values and coincident
percentile ranks, and retain it only if time, memory and counted source improve
the overall coefficient on both full seeds.
