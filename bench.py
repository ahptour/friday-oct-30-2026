#!/usr/bin/env python3
"""Benchmark Python tracks natively against a pinned, independent reference.

Python 3.11+, git and /usr/bin/time; no third-party packages or Docker.
See BENCHMARKING.md for measurement limits and experiment commands.
"""
import argparse
import ast
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import statistics
import subprocess
import sys
import threading
import time
import tokenize
import tomllib
from datetime import datetime, timezone

from score import compare, score

ROOT = Path(__file__).resolve().parent
REFERENCE_REV = "54a4eb2af132d507bd4f2e2e12f388f7e7d02142"
TRACKS = {
    "a": ("a_ingest.py", "summary.txt", 56),
    "b": ("b_index.py", "results.txt", 91),
    "c": ("c_join.py", "aggregates.txt", 108),
    "d": ("d_traverse.py", "distances.txt", 43),
}
SNAPSHOT_IGNORES = ("__pycache__", "*.pyc", ".git", "target")


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def file_hashes(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if any(part in {"__pycache__", ".git", "target"} for part in path.relative_to(directory).parts) or path.suffix == ".pyc":
            continue
        if path.is_symlink():
            raise ValueError(f"symlinks are unsupported in benchmark inputs/snapshots: {path}")
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = sha256(path)
    return result


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def snapshot_reference(track, dest):
    prefix = f"track-{track}/"
    names = git("ls-tree", "-rz", "--name-only", REFERENCE_REV, "--", prefix).decode().split("\0")
    dest.mkdir()
    for name in filter(None, names):
        relative = Path(name).relative_to(prefix)
        target = dest / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(git("show", f"{REFERENCE_REV}:{name}"))
    if not (dest / TRACKS[track][0]).is_file():
        raise ValueError("pinned reference is absent from git history; fetch the initial commit")


def python_lines(source):
    """Physical code-line estimate; excludes comments and standalone docstrings."""
    tree = ast.parse(source)
    docstrings = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) and node.body:
            first = node.body[0]
            if isinstance(first, ast.Expr) and isinstance(first.value, ast.Constant) and isinstance(first.value.value, str):
                docstrings.add((first.value.lineno, first.value.col_offset))
    ignored = {tokenize.ENCODING, tokenize.ENDMARKER, tokenize.NEWLINE, tokenize.NL, tokenize.INDENT, tokenize.DEDENT, tokenize.COMMENT}
    lines = set()
    for token in tokenize.generate_tokens(io.StringIO(source).readline):
        if token.type not in ignored and not (token.type == tokenize.STRING and token.start in docstrings):
            lines.update(range(token.start[0], token.end[0] + 1))
    return len(lines)


def source_estimate(directory, published_hashes):
    counts, exempt, unsupported = {}, [], []
    for name, digest in file_hashes(directory).items():
        path = directory / name
        if path.suffix == ".py" and digest in published_hashes:
            exempt.append(name)
        elif path.suffix == ".py":
            counts[name] = python_lines(path.read_text(encoding="utf-8-sig"))
        elif path.name == "Dockerfile":
            counts[name] = sum(bool(line.strip()) and not line.lstrip().startswith("#") for line in path.read_text().splitlines())
        elif path.suffix.lower() not in {".md", ".txt", ".toml", ".json", ".csv", ".yaml", ".yml"} and path.name not in {"LICENSE", ".gitignore", ".dockerignore"}:
            unsupported.append(name)
    return {"lines": sum(counts.values()), "method": "estimate: unformatted Python tokens plus Dockerfile code lines; no registry exemptions", "by_file": counts, "published_copies_excluded": exempt, "unsupported_files": unsupported}


def trimmed_mean(samples):
    values = sorted(samples)
    return statistics.mean(values[1:-1] if len(values) >= 5 else values)


def warm_inputs(data):
    for path in sorted(data.rglob("*")):
        if path.is_file():
            with path.open("rb") as stream:
                while stream.read(1 << 20):
                    pass


def execute(command, cwd, env, log_prefix, timeout):
    """Blocking wait keeps short runs free of subprocess timeout polling delays."""
    timed_out = threading.Event()
    with log_prefix.with_suffix(".stdout").open("wb") as out, log_prefix.with_suffix(".stderr").open("wb") as err:
        started = time.perf_counter()
        proc = subprocess.Popen(command, cwd=cwd, env=env, stdout=out, stderr=err, start_new_session=True)

        def kill():
            timed_out.set()
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, kill)
        timer.daemon = True
        timer.start()
        try:
            code = proc.wait()
            elapsed = time.perf_counter() - started
        except BaseException:
            kill()
            proc.wait()
            raise
        finally:
            timer.cancel()
            timer.join()
    return {"elapsed_s": elapsed, "returncode": code, "timed_out": timed_out.is_set()}


