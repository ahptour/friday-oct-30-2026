"""Compare Track D's Rust implementation with the unchanged Python reference."""
import importlib.util
from pathlib import Path
from random import Random
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TRACK = ROOT / 'track-d'
SPEC = importlib.util.spec_from_file_location('d_reference', TRACK / 'd_traverse.py')
REFERENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REFERENCE)


class TrackDTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '--manifest-path', str(TRACK / 'Cargo.toml'),
                        '--release', '--offline', '--locked'], check=True)

    def check_graph(self, n, edges, queries, expected=None):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            data, rust, reference = base / 'data', base / 'rust', base / 'reference'
            data.mkdir()
            reference.mkdir()
            (data / 'graph.txt').write_text(f'{n} {len(edges)}\n' + ''.join(
                f'{u} {v} {w}\n' for u, v, w in edges), encoding='ascii')
            (data / 'queries.txt').write_text(''.join(
                f'{s} {t}\n' for s, t in queries), encoding='ascii')
            REFERENCE.main(str(data), str(reference))
            subprocess.run([str(TRACK / 'target/release/solution'), '--data', str(data),
                            '--out', str(rust)], check=True, capture_output=True)
            self.assertEqual([p.name for p in rust.iterdir()], ['distances.txt'])
            result = (rust / 'distances.txt').read_bytes()
            self.assertEqual(result, (reference / 'distances.txt').read_bytes())
            if expected is not None:
                self.assertEqual(result, ''.join(f'{x}\n' for x in expected).encode())

    def test_worked_example(self):
        edges = [(0, 1, 4), (1, 2, 3), (0, 2, 9), (2, 0, 5), (2, 1, 6),
                 (1, 3, 0), (3, 4, 0), (4, 4, 7), (4, 5, 4294967295),
                 (2, 1, 2), (5, 6, 4294967295), (0, 1, 4)]
        queries = [(0, 2), (2, 1), (1, 4), (4, 4), (7, 7), (0, 6), (4, 5),
                   (6, 0), (0, 7), (1, 0), (2, 1)]
        self.check_graph(8, edges, queries,
                         [7, 2, 0, 0, 0, 8589934594, 4294967295, -1, -1, 8, 2])

    def test_no_edges_and_no_queries(self):
        self.check_graph(1, [], [], [])
        self.check_graph(3, [], [(0, 0), (1, 2), (2, 1)], [0, -1, -1])

    def test_long_path_large_weights_and_direction(self):
        edges = [(i, i + 1, 4294967295) for i in range(200)]
        edges += [(i, i, 0) for i in range(201)]
        queries = [(0, 200), (200, 0), (199, 200), (200, 200)] * 5
        self.check_graph(201, edges, queries)

    def test_zero_cycles_parallel_edges_and_disconnected_components(self):
        edges = [(0, 1, 0), (1, 2, 0), (2, 0, 0), (2, 3, 900), (2, 3, 4),
                 (2, 3, 4), (3, 4, 3), (4, 5, 2), (5, 4, 1), (7, 8, 0)]
        self.check_graph(10, edges, [(s, t) for s in range(10) for t in range(10)])

    def test_random_graphs_all_pairs_and_repeated_queries(self):
        for seed in range(8):
            with self.subTest(seed=seed):
                rng = Random(seed)
                n = 31 + seed
                edges = [(rng.randrange(n), rng.randrange(n),
                          rng.choice([0, 1, 4294967295, rng.randrange(10000)]))
                         for _ in range(7 * n)]
                edges += edges[::11]
                rng.shuffle(edges)
                queries = [(s, t) for s in range(n) for t in range(n)]
                rng.shuffle(queries)
                queries += queries[::7]
                self.check_graph(n, edges, queries)


if __name__ == '__main__':
    unittest.main()
