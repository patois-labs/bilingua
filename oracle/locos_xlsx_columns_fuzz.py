"""Random differential for XLSX column mode against LocOS: run with the Localization OS venv Python, read-only.

    <local-tms>/.venv/Scripts/python.exe -I oracle/locos_xlsx_columns_fuzz.py [seed] [count]

Builds `count` random workbooks from `seed` (default 1206, 40): header rows naming locales or roles, shared and
inline strings, ints and floats in every spelling, booleans, date / time / duration number formats, ISO dates,
formulas and shared-formula groups, merged areas, cells without an address, gaps, a second sheet. Each is parsed by
LocOS with one of a few column mappings and built for one target; the result goes into
`bilingua-xlsx/tests/data/modes/` as `fuzz_<n>.*` in the format `locos_xlsx_modes.py` writes, and
`tests_modes.rs` (`column_mode_random_workbooks_match_locos`) checks every one. Workbooks LocOS refuses are kept
too: Bilingua must refuse them as well.
"""

import json
import os
import random
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import locos_xlsx_modes as m  # noqa: E402

STYLES = (
    m.DECL + f'<styleSheet xmlns="{m.SML}"><numFmts count="3">'
    '<numFmt numFmtId="164" formatCode="yyyy-mm-dd"/><numFmt numFmtId="165" formatCode="[hh]:mm"/>'
    '<numFmt numFmtId="166" formatCode="&quot;day&quot; 0.0"/></numFmts>'
    '<cellXfs count="7"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="165"/><xf numFmtId="14"/>'
    '<xf numFmtId="21"/><xf numFmtId="166"/><xf numFmtId="2"/></cellXfs></styleSheet>'
)
WORDS = ["Hello", "Save file", "Cancel", "Open the door", "Zwei", "日本語", "Ünïcödé", "x", "12", "=SUM(A1)", " Padded ", "a&amp;b", "Total:"]
NUMBERS = ["1", "007", "-0", "1.0", "2.5", "1e20", "1E-7", "0.0001", "123456789012345678", "1.5e16", "3.14159", "45000.5", "0.25", "-1.25", "60", "59.5", "1e10", "2958466"]


def cell(rng, ref, sst, kind=None, with_r=True):
    r = f' r="{ref}"' if with_r else ""
    kind = kind or rng.choice(["inline", "shared", "num", "num", "date", "bool", "iso", "formula", "str", "err", "empty"])
    w = rng.choice(WORDS)
    if kind == "inline":
        return f'<c{r} t="inlineStr"><is><t xml:space="preserve">{w}</t></is></c>'
    if kind == "shared":
        if w not in sst:
            sst.append(w)
        return f'<c{r} t="s"><v>{sst.index(w)}</v></c>'
    if kind == "num":
        return f'<c{r}><v>{rng.choice(NUMBERS)}</v></c>'
    if kind == "date":
        return f'<c{r} s="{rng.randint(1, 6)}"><v>{rng.choice(NUMBERS)}</v></c>'
    if kind == "bool":
        return f'<c{r} t="b"><v>{rng.choice(["0", "1", "2"])}</v></c>'
    if kind == "iso":
        return f'<c{r} t="d"><v>{rng.choice(["2024-02-29", "2024-01-05T10:30:00", "12:30:15.5", "PT1H30M", "2024-01-05T08:00Z"])}</v></c>'
    if kind == "formula":
        return f'<c{r}><f>{rng.choice(["A2&amp;B2", "SUM(A1:B3)", "UPPER(A2)", "1+2"])}</f><v>0</v></c>'
    if kind == "str":
        return f'<c{r} t="str"><v>{w}</v></c>'
    if kind == "err":
        return f'<c{r} t="e"><v>#N/A</v></c>'
    return f"<c{r}/>"


HEADERS = [["en", "de", "Notes", "ID"], ["Source", "Translation", "Comment", "Key"], ["en-US", "de-DE", "Notes", "fr"]]
MAPPINGS = [
    "source=en,target.de=de,context=Notes",
    "source=1,target=2,context=3,key=4",
    "header=auto",
    "source=en-US,target.de-DE=de-DE,context=Notes",
    "source=1,target=6,context=3,header=yes",
]


