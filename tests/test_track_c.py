"""Differential CSV/join checks against the unchanged Track C reference."""
import importlib.util
from pathlib import Path
from random import Random
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TRACK = ROOT / 'track-c'
SPEC = importlib.util.spec_from_file_location('c_reference', TRACK / 'c_join.py')
REFERENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REFERENCE)
BOM = b'\xef\xbb\xbf'
HEADER = BOM + b'id,name,note,region_id,amount\r\n'
REGIONS = b'region_id,country\n3,BBB\n7,AAA\n12,AAA\n20,CCC\n21,DDD\n'


def record(identifier, name, note, region=7, amount='0.00', newline='\r\n'):
    def text(value):
        return '"' + value.replace('"', '""') + '"'
    return f'{identifier},{text(name)},{text(note)},{region},{amount}{newline}'.encode()


class TrackCTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '--manifest-path', str(TRACK / 'Cargo.toml'),
                        '--release', '--offline', '--locked'], check=True)

    def check_bytes(self, records, regions=REGIONS, expected=None):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            data = base / 'data'
            data.mkdir()
            (data / 'records.csv').write_bytes(records)
            (data / 'regions.csv').write_bytes(regions)
            REFERENCE.main(str(data), str(base / 'reference'))
            subprocess.run([str(TRACK / 'target/release/solution'), '--data', str(data),
                            '--out', str(base / 'rust')], check=True, capture_output=True)
            self.assertEqual([p.name for p in (base / 'rust').iterdir()], ['aggregates.txt'])
            actual = (base / 'rust/aggregates.txt').read_bytes()
            self.assertEqual(actual, (base / 'reference/aggregates.txt').read_bytes())
            if expected is not None:
                self.assertEqual(actual, expected)

    def test_worked_example_and_mixed_terminators(self):
        records = HEADER + (
            b'1,"Kaphar, Titus","said ""no"" twice",7,1420.00\r\n'
            b'2,"Betts","a note that\nspans two lines",7,88.50\n'
            b'3,,"",12,0.00\r\n'
            b'4,"trailing space ","comma, quote "" and newline\ntogether",3,-5.25\n'
            + BOM + b'5,"Betts ","one\ntwo\nthree",12,-0.75\r\n'
            b'6,"",,12,10.00\r\n'
            + BOM + b'7,"Ann ""Annie"" Lee","x",5,2.00\n'
            b'8,"Nobody","",4000000,-1.00\r\n'
            b'9,"Neg","n",20,-0.05\r\n'
        )
        self.check_bytes(records, expected=(
            b'AAA 5 17 1517.75 -0.75 1420.00 4 24 55\n'
            b'BBB 1 4 -5.25 -5.25 -5.25 1 15 35\n'
            b'CCC 1 9 -0.05 -0.05 -0.05 1 3 1\n'
            b'_UNMATCHED 2 15 1.00 -1.00 2.00 2 21 1\n'))

    def test_header_only_and_final_record_without_terminator(self):
        self.check_bytes(HEADER, expected=b'')
        self.check_bytes(HEADER + record(1, '', '', 20, '-0.01', newline=''))

    def test_quotes_empty_values_trailing_spaces_and_literal_bom(self):
        rows = [HEADER, b'1,,,7,0.00\n', b'2,"","",7,0.00\r\n']
        for i, name in enumerate(['Betts', 'Betts ', 'Betts  ', 'Ann "Annie" Lee', '', 'A' + ' ' * 1000], 3):
            rows += [record(i, name, '"no"\none, two\n"\nlast\ufeff word', 7, '-0.99')]
        self.check_bytes(b''.join(rows))

    def test_parallel_boundaries_and_large_sums(self):
        rng = Random(17)
        rows = [HEADER]
        names = ['Betts', 'Betts ', 'Ann "Annie" Lee', '', 'Long' + ' ' * 140]
        notes = ['', 'one\ntwo\nthree\nfour', 'comma, and "quote"', '"no"', 'trailing ']
        for i in range(12_000):
            if i % 83 == 0 and i:
                rows.append(BOM)
            rows.append(record(25_000_000 + i, rng.choice(names), rng.choice(notes),
                               rng.choice([3, 7, 12, 20, 1, 9_999_999]),
                               rng.choice(['99999.99', '-999.99', '0.00', '-0.01']),
                               rng.choice(['\n', '\r\n'])))
        self.check_bytes(b''.join(rows))

    def test_hash_collisions_and_overflow_name_table(self):
        # Every name has the same sampled first/middle/last words and length.
        names = []
        for i in range(80):
            name = list('A' * 120)
            name[20] = chr(ord('A') + i // 26)
            name[21] = chr(ord('A') + i % 26)
            names.append(''.join(name))
        rng = Random(91)
        rows = [HEADER]
        for i in range(12_000):
            rows.append(record(i + 1, rng.choice(names), 'one\ntwo',
                               rng.choice([3, 7, 12, 20, 9999999]), '-0.05'))
        self.check_bytes(b''.join(rows))

    def test_parallel_country_words_and_shared_names(self):
        codes = [chr(65 + i // 676) + chr(65 + i // 26 % 26) + chr(65 + i % 26)
                 for i in range(240)]
        regions = b'region_id,country\n' + ''.join(
            f'{i + 1},{code}\n' for i, code in enumerate(codes)).encode()
        # Enough input for all workers: each country word and repeated pair is
        # updated from several chunks, including the unmatched country's bit.
        rows = [HEADER]
        for repeat in range(180):
            for country in range(241):
                rows.append(record(repeat * 241 + country + 1,
                                   ['Shared', 'Shared ', ''][repeat % 3], 'parallel',
                                   country + 1 if country < 240 else 9_999_999))
        self.check_bytes(b''.join(rows), regions)

    def test_all_country_ids_and_unknown_regions(self):
        # Full/sample permit 240 countries plus _UNMATCHED, fitting one byte.
        # Repeated names exercise every word of the four-word country bitset.
        codes = [chr(65 + i // 676) + chr(65 + i // 26 % 26) + chr(65 + i % 26)
                 for i in range(240)]
        regions = b'region_id,country\n' + ''.join(
            f'{i + 1},{code}\n' for i, code in enumerate(codes)).encode()
        rows = [HEADER]
        for repeat in range(3):
            for i in range(240):
                rows.append(record(repeat * 240 + i + 1, 'Shared', '', i + 1))
        rows += [record(1000, 'Shared', '', 999999),
                 record(1001, 'Shared', '', 4000000)]
        self.check_bytes(b''.join(rows), regions)


if __name__ == '__main__':
    unittest.main()
