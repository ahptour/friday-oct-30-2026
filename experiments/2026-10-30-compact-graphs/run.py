#!/usr/bin/env python3
"""Reproduce exact layout candidates and serialized local Docker comparisons."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
WORK = ROOT / '.out/compact-graphs'
BASELINE = '00f82d78'
VARIANTS = {'a': ['base', 'persistent'],
            'd': ['base', 'tag10', 'escape11', 'reverse']}
OUTPUTS = {'a': 'summary.txt', 'd': 'distances.txt'}
HASHES = {
    'a': {1: '495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3',
          42: 'b148ea775ca3fe99edb1329adfc5e85f75f9a67c9dffc37a2caff92d3bbdf9b1'},
    'd': {1: 'a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e',
          42: '0069046cc261b9825d90583ea3e6417d9728b8e7356b3fbde986f65bdeed7945'},
}


def context(track, variant):
    return WORK / f'{track}-{variant}'


def image(track, variant):
    prefix = 'speeddemon-a-persistent' if track == 'a' else 'speeddemon-d-layout'
    return f'{prefix}/{"candidate" if variant == "persistent" else variant}'


def prepare(tracks):
    for track in tracks:
        for variant in VARIANTS[track]:
            directory = context(track, variant)
            (directory / 'src').mkdir(parents=True, exist_ok=True)
            for name in ['Cargo.toml', 'Cargo.lock', 'Dockerfile', 'src/main.rs']:
                content = subprocess.check_output(
                    ['git', 'show', f'{BASELINE}:track-{track}/{name}'], cwd=ROOT)
                (directory / name).write_bytes(content)
            if variant != 'base':
                subprocess.run(['patch', '--batch', '--forward', '-p1', '-i',
                                str(HERE / f'{track}-{variant}.patch')],
                               cwd=directory, check=True)


def build(tracks):
    for track in tracks:
        for variant in VARIANTS[track]:
            subprocess.run(['docker', 'build', '--network=none', '-q', '-t',
                            image(track, variant), str(context(track, variant))], check=True)
    rust = (context(tracks[0], 'base') / 'Dockerfile').read_text().splitlines()[0].split()[1]
    timer = '''use std::{process::Command,time::Instant};
fn main() {
    let start=Instant::now();
    let status=Command::new("/app/solution").status().unwrap();
    assert!(status.success());
    let seconds=start.elapsed().as_secs_f64();
    let stat=std::fs::read_to_string("/sys/fs/cgroup/memory.stat").unwrap();
    let field=|key| stat.lines().find_map(|line| line.strip_prefix(key)).unwrap().trim().parse::<u64>().unwrap();
    println!("{} {} {} {}",seconds,std::fs::read_to_string("/sys/fs/cgroup/memory.peak").unwrap().trim(),field("file "),field("anon "));
}
'''
    (WORK / 'timer.rs').write_text(timer)
    subprocess.run(['docker', 'run', '--rm', '--network=none', '-v', f'{WORK}:/work',
                    rust, 'rustc', '-O', '/work/timer.rs', '-o', '/work/timer'], check=True)
    # Pin only in this separate, idle diagnostic container, never in a solver.
    warmer = '''use std::{ffi::c_void,fs::{self,File},io::Write,os::fd::AsRawFd};
unsafe extern "C" {
    fn mmap(p:*mut c_void,n:usize,prot:i32,flags:i32,fd:i32,off:i64)->*mut c_void;
    fn mlock(p:*const c_void,n:usize)->i32;
}
fn main() {
    let mut files=Vec::new();
    for entry in fs::read_dir("/data").unwrap() {
        let file=File::open(entry.unwrap().path()).unwrap();
        let n=file.metadata().unwrap().len() as usize;
        if n>0 {
            let p=unsafe { mmap(std::ptr::null_mut(),n,1,2,file.as_raw_fd(),0) };
            assert_ne!(p as isize,-1);
            assert_eq!(unsafe { mlock(p,n) },0,"{}",std::io::Error::last_os_error());
        }
        files.push(file);
    }
    println!("ready");
    std::io::stdout().flush().unwrap();
    loop { std::thread::park(); }
}
'''
    (WORK / 'warmer.rs').write_text(warmer)
    subprocess.run(['docker', 'run', '--rm', '--network=none', '-v', f'{WORK}:/work',
                    rust, 'rustc', '-O', '/work/warmer.rs', '-o', '/work/warmer'], check=True)


def differential(tracks):
    for track in tracks:
        for variant in VARIANTS[track]:
            directory = context(track, variant)
            subprocess.run(['cargo', 'build', '--release', '--offline', '--locked',
                            '--manifest-path', str(directory / 'Cargo.toml')], check=True)
            spec = importlib.util.spec_from_file_location(
                f'{track}_tests', ROOT / f'tests/test_track_{track}.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)  # Load the unchanged Python oracle first.
            module.TRACK = directory
            test_class = getattr(module, f'Track{track.upper()}Test')
            test_class.setUpClass = classmethod(lambda cls: None)
            suite = unittest.defaultTestLoader.loadTestsFromTestCase(test_class)
            print(track, variant, flush=True)
            if not unittest.TextTestRunner(verbosity=1).run(suite).wasSuccessful():
                raise SystemExit(1)


def start_warmer(track, data):
    result = subprocess.run(
        ['docker', 'run', '-d', '--rm', '--network=none', '--read-only',
         '--memory=2g', '--memory-swap=2g', '--cap-drop=ALL', '--cap-add=IPC_LOCK',
         '--ulimit', 'memlock=-1', '--user', '0:0', '-v', f'{data}:/data:ro',
         '-v', f'{WORK}/warmer:/warmer:ro', '--entrypoint', '/warmer', image(track, 'base')],
        check=True, capture_output=True, text=True)
    container = result.stdout.strip()
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        logs = subprocess.run(['docker', 'logs', container], capture_output=True, text=True)
        if 'ready' in logs.stdout:
            return container
        state = subprocess.check_output(['docker', 'inspect', '--format',
                                         '{{.State.Running}}', container], text=True).strip()
        if state != 'true':
            break
        time.sleep(0.05)
    subprocess.run(['docker', 'rm', '-f', container], check=False, capture_output=True)
    raise RuntimeError(f'input pinning failed: {logs.stderr}')


def benchmark(tracks, seed, trials, cpuset, pin_inputs):
    report = {'baseline': BASELINE, 'host': platform.platform(), 'seed': seed,
              'cpuset': cpuset, 'prewarm': 'separate container before every run',
              'measurement': 'local process time and container cgroup memory.peak',
              'summary': 'exclude trial 0 per variant; median time, maximum memory',
              'source_sha256': {}, 'samples': {}}
    report['input_pages_pinned_outside_measured_cgroup'] = pin_inputs
    for track in tracks:
        data = ROOT / f'data/{track}/full-seed{seed}'
        assert data.is_dir(), data
        warmer = start_warmer(track, data) if pin_inputs else None
        try:
            for variant in VARIANTS[track]:
                key = f'{track}-{variant}'
                report['source_sha256'][key] = hashlib.sha256(
                    (context(track, variant) / 'src/main.rs').read_bytes()).hexdigest()
            for trial in range(trials):
                order = VARIANTS[track] if trial % 2 == 0 else VARIANTS[track][::-1]
                for variant in order:
                    subprocess.run(['docker', 'run', '--rm', '--network=none', '-v',
                                    f'{data}:/data:ro', '--entrypoint', '/bin/sh',
                                    image(track, variant), '-c', 'cat /data/* >/dev/null'],
                                   check=True)
                    with tempfile.TemporaryDirectory(dir=WORK) as temporary:
                        out = Path(temporary)
                        out.chmod(0o777)
                        command = ['docker', 'run', '--rm', '--network=none', '--read-only',
                                   '--tmpfs', '/tmp:rw,size=1g,exec', '--memory=8g',
                                   '--memory-swap=8g', '--pids-limit=512', '--cap-drop=ALL',
                                   '--security-opt=no-new-privileges', '--user', '65534:65534',
                                   '--cpuset-cpus', cpuset, '-v', f'{data}:/data:ro',
                                   '-v', f'{out}:/out:rw', '-v', f'{WORK}/timer:/timer:ro',
                                   '--entrypoint', '/timer', image(track, variant)]
                        result = subprocess.run(command, capture_output=True, text=True, check=True)
                        seconds, memory, file_memory, anon_memory = result.stdout.split()
                        assert sorted(p.name for p in out.iterdir()) == [OUTPUTS[track]]
                        digest = hashlib.sha256((out / OUTPUTS[track]).read_bytes()).hexdigest()
                        assert digest == HASHES[track][seed], (track, variant, digest)
                    row = {'trial': trial, 'seconds': float(seconds),
                           'memory': int(memory), 'sha256': digest,
                           'file_after': int(file_memory), 'anon_after': int(anon_memory)}
                    key = f'{track}-{variant}'
                    report['samples'].setdefault(key, []).append(row)
                    print(key, row, flush=True)
                    (WORK / f'samples-seed{seed}.json').write_text(json.dumps(report, indent=2) + '\n')
        finally:
            if warmer:
                subprocess.run(['docker', 'rm', '-f', warmer], check=True, capture_output=True)
    for key, rows in report['samples'].items():
        warm = rows[1:]
        print(key, 'median', statistics.median(r['seconds'] for r in warm),
              'peak', max(r['memory'] for r in warm), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'build', 'test', 'bench'])
    parser.add_argument('--track', choices=['a', 'd', 'all'], default='d')
    parser.add_argument('--seed', type=int, choices=[1, 42], default=1)
    parser.add_argument('--trials', type=int, default=9)
    parser.add_argument('--cpuset', default='0-11')
    parser.add_argument('--pin-inputs', action='store_true',
                        help='diagnostic mlocked input mappings in a separate idle container')
    args = parser.parse_args()
    tracks = list(VARIANTS) if args.track == 'all' else [args.track]
    if args.action == 'bench':
        if args.trials < 2:
            parser.error('at least two trials are required')
        benchmark(tracks, args.seed, args.trials, args.cpuset, args.pin_inputs)
    else:
        {'prepare': prepare, 'build': build, 'test': differential}[args.action](tracks)
