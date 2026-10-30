"""Differential checks for the Rust search engine against the published reference."""
import importlib.util
from itertools import product
from pathlib import Path
from random import Random
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TRACK = ROOT / 'track-b'
SPEC = importlib.util.spec_from_file_location('reference_b', TRACK / 'b_index.py')
REFERENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REFERENCE)


class TrackBTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '--manifest-path', str(TRACK / 'Cargo.toml'),
                        '--release', '--offline', '--locked'], check=True)

    def check(self, docs, queries):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            data = base / 'data'
            data.mkdir()
            (data / 'docs.txt').write_bytes(b''.join(
                str(i).encode() + b'\t' + text.encode('utf-8') + b'\n'
                for i, text in enumerate(docs)))
            (data / 'queries.txt').write_text(''.join(q + '\n' for q in queries), encoding='ascii')
            REFERENCE.main(str(data), str(base / 'reference'))
            subprocess.run([str(TRACK / 'target/release/solution'), '--data', str(data),
                            '--out', str(base / 'rust')], check=True, capture_output=True)
            self.assertEqual(sorted(p.name for p in (base / 'rust').iterdir()), ['results.txt'])
            self.assertEqual((base / 'rust/results.txt').read_bytes(),
                             (base / 'reference/results.txt').read_bytes())

    def test_worked_example_and_left_associativity(self):
        self.check([
            'The surety posted bail; the BAIL BOND was forfeited.',
            'Writ of Habeas-Corpus: see 42 U.S.C. § 1983 (bail bond).', '',
            'Zürich bail—bond, bail bail bond. Case 007.',
            'In the matter of the estate. Bond not posted, No. 7.',
        ], ['BOOL bail AND bond', 'BOOL bail', 'BOOL estate OR surety AND bail',
            'BOOL bond NOT bail', 'BOOL bail NOT bail', 'BOOL zurich', 'BOOL rich AND z',
            'BOOL not AND bond', 'BOOL 7', 'BOOL the OR writ OR case',
            'PHRASE "bail bond"', 'PHRASE "bail bail bond"', 'PHRASE "bond bail"',
            'PHRASE "habeas corpus"', 'PHRASE "u s c 1983"', 'PHRASE "estate bond"',
            'PHRASE "forfeited writ"', 'PHRASE "in the matter of"'])

    def test_ascii_only_tokenization_and_maximum_token(self):
        self.check(['Zürich naïve Straße İSTANBUL 273K ﬁduciary café',
                    '007 7', 'A' * 32 + ' B' + 'C' * 31, 'bail\u00a0bond', '§§ ¶ * * *'],
                   ['BOOL zurich OR cafe OR strasse OR 273k', 'BOOL z AND rich',
                    'PHRASE "na ve"', 'PHRASE "273 duciary caf"', 'PHRASE "007 7"',
                    'PHRASE "' + 'a' * 32 + ' b' + 'c' * 31 + '"', 'PHRASE "bail bond"'])

    def test_phrase_failure_links_repeats_duplicates_and_document_boundaries(self):
        docs = ['a b a b a a a', 'a a', 'a', 'b', 'a missing b', 'a, b', '', 'b a a a a']
        queries = ['PHRASE "a b"', 'PHRASE "b a"', 'PHRASE "a a"', 'PHRASE "a a a"',
                   'PHRASE "a a a a"', 'PHRASE "b a a"', 'PHRASE "b a a a"',
                   'PHRASE "a b a b a"', 'PHRASE "a b"', 'BOOL a NOT a OR b AND a']
        self.check(docs, queries)

    def test_all_operator_orders_and_sampled_rare_terms(self):
        words = ['alpha', 'beta', 'gamma', 'delta', 'epsilon', 'zeta']
        docs = [' '.join(w for i, w in enumerate(words) if mask & (1 << i))
                for mask in range(64)]
        queries = []
        for ops in product(['AND', 'OR', 'NOT'], repeat=5):
            for terms in [words, ['alpha', 'beta', 'alpha', 'beta', 'gamma', 'alpha']]:
                query = 'BOOL ' + terms[0]
                for op, term in zip(ops, terms[1:]):
                    query += ' ' + op + ' ' + term
                queries.append(query)
        self.check(docs, queries)
        self.check(['unrelated'] * 1000 + docs, queries)

    def test_no_documents_or_queries(self):
        self.check([], ['BOOL absent', 'PHRASE "a b"'])
        self.check(['a b', ''], [])

    def test_query_categories_and_interleaved_output_order(self):
        docs = ['a b a b' if i % 3 == 0 else 'a a' if i % 3 == 1 else 'b'
                for i in range(195)]
        boolean = ['BOOL a NOT a', 'BOOL a OR b', 'BOOL b NOT a',
                   'BOOL absent', 'BOOL a OR b OR a']
        phrases = ['PHRASE "a b"', 'PHRASE "a a"', 'PHRASE "b a"',
                   'PHRASE "a b"', 'PHRASE "absent a"']
        self.check(docs, boolean)
        self.check(docs, phrases)
        mixed = [query for pair in zip(phrases, boolean) for query in pair]
        self.check(docs, mixed)
        self.check(docs, mixed[::-1])

    def test_maximum_query_widths_and_repeated_phrase_hits(self):
        patterns = ['a b c d e f g h a b c d e f g h',
                    'a a a a a a a a a', 'h g f e d c b a',
                    'a b c d unqueried e f g h', 'a b c d e f g', '']
        docs = [patterns[i % len(patterns)] for i in range(195)]
        queries = ['PHRASE "a b c d e f g h"',
                   'BOOL a OR b NOT c AND d OR e NOT f',
                   'PHRASE "a a a a a a a a"',
                   'BOOL a AND b AND c AND d AND e AND f',
                   'PHRASE "h g f e d c b a"',
                   'BOOL a NOT a OR b NOT b OR c NOT c',
                   'PHRASE "a b c d e f g h"', 'BOOL a']
        self.check(docs, queries)

    def test_randomized_sparse_dense_sets_and_parallel_chunks(self):
        for seed in [2, 19]:
            with self.subTest(seed=seed):
                rng = Random(seed)
                words = ['the', 'and', 'or', 'not'] + [f'word{i}' for i in range(100)]
                docs = [' '.join(rng.choices(words, k=rng.randrange(1, 100))) for _ in range(2500)]
                docs += ['rare1 rare2', 'rare1', '', 'rare2 the']
                queries = []
                for _ in range(250):
                    terms = rng.choices(words + ['absent', 'rare1', 'rare2'], k=rng.randrange(1, 7))
                    query = 'BOOL ' + terms[0]
                    for term in terms[1:]:
                        query += ' ' + rng.choice(['AND', 'OR', 'NOT']) + ' ' + term
                    queries.append(query)
                for _ in range(250):
                    tokens = rng.choice(docs[:2500]).split()
                    start = rng.randrange(len(tokens))
                    phrase = tokens[start:start + rng.randrange(2, 9)]
                    if len(phrase) >= 2:
                        queries.append('PHRASE "' + ' '.join(phrase) + '"')
                self.check(docs, queries)


if __name__ == '__main__':
    unittest.main()
