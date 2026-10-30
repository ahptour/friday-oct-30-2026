#!/usr/bin/env python3
"""Reproduce isolated candidates, differential checks, and serialized Docker trials."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
WORK = ROOT / '.out/score-tradeoffs'
BASELINE = 'e2db23906110cb7dad9b86a79c5cf80a94a94334'
VARIANTS = {'c': ['base', 'u8', 'half'], 'b': ['w12', 'w8', 'w6', 'w4'],
            'd': ['base', 'contract', 'guarded', 'offset']}
HASHES = {'b': 'f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927',
          'c': 'd8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536',
          'd': 'a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e'}


def original(track, path):
    return subprocess.check_output(['git', 'show', f'{BASELINE}:track-{track}/{path}'],
                                   cwd=ROOT).decode()


def prepare():
    for track, variants in VARIANTS.items():
        for variant in variants:
            context = WORK / f'{track}-{variant}'
            (context / 'src').mkdir(parents=True, exist_ok=True)
            for filename in ['Cargo.toml', 'Cargo.lock', 'Dockerfile']:
                (context / filename).write_text(original(track, filename))
            source = original(track, 'src/main.rs')
            if track == 'c' and variant != 'base':
                for old, new in [('Vec<u16>', 'Vec<u8>'), ('0u16', '0u8'),
                                 ('as u16', 'as u8'), ('&[u16]', '&[u8]')]:
                    source = source.replace(old, new)
                if variant == 'half':
                    source = source.replace('1 << 18', '1 << 17')
            if track == 'b':
                source = source.replace('.min(12)', f'.min({variant[1:]})')
            if track == 'd' and variant != 'base':
                filename = {'contract': 'd-contraction.rs', 'guarded': 'd-guarded-contraction.rs',
                            'offset': 'd-text-offsets.rs'}[variant]
                source = (HERE / filename).read_text()
            (context / 'src/main.rs').write_text(source)


def differential(variant):
    context = WORK / f'd-{variant}'
    subprocess.run(['cargo', 'build', '--manifest-path', str(context / 'Cargo.toml'),
                    '--release', '--offline', '--locked'], check=True)
    spec = importlib.util.spec_from_file_location('dtests', ROOT / 'tests/test_track_d.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # Loads the unchanged Python reference first.
    module.TRACK = context
    module.TrackDTest.setUpClass = classmethod(lambda cls: None)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(module.TrackDTest)
    if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
        raise SystemExit(1)
    subprocess.run(['cargo', 'test', '--manifest-path', str(context / 'Cargo.toml'),
                    '--offline', '--locked'], check=True)


def build():
    for track, variants in VARIANTS.items():
        for variant in variants:
            name = f'{track}-{variant}'
            subprocess.run(['docker', 'build', '--network=none', '-q', '-t',
                            f'speeddemon-next/{name}', str(WORK / name)], check=True)
    # Time exec-to-exit and read the run cgroup's peak, including this tiny helper.
    timer = '''use std::{process::Command,time::Instant};
fn main() {
    let start=Instant::now();
    let status=Command::new("/app/solution").args(std::env::args_os().skip(1)).status().unwrap();
    assert!(status.success());
    println!("{} {}",start.elapsed().as_secs_f64(),std::fs::read_to_string("/sys/fs/cgroup/memory.peak").unwrap().trim());
}
'''
    (WORK / 'timer.rs').write_text(timer)
    image = original('d', 'Dockerfile').splitlines()[0].split()[1]
    subprocess.run(['docker', 'run', '--rm', '--network=none', '-v', f'{WORK}:/work',
                    '-w', '/work', image, 'rustc', '-O', 'timer.rs', '-o', 'timer'], check=True)


def benchmark(trials):
    results = {}
    for track, variants in VARIANTS.items():
        data = ROOT / f'data/{track}/full-seed1'
        # Host-side input prewarming must precede each measured run cgroup.
        subprocess.run(['docker', 'run', '--rm', '--network=none', '-v', f'{data}:/data:ro',
                        '--entrypoint', '/bin/sh', f'speeddemon-next/{track}-{variants[0]}',
                        '-c', 'cat /data/* >/dev/null'], check=True)
        for trial in range(trials):
            for variant in (variants if trial % 2 == 0 else variants[::-1]):
                out = Path(tempfile.mkdtemp(dir=WORK))
                out.chmod(0o777)
                name = f'{track}-{variant}'
                cmd = ['docker', 'run', '--rm', '--network=none', '--read-only', '--tmpfs',
                       '/tmp:rw,size=1g,exec', '--memory=8g', '--memory-swap=8g',
                       '--pids-limit=512', '--cap-drop=ALL', '--security-opt=no-new-privileges',
                       '--user', '65534:65534', '--cpuset-cpus=0-11', '-v', f'{data}:/data:ro',
                       '-v', f'{out}:/out:rw', '-v', f'{WORK}/timer:/timer:ro',
                       '--entrypoint', '/timer', f'speeddemon-next/{name}']
                subprocess.run(['docker', 'run', '--rm', '--network=none', '-v',
                                f'{data}:/data:ro', '--entrypoint', '/bin/sh',
                                f'speeddemon-next/{name}', '-c', 'cat /data/* >/dev/null'],
                               check=True)
                result = subprocess.run(cmd, capture_output=True, text=True, check=True)
                seconds, memory = result.stdout.split()
                files = list(out.iterdir())
                assert len(files) == 1, files
                digest = hashlib.sha256(files[0].read_bytes()).hexdigest()
                assert digest == HASHES[track], (name, digest)
                row = {'trial': trial, 'seconds': float(seconds), 'memory': int(memory),
                       'sha256': digest}
                results.setdefault(name, []).append(row)
                print(name, row, flush=True)
                (WORK / 'samples.json').write_text(json.dumps(results, indent=2) + '\n')
    for name, rows in results.items():
        warm = rows[1:]
        print(name, 'median', statistics.median(row['seconds'] for row in warm),
              'peak', max(row['memory'] for row in warm), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'test', 'build', 'bench'])
    parser.add_argument('--variant', choices=['contract', 'guarded', 'offset'], default='contract')
    parser.add_argument('--trials', type=int, default=7)
    args = parser.parse_args()
    if args.action == 'prepare':
        prepare()
    elif args.action == 'test':
        differential(args.variant)
    elif args.action == 'build':
        build()
    else:
        if args.trials < 2:
            parser.error('at least two trials are needed to exclude the cold trial')
        benchmark(args.trials)
