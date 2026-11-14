"""Writes tests/data/okapi_units.json: the text units Okapi (Tikal 1.48.0, filter okf_ts) extracts from Okapi's own
test resources in tests/data/okapi, in the normal form of the Okapi oracle (inline codes as U+00A4, white space
collapsed). `differs` says why Bilingua (which follows LocOS) lists fewer units.

    python gen_okapi.py   (needs the okapi-oracle folder next to local-tms, and Java 17)
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "..", "data", "okapi")
ORACLE = os.environ.get("OKAPI_ORACLE", r"C:\Users\brent\code\okapi-oracle")
REASONS = {
    "TestInQT.ts": "Okapi also lists the message that has no translation element; LocOS skips it",
    "TestInQT_Saved.ts": "Okapi also lists the message that has no translation element; LocOS skips it",
    "Complete_valid_utf8_bom_crlf.ts": "the file nests a context inside a context (not valid Qt); LocOS reads only top-level contexts, Okapi reads both",
}


def main():
    out = {}
    for name in sorted(os.listdir(DATA)):
        r = subprocess.run([sys.executable, os.path.join(ORACLE, "okapi_dump.py"), os.path.join(DATA, name), "--fc", "okf_ts"],
                           capture_output=True, text=True, encoding="utf-8", check=True)
        units = [json.loads(l)["text"] for l in r.stdout.splitlines() if l.strip()]
        out[name] = {"units": units, "differs": REASONS.get(name, "")}
    with open(os.path.join(HERE, "..", "data", "okapi_units.json"), "w", encoding="utf-8", newline="\n") as fh:
        json.dump(out, fh, ensure_ascii=False, indent=1)
    print({k: len(v["units"]) for k, v in out.items()})


main()
