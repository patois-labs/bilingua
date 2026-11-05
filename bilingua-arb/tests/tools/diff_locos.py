"""Differential test of bilingua-arb against the LocOS ARB filter on random messages.

    <local-tms venv python> -I diff_locos.py [SEED] [MESSAGES]

Builds one ARB file of random ICU-like messages (valid, nested, broken, quoted), then for several target languages
compares (1) the unit list and (2) the bytes of a merge in which every unit is translated to "T:" + text, between
the LocOS filter (read only) and bilingua-arb (run through tests/diff_locos.rs). Prints the differences.
"""

import json
import os
import random
import subprocess
import sys
import tempfile

sys.stdout.reconfigure(encoding="utf-8")
LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-diff-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
from app import config  # noqa: E402
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10**6
HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(os.path.dirname(HERE))

WORDS = ["item", "files", "x", "Hello", "world", "caf\u00e9", "\u30c6\u30b9\u30c8", "\u0645\u0631\u062d\u0628\u0627", "42", "a b", " ", ".", ","]
SIMPLE = ["{name}", "{n, number}", "{d, date, short}", "{x,time}", "{0}", "{ spaced }"]
QUOTES = ["'", "''", "'{'", "'}'", "'#'", "it's", "'{x}'"]


def rnd_message(r: random.Random, depth: int = 0, in_plural: bool = False) -> str:
    out = []
    for _ in range(r.randint(0, 4)):
        c = r.random()
        if c < 0.45:
            out.append(r.choice(WORDS))
        elif c < 0.60:
            out.append(r.choice(SIMPLE))
        elif c < 0.68:
            out.append(r.choice(QUOTES))
        elif c < 0.74 and in_plural:
            out.append("#")
        elif c < 0.95 and depth < 3:
            out.append(rnd_complex(r, depth))
        elif c < 0.98:
            out.append(r.choice(["{", "}", "{{", "}}", "{n, plural, ", "one{", "{a, select}", "{,}"]))  # damage
        else:
            out.append(r.choice(["\n", "\t", "\\", '"']))
    return "".join(out)


def rnd_complex(r: random.Random, depth: int) -> str:
    kind = r.choice(["plural", "plural", "select", "selectordinal"])
    arg = r.choice(["count", "n", "gender", "who"])
    if kind == "select":
        sels = r.sample(["male", "female", "other", "x"], r.randint(1, 3)) + ["other"]
    else:
        sels = r.sample(["=0", "=1", "zero", "one", "two", "few", "many", "=5"], r.randint(0, 4)) + ["other"]
    head = "{" + arg + r.choice([", ", " , "]) + kind + r.choice([",", ", ", " ,"]) + r.choice(["", "", " offset:1 "] if kind == "plural" else [""])
    body = ""
    for s in dict.fromkeys(sels):
        body += s + r.choice(["", " ", ""]) + "{" + rnd_message(r, depth + 1, kind != "select") + "}" + r.choice(["", " ", "  "])
    return head + body + "}"


def build_file(seed: int, n: int) -> str:
    r = random.Random(seed)
    obj = {}
    if r.random() < 0.7:
        obj["@@locale"] = "en"
    for i in range(n):
        obj[f"m{i}"] = rnd_message(r)
        if r.random() < 0.3:
            obj[f"@m{i}"] = {"description": "d"}
    # suffix style plural groups
    for stem in ("itemCount", "dayCount"):
        obj[stem + "One"] = "{n} item"
        if r.random() < 0.5:
            obj[stem + "Few"] = "{n} few"
        obj[stem + "Other"] = "{n} items"
    indent = r.choice([2, 4, None])
    return json.dumps(obj, ensure_ascii=r.random() < 0.3, indent=indent) + "\n"


def main() -> None:
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 400
    text = build_file(seed, n)
    data = text.encode("utf-8")
    tmp = tempfile.mkdtemp(prefix="arb-diff-")
    src = os.path.join(tmp, "in.arb")
    with open(src, "wb") as fh:
        fh.write(data)
    filt = get_filter("arb")
    lo_units = [u.text for u in filt.parse(data).units]
    bad = 0
    for target in ["ru", "ja", "ar", "en", "pl-PL", "pt-BR", "xx"]:
        env = dict(os.environ, ARB_DIFF_IN=src, ARB_DIFF_UNITS=os.path.join(tmp, "units.json"),
                   ARB_DIFF_TARGET=target, ARB_DIFF_MERGED=os.path.join(tmp, "merged.arb"))
        p = subprocess.run(["cargo", "test", "-p", "bilingua-arb", "--offline", "--test", "diff_locos", "--", "--ignored"],
                           cwd=CRATE, env=env, capture_output=True)
        if p.returncode != 0:
            print("cargo failed:", p.stderr.decode("utf-8", "replace")[-600:])
            return
        with open(env["ARB_DIFF_UNITS"], encoding="utf-8") as fh:
            my_units = json.load(fh)
        if my_units != lo_units:
            bad += 1
            print(f"[{target}] UNIT LISTS DIFFER: locos {len(lo_units)}, bilingua {len(my_units)}")
            for i, (a, b) in enumerate(zip(lo_units, my_units)):
                if a != b:
                    print("   first difference at", i, repr(a), "!=", repr(b))
                    break
            continue
        texts = {str(i): "T:" + t for i, t in enumerate(lo_units)}
        want = filt.build_localized(data, texts, target)
        with open(env["ARB_DIFF_MERGED"], "rb") as fh:
            got = fh.read()
        if want != got:
            bad += 1
            w, g = want.decode("utf-8"), got.decode("utf-8")
            at = next((i for i, (x, y) in enumerate(zip(w, g)) if x != y), min(len(w), len(g)))
            print(f"[{target}] MERGED BYTES DIFFER ({len(w)} vs {len(g)}) at {at}")
            print("   locos:   ", repr(w[max(0, at - 90):at + 90]))
            print("   bilingua:", repr(g[max(0, at - 90):at + 90]))
    print(f"seed {seed}: {len(lo_units)} units, {bad} differing targets of 7")


if __name__ == "__main__":
    main()
