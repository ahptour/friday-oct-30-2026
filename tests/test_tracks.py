"""Run Tracks B/C/D through their real CLIs against the pinned reference."""
import csv
import io
import itertools
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest

from bench import REFERENCE_REV, ROOT, TRACKS


class ReferenceCLI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module, cls.output, _ = TRACKS[cls.track]
        cls.candidate = Path(os.environ.get(f"TRACK_{cls.track.upper()}_CANDIDATE",
                                            ROOT / f"track-{cls.track}" / cls.module)).resolve()
        cls.reference = subprocess.check_output(
            ["git", "-C", str(ROOT), "show", f"{REFERENCE_REV}:track-{cls.track}/{cls.module}"])

    def compare(self, inputs, expected=None):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            data = temp / "data"
            data.mkdir()
            for name, value in inputs.items():
                (data / name).write_bytes(value if isinstance(value, bytes) else value.encode("utf-8"))
            oracle = temp / "reference.py"
            oracle.write_bytes(self.reference)
            outputs = []
            for name, script in (("oracle", oracle), ("candidate", self.candidate)):
                out = temp / name / "nested"
                # The original D CLI requires an existing output directory.
                if name == "oracle":
                    out.mkdir(parents=True)
                result = subprocess.run(
                    [sys.executable, "-B", str(script), "--data", str(data), "--out", str(out)],
                    capture_output=True, timeout=30,
                    env=dict(os.environ, PYTHONHASHSEED="0", PYTHONDONTWRITEBYTECODE="1"))
                self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
                self.assertEqual([path.name for path in out.iterdir()], [self.output])
                outputs.append((out / self.output).read_bytes())
            self.assertEqual(outputs[1], outputs[0])
            if expected is not None:
                self.assertEqual(outputs[1], expected)
            return outputs[1]


class IndexTests(ReferenceCLI):
    track = "b"

    def corpus(self, texts, queries, expected=None):
        docs = "".join(f"{i}\t{text}\n" for i, text in enumerate(texts))
        return self.compare({"docs.txt": docs, "queries.txt": "".join(q + "\n" for q in queries)}, expected)

    def test_empty_corpus_and_queries(self):
        self.corpus([], [], b"")
        self.corpus(["", "§§ ¶", "* * *"], ['BOOL absent', 'PHRASE "absent term"'], b"0 0\n0 0\n")

    def test_ascii_tokenization_and_exact_boundaries(self):
        self.corpus(["Zürich naïve Straße İSTANBUL 273K ﬁduciary 𝔄ct", "BAIL—BOND habeas-corpus U.S.C.",
                     "007 7 and or not", "ab bail bonds", "bail", "bond", "bail unknown bond"],
                    ['BOOL zurich OR cafe OR strasse OR 273k', 'BOOL rich AND z', 'BOOL 273',
                     'BOOL 007 NOT 7', 'BOOL and AND not', 'PHRASE "bail bond"',
                     'PHRASE "habeas corpus"', 'PHRASE "u s c"', 'PHRASE "bail bonds"'])

    def test_exhaustive_six_term_left_to_right_operators(self):
        texts = [" ".join(term for bit, term in enumerate("abcdef") if mask & (1 << bit))
                 for mask in range(64)]
        queries = []
        for ops in itertools.product(("AND", "OR", "NOT"), repeat=5):
            queries.append("BOOL a " + " ".join(f"{op} {term}" for op, term in zip(ops, "bcdef")))
        queries.extend(('BOOL absent OR a AND b', 'BOOL a NOT a OR c', 'BOOL a OR b NOT a NOT b'))
        self.corpus(texts, queries)

    def test_phrases_every_anchor_position_repetition_and_unknown_tokens(self):
        texts, queries = [], []
        for size in range(2, 9):
            for anchor in range(size):
                terms = ["common"] * size
                terms[anchor] = f"anchor{size}{anchor}"
                phrase = " ".join(terms)
                texts.extend((phrase, phrase + " " + phrase, " unknown ".join(terms)))
                queries.extend((f'PHRASE "{phrase}"', f'PHRASE "{" ".join(reversed(terms))}"'))
        texts.extend(("x x", "x x x", "x y x", "x", "x x x x"))
        queries.extend(('PHRASE "x x"', 'PHRASE "x x x"', 'PHRASE "x x x x"', 'PHRASE "x x x x x"'))
        self.corpus(texts, queries * 2)

    def test_long_tokens_and_more_than_ten_results(self):
        first, second = "x" * 31 + "a", "x" * 31 + "b"
        texts = [f"common {first if i % 2 else second}" for i in range(103)]
        self.corpus(texts, ['BOOL common', f'BOOL {first}', f'BOOL {first} NOT {second}',
                            f'PHRASE "common {second}"', f'BOOL {"x" * 31}'])

    def test_randomized_corpora_and_repeated_processes(self):
        for seed in (1, 42, 991):
            rng = random.Random(seed)
            vocabulary = ["and", "or", "not", "007", "7", "a", "b", "c", "d", "absent"]
            texts = [rng.choice((" ", "—", ",", "'s ")).join(rng.choices(vocabulary[:-1], k=rng.randrange(25)))
                     for _ in range(120)]
            queries = []
            for _ in range(200):
                terms = rng.choices(vocabulary, k=rng.randrange(2, 7))
                queries.append('PHRASE "' + " ".join(terms) + '"')
                queries.append("BOOL " + terms[0] + " " + " ".join(
                    rng.choice(("AND", "OR", "NOT")) + " " + term for term in terms[1:]))
            with self.subTest(seed=seed):
                self.corpus(texts, queries)


