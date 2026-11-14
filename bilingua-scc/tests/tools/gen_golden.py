"""Builds tests/data/locos_golden.json: random SCC files and what the LocOS SCC filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (or the error class) and, per
mapping, the bytes LocOS builds (or the refusal message). Deterministic (seed 1105). `gen_corpus` in this file
also writes the hand-shaped corpus folder when called with --corpus DIR.
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-scc-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters import scc_filter as S  # noqa: E402
from app.filters.base import get_filter  # noqa: E402

W = S._word
PAC = S._PAC_ROWS

WORDS = [
    "HELLO", "WORLD", "Bonjour", "caf\u00e9", "I say", "hi!", "Yes", "No way", "\u00a1Hola!", "\u00bfQu\u00e9?", "\u266a la la \u266a",
    "\u00dcber", "na\u00efve", "Se\u00f1or", "\u00c5ngstr\u00f6m", "\u00d8l", "100%", "a_b", "x*y", "{ok}", "\u00abquote\u00bb",
    "\u2018single\u2019", "\u201cdouble\u201d", "\u2014dash", "ja\u00dfa", "\u00a9 2026", "\u00a5en", "Ma\u00f1ana", "\u00e7a va",
    "\u00fa\u00f3\u00ed", "\u00d1and\u00fa", "[music]", "$5.00", "A-B", "it's", "Qu'est-ce", "\u00f7",
]


def text_words(s):
    return S._encode_atoms([("c", c) for c in s])


def row_words(rng, text=None, mid=False):
    t = text or " ".join(rng.choice(WORDS) for _ in range(rng.randint(1, 5)))
    t = S.normalize_608(t)
    t = "".join(c for c in t if c in S._ENC)[: rng.randint(4, 36)]
    out = []
    if mid and len(t) > 3:
        cut = rng.randint(1, len(t) - 1)
        out += text_words(t[:cut])
        mc = 0x20 + rng.randint(0, 15)
        out += [W(0x11, mc)] * 2
        out += text_words(t[cut:])
    else:
        out = text_words(t)
    return out


def pac(rng, row=None):
    row = row or rng.randint(1, 15)
    b1, base = PAC[row]
    if rng.random() < 0.5:
        attr = (rng.randint(0, 7) << 1) | rng.randint(0, 1)
    else:
        attr = 0x10 | (rng.randint(0, 7) << 1) | rng.randint(0, 1)
    out = [W(b1, base | attr)] * 2
    if rng.random() < 0.25:
        out += [W(0x17, 0x20 + rng.randint(1, 3))] * 2
    return out


def caption(rng):
    kind = rng.choices(
        ["popon", "rollup", "painton", "ch2", "bs", "junk", "empty", "multi"],
        [40, 20, 12, 5, 4, 6, 3, 4],
    )[0]
    d = lambda b2: [W(0x14, b2)] * 2  # noqa: E731
    if kind == "popon":
        out = d(0x2E) + d(0x20)
        rows = rng.randint(1, 4)
        base = rng.randint(rows, 15)
        for r in range(rows):
            out += pac(rng, base - rows + 1 + r) + row_words(rng, mid=rng.random() < 0.3)
        return out + d(0x2F)
    if kind == "rollup":
        out = d(0x25 + rng.randint(0, 2))
        for _ in range(rng.randint(1, 3)):
            out += d(0x2D) + pac(rng, rng.randint(1, 15)) + row_words(rng, mid=rng.random() < 0.4)
        return out
    if kind == "painton":
        return d(0x29) + pac(rng) + row_words(rng, mid=rng.random() < 0.3)
    if kind == "ch2":
        return [W(0x1C, 0x20)] * 2 + [W(0x1C, 0x70)] * 2 + text_words("CC TWO")
    if kind == "bs":
        return d(0x20) + pac(rng, 15) + text_words("ABCD") + d(0x21) + text_words("EF") + d(0x2F)
    if kind == "multi":
        return d(0x20) + pac(rng, 14) + row_words(rng) + d(0x2F) + d(0x2C) + d(0x20) + pac(rng, 15) + row_words(rng) + d(0x2F)
    if kind == "empty":
        return d(0x2C)
    return [("%04x" % rng.randint(0, 0xFFFF)) for _ in range(rng.randint(1, 8))]


def make(rng):
    nl = rng.choice(["\r\n", "\n", "\r", "\r\n"])
    lines = ["Scenarist_SCC V1.0", ""]
    t = 0
    for _ in range(rng.randint(0, 14)):
        t += rng.randint(1, 4000)
        fr = t % 30
        sec = (t // 30) % 60
        mn = (t // 1800) % 60
        sep = rng.choice([":", ":", ":", ";"])
        tc = "%02d:%02d:%02d%s%02d" % (0, mn, sec, sep, fr)
        words = caption(rng)
        if rng.random() < 0.1:
            words = [w.upper() for w in words]
        gap = rng.choice(["\t", "\t", "\t", " ", "  ", " \t"])
        lead = rng.choice(["", "", "", "", "  "])
        lines.append(lead + tc + gap + " ".join(words))
        if rng.random() < 0.8:
            lines.append("")
    text = nl.join(lines) + (nl if rng.random() < 0.8 else "")
    kind = rng.choice(["utf8", "utf8", "utf8", "utf8bom", "utf16le", "utf16be"])
    if kind == "utf8":
        return text.encode("utf-8")
    if kind == "utf8bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "utf16le":
        return b"\xff\xfe" + text.encode("utf-16-le")
    return b"\xfe\xff" + text.encode("utf-16-be")


def variants(texts, rng):
    """Mappings of unit index -> new text (LocOS token form)."""
    n = len(texts)
    out = {}
    out["same"] = {str(i): t for i, t in enumerate(texts)}
    out["words"] = {str(i): re.sub(r"[^\W\d_]+", "Mot", t) for i, t in enumerate(texts)}
    out["extra"] = {str(i): t + " ok" for i, t in enumerate(texts)}
    out["long"] = {str(i): " ".join(["Voici une phrase vraiment longue"] * (i % 5 + 1)) for i in range(n)}
    out["latin"] = {
        str(i): "\u00dcber na\u00efve caf\u00e9 \u266a \u00aboui\u00bb \u2013 l\u2019\u00e9t\u00e9\u2026 \u00d1and\u00fa \u00bfqu\u00e9? {x} ~ | ^ _ \\ *" for i in range(n)
    }
    out["cjk"] = {str(i): ("\u65e5\u672c\u8a9e" if i % 4 == 0 else "ok") for i in range(n)}
    out["third"] = {str(i): "Zeta %d" % i for i in range(0, n, 3)}
    out["short"] = {str(i): "x" * (i % 39 + 1) for i in range(n)}
    out["lines"] = {str(i): "Ligne un\nLigne deux\nLigne trois" for i in range(n)}
    out["nbsp"] = {str(i): "a\u00a0b  c \u2009d" for i in range(n)}
    return out


def golden(data):
    f = get_filter("scc")
    rec = {"hex": data.hex()}
    try:
        pr = f.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    texts = [u.text for u in pr.units]
    rec["units"] = texts
    rec["kept"] = pr.skeleton.get("scc_lines_kept_verbatim", 0)
    builds = {}
    for name, m in variants(texts, None).items():
        try:
            builds[name] = {"hex": f.build(data, m).hex()}
        except Exception as e:  # noqa: BLE001
            builds[name] = {"error": str(e)}
    rec["builds"] = builds
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "00",
    "5363656e61726973745f5343432056312e300a0a30303a30303a30313a303009",
    "5363656e61726973745f53434320562e300a",
    "fffe53006300",
    "5363656e61726973745f53434320563120300a0a",
)]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    rng = random.Random(1105)
    cases = []
    for i in range(140):
        cases.append({"name": f"random-{i}", **golden(make(rng))})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h)})
    for path in extra:
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, separators=(",", ":"))
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused,",
          sum(len(c.get("units", [])) for c in cases), "units")


if __name__ == "__main__":
    main()
