#!/usr/bin/env python3
"""Track A: exact hourly summaries with streaming input and uint32 storage."""
import argparse
from array import array
import os


def main(data_dir, out_dir):
    groups = {}
    with open(os.path.join(data_dir, "requests.log"), "rb") as stream:
        for line in stream:
            # The grammar fixes the UTC timestamp at 27 bytes plus one space.
            _method, endpoint, status, latency = line[28:].split(b" ")
            key = (line[:13], endpoint)
            group = groups.get(key)
            if group is None:
                # Keep every latency exactly, including the full uint32 range.
                groups[key] = group = [array("I"), 0]
            group[0].append(int(latency))
            group[1] += status[0] == 53  # All valid statuses are three digits, 100..599.

    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "summary.txt"), "wb") as stream:
        # Fixed-width UTC hours and ASCII endpoints sort in the required order.
        for hour, endpoint in sorted(groups):
            values, errors = groups[(hour, endpoint)]
            values = sorted(values)
            count = len(values)
            p50 = values[(50 * count + 99) // 100 - 1]
            p95 = values[(95 * count + 99) // 100 - 1]
            p99 = values[(99 * count + 99) // 100 - 1]
            summary = (
                f"{hour.decode('ascii')}:00:00Z {endpoint.decode('ascii')} "
                f"{count} {p50} {p95} {p99} {errors} {errors * 1_000_000 // count}\n"
            )
            stream.write(summary.encode("ascii"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default="/data")
    parser.add_argument("--out", default="/out")
    args = parser.parse_args()
    main(args.data, args.out)