def csv_record(fields, ending="\n"):
    stream = io.StringIO(newline="")
    csv.writer(stream, lineterminator=ending).writerow(fields)
    return stream.getvalue()


class JoinTests(ReferenceCLI):
    track = "c"
    regions = "region_id,country\n3,BBB\n7,AAA\n12,AAA\n20,CCC\n21,DDD\n"

    def records(self, body, expected=None):
        return self.compare({"records.csv": body, "regions.csv": self.regions}, expected)

    def test_header_only(self):
        self.records("\ufeffid,name,note,region_id,amount\r\n", b"")

    def test_worked_example(self):
        self.records('\ufeffid,name,note,region_id,amount\r\n'
                     '1,"Kaphar, Titus","said ""no"" twice",7,1420.00\r\n'
                     '2,"Betts","a note that\nspans two lines",7,88.50\n'
                     '3,,"",12,0.00\r\n'
                     '4,"trailing space ","comma, quote "" and newline\ntogether",3,-5.25\n'
                     '\ufeff5,"Betts ","one\ntwo\nthree",12,-0.75\r\n'
                     '6,"",,12,10.00\r\n'
                     '\ufeff7,"Ann ""Annie"" Lee","x",5,2.00\n'
                     '8,"Nobody","",4000000,-1.00\r\n'
                     '9,"Neg","n",20,-0.05\r\n',
                     b'AAA 5 17 1517.75 -0.75 1420.00 4 24 55\n'
                     b'BBB 1 4 -5.25 -5.25 -5.25 1 15 35\n'
                     b'CCC 1 9 -0.05 -0.05 -0.05 1 3 1\n'
                     b'_UNMATCHED 2 15 1.00 -1.00 2.00 2 21 1\n')

    def test_empty_quoted_names_trailing_spaces_and_exact_identity(self):
        body = 'id,name,note,region_id,amount\n1,,,7,0.00\n2,"","",12,0.00\n'
        for i, name in enumerate(("Betts", "Betts ", "Betts  ", 'Ann "Annie" Lee', "Betts", "betts"), 3):
            body += csv_record((i, name, '"quoted", note\nnext', 7, "-0.01"), "\r\n")
        self.records(body)

    def test_bom_data_multiline_quotes_and_unterminated_last_record(self):
        body = "\ufeffid,name,note,region_id,amount\r\n"
        for i, note in enumerate(('"', '"first', 'last"', '"a",\nb\nc\nd', "\ufeffinside", "x\ufeffy"), 1):
            body += ("\ufeff" if i > 1 else "") + csv_record((i, "Name\ufeff ", note, 7, "-0.05"),
                                                                              "\n" if i % 2 else "\r\n")
        self.records(body.rstrip("\r\n"))

    def test_large_integer_sums_and_negative_money(self):
        body = "id,name,note,region_id,amount\n"
        for i in range(1000):
            body += csv_record((40000000 + i, "same", "", 20, "-999.99"))
            body += csv_record((50000000 + i, "same ", "", 9999999, "99999.99"))
        self.records(body)

    def test_randomized_hostile_csv(self):
        for seed in (1, 42, 991):
            rng = random.Random(seed)
            body = "\ufeffid,name,note,region_id,amount\r\n"
            for i in range(1, 501):
                name = rng.choice(("", "Name", "Name ", "Name  ", 'A "quoted", name', "é\ufeff"))
                note = rng.choice(("", '"', "one\ntwo\nthree\nfour", 'comma, quote "\nnext', "a\ufeffb"))
                cents = rng.choice((-99999, -99, -1, 0, 1, 9999999))
                amount = f"{'-' if cents < 0 else ''}{abs(cents) // 100}.{abs(cents) % 100:02d}"
                body += ("\ufeff" if i % 19 == 0 else "") + csv_record(
                    (i, name, note, rng.choice((3, 7, 12, 20, 5, 9999999)), amount), rng.choice(("\n", "\r\n")))
            with self.subTest(seed=seed):
                self.records(body)


