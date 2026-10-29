"""Conformance scoreboard: how far Bilingua is from the Localization OS filters, format by format.

    python run.py --scoreboard [--build] [--out SCOREBOARD.md]

* Inventory: every format the LocOS registry knows (`locos_dump.py --inventory`) against what Bilingua parses
  (`bl-dump --formats` for the source formats, a fixed list for the bilingual formats that live in core/catcore).
* Files: the made-up `corpus/`, plus the LocOS fixtures read in place (`tests/fixtures/formats/**`, `sample-docs/`,
  and the packages its plain-script generators write). Nothing is copied into this repository; the generated
  table lists file names and counts only, never text from a LocOS file.
* Per format one row: files, units on both sides, text match, round trip, status.

Comparison rules are `run.py`'s (`compare`). LocOS is only imported or read, never written to or started.
"""

import json
import os
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

import run as R

HERE = R.HERE
REPO = HERE.parents[1]
CATCORE = REPO / "core" / "target" / "debug" / ("catcore-cli.exe" if os.name == "nt" else "catcore-cli")

# Bilingual formats that live in core/catcore for now (they will move into Bilingua). Extension -> label.
CATCORE_EXTS = {"xlf": "XLIFF 1.2/2", "xliff": "XLIFF 1.2/2", "sdlxliff": "SDLXLIFF", "mqxliff": "MQXLIFF", "mxliff": "MXLIFF"}
CATCORE_PACKAGES = {"sdlppx": "SDL project package", "sdlrpx": "SDL return package", "mqxlz": "memoQ package"}

# Plain-script generators in the LocOS fixture tree: (folder/script, function, file name written).
GENERATORS = [
    ("pptx/make_pptx.py", "make_pptx", "generated_pptx.pptx"),
    ("xlsx/make_xlsx_objects.py", "make_workbook", "generated_xlsx.xlsx"),
    ("ooxml_strict/make_strict.py", "make_docx", "strict_docx.docx"),
    ("ooxml_strict/make_strict.py", "make_xlsx", "strict_xlsx.xlsx"),
    ("ooxml_strict/make_strict.py", "make_pptx", "strict_pptx.pptx"),
]

# Missing formats, most used first (a guess): software resources, then Office / DTP / documentation, then the tail.
SOFTWARE = ["properties", "yaml", "xml", "strings", "xcstrings", "stringsdict", "resx", "arb", "json5", "csv", "plist",
            "js", "ts", "ftl", "php", "ini", "toml", "mo", "xmb", "xtb", "resjson", "rc", "wxl", "desktop", "dtd",
            "tsv", "xaml", "erb", "catkeys"]
OFFICE = ["odt", "ods", "odp", "odg", "idml", "icml", "mif", "pdf", "epub", "vsdx", "xtg", "adoc", "rst", "mdx", "tex",
          "dita", "docbook", "wiki", "dokuwiki", "txlf", "txml", "ttx", "xlz", "dxf", "eml", "flprjzip", "hhc", "hhk",
          "hhp", "aspx", "jsp", "phtml", "shtml", "xslt"]
RANK = {f: (1, i) for i, f in enumerate(SOFTWARE)} | {f: (2, i) for i, f in enumerate(OFFICE)}
TIER = {1: "software resource", 2: "Office, DTP, docs, CAT", 3: "long tail"}


def tier(fmt):
    return RANK.get(fmt, (3, 0))


def locos_inventory():
    out = Path(tempfile.mkdtemp(prefix="oracle-inv-")) / "inv.json"
    subprocess.run([str(R.LOCOS_PY), str(HERE / "locos_dump.py"), "--inventory", str(out)], check=True, capture_output=True, timeout=300)
    return json.loads(out.read_text(encoding="utf-8"))


def bilingua_formats():
    p = subprocess.run([str(R.BL), "--formats"], capture_output=True, timeout=60)
    return json.loads(p.stdout.decode("utf-8"))


