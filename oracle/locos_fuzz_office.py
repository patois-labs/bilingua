"""Random differential corpus for the LocOS profile, Track D group B: the XLIFF family (xliff, sdlxliff, mxliff,
mqxliff and the xlz, mqxlz, sdlrpx packages), HTML, DOCX, PPTX and XLSX. Run with the Localization OS venv Python,
read-only:

    <local-tms>/.venv/Scripts/python.exe oracle/locos_fuzz.py OUT_DIR [--n 100] [--seed 1] docx pptx xlsx ...

`locos_fuzz.py` takes these formats from here. Each file is a mutation of a random input of
`bilingua/tests/locos/<format>/` (the golden corpus, so every LocOS path it reaches is a real one): in every XML
part (every part of a zip package but the relationship and content-type parts) text is replaced with random
phrases (scripts, white space at the ends, placeholders, markup-looking text, forged LocOS tokens), elements are
duplicated, dropped or emptied, and white space is put between elements; HTML is mutated as text (text between
tags, inline tags, entities, comments). The golden next to each file has the usual builds plus `inject`: every unit
translated to hostile text (closing tags of the format, CDATA ends, forged and shuffled tokens, controls,
new lines). Zip goldens keep their rewritten entries in `<file>.golden.zip`, as `locos_golden.py` writes them.
"""

import copy
import hashlib
import io
import json
import os
import random
import re
import zipfile

from lxml import etree

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(os.path.dirname(HERE), "bilingua", "tests", "locos")
FORMATS = ["xliff", "sdlxliff", "mxliff", "mqxliff", "xlz", "mqxlz", "sdlrpx", "html", "docx", "pptx", "xlsx"]
TOKEN = re.compile("(\u27e6/?[a-z]?\\d+\u27e7)")
# elements worth duplicating or dropping (local names): paragraphs, runs, cells, units, inline codes
STRUCT = {"p", "r", "t", "br", "fld", "tab", "si", "c", "row", "tr", "tc", "sp", "hyperlink", "ins", "del",
          "trans-unit", "unit", "segment", "source", "target", "seg-source", "mrk", "g", "x", "bx", "ex", "ph",
          "bpt", "ept", "it", "pc", "sc", "ec", "group", "sdt", "smartTag", "fldSimple", "rPr", "pPr"}
SCRIPTS = ["Save the file", "Größe ändern", "日本語のテキスト", "مرحبا بالعالم", "שלום עולם", "Привет", "한국어",
           "e\u0301t\u00e9", "x\u00a0y", "zero\u200bwidth", "emoji \U0001F600", "tab\there", "a&b <c> \"d\" 'e'",
           "{0} files", "%1$s of %2$d", "{{name}}", "<b>not a tag</b>", "]]>", "&amp;", "C:\\Temp\\x", "50%"]
# Source text that looks like LocOS tokens: off by default (LocOS reads it as tokens and drops or moves it on
# export, see the gate report); LOCOS_FUZZ_FORGED=0.08 puts it back.
FORGED_P = float(os.environ.get("LOCOS_FUZZ_FORGED", "0"))
FORGED = ["\u27e6r1\u27e7", "\u27e6/r1\u27e7", "\u27e6x2\u27e7", "\u27e6g1\u27e7x\u27e6/g1\u27e7", "\u27e6p9\u27e7",
          "\u27e6", "\u27e7", "\u27e699\u27e7"]
INJECT = ["</w:t></w:r><w:r><w:t>evil", "</a:t></a:r><a:r><a:t>evil", "</t></si><si><t>evil", "</target><target>x",
          "</source></trans-unit><trans-unit id=\"evil\"><source>x", "]]>", "<![CDATA[x]]>", "<!-- x", "-->",
          "<?pi x?>", "&amp;&lt;&#0;", "&#x1;", "\u000b", "\u0001\u0002", "\u000c", "\ufffe", "a\nb", "a\r\nb",
          "\t", "  lead and trail  ", "", " ", "\u2028", "<script>alert(1)</script>", "<b>bold", "</p><p>x",
          "\" onmouseover=\"x", "\u27e6r99\u27e7orphan\u27e6/r99\u27e7", "\u27e6x1\u27e7\u27e6x1\u27e7", "\u27e6/r1\u27e7",
          "مرحبا", "\U0001F600" * 3, "x" * 300]


