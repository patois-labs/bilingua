"""Builds tests/data/locos_golden.json: random EBU STL files and what the LocOS STL filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    <LocOS venv python> -B gen_golden.py OUT.json

For each file the golden holds the input bytes (hex), the units LocOS parses (text, or the error class and
message) and, for several translation maps and target locales, the bytes LocOS builds (or the refusal message).
Deterministic (seed 1029).
"""

import hashlib
import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-stl-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app.filters import get_filter  # noqa: E402

STL = get_filter("stl")
rnd = random.Random(1029)


def gsi(*, cct=b"00", lc=b"09", dsc=b"1", mnc=b"40", tnb=0, dfc=b"STL25.01"):
    g = bytearray(b" " * 1024)
    g[0:3] = b"850"
    g[3:11] = dfc
    g[11:12] = dsc
    g[12:14] = cct
    g[14:16] = lc
    g[16:48] = b"Golden programme".ljust(32)
    g[224:230] = b"261010"
    g[230:236] = b"261010"
    g[236:238] = b"01"
    g[238:243] = b"%05d" % tnb
    g[243:248] = b"%05d" % tnb
    g[248:251] = b"001"
    g[251:253] = mnc
    g[253:255] = b"23"
    g[255:256] = b"1"
    g[448:460] = b"user area ok"
    return bytes(g)


def tti(sn, tf, *, ebn=0xFF, cf=0, vp=20, jc=2, cs=0, tci=(10, 0, 1, 0), tco=(10, 0, 3, 0)):
    assert len(tf) <= 112
    head = bytes([0]) + sn.to_bytes(2, "little") + bytes([ebn, cs, *tci, *tco, vp, jc, cf])
    return head + tf + b"\x8f" * (112 - len(tf))


def blocks_of(sn, tf, **kw):
    """A subtitle's TF split over extension blocks (never between a diacritic and its letter)."""
    chunks, i = [], 0
    while i < len(tf):
        end = min(i + 112, len(tf))
        if end < len(tf) and 0xC1 <= tf[end - 1] <= 0xCF:
            end -= 1
        chunks.append(tf[i:end])
        i = end
    if not chunks:
        chunks = [b""]
    return [tti(sn, c, ebn=(0xFF if k == len(chunks) - 1 else k), **kw) for k, c in enumerate(chunks)]


DH = b"\x0d\x0b\x0b"
END = b"\x0a\x0a"

WORDS_00 = [
    b"Hello", b"there,", b"how", b"are", b"you?", b"Caf\xc2e", b"cr\xc8eme", b"\xe1on", b"$5", b"#1", b"\xa3" + b"20",
    b"na\xc8ive", b"\xcbca", b"\xc5a", b"stra\xfbe", b"\xe9", b"A\xc4", b"\xc3o", b"\xcf" + b"c", b"x\xc1", b"\xd4", b"\xd5",
    b"quoted", b"\xaaq\xba", b"\xab" + b"w\xbb", b"\xa9s\xb9", b"0123", b"end.", b"1/2", b"\xbd",
]
CODES = [b"\x01", b"\x07", b"\x80", b"\x81", b"\x82", b"\x83", b"\x0c", b"\x1d", b"\x03"]
ODD = [b"\x7f", b"\x9f", b"\x90", b"\xc1", b"\xc8", b"\x00", b"\x1f", b"\xa0"]


def high_words(cct):
    """Words for tables 01-04: ASCII plus bytes of the upper half (the table itself decides what they mean)."""
    out = [b"abc", b"x1", b"Hello", b"%d", b"-"]
    for _ in range(14):
        out.append(bytes(rnd.choice([rnd.randrange(0xA1, 0xFF), rnd.randrange(0x61, 0x7A)]) for _ in range(rnd.randrange(2, 7))))
    return out


def row_text(cct, mnc, allow_codes):
    words = high_words(cct) if cct != b"00" else WORDS_00
    n = rnd.randrange(1, 6)
    parts = []
    for _ in range(n):
        w = rnd.choice(words)
        if allow_codes and rnd.random() < 0.18:
            w = w + rnd.choice(CODES) + rnd.choice(words)
        if rnd.random() < 0.05:
            w = w + rnd.choice(ODD)
        parts.append(w)
    return b" ".join(parts)


def subtitle(sn, cct, mnc, *, rows=None):
    rows = rows or rnd.choice([1, 1, 2, 2, 3])
    dh = rnd.random() < 0.5
    pre = DH if dh else rnd.choice([b"", b"\x0b\x0b", b"\x01", b"\x0c"])
    suf = END if dh else rnd.choice([b"", b"\x0a\x0a", b"\x07"])
    sep = b"\x8a\x8a" if dh else b"\x8a"
    tf = b""
    for r in range(rows):
        if r:
            tf += sep if rnd.random() < 0.85 else b"\x8a"
        tf += pre + row_text(cct, mnc, True) + suf
    if rnd.random() < 0.1:
        tf = b"\x8a" + tf
    if rnd.random() < 0.1:
        tf += b"\x8a\x8a"
    return tf


