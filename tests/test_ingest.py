"""Exercise the actual Track A CLI against the independent pinned reference."""
from datetime import datetime, timedelta
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest

from bench import REFERENCE_REV, ROOT


CANDIDATE = Path(os.environ.get("TRACK_A_CANDIDATE", ROOT / "track-a/a_ingest.py")).resolve()
METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")
START = datetime(2026, 10, 30, 18)


def request(hour=0, endpoint="/", status=200, latency=0, method="GET", tail="00:00.000000"):
    stamp = (START + timedelta(hours=hour)).strftime("%Y-%m-%dT%H:") + tail + "Z"
    return f"{stamp} {method} {endpoint} {status} {latency}\n"


class IngestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.reference = subprocess.check_output(
            ["git", "-C", str(ROOT), "show", f"{REFERENCE_REV}:track-a/a_ingest.py"]
        )

    def compare(self, text, expected=None):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            data = temp / "data"
            data.mkdir()
            (data / "requests.log").write_bytes(text.encode("ascii"))
            oracle = temp / "reference.py"
            oracle.write_bytes(self.reference)
            outputs = []
            for name, script in (("oracle", oracle), ("candidate", CANDIDATE)):
                out = temp / name / "nested"
                result = subprocess.run(
                    [sys.executable, "-B", str(script), "--data", str(data), "--out", str(out)],
                    capture_output=True, timeout=30,
                    env=dict(os.environ, PYTHONHASHSEED="0", PYTHONDONTWRITEBYTECODE="1"),
                )
                self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
                self.assertEqual([path.name for path in out.iterdir()], ["summary.txt"])
                outputs.append((out / "summary.txt").read_bytes())
            self.assertEqual(outputs[1], outputs[0])
            if expected is not None:
                self.assertEqual(outputs[1], expected)
            return outputs[1]

    def test_empty_input(self):
        self.compare("", b"")

    def test_duplicate_requests_and_integer_error_rate(self):
        text = request(latency=7, status=499) * 2 + request(latency=4294967295, status=500)
        self.compare(text, b"2026-10-30T18:00:00Z / 3 7 4294967295 4294967295 1 333333\n")

    def test_nearest_rank_boundaries_ties_and_uint32_values(self):
        rng = random.Random(41)
        lines = []
        for count in (1, 2, 3, 4, 19, 20, 21, 49, 50, 51, 99, 100, 101, 199, 200, 201, 999):
            for tied in (False, True):
                for i in range(count):
                    value = rng.choice((0, 0, 17, 17, 4294967295)) if tied else i * 4294967295 // max(count - 1, 1)
                    lines.append(request(endpoint=f"/ranks/{count}/{int(tied)}", latency=value))
        rng.shuffle(lines)
        self.compare("".join(lines))

    def test_every_status_and_method(self):
        lines = [request(endpoint=f"/method/{method}", method=method, status=status, latency=status)
                 for method in METHODS for status in range(100, 600)]
        output = self.compare("".join(reversed(lines)))
        self.assertEqual(len(output.splitlines()), 7)
        self.assertTrue(all(line.endswith(b" 100 200000") for line in output.splitlines()))

    def test_all_hours_boundaries_and_arbitrary_disorder(self):
        lines = []
        for hour in range(72):
            lines.extend((request(hour, latency=hour), request(hour, latency=hour + 1, tail="59:59.999999")))
        self.compare("".join(reversed(lines)))

    def test_endpoint_byte_order_and_exact_identity(self):
        endpoints = ["/", "/A", "/a", "/a/", "/a//", "/v2/orders", "/v2/orders-export",
                     "/v2/orders.csv", "/v2/orders/{id}", "/v2/orders/{id}/items",
                     "/v2/ordersByDate", "/v2/orders_legacy", "/v2/orderstatus", "/rank100", "/rank20",
                     "/" + "x" * 123 + "A", "/" + "x" * 123 + "a", "/a_0-1.2/{id}:download"]
        output = self.compare("".join(request(endpoint=name) for name in reversed(endpoints)))
        self.assertEqual([line.split(b" ")[1].decode("ascii") for line in output.splitlines()], sorted(endpoints))

    def test_randomized_grouping_and_repeated_processes(self):
        for seed in (1, 42, 991):
            with self.subTest(seed=seed):
                rng = random.Random(seed)
                lines = [request(rng.randrange(72), f"/random/{rng.randrange(32)}",
                                 rng.randrange(100, 600), rng.randrange(1 << 32), rng.choice(METHODS))
                         for _ in range(5000)]
                lines.extend(lines[:100])
                rng.shuffle(lines)
                self.compare("".join(lines))


if __name__ == "__main__":
    unittest.main()