def _phrase(R):
    parts = []
    for _ in range(R.randint(1, 4)):
        r = R.random()
        parts.append(R.choice(FORGED) if r < FORGED_P else R.choice(SCRIPTS))
    s = " ".join(parts)
    r = R.random()
    if r < 0.1:
        s = "  " + s
    elif r < 0.2:
        s += " "
    elif r < 0.25:
        s = "\n" + s + "\n"
    elif r < 0.28:
        s = " "
    return s


def _local(el):
    t = el.tag
    return t.rsplit("}", 1)[-1] if isinstance(t, str) else ""


def _mutate_tree(root, R):
    els = [e for e in root.iter() if isinstance(e.tag, str)]
    for e in els:
        # cell values and formulas mostly stay (a text in a number cell is a broken workbook to openpyxl); the
        # package dates always do (openpyxl refuses a workbook whose core.xml date is not ISO: a known difference)
        if _local(e) in ("v", "f") and R.random() < 0.95 or _local(e) in ("created", "modified", "lastPrinted"):
            continue
        if e.text and e.text.strip() and R.random() < 0.5:
            e.text = _phrase(R)
        if e.tail and e.tail.strip() and R.random() < 0.3:
            e.tail = _phrase(R)
    cands = [e for e in els if e.getparent() is not None and _local(e) in STRUCT]
    for _ in range(R.randint(0, 3)):
        if not cands:
            break
        e = R.choice(cands)
        if e.getparent() is None:
            continue
        op = R.random()
        if op < 0.45:
            e.addnext(copy.deepcopy(e))
        elif op < 0.65:
            e.getparent().remove(e)
            cands = [c for c in cands if c is not e]
        elif op < 0.8:
            for k in list(e):
                e.remove(k)
            e.text = R.choice(["", " ", None, _phrase(R)])
        else:
            # pretty-printing white space between elements (lxml keeps it as text and tails)
            e.tail = (e.tail or "") + R.choice(["\n  ", "\n", " ", "\t"])
    return root


def _mutate_xml(data, R):
    parser = etree.XMLParser(resolve_entities=False, remove_blank_text=False, huge_tree=False)
    try:
        root = etree.fromstring(data, parser)
    except etree.XMLSyntaxError:
        return data
    _mutate_tree(root, R)
    head = data[:200]
    standalone = b'standalone="yes"' in head or b"standalone='yes'" in head
    decl = head.lstrip().startswith(b"<?xml")
    return etree.tostring(root, xml_declaration=decl, encoding="UTF-8", standalone=True if standalone else None)


def _mutate_html(text, R):
    def text_sub(m):
        if not m.group(1).strip() or R.random() < 0.4:
            return m.group(0)
        s = _phrase(R).replace("&", "&amp;").replace("<", "&lt;")
        r = R.random()
        if r < 0.15:
            s = f"{s} <b>{_phrase(R)}</b>".replace("& ", "&amp; ")
        elif r < 0.25:
            s = f"{s}<br>{R.choice(['&nbsp;', '&copy;', '&#x27E6;r1&#x27E7;', '&amp;'])}"
        elif r < 0.3:
            s = f"<i>{s}"  # an inline tag never closed
        elif r < 0.33:
            s = f"{s}<!-- note -->"
        elif r < 0.36:
            s = f'<a href="https://e.com/?a=1&amp;b=2" title="{_phrase(R).replace(chr(34), "&quot;")}">{s}</a>'
        return ">" + s + "<"
    return re.sub(r">([^<>]+)<", text_sub, text)


