"""Compares Bilingua's units with Okapi okf_tex on the Okapi test files (read only use of the oracle).

    python -I compare_okapi.py LATEX_DUMP_EXE FILE [FILE ...]

Text is compared with every code removed (Okapi's U+00A4 markers, Bilingua's tokens) and whitespace collapsed.
"""
import io
import json
import re
import subprocess
import sys

OKAPI = r"C:\Users\brent\code\okapi-oracle\okapi_dump.py"


def norm(s):
    s = re.sub("\u27e6/?[a-z]?[0-9]+\u27e7", "", s).replace("\u00a4", "").replace("\u00a0", " ")
    return re.sub(r"\s+", " ", s).strip()


sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")


def main():
    exe, files = sys.argv[1], sys.argv[2:]
    for f in files:
        ours = [norm(l.split("\t", 1)[1]) for l in subprocess.run([exe, f], capture_output=True, text=True, encoding="utf-8").stdout.splitlines() if "\t" in l]
        out = subprocess.run([sys.executable, "-I", OKAPI, f, "--fc", "okf_tex"], capture_output=True, text=True, encoding="utf-8").stdout
        okapi = [norm(json.loads(l)["text"]) for l in out.splitlines() if l.startswith("{")]
        okapi = [t for t in okapi if t]
        same = [t for t in okapi if t in ours]
        print(f"{f}: Okapi {len(okapi)} units, Bilingua {len(ours)} units, Okapi units found in Bilingua (codes removed): {len(same)}")
        for t in okapi:
            if t not in ours:
                print("  only Okapi:", t[:100])
        for t in ours:
            if t not in okapi:
                print("  only Bilingua:", t[:100])


if __name__ == "__main__":
    main()
