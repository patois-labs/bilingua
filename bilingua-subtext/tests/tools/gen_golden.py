"""Builds tests/data/locos_golden.json: random SBV / SubViewer 2.0 / MicroDVD files and what the LocOS filters make of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json

For each input the golden holds the format, the bytes (hex), the unit texts LocOS parses (or the error class) and the
bytes LocOS builds (length:FNV-1a-64 digests, or the error) with several mappings of the unit texts (see MAPS).
Deterministic (seed 1105). The Rust test applies the same mappings.
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-subtext-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

TOK = re.compile("\u27e6/?[a-z]?\\d+\u27e7")


def fnv(b: bytes) -> str:
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{len(b)}:{h:016x}"


def map_text(name: str, i: int, t: str):
    if name == "echo":
        return t
    if name == "mark":
        return "~" + t + "!"
    if name == "all":
        return f"T{i} \u00e9 x\n\u65e5\u672c\u8a9e"
    if name == "third":
        return f"Zeta {i} \u65e5" if i % 3 == 0 else None
    if name == "cjk":
        return "\u65e5\u672c" + t.replace(" ", "") + "\u3067\u3059"
    if name == "long":
        return t + " " + t
    if name == "short":
        return "Kurz" if i % 2 == 0 else None
    if name == "rev":
        return "\n".join(reversed(t.split("\n")))
    if name == "notok":
        return TOK.sub("", t) + "!"
    if name == "nl":
        return t.replace(" ", "\n")
    if name == "ws":
        return "  " + t + " \n"
    raise ValueError(name)


MAPS = ["echo", "mark", "all", "third", "cjk", "long", "short", "rev", "notok", "nl", "ws"]


def run_case(fmt: str, data: bytes) -> dict:
    F = get_filter(fmt)
    out: dict = {"fmt": fmt, "hex": data.hex()}
    try:
        r = F.parse(data)
    except Exception as e:  # noqa: BLE001
        out["error"] = type(e).__name__
        return out
    texts = [u.text for u in r.units]
    out["units"] = texts
    out["maps"] = [{k: v for k, v in u.inline_map.items()} for u in r.units]
    builds = {}
    for m in MAPS:
        mp = {str(i): v for i, t in enumerate(texts) if (v := map_text(m, i, t)) is not None}
        try:
            builds[m] = fnv(F.build(data, mp))
        except Exception as e:  # noqa: BLE001
            builds[m] = {"err": type(e).__name__, "msg": str(e)}
    out["builds"] = builds
    return out


WORDS = ["the", "quick", "brown", "fox", "Hello!", "How", "are", "you?", "Caf\u00e9", "na\u00efve", "x", "A", "2024", "{0}", "%s", "%1$s", "a|b", "[br]", "{", "}", "|"]
CJK = ["\u65e5\u672c\u8a9e", "\u4e2d\u6587", "\ud55c\uad6d\uc5b4", "\u0627\u0644\u0639\u0631\u0628\u064a\u0629", "e\u0301t\u00e9", "\u0939\u093f\u0928\u094d\u0926\u0940", "x\u3000", "\u00a0y"]
# a piece of only Unicode spaces (not [ \t]) is the one place the Rust filter differs on purpose: see hostile.rs
SEPS = [" ", " ", " ", "  ", "\t", "\u3000"]


def words(rng, n=None):
    parts = []
    for _ in range(n or rng.randint(1, 6)):
        parts.append(rng.choice(CJK) if rng.random() < 0.15 else rng.choice(WORDS))
    s = parts[0]
    for p in parts[1:]:
        s += rng.choice(SEPS) + p
    return s


def tcode(rng, ms=True):
    def one():
        h, m, s = rng.randint(0, 9), rng.randint(0, 59), rng.randint(0, 59)
        f = rng.randint(0, 999) if ms else rng.randint(0, 99)
        fs = f"{f:03d}" if ms else f"{f:02d}"
        if rng.random() < 0.1:
            fs = fs[: rng.randint(1, len(fs))]
        t = f"{h}:{m:02d}:{s:02d}.{fs}"
        if rng.random() < 0.03:
            t = t.translate(str.maketrans("0123456789", "\u0660\u0661\u0662\u0663\u0664\u0665\u0666\u0667\u0668\u0669"))
        return t

    sep = rng.choice([",", ",", ",", " , ", ", "])
    t = one() + sep + one()
    if rng.random() < 0.1:
        t = rng.choice([" ", "\t"]) + t
    if rng.random() < 0.1:
        t += rng.choice([" ", "\t"])
    if rng.random() < 0.04:
        t = t[:-1]  # broken
    return t


NLS = ["\n", "\n", "\n", "\r\n", "\r"]


def text_lines(rng):
    n = rng.choice([0, 1, 1, 1, 2, 2, 3])
    out = []
    for _ in range(n):
        ln = words(rng)
        if rng.random() < 0.15:
            ln += rng.choice(["  ", "\t", " "])
        if rng.random() < 0.1:
            ln = " " + ln
        out.append(ln)
    return out


def blank(rng):
    return rng.choice(["", "", "", "", " ", "\t", "\u3000"])


def finish(rng, body: list, nl: str, pad=True) -> bytes:
    nls = [rng.choice(NLS) for _ in body] if rng.random() < 0.08 else [nl] * len(body)
    s = "".join(b + n for b, n in zip(body, nls))
    if rng.random() < 0.15 and nls:
        s = s[: len(s) - len(nls[-1])]
    data = s.encode("utf-8")
    r = rng.random()
    if r < 0.1:
        data = b"\xef\xbb\xbf" + data
    elif r < 0.14:
        data = "\ufeff".encode("utf-16-le")[:0] + s.encode("utf-16")
    elif r < 0.17:
        data = s.encode("utf-16-be")
        data = b"\xfe\xff" + data
    elif r < 0.2:
        try:
            data = s.encode("cp1252")
        except UnicodeEncodeError:
            pass
    return data


def sbv_doc(rng):
    nl = rng.choice(NLS)
    body = []
    if rng.random() < 0.1:
        body.append(words(rng))
    for _ in range(rng.randint(0, 8)):
        body.append(tcode(rng))
        body += text_lines(rng)
        for _ in range(rng.choice([1, 1, 1, 2, 0])):
            body.append(blank(rng))
    return finish(rng, body, nl)


def sv2_doc(rng):
    nl = rng.choice(["\r\n", "\r\n", "\n", "\r"])
    body = []
    if rng.random() < 0.8:
        body += ["[INFORMATION]", "[TITLE]" + words(rng, 2), "[AUTHOR]x", "[END INFORMATION]", "[SUBTITLE]", "[COLF]&HFFFFFF,[STYLE]bd,[SIZE]18,[FONT]Arial"]
    for _ in range(rng.randint(1, 8)):
        body.append(tcode(rng, ms=False))
        ls = text_lines(rng)
        if ls and rng.random() < 0.7:
            br = rng.choice(["[br]", "[br]", "[BR]", "[Br]"])
            body.append(br.join(ls))
        else:
            body += ls
        body.append(blank(rng))
    return finish(rng, body, nl)


CODES = ["{y:i}", "{y:b}", "{Y:b}", "{Y:u}", "{c:$0000FF}", "{C:$FF0000}", "{f:Arial}", "{F:Arial}", "{s:12}", "{P:10,20}", "{H:Greek}", "{y:b,u}", "{x:|}", "{y:}", "{y:i"]


def mdvd_text(rng):
    parts = []
    for _ in range(rng.randint(1, 5)):
        r = rng.random()
        if r < 0.35:
            parts.append(rng.choice(CODES))
        elif r < 0.5:
            parts.append(rng.choice(["|", " | ", "|"]))
        else:
            parts.append(words(rng, rng.randint(1, 3)))
    return "".join(parts)


def mdvd_doc(rng):
    nl = rng.choice(NLS)
    body = []
    if rng.random() < 0.5:
        body.append(rng.choice(["{1}{1}23.976", "{0}{1}25", "{1}{0} 29.97", "{1}{1}hello", "{0}{0}1000"]))
    if rng.random() < 0.3:
        body.append("{DEFAULT}{C:$FF0000}{F:Arial}")
    f = 1
    for _ in range(rng.randint(0, 8)):
        a = f + rng.randint(0, 50)
        b = a + rng.randint(0, 80)
        pre = "{%d}{%d}" % (a, b)
        if rng.random() < 0.05:
            pre = "{%d}{}" % a
        if rng.random() < 0.03:
            pre = "{\u0661\u0662}{\u0663}"
        if rng.random() < 0.05:
            pre = " " + pre
        body.append(pre + (mdvd_text(rng) if rng.random() < 0.93 else ""))
        f = b
        if rng.random() < 0.05:
            body.append(words(rng, 2))
        if rng.random() < 0.05:
            body.append("")
    return finish(rng, body, nl)


def mutate(rng, data: bytes) -> bytes:
    data = bytearray(data)
    for _ in range(rng.randint(1, 3)):
        if not data:
            break
        i = rng.randrange(len(data))
        r = rng.random()
        if r < 0.4:
            del data[i]
        elif r < 0.7:
            data.insert(i, rng.choice(b"{}|[]:,.\n\r 0"))
        else:
            data[i] = rng.randrange(256)
    return bytes(data)


def main():
    out_path = sys.argv[1]
    rng = random.Random(1105)
    cases = []
    gens = [("sbv", sbv_doc), ("sub", sv2_doc), ("sub", mdvd_doc), ("sub", sbv_doc)]
    for n in range(2600):
        fmt, g = gens[n % len(gens)]
        data = g(rng)
        if rng.random() < 0.15:
            data = mutate(rng, data)
        c = run_case(fmt, data)
        c["name"] = f"{fmt}-{g.__name__}-{n}"
        cases.append(c)
    # a few mixes that sniff differently
    for n in range(60):
        a, b = mdvd_doc(rng), sv2_doc(rng)
        try:
            data = a.decode("utf-8") + b.decode("utf-8")
        except UnicodeDecodeError:
            continue
        c = run_case("sub", data.encode("utf-8"))
        c["name"] = f"mix-{n}"
        cases.append(c)
    with open(out_path, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(cases, fh, separators=(",", ":"))
    errs = sum(1 for c in cases if "error" in c)
    ref = sum(1 for c in cases for b in c.get("builds", {}).values() if isinstance(b, dict))
    print(len(cases), "cases,", errs, "refused files,", ref, "refused builds")


main()