def _mutate_zip(data, R):
    out = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(data)) as zin, zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            b = zin.read(info)
            n = info.filename
            low = n.lower()
            if not (low.endswith(".rels") or low.endswith("[content_types].xml")) and b.lstrip()[:1] == b"<":
                if R.random() < 0.85:
                    b = _mutate_xml(b, R)
            zi = zipfile.ZipInfo(n, (1980, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(zi, b)
    return out.getvalue()


def templates(fmt):
    d = os.path.join(CORPUS, fmt)
    return sorted(os.path.join(d, n) for n in os.listdir(d) if not n.endswith((".golden.json", ".golden.zip", ".maps.json")))


def make(fmt, R):
    """(bytes, extension) of one mutated input of `fmt`."""
    src = R.choice(templates(fmt))
    ext = src.rsplit(".", 1)[-1]
    with open(src, "rb") as fh:
        data = fh.read()
    if data.startswith(b"PK"):
        return _mutate_zip(data, R), ext
    if fmt == "html":
        enc = "utf-8"
        text = data.decode(enc, "replace")
        return _mutate_html(text, R).encode(enc), ext
    return _mutate_xml(data, R), ext


# Targets with tag damage (a token twice, a token the unit does not have): off by default, since LocOS's export
# gate (`export_gate`, tag damage) keeps such a segment from the build and the adapter refuses it the same way;
# LOCOS_FUZZ_DAMAGE=1 puts them back.
DAMAGE = os.environ.get("LOCOS_FUZZ_DAMAGE") == "1"


def _shuffle_tokens(text, R):
    toks = TOKEN.findall(text)
    clean = [x for x in INJECT if DAMAGE or not TOKEN.search(x)]
    if not toks:
        return R.choice(clean)
    r = R.random()
    if r < 0.4:
        R.shuffle(toks)
    elif r < 0.6:
        toks = toks[: R.randint(0, len(toks))]
    elif DAMAGE:
        toks = toks + [R.choice(toks)]
    return R.choice(SCRIPTS) + "".join(t + R.choice(["", " x ", R.choice(clean)]) for t in toks)


def inject_map(units, R):
    pool = INJECT if DAMAGE else [x for x in INJECT if not TOKEN.search(x)]
    return {str(u["p_index"]): (_shuffle_tokens(u["text"], R) if R.random() < 0.4 else R.choice(pool)) for u in units}


def golden(og, fmt, path, R):
    """The golden for one fuzz input (with the `inject` build), and the blobs of its rewritten zip entries."""
    blobs = {}
    try:
        g = og.golden(fmt, path, blobs)
    except Exception as exc:  # LocOS refuses the file
        return {"format": fmt, "parse_error": f"{type(exc).__name__}: {exc}"}, {}
    mapping = inject_map(g["units"], R)
    loc = R.choice(["de-DE", "ar-SA", "ja-JP"])
    with open(path, "rb") as fh:
        data = fh.read()
    filt = og.get_filter(fmt)
    original = dict(og.zip_entries(data) or [])
    try:
        out = filt.build_localized(data, mapping, loc)
        entries = og.zip_entries(out) if original else None
        b = {"name": "inject", "locale": loc, "map": mapping}
        if entries is None:
            import base64

            b["out_b64"] = base64.b64encode(out).decode()
        else:
            z = []
            for n, x in entries:
                if original.get(n) == x:
                    z.append({"name": n, "same": True})
                else:
                    sha = hashlib.sha1(x).hexdigest()
                    blobs[sha] = x
                    z.append({"name": n, "sha1": sha})
            b["zip"] = z
        g["builds"].append(b)
    except Exception as exc:
        g["builds"].append({"name": "inject", "locale": loc, "map": mapping, "error": f"{type(exc).__name__}: {exc}"})
    return g, blobs


def write_corpus(og, out, fmts, n, seed):
    for fmt in fmts:
        d = os.path.join(out, fmt)
        os.makedirs(d, exist_ok=True)
        refused = 0
        for k in range(n):
            R = random.Random(f"{seed}-{fmt}-{k}")
            data, ext = make(fmt, R)
            p = os.path.join(d, f"r{seed}_{k:04d}.{ext}")
            with open(p, "wb") as fh:
                fh.write(data)
            g, blobs = golden(og, fmt, p, R)
            refused += "parse_error" in g
            with open(p + ".golden.json", "w", encoding="utf-8", newline="\n") as fh:
                json.dump(g, fh, ensure_ascii=False)
            side = p + ".golden.zip"
            if blobs:
                with zipfile.ZipFile(side, "w") as zf:
                    for sha in sorted(blobs):
                        zf.writestr(zipfile.ZipInfo(sha, (1980, 1, 1, 0, 0, 0)), blobs[sha], zipfile.ZIP_DEFLATED, 6)
        print(fmt, n, "files", refused, "refused by LocOS", flush=True)