def generate_locos_fixtures(fx: Path):
    """Runs the plain-script generators (stdlib only) with the output going to a scratch folder."""
    scratch = Path(tempfile.gettempdir()) / "oracle-scoreboard-generated"
    scratch.mkdir(parents=True, exist_ok=True)
    made = []
    for script, fn, name in GENERATORS:
        src = fx / script
        if not src.is_file():
            continue
        code = (
            "import runpy, pathlib\n"
            f"ns = runpy.run_path(r'{src}')\n"
            f"pathlib.Path(r'{scratch / name}').write_bytes(ns['{fn}']())\n"
        )
        try:
            subprocess.run([sys.executable, "-I", "-c", code], check=True, capture_output=True, timeout=60)
            made.append(scratch / name)
        except Exception:  # noqa: BLE001
            pass
    return made


def collect_files():
    """(path, origin) pairs. Origins: corpus (made up, committed), fixture, sample, generated (LocOS tree, read in place)."""
    files = [(p, "corpus") for p in sorted((HERE / "corpus").rglob("*")) if p.is_file() and p.suffix[1:].lower() in R.FORMATS]
    fx = R.LOCOS / "tests" / "fixtures" / "formats"
    if fx.is_dir():
        files += [(p, "fixture") for p in sorted(fx.rglob("*")) if p.is_file() and p.suffix.lower() != ".py"]
        files += [(p, "generated") for p in generate_locos_fixtures(fx)]
    sd = R.LOCOS / "sample-docs"
    if sd.is_dir():
        files += [(p, "sample") for p in sorted(sd.iterdir()) if p.is_file()]
    return files


def run_catcore(path: Path):
    """Bilingual files through core/catcore: units from `dump`, round trip from `roundtrip`."""
    exe = str(CATCORE)
    try:
        d = subprocess.run([exe, "dump", str(path), "--range", "0.."], capture_output=True, timeout=300)
        if d.returncode != 0:
            return {"error": d.stderr.decode("utf-8", "replace").strip()[:200] or "catcore dump failed"}
        segs = json.loads(d.stdout.decode("utf-8"))
        mark = {"open": "", "close": ""}
        units = []
        for s in segs:
            text = "".join(x["text"] if "text" in x else mark.get(x["tag"]["kind"], "") for x in s["source"])
            # LocOS leaves finished segments (translated, confirmed) out of its units; catcore lists every
            # segment with its state. Finished ones are marked locked here so that the lists are comparable.
            done = s.get("state") in ("translated", "confirmed")
            units.append({"id": s["id"], "text": text, "locked": bool(s.get("locked")) or done})
        rt = subprocess.run([exe, "roundtrip", str(path)], capture_output=True, timeout=300).stdout.decode("utf-8", "replace")
        unchanged = "identical" if rt.startswith("OK") else "differs: " + rt.strip()[:120]
        # catcore has no pseudo-translate command, so only the unchanged round trip is checked.
        return {"units": units, "unchanged": unchanged, "pseudo": {"status": "n/a"}}
    except Exception as e:  # noqa: BLE001
        return {"error": f"harness: {e}"}


def run_catcore_package(path: Path):
    p = subprocess.run([str(CATCORE), "package", str(path)], capture_output=True, timeout=300)
    if p.returncode != 0:
        return {"error": p.stderr.decode("utf-8", "replace").strip()[:200]}
    return {"units": [], "unchanged": "n/a", "pseudo": {"status": "n/a"}}


