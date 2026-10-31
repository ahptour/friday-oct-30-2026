"""Extra differential cases for direct phrase verification; accepts a native binary."""
import argparse
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('reference_b_anchor', ROOT / 'track-b/b_index.py')
REFERENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REFERENCE)
BINARY = None


class AnchorBytesTest(unittest.TestCase):
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
            subprocess.run([str(BINARY), '--data', str(data), '--out', str(base / 'rust')],
                           check=True, capture_output=True)
            self.assertEqual(sorted(p.name for p in (base / 'rust').iterdir()), ['results.txt'])
            self.assertEqual((base / 'rust/results.txt').read_bytes(),
                             (base / 'reference/results.txt').read_bytes())

    def test_every_anchor_position_and_missing_neighbors(self):
        # The first 1,000 documents make each ordinary word common and each
        # anchor absent. Every phrase therefore selects its designated position.
        common = [f'base{i}' for i in range(7)]
        docs = [' '.join(common)] * 1000
        queries = []
        separators = ['—', '/', "'", '   ', '\u00a0', '\u2019', ', ', '.']
        for position in range(8):
            terms = common.copy()
            terms.insert(position, f'anchor{position}')
            phrase = ' '.join(terms)
            queries.extend(['PHRASE "' + phrase + '"'] * 2)
            decorated = ''.join(term.upper() + separators[i] for i, term in enumerate(terms))
            docs.extend([phrase, decorated, 'extra ' + phrase + ' suffix',
                         phrase + ' ' + phrase, ' '.join(terms[:position + 1]),
                         ' '.join(terms[position:]), ' '.join(terms[:position]),
                         ' '.join(terms[position + 1:]), '', '§§¶'])
            for index in range(8):
                wrong = terms.copy()
                wrong[index] += 'x'
                docs.append(' '.join(wrong))
                docs.append(' '.join(wrong) + ' ' + phrase)
            for gap in range(1, 8):
                docs.append(' '.join(terms[:gap] + ['unqueried'] + terms[gap:]))
        self.check(docs, queries)

    def test_borrowed_spelling_maximum_length_and_sparse_ids(self):
        # Phrase-only terms precede Boolean terms, leaving gaps in retained IDs.
        long_word = 'a' * 32
        docs = ['shared left right tail 007 7'] * 1000 + [
            long_word.upper() + ' RARE tail', 'left rare right', 'left rare right tail',
            'shared rare shared', 'shared other rare shared', '007 rare 7',
            '007rare7', 'left rare', 'right', 'rare right', 'left',
            'rare missing right', 'left missing rare right', 'left RARE—right',
        ]
        queries = ['PHRASE "' + long_word + ' rare tail"',
                   'PHRASE "left rare right"', 'PHRASE "shared rare shared"',
                   'PHRASE "007 rare 7"', 'BOOL shared NOT rare',
                   'BOOL right AND rare', 'BOOL absent OR tail NOT shared',
                   'PHRASE "rare right tail"']
        self.check(docs, queries)

    def test_long_parallel_input_and_cross_document_fragments(self):
        docs = ['before center after'] * 1000
        patterns = ['before mark after', 'before MARK—after', 'before mark',
                    'after', 'mark after', 'before', 'before unknown mark after',
                    'before mark unknown after', '', '—§—',
                    'before mark after before mark after']
        docs.extend(patterns * 700)
        self.check(docs, ['PHRASE "before mark after"', 'PHRASE "mark after"',
                          'PHRASE "before mark"', 'BOOL after NOT mark',
                          'PHRASE "after before"', 'PHRASE "before mark after"'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    args = parser.parse_args()
    BINARY = args.binary.resolve()
    unittest.main(argv=['test_b_anchor_bytes.py'], verbosity=2)