def memory_peak(path):
    text = path.read_text()
    if sys.platform == "darwin":
        rss = next(int(line.split()[0]) for line in text.splitlines() if "maximum resident set size" in line)
        footprint = next((int(line.split()[0]) for line in text.splitlines() if "peak memory footprint" in line), None)
    else:
        rss = next(int(line.rsplit(":", 1)[1]) * 1024 for line in text.splitlines() if "Maximum resident set size (kbytes)" in line)
        footprint = None
    return {"maximum_rss_bytes": rss, "peak_footprint_bytes": footprint}


def run_once(report, variant, kind, number, args, expected=None):
    name = f"{kind}-{number:02d}-{variant}"
    work = Path(report["output_dir"]) / "runs" / name
    work.mkdir(parents=True)
    out = work / "out"
    out.mkdir()
    source = Path(report["output_dir"]) / variant
    command = [sys.executable, "-B", str(source / TRACKS[args.track][0]), "--data", str(args.data), "--out", str(out)]
    if kind == "memory":
        command = ["/usr/bin/time", "-l" if sys.platform == "darwin" else "-v", "-o", str(work / "memory.txt"), *command]
    env = dict(os.environ, PYTHONHASHSEED="0", PYTHONDONTWRITEBYTECODE="1", TRACK=args.track, LC_ALL="C")
    warm_inputs(args.data)
    sample = {"variant": variant, "kind": kind, "number": number, "command": command, "work_dir": str(work)}
    report["samples"].append(sample)
    sample.update(execute(command, source, env, work / "process", args.timeout))
    if sample["timed_out"] or sample["returncode"]:
        raise ValueError(f"{name} {'timed out' if sample['timed_out'] else 'failed'}; see {work / 'process.stderr'}")
    files = list(out.iterdir())
    output = out / TRACKS[args.track][1]
    if files != [output] or not output.is_file() or output.is_symlink():
        raise ValueError(f"{name}: output must contain exactly {output.name}")
    sample["output_sha256"] = sha256(output)
    sample["correct"] = expected is None or sample["output_sha256"] == expected
    if not sample["correct"]:
        raise ValueError(f"{name}: output hash mismatch: {sample['output_sha256']} != {expected}")
    if kind == "memory":
        sample.update(memory_peak(work / "memory.txt"))
    return sample


def summarize(report, variant, source, dependencies):
    timed = [s["elapsed_s"] for s in report["samples"] if s["variant"] == variant and s["kind"] == "timing"]
    peaks = [s["maximum_rss_bytes"] for s in report["samples"] if s["variant"] == variant and s["kind"] == "memory"]
    return {
        "metrics": {"T": trimmed_mean(timed), "M": max(peaks), "S": source, "D": 1 + dependencies},
        "timing": {"raw_s": timed, "min_s": min(timed), "median_s": statistics.median(timed), "max_s": max(timed), "stdev_s": statistics.stdev(timed) if len(timed) > 1 else 0, "method": "drop one lowest and one highest, mean remaining" if len(timed) >= 5 else "arithmetic mean (fewer than five samples)"},
        "memory_raw_bytes": peaks,
    }


