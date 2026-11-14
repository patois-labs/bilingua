"""Writes the oracle corpus folder oracle/corpus/scc/ (run with the LocOS venv Python, read only use of LocOS).

    python gen_corpus.py OUTDIR
"""

import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_golden as G  # noqa: E402

SUITE = [
    "Scenarist_SCC V1.0", "",
    "00:00:01:00\t94ae 94ae 9420 9420 9440 9440 c845 4c4c 4f80 94e0 94e0 574f 524c c480 942f 942f", "",
    "00:00:03:00\t942c 942c", "",
    "00:00:04:00\t9425 9425 94ad 94ad 9470 9470 4920 7361 7920 91ae 91ae 68e9 a120",
    "00:00:05:00\t94ad 94ad 9470 9470 d580 92a4 92a4 6e64 9137 9137",
    "00:00:06;02\t9429 9429 9154 9154 97a2 97a2 e361 e6dc",
    "00:00:07:00\t1c20 1c20 1c70 1c70 c1c2",
    "00:00:08:00\t9420 9420 9470 9470 c1c2 94a1 94a1 43c4 942f 942f",
]


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    files = {
        "lumen.scc": ("\r\n".join(SUITE) + "\r\n").encode(),
        "lumen_lf.scc": ("\n".join(SUITE) + "\n").encode(),
        "lumen_bom.scc": b"\xef\xbb\xbf" + ("\r\n".join(SUITE) + "\r\n").encode(),
        "lumen_utf16.scc": b"\xff\xfe" + ("\r\n".join(SUITE) + "\r\n").encode("utf-16-le"),
    }
    from app.filters.base import get_filter

    f = get_filter("scc")
    for seed in range(1, 9):
        rng = random.Random(9000 + seed)
        while True:  # a corpus file must hold at least three captions
            data = G.make(rng)
            if len(f.parse(data).units) >= 3:
                break
        files[f"random{seed}.scc"] = data
    for name, data in files.items():
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data)
    print(len(files), "files")


if __name__ == "__main__":
    main()
