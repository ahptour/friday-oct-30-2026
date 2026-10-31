#!/usr/bin/env python3
"""Track B: query-driven postings and exact normalized token phrases."""
import argparse
from heapq import nsmallest
import os
import re


def main(data_dir, out_dir):
    with open(os.path.join(data_dir, "queries.txt"), "rb") as stream:
        queries = [line.rstrip(b"\n") for line in stream]
    index = {}
    for query in queries:
        if query.startswith(b"BOOL "):
            terms = query[5:].split(b" ")[::2]
        else:
            terms = query[8:-1].split(b" ")
        for term in terms:
            index.setdefault(term, set())

    tokenize = re.compile(rb"[a-z0-9]+").findall
    texts = []
    with open(os.path.join(data_dir, "docs.txt"), "rb") as stream:
        for doc_id, line in enumerate(stream):
            tokens = tokenize(line.partition(b"\t")[2].lower())
            # Keep unknown tokens too: removing them would create false phrases.
            texts.append(b" " + b" ".join(tokens) + b" ")
            for term in set(tokens):
                postings = index.get(term)
                if postings is not None:
                    postings.add(doc_id)

    cache = {}
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "results.txt"), "w", encoding="ascii", newline="\n") as stream:
        for query in queries:
            answer = cache.get(query)
            if answer is None:
                if query.startswith(b"BOOL "):
                    words = query[5:].split(b" ")
                    matches = index[words[0]]
                    for op, term in zip(words[1::2], words[2::2]):
                        if op == b"AND":
                            matches = matches & index[term]
                        elif op == b"OR":
                            matches = matches | index[term]
                        else:
                            matches = matches - index[term]
                else:
                    terms = query[8:-1].split(b" ")
                    postings = sorted((index[term] for term in terms), key=len)
                    matches = postings[0]
                    for posting in postings[1:]:
                        matches = matches & posting
                        if not matches:
                            break
                    phrase = b" " + b" ".join(terms) + b" "
                    matches = [doc_id for doc_id in matches if phrase in texts[doc_id]]
                answer = " ".join(map(str, [len(matches), sum(matches), *nsmallest(10, matches)])) + "\n"
                cache[query] = answer
            stream.write(answer)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default="/data")
    parser.add_argument("--out", default="/out")
    args = parser.parse_args()
    main(args.data, args.out)
