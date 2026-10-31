#!/usr/bin/env python3
"""Speed Demon 2026, Track A (Ingest): the naive reference implementation.

    python3.13 reference/a_ingest.py [--data /data] [--out /out]

Reads requests.log and writes summary.txt: for every (UTC hour, endpoint) pair that
has at least one request, the request count, the p50/p95/p99 latency (nearest rank) and
the number and rate of 5xx responses. specs/a-ingest.md is the contract.

This is the obvious version on purpose, and correct: read lines, split them, parse each
timestamp with the datetime library, append each latency to a list per group, sort
every list at the end.
"""
import argparse
import os
import resource
import sys
import threading
import time
import tomllib
from datetime import datetime
from pathlib import Path

INPUT_NAME = "requests.log"
OUTPUT_NAME = "summary.txt"
TIMESTAMP_FORMAT = "%Y-%m-%dT%H:%M:%S.%f%z"   # RFC 3339; %z reads the trailing Z as UTC
STATS_INTERVAL = 3.0


def percentile(sorted_values, p):
    """Nearest rank: the value at 1-indexed rank ceil(p * n / 100)."""
    n = len(sorted_values)
    rank = (p * n + 99) // 100   # ceil(p * n / 100) in integer arithmetic
    return sorted_values[rank - 1]


def main(data_dir, out_dir):
    started = time.monotonic()
    track_dir = Path(__file__).resolve().parent
    with (track_dir / ".speed-demon.toml").open("rb") as manifest:
        dependency_weight = 1 + tomllib.load(manifest)["dependency_count"]
    # A local estimate only; the harness counts formatted source with scc.
    source_lines = sum(
        bool(line.strip()) and not line.lstrip().startswith("#")
        for path in track_dir.rglob("*")
        if path.suffix == ".py" or path.name == "Dockerfile"
        for line in path.read_text(encoding="utf-8").splitlines()
    )
    stopped = threading.Event()

    def report(state):
        elapsed = time.monotonic() - started
        peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        peak_mb = peak / 1_000_000 if sys.platform == "darwin" else peak * 1024 / 1_000_000
        coefficient = (
            (32.05 / max(elapsed, 1e-9)) ** 0.50
            * min(228 / max(peak_mb, 1e-9), 50) ** 0.20
            * min(56 / max(source_lines, 1), 1) ** 0.15
            * min(1 / dependency_weight, 1) ** 0.15
        )
        print(
            f"[stats {state}] T={elapsed:.2f} s "
            f"M={peak_mb:.2f} MB S~{source_lines} lines D={dependency_weight} "
            f"K~{coefficient:.3f}",
            file=sys.stderr, flush=True,
        )

    def report_periodically():
        while not stopped.wait(STATS_INTERVAL):
            report("running")

    print(
        "[stats] Local diagnostics: T=elapsed; M=process peak RSS; "
        "S=nonblank/noncomment Python + Dockerfile lines before formatting/scc; "
        "D=1 + manifest runtime dependencies. Official T/M/S are measured by the harness. "
        "K uses these local estimates and is provisional while running.\n"
        "[stats] References (weight, ratio cap): T=32.05 s (0.50, none); "
        "M=228 MB (0.20, 50x); S=56 lines (0.15, 1x); D=1 (0.15, 1x).",
        file=sys.stderr, flush=True,
    )
    report("start")
    reporter = threading.Thread(target=report_periodically, daemon=True)
    reporter.start()
    try:
        ingest(data_dir, out_dir)
    finally:
        stopped.set()
        reporter.join()
        report("final")


def ingest(data_dir, out_dir):
    latencies = {}   # (hour, endpoint) -> list of latency_us, in input order
    errors = {}      # (hour, endpoint) -> number of 5xx responses

    with open(os.path.join(data_dir, INPUT_NAME), encoding="utf-8") as f:
        for line in f:
            ts, method, endpoint, status, latency_us = line.rstrip("\n").split(" ")
            when = datetime.strptime(ts, TIMESTAMP_FORMAT)
            hour = when.replace(minute=0, second=0, microsecond=0)
            key = (hour, endpoint)
            if key not in latencies:
                latencies[key] = []
                errors[key] = 0
            latencies[key].append(int(latency_us))
            if 500 <= int(status) <= 599:
                errors[key] += 1

    lines = []
    # Endpoints are ASCII (see the spec), so Python's string order is bytewise order.
    for key in sorted(latencies):
        hour, endpoint = key
        values = sorted(latencies[key])
        count = len(values)
        errs = errors[key]
        ppm = errs * 1_000_000 // count
        lines.append(
            f"{hour.strftime('%Y-%m-%dT%H:00:00Z')} {endpoint} {count} "
            f"{percentile(values, 50)} {percentile(values, 95)} {percentile(values, 99)} "
            f"{errs} {ppm}\n"
        )

    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, OUTPUT_NAME), "w", encoding="utf-8", newline="\n") as f:
        f.write("".join(lines))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Track A (Ingest) reference implementation.")
    parser.add_argument("--data", default="/data", help="directory holding requests.log")
    parser.add_argument("--out", default="/out", help="directory to write summary.txt into")
    args = parser.parse_args()
    main(args.data, args.out)
    sys.exit(0)