def workbook(rng):
    sst = []
    sheets = []
    for n in range(rng.choice([1, 1, 2])):
        head = rng.choice(HEADERS)
        order = list(range(4))
        if rng.random() < 0.3:
            rng.shuffle(order)
        rows = [m.row(1, *[m.s(f"{m_col(i + 1)}1", head[order[i]]) for i in range(4)])]
        shared_si = 0
        masters = {}  # column -> its shared groups (a dependent outside its master's column is mostly out of range)
        r = 2
        for _ in range(rng.randint(2, 9)):
            r += rng.choice([1, 1, 1, 2])
            cells = []
            explicit = rng.random() > 0.1
            for col in range(1, rng.randint(2, 6)):
                ref = f"{m_col(col)}{r}"
                role = head[order[col - 1]] if col <= 4 else ""
                if role in ("en", "Source", "en-US") and rng.random() < 0.7:
                    cells.append(cell(rng, ref, sst, rng.choice(["inline", "shared", "str"]), explicit))
                elif rng.random() < 0.12:
                    # a shared formula group down this column
                    cells.append(f'<c r="{ref}" t="str"><f t="shared" ref="{ref}:{m_col(col)}{r + 3}" si="{shared_si}">{rng.choice(["UPPER(A" + str(r) + ")", "$A$1&amp;B" + str(r), "A" + str(r) + "  +  1", "SUM(A:A)"])}</f><v>X</v></c>')
                    masters.setdefault(col if rng.random() < 0.9 else 0, []).append(shared_si)
                    shared_si += 1
                elif rng.random() < 0.3 and masters.get(col):
                    cells.append(f'<c r="{ref}" t="str"><f t="shared" si="{rng.choice(masters[col])}"/><v>Y</v></c>')
                else:
                    cells.append(cell(rng, ref, sst, None, explicit))
            rows.append(m.row(r, *cells))
        body = m.sheet(*rows)
        if rng.random() < 0.3:
            a = rng.randint(2, r)
            c = rng.randint(1, 3)
            body += f'<mergeCells count="1"><mergeCell ref="{m_col(c)}{a}:{m_col(c + rng.randint(0, 1))}{a + rng.randint(0, 2)}"/></mergeCells>'
        sheets.append((f"S{n + 1}", body, []))
    extra = {"xl/styles.xml": STYLES} if rng.random() < 0.95 else {}
    if extra and rng.random() < 0.2:
        extra["xl/workbook.xml"] = None  # 1904 epoch: patched below
    return sheets, sst, extra


def m_col(n):
    out = ""
    while n:
        n, rem = divmod(n - 1, 26)
        out = chr(65 + rem) + out
    return out


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 1206
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 60
    rng = random.Random(seed)
    filt = m.get_filter("xlsx")
    for old in os.listdir(m.OUT):
        if old.startswith("fuzz_"):
            os.remove(os.path.join(m.OUT, old))
    summary = {"ok": 0, "refused": 0}
    for k in range(count):
        sheets, sst, extra = workbook(rng)
        mac = "xl/workbook.xml" in extra
        extra.pop("xl/workbook.xml", None)
        data = m.workbook(sheets, sst=sst or None, extra=extra)
        if mac:
            import io
            import zipfile

            src = zipfile.ZipFile(io.BytesIO(data))
            buf = io.BytesIO()
            with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
                for zi in src.infolist():
                    body = src.read(zi.filename)
                    if zi.filename == "xl/workbook.xml":
                        body = body.replace(b"<sheets>", b'<workbookPr date1904="1"/><sheets>')
                    zf.writestr(zi, body)
            data = buf.getvalue()
        name = f"fuzz_{k:03}"
        parse = {"columns": rng.choice(MAPPINGS), "source_locale": rng.choice(["en", "en-US"]), "target_locales": rng.choice([["de"], ["de-DE"], ["de", "fr"]])}
        locale = rng.choice(parse["target_locales"])
        with open(os.path.join(m.OUT, name + ".xlsx"), "wb") as fh:
            fh.write(data)
        info = {"parse": parse, "units": [], "error": False, "builds": []}
        try:
            res = filt.parse(data, **parse)
        except Exception as exc:
            info["error"] = True
            info["message"] = f"{type(exc).__name__}: {exc}"
            res = None
        if res is not None:
            sk = {u["p_index"]: u for u in res.skeleton["units"]}
            info["units"] = [{"p_index": u.p_index, "text": u.text, "style": u.style or "", "note": sk[u.p_index].get("note", "")} for u in res.units]
            mapping = {str(u["p_index"]): (u["text"] if rng.random() < 0.2 else "T " + u["text"]) for u in info["units"]}
            try:
                out = filt.build_localized(data, mapping, locale, table=res.skeleton.get("table"))
                fname = f"{name}.b.xlsx"
                with open(os.path.join(m.OUT, fname), "wb") as fh:
                    fh.write(out)
                info["builds"].append({"name": "b", "locale": locale, "map": mapping, "file": fname})
            except Exception as exc:
                info["build_error"] = f"{type(exc).__name__}: {exc}"
        summary["refused" if info["error"] else "ok"] += 1
        with open(os.path.join(m.OUT, name + ".json"), "w", encoding="utf-8") as fh:
            json.dump(info, fh, ensure_ascii=False, indent=1)
    print(summary)


if __name__ == "__main__":
    main()