def main():
    args = sys.argv[1:]
    out = HERE / "SCOREBOARD.md"
    if "--out" in args:
        out = Path(args[args.index("--out") + 1])
    if "--build" in args:
        subprocess.run(["cargo", "build", "-p", "bilingua-oracle"], cwd=R.ROOT, check=True)
        subprocess.run(["cargo", "build", "-p", "catcore-cli"], cwd=REPO / "core", check=True)
    if not R.LOCOS_PY.exists():
        sys.exit("LocOS venv python not found; set LOCOS_DIR")
    if not CATCORE.exists():
        print("note: catcore-cli is not built (cargo build -p catcore-cli in core/); bilingual formats will be reported as missing")

    inv = locos_inventory()
    bl_filters = bilingua_formats()
    bl_exts = {e for f in bl_filters for e in f["extensions"]}
    bi_exts = set(CATCORE_EXTS) | set(CATCORE_PACKAGES) if CATCORE.exists() else set()
    ext_fmt = {e: row["fmt"] for row in inv for e in row["exts"]}

    files = collect_files()
    lo = R.run_locos([p for p, _ in files])
    scratch = Path(tempfile.mkdtemp(prefix="oracle-sb-"))
    rows = defaultdict(lambda: {"files": [], "bl_n": 0, "lo_n": 0, "m_n": 0, "m_d": 0, "issues": Counter(), "notes": Counter(),
                                "bl_err": [], "lo_err": [], "rt_bad": [], "rt_checked": 0, "supported": False})
    for n, (p, origin) in enumerate(files):
        ext = p.suffix[1:].lower()
        lres = lo.get(str(p)) or {"error": "LocOS not available"}
        fmt = lres.get("filter") or ext_fmt.get(ext) or ext
        row = rows[fmt]
        row["files"].append((p.name, origin))
        if ext in CATCORE_PACKAGES:
            bres = run_catcore_package(p) if CATCORE.exists() else {"error": "no parser (catcore-cli not built)"}
        elif ext in CATCORE_EXTS and CATCORE.exists():
            bres = run_catcore(p)
        else:
            bres = R.run_bl(p, scratch / f"{n}-{p.name}")
        if "no parser" in (bres.get("error") or "") or "not built" in (bres.get("error") or ""):
            # Bilingua has no filter for this file: only the LocOS side is counted.
            if not lres.get("error"):
                row["lo_n"] += sum(1 for u in lres["units"] if R.has_letters(R.strip_codes(R.norm_lo(u["text"]))))
            else:
                row["lo_err"].append(p.name)
            continue
        row["supported"] = True
        r = R.compare(p, bres, lres)
        if r["bl_error"]:
            row["bl_err"].append(f"{p.name}: {R.short(r['bl_error'], 60)}")
            if not r["lo_error"]:
                row["lo_n"] += sum(1 for u in lres["units"] if R.has_letters(R.strip_codes(R.norm_lo(u["text"]))))
            continue
        if r["lo_error"]:
            row["lo_err"].append(p.name)
            row["bl_n"] += sum(1 for u in bres.get("units", []) if not u["locked"] and R.has_letters(R.strip_codes(R.norm_bl(u["text"]))))
        else:
            row["bl_n"] += r["bl_n"]
            row["lo_n"] += r["lo_n"]
            row["m_n"] += r.get("match_n", 0)
            row["m_d"] += r.get("match_d", 0)
            for k, _, _ in r["issues"]:
                row["issues"][k] += 1
            for k, _, _ in r["notes"]:
                row["notes"][k] += 1
        row["rt_checked"] += 1
        if r["unchanged"] not in ("identical", "identical-parts", "n/a") or r["pseudo"] not in ("ok", "n/a"):
            row["rt_bad"].append(f"{p.name}: {R.short(r['unchanged'] if r['unchanged'] not in ('identical', 'identical-parts') else r['pseudo'], 70)}")

    text = render(inv, rows, bl_exts, bi_exts, files)
    out.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {out}: {len(inv)} LocOS formats, {len(files)} files")


def support(row_inv, bl_exts, bi_exts):
    """('source' | 'bilingual' | '-', note) for a LocOS format, from the extensions each side claims."""
    exts = set(row_inv["exts"])
    src, bi = exts & bl_exts, exts & bi_exts
    kind = "source" if src else "bilingual (catcore)" if bi else "-"
    part = ""
    got = src or bi
    if got and got != exts and row_inv["fmt"] not in got:
        part = " (only " + ", ".join(sorted(got)) + ")"
    return kind + part, bool(got)


