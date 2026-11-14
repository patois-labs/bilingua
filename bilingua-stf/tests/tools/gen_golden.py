"""Builds tests/data/locos_golden.json: random STF files plus the corpus, and what the LocOS STF filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the unit texts and styles LocOS parses (or the error class),
its records, and the bytes LocOS builds with several mappings, with and without a target locale. Deterministic.
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-stf-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402
from app.filters.stf_filter import records  # noqa: E402

T = "\t"
LABELS = [
    "Save", "Cancel", "Welcome back, {0}!", "See {!$User.FirstName} now", "Line one\\nLine two", "Tab\\there",
    "Back\\\\slash", "{0}", "\\n", "  padded  ", "Caf\u00e9 cr\u00e8me", "\u65e5\u672c\u8a9e\u306e\u30c6\u30ad\u30b9\u30c8",
    "\u0645\u0631\u062d\u0628\u0627 {1,number}", "\ud55c\uad6d\uc5b4", "{12,date,short} items", "{1234} four digits",
    "{!} empty merge", "{!a}{!b}", "x\x85y", "emoji \U0001f600", "100%", "a\\x unknown escape", "{ 0}", "{0,}", "",
    "\u00a0", "\u0661\u0662 {\u0661\u0662}", "\u2460 circled",
]
KEYS = [
    "CustomLabel.Welcome", "CustomField.Account.Region.FieldLabel", "CustomField.Account.Region.HelpText",
    "CustomField.A.B.RelationshipLabel", "PicklistValue.A.B.C", "ValidationFormula.A.R.ErrorMessage",
    "RecordType.A.B.Name", "ButtonOrLink.Account.Call", "k", "Key With Space", "\u00fcber.key", "CustomField..FieldLabel",
]
HEADERS = [
    "Language code: de", "Language code: en_US", "Language code:\tfr", "language CODE : pt_BR", "Language code:",
    "Type: Bilingual", "Type: Source", "Type: Outdated and untranslated", "TYPE:Source", "Translation type: Metadata",
    "Translation type : Metadata  ", "Language code: \u00e9s", "Translat\u0131on type: X", "Translation type: Y\u017f",
]
SECTIONS = [
    "------------------TRANSLATED-------------------", "------------------UNTRANSLATED-----------------",
    "---------OUTDATED AND UNTRANSLATED---------", "--- Foo  Bar ---", "-- two --", "----x----", "---  Translated  ---  ",
    "-----not a marker", "------------------Translated------------------", "---\tTab---",
]
COLUMNS = [
    f"# KEY{T}LABEL{T}TRANSLATION{T}OUT OF DATE", f"# KEY{T}LABEL", f"#KEY{T}TRANSLATION{T}LABEL", f"# key{T}label{T}translation",
    f"#  KEY{T} Label {T}Translation", f"# KEY{T}TRANSLATION", "# KEY LABEL", f"# KEY{T}LABEL{T}Translat\u0131on",
]
COMMENTS = ["# a comment", "#", "# CustomLabel", f"#CustomLabel.B{T}Commented", "   ", ""]


def row(rng):
    k = rng.choice(KEYS)
    n = rng.choice([1, 1, 1, 2, 2, 3, 3, 4, 5])
    cells = [k] + [rng.choice(LABELS + ["*", "-", "Verkaufsregion"]) for _ in range(n)]
    if rng.random() < 0.05:
        cells[0] = rng.choice(["", "  "])
    return T.join(cells)


def make(rng):
    nl = rng.choice(["\n", "\r\n", "\r", "\n"])
    lines = []
    for _ in range(rng.randint(0, 30)):
        r = rng.random()
        if r < 0.15:
            lines.append(rng.choice(HEADERS))
        elif r < 0.25:
            lines.append(rng.choice(SECTIONS))
        elif r < 0.33:
            lines.append(rng.choice(COLUMNS))
        elif r < 0.4:
            lines.append(rng.choice(COMMENTS))
        elif r < 0.44:
            lines.append("stray line without tab")
        else:
            lines.append(row(rng))
    text = nl.join(lines) + rng.choice(["", nl])
    mode = rng.random()
    if mode < 0.55:
        return text.encode("utf-8")
    if mode < 0.65:
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if mode < 0.72:
        return text.encode("utf-16")
    if mode < 0.78:
        return (b"\xff\xfe" + text.encode("utf-16-le")) if rng.random() < 0.5 else (b"\xfe\xff" + text.encode("utf-16-be"))
    if mode < 0.9:
        try:
            return text.encode("cp1252")
        except UnicodeEncodeError:
            return text.encode("utf-8")
    if mode < 0.93:
        return text.encode("utf-8") + bytes([rng.choice([0, 0x81, 0xFF, 0xC3])])
    return text.encode("utf-8", "replace").replace(b"\n", rng.choice([b"\n", b"\x00\n"]), 1)


def mappings(units):
    def all_(i, t):
        return "[de] " + t

    def plain(i, t):
        return "Plain%d" % i

    def esc(i, t):
        return "Z\tq%d\r\nline\rend" % i + t

    def third(i, t):
        return "Zeta \u65e5 " + t

    return {
        "all": {str(i): all_(i, t) for i, t in enumerate(units)},
        "plain": {str(i): plain(i, t) for i, t in enumerate(units)},
        "esc": {str(i): esc(i, t) for i, t in enumerate(units)},
        "third": {str(i): third(i, t) for i, t in enumerate(units) if i % 3 == 0},
    }


LOCALES = {"loc": "pt-BR", "zh": "zh-Hant-TW", "he": "he"}
COMBO = {"all": {"loc": "pt-BR"}, "esc": {"zh": "zh-Hant-TW"}, "plain": {"he": "he"}}


def case(name, data, f):
    c = {"name": name, "hex": data.hex()}
    try:
        pr = f.parse(data)
    except Exception as e:  # noqa: BLE001
        c["error"] = type(e).__name__
        return c
    units = [u.text for u in pr.units]
    c["units"] = units
    c["styles"] = [u.style for u in pr.units]
    c["records"] = records(data)
    builds = {}
    for label, m in mappings(units).items():
        builds[label] = f.build(data, m).hex()
        for lname, loc in COMBO.get(label, {}).items():
            builds[label + "@" + lname] = f.build_localized(data, m, loc).hex()
    builds["none@loc"] = f.build_localized(data, {}, "pt-BR").hex()
    builds["none"] = f.build(data, {}).hex()
    c["builds"] = builds
    return c


def main():
    out, files = sys.argv[1], sys.argv[2:]
    f = get_filter("stf")
    rng = random.Random(1105)
    cases = []
    for path in files:
        with open(path, "rb") as fh:
            cases.append(case("corpus/" + os.path.basename(path), fh.read(), f))
    for n in range(330):
        cases.append(case("random-%d" % n, make(rng), f))
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    refused = sum(1 for c in cases if "error" in c)
    print(f"{len(cases)} cases, {refused} refused by LocOS")


if __name__ == "__main__":
    main()