def make_file(cct, k):
    mnc = rnd.choice([b"40", b"40", b"32", b"37", b"  ", b"99", b"+8", b"xx", b"-5", b"20"])
    dsc = rnd.choice([b"1", b"2", b"0", b" "])
    blocks = []
    nsub = rnd.randrange(1, 9)
    for sn in range(1, nsub + 1):
        kind = rnd.random()
        tf = subtitle(sn, cct, mnc)
        vp = rnd.choice([0, 1, 2, 18, 20, 22, 250, 255])
        if kind < 0.12:
            blocks.append(tti(sn, b"Translator note " + rnd.choice(WORDS_00), cf=1))
            continue
        if kind < 0.2:
            big = tf * rnd.randrange(3, 9)
            bl = blocks_of(sn, big, vp=vp)
            blocks.extend(bl)
        else:
            bl = blocks_of(sn, tf, vp=vp, tci=(10, 0, sn, 0), tco=(10, 0, sn + 2, 0))
            blocks.extend(bl)
            if rnd.random() < 0.12:
                blocks.insert(len(blocks) - 1, tti(sn, b"user data", ebn=0xFE)) if len(bl) > 1 else blocks.append(tti(sn, b"user", ebn=0xFE))
        if rnd.random() < 0.08:
            blocks.append(tti(900 + sn, b"\x0d\x0b\x0b \x0a\x0a"))  # a subtitle with no text
    return gsi(cct=cct, dsc=dsc, mnc=mnc, tnb=len(blocks), lc=rnd.choice([b"09", b"00", b"0F", b"  "])) + b"".join(blocks)


LANG = {
    "en": "Hello there, how are you today?",
    "fr": "Bonjour, ça va très bien ? Où êtes-vous ?",
    "de": "Schöne Grüße aus Köln, ÄÖÜ und ß",
    "es": "¿Cómo estás? ¡Muy bien, señor!",
    "pl": "Zażółć gęślą jaźń",
    "tr": "Şişli çocuğu ığdır",
    "vi": "Xin chào thế giới đẹp",
    "ru": "Привет, как дела? Всё хорошо «да» — конечно…",
    "uk": "Привіт, як справи їжак",
    "ar": "مرحبا بالعالم كيف حالك",
    "el": "Καλημέρα κόσμε, τι κάνεις;",
    "he": "שלום עולם מה שלומך",
    "ja": "こんにちは世界",
    "zh": "你好世界",
    "ko": "안녕하세요 세계",
    "mix": "Привет 你好",
    "quotes": "«Bonjour» — “ami” ‘x’ „y“ … 5–6 a\u202fb\u200bc",
    "long": "This translation is much longer than the original line and wraps over the second row of the subtitle",
    "huge": "word " * 80,
    "wide": "x" * 120,
    "nfd": "Cafe\u0301 cre\u0300me bru\u0302le\u0301e a\u0308 x\u0300",
    "ctl": "tab\there nul\x00 bell\x07",
    "empty": "   ",
}
LOCALES = ["fr-FR", "nl-BE", "ru", "ar-EG", "ja-JP", "zh-Hant-TW", "ko", "x-klingon", "iw", "pt_BR", "he", "EN"]