def pct(a, b):
    return "-" if not b else f"{100.0 * a / b:.1f}%"


def render(inv, rows, bl_exts, bi_exts, files):
    inv_by = {r["fmt"]: r for r in inv}
    table = []
    for fmt in sorted(set(inv_by) | set(rows)):
        row = rows.get(fmt)
        meta = inv_by.get(fmt, {"fmt": fmt, "exts": [fmt], "cls": None})
        sup, claims = support(meta, bl_exts, bi_exts)
        have_files = bool(row and row["files"])
        exts = ", ".join(e for e in meta["exts"] if e != fmt) or ""
        corp = sum(1 for _, o in (row["files"] if row else []) if o == "corpus")
        fix = (len(row["files"]) - corp) if row else 0
        status, why = "missing", ""
        if claims or (row and row["supported"]):
            if not have_files:
                status, why = "untested", "no fixture file in the LocOS tree"
            else:
                reasons = []
                if row["bl_err"]:
                    reasons.append(f"Bilingua failed on {len(row['bl_err'])} file(s)")
                if row["rt_bad"]:
                    reasons.append(f"round trip fails on {len(row['rt_bad'])} file(s)")
                reasons += [f"{k} x{c}" for k, c in row["issues"].most_common(3)]
                if row["lo_err"] and len(row["lo_err"]) == len(row["files"]):
                    status, why = "bilingua-only", "LocOS could not read the file(s)"
                elif row["lo_err"]:
                    reasons.append(f"LocOS failed on {len(row['lo_err'])} file(s)")
                if status != "bilingua-only":
                    # TXT and Markdown units are paragraphs in Bilingua and lines in LocOS: counts are not comparable.
                    counts_differ = row["bl_n"] != row["lo_n"] and fmt not in ("txt", "md")
                    if reasons or counts_differ:
                        status = "differs"
                        if counts_differ and not reasons:
                            reasons.append("unit counts differ")
                        why = "; ".join(reasons)
                    else:
                        status = "same"
                        why = ""
        table.append({"fmt": fmt, "exts": exts, "sup": sup, "claims": claims, "row": row, "files": f"{corp}+{fix}" if have_files else "0",
                      "status": status, "why": why, "tier": tier(fmt)})

    # Formats Bilingua parses that LocOS has no registry entry for.
    known = {e for r in inv for e in r["exts"]}
    only = sorted(e for e in bl_exts | bi_exts if e not in known)

    cnt = Counter(t["status"] for t in table)
    L = ["# Bilingua conformance scoreboard", "",
         "Generated by `python run.py --scoreboard` (`bilingua/oracle/scoreboard.py`); do not edit the tables by hand.",
         "Localization OS is an oracle only (D16): its filters were imported and its fixtures read in place; nothing was",
         "copied here and no LocOS text is quoted. Texts are normalised as in `REPORT.md` (NFC, white space collapsed, inline",
         "codes as one marker). Files are shown as `corpus+LocOS` (the corpus is made up and committed; LocOS files live only in",
         "the LocOS tree).", "",
         "## Summary", "",
         f"LocOS formats: {len(inv)}. Bilingua parses {sum(1 for t in table if t['claims'])} of them (source formats and the bilingual "
         f"formats still in `core/catcore`).", "",
         "| Status | Formats |", "|---|---|"]
    for s in ("same", "differs", "bilingua-only", "untested", "missing"):
        L.append(f"| {s} | {cnt.get(s, 0)} |")
    L.append("")
    L.append("Status rules. `same`: every fixture file gives the same units (normalised) on both sides, an unchanged merge gives the same "
             "file and a pseudo-translated merge reads back. `differs`: anything else, with the top reasons (counts of unit-level "
             "differences, listed by kind). `bilingua-only`: Bilingua reads the files, LocOS cannot. `untested`: Bilingua parses the "
             "format but no fixture exists. `missing`: Bilingua has no parser; the LocOS unit count is shown when a fixture exists.")
    L.append("")
    if only:
        L += ["Extensions Bilingua parses with no LocOS registry entry: " + ", ".join(only) + ".", ""]

    L += ["## Formats Bilingua parses", "",
          "| LocOS format | Also reads as | Bilingua | Files | Units LocOS | Units Bilingua | Text match | Round trip | Status | Top reasons |",
          "|---|---|---|---|---|---|---|---|---|---|"]
    for t in sorted((t for t in table if t["status"] != "missing"), key=lambda t: (["same", "differs", "bilingua-only", "untested"].index(t["status"]), t["fmt"])):
        r = t["row"]
        if r and r["files"]:
            rt = "n/a" if not r["rt_checked"] else "ok" if not r["rt_bad"] else f"FAIL {len(r['rt_bad'])}/{r['rt_checked']}"
            L.append(f"| {t['fmt']} | {t['exts']} | {t['sup']} | {t['files']} | {r['lo_n']} | {r['bl_n']} | {pct(r['m_n'], r['m_d'])} | {rt} | {t['status']} | {t['why']} |")
        else:
            L.append(f"| {t['fmt']} | {t['exts']} | {t['sup']} | 0 | - | - | - | - | {t['status']} | {t['why']} |")
    L.append("")

    miss = sorted((t for t in table if t["status"] == "missing"), key=lambda t: (t["tier"], t["fmt"]))
    L += ["## Formats Bilingua does not parse yet", "",
          "Ordered by a usage guess: software resource formats first, then Office, DTP, documentation and CAT formats, then the long tail "
          "(alphabetical inside the tail). `Units LocOS` is what the LocOS filter extracts from the fixtures we have (0 or `-`: no fixture).", "",
          "| # | LocOS format | Also reads as | Group | Files | Units LocOS | LocOS read errors |", "|---|---|---|---|---|---|---|"]
    for i, t in enumerate(miss, 1):
        r = t["row"]
        L.append(f"| {i} | {t['fmt']} | {t['exts']} | {TIER[t['tier'][0]]} | {t['files']} | {r['lo_n'] if r and r['files'] else '-'} | {len(r['lo_err']) if r else 0} |")
    L.append("")

    bad = [(t["fmt"], x) for t in table if t["row"] for x in t["row"]["bl_err"] + t["row"]["rt_bad"]]
    L += ["## Bilingua errors and round-trip failures on these files", ""]
    if bad:
        L += ["| Format | File and problem |", "|---|---|"] + [f"| {f} | {x} |" for f, x in bad]
    else:
        L.append("None: every file Bilingua claims opened, merged unchanged to the same file and read back after a pseudo-translated merge "
                 "(bilingual formats from `core/catcore`: unchanged merge only, there is no pseudo command).")
    L.append("")

    L += ["## Files read", ""]
    nf = Counter(o for _, o in files)
    verify = sorted((R.LOCOS / "tests").glob("verify_*filter*.py")) if (R.LOCOS / "tests").is_dir() else []
    L.append(f"{len(files)} files: {nf['corpus']} corpus, {nf['fixture']} in `local-tms/tests/fixtures/formats/`, {nf['generated']} written by "
             f"its plain-script generators (stdlib `make_*.py`, to a scratch folder), {nf['sample']} in `local-tms/sample-docs/`. "
             f"The {len(verify)} `verify_*filter*.py` suites build their inputs inside the suite and import the application, so they were "
             "not run; the top-level files of `tests/fixtures/` are UI contracts, not localization files, and are not read.")
    L.append("")
    L += ["| LocOS format | LocOS fixture file names |", "|---|---|"]
    for t in sorted(table, key=lambda t: t["fmt"]):
        r = t["row"]
        names = [n for n, o in (r["files"] if r else []) if o != "corpus"]
        if names:
            L.append(f"| {t['fmt']} | {', '.join(names)} |")
    L.append("")

    from scoreboard_text import TEXT

    L.append(TEXT)
    return "\n".join(L) + "\n"
