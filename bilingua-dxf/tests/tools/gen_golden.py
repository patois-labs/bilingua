"""Builds tests/data/locos_golden.json: random DXF files, mutated and truncated ones, and the corpus, with what the
LocOS DXF filter makes of each.

    <LocOS venv python> gen_golden.py OUT.json CORPUS_DIR

Read only use of the LocOS filters (nothing is written there). For each file the golden holds the bytes (hex), the units
LocOS parses as [style, text] (or the error class) and the bytes LocOS builds under four mappings. Deterministic
(seed 1105). The mappings are written out in tests/locos_golden.rs; keep the two in step.
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-dxf-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

BS = "\\"
WORDS = [
    "Living room", "Fire exit", "Notausgang", "Plan du rez-de-chaussée", "Гостиная", "非常口", "مخرج الطوارئ", "한국어 텍스트",
    "Σαλόνι", "שלום עולם", "สวัสดี", "x", "1200", "  ", "", "A", "Größe", "Caf" + BS + "U+00E9", "wide " + BS + "U+4E2D",
    "pair " + BS + "U+D83D" + BS + "U+DE00", "bad " + BS + "U+D83D end", "m " + BS + "M+18A8A x", "m " + BS + "M+28A8A x",
    "%%c50 pipe, 45%%d", "100%%%", "%%123x", "%%u under", "^I tab ^J", "<> typical", "[] alt", "<>",
    "{" + BS + "fArial|b1;Bold}" + BS + "Pnext", "a{b", "a}b", "{{x}}", BS + "S1/2; inch", BS + "S1^2; open", BS + "H2.5x;tall",
    BS + "~ nb", BS + BS + " double", BS + "{ brace" + BS + "}", "trail" + BS, "ctrl" + chr(1) + "x", "emoji \U0001f600 x",
    "tab\there", "x" * 300, ("long text " * 40).strip(), BS + "P" * 5 + "word", BS + "Q15;" + BS + "C1;colored" + BS + "c255;",
]
ENTS = [
    ("TEXT", [1]), ("MTEXT", [1, 3]), ("ATTRIB", [1]), ("ATTDEF", [1, 3]), ("DIMENSION", [1]), ("MULTILEADER", [304]),
    ("MLEADER", [304]), ("INSERT", []), ("LINE", []), ("LWPOLYLINE", []), ("TEXT", [1]), ("MTEXT", [1, 3]),
]
VERS = [None, "AC1006", "AC1009", "AC1015", "AC1018", "AC1021", "AC1027", "AC1032", "ACXX", "ac1015", ""]
CPS = [None, "ANSI_1252", "ANSI_1251", "ANSI_1253", "ANSI_1256", "ANSI_932", "ANSI_936", "ANSI_949", "ANSI_950", "DOS437",
       "DOS864", "DOS866", "UTF8", "ANSI_9999", "ansi_1252", ""]
CODECS = ["utf-8", "cp1252", "cp1251", "cp932", "gbk", "cp949", "cp950", "cp1253", "cp1256", "cp437", "cp864", "cp874"]


def make(rng):
    nl = rng.choice(["\r\n", "\n", "\r", "\r\n"])
    pairs = []
    ver, cp = rng.choice(VERS), rng.choice(CPS)
    if rng.random() < 0.85:
        pairs += [(0, "SECTION"), (2, "HEADER")]
        if ver is not None:
            pairs += [(9, "$ACADVER"), (1, ver)]
        if cp is not None:
            pairs += [(9, "$DWGCODEPAGE"), (3, cp)]
        pairs += [(9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]
    for sec in rng.sample(["TABLES", "BLOCKS", "ENTITIES", "ENTITIES", "OBJECTS", "CLASSES"], rng.randint(1, 4)):
        pairs += [(0, "SECTION"), (2, sec)]
        for _ in range(rng.randint(0, 12)):
            name, codes = rng.choice(ENTS)
            pairs += [(0, name), (5, "%X" % rng.randint(1, 4000)), (8, rng.choice(["0", "Walls Text", "Notes"]))]
            for code in codes:
                if code == 3 and name == "MTEXT":
                    for _ in range(rng.randint(0, 3)):
                        pairs.append((3, rng.choice(WORDS)[:250]))
                    pairs.append((1, rng.choice(WORDS)))
                else:
                    pairs.append((code, rng.choice(WORDS)))
                if rng.random() < 0.2:
                    pairs.append((rng.choice([7, 10, 40, 70, 1000, 1001, 100]), rng.choice(["Standard", "1.0", "0"])))
            if name in ("ATTRIB", "ATTDEF", "MTEXT") and rng.random() < 0.3:
                pairs += [(101, "Embedded Object"), (1, rng.choice(WORDS)), (3, rng.choice(WORDS))]
        pairs.append((0, "ENDSEC"))
    pairs.append((0, "EOF"))
    pad = rng.random() < 0.3
    text = ""
    for code, val in pairs:
        text += (f"{code:>3}" if pad else str(code)) + nl + val + nl
    if rng.random() < 0.15:
        text = text.rstrip("\r\n")
    codec = rng.choice(CODECS)
    data = text.encode(codec, "replace")
    if rng.random() < 0.15:
        data = b"\xef\xbb\xbf" + data
    if rng.random() < 0.1:
        data += b"\x1a\x00 \r\n"
    return data


def mutate(rng, data):
    d = bytearray(data)
    for _ in range(rng.randint(1, 4)):
        if not d:
            break
        p = rng.randrange(len(d))
        op = rng.randrange(5)
        if op == 0:
            d[p] = rng.randrange(256)
        elif op == 1:
            del d[p]
        elif op == 2:
            d.insert(p, rng.randrange(256))
        elif op == 3:
            del d[p:]
        else:
            d[p:p] = d[p : p + rng.randint(1, 40)]
    return bytes(d)


def maps(units):
    n = len(units)
    allm = {}
    third = {}
    plain = {}
    longj = {}
    for i, u in enumerate(units):
        t = u.text
        allm[str(i)] = f"[T{i}] {t} \u00e9\u65e5\nx{i}\t^%%{{}}\\"
        if i % 3 == 0:
            third[str(i)] = f"Zeta {i} \u65e5\u672c\u8a9e " + "x" * (300 if i % 2 == 0 else 5)
        plain[str(i)] = f"Plain{i}"
        if i % 5 == 0:
            longj[str(i)] = "\u65e5\u672c\u8a9e\u306e\u30c6\u30ad\u30b9\u30c8" * 40 + t
    return {"all": allm, "third": third, "plain": plain, "longj": longj}


def golden(data):
    f = get_filter("dxf")
    rec = {"hex": data.hex()}
    try:
        units = f.parse(data).units
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    rec["units"] = [[u.style, u.text] for u in units]
    builds = {}
    for name, m in maps(units).items():
        try:
            builds[name] = f.build(data, m).hex()
        except Exception as e:  # noqa: BLE001
            builds[name] = "error:" + type(e).__name__
    rec["builds"] = builds
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "", "0a", "2020200a", "300d0a534543544f4e0d0a", "AutoCAD Binary DXF\r\n\x1a\x00".encode().hex(),
    "20200a53454354494f4e", "300a53454354494f4e0a320a454e544954494553", "ffff", "300a45454f46",
)]


def main():
    out, corpus = sys.argv[1], sys.argv[2]
    rng = random.Random(1105)
    cases = []
    for i in range(110):
        cases.append({"name": f"random-{i}", **golden(make(rng))})
    for i in range(70):
        cases.append({"name": f"mutated-{i}", **golden(mutate(rng, make(rng)))})
    for i in range(30):
        d = make(rng)
        cases.append({"name": f"truncated-{i}", **golden(d[: rng.randrange(len(d) + 1)])})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h)})
    for name in sorted(os.listdir(corpus)):
        with open(os.path.join(corpus, name), "rb") as fh:
            cases.append({"name": name, **golden(fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()
