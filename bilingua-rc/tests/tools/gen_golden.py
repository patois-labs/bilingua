"""Builds tests/data/locos_golden.json: random resource scripts and what the LocOS rc filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the unit texts and styles LocOS parses (or the error class),
and the bytes LocOS builds (length:FNV-1a-64 digests) with several mappings: every unit translated, every third, plain text, the unit's own
text plus a mark (codes kept), and a localized build (every unit translated, target de-DE / ja-JP / ar-SA /
zh-Hant / es-419). Deterministic (seed 1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-rc-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

STR = [
    "Save", "Cancel", "&File", "&Open...\\tCtrl+O", "Save && E&xit", "Hello, %s! You have %d new messages.",
    'Say ""hi"" to C:\\\\Temp\\nthen leave', "Copying %1 to %2!s!...", "%d%%", "", "OK", "&Yes", "No&",
    "Caf\u00e9 &Menu", "\u65e5\u672c\u8a9e&(F)", "\u0645\u0631\u062d\u0628\u0627", "Don\\'t", "tab\\there",
    "octal \\101\\102", "bad \\q escape", "5\\x00BC-Inch", "mixed \\x41 \\xZZ", "line \\\n   continued",
    "A & B", "&&", "&", "Name:", "Size (&S):", "100%", "%-5.2f units", "%I64d bytes", "{0} items", "\\\\server\\\\share",
    "Version %1!d!.%2!d!", "  padded  ", "\u00a0nbsp", "end\\r\\n",
]
KEYS = ["IDS_A", "IDS_B", "1", "2", "0x10", "IDS_LONG_NAME_X", "IDS_C"]
VKEYS = ["FileDescription", "ProductName", "Comments", "LegalCopyright", "LegalTrademarks", "CompanyName", "FileVersion", "InternalName"]
LANGS = ["LANGUAGE LANG_ENGLISH, SUBLANG_ENGLISH_US", "LANGUAGE 0x09, 0x01", "LANGUAGE 7, 1", "LANGUAGE LANG_FRENCH,SUBLANG_DEFAULT", "LANGUAGE 0x11, 0x01"]
CTRL = ["LTEXT", "RTEXT", "CTEXT", "PUSHBUTTON", "DEFPUSHBUTTON", "CHECKBOX", "AUTOCHECKBOX", "RADIOBUTTON", "GROUPBOX", "STATE3"]


def lit(rng, wide_ok=True):
    s = rng.choice(STR)
    w = "L" if wide_ok and rng.random() < 0.15 else ""
    return f'{w}"{s}"'


def stringtable(rng):
    out = ["STRINGTABLE"]
    if rng.random() < 0.2:
        out.append(rng.choice(LANGS))
    out.append(rng.choice(["BEGIN", "{"]))
    for _ in range(rng.randint(0, 6)):
        out.append("    " + rng.choice(KEYS) + rng.choice(["  ", ",", ", "]) + lit(rng))
    out.append(rng.choice(["END", "}"]))
    return out


def menu(rng, depth=0):
    kind = rng.choice(["MENU", "MENUEX"])
    out = [f"IDR_M{rng.randint(1, 99)} {kind}", "BEGIN"]

    def items(d):
        for _ in range(rng.randint(0, 4)):
            r = rng.random()
            if r < 0.25 and d < 2:
                out.append(f'    POPUP {lit(rng, False)}' + (", 0, MFT_STRING, MFS_ENABLED" if kind == "MENUEX" else ""))
                out.append("    BEGIN")
                items(d + 1)
                out.append("    END")
            elif r < 0.35:
                out.append("    MENUITEM SEPARATOR" if kind == "MENU" else '    MENUITEM "", 0, MFT_SEPARATOR')
            else:
                out.append(f"    MENUITEM {lit(rng, False)}, ID_X{rng.randint(1, 9)}" + (", MFT_STRING, MFS_ENABLED" if kind == "MENUEX" else ""))

    items(0)
    out.append("END")
    return out


def dialog(rng):
    out = [f"IDD_D{rng.randint(1, 9)} {rng.choice(['DIALOG', 'DIALOGEX'])} 0, 0, 170, 62"]
    if rng.random() < 0.3:
        out.append(rng.choice(LANGS))
    out.append("STYLE DS_SETFONT | WS_POPUP")
    if rng.random() < 0.8:
        out.append(f"CAPTION {lit(rng, False)}")
    out.append('FONT 8, "MS Shell Dlg", 0, 0, 0x1')
    out.append("BEGIN")
    for _ in range(rng.randint(0, 6)):
        r = rng.random()
        if r < 0.5:
            out.append(f"    {rng.choice(CTRL)}   {lit(rng, False)}, IDC_X, 4, 4, 20, 20" + rng.choice(["", ", WS_GROUP", ", SS_NOPREFIX"]))
        elif r < 0.65:
            style = rng.choice(["BS_AUTOCHECKBOX | WS_TABSTOP", "SS_BITMAP", "SS_ICON", "SS_OWNERDRAW", "WS_GROUP"])
            out.append(f'    CONTROL {lit(rng, False)}, IDC_C, "Static", {style},')
            out.append("                    42, 26, 100, 10")
        elif r < 0.8:
            out.append("    EDITTEXT        IDC_NAME, 42, 36, 80, 12")
        elif r < 0.9:
            out.append("    ICON            IDI_APP, IDC_STATIC, 14, 14, 20, 20")
        else:
            out.append('    COMBOBOX        IDC_COMBO, 7, 7, 100, 30, CBS_DROPDOWN')
    out.append("END")
    return out


def version(rng):
    block = rng.choice(["040904b0", "040904B0", "041104b0", '0c0904e4'])
    out = ["VS_VERSION_INFO VERSIONINFO", " FILEVERSION 1,0,0,1", "BEGIN", '    BLOCK "StringFileInfo"', "    BEGIN", f'        BLOCK "{block}"', "        BEGIN"]
    for _ in range(rng.randint(0, 6)):
        out.append(f'            VALUE "{rng.choice(VKEYS)}", {lit(rng, False)}')
    out += ["        END", "    END", '    BLOCK "VarFileInfo"', "    BEGIN"]
    out.append(rng.choice(['        VALUE "Translation", 0x409, 1200', '        VALUE "Translation", 1033, 1200', '        VALUE "Translation", 0x411, 932']))
    out += ["    END", "END"]
    return out


def other(rng):
    return rng.choice([
        ['IDI_APP                 ICON                    "res\\\\app.ico"'],
        ['IDB_LOGO BITMAP "res\\\\logo.bmp"'],
        ["IDR_ACCEL ACCELERATORS", "BEGIN", '    "O",  ID_FILE_OPEN, VIRTKEY, CONTROL', "END"],
        ["3 TEXTINCLUDE", "BEGIN", '    "#include ""winres.h""\\r\\n"', '    "\\0"', "END"],
        ["IDR_DATA RCDATA", "BEGIN", '    "Some text",', "    0x1234, 5", "END"],
        ["IDR_UNK MYTYPE", "BEGIN", '    "not text"', "END"],
        ['1 24 "manifest.xml"'],
        ["VERSION 1", "CHARACTERISTICS 2"],
        ['rcinclude other.rc'],
        ['// a "comment" with a string'],
        ['/* block "comment" */'],
        ['#define APP_NAME "My App"'],
        ["#if 0", 'STRINGTABLE BEGIN 99 "in if zero" END', "#endif"],
        ["#ifdef APSTUDIO_INVOKED", "#endif    // APSTUDIO_INVOKED"],
        ['#include "resource.h"'],
        ["#pragma code_page(DEFAULT)"],
        [rng.choice(LANGS)],
        [""],
    ])


def make(rng):
    nl = rng.choice(["\n", "\r\n", "\r\n", "\n", "\r"])
    kind = rng.choice(["utf8", "utf8", "utf8bom", "utf16le", "utf16be", "utf16nobom", "cp1252", "cp1252", "cp932", "cp1251", "cp1250", "cp936", "cp949", "utf8pragma", "koi8", "iso2"])
    lines = []
    if kind in ("cp1252", "cp1252") and rng.random() < 0.8:
        lines.append("#pragma code_page(1252)")
    codec = {"cp932": ("932", "cp932"), "cp1251": ("1251", "cp1251"), "cp1250": ("1250", "cp1250"), "cp936": ("936", "gbk"), "cp949": ("949", "cp949"), "utf8pragma": ("65001", "utf-8"), "koi8": ("20866", "koi8_r"), "iso2": ("28592", "iso8859-2")}
    if kind in codec:
        lines.append(f"#pragma code_page({codec[kind][0]})")
    for _ in range(rng.randint(0, 8)):
        r = rng.random()
        if r < 0.3:
            lines += stringtable(rng)
        elif r < 0.45:
            lines += menu(rng)
        elif r < 0.65:
            lines += dialog(rng)
        elif r < 0.75:
            lines += version(rng)
        else:
            lines += other(rng)
        lines.append("")
    text = nl.join(lines) + (nl if rng.random() < 0.8 else "")
    if rng.random() < 0.35:  # structural mutation: keeps the parity honest on broken scripts
        chars = list(text)
        for _ in range(rng.randint(1, 4)):
            if not chars:
                break
            p = rng.randrange(len(chars))
            op = rng.choice(["del", "ins", "dup"])
            if op == "del":
                del chars[p : p + rng.randint(1, 6)]
            elif op == "ins":
                chars[p:p] = list(rng.choice(['"', "\\", "BEGIN", "END", "{", "}", ",", "\n", "#", "/*", "//", "L", "STRINGTABLE", "LANGUAGE 9, 1"]))
            else:
                chars[p:p] = chars[p : p + rng.randint(1, 30)]
        text = "".join(chars)
    if kind == "utf8" or kind == "utf8pragma":
        return text.encode("utf-8")
    if kind == "utf8bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "utf16le":
        return b"\xff\xfe" + text.encode("utf-16-le", "surrogatepass")
    if kind == "utf16be":
        return b"\xfe\xff" + text.encode("utf-16-be", "surrogatepass")
    if kind == "utf16nobom":
        return text.encode("utf-16-le", "surrogatepass")
    py = codec.get(kind, ("", "cp1252"))[1]
    try:
        return text.encode(py)
    except UnicodeEncodeError:
        return text.encode("utf-8")


MAPS = {
    "all": lambda i, t: 'T%d "q" \u00e9\nline2\t&x' % i,
    "third": lambda i, t: "Zeta %d \u65e5" % i if i % 3 == 0 else None,
    "plain": lambda i, t: "Plain%d" % i,
    "echo": lambda i, t: t + "!",
    "acc": lambda i, t: ["Datei", "&Info &\u00fcber", "\u30d5\u30a1\u30a4\u30eb", "Open...", "X"][i % 5],
}
LOCALES = ["de-DE", "ja-JP", "ar-SA", "zh-Hant", "es-419", "ko", "pt_BR", "ru"]


def digest(b):
    """length and FNV-1a 64 of the bytes (the Rust test computes the same)."""
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "%d:%016x" % (len(b), h)


def golden(data):
    f = get_filter("rc")
    rec = {"hex": data.hex()}
    try:
        res = f.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    units = [u.text for u in res.units]
    rec["units"] = units
    rec["styles"] = [u.style for u in res.units]
    rec["enc"] = res.skeleton.get("source_encoding", "utf-8")
    builds = {}
    for name, fn in MAPS.items():
        m = {}
        for i, t in enumerate(units):
            v = fn(i, t)
            if v is not None:
                m[str(i)] = v
        builds[name] = digest(f.build(data, m))
    rec["builds"] = builds
    loc = {}
    if units:
        for k, lc in enumerate(LOCALES):
            m = {str(i): MAPS["plain"](i, t) if k % 2 else MAPS["acc"](i, t) for i, t in enumerate(units)}
            loc[lc] = digest(f.build_localized(data, m, lc))
            loc[lc + "|map"] = "plain" if k % 2 else "acc"
    rec["localized"] = loc
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "5354524e47544142"  "4c45",  # truncated word
    "fffe53",  # UTF-16 odd
    "fffe00d8",  # lone surrogate
    "efbbbfff",  # bad UTF-8 after BOM
    "0102030405060708" * 20 + "e9",  # control bytes
    "2370726167" "6d6120636f64655f706167652831323334290a",  # unknown code page
    "810a",  # 0x81 undefined in cp1252
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
