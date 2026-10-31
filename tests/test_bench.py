import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from bench import ROOT, python_lines, trimmed_mean


class HarnessTests(unittest.TestCase):
    def test_source_comments_docstrings_and_trimming(self):
        source = '"""module docs"""\n# comment\ndef f():\n    """function docs"""\n    return "data"  # useful comment\n'
        self.assertEqual(python_lines(source), 2)
        self.assertEqual(trimmed_mean([100, 1, 3, 4, 5]), 4)

    def run_bench(self, temp, candidate=None, *extra):
        data = temp / "data"
        data.mkdir(exist_ok=True)
        (data / "graph.txt").write_text("4 4\n0 1 3\n1 2 5\n0 2 20\n2 2 0\n")
        (data / "queries.txt").write_text("0 2\n2 0\n3 3\n")
        evidence = temp / "evidence"
        cmd = [sys.executable, str(ROOT / "bench.py"), "d", "--data", str(data), "--seed", "123", "--scale", "custom", "--runs", "1", "--warmups", "0", "--output-dir", str(evidence), *extra]
        if candidate:
            cmd += ["--track-dir", str(candidate)]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=20)
        report = json.loads((evidence / "report.json").read_text())
        return result, report, evidence

    def test_native_batch_checks_and_records_actual_processes(self):
        with tempfile.TemporaryDirectory() as temp:
            result, report, evidence = self.run_bench(Path(temp))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(report["status"], "passed")
            self.assertEqual(len(report["samples"]), 5)  # oracle + two timings + two memory runs
            self.assertTrue(all(s["correct"] for s in report["samples"]))
            self.assertEqual(report["candidate"]["metrics"]["D"], 1)
            self.assertGreater(report["candidate"]["metrics"]["M"], 0)
            self.assertIsNone(report["aggregate"])
            output = evidence / "runs" / "timing-00-candidate" / "out" / "distances.txt"
            self.assertEqual(output.read_bytes(), b"8\n-1\n0\n")
            self.assertTrue((evidence / "candidate" / "d_traverse.py").is_file())

    def test_isolated_wrong_output_mutation_fails_without_passing_k(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            candidate = temp / "candidate"
            shutil.copytree(ROOT / "track-d", candidate, ignore=shutil.ignore_patterns("__pycache__"))
            (candidate / "d_traverse.py").write_text('import argparse\nfrom pathlib import Path\np = argparse.ArgumentParser()\np.add_argument("--data")\np.add_argument("--out")\na = p.parse_args()\nPath(a.out, "distances.txt").write_text("wrong\\n")\n')
            result, report, _ = self.run_bench(temp, candidate)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(report["status"], "failed")
            self.assertIn("hash mismatch", report["error"])
            self.assertNotIn("comparison", report)
            self.assertNotIn("candidate", report)
            self.assertFalse(report["samples"][-1]["correct"])

    def test_timeout_is_a_failure_with_saved_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            result, report, _ = self.run_bench(Path(temp), None, "--timeout", "0.001")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(report["status"], "failed")
            self.assertTrue(report["samples"][-1]["timed_out"])
            self.assertNotIn("comparison", report)


if __name__ == "__main__":
    unittest.main()
