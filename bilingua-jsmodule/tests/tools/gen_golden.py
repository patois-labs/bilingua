"""Builds tests/data/locos_golden.json: random JS/TS locale modules and what the LocOS js filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (text and the placeholder strings of
each unit) or the refusal, the records() view, and the bytes LocOS builds with three mappings: every unit
translated ("all"), every third unit translated ("third"), and every unit's own text plus " Z" ("tok", tokens kept).
Deterministic (seed 1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-js-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Hello, {name}!", "{count} items", "no apples | one apple | {count} apples",
    "See @:nav.home", "Write {'@'} us", "Welcome, {{name}}!", "$t(a.b) now", "Click <1>here</1>", "Saved %s of %1$d",
    "50% off", "{count, plural, =0 {None} one {# item} other {# items}}",
    "{g, select, female {She} male {He} other {They}} replied", "{n, number} units", "Caf\u00e9", "\u65e5\u672c\u8a9e",
    "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "<b>Bold</b> text", "It's \"quoted\"", "back\\slash",
    "tab\there", "line1\nline2", "emoji \U0001f600", "---", "{{count}}", "%s", "", " ", "a ${b} c", "Use `tick`",
    "x", "100%", "{a}{b}", "'{'quoted'}'", "{n, plural, offset:1 =0 {zero} other {# more}}", "\u2028sep",
    "%(name)s left", "\u27e6x1\u27e7 literal", "<br/>", "{ spaced }", "@.lower:key", "Price: ${price}",
]
KEYS = ["title", "nav", "home", "about", "k", "a-b", "x.y", "\u00fcber", "1", "class", "default", "if", "ok", "msg"]
COMMENTS = ["// note", "/* block */", "/** doc\n * more */", "// \u00e9\u65e5"]


def lit(rng, v):
    q = rng.choice(["'", '"', "`", "'", '"'])
    out = []
    for i, ch in enumerate(v):
        if ch == "\\":
            out.append("\\\\")
        elif ch == q:
            out.append("\\" + ch)
        elif q == "`" and ch == "$" and v[i + 1:i + 2] == "{":
            out.append("\\$")
        elif ch == "\n":
            out.append("\n" if q == "`" and rng.random() < 0.5 else "\\n")
        elif ch == "\t":
            out.append(rng.choice(["\\t", "\t"]) if q != "`" else "\\t")
        elif ch in "\u2028\u2029":
            out.append("\\u%04x" % ord(ch))
        elif ord(ch) < 0x20:
            out.append("\\x%02x" % ord(ch))
        elif ord(ch) > 0x7F and rng.random() < 0.15:
            cp = ord(ch)
            if cp > 0xFFFF:
                out.append("\\u{%X}" % cp if rng.random() < 0.5 else "\\u%04X\\u%04X" % (
                    0xD800 + ((cp - 0x10000) >> 10), 0xDC00 + ((cp - 0x10000) & 0x3FF)))
            else:
                out.append("\\u%04x" % cp)
        elif ch == "x" and rng.random() < 0.05:
            out.append("\\x78")
        else:
            out.append(ch)
    s = "".join(out)
    if q != "`" and "\n" in s:  # a raw newline cannot sit in a quoted string
        s = s.replace("\n", "\\n")
    return q + s + q


def value(rng, depth, nl):
    r = rng.random()
    if depth < 3 and r < 0.2:
        return obj(rng, depth + 1, nl)
    if r < 0.28:
        n = rng.randint(0, 4)
        return "[" + ", ".join(value(rng, depth + 1, nl) for _ in range(n)) + ("," if n and rng.random() < 0.3 else "") + "]"
    if r < 0.33:
        return rng.choice(["3", "-1.5e3", "0x1F", "true", "false", "null", "1_000", ".5"])
    if r < 0.4:
        return lit(rng, rng.choice(WORDS)) + " + " + lit(rng, rng.choice(WORDS))
    return lit(rng, rng.choice(WORDS))


def key(rng):
    k = rng.choice(KEYS)
    if k.isidentifier() and rng.random() < 0.6:
        return k
    q = rng.choice(["'", '"'])
    return q + k + q


def obj(rng, depth, nl):
    n = rng.randint(0, 5)
    pad = "  " * depth
    parts = []
    for _ in range(n):
        c = (rng.choice(COMMENTS) + nl + pad) if rng.random() < 0.12 else ""
        k = key(rng)
        parts.append(f"{c}{k}: {value(rng, depth, nl)}")
    if not parts:
        return "{}"
    sep = "," + nl + pad
    trail = "," if rng.random() < 0.5 else ""
    return "{" + nl + pad + sep.join(parts) + trail + nl + ("  " * (depth - 1)) + "}"


def make(rng):
    nl = rng.choice(["\n", "\n", "\r\n", "\r"])
    shape = rng.choice(["esm", "esm", "cjs", "const", "named", "ts", "ts2", "exports", "paren", "bad"])
    body = obj(rng, 1, nl)
    semi = rng.choice([";", ""])
    head = ""
    if rng.random() < 0.3:
        head = rng.choice(COMMENTS) + nl
    if rng.random() < 0.1:
        head = "#!/usr/bin/env node" + nl + head
    if shape == "esm":
        src = f"export default {body}{semi}{nl}"
    elif shape == "cjs":
        src = f"'use strict';{nl}module.exports = {body}{semi}{nl}"
    elif shape == "const":
        src = f"const messages = {body}{semi}{nl}export default messages{semi}{nl}"
    elif shape == "named":
        src = f"export const en = {body}{semi}{nl}export const de = {obj(rng, 1, nl)}{semi}{nl}"
    elif shape == "ts":
        src = (f"import type {{ M }} from './t'{nl}type X = typeof en{nl}export const en: M = {body} as const{semi}{nl}"
               f"export default en{nl}")
    elif shape == "ts2":
        src = f"interface E {{ a?: string }}{nl}export default {body} satisfies Record<string, unknown>{semi}{nl}"
    elif shape == "exports":
        src = f"exports.home = {body}{semi}{nl}exports.other = {obj(rng, 1, nl)}{semi}{nl}"
    elif shape == "paren":
        src = f"export default ({body}){semi}{nl}"
    else:  # not a static module: inject something LocOS must refuse (or accept, whichever it decides)
        bad = rng.choice([
            "foo()", "...base", "[k]: 'A'", "a: t('x')", "a() {}", "a: `Hi ${x}`", "a: x", "a: () => 1", "a: 'A'.b",
            "a: 'A", "a: '\\101'", "@", "a: /re/", "a: 1 +",
        ])
        body = body[:-1] + ("," if len(body) > 2 else "") + bad + "}"
        src = f"export default {body}{semi}{nl}"
    return head + src


def main():
    out_path = sys.argv[1]
    files = [(os.path.basename(p), open(p, "rb").read()) for p in sys.argv[2:]]
    rng = random.Random(1104)
    for i in range(450):
        files.append((f"random-{i:03d}", make(rng).encode("utf-8")))
    f = get_filter("js")
    cases = []
    for name, data in files:
        case = {"name": name, "hex": data.hex()}
        try:
            pr = f.parse(data)
            texts = [u.text for u in pr.units]
            case["units"] = texts
            case["points"] = [[v["point"] for _, v in sorted(u.inline_map.items(), key=lambda kv: int(kv[0]))] for u in pr.units]
            case["records"] = [[r["key"], r["value"]] for r in f.records(data)]
            case["builds"] = {
                "all": f.build(data, {str(i): 'T%d "q" \u00e9\nline2' % i for i in range(len(texts))}).hex(),
                "third": f.build(data, {str(i): "Zeta %d \u65e5" % i for i in range(0, len(texts), 3)}).hex(),
                "tok": f.build(data, {str(i): t + " Z" for i, t in enumerate(texts)}).hex(),
            }
        except Exception as e:  # noqa: BLE001
            case["error"] = type(e).__name__
        cases.append(case)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    ok = sum(1 for c in cases if "error" not in c)
    print(f"{len(cases)} cases, {ok} parsed, {len(cases) - ok} refused")


main()
