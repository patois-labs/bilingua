"""Builds the differential goldens in tests/data: random .desktop and .catkeys files and what the LocOS filters make
of them, plus the locale spellings (`posix_locale`, `language_name`).

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe -I gen_golden.py OUT_DIR

Writes OUT_DIR/locos_golden_desktop.json, locos_golden_catkeys.json and locos_locales.json. For each file: the input
bytes (hex), the units LocOS parses (or the error class), its records(), and a 64 bit FNV-1a digest of the bytes
LocOS builds with three mappings (every unit, every third unit, plain) and for several target locales (None means
`build`, the rest `build_localized`). Deterministic (seed 1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-desktop-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters import catkeys_filter, desktop_filter  # noqa: E402
from app.filters.base import get_filter  # noqa: E402

LOCALES = [None, "de", "fr-FR", "zh-Hans", "sr-Latn", "pt-BR", "ar", "ja-JP"]


def fnv(b: bytes) -> str:
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def mappings(n):
    return {
        "all": {str(i): f"T{i} \"q\" \u00e9; x\nline2\t\\ end " for i in range(n)},
        "third": {str(i): f" Zeta {i} \u65e5;\u672c" for i in range(0, n, 3)},
        "plain": {str(i): f"Plain{i}" for i in range(n)},
    }


# ------------------------------------------------------------------ .desktop
D_VALUES = [
    "Text Editor", "Edit\\stext files;\\nquickly\\\\safely", "text;editor;plain\\;simple;", "a;b", "x;;y;", ";", "",
    "  spaced  ", "\\s\\sx\\s", "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "100%",
    "%U", "tab\\there", "\\q unknown", "ends with backslash \\", "one;two;three;", "emoji \U0001f600", "\u00a0nbsp", "\u00bd",
    "\\;", "\\\\;", "k1;\\s;k3", "x = y",
]
D_OTHER = ["Exec=app %F", "Icon=app", "Type=Application", "Categories=Utility;", "Version=1.5", "X-Foo=bar", "MimeType=a/b;"]


def make_desktop(rng):
    nl = rng.choice(["\n", "\r\n", "\n", "\n"])
    lines = []
    if rng.random() < 0.9:
        lines.append("[Desktop Entry]")
    for _ in range(rng.randint(0, 25)):
        r = rng.random()
        if r < 0.40:
            k = rng.choice(["Name", "GenericName", "Comment", "Keywords", "Name", "Comment"])
            sp = rng.choice(["", " ", "\t", "  "])
            lines.append(f"{k}{rng.choice(['', ' '])}={sp}{rng.choice(D_VALUES)}{rng.choice(['', ' ', '\t'])}")
        elif r < 0.52:
            k = rng.choice(["Name", "Comment", "Keywords", "GenericName"])
            lines.append(f"{k}[{rng.choice(['de', 'fr_FR', 'zh_CN', 'sr@latin', 'pt_BR', 'ar', 'ja_JP'])}]={rng.choice(D_VALUES)}")
        elif r < 0.62:
            lines.append(rng.choice(D_OTHER))
        elif r < 0.70:
            lines.append(rng.choice(["[Desktop Action new-window]", "[Desktop Action x]", "[X-Vendor Settings]", "[Desktop Entry]", "[]", "[Foo"]))
        elif r < 0.78:
            lines.append(rng.choice(["# comment", "#", "  # indented note", "#Name=x"]))
        elif r < 0.84:
            lines.append("")
        elif r < 0.90:
            lines.append(rng.choice(["garbage line", "Name[x y]=z", "=novalue", "Na me=x", "Name[]=x", "N\u00e4me=x"]))
        else:
            lines.append(rng.choice(["Name", "Name[de]", "Keywords=", "Name=", "Comment=\\s"]))
    body = nl.join(lines) + rng.choice([nl, nl, ""])
    data = body.encode("utf-8")
    if rng.random() < 0.1:
        data = b"\xef\xbb\xbf" + data
    if rng.random() < 0.03:
        data = data + b"\xff"
    return data


# ------------------------------------------------------------------ .catkeys
C_SRC = [
    "Open", "Save %s?", "(", "Line one\\nLine \\\"two\\\"", "Quit", "tab\\there", "back\\\\slash", "\\x41BC", "bad\\xZZ", "\\q",
    "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "100%", "%d", "---", "x", "\u00bd", "\u2460",
    "\\a\\b\\f\\v", "end\\", "\\xC3\\xA9", "\\xFF", "emoji \U0001f600", "", " ", "Hello World",
]
C_CTX = ["MainWindow", "About", "", "CalcView", "Menu"]
C_COMMENT = ["", "file name", "button", "a\\tb", ""]
C_TR = ["", "Open", "\u00d6ffnen", "Line one\\nLine \\\"two\\\"", "x\\\\y", "a b", "\\q", "Beenden", ""]


def make_catkeys(rng):
    nl = rng.choice(["\n", "\r\n", "\n"])
    lang = rng.choice(["english", "german", "portuguese (brazil)", "english", "xx"])
    fp = rng.choice(["123456789", "0", "1", "42"])
    lines = [f"1\t{lang}\tx-vnd.Example-App\t{fp}"]
    for _ in range(rng.randint(0, 25)):
        r = rng.random()
        if r < 0.93:
            lines.append("\t".join([rng.choice(C_SRC), rng.choice(C_CTX), rng.choice(C_COMMENT), rng.choice(C_TR)]))
        elif r < 0.96:
            lines.append("")
        elif r < 0.98:
            lines.append(rng.choice(["only\ttwo", "a\tb\tc", "no tabs"]))
        else:
            lines.append("tab\tin\tlast\tfield\twith\textra")
    if rng.random() < 0.04:
        lines[0] = rng.choice(["2\tenglish\tx\t1", "1\tenglish\tx\tabc", "1\tenglish", ""])
    body = nl.join(lines) + rng.choice([nl, nl, ""])
    data = body.encode("utf-8")
    if rng.random() < 0.1:
        data = b"\xef\xbb\xbf" + data
    if rng.random() < 0.03:
        data = data + b"\xff"
    return data


def golden(fmt, make, n, seed, extra_files=()):
    filt = get_filter(fmt)
    rng = random.Random(seed)
    datas = [(f"rand{i}", make(rng)) for i in range(n)] + list(extra_files)
    out = []
    for name, data in datas:
        case = {"name": name, "hex": data.hex()}
        try:
            pr = filt.parse(data)
        except Exception as exc:  # noqa: BLE001
            case["error"] = type(exc).__name__
            out.append(case)
            continue
        units = [u.text for u in pr.units]
        case["units"] = units
        mod = desktop_filter if fmt == "desktop" else catkeys_filter
        recs = mod_records(filt, data)
        case["records"] = recs
        builds = {}
        for label, m in mappings(len(units)).items():
            for loc in LOCALES:
                try:
                    b = filt.build(data, m) if loc is None else filt.build_localized(data, m, loc)
                    builds[f"{label}|{loc}"] = fnv(b)
                except Exception as exc:  # noqa: BLE001
                    builds[f"{label}|{loc}"] = "ERR:" + type(exc).__name__
        case["builds"] = builds
        out.append(case)
    return out


def mod_records(filt, data):
    rows = []
    for r in filt.records(data):
        rows.append({"key": r.key, "value": r.value, "comment": r.comment, "context": r.context,
                     "unit_index": r.unit_index, "duplicate": r.duplicate})
    return rows


def locales():
    from babel import localedata

    ids = sorted(localedata.locale_identifiers())
    rng = random.Random(7)
    codes = set(i.replace("_", "-") for i in ids)
    langs = sorted({i.split("_")[0] for i in ids})
    terrs = ["US", "GB", "DE", "FR", "BR", "PT", "CN", "TW", "HK", "ES", "MX", "419", "001", "JP", "KR", "SA", "IL", "XX", "CH", "AT"]
    for lang in langs:
        for _ in range(2):
            codes.add(f"{lang}-{rng.choice(terrs)}")
    codes |= {
        "de", "DE", "de-de", "DE-de", "pt_BR", "pt-br", "zh", "zh-Hans", "zh-Hant", "zh-Hans-CN", "zh-Hant-HK", "zh-TW",
        "zh-CN", "sr-Latn", "sr-Latn-RS", "sr-Cyrl", "sr-Cyrl-RS", "sr@latin", "sr_RS@latin", "ca-ES-valencia", "es-419",
        "en-001", "xx", "xx-!!", "x", "tlh", "iw", "in", "ji", "no", "nb", "nn", "tl", "fil", "he", "en-US-POSIX",
        "de-CH-1996", "de-Latn", "de-Latn-DE", "en-GB-oxendict", "de-DE-u-co-phonebk", "de.UTF-8", "de_DE.UTF-8",
        "  de  ", "", "-", "_", "de-", "-de", "de--DE", "pa-Arab", "pa-Guru-IN", "uz-Cyrl", "uz-Latn-UZ",
        "az-Cyrl", "mn-Mong", "ff-Adlm", "ff-Latn-SN", "fr-CA", "fr-BE", "nl-BE", "ar-EG", "ar-001", "ru-RU", "ko-KR",
        "ja-JP", "tr-TR", "hi-IN", "de-AT", "en-AU", "en-IN", "ro-MD", "es-US", "sw-KE", "Zh-hANS", "pt-pt", "az",
        "\u00e9", "d\u00e9", "de-\u00c9\u00c9", "english", "deutsch", "abc-def-ghi-jkl", "a-b", "root", "en_US", "en-us",
    }
    rows = []
    for c in sorted(codes):
        rows.append({"code": c, "name": catkeys_filter.language_name(c), "posix": desktop_filter.posix_locale(c)})
    return rows


def main():
    outdir = sys.argv[1]
    here = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")
    extra_d, extra_c = [], []
    for fn in sorted(os.listdir(here)):
        p = os.path.join(here, fn)
        if fn.endswith(".desktop"):
            extra_d.append((fn, open(p, "rb").read()))
        elif fn.endswith(".catkeys"):
            extra_c.append((fn, open(p, "rb").read()))
    for fmt, make, extra in (("desktop", make_desktop, extra_d), ("catkeys", make_catkeys, extra_c)):
        g = golden(fmt, make, 600, 1104, extra)
        with open(os.path.join(outdir, f"locos_golden_{fmt}.json"), "w", encoding="utf-8") as fh:
            json.dump(g, fh, ensure_ascii=True, separators=(",", ":"))
        print(fmt, len(g), sum(1 for c in g if "error" in c), "errors")
    rows = locales()
    with open(os.path.join(outdir, "locos_locales.json"), "w", encoding="utf-8") as fh:
        json.dump(rows, fh, ensure_ascii=True, separators=(",", ":"))
    print("locales", len(rows))


if __name__ == "__main__":
    main()
