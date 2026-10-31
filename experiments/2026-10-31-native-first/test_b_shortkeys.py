"""Exact short-key boundary differential cases; accepts a native binary."""
import argparse
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('reference_b_shortkeys', ROOT / 'track-b/b_index.py')
REFERENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REFERENCE)
BINARY = None


class ShortKeysTest(unittest.TestCase):
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


    def test_key_widths_equal_prefixes_and_all_phrase_positions(self):
        lengths = [1, 7, 8, 9, 15, 16, 17, 23, 24, 31, 32]
        words = ['a' * length for length in lengths]
        words += ['a' * 16 + 'b', 'a' * 16 + 'c', 'a' * 31 + 'b']
        docs = words + [word.upper() for word in words] + [' '.join(words), '', '§§¶']
        queries = ['BOOL ' + word for word in words]
        for i, word in enumerate(words):
            other = words[(i + 1) % len(words)]
            queries += ['BOOL ' + word + ' NOT ' + other,
                        'BOOL ' + word + ' OR ' + other,
                        'BOOL ' + word + ' AND ' + other]
        for position in range(8):
            terms = [words[(position + i) % len(words)] for i in range(8)]
            terms[position] = 'abcdefghijklmnopz'
            docs += [' '.join(terms), '—'.join(word.upper() for word in terms),
                     ' '.join(terms[:position]), ' '.join(terms[position:])]
            queries += ['PHRASE "' + ' '.join(terms) + '"'] * 2
        self.check(docs, queries)

    def test_only_short_only_long_and_parallel_mixed_dictionaries(self):
        for words in [['a', '12345678'], ['a'*17, 'a'*31+'b'],
                      ['a'*16, 'a'*17, 'a'*16+'b', 'a'*32]]:
            with self.subTest(words=words):
                patterns = [' '.join(words), '—'.join(w.upper() for w in words),
                            ' '.join(words[::-1]), '', words[0], words[-1]]
                docs = patterns * 1600
                queries = ['BOOL ' + w for w in words]
                queries += ['PHRASE "' + ' '.join(words) + '"',
                            'PHRASE "' + ' '.join(words[::-1]) + '"',
                            'BOOL ' + words[0] + ' NOT ' + words[-1]]
                self.check(docs, queries)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    args = parser.parse_args()
    BINARY = args.binary.resolve()
    unittest.main(argv=['test_b_shortkeys.py'], verbosity=2)
