"""Compare Bilingua's parsers with the Localization OS Python filters on the same files.

    python run.py [--build] [--out REPORT.md] [--external] [EXTRA_FILE ...]

* Bilingua side: `bl-dump` (crate bilingua-oracle) prints the units and runs Bilingua's own round-trip checks.
* LocOS side: `locos_dump.py` run with the LocOS venv Python. LocOS is only imported, never edited or started.
* Files: everything under `corpus/` (made up, committed) plus, with --external, files that live only in the
  LocOS / corpus-data trees (read in place, never copied, never committed).

Texts are normalised (NFC, whitespace collapsed, every inline code replaced by one marker) before comparing.
Decision D16: LocOS is an oracle, not a dependency. Python 3.9+, stdlib only.
"""

import difflib
import json
import os
import re
import subprocess
import sys
import tempfile
import unicodedata
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
LOCOS = Path(os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms"))
LOCOS_PY = LOCOS / ".venv" / "Scripts" / "python.exe"
BL = ROOT / "target" / "debug" / ("bl-dump.exe" if os.name == "nt" else "bl-dump")
if not BL.exists():
    BL = HERE.parent / "target" / "debug" / "bl-dump.exe"

FORMATS = {
    "docx": "DOCX", "docm": "DOCX", "dotx": "DOCX", "xlsx": "XLSX", "xlsm": "XLSX", "pptx": "PPTX",
    "html": "HTML", "htm": "HTML", "md": "Markdown", "markdown": "Markdown", "json": "JSON", "po": "PO",
    "pot": "PO", "srt": "SRT", "vtt": "VTT", "txt": "TXT",
}
ORDER = ["DOCX", "XLSX", "PPTX", "HTML", "Markdown", "JSON", "PO", "SRT", "VTT", "TXT"]
CODE = "\u00a4"
LINE_BASED = {"txt": True, "md": True, "markdown": True}
LOCOS_TOKEN = re.compile("\u27e6/?[a-z]?\\d+\u27e7")
BL_TAGS = re.compile("[" + chr(0xE010) + chr(0xE011) + chr(0xE012) + "]")  # private-use markers from bl-dump


def norm(s: str) -> str:
    s = unicodedata.normalize("NFC", s).replace("\u00a0", " ")
    return re.sub(r"\s+", " ", s).strip()


def norm_bl(text: str):
    t = BL_TAGS.sub(CODE, text)
    return norm(t.replace(CODE, "\u0001")).replace("\u0001", CODE)


def norm_lo(text: str):
    t = LOCOS_TOKEN.sub("\u0001", text)
    return norm(t).replace("\u0001", CODE)


def strip_codes(s: str) -> str:
    return norm(s.replace(CODE, " "))


MD_NOISE = re.compile(r"`[^`]*`|</?[A-Za-z][^>]*>|[*_|#>\~]+|^\s*[-+]\s|\s-{3,}\s|\s:?-{2,}:?(?=\s)", re.M)


def words_of(text: str, markdown: bool):
    """Words for the line-based comparison. For Markdown, LocOS keeps the markup in its units while Bilingua turns
    it into tags, so emphasis marks, table pipes, HTML tags and code spans (not translated by Bilingua) are dropped."""
    text = text.replace(CODE, " ")
    if markdown:
        text = MD_NOISE.sub(" ", text)
    # Bilingua leaves cells and lines that hold only numbers or symbols untranslated (deliberate), so tokens
    # without a letter are not compared.
    return [w for w in norm(text).split() if any(c.isalpha() for c in w)]


def squash(s: str) -> str:
    """Text with codes and every kind of white space removed (for 'differs only in spaces')."""
    return re.sub(r"\s+", "", s.replace(CODE, ""))


def run_bl(path: Path) -> dict:
    try:
        p = subprocess.run([str(BL), str(path)], capture_output=True, timeout=300)
        return json.loads(p.stdout.decode("utf-8") or "{}") or {"error": p.stderr.decode("utf-8", "replace")[:300]}
    except Exception as e:  # noqa: BLE001
        return {"error": f"harness: {e}"}


def run_locos(paths) -> dict:
    out = Path(tempfile.mkdtemp(prefix="oracle-")) / "locos.json"
    subprocess.run([str(LOCOS_PY), str(HERE / "locos_dump.py"), str(out), *map(str, paths)], check=True,
                   capture_output=True, timeout=1200)
    return json.loads(out.read_text(encoding="utf-8"))


def align(bl, lo):
    """Pairs units of both sides. Returns (pairs, missing_in_bilingua, extra_in_bilingua)."""
    a = [strip_codes(x[0]) for x in bl]
    b = [strip_codes(x[0]) for x in lo]
    pairs, missing, extra = [], [], []
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            pairs += [(bl[i1 + k], lo[j1 + k]) for k in range(i2 - i1)]
        elif op == "replace":
            xs, ys = list(range(i1, i2)), list(range(j1, j2))
            # greedy best-ratio pairing keeps order
            used = set()
            for i in xs:
                best, bj = 0.0, None
                for j in ys:
                    if j in used:
                        continue
                    r = difflib.SequenceMatcher(None, a[i], b[j]).ratio()
                    if r > best:
                        best, bj = r, j
                if bj is not None and best >= 0.5:
                    used.add(bj)
                    pairs.append((bl[i], lo[bj]))
                else:
                    extra.append(bl[i])
            missing += [lo[j] for j in ys if j not in used]
        elif op == "delete":
            extra += bl[i1:i2]
        else:
            missing += lo[j1:j2]
    return pairs, missing, extra


def short(s: str, n=70) -> str:
    s = s.replace("|", "\\|").replace("\n", " ")
    return s if len(s) <= n else s[: n - 1] + "..."


def compare(path: Path, bl: dict, lo: dict) -> dict:
    r = {"file": path.name, "bl_error": bl.get("error"), "lo_error": lo.get("error"), "issues": []}
    r["unchanged"] = bl.get("unchanged", "n/a")
    r["pseudo"] = (bl.get("pseudo") or {}).get("status", "n/a")
    r["pseudo_first"] = (bl.get("pseudo") or {}).get("first")
    if bl.get("error") or lo.get("error"):
        return r
    bu = [(norm_bl(u["text"]), u["id"]) for u in bl["units"] if not u["locked"]]
    bu = [x for x in bu if strip_codes(x[0])]
    lu = [(norm_lo(u["text"]), u["i"]) for u in lo["units"]]
    lu = [x for x in lu if strip_codes(x[0])]
    r["bl_n"], r["lo_n"] = len(bu), len(lu)
    if LINE_BASED.get(path.suffix[1:].lower()):
        # Unit boundaries are a design choice here (Bilingua: paragraphs, LocOS: lines). Compare the words.
        md = path.suffix[1:].lower() in ("md", "markdown")
        a = words_of(" ".join(x[0] for x in bu), md)
        b = words_of(" ".join(x[0] for x in lu), md)
        r["pairs"] = "words"
        sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
        for op, i1, i2, j1, j2 in sm.get_opcodes():
            if op == "equal":
                continue
            kind = {"delete": "extra words in Bilingua", "insert": "words missing in Bilingua"}.get(op, "different words")
            r["issues"].append((kind, f"B: {short(' '.join(a[i1:i2]), 45)} / L: {short(' '.join(b[j1:j2]), 45)}", f"word {i1}"))
        return r
    pairs, missing, extra = align(bu, lu)
    r["pairs"] = len(pairs)
    for m in missing:
        r["issues"].append(("missing in Bilingua", short(m[0]), f"LocOS #{m[1]}"))
    for e in extra:
        r["issues"].append(("extra in Bilingua", short(e[0]), f"Bilingua {e[1]}"))
    for x, y in pairs:
        if strip_codes(x[0]) != strip_codes(y[0]) and squash(x[0]) == squash(y[0]):
            r["issues"].append(("spacing only", f"B: {short(x[0], 50)} / L: {short(y[0], 50)}", f"{x[1]} / #{y[1]}"))
        elif strip_codes(x[0]) != strip_codes(y[0]):
            r["issues"].append(("different text", f"B: {short(x[0], 50)} / L: {short(y[0], 50)}", f"{x[1]} / #{y[1]}"))
        elif x[0].count(CODE) != y[0].count(CODE):
            r["issues"].append(("code count", f"B {x[0].count(CODE)} / L {y[0].count(CODE)}: {short(x[0], 50)}", f"{x[1]} / #{y[1]}"))
    return r


def collect(extra_files, external):
    files = [p for p in sorted((HERE / "corpus").rglob("*")) if p.is_file() and p.suffix[1:].lower() in FORMATS]
    if external:
        files += external_files()
    files += [Path(f) for f in extra_files]
    return files


def external_files():
    """Files read in place from other trees. They are never copied into the repository."""
    out = []
    sd = LOCOS / "sample-docs"
    if sd.is_dir():
        out += [p for p in sorted(sd.iterdir()) if p.suffix[1:].lower() in FORMATS]
    fx = LOCOS / "tests" / "fixtures" / "formats"
    for sub in ("pptx", "xlsx"):
        out += generated(fx / sub, sub)
    for f in ((fx / "mdx" / "guide.mdx"),):
        pass
    cd = Path(r"C:\Users\brent\code\corpus-data\demodoc")
    if cd.is_dir():
        out += [p for p in sorted(cd.glob("*.md"))][:6]
    return out


def generated(folder: Path, kind: str):
    """Runs a LocOS fixture generator (a stdlib script) and writes its packages to a scratch folder."""
    if not folder.is_dir():
        return []
    scratch = Path(tempfile.gettempdir()) / "oracle-generated" / kind
    scratch.mkdir(parents=True, exist_ok=True)
    code = (
        "import sys, runpy, pathlib\n"
        f"ns = runpy.run_path(r'{folder}\\make_{'pptx' if kind == 'pptx' else 'xlsx_objects'}.py')\n"
        f"fn = ns.get('make_pptx') or ns.get('make_workbook')\n"
        f"pathlib.Path(r'{scratch}\\generated_{kind}.{kind}').write_bytes(fn())\n"
    )
    try:
        subprocess.run([sys.executable, "-I", "-c", code], check=True, capture_output=True, timeout=60)
    except Exception:  # noqa: BLE001
        return []
    return sorted(scratch.glob(f"*.{kind}"))


def report(results, locos_ok) -> str:
    by = defaultdict(list)
    for r in results:
        by[FORMATS[r["file"].rsplit(".", 1)[-1].lower()]].append(r)
    L = ["# Bilingua vs Localization OS filters", "",
         "Generated by `bilingua/oracle/run.py`. Localization OS is an oracle only (decision D16): its filters were",
         "imported read-only to extract the same files. Texts are normalised (NFC, whitespace collapsed, each inline",
         "code replaced by one marker) before they are compared. `unchanged` = merge with nothing translated;",
         "`pseudo` = every unit pseudo-translated, merged, reopened.", ""]
    tot = defaultdict(int)
    for fmt in ORDER:
        rs = by.get(fmt)
        if not rs:
            continue
        L += [f"## {fmt}", "", "| File | Bilingua units | LocOS units | Matched | Differences | Unchanged merge | Pseudo round trip |",
              "|---|---|---|---|---|---|---|"]
        for r in rs:
            err = r["bl_error"] or r["lo_error"]
            if err:
                L.append(f"| {r['file']} | - | - | - | {'Bilingua: ' + short(r['bl_error']) if r['bl_error'] else 'LocOS: ' + short(r['lo_error'])} | {r['unchanged']} | {short(r['pseudo'])} |")
            else:
                L.append(f"| {r['file']} | {r['bl_n']} | {r['lo_n']} | {r['pairs']} | {len(r['issues'])} | {short(r['unchanged'], 60)} | {short(r['pseudo'], 60)} |")
            tot["files"] += 1
            tot["issues"] += len(r["issues"])
            tot["rt_bad"] += (r["unchanged"] not in ("identical", "identical-parts")) + (r["pseudo"] != "ok")
        L.append("")
        for r in rs:
            if r["issues"] or r.get("pseudo_first"):
                L += [f"**{r['file']}**", "", "| Kind | Detail | Where |", "|---|---|---|"]
                for k, d, w in r["issues"][:40]:
                    L.append(f"| {k} | {d} | {w} |")
                if len(r["issues"]) > 40:
                    L.append(f"| ... | {len(r['issues']) - 40} more | |")
                if r.get("pseudo_first"):
                    L.append(f"| pseudo round trip | {short(r['pseudo_first'], 100)} | |")
                L.append("")
    L.insert(7, f"Files: {tot['files']}. Comparison differences listed: {tot['issues']}. Round-trip failures: {tot['rt_bad']}.\n")
    return "\n".join(L) + "\n"


def main():
    args = sys.argv[1:]
    out = HERE / "REPORT.md"
    external = "--external" in args
    if "--out" in args:
        out = Path(args[args.index("--out") + 1])
    extra = [a for a in args if not a.startswith("--") and Path(a).is_file() and a != str(out)]
    if "--build" in args:
        subprocess.run(["cargo", "build", "-p", "bilingua-oracle"], cwd=ROOT, check=True)
    files = collect(extra, external)
    lo = run_locos(files) if LOCOS_PY.exists() else {}
    results = []
    for f in files:
        key = str(f)
        results.append(compare(f, run_bl(f), lo.get(key) or {"error": "LocOS not available"}))
    out.write_text(report(results, bool(lo)), encoding="utf-8", newline="\n")
    print(f"wrote {out}: {len(results)} files")


main()