def translation(unit_text, lang):
    toks = [t for t in unit_text.replace("\n", " ").split() if t.startswith("\u27e6")]
    # keep the codes that are inside words as their own words
    import re

    codes = re.findall("\u27e6x\\d+\u27e7", unit_text)
    base = LANG[lang]
    if "\n" in unit_text and lang not in ("huge", "wide", "empty", "ctl"):
        words = base.split(" ")
        h = max(1, len(words) // 2)
        base = " ".join(words[:h]) + "\n" + " ".join(words[h:])
    return (base + (" " + " ".join(codes) if codes and lang not in ("empty",) else "")).rstrip("\n") if lang != "empty" else base


def attempt(fn):
    try:
        out = fn()
        return {"sha": hashlib.sha256(out).hexdigest(), "len": len(out), "hex": out.hex() if len(out) <= 600 else ""}
    except Exception as e:  # noqa: BLE001
        return {"err": type(e).__name__, "msg": str(e)}


def case(name, data):
    c = {"name": name, "hex": data.hex()}
    try:
        pr = STL.parse(data)
    except Exception as e:  # noqa: BLE001
        c["error"] = type(e).__name__
        c["msg"] = str(e)
        return c
    units = [u.text for u in pr.units]
    c["units"] = units
    builds = {}
    n = len(units)

    def mapping(label):
        if label == "same":
            return {str(i): t for i, t in enumerate(units)}
        if label == "none":
            return {}
        if label == "third":
            return {str(i): "«" + t + "»" for i, t in enumerate(units) if i % 3 == 0}
        if label == "tokens-moved":
            import re

            out = {}
            for i, t in enumerate(units):
                cs = re.findall("\u27e6x\\d+\u27e7", t)
                if len(cs) >= 2:
                    out[str(i)] = " ".join(reversed(cs)) + " moved"
            return out
        if label == "bad-token":
            return {"0": "text \u27e6x9\u27e7 more"} if n else {}
        lang = label
        return {str(i): translation(t, lang) for i, t in enumerate(units)}

    labels = ["same", "none", "third", "tokens-moved", "bad-token"] + list(LANG)
    for label in labels:
        m = mapping(label)
        r = attempt(lambda m=m: STL.build(data, m))
        r["map"] = m
        builds[label] = r
    c["builds"] = builds
    loc = {}
    for lg in ["en", "fr", "ru", "ar"]:
        m = mapping(lg)
        for locale in LOCALES:
            r = attempt(lambda m=m, locale=locale: STL.build_localized(data, m, locale))
            r["lang"] = lg
            loc[f"{lg}:{locale}"] = r
    for locale in LOCALES:
        r = attempt(lambda locale=locale: STL.build_localized(data, {}, locale))
        r["lang"] = "none"
        loc[f"none:{locale}"] = r
    c["localized"] = loc
    return c


def suite_files():
    subs = [
        tti(1, DH + b"Hello there," + END + b"\x8a\x8a" + DH + b"how are you?" + END),
        tti(2, DH + b"Caf\xc2e cr\xc8eme" + END, vp=22, tci=(10, 0, 4, 0), tco=(10, 0, 6, 0)),
        tti(3, DH + b"Red \x01alert\x07 now" + END, tci=(10, 0, 7, 0), tco=(10, 0, 9, 0)),
        tti(4, b"Translator note: check the name", cf=1),
        tti(4, b"user data block", ebn=0xFE),
        tti(5, DH + b"A long subtitle in two " + END, ebn=0x00),
        tti(5, b"\x8a\x8a" + DH + b"extension blocks." + END),
        tti(6, b"\x80Italic\x81 open text $5 #1"),
    ]
    out = {"suite": gsi(tnb=len(subs)) + b"".join(subs)}
    out["suite30"] = gsi(tnb=len(subs), dfc=b"STL30.01") + b"".join(subs)
    out["plain"] = gsi(tnb=2) + tti(1, DH + b"Hello," + END + b"\x8a\x8a" + DH + b"friend" + END) + tti(2, b"See you $5")
    out["odd"] = gsi(tnb=1) + tti(1, b"ab\xc2\x8b\x9f\x7fend\xc8")
    out["empty"] = gsi(tnb=0)
    out["only-comment"] = gsi(tnb=1) + tti(1, b"just a note", cf=1)
    out["nbsp"] = gsi(tnb=1) + tti(1, b"\xa0\xa0")
    for cct in (b"01", b"02", b"03", b"04", b"  "):
        out["cct" + cct.decode().strip()] = gsi(cct=cct, tnb=1) + tti(1, DH + bytes(range(0x41, 0x4B)) + b" " + bytes(range(0xB0, 0xC0)) + END)
    # hostile
    out["3d-model"] = b"solid cube\nfacet normal 0 0 1\n"
    out["zeros"] = b"\x00" * 84
    out["truncated"] = out["suite"][:1100]
    out["bad-cct"] = gsi(cct=b"09")
    out["bad-cct-spaceA"] = gsi(cct=b" 0")
    out["bad-dfc"] = gsi(dfc=b"STL26.01")
    return out


def main():
    cases = [case(n, d) for n, d in suite_files().items()]
    for i in range(120):
        cct = [b"00", b"00", b"00", b"01", b"02", b"03", b"04", b"  "][i % 8]
        cases.append(case(f"rand-{i:03d}-{cct.decode().strip() or 'sp'}", make_file(cct, i)))
    with open(sys.argv[1], "w", encoding="utf-8") as fh:
        json.dump(cases, fh, ensure_ascii=False)
    ok = sum(1 for c in cases if "units" in c)
    errs = sum(1 for c in cases if "error" in c)
    builds = sum(len(c.get("builds", {})) + len(c.get("localized", {})) for c in cases)
    refused = sum(1 for c in cases for v in list(c.get("builds", {}).values()) + list(c.get("localized", {}).values()) if "err" in v)
    print(f"{len(cases)} cases, {ok} parsed, {errs} refused, {builds} builds ({refused} refused)")


main()
