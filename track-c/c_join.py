#!/usr/bin/env python3
"""Track C: streaming CSV join with exact integer aggregates."""
import argparse
import csv
import os


def money(cents):
    return f"{'-' if cents < 0 else ''}{abs(cents) // 100}.{abs(cents) % 100:02d}"


def main(data_dir, out_dir):
    groups = {}
    names = {}
    regions = {}
    with open(os.path.join(data_dir, "regions.csv"), encoding="ascii", newline="") as stream:
        next(stream)
        for line in stream:
            region, country = line.rstrip("\n").split(",")
            regions[int(region)] = country
    with open(os.path.join(data_dir, "records.csv"), encoding="utf-8", newline="") as stream:
        reader = csv.reader(stream)
        next(reader)
        for record_id, name, note, region, amount in reader:
            key = regions.get(int(region), "_UNMATCHED")
            cents = int(amount.replace(".", ""))
            group = groups.get(key)
            if group is None:
                # count, id sum, cent sum/min/max, distinct names, name/note bytes
                groups[key] = group = [0, 0, 0, cents, cents, set(), 0, 0]
            group[0] += 1
            group[1] += int(record_id.removeprefix("\ufeff"))
            group[2] += cents
            if cents < group[3]:
                group[3] = cents
            if cents > group[4]:
                group[4] = cents
            name = names.setdefault(name, name)
            group[5].add(name)
            group[6] += len(name.encode("utf-8"))
            group[7] += len(note.encode("utf-8"))
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "aggregates.txt"), "w", encoding="utf-8", newline="\n") as stream:
        for key in sorted(groups):
            count, ids, total, low, high, names, name_bytes, note_bytes = groups[key]
            stream.write(f"{key} {count} {ids} {money(total)} {money(low)} {money(high)} "
                         f"{len(names)} {name_bytes} {note_bytes}\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default="/data")
    parser.add_argument("--out", default="/out")
    args = parser.parse_args()
    main(args.data, args.out)
