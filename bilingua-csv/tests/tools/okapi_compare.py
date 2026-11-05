r"""Builds tests/data/okapi_table.json: what Okapi's table filters (okf_table_csv, okf_table_tsv and the .fprm
configurations of Okapi's own tests) and the LocOS tabular filter make of Okapi's table test resources.

    python okapi_compare.py OUT.json            (system Python 3, standard library only for the Okapi side)

It runs C:\Users\brent\code\okapi-oracle\okapi_dump.py once per file and configuration, and the LocOS filter through
the LocOS venv (a child process, read only use). The Rust test `tests/okapi_table.rs` replays the same calls against
Bilingua, requires Bilingua == LocOS and prints the Okapi comparison.
"""

import json
import os
import re
import subprocess
import sys
import unicodedata
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "..", "data", "okapi")
ORACLE = os.environ.get("OKAPI_ORACLE", r"C:\Users\brent\code\okapi-oracle")
LOCOS_PY = r"C:\Users\brent\code\local-tms\.venv\Scripts\python.exe"
LOCOS_DIR = r"C:\Users\brent\code\local-tms"

# data file -> the .fprm of Okapi's own tests that reads it
PAIRS = {
    "CSVTesting01.csv": "CSVTesting01",
    "CSVTest_96.txt": "copy-of-csv_96",
    "CSVTest_97.txt": "copy-of-csv_97",
    "strong.csv": "strong",
    "test2cols.csv": "2Cols_ID_Text",
    "TSV_test.txt": "table_src_tab_trg",
    "Locale_defined_TSV_test.txt": "defined_locales",
    "csv_testa.txt": "record_id",
}


def norm(s):
    s = unicodedata.normalize("NFC", s).replace("\u00a0", " ")
    return re.sub(r"\s+", " ", s).strip()


def fprm(name):
    out = {}
    with open(os.path.join(DATA, f"okf_table@{name}.fprm"), encoding="utf-8") as fh:
        for line in fh:
            if "=" in line and not line.startswith("#"):
                k, v = line.rstrip("\r\n").split("=", 1)
                out[k] = v
    return out


def spec_of(p):
    """The LocOS column mapping closest to an Okapi configuration (first column of each role)."""
    first = lambda v: (v.split(",")[0].strip() if v else "")  # noqa: E731
    parts = ["header=yes" if int(p.get("columnNamesLineNum.i", "0")) > 0 else "header=no"]
    if first(p.get("sourceColumns", "")):
        parts.append("source=" + first(p["sourceColumns"]))
    if first(p.get("targetColumns", "")):
        parts.append("target=" + first(p["targetColumns"]))
    if first(p.get("sourceIdColumns", "")):
        parts.append("key=" + first(p["sourceIdColumns"]))
    if first(p.get("commentColumns", "")):
        parts.append("context=" + first(p["commentColumns"]))
    d = p.get("fieldDelimiter")
    if d == "\t":
        parts.append("delimiter=tab")
    elif d == ",":
        parts.append("delimiter=comma")
    return ",".join(parts)


def okapi(path, fc, pd=None):
    cmd = [sys.executable, os.path.join(ORACLE, "okapi_dump.py"), path, "--fc", fc]
    if pd:
        cmd += ["--pd", pd]
    p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    units = []
    for line in p.stdout.splitlines():
        if line.startswith("{"):
            d = json.loads(line)
            if "text" in d:
                units.append(d["text"])
    err = p.stderr.strip().splitlines()[-1] if p.returncode and p.stderr.strip() else None
    return units, err


LOCOS_CHILD = r'''
import json, os, sys, tempfile
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-csv-okapi-")
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
from app import filters as _f
from app.filters.base import get_filter
calls = json.load(open(sys.argv[2], encoding="utf-8"))
out = []
for c in calls:
    f = get_filter(c["fmt"])
    data = open(c["path"], "rb").read()
    try:
        pr = f.parse(data, columns=c["columns"], source_locale="en", target_locales=["fr"])
        out.append({"units": [u.text for u in pr.units]})
    except Exception as e:
        out.append({"error": type(e).__name__ + ": " + str(e)[:120]})
json.dump(out, open(sys.argv[3], "w", encoding="utf-8"))
'''


def locos(calls):
    import tempfile

    d = tempfile.mkdtemp(prefix="okapi-cmp-")
    cj, oj, sj = (os.path.join(d, n) for n in ("calls.json", "out.json", "child.py"))
    json.dump(calls, open(cj, "w", encoding="utf-8"))
    open(sj, "w", encoding="utf-8").write(LOCOS_CHILD)
    subprocess.run([LOCOS_PY, "-I", sj, LOCOS_DIR, cj, oj], check=True)
    return json.load(open(oj, encoding="utf-8"))


def main():
    out_path = sys.argv[1]
    files = sorted(f for f in os.listdir(DATA) if not f.endswith(".fprm") and not f.endswith(".catkeys"))
    jobs = []  # (file, config, fmt, columns, okapi fc)
    for f in files:
        tsv = "tsv" in f.lower()
        fmt = "tsv" if tsv else "csv"
        jobs.append((f, "default", fmt, None, "okf_table_tsv" if tsv else "okf_table_csv"))
        jobs.append((f, "default-header-no", fmt, "header=no", "okf_table_tsv" if tsv else "okf_table_csv"))
    for f, name in PAIRS.items():
        p = fprm(name)
        fmt = "tsv" if "tsv" in p.get("parametersClass", "") else "csv"
        jobs.append((f, "fprm:" + name, fmt, spec_of(p), "okf_table@" + name))

    def run(j):
        f, cfg, fmt, cols, fc = j
        path = os.path.abspath(os.path.join(DATA, f))
        if cfg == "default-header-no":
            return None  # same Okapi run as "default"
        pd = os.path.abspath(DATA) if "@" in fc else None
        units, err = okapi(path, fc, pd)
        return [norm(u) for u in units], err

    with ThreadPoolExecutor(6) as ex:
        res = list(ex.map(run, jobs))
    okapi_default = {j[0]: r for j, r in zip(jobs, res) if j[1] == "default"}
    calls = [{"fmt": j[2], "columns": j[3], "path": os.path.abspath(os.path.join(DATA, j[0]))} for j in jobs]
    lo = locos(calls)
    cases = []
    for j, r, l in zip(jobs, res, lo):
        f, cfg, fmt, cols, fc = j
        if r is None:
            r = okapi_default[f]
        cases.append({
            "file": f, "config": cfg, "fmt": fmt, "columns": cols, "okapi_fc": fc,
            "okapi": r[0], "okapi_error": r[1],
            "locos": [norm(u) for u in l["units"]] if "units" in l else None, "locos_error": l.get("error"),
        })
    json.dump(cases, open(out_path, "w", encoding="utf-8"), indent=0, ensure_ascii=False)
    print(len(cases), "cases")


if __name__ == "__main__":
    main()
