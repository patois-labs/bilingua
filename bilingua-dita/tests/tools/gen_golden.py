"""Builds tests/data/locos_golden.json: DITA and DocBook documents and what the LocOS `dita` and `docbook` filters
make of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

Each case names the LocOS filter (`dita` or `docbook`, the one its extension picks), the bytes (text, or hex when
not UTF-8), the units LocOS parses (text, style, note, or the refusal) and the bytes LocOS builds for two mappings
("all": every unit translated, "third": every third unit), in place (`build`) and for target languages
(`build_localized`). Random documents are deterministic (seed 1105): DITA topics of every type, maps and bookmaps,
DocBook 4.5 and 5, and documents whose root the generic filter would not recognise (the fixed dialect applies).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-dita-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Fish &amp; chips", "1 &lt; 2", "caf&#233;", "&#x65E5;&#x672C;", "100%",
    "{0} items", "%s of %d", "{{name}}", "Don't", "say &quot;hi&quot;", "x", "A longer sentence with several words.",
    "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "\u4e2d\u6587\u6587\u672c",
    "tab\there", "emoji \U0001f600", "  spaced   out  ", "line\nbreak",
]
DITA_INLINE = ["ph", "b", "i", "u", "tt", "sup", "sub", "keyword", "term", "uicontrol", "wintitle", "cite", "q", "tm",
               "userinput", "systemoutput", "varname", "xref", "menucascade", "synph", "abbreviated-form", "text"]
DITA_CODE = ["codeph", "filepath", "cmdname", "apiname", "option", "parmname", "msgnum", "xmlelement"]
DITA_BLOCK = ["p", "note", "li", "dt", "dd", "entry", "cmd", "info", "stepresult", "shortdesc", "abstract", "lq",
              "fig", "title", "section", "example"]
DB_INLINE = ["emphasis", "phrase", "link", "quote", "citetitle", "firstterm", "glossterm", "acronym", "trademark",
             "productname", "application", "guilabel", "guibutton", "guimenu", "keycap", "subscript", "superscript",
             "replaceable", "foreignphrase", "citation", "wordasword", "database", "hardware"]
DB_CODE = ["literal", "code", "filename", "command", "option", "envar", "function", "varname", "userinput", "uri",
           "email", "systemitem", "prompt", "tag", "classname"]
DB_BLOCK = ["para", "simpara", "title", "term", "entry", "note", "tip", "warning", "caption", "member"]


def t(rng, ws=False):
    s = rng.choice(WORDS)
    if ws and rng.random() < 0.3:
        s = rng.choice(["  ", "\n  ", "\t"]) + s + rng.choice(["  ", "\n", " "])
    return s


def q(rng, v):
    c = rng.choice(['"', "'"])
    return f"{c}{v}{c}"


def dita_inline(rng, depth):
    r = rng.random()
    if r < 0.2:
        n = rng.choice(DITA_CODE)
        return f"<{n}>x {{0}} &amp; y</{n}>"
    if r < 0.27:
        return rng.choice(['<xref href="a.dita"/>', '<keyword keyref="prod"/>', '<ph keyref="v"/>', '<image href="i.png" alt="Inline image"/>',
                           '<ph conref="c.dita#c/p">Old</ph>', '<ph conkeyref="k/p"/>', "&product;", "<!-- c -->", "<?pi x?>"])
    if r < 0.33:
        return f'<fn>{t(rng)}</fn>' if rng.random() < 0.5 else f"<indexterm>{t(rng)}<indexterm>{t(rng)}</indexterm></indexterm>"
    if r < 0.38:
        return f'<draft-comment>{t(rng)}</draft-comment>'
    n = rng.choice(DITA_INLINE)
    attrs = rng.choice(["", ' translate="no"', ' translate="yes"', ' keyref="k"', ' outputclass="x"', ' href="h.dita"'])
    inner = t(rng) if depth > 1 or rng.random() < 0.6 else dita_inline(rng, depth + 1)
    return f"<{n}{attrs}>{inner}</{n}>"


def dita_block(rng, depth=0):
    n = rng.choice(DITA_BLOCK)
    attrs = rng.choice(["", "", "", ' translate="no"', ' translate="yes"', ' conref="s.dita#s/p"', ' conkeyref="k/x"', ' id="b1"',
                        ' xml:space="preserve"', ' audience="admin"', ' xml:lang="de-DE"'])
    if rng.random() < 0.1:
        return f"<{n}{attrs}/>"
    parts = []
    for _ in range(rng.randint(1, 4)):
        k = rng.random()
        if k < 0.45:
            parts.append(t(rng, True))
        elif k < 0.8:
            parts.append(dita_inline(rng, 0))
        elif k < 0.88 and depth < 2:
            parts.append(dita_block(rng, depth + 1))
        elif k < 0.93:
            parts.append(f"<![CDATA[{rng.choice(['a < b', 'if (a&&b) {}', 'x'])}]]>")
        else:
            parts.append(rng.choice(["\n      ", " "]))
    return f"<{n}{attrs}>{''.join(parts)}</{n}>"


def dita_topic(rng):
    kind = rng.choice(["topic", "concept", "task", "reference", "glossentry", "troubleshooting"])
    body = {"topic": "body", "concept": "conbody", "task": "taskbody", "reference": "refbody",
            "glossentry": "glossBody", "troubleshooting": "troublebody"}[kind]
    blocks = [dita_block(rng) for _ in range(rng.randint(1, 6))]
    extra = rng.choice(["", '\n    <codeblock xml:space="preserve">a\n  b &lt;c&gt;</codeblock>', '\n    <image href="p.png" alt="Alt &amp; text"/>',
                        '\n    <p><data name="d" value="v">data text</data></p>', '\n    <required-cleanup>old</required-cleanup>',
                        '\n    <pre>keep   this\n  spacing</pre>', '\n    <lines>one\n  two</lines>'])
    head = rng.choice([
        f'<!DOCTYPE {kind} PUBLIC "-//OASIS//DTD DITA {kind.title()}//EN" "{kind}.dtd">\n',
        f'<!DOCTYPE {kind} PUBLIC "-//OASIS//DTD DITA {kind.title()}//EN" "http://127.0.0.1:9/{kind}.dtd">\n',
        "",
    ])
    arch = rng.choice(["", ' xmlns:ditaarch="http://dita.oasis-open.org/architecture/2005/" ditaarch:DITAArchVersion="1.3"'])
    lang = rng.choice(["", ' xml:lang="en-US"', ' xml:lang="en"'])
    tr = rng.choice(["", "", ' translate="no"'])
    nl = rng.choice(["\n", "\r\n"])
    decl = rng.choice(['<?xml version="1.0" encoding="UTF-8"?>' + nl, ""])
    doc = (f'{decl}{head}<{kind} id="t1"{arch}{lang}{tr}>{nl}  <title>{t(rng)} {dita_inline(rng, 1)}</title>{nl}'
           f'  <shortdesc>{t(rng)}</shortdesc>{nl}  <{body}>{nl}    ' + f"{nl}    ".join(blocks) + extra + f"{nl}  </{body}>{nl}</{kind}>{nl}")
    return doc


def dita_map(rng):
    kind = rng.choice(["map", "bookmap"])
    refs = []
    for _ in range(rng.randint(1, 6)):
        r = rng.random()
        if r < 0.3:
            refs.append(f'<topicref href="a.dita" navtitle={q(rng, rng.choice(["Intro", "Set up &amp; go", "Caf&#233;"]))}/>')
        elif r < 0.5:
            refs.append(f"<topicref href=\"b.dita\"><topicmeta><navtitle>{t(rng)}</navtitle><shortdesc>{t(rng)}</shortdesc></topicmeta></topicref>")
        elif r < 0.6:
            refs.append('<topicref keyref="k1"/>')
        elif r < 0.7:
            refs.append(f'<keydef keys="k1" href="k.dita"><topicmeta><keywords><keyword>{t(rng)}</keyword></keywords></topicmeta></keydef>')
        elif r < 0.8:
            refs.append(f'<topicref href="c.dita" navtitle="{t(rng)}" translate="no"/>')
        elif r < 0.9:
            refs.append(f'<topichead navtitle="Head"><topicref href="d.dita" conref="m.ditamap#m/r"/></topichead>')
        else:
            refs.append(f'<reltable><relrow><relcell><topicref href="e.dita"/></relcell></relrow></reltable>')
    if kind == "bookmap":
        head = f'<booktitle><mainbooktitle>{t(rng)}</mainbooktitle><booktitlealt>{t(rng)}</booktitlealt></booktitle>'
        refs = [r.replace("<topicref ", "<chapter ", 1).replace("</topicref>", "</chapter>") if r.startswith("<topicref") and "</topicref>" not in r[10:-11] else r for r in refs]
    else:
        head = f"<topicmeta><navtitle>{t(rng)}</navtitle></topicmeta>"
    title = rng.choice(["", f' title="{t(rng)}"'])
    dt = rng.choice(["", f'<!DOCTYPE {kind} PUBLIC "-//OASIS//DTD DITA {"BookMap" if kind == "bookmap" else "Map"}//EN" "{kind}.dtd">\n'])
    lang = rng.choice(["", ' xml:lang="en-US"'])
    return f'<?xml version="1.0" encoding="UTF-8"?>\n{dt}<{kind} id="m"{title}{lang}>\n  {head}\n  ' + "\n  ".join(refs) + f"\n</{kind}>\n"


def db_inline(rng, depth):
    r = rng.random()
    if r < 0.25:
        n = rng.choice(DB_CODE)
        return f"<{n}>a {{1}} &amp; b</{n}>"
    if r < 0.33:
        return rng.choice(['<xref linkend="x"/>', '<anchor xml:id="a1"/>', '<footnoteref linkend="f"/>', "&product;", "<!-- c -->",
                           '<inlinemediaobject><imageobject><imagedata fileref="i.png"/></imageobject></inlinemediaobject>'])
    if r < 0.4:
        return f"<footnote><para>{t(rng)}</para></footnote>" if rng.random() < 0.6 else f"<indexterm><primary>{t(rng)}</primary></indexterm>"
    if r < 0.45:
        return f"<remark>{t(rng)}</remark>"
    n = rng.choice(DB_INLINE)
    attrs = rng.choice(["", ' role="strong"', ' xreflabel="Label"', ' linkend="x"'])
    inner = t(rng) if depth > 1 or rng.random() < 0.6 else db_inline(rng, depth + 1)
    return f"<{n}{attrs}>{inner}</{n}>"


def db_block(rng):
    r = rng.random()
    if r < 0.12:
        return rng.choice(["<programlisting>run --it &lt;now&gt;</programlisting>", "<screen>$ ls -l</screen>",
                           "<literallayout>keep\n   this</literallayout>", "<synopsis>f(x)</synopsis>",
                           "<address>1 Main St\n  Town</address>"])
    n = rng.choice(DB_BLOCK)
    attrs = rng.choice(["", "", ' xreflabel="Block label"', ' xml:id="p1"', ' id="p2"', ' lang="fr"'])
    parts = []
    for _ in range(rng.randint(1, 4)):
        k = rng.random()
        if k < 0.5:
            parts.append(t(rng, True))
        elif k < 0.9:
            parts.append(db_inline(rng, 0))
        else:
            parts.append(f"<![CDATA[{rng.choice(['a < b', 'x'])}]]>")
    return f"<{n}{attrs}>{''.join(parts)}</{n}>"


def docbook(rng):
    blocks = "\n    ".join(db_block(rng) for _ in range(rng.randint(1, 6)))
    root = rng.choice(["book", "article", "chapter", "section"])
    v = rng.random()
    if v < 0.4:
        lang = rng.choice(["", ' xml:lang="en"', ' xml:lang="en-GB"'])
        return (f'<?xml version="1.0" encoding="UTF-8"?>\n<{root} xmlns="http://docbook.org/ns/docbook" xmlns:xlink="http://www.w3.org/1999/xlink" version="5.0"{lang}>\n'
                f"  <title>{t(rng)}</title>\n  <section>\n    <title>{t(rng)}</title>\n    {blocks}\n  </section>\n</{root}>\n")
    if v < 0.8:
        lang = rng.choice(["", ' lang="en"', ' xml:lang="en"'])
        return (f'<?xml version="1.0"?>\n<!DOCTYPE {root} PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN" "http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd">\n'
                f"<{root}{lang}>\n  <title>{t(rng)}</title>\n  <chapter>\n    {blocks}\n  </chapter>\n</{root}>\n")
    # neither DOCTYPE nor namespace: only the fixed dialect of the docbook filter applies
    return f"<{root}{rng.choice(['', ' lang=\"en\"'])}>\n  <title>{t(rng)}</title>\n  {blocks}\n</{root}>\n"


def unknown_root(rng):
    return f'<doc{rng.choice(["", " xml:lang=\"en\""])}>\n  ' + "\n  ".join(dita_block(rng) for _ in range(rng.randint(1, 4))) + "\n</doc>\n"


HAND = [
    ("dita", "keyref.dita", '<topic id="t"><title>T</title><body><p>Use <keyword keyref="prod"/> and <ph keyref="v">fallback</ph>. <xref keyref="k">link text</xref></p><p conkeyref="k/p"/><p conref="#a/b">old</p></body></topic>'),
    ("dita", "translate.dita", '<topic id="t"><title translate="no">T <ph translate="yes">yes</ph></title><body><p translate="no">No <b translate="yes">yes b</b></p><section translate="no"><p translate="yes">Back on</p><p>still off</p></section></body></topic>'),
    ("dita", "inlines.dita", '<topic id="t"><title>A <b>b</b> <i>i</i> <codeph>cp</codeph> <xref href="x.dita">X</xref> <keyword>K</keyword> <tm tmtype="reg">TM</tm> <term>term</term> <uicontrol>UI</uicontrol> <filepath>/a</filepath></title></topic>'),
    ("dita", "its.dita", '<topic id="t" xmlns:its="http://www.w3.org/2005/11/its"><its:rules version="2.0"><its:translateRule selector="//shortdesc" translate="no"/><its:locNoteRule selector="//title"><its:locNote>Short!</its:locNote></its:locNoteRule></its:rules><title>T</title><shortdesc>S</shortdesc><body><p its:translate="no">N</p><p>Y</p></body></topic>'),
    ("dita", "fixed.dita", '<doc><p>Hi <ph>x</ph></p><codeph>c</codeph></doc>'),
    ("docbook", "plain.dbk", '<article><para>Hi <emphasis>there</emphasis></para></article>'),
    ("docbook", "nslang.dbk", '<article xmlns="http://docbook.org/ns/docbook" version="5.0" lang="en"><para>Hi</para></article>'),
    ("docbook", "prefixed.dbk", '<db:article xmlns:db="http://docbook.org/ns/docbook"><db:para>Hi <db:emphasis>x</db:emphasis> <db:literal>y</db:literal></db:para></db:article>'),
    # gate cases (11-05): translate switch-back several levels deep, conref / conkeyref / keyref in every position,
    # nested units, translatable attributes with entities and quotes, language stamping corners, DTDs and XInclude
    ("dita", "gate_switch.dita", '<topic id="t"><title>T</title><body><p translate="no">a <ph translate="yes">b <ph>c</ph> <ph translate="no">d <b translate="yes">e</b> f</ph> g</ph> h</p><section translate="no"><title>off</title><div translate="yes"><p>on <i>i</i></p><p translate="no">off <ph translate="yes">on2</ph></p></div></section></body></topic>'),
    ("dita", "gate_conref.dita", '<topic id="t"><title>T <ph conref="a.dita#a/b"/> x <ph conkeyref="k/v">fb</ph> <keyword keyref="kw"/> <ph keyref="v">keep</ph></title><body><p conref="a.dita#a/p">x <ph translate="yes">y</ph></p><p conref="a.dita#a/q" translate="yes">z</p><section conkeyref="s/x"><p>in</p></section><p>after <xref keyref="k" href="h.dita">xr</xref></p></body></topic>'),
    ("dita", "gate_nested.dita", '<topic id="t"><title>T</title><body><p>Text<fn>Foot <b>b</b><fn>inner</fn> tail</fn> and <indexterm>alpha<indexterm>beta<index-see>gamma</index-see></indexterm></indexterm> end</p><p><fn>only</fn></p><p>x<draft-comment>dc</draft-comment>y</p></body></topic>'),
    ("dita", "gate_attrs.ditamap", '<map title="Map &amp; &quot;T&quot;" xml:lang="en"><topicref href="a.dita" navtitle=\'Nav "q" &amp; it\'/><topicref navtitle="  spaced   out "/><topicref navtitle="caf&#233; &#x65E5; &lt;b&gt;"/><topicref navtitle=""/><topicref href="b.dita" navtitle="Off" translate="no"/><topicref href="c.dita" navtitle="Ref" conref="m.ditamap#m/r"/></map>'),
    ("dita", "gate_image.dita", '<topic id="t"><title>T</title><body><p>See <image href="a.png" alt="Alt &lt;1&gt;"/> here</p><image href="b.png" alt="Block alt"><alt>Child alt</alt></image><fig><title>F</title><image href="c.png" alt="  "/></fig></body></topic>'),
    ("dita", "gate_lang_add.dita", '<?xml version="1.0"?>\n<!DOCTYPE concept PUBLIC "-//OASIS//DTD DITA Concept//EN" "concept.dtd">\n<concept id="c"><title>Hi</title></concept>\n'),
    ("dita", "gate_lang_has.dita", "<concept id='c' xml:lang='en-US' translate='yes'><title>Hi</title></concept>"),
    ("dita", "gate_dita_root.dita", '<dita><topic id="a" xml:lang="en"><title>A</title></topic><topic id="b"><title>B</title></topic></dita>'),
    ("dita", "gate_dtd.dita", '<!DOCTYPE topic PUBLIC "-//OASIS//DTD DITA Topic//EN" "http://127.0.0.1:9/topic.dtd"><topic id="t"><title>Hi &product; &nbsp;</title></topic>'),
    ("dita", "gate_system.dita", '<!DOCTYPE topic SYSTEM "file:///etc/passwd"><topic id="t"><title>Hi</title></topic>'),
    ("dita", "gate_xxe.dita", '<!DOCTYPE topic [<!ENTITY x SYSTEM "file:///etc/passwd">]><topic id="t"><title>&x;</title></topic>'),
    ("dita", "gate_xinclude.dita", '<topic id="t" xmlns:xi="http://www.w3.org/2001/XInclude"><title>T <xi:include href="file:///etc/passwd" parse="text"/></title><body><xi:include href="other.dita"/></body></topic>'),
    ("dita", "gate_cdata.dita", '<topic id="t"><title>T</title><body><p><![CDATA[a < b]]></p><p>x <![CDATA[y]]> z</p><codeblock><![CDATA[keep  ]]> me</codeblock></body></topic>'),
    ("dita", "gate_comments.dita", '<topic id="t"><title>A <!-- c --> B <?pi x?> C</title><body><p>a<!--only--></p></body></topic>'),
    ("docbook", "gate_db45.dbk", '<?xml version="1.0"?>\n<!DOCTYPE book PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN" "http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd">\n<book><chapter xreflabel="Ch &lt;1&gt;"><title>T</title><para>P <footnote><para>F <emphasis>e</emphasis></para></footnote> <indexterm><primary>idx</primary><secondary>two</secondary></indexterm> <remark>R</remark> <literal>lit</literal></para><programlisting>code  here</programlisting></chapter></book>'),
    ("docbook", "gate_db5_both.dbk", '<book xmlns="http://docbook.org/ns/docbook" version="5.0" lang="en" xml:lang="en"><title>T</title><para>Hi</para></book>'),
    ("docbook", "gate_db5_none.dbk", '<article xmlns="http://docbook.org/ns/docbook" version="5.0"><title>T</title><para>Hi</para></article>'),
    ("docbook", "gate_db_prefix_decl.dbk", '<!DOCTYPE article PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN" "docbookx.dtd"><article xmlns:db="http://docbook.org/ns/docbook"><para>Hi</para></article>'),
    ("docbook", "gate_db45_lang.dbk", '<!DOCTYPE article PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN" "docbookx.dtd"><article lang="en"><para>Hi <footnote><para>F</para></footnote></para></article>'),
]


def enc(b):
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
        if kind == "all":
            m[str(i)] = f"T{i} \u00e9 " + u["text"].replace("a", "\u00e0") + " <&>"
        elif kind == "third" and i % 3 == 0:
            m[str(i)] = f"Zeta {i} \u65e5"
    return m


def case(fmt, name, data):
    filt = get_filter(fmt)
    rec = {"name": name, "filter": fmt, "data": enc(data)}
    try:
        pr = filt.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = f"{type(e).__name__}: {str(e)[:160]}"
        return rec
    units = units_of(pr)
    rec["units"] = units
    builds = {}
    for kind in ("all", "third"):
        mp = token_map(units, kind)
        try:
            builds[kind] = enc(filt.build(data, mp))
        except Exception as e:  # noqa: BLE001
            builds[kind] = "ERR " + type(e).__name__
        for loc in ("de-DE", "ar", "ja-JP"):
            try:
                builds[f"{kind}@{loc}"] = enc(filt.build_localized(data, mp, loc))
            except Exception as e:  # noqa: BLE001
                builds[f"{kind}@{loc}"] = "ERR " + type(e).__name__
    rec["builds"] = builds
    return rec


EXT = {"dita": "dita", "ditamap": "dita", "bookmap": "dita", "dbk": "docbook", "docbook": "docbook", "xml": "docbook"}


def main():
    out_path, files = sys.argv[1], sys.argv[2:]
    rng = random.Random(1105)
    cases = []
    for f in files:
        with open(f, "rb") as fh:
            cases.append(case(EXT[f.rsplit(".", 1)[1].lower()], os.path.basename(f), fh.read()))
    for fmt, name, text in HAND:
        cases.append(case(fmt, name, text.encode("utf-8")))
    for fn, fmt, n in [(dita_topic, "dita", 150), (dita_map, "dita", 50), (unknown_root, "dita", 20), (docbook, "docbook", 150)]:
        for i in range(n):
            cases.append(case(fmt, f"{fn.__name__}_{i}", fn(rng).encode("utf-8")))
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, ensure_ascii=False)
    errs = sum(1 for c in cases if "error" in c)
    print(f"{len(cases)} cases, {errs} refused by LocOS")


if __name__ == "__main__":
    main()