def benchmark(args, report):
    directory = Path(report["output_dir"])
    reference = directory / "reference"
    candidate = directory / "candidate"
    snapshot_reference(args.track, reference)
    original_hashes = file_hashes(args.track_dir)
    shutil.copytree(args.track_dir, candidate, ignore=shutil.ignore_patterns(*SNAPSHOT_IGNORES))
    candidate_hashes = file_hashes(candidate)
    if original_hashes != candidate_hashes:
        raise ValueError("candidate files changed while being copied")
    ref_hashes = file_hashes(reference)
    report.update({
        "reference_revision": REFERENCE_REV,
        "reference_files_sha256": ref_hashes,
        "candidate_files_sha256": candidate_hashes,
        "input_files_sha256": file_hashes(args.data),
        "git_head": git("rev-parse", "HEAD").decode().strip(),
        "git_status": git("status", "--short").decode(),
    })
    estimate = source_estimate(candidate, {digest for name, digest in ref_hashes.items() if name.endswith(".py")})
    report["source_size"] = estimate
    if args.source_lines is None and estimate["unsupported_files"]:
        raise ValueError("source estimate cannot count these files; supply --source-lines: " + ", ".join(estimate["unsupported_files"]))
    report["source_size"]["used_lines"] = estimate["lines"] if args.source_lines is None else args.source_lines
    report["source_size"]["basis"] = "estimated" if args.source_lines is None else "user-supplied; verify against official static axes"
    if args.dependencies is None:
        manifest = tomllib.loads((candidate / ".speed-demon.toml").read_text())
        dependencies = manifest["dependency_count"]
        report["dependencies_basis"] = ".speed-demon.toml declaration; not a manifest audit"
    else:
        dependencies = args.dependencies
        report["dependencies_basis"] = "user-supplied direct runtime dependency count"
    if isinstance(dependencies, bool) or not isinstance(dependencies, int) or dependencies < 0:
        raise ValueError("dependency_count must be a nonnegative integer")

    print("Running pinned reference to establish exact output…", flush=True)
    oracle = run_once(report, "reference", "oracle", 0, args, args.expected)
    expected = oracle["output_sha256"]
    report["correctness"] = {"oracle": "fresh pinned-reference process", "expected_sha256": expected, "external_expected_sha256": args.expected}
    for kind, count in (("warmup", args.warmups), ("timing", args.runs), ("memory", args.memory_runs)):
        for number in range(count):
            order = ("reference", "candidate") if number % 2 == 0 else ("candidate", "reference")
            for variant in order:
                sample = run_once(report, variant, kind, number, args, expected)
                print(f"{kind:7} {number + 1:2} {variant:9} {sample['elapsed_s'] * 1000:10.3f} ms  hash OK", flush=True)

    for path, hashes in ((args.data, report["input_files_sha256"]), (reference, ref_hashes), (candidate, candidate_hashes)):
        if file_hashes(path) != hashes:
            raise ValueError(f"files changed during the benchmark: {path}")
    report["production_source_unchanged"] = file_hashes(args.track_dir) == original_hashes
    if not report["production_source_unchanged"]:
        raise ValueError("production source changed during benchmark; rerun with a stable source tree")
    if sha256(Path(sys.executable).resolve()) != report["runtime"]["executable_sha256"]:
        raise ValueError("Python executable changed during benchmark")
    report["reference"] = summarize(report, "reference", TRACKS[args.track][2], 0)
    report["candidate"] = summarize(report, "candidate", report["source_size"]["used_lines"], dependencies)
    ref = report["reference"]["metrics"]
    metrics = report["candidate"]["metrics"]
    report["candidate"]["score"] = score(metrics, ref)
    old = ref
    report["comparison_basis"] = "fresh local reference"
    if args.baseline:
        prior = json.loads(args.baseline.read_text())
        for key in ("track", "runtime", "machine", "input_files_sha256", "reference_files_sha256", "measurement"):
            if prior[key] != report[key]:
                raise ValueError(f"baseline {key} differs; cannot compare these local reports")
        if prior["status"] != "passed":
            raise ValueError("baseline report did not pass correctness")
        old = prior["candidate"]["metrics"]
        report["comparison_basis"] = f"previous candidate in {args.baseline}; separate batch, rescored against current local reference"
        report["baseline_metrics"] = old
    report["comparison"] = compare(old, metrics, ref, args.gain)
    report["aggregate"] = None
    report["status"] = "passed"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("track", choices=sorted(TRACKS))
    ap.add_argument("--data", required=True, type=Path)
    ap.add_argument("--seed", required=True, type=int, help="dataset label, recorded with input hashes")
    ap.add_argument("--scale", required=True, choices=("tiny", "sample", "full", "custom"))
    ap.add_argument("--track-dir", type=Path, help="candidate folder; defaults to track-<track>")
    ap.add_argument("--output-dir", type=Path, help="fresh evidence directory; defaults to .out/python-bench/<timestamp>")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--warmups", type=int, default=1)
    ap.add_argument("--memory-runs", type=int, default=1, help="separate untimed memory runs per variant")
    ap.add_argument("--timeout", type=float, default=600, help="seconds per process")
    ap.add_argument("--expected", help="optional trusted output sha256 for this exact dataset")
    ap.add_argument("--source-lines", type=int, help="override estimated S with verified counted lines")
    ap.add_argument("--dependencies", type=int, help="direct runtime dependencies; default: track declaration")
    ap.add_argument("--baseline", type=Path, help="prior local report.json to compare, with matching environment and inputs")
    ap.add_argument("--gain", type=float, default=5, help="target pre-clamp machine-point gain")
    args = ap.parse_args()
    args.data = args.data.resolve()
    args.track_dir = (args.track_dir or ROOT / f"track-{args.track}").resolve()
    if args.runs < 1 or args.warmups < 0 or args.memory_runs < 1 or not 0 < args.timeout < float("inf") or not 0 <= args.gain < float("inf"):
        ap.error("runs/memory-runs/timeout must be positive; warmups/gain must be nonnegative")
    if args.source_lines is not None and args.source_lines < 0:
        ap.error("source lines must be nonnegative")
    if not args.data.is_dir() or not (args.track_dir / TRACKS[args.track][0]).is_file():
        ap.error("data directory and candidate track module must exist")
    if args.expected:
        args.expected = args.expected.lower().strip()
        if len(args.expected) != 64 or any(c not in "0123456789abcdef" for c in args.expected):
            ap.error("expected must be a SHA-256 hex digest")
    if sys.platform not in ("darwin", "linux") or not Path("/usr/bin/time").is_file():
        ap.error("native measurement requires macOS or Linux and /usr/bin/time")
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    directory = (args.output_dir or ROOT / ".out" / "python-bench" / f"{args.track}-{stamp}").resolve()
    if directory.is_relative_to(args.track_dir) or directory.is_relative_to(args.data):
        ap.error("evidence directory must be outside the candidate and data folders")
    try:
        directory.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        ap.error("evidence directory already exists; choose a new --output-dir")
    report = {
        "schema_version": 1, "status": "running", "track": args.track, "seed": args.seed, "scale": args.scale,
        "started_utc": stamp, "output_dir": str(directory), "data_dir": str(args.data), "track_dir": str(args.track_dir),
        "runtime": {"executable": sys.executable, "version": sys.version, "executable_sha256": sha256(Path(sys.executable).resolve()), "inherited_environment": {key: os.environ.get(key) for key in ("PYTHONPATH", "PYTHONHOME", "PYTHONMALLOC", "PYTHONOPTIMIZE", "PYTHON_JIT", "PYTHON_GIL", "OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "VECLIB_MAXIMUM_THREADS")}},
        "machine": {"system": platform.system(), "release": platform.release(), "machine": platform.machine(), "host": platform.node(), "logical_cpus": os.cpu_count()},
        "measurement": {"time": "native parent monotonic spawn-to-exit; includes process launch, imports, setup, work and output", "memory": "/usr/bin/time maximum RSS, highest separate run; not cgroup memory.peak", "memory_flag": "-l" if sys.platform == "darwin" else "-v", "environment_overrides": {"PYTHONHASHSEED": "0", "PYTHONDONTWRITEBYTECODE": "1", "TRACK": args.track, "LC_ALL": "C"}},
        "settings": {"runs": args.runs, "warmups": args.warmups, "memory_runs": args.memory_runs, "timeout_s": args.timeout},
        "score_basis": "LOCAL ESTIMATE: measured local reference T/M, published Sref, estimated or supplied candidate S, declared D",
        "limitations": ["Local K/points are not official Intel scores or sandbox validation.", "Timing includes process-launch overhead; memory is RSS, not Linux cgroup memory.peak.", "Five runs mimic the time trimming rule; use 7–10+ for selection and repeat marginal results.", "Seed/scale are user labels; input hashes establish identity.", "Aggregate needs coherent all-track points and explicit judge assumptions; use score.py for scenarios."],
        "samples": [],
    }
    try:
        benchmark(args, report)
    except (OSError, ValueError, KeyError, SyntaxError, subprocess.SubprocessError, StopIteration) as exc:
        report.update(status="failed", error=str(exc))
        print(f"FAIL  {exc}", file=sys.stderr)
    except KeyboardInterrupt:
        report.update(status="interrupted", error="interrupted by user")
    finally:
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        (directory / "report.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
        print(f"Evidence: {directory / 'report.json'}", flush=True)
    if report["status"] != "passed":
        return 1
    metrics, result = report["candidate"]["metrics"], report["candidate"]["score"]
    delta = report["comparison"]
    print(f"\nLOCAL ESTIMATE  T={metrics['T'] * 1000:.3f} ms  M={metrics['M'] / 1e6:.3f} MB RSS  S={metrics['S']} ({report['source_size']['basis']})  D={metrics['D']}")
    print(f"E={result['adjusted_runtime_s'] * 1000:.3f} ms  K={result['K']:.6f}  machine points={result['machine_points']:.6f}")
    print(f"Comparison: {report['comparison_basis']}")
    print(f"Machine point change={delta['machine_point_change']:+.6f}; pre-clamp contributions: " + "  ".join(f"{axis}={value:+.6f}" for axis, value in delta['pre_clamp_axis_point_changes'].items()))
    print(f"Break-even T<{delta['break_even_runtime_s'] * 1000:.3f} ms; +{args.gain:g} pre-clamp points target T<={delta['target_runtime_s'] * 1000:.3f} ms")
    return 0


if __name__ == "__main__":
    sys.exit(main())
