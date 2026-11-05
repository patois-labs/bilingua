"""Builds tests/data/locos_golden.json: random DTD files and what the LocOS DTD filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (or the error class), and the bytes
LocOS builds with three mappings (all, every third, plain). Deterministic (seed 1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-dtd-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

VALUES = [
    "Save", "Cancel", "&brandShortName; Settings", "Save &amp; close", "Say &quot;hi&quot; to &#169; 100&#037;",
    "Click <html:b>here</html:b> &lt;now&gt;", "&brandShortName;", "  padded  ", "Caf\u00e9", "\u65e5\u672c\u8a9e",
    "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "42", "---", "", "line one\nline two", "&#x41;bc &#65; &#xD800; &#0;",
    "&lt;i>Text&lt;/i>", "a &apos;b&apos; c", "It&apos;s", "&amp;test1;", "<br/>text", "<b>bold <i>x</b>", "50&#037; off",
    "&unknown; and &#99999999999;", "\u00a0nbsp\u00a0", "emoji \U0001f600", "&#128512; smile",
]
NAMES = ["window.title", "save.label", "a", "b_c", "x-y", "dup", "x\u00fcber", "k:1", "e.5"]
COMMENTS = ["<!-- note -->", "<!-- LOCALIZATION NOTE (a): text &x; -->", "<!--multi\n   - line\n   - note-->", "<!---->"]
OTHER = [
    '<!ENTITY % brandDTD SYSTEM "chrome://branding/locale/brand.dtd">', "%brandDTD;", '<!ENTITY ext SYSTEM "x.dtd">',
    '<!ENTITY pub PUBLIC "-//X//EN" "x.dtd">', "<!ELEMENT r (#PCDATA)>", '<!ATTLIST r a CDATA "x>y">',
    '<?xml version="1.0"?>', "<?pi stuff?>", '<!ENTITY % pe "<!ENTITY inner \'Hidden\'>">',
]
BAD = ["stray text", "<!ENTITY", '<!ENTITY a "unterminated>', "<!-- open", '<!ENTITY a "x" junk>', "<!ENTITYa 'x'>",
       "<!ENTITY %a 'x'>", "<!ENTITY a>", "%pe", "<? open", "<!ELEMENT open"]


def make(rng):
    nl = rng.choice(["\n", "\r\n", "\r"])
    lines = []
    for _ in range(rng.randint(0, 20)):
        r = rng.random()
        if r < 0.12:
            lines.append(rng.choice(COMMENTS))
        elif r < 0.2:
            lines.append(rng.choice(OTHER))
        elif r < 0.25:
            lines.append("")
        elif r < 0.2515:
            lines.append(rng.choice(BAD))
        else:
            v = rng.choice(VALUES)
            q = rng.choice(['"', "'"])
            if q in v:
                v = v.replace(q, "&quot;" if q == '"' else "&apos;")
            sp = rng.choice([" ", "  ", "\n    ", "\t"])
            end = rng.choice([">", " >", "\n>"])
            lines.append(f"<!ENTITY{sp}{rng.choice(NAMES)}{sp}{q}{v}{q}{end}")
    text = nl.join(lines) + (nl if rng.random() < 0.7 else "")
    kind = rng.choice(["utf8"] * 12 + ["utf8bom"] * 4 + ["latin1"])
    if kind == "utf8bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "latin1":
        try:
            return text.encode("latin-1")
        except UnicodeEncodeError:
            pass
    return text.encode("utf-8")


def golden(data):
    f = get_filter("dtd")
    rec = {"hex": data.hex()}
    try:
        units = [u.text for u in f.parse(data).units]
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    rec["units"] = units
    maps = {
        "all": {str(i): 'T%d "q" \'a\' 50%% A&B &ref; \u00e9\nline2' % i for i in range(len(units))},
        "third": {str(i): "Zeta %d \u65e5" % i for i in range(0, len(units), 3)},
        "plain": {str(i): "Plain%d" % i for i in range(len(units))},
    }
    rec["builds"] = {}
    for name, m in maps.items():
        try:
            rec["builds"][name] = f.build(data, m).hex()
        except Exception as e:  # noqa: BLE001
            rec["builds"][name] = "error:" + type(e).__name__
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "3c21454e54495459206120227822203e0a00",  # NUL
    "3c21454e54495459206120227822203ee9",  # latin-1 after
    "fffe3c00",  # UTF-16 BOM
    "efbbbf3c21454e5449545920612022ff22203e",  # bad UTF-8 after BOM
    "0102030405060708" * 20 + "e9",
)]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    rng = random.Random(1104)
    cases = []
    for i in range(500):
        cases.append({"name": f"random-{i}", **golden(make(rng))})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h)})
    for path in extra:
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()
