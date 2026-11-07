"""Make or check bilingua-model/src/plural_table.rs from Localization OS's gettext plural rules.

LocOS picks a target's rule with `po_filter._target_plural` (babel's legacy gettext table, else the CLDR rule as
gettext). This script asks it for every locale babel can parse (its identifiers and aliases, every two- and
three-letter code, and a few region and script forms of the old codes) and keeps one row per key whose rule is not
already what the Rust lookup gives (the full key first, then its language).

Run it with the LocOS venv (LocOS is only read):

    <local-tms>/.venv/Scripts/python.exe -I bilingua/tools/gen-plural-table.py --locos <local-tms> [--write]

Without --write it lists the keys where the table and LocOS differ and exits 1 if there are any. A key LocOS does
not know (`sr_XX`, `zh_US`) is listed as a known difference but not counted: Bilingua falls back to the language.
"""

import argparse
import itertools
import os
import re
import string
import sys
import tempfile

HEAD = """//! Generated from the Localization OS plural rules (babel's gettext table, CLDR rules for the rest): one row per
//! locale whose rule differs from its language's. (key, nplurals, expression, slot for n = 1, catch-all slot).
//! Made by bilingua/tools/gen-plural-table.py; edit that, not this file.

pub const RULES: &[(&str, u8, &str, u8, u8)] = &[
"""
ROW = re.compile(r'\("([^"]+)", (\d+), "((?:[^"\\]|\\.)*)", (\d+), (\d+)\)')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--locos", required=True, help="a Localization OS checkout (read only)")
    ap.add_argument("--write", action="store_true", help="rewrite the table instead of checking it")
    ap.add_argument("--table", default=os.path.join(os.path.dirname(__file__), "..", "bilingua-model", "src", "plural_table.rs"))
    a = ap.parse_args()
    sys.path.insert(0, os.path.abspath(a.locos))
    # LocOS reads its settings on import: keep anything it writes out of the checkout
    os.environ.setdefault("LOC_TMS_DATA_DIR", os.path.join(tempfile.gettempdir(), "bili-plural-gen"))
    from babel import localedata
    from babel.core import LOCALE_ALIASES, Locale, UnknownLocaleError

    from app.filters.plurals import _gettext_one_and_catch_all
    from app.filters.po_filter import _target_plural

    def locos(ident):
        try:
            r = _target_plural(Locale.parse(ident))
        except (UnknownLocaleError, ValueError):
            return None
        p = _gettext_one_and_catch_all(r.plural_expr)
        return (r.num_plurals, r.plural_expr, p[0] if p else 0, p[1] if p else 0)

    letters = string.ascii_lowercase
    idents = set(localedata.locale_identifiers()) | set(LOCALE_ALIASES)
    idents |= {"".join(t) for t in itertools.product(letters, repeat=2)}
    idents |= {"".join(t) for t in itertools.product(letters, repeat=3)}
    idents |= {f"{i}_{r}" for i in ("tl", "mo", "iw", "in", "ji", "no", "sh", "sr", "zh") for r in ("PH", "US", "RS", "CN", "TW", "XX", "Latn", "Cyrl")}
    want = {i: locos(i) for i in sorted(idents)}

    def lookup(rows, ident):
        return rows.get(ident) or rows.get(ident.split("_")[0])

    if a.write:
        rows = {}
        # languages first, so a regional key gets a row only when it differs from its language
        for ident in sorted(want, key=lambda i: (i.count("_"), i)):
            w = want[ident]
            if w and lookup(rows, ident) != w:
                rows[ident] = w
        with open(a.table, "w", encoding="utf-8", newline="\n") as f:
            f.write(HEAD)
            for k in sorted(rows):
                n, expr, one, catch = rows[k]
                expr = expr.replace("\\", "\\\\").replace('"', '\\"')
                f.write(f'    ("{k}", {n}, "{expr}", {one}, {catch}),\n')
            f.write("];\n")
        print(len(rows), "rows written to", a.table)
        return 0

    text = open(a.table, encoding="utf-8").read()
    rows = {}
    for m in ROW.finditer(text):
        rows[m[1]] = (int(m[2]), m[3].replace('\\"', '"').replace("\\\\", "\\"), int(m[4]), int(m[5]))
    keys = [m[1] for m in ROW.finditer(text)]
    bad = 0
    if keys != sorted(keys):
        print("the table is not sorted (the lookup is a binary search)")
        bad += 1
    for ident, w in want.items():
        got = lookup(rows, ident)
        if w != got:
            known = w is None
            bad += not known
            print(ident, "LocOS", w, "table", got, "(known: LocOS does not know it)" if known else "")
    print(bad, "differences over", len(want), "keys")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
