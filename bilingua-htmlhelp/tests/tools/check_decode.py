r"""Checks the crate's decode against Python's codecs, end to end (gate 11-05). For each charset label:

    cargo run --release -p bilingua-htmlhelp --example htmlhelp_decmap -- LABEL DIR/LABEL.txt   (add 3 for euc-jp)
    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe -I check_decode.py DIR LABEL...

Counts, per label, sequences where A: Python decodes and the crate does not (it fell back), B: both decode to
different text, C: Python refuses and the crate decodes. All three must be 0. Sequences that are valid UTF-8 are
skipped (both read the file as UTF-8)."""
import codecs
import sys

for label in sys.argv[2:]:
    py = codecs.lookup(label).name
    cnt = {"A": 0, "B": 0, "C": 0}
    ex = {"A": [], "B": [], "C": []}
    rust_tags = set()
    for line in open(f"{sys.argv[1]}/{label}.txt", encoding="ascii"):
        p = line.rstrip("\n").split("\t")
        seq = bytes.fromhex(p[0])
        try:
            seq.decode("utf-8")
            continue  # read as UTF-8 by both
        except UnicodeDecodeError:
            pass
        try:
            pt = " ".join(f"{ord(c):x}" for c in seq.decode(py))
        except UnicodeDecodeError:
            pt = None
        if p[1] == "ERR":
            rt = None
        else:
            rust_tags.add(p[1])
            rt = None if p[1] in ("windows-1252",) and label not in ("windows-1252", "cp1252") else (p[2] if len(p) > 2 else "")
        if pt == rt:
            continue
        k = "A" if rt is None else "C" if pt is None else "B"
        cnt[k] += 1
        if len(ex[k]) < 6:
            ex[k].append((p[0], pt, rt))
    print(label, py, cnt, sorted(rust_tags))
    for k in "ABC":
        if ex[k]:
            print("  ", k, ex[k])
