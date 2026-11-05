"""Builds tests/data/locos_golden.json: random MO catalogs and what the LocOS MO filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    <LocOS venv python> -B gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (text, or the error class and
message), the bytes it builds with three mappings (every unit, every third unit, plain text) and the localized
builds for several target locales. Deterministic (seed 1104).
"""

import json
import os
import random
import struct
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-mo-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

MO = get_filter("mo")

WORDS = [
    "Open", "Save as", "File", "Edit", "%d file", "%s of %s", "Hello {name}", "Name: %(n)s", "Tab\tsep",
    "Line\nbreak\n", "\n\nLeading", "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u03a9mega", "%1$s then %2$s", "100%% sure",
    "{0} items", "quote \"x\" back\\slash", "emoji \U0001f600", "  padded  ", "x", "Close", "Yes", "No",
    "Settings", "Help",
]
TRANS = [
    "Ouvrir", "Enregistrer sous", "Fichier", "\u00c9diter", "Z\u00fcrich", "\u4e2d\u6587", "\u0645\u0631\u062d\u0628\u0627",
    "\ud55c\uad6d\uc5b4", "Caf\u00e9 cr\u00e8me", "%d Datei", "a\tb", "mehrzeilig\nText\n", "{0} Elemente", "Ja", "Nein",
]
LOCALES = ["ru-RU", "ar", "ja-JP", "de-DE", "pl-PL", "fr", "en-GB", "pt-BR", "zh-Hant-TW", "id-ID", "cy", "ko-KR"]
FULL_LOCALES = ["ru-RU", "ar", "ja-JP", "de-DE", "pl-PL"]


def pack(pairs, *, endian="<", hash_size=0, encoding="utf-8"):
    keys = [k.encode(encoding) if isinstance(k, str) else k for k, _v in pairs]
    vals = [v.encode(encoding) if isinstance(v, str) else v for _k, v in pairs]
    n = len(pairs)
    o = 28
    t = o + 8 * n
    h = t + 8 * n
    pos = h + 4 * hash_size
    otab, ttab, blob = b"", b"", b""
    for k in keys:
        otab += struct.pack(endian + "II", len(k), pos)
        blob += k + b"\x00"
        pos += len(k) + 1
    for v in vals:
        ttab += struct.pack(endian + "II", len(v), pos)
        blob += v + b"\x00"
        pos += len(v) + 1
    head = struct.pack(endian + "7I", 0x950412DE, 0, n, o, t, hash_size, h)
    return head + otab + ttab + b"\x00" * (4 * hash_size) + blob


def header(rng, legacy):
    if rng.random() < 0.12:
        return None
    lines = ["Project-Id-Version: demo %d.0" % rng.randint(1, 9)]
    if rng.random() < 0.7:
        lines.append("Language: " + rng.choice(["fr", "de", "en", "ru", "pl_PL", "ja", "ar", "pt_BR", "ru_RU"]))
    lines.append("MIME-Version: 1.0")
    if legacy:
        lines.append("Content-Type: text/plain; charset=ISO-8859-1")
    else:
        lines.append(
            rng.choice(
                [
                    "Content-Type: text/plain; charset=UTF-8",
                    "Content-Type: text/plain; charset=UTF-8",
                    "Content-Type: text/plain; charset=CHARSET",
                    "Content-Type: text/plain; charset=utf8",
                    "Content-Type: text/plain",
                    "Content-type: text/plain; Charset = utf-8 ",
                ]
            )
        )
    if rng.random() < 0.6:
        lines.append(rng.choice(["Plural-Forms: nplurals=2; plural=(n != 1);", "Plural-Forms: nplurals=2; plural=(n > 1);", "Plural-Forms: nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);"]))
    if rng.random() < 0.3:
        lines.append("X-Generator: tool")
    return "\n".join(lines) + ("\n" if rng.random() < 0.9 else "")


def make(rng, legacy=False):
    n = rng.randint(0, 14)
    pairs = []
    pool = [w for w in WORDS if not legacy or all(ord(c) < 256 for c in w)]
    tpool = [w for w in TRANS if not legacy or all(ord(c) < 256 for c in w)]
    for _ in range(n):
        msgid = rng.choice(pool) + (rng.choice(["", " " + rng.choice(pool)]))
        ctx = rng.choice([None, None, None, "menu", "state", ""])
        plural = rng.random() < 0.3
        if plural:
            key = msgid + "\x00" + rng.choice(["%d " + msgid, msgid + "s"])
            slots = rng.choice([1, 2, 2, 2, 3, 4])
            val = "\x00".join(rng.choice(tpool + [""]) if rng.random() < 0.8 else "" for _ in range(slots))
        else:
            key = msgid
            val = rng.choice(tpool) if rng.random() < 0.85 else ""
        if ctx is not None:
            key = ctx + "\x04" + key
        pairs.append((key, val))
    h = header(rng, legacy)
    if h is not None:
        pairs.append(("", h))
    if rng.random() < 0.9:
        pairs.sort(key=lambda kv: kv[0].encode("utf-8"))
    endian = rng.choice(["<", "<", ">"])
    hash_size = rng.choice([0, 0, 0, 5, 11])
    return pack(pairs, endian=endian, hash_size=hash_size, encoding="latin-1" if legacy else "utf-8")


def mappings(n):
    return {
        "all": {str(i): "T%d \u00e9 %%d {0}" % i if i % 2 else "T%d \u00e9\n[b] two" % i for i in range(n)},
        "third": {str(i): "Zeta %d \u65e5" % i for i in range(0, n, 3)},
        "plain": {str(i): "Plain%d" % i for i in range(n)},
    }


def case(name, data):
    out = {"name": name, "hex": data.hex()}
    try:
        res = MO.parse(data)
    except Exception as exc:  # noqa: BLE001
        out["error"] = type(exc).__name__ + ": " + str(exc)
        return out
    n = len(res.units)
    out["units"] = [u.text for u in res.units]
    if n == 0:
        return out
    maps = mappings(n)
    out["builds"] = {k: MO.build(data, v).hex() for k, v in maps.items()}
    loc = {}
    for lc in LOCALES:
        loc["all:" + lc] = MO.build_localized(data, maps["all"], lc).hex()
    for lc in FULL_LOCALES:
        loc["third:" + lc] = MO.build_localized(data, maps["third"], lc).hex()
        loc["plain:" + lc] = MO.build_localized(data, maps["plain"], lc).hex()
    out["localized"] = loc
    return out


def main():
    out_path = sys.argv[1]
    rng = random.Random(1104)
    cases = []
    for path in sys.argv[2:]:
        with open(path, "rb") as fh:
            cases.append(case(os.path.basename(path), fh.read()))
    for i in range(140):
        cases.append(case("random-%03d" % i, make(rng)))
    for i in range(24):
        cases.append(case("legacy-%03d" % i, make(rng, legacy=True)))
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    ok = sum(1 for c in cases if "units" in c)
    print(len(cases), "cases,", ok, "parsed,", len(cases) - ok, "refused")


if __name__ == "__main__":
    main()