class TraverseTests(ReferenceCLI):
    track = "d"

    def graph(self, nodes, edges, queries, expected=None):
        return self.compare({"graph.txt": f"{nodes} {len(edges)}\n" + "".join(f"{u} {v} {w}\n" for u, v, w in edges),
                             "queries.txt": "".join(f"{s} {t}\n" for s, t in queries)}, expected)

    def test_empty_queries_and_edgeless_graph(self):
        self.graph(1, [], [], b"")
        self.graph(3, [], [(0, 0), (0, 1), (2, 2)], b"0\n-1\n0\n")

    def test_worked_example(self):
        edges = [(0, 1, 4), (1, 2, 3), (0, 2, 9), (2, 0, 5), (2, 1, 6), (1, 3, 0),
                 (3, 4, 0), (4, 4, 7), (4, 5, 4294967295), (2, 1, 2), (5, 6, 4294967295), (0, 1, 4)]
        queries = [(0, 2), (2, 1), (1, 4), (4, 4), (7, 7), (0, 6), (4, 5), (6, 0), (0, 7), (1, 0), (2, 1)]
        self.graph(8, edges, queries, b"7\n2\n0\n0\n0\n8589934594\n4294967295\n-1\n-1\n8\n2\n")

    def test_first_meeting_is_not_shortest_and_wide_to_small_improvements(self):
        edges = [(0, 1, 100), (1, 5, 0), (0, 2, 1), (2, 3, 1), (3, 5, 1),
                 (0, 4, 4294967295), (3, 4, 0), (4, 5, 0), (5, 6, 4294967295)]
        self.graph(8, edges, list(itertools.product(range(8), repeat=2)))

    def test_zero_cycles_parallel_edges_and_disconnected_components(self):
        edges = [(0, 1, 10), (0, 1, 0), (1, 0, 0), (1, 2, 0), (2, 2, 0),
                 (3, 4, 4294967295), (3, 4, 9), (3, 4, 9), (4, 5, 4294967295)]
        self.graph(7, edges, list(itertools.product(range(7), repeat=2)) * 2)

    def test_randomized_directed_graphs_all_pairs(self):
        for seed in range(12):
            rng = random.Random(seed)
            nodes = rng.randrange(8, 35)
            edges = [(rng.randrange(nodes), rng.randrange(nodes), rng.choice((0, 1, 2, 19, 4294967295)))
                     for _ in range(nodes * 4)]
            edges.extend(edges[:5])
            rng.shuffle(edges)
            queries = list(itertools.product(range(nodes), repeat=2))
            rng.shuffle(queries)
            with self.subTest(seed=seed):
                self.graph(nodes, edges, queries)

    def test_long_path_and_many_frontier_improvements(self):
        edges = [(i, i + 1, 4294967295) for i in range(100)]
        edges.extend((i, i + 1, i % 3) for i in range(50))
        edges.extend((0, i, 4294967295) for i in range(1, 50))
        self.graph(105, list(reversed(edges)), [(0, i) for i in range(105)] + [(104, 0), (0, 100)])


if __name__ == "__main__":
    unittest.main()
