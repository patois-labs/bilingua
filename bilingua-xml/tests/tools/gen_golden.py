"""Builds tests/data/locos_golden.json: XML documents and what the LocOS generic XML filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For every document the golden holds the bytes (text, or hex when not UTF-8), the units LocOS parses (text, style, note, or the refusal),
and the bytes LocOS builds for two mappings ("all": every unit translated, "third": every third unit), each in place (`build`) and for a target language (`build_localized`).
Random documents are deterministic (seed 1104): plain XML with ITS rules and local ITS markup, DITA-like and
DocBook-like roots, multilingual layouts.
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-xml-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Hello, world", "Fish &amp; chips", "1 &lt; 2", "caf&#233;", "&#x65E5;&#x672C;",
    "100%", "{0} items", "%s of %d", "{{name}}", "Don't", "say &quot;hi&quot;", "x", "A longer sentence with several words.",
    "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "tab\there", "emoji \U0001f600",
]
NAMES = ["p", "item", "title", "note", "desc", "label", "msg", "entry", "section", "para", "cmd", "step"]
INLINE = ["b", "i", "em", "span", "code", "a"]
ATTRS = ["title", "alt", "label", "class", "href", "id", "name"]
SELECTORS = [
    "//b", "//code", "//p/@title", "//*[@class='x']", "//note", "//item[title]", "//a/@href", "//p[1]",
    "//span[not(@x)]", "//*[contains(@class,'n')]", "//desc/@label", "//section//note", "//*[@id]",
    "//item/@alt", "//entry[last()]", "//msg[position()>1]", "//p | //para", "//title/following-sibling::*" if False else "//title",
]


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;")


def text(rng, ws=True):
    t = rng.choice(WORDS)
    if ws and rng.random() < 0.3:
        t = rng.choice(["  ", "\n  ", "\t"]) + t + rng.choice(["  ", "\n", " "])
    return t


def element(rng, depth, its):
    name = rng.choice(NAMES if depth < 2 else INLINE + NAMES[:3])
    attrs = ""
    for a in rng.sample(ATTRS, rng.randint(0, 2)):
        v = rng.choice(["x", "n1", "Some text", "Caf\u00e9 &amp; Co", "a b  c", "http://x/y?a=1&amp;b=2"])
        q = rng.choice(['"', "'"])
        attrs += f" {a}={q}{v}{q}"
    if its and rng.random() < 0.1:
        attrs += rng.choice([' its:translate="no"', ' its:translate="yes"', ' its:withinText="yes"', ' its:withinText="nested"', ' its:locNote="Be brief"'])
    if rng.random() < 0.06:
        attrs += ' xml:space="preserve"'
    r = rng.random()
    if r < 0.08:
        return f"<{name}{attrs}/>"
    body = []
    for _ in range(rng.randint(0, 4)):
        k = rng.random()
        if k < 0.45 or depth >= 4:
            body.append(text(rng))
        elif k < 0.7:
            body.append(element(rng, depth + 1, its))
        elif k < 0.78:
            body.append(f"<![CDATA[{rng.choice(['a < b', 'x', 'if (a&&b) {}'])}]]>")
        elif k < 0.84:
            body.append("<!-- " + rng.choice(["note", "todo"]) + " -->")
        elif k < 0.88:
            body.append("<?pi data?>")
        else:
            body.append(rng.choice(["\n    ", " ", ""]))
    return f"<{name}{attrs}>{''.join(body)}</{name}>"


def rules(rng):
    out = []
    for _ in range(rng.randint(1, 4)):
        sel = rng.choice(SELECTORS)
        k = rng.random()
        if k < 0.35:
            out.append(f'<its:translateRule selector="{sel}" translate="{rng.choice(["yes", "no"])}"/>')
        elif k < 0.6:
            out.append(f'<its:withinTextRule selector="{sel}" withinText="{rng.choice(["yes", "no", "nested"])}"/>')
        elif k < 0.8:
            out.append(f'<its:locNoteRule selector="{sel}"><its:locNote>Note {rng.randint(1, 9)}</its:locNote></its:locNoteRule>')
        else:
            out.append(f'<its:preserveSpaceRule selector="{sel}" space="{rng.choice(["preserve", "default"])}"/>')
    return "<its:rules version=\"2.0\">" + "".join(out) + "</its:rules>"


def plain_doc(rng):
    its = rng.random() < 0.6
    ns = ' xmlns:its="http://www.w3.org/2005/11/its" its:version="2.0"' if its else ""
    lang = rng.choice(["", ' xml:lang="en"', ' xml:lang="en-US"'])
    root = rng.choice(["root", "doc", "catalog", "messages"])
    kids = [element(rng, 1, its) for _ in range(rng.randint(1, 8))]
    if its and rng.random() < 0.7:
        kids.insert(0, rules(rng))
    sep = rng.choice(["\n  ", "\n", ""])
    decl = rng.choice(['<?xml version="1.0" encoding="UTF-8"?>\n', "", '<?xml version="1.0"?>\r\n'])
    dt = rng.choice(["", "", '<!DOCTYPE doc SYSTEM "doc.dtd">\n'])
    com = rng.choice(["", "<!-- header -->\n"])
    return f"{decl}{dt}{com}<{root}{ns}{lang}>{sep}{sep.join(kids)}\n</{root}>\n"


def dita_doc(rng):
    inner = []
    for _ in range(rng.randint(1, 5)):
        inner.append(rng.choice([
            f"<p>{text(rng, False)} <codeph>x {{0}}</codeph> and <b>{text(rng, False)}</b></p>",
            f"<p conref=\"a.dita#a/b\"/>",
            f"<p translate=\"no\">{text(rng, False)}</p>",
            f"<note>{text(rng, False)}<indexterm>term<indexterm>sub</indexterm></indexterm></note>",
            f"<image href=\"a.png\" alt=\"{rng.choice(['Alt one', 'Alt &amp; two'])}\"/>",
            f"<codeblock xml:space=\"preserve\">a\n  b</codeblock>",
            f"<draft-comment>{text(rng, False)}</draft-comment>",
        ]))
    root = rng.choice(["task", "concept", "topic"])
    return (
        f'<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE {root} PUBLIC "-//OASIS//DTD DITA {root.title()}//EN" "{root}.dtd">\n'
        f'<{root} id="t1"{rng.choice(["", " xml:lang=\"en-US\""])}>\n  <title>{text(rng, False)}</title>\n  <{root}body>\n    '
        + "\n    ".join(inner)
        + f"\n  </{root}body>\n</{root}>\n"
    )


def docbook_doc(rng):
    inner = []
    for _ in range(rng.randint(1, 5)):
        inner.append(rng.choice([
            f"<para>{text(rng, False)} <emphasis>{text(rng, False)}</emphasis><footnote><para>{text(rng, False)}</para></footnote></para>",
            f"<programlisting>run it</programlisting>",
            f"<remark>{text(rng, False)}</remark>",
            f"<para>See <xref linkend=\"x\"/> and <filename>/etc/a</filename>.</para>",
        ]))
    if rng.random() < 0.5:
        return (
            '<?xml version="1.0"?>\n<article xmlns="http://docbook.org/ns/docbook" version="5.0"'
            + rng.choice(["", ' xml:lang="en"']) + ">\n  <title>T</title>\n  " + "\n  ".join(inner) + "\n</article>\n"
        )
    return (
        '<?xml version="1.0"?>\n<!DOCTYPE book PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN" "docbookx.dtd">\n<book'
        + rng.choice(["", ' lang="en"']) + ">\n  <title>T</title>\n  <chapter>\n    " + "\n    ".join(inner) + "\n  </chapter>\n</book>\n"
    )


def ml_doc(rng):
    langs = rng.sample(["en", "de", "fr", "es", "ja", "ar", "zh"], rng.randint(2, 5))
    if "en" not in langs:
        langs[0] = "en"
    n = rng.randint(2, 6)
    mode = rng.choice(["attr", "attr_lang", "element"])
    rows = []
    for i in range(n):
        have = [l for l in langs if l == "en" or rng.random() < 0.7]
        if mode == "element":
            rows.append("  <g>" + "".join(f"<{l}>{text(rng, False)} <b>x</b></{l}>" for l in have) + "</g>")
        else:
            at = "xml:lang" if mode == "attr" else "lang"
            rows.append("  <g>" + "".join(f'<t {at}="{l}"{rng.choice(["", " id=\"i%d%s\"" % (i, l)])}>{text(rng, False)}</t>' for l in have) + "</g>")
    return '<?xml version="1.0"?>\n<r>\n' + "\n".join(rows) + "\n</r>\n"


def enc(b):
    """Text when the bytes are UTF-8, else "h:" and hex (keeps the golden small and readable)."""
    try:
        return "t:" + b.decode("utf-8")
    except UnicodeDecodeError:
        return "h:" + b.hex()


def units_of(pr):
    sk = {u["p_index"]: u for u in pr.skeleton["units"]}
    out = []
    for u in pr.units:
        s = sk.get(u.p_index, {})
        out.append({"text": u.text, "style": getattr(u, "style", "") or s.get("style", "") or "", "note": s.get("note", "")})
    return out


def token_map(units, kind):
    m = {}
    for i, u in enumerate(units):
        t = u["text"]
        if kind == "all":
            m[str(i)] = f"T{i} \u00e9 " + t.replace("a", "\u00e0") + " <&>"
        elif kind == "third" and i % 3 == 0:
            m[str(i)] = f"Zeta {i} \u65e5"
        elif kind == "tok":
            m[str(i)] = t + " Z"
    return m


def case(name, data):
    filt = get_filter("xml")
    rec = {"name": name, "data": enc(data)}
    try:
        pr = filt.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = f"{type(e).__name__}: {str(e)[:160]}"
        return rec
    units = units_of(pr)
    rec["units"] = units
    rec["dialect"] = pr.skeleton.get("dialect", "")
    builds = {}
    for kind in ("all", "third"):
        mp = token_map(units, kind)
        try:
            builds[kind] = enc(filt.build(data, mp))
        except Exception as e:  # noqa: BLE001
            builds[kind] = "ERR " + type(e).__name__
        for loc in (("de-DE", "fr") if rec["dialect"] == "multilingual" else ("de-DE",)):
            try:
                builds[f"{kind}@{loc}"] = enc(filt.build_localized(data, mp, loc))
            except Exception as e:  # noqa: BLE001
                builds[f"{kind}@{loc}"] = "ERR " + type(e).__name__
    rec["builds"] = builds
    return rec


def main():
    out_path, files = sys.argv[1], sys.argv[2:]
    rng = random.Random(1104)
    cases = []
    for f in files:
        with open(f, "rb") as fh:
            cases.append(case(os.path.basename(f), fh.read()))
    gens = [(plain_doc, 220), (dita_doc, 40), (docbook_doc, 40), (ml_doc, 60)]
    for fn, n in gens:
        for i in range(n):
            cases.append(case(f"{fn.__name__}_{i}", fn(rng).encode("utf-8")))
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    errs = sum(1 for c in cases if "error" in c)
    print(f"{len(cases)} cases, {errs} refused by LocOS")


if __name__ == "__main__":
    main()
