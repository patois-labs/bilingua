"""Writes src/cldr_table.rs: for every locale key the Bilingua ARB table knows, the CLDR plural categories that
whole numbers 0..1000 reach, as Babel computes them (the rule Localization OS's plural_categories uses for a Qt
numerus message). Run with the LocOS venv Python, read only:

    <local-tms venv python> -I gen_cldr.py ARB_CLDR_TABLE_RS OUT_RS
"""
import re
import sys

from babel import Locale
import babel

ORDER = ["zero", "one", "two", "few", "many", "other"]


def mask(key):
    rule = Locale.parse(key).plural_form
    cats = {rule(n) for n in range(0, 1001)}
    return sum(1 << i for i, c in enumerate(ORDER) if c in cats)


def main():
    src = open(sys.argv[1], encoding="utf-8").read()
    table, aliases = src.split("static ALIASES", 1) if "static ALIASES" in src else (src, "")
    keys = re.findall(r'\("([A-Za-z0-9_]+)", \d+, \d+\)', table)
    akeys = re.findall(r'\("([A-Za-z0-9_]+)", \d+, \d+\)', aliases)
    out = [f"// Generated from Babel {babel.__version__} by tests/tools/gen_cldr.py.",
           "// (language or language_Script_TERRITORY, categories whole numbers 0..=1000 reach); bit 0 zero, 1 one, 2 two, 3 few, 4 many, 5 other.",
           f'pub const BABEL_VERSION: &str = "{babel.__version__}";']
    for name, ks in (("TABLE", keys), ("ALIASES", akeys)):
        rows = []
        for k in ks:
            try:
                rows.append((k, mask(k)))
            except Exception:
                pass
        rows.sort()
        out.append(f"pub(crate) static {name}: &[(&str, u8)] = &[")
        line = "   "
        for k, m in rows:
            item = f' ("{k}", {m}),'
            if len(line) + len(item) > 118:
                out.append(line)
                line = "   "
            line += item
        out.append(line)
        out.append("];")
    open(sys.argv[2], "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")


main()
