#!/usr/bin/env python3
"""Build, check and compare native solutions serially; never uses Docker."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import random
import re
import shutil
import statistics
import subprocess
import time
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
WORK = ROOT / '.out/native-first'
FLAGS = ['--edition=2024', '-C', 'opt-level=3', '-C', 'lto=thin',
         '-C', 'codegen-units=1', '-C', 'panic=abort', '-C', 'strip=symbols',
         '-C', 'target-cpu=native']
OUTPUT = {'a': 'summary.txt', 'b': 'results.txt', 'c': 'aggregates.txt', 'd': 'distances.txt'}


def digest_file(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def context(name):
    return WORK / name


def binary(name):
    return context(name) / 'target/release/solution'


def build(names):
    for name in names:
        track = name[0]
        target = context(name)
        (target / 'src').mkdir(parents=True, exist_ok=True)
        (target / 'target/release').mkdir(parents=True, exist_ok=True)
        shutil.copyfile(HERE / f'{name}.rs', target / 'src/main.rs')
        for f in ('Cargo.toml', 'Cargo.lock'):
            shutil.copyfile(ROOT / f'track-{track}' / f, target / f)
        command = ['rustc', *FLAGS, str(target / 'src/main.rs'), '-o', str(binary(name))]
        subprocess.run(command, check=True)
        metadata = dict(command=command, source_sha256=digest_file(target / 'src/main.rs'),
                        executable_sha256=digest_file(binary(name)),
                        compiler=subprocess.check_output(['rustc', '-vV'], text=True).strip())
        (target / 'build.json').write_text(json.dumps(metadata, indent=2) + '\n')
        print('built', name, flush=True)


def test(names):
    for name in names:
        track = name[0]
        spec = importlib.util.spec_from_file_location(f'test_{track}', ROOT / f'tests/test_track_{track}.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.TRACK = context(name)
        cls = getattr(module, f'Track{track.upper()}Test')
        cls.setUpClass = classmethod(lambda cls: None)
        print('testing', name, flush=True)
        if not unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(cls)).wasSuccessful():
            raise SystemExit(1)


def benchmark(names, seeds, trials, report_name):
    assert len({name[0] for name in names}) == 1
    track = names[0][0]
    rows = []
    source_hashes = {name: hashlib.sha256((HERE / f'{name}.rs').read_bytes()).hexdigest() for name in names}
    builds = {}
    input_hashes = {}
    oracle_hashes = {}
    for name in names:
        assert (HERE / f'{name}.rs').read_bytes() == (context(name) / 'src/main.rs').read_bytes(), f'rebuild {name}'
        builds[name] = json.loads((context(name) / 'build.json').read_text())
        assert builds[name]['source_sha256'] == source_hashes[name], f'rebuild {name}'
        assert builds[name]['executable_sha256'] == digest_file(binary(name)), f'rebuild {name}'
    for seed in seeds:
        data = ROOT / f'data/{track}/full-seed{seed}'
        expected_path = WORK / f'oracle-{track}-seed{seed}' / OUTPUT[track]
        if expected_path.exists():
            expected_bytes = expected_path.read_bytes()
            expected = hashlib.sha256(expected_bytes).hexdigest()
        else:
            spec = (ROOT / f'.out/setup/harness/specs/{dict(a="a-ingest",b="b-index",c="c-join",d="d-traverse")[track]}.md').read_text()
            raise RuntimeError(f'Missing oracle output {expected_path}; generate or copy verified reference output first')
        oracle_hashes[str(seed)] = dict(sha256=expected, path=str(expected_path),
                                       origin='previously verified reference output; not a fresh reference run')
        input_hashes[str(seed)] = {f.name: digest_file(f) for f in sorted(data.iterdir()) if f.is_file()}
        # Warm page cache outside the timed process. Benchmarks remain serial.
        for f in data.iterdir():
            if f.is_file():
                with f.open('rb') as handle:
                    while handle.read(8 * 1024 * 1024):
                        pass
        for trial in range(-2, trials + 3):
            phase = 'warmup' if trial < 0 else 'timing' if trial < trials else 'memory'
            order = names.copy()
            random.Random(8191 + trial).shuffle(order)
            for name in order:
                out = WORK / 'output' / name
                out.mkdir(parents=True, exist_ok=True)
                command = [str(binary(name)), '--data', str(data), '--out', str(out)]
                if phase == 'memory':
                    command = ['/usr/bin/time', '-l', *command]
                start = time.perf_counter_ns()
                process = subprocess.run(command, check=True, capture_output=True, text=True)
                elapsed = (time.perf_counter_ns() - start) / 1e6
                assert sorted(p.name for p in out.iterdir()) == [OUTPUT[track]]
                actual_bytes = (out / OUTPUT[track]).read_bytes()
                assert actual_bytes == expected_bytes, (name, seed, 'output bytes differ from reference')
                digest = hashlib.sha256(actual_bytes).hexdigest()
                assert digest == expected, (name, seed, digest, expected)
                row = dict(variant=name, seed=seed, trial=trial, phase=phase, elapsed_ms=elapsed, sha256=digest)
                if process.stderr:
                    row['stderr'] = process.stderr
                for field in ('maximum resident set size', 'peak memory footprint'):
                    match = re.search(r'(\d+)\s+' + field, process.stderr)
                    if match:
                        row[field] = int(match.group(1))
                rows.append(row)
        print('completed seed', seed, flush=True)
    summary = {}
    for seed in seeds:
        for name in names:
            timing = [r['elapsed_ms'] for r in rows if r['seed'] == seed and r['variant'] == name and r['phase'] == 'timing']
            memory = [r for r in rows if r['seed'] == seed and r['variant'] == name and r['phase'] == 'memory']
            summary[f'{name}/seed{seed}'] = dict(median_ms=statistics.median(timing), min_ms=min(timing), max_ms=max(timing),
                max_rss_bytes=max(r['maximum resident set size'] for r in memory),
                max_peak_footprint_bytes=max(r['peak memory footprint'] for r in memory))
    report = dict(recorded_utc=datetime.now(timezone.utc).isoformat(),
                  runner_sha256=digest_file(Path(__file__)),
                  build=builds, input_sha256=input_hashes, oracle=oracle_hashes,
                  platform=platform.platform(), machine=platform.machine(),
                  compiler=subprocess.check_output(['rustc', '--version'], text=True).strip(),
                  flags='opt-level=3 lto=thin codegen-units=1 panic=abort strip=symbols target-cpu=native',
                  timing='perf_counter_ns around native subprocess; 2 warmups; randomized serialized order',
                  memory='separate macOS time -l runs; not Linux cgroup memory.peak',
                  source_sha256=source_hashes, summary=summary, rows=rows)
    (HERE / report_name).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['build', 'test', 'bench'])
    parser.add_argument('names', nargs='+')
    parser.add_argument('--seeds', nargs='+', type=int, default=[1, 42])
    parser.add_argument('--trials', type=int, default=9)
    parser.add_argument('--report', default='native-results.json')
    args = parser.parse_args()
    if args.action == 'bench':
        benchmark(args.names, args.seeds, args.trials, args.report)
    else:
        {'build': build, 'test': test}[args.action](args.names)
