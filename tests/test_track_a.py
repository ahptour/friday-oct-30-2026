"""Compare the release Rust executable with the unchanged Python reference."""

import importlib.util
from datetime import datetime, timedelta
from pathlib import Path
from random import Random
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TRACK = ROOT / "track-a"
SPEC = importlib.util.spec_from_file_location("reference", TRACK / "a_ingest.py")
REFERENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REFERENCE)
START = datetime(2026, 10, 30, 18)
LONGEST = (
    "/internal/v1/tenants/{tenant_id}/projects/{project_id}"
    "/pipelines/{pipeline_id}/runs/{run_id}/artifacts/{artifact_id}:download"
)


def row(hour, endpoint, status=200, latency=0, method="GET"):
    timestamp = (START + timedelta(hours=hour)).strftime("%Y-%m-%dT%H:00:00.000000Z")
    return f"{timestamp} {method} {endpoint} {status} {latency}"


class TrackATest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(
            ["cargo", "build", "--manifest-path", str(TRACK / "Cargo.toml"),
             "--release", "--offline", "--locked"],
            check=True,
        )

    def check_rows(self, rows, expected=None):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            data = base / "data"
            data.mkdir()
            (data / "requests.log").write_text(
                "\n".join(rows) + ("\n" if rows else ""), encoding="ascii"
            )
            reference_out = base / "reference"
            rust_out = base / "rust"
            REFERENCE.main(str(data), str(reference_out))
            subprocess.run(
                [str(TRACK / "target/release/solution"), "--data", str(data),
                 "--out", str(rust_out)],
                check=True,
                capture_output=True,
            )
            self.assertEqual(sorted(p.name for p in rust_out.iterdir()), ["summary.txt"])
            actual = (rust_out / "summary.txt").read_bytes()
            self.assertEqual(actual, (reference_out / "summary.txt").read_bytes())
            if expected is not None:
                self.assertEqual(actual, expected.encode("ascii"))

    def test_empty_input(self):
        self.check_rows([], "")

    def test_spec_worked_example(self):
        rows = """2026-10-30T19:00:00.000000Z GET /v2/orders/{id} 200 1500
2026-10-30T18:59:59.999999Z GET /v2/orders/{id} 200 1841
2026-10-30T18:00:00.000000Z POST /v2/checkout 503 204117
2026-10-30T18:00:00.000000Z POST /v2/checkout 201 30000
2026-10-30T18:00:00.115332Z GET /health 200 62
2026-10-30T18:00:00.115332Z GET /health 200 62
2026-10-30T18:42:17.000001Z HEAD /health 200 4294967295
2026-10-30T18:30:00.000001Z OPTIONS / 200 0
2026-10-30T18:12:00.500000Z POST /v2/checkout 500 0
2026-10-30T18:12:00.600000Z POST /v2/checkout 499 1500
2026-10-30T18:12:00.700000Z POST /v2/checkout 599 2500
2026-10-30T18:20:00.000000Z GET /v2/orders/{id} 500 900
2026-10-30T18:21:00.000000Z DELETE /v2/orders/{id} 404 77
2026-10-30T19:05:00.000000Z GET /v2/orders_legacy 200 60
2026-10-30T19:05:00.000001Z POST /v2/orders-export 202 20
2026-10-30T19:05:00.000002Z GET /v2/orders 200 10
2026-10-31T23:59:59.999999Z POST /v2/checkout 502 7
2026-11-01T00:00:00.000000Z POST /v2/checkout 503 8""".splitlines()
        expected = """2026-10-30T18:00:00Z / 1 0 0 0 0 0
2026-10-30T18:00:00Z /health 3 62 4294967295 4294967295 0 0
2026-10-30T18:00:00Z /v2/checkout 5 2500 204117 204117 3 600000
2026-10-30T18:00:00Z /v2/orders/{id} 3 900 1841 1841 1 333333
2026-10-30T19:00:00Z /v2/orders 1 10 10 10 0 0
2026-10-30T19:00:00Z /v2/orders-export 1 20 20 20 0 0
2026-10-30T19:00:00Z /v2/orders/{id} 1 1500 1500 1500 0 0
2026-10-30T19:00:00Z /v2/orders_legacy 1 60 60 60 0 0
2026-10-31T23:00:00Z /v2/checkout 1 7 7 7 1 1000000
2026-11-01T00:00:00Z /v2/checkout 1 8 8 8 1 1000000
"""
        self.check_rows(rows, expected)

    def test_nearest_rank_with_duplicates_and_uint32_extremes(self):
        rng = Random(31)
        rows = []
        for count in [*range(1, 202), 1000, 1001, 10001]:
            for _ in range(count):
                latency = rng.choice([0, 4294967295, 7, rng.randrange(2**32)])
                rows.append(row(0, f"/n/{count}", latency=latency))
        rows.extend(row(1, "/equal", latency=123) for _ in range(301))
        rng.shuffle(rows)
        self.check_rows(rows)

    def test_every_status_and_integer_error_rate(self):
        rows = [row(0, "/statuses", status) for status in range(100, 600)]
        rows += [row(0, "/fraction", status) for status in [200, 499, 500]]
        rows += [row(0, "/extremes", 200, 0), row(0, "/extremes", 599, 4294967295)]
        self.check_rows(rows, """2026-10-30T18:00:00Z /extremes 2 0 4294967295 4294967295 1 500000
2026-10-30T18:00:00Z /fraction 3 0 0 0 1 333333
2026-10-30T18:00:00Z /statuses 500 0 0 0 100 200000
""")

    def test_all_hour_boundaries_in_reverse_and_endpoint_byte_order(self):
        endpoints = ["/", LONGEST, "/A", "/a", "/v2/orders", "/v2/orders-export",
                     "/v2/orders.csv", "/v2/orders/{id}", "/v2/orders/{id}/items",
                     "/v2/ordersByDate", "/v2/orders_legacy", "/v2/orderstatus",
                     "/v2/rank100", "/v2/rank20"]
        rows = []
        for hour in reversed(range(72)):
            for endpoint in reversed(endpoints):
                line = row(hour, endpoint, latency=hour)
                rows += [line, line.replace(":00:00.000000Z", ":59:59.999999Z")]
        self.check_rows(rows)

    def test_randomized_disorder_methods_and_duplicate_requests(self):
        methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
        for seed in [0, 2, 42]:
            with self.subTest(seed=seed):
                rng = Random(seed)
                rows = [
                    row(rng.randrange(72), rng.choice(["/", LONGEST, f"/item/{rng.randrange(30)}"]),
                        rng.randrange(100, 600), rng.randrange(2**32), rng.choice(methods))
                    for _ in range(10_000)
                ]
                rows += rows[::7]
                rng.shuffle(rows)
                self.check_rows(rows)

    def test_parallel_chunk_edges_and_fewer_records_than_workers(self):
        # Equal-size lines put some byte partitions exactly on a line start;
        # unequal lengths put them inside timestamps, endpoints and numbers.
        for count in [*range(1, 25), 71, 72, 73, 143, 144, 145,
                      1023, 1024, 1025, 4095, 4096, 4097, 8191, 8192, 8193]:
            with self.subTest(count=count, layout="fixed"):
                rows = [row(i % 72, f"/item/{i:04}", 500, 4_000_000_000 + i)
                        for i in range(count)]
                self.check_rows(rows)
            with self.subTest(count=count, layout="variable"):
                rows = [row(i % 72, "/" + "a" * (i % 125),
                            499 if i % 2 else 599, i if i % 3 else 4294967295,
                            "GET" if i % 2 else "OPTIONS")
                        for i in range(count)]
                self.check_rows(rows)

    def test_parallel_merge_of_groups_across_the_entire_file(self):
        rng = Random(2026)
        endpoints = ["/", LONGEST] + [f"/item/{i}/" + "x" * i for i in range(31)]
        groups = [(hour, endpoint) for hour in range(72) for endpoint in endpoints]
        rows = []
        # Every block spans all hours and endpoints. Different blocks have
        # different latency distributions, so local percentiles cannot be merged.
        for block in range(16):
            rng.shuffle(groups)
            for hour, endpoint in groups:
                latency = [0, 4294967295, block * 1000 + hour][block % 3]
                rows.append(row(hour, endpoint, [200, 499, 500, 599][block % 4], latency))
        self.check_rows(rows)

    def test_revisited_hour_runs_with_missing_hours_and_skewed_sizes(self):
        rows = []
        # Long runs cross byte partitions. Revisited hours must keep every
        # separated run, and empty hours must not block ordered output.
        runs = [(71, 1), (0, 4096), (35, 3), (71, 2048),
                (0, 2), (35, 1024), (1, 1), (71, 4097)]
        for run, (hour, count) in enumerate(runs):
            for i in range(count):
                endpoint = ["/", LONGEST, "/revisited"][i % 3]
                latency = [0, 4294967295, run * 1000 + i][run % 3]
                rows.append(row(hour, endpoint, [200, 499, 500, 599][run % 4],
                                latency, "GET" if i % 2 else "OPTIONS"))
        self.check_rows(rows)

    def test_hash_collisions_preserve_exact_endpoint_identity(self):
        # These 100-byte endpoints share the first, middle and last eight bytes
        # sampled by FastHasher. HashMap must still compare the entire key.
        endpoints = ["/" + "x" * 19 + a + "x" * 54 + b + "x" * 24
                     for a, b in [("a", "a"), ("A", "a"), ("_", "a"),
                                  ("a", "A"), ("a", "_"), ("a", "{")]]
        rows = []
        for block in range(5):
            for hour in range(72):
                for index, endpoint in enumerate(endpoints):
                    latency = [0, 4294967295, index * 1000 + hour][block % 3]
                    status = [100, 200, 499, 500, 599][(block + index) % 5]
                    rows.append(row(hour, endpoint, status, latency))
        Random(50).shuffle(rows)
        self.check_rows(rows)


if __name__ == "__main__":
    unittest.main()
