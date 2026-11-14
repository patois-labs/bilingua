"""Builds tests/data/locos_golden.json: random PHP language files and what the LocOS php filter makes of them.

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
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-php-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.php_array_filter import PhpArrayFilter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Hello, :name!", "Hello :NAME, you have :count items", "{0} none|[1,19] some|[20,*] many",
    "apple|apples", "{1} one apple|[2,*] :count apples", "Name: %1$s (%2$d)", "Invalid: %s", "50%% off", "100%", "Price %05.2f",
    "<b>Bold</b> text", "<a href=\"x\">link</a>", "{name} says {0, number}", "{count, plural, one {# item} other {# items}}",
    "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "It's \"quoted\"", "back\\slash",
    "tab\there", "line1\nline2", "emoji \U0001f600", "---", "...", "", " ", "x", "a:b", "http://x.y/z", "time 10:30", "::x",
    "\u27e6x1\u27e7 literal", "a | b", "| leading", " {0} spaced", "\t[1,5] tabbed", "[a,b]", "{ x }", "$5 only", "dollar $ sign",
    "Use `tick`", "trailing :", "50:", "ratio :1", "email@x.com", "<>", "a < b > c", "100% sure %", "%", "%s%s", "%-5s|%5s",
]
KEYS = ["title", "nav", "home", "about", "k", "a-b", "x.y", "\u00fcber", "1", "class", "default", "if", "ok", "msg", "-3", "007"]
COMMENTS = ["// note", "# hash note", "/* block */", "/** doc\n * more */", "// \u00e9\u65e5", "// ends with ? mark?"]


def sq(v):
    return "'" + v.replace("\\", "\\\\").replace("'", "\\'") + "'"


def dq(rng, v):
    out = []
    for ch in v:
        if ch in '\\"$':
            out.append("\\" + ch)
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append(rng.choice(["\\t", "\t"]))
        elif ord(ch) < 0x20:
            out.append("\\x%02x" % ord(ch))
        elif ord(ch) > 0x7F and rng.random() < 0.15:
            cp = ord(ch)
            out.append("\\u{%X}" % cp)
        elif ch == "x" and rng.random() < 0.05:
            out.append("\\x78")
        elif ch == "a" and rng.random() < 0.03:
            out.append("\\141")
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def heredoc(rng, v, nl):
    label = rng.choice(["EOT", "TXT", "L\u00e4bel", "_x1"])
    if rng.random() < 0.5:
        body = v.replace("\\", "\\\\").replace("$", "\\$") if "\n" not in v else v.replace("\\", "\\\\").replace("$", "\\$")
        ind = rng.choice(["", "    "])
        lines = body.split("\n")
        body = nl.join(ind + ln for ln in lines)
        return f"<<<{label}{nl}{body}{nl}{ind}{label}"
    ind = rng.choice(["", "  "])
    lines = v.split("\n")
    body = nl.join(ind + ln for ln in lines)
    return f"<<<'{label}'{nl}{body}{nl}{ind}{label}"


def lit(rng, v, nl):
    r = rng.random()
    if r < 0.45:
        return sq(v)
    if r < 0.85:
        return dq(rng, v)
    return heredoc(rng, v, nl)


def value(rng, depth, nl):
    r = rng.random()
    if depth < 3 and r < 0.2:
        return arr(rng, depth + 1, nl)
    if r < 0.33:
        return rng.choice(["3", "-1.5e3", "0x1F", "true", "FALSE", "Null", "1_000", "+4"])
    if r < 0.42:
        pieces = [lit(rng, rng.choice(WORDS), nl) for _ in range(rng.randint(2, 3))]
        return (" . " if rng.random() < 0.7 else "\n   . ").join(pieces)
    return lit(rng, rng.choice(WORDS), nl)


def key(rng):
    k = rng.choice(KEYS)
    r = rng.random()
    if k.lstrip("-").isdigit() and r < 0.5:
        return k
    return rng.choice([sq(k), dq(rng, k)])


def arr(rng, depth, nl):
    n = rng.randint(0, 5)
    pad = "    " * depth
    parts = []
    old = rng.random() < 0.25
    for _ in range(n):
        c = (rng.choice(COMMENTS) + nl + pad) if rng.random() < 0.12 else ""
        if rng.random() < 0.2:
            parts.append(f"{c}{value(rng, depth, nl)}")
        else:
            parts.append(f"{c}{key(rng)} => {value(rng, depth, nl)}")
    if not parts:
        return "array()" if old else "[]"
    sep = "," + nl + pad
    trail = "," if rng.random() < 0.6 else ""
    inner = sep.join(parts) + trail
    if old:
        return "array(" + nl + pad + inner + nl + ("    " * (depth - 1)) + ")"
    return "[" + nl + pad + inner + nl + ("    " * (depth - 1)) + "]"


def make(rng):
    nl = rng.choice(["\n", "\n", "\r\n"])
    shape = rng.choice(["ret", "ret", "ret", "ci3", "ci3b", "decl", "closed", "bad", "bad"])
    head = "<?php" + rng.choice([nl, " ", "\t"]) if rng.random() < 0.9 else "<?PHP" + nl
    if rng.random() < 0.15:
        head = nl + head
    if rng.random() < 0.3:
        head += rng.choice(COMMENTS) + nl
    semi = ";"
    if shape == "ret":
        src = f"return {arr(rng, 1, nl)}{semi}{nl}"
    elif shape == "decl":
        src = f"declare(strict_types=1);{nl}return {arr(rng, 1, nl)}{semi}{nl}"
    elif shape == "closed":
        src = f"return {arr(rng, 1, nl)}{semi}{nl}?>{rng.choice(['', nl, nl + nl, '  '])}"
    elif shape == "ci3":
        lines = []
        if rng.random() < 0.7:
            lines.append("defined('BASEPATH') OR exit('No direct script access allowed');" + nl)
        for _ in range(rng.randint(1, 8)):
            k = key(rng)
            sub = f"[{key(rng)}]" if rng.random() < 0.3 else ""
            lines.append(f"$lang[{k}]{sub} = {value(rng, 1, nl)};")
        src = nl.join(lines) + nl + rng.choice(["", "?>" + nl])
    elif shape == "ci3b":
        src = f"$lang = {arr(rng, 1, nl)};{nl}$other['a'][] = {lit(rng, 'Appended', nl)};{nl}"
    else:
        bad = rng.choice([
            "__('x')", "env('A')", "SOME_CONST", "$v", "'a' . $b", "'a' . 5", "\"Hi $name\"", "\"Hi {$x->y}\"", "\"Hi ${n}\"", "fn() => 1",
            "'unterminated", "\"\\xff\"", "<<<EOT\nopen", "new Foo()", "[1, 2", "'a' 'b'", "-x", "!true", "1 + 2", "[,]", "array(1,,2)",
            "\"\\u{110000}\"", "\"\\u{D800}\"", "@'a'", "'a' ?? 'b'", "static::A", "\"x\" . ", "<<<EOT\n  a\n b\n  EOT",
        ])
        body = arr(rng, 1, nl)
        body = body[:-1] + (("," if len(body) > 2 else "") + bad) + body[-1]
        src = f"return {body}{semi}{nl}"
    return head + src


def main():
    out_path = sys.argv[1]
    files = [(os.path.basename(p), open(p, "rb").read()) for p in sys.argv[2:]]
    rng = random.Random(1104)
    for i in range(500):
        files.append((f"random-{i:03d}", make(rng).encode("utf-8")))
    f = PhpArrayFilter()
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
                "all": f.build(data, {str(i): 'T%d "q" \u00e9\nline2 $x \\' % i for i in range(len(texts))}).hex(),
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
