# Track C: Join

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

Stream records through Python's standard CSV parser with `newline=""`. It handles
quoted commas, doubled quotes, embedded newlines, mixed record terminators and
the final record without a terminator. Remove a single BOM only from the start
of the unquoted id field; BOMs inside names and notes remain literal data.

Load the region lookup once and group each record by its country's code, or
`_UNMATCHED`. Convert the valid two-decimal amount directly to signed integer
cents, avoiding floating point and repeated regular-expression validation.
Update count, id sum, amount sum/min/max and byte totals in one streaming loop.

Share one exact string for each name across countries, then keep a set of those
strings per group. This reduces retained duplicate strings while preserving
trailing spaces, empty names, escaped quotes and Unicode identity. UTF-8 byte
lengths are computed after CSV unescaping. Only groups with records are emitted,
in byte order, with exact signed money formatting.

The CLI remains `--data DIR --out DIR`, and `run.py` calls `main`. No seed,
country count, name-pool size or record-order shortcut is required.

## Dependencies

None beyond Python 3.13's standard library. The existing pinned Python image and
zero runtime dependency declaration are unchanged.

## Validation

From the repository root:

```sh
python3.13 -m unittest discover -s tests -v
```

Tests execute the actual CLI against an independent pinned reference. They cover
the frozen worked example, BOM placement, multiline and escaped quotes, mixed
line endings and EOF, exact name identity, integer overflow boundaries,
negative amounts, unmatched regions and randomized hostile CSV records.

See [PERF.md](PERF.md) for native measurements and
[the experiment archive](../experiments/2026-10-31-tracks-bcd/README.md) for complete
candidate sources, raw reports, oracle provenance and reproduction commands.

## With another 24 hours

Measure on the official Intel runner before claiming a contest score. Profile
parsing, join lookups and retained name sets there. Compare more compact name
identities against this exact-string baseline, including their construction and
collision handling. Keep the full CSV edge-case suite and independent oracle
checks for every parser or representation change.
