"""Random differential corpus for the LocOS profile (Track D): run with the Localization OS venv Python, read-only.

    <local-tms>/.venv/Scripts/python.exe oracle/locos_fuzz.py OUT_DIR [--n 300] [--seed 1] [FORMAT ...]

Writes OUT_DIR/<format>/r<seed>_<n>.<ext> files from per-format generators (CRLF, CR, mixed line ends, BOMs,
blank and whitespace lines, nesting, plural forms and placeholders of every kind) and, next to each, the golden
`oracle/locos_golden.py` would write, plus one more build, `inject`: every unit translated to text that looks like
the format's own structure (new cues, new PO entries, new Markdown blocks, quotes, closing tags, forged tokens).
A file LocOS refuses gets `{"format": .., "parse_error": ..}` instead. Point the golden test at the corpus with
`BILI_LOCOS_ROOT=OUT_DIR` (cargo: `--config env.BILI_LOCOS_ROOT="OUT_DIR"`). Nothing in the LocOS tree is written.
"""

import base64
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import locos_golden as og  # noqa: E402  (sets LocOS up: throwaway data dir, filters registered)

R = random.Random()

WORDS = [
    "Save", "file", "open", "the", "settings", "Welcome", "user", "error", "café", "naïve", "Größe", "straße",
    "日本語", "テキスト", "中文", "한국어", "مرحبا", "שלום", "Привет", "ελληνικά", "emoji 😀", "e\u0301", "x\u00a0y",
    "zero\u200bwidth", "tab\there", "50%", "$5", "a&b", "1<2", "3>2", "it's", "\"quoted\"", "C:\\Temp", "back\\slash",
    "#hash", "*star*", "_under_", "`tick`", "[brackets]", "(parens)", "{braces}", "~tilde~", "|pipe|", "a--b", "...",
    "12", "3.14", "https://example.com/a?b=1&c=2", "user@example.com", "ID-42", "OK", "x", "",
    "__bold__", "**strong**", "www.example.org", "https://e.com/p_(x)", "保存https://e.com/z。", "**「保存」**を",
]
# Words every Windows-1252 file can hold (a file made of them is written in a code page)
LATIN_WORDS = [
    "Save", "file", "open", "the", "settings", "café", "naïve", "Größe", "straße", "œuvre", "€5", "„quoted“",
    "x\u00a0y", "tab\there", "50%", "a&b", "1<2", "it's", "C:\\Temp", "*star*", "_under_", "[brackets]", "12",
    "https://example.com/a?b=1&c=2", "ID-42", "OK", "x", "",
]
LATIN = False
PH = [
    "%s", "%d", "%1$s", "%2$d", "%@", "%1$@", "%(name)s", "%.2f", "%%", "%lld", "%-5s", "{0}", "{1}", "{name}",
    "{{name}}", "${var}", "$var$", "[ProductName]", "[1]", ":count", "{0:N2}", "%{count}", "{{count}}", "@:link",
    "\\n", "&#x41;", "&nbsp;", "{ $var }", "{count, plural, one {# item} other {# items}}",
    "{sex, select, male {he} female {she} other {they}}", "<b>bold</b>", "<br/>", "<a href=\"x\">link</a>",
    "<i>it</i>",
]


def word():
    return R.choice(LATIN_WORDS if LATIN else WORDS)


def phrase(lo=1, hi=8, ph=0.3):
    out = []
    for _ in range(R.randint(lo, hi)):
        out.append(R.choice(PH) if R.random() < (ph if not LATIN else ph / 2) else word())
    s = " ".join(x for x in out if x)
    if R.random() < 0.1:
        s = "  " + s
    if R.random() < 0.1:
        s += R.choice(["  ", " ", "\t", "."])
    return s or "text"


def plain(lo=1, hi=8):
    s = " ".join(w for w in (word() for _ in range(R.randint(lo, hi))) if w)
    return s or "word"


def ident():
    return R.choice(["key", "title", "msg", "btn", "label", "app.name", "a_b", "x1", "menu-open", "Ok", "k"]) + str(R.randint(0, 99))


def eol_pick():
    return R.choice(["\n"] * 6 + ["\r\n"] * 3 + ["\r", "mixed"])


def join_lines(lines, eol, final=None):
    if final is None:
        final = R.random() < 0.8
    out = []
    for i, ln in enumerate(lines):
        out.append(ln)
        if i < len(lines) - 1 or final:
            out.append(R.choice(["\n", "\r\n", "\r"]) if eol == "mixed" else eol)
    return "".join(out)


def encode(s, encs=("utf-8",)):
    enc = R.choice(encs)
    if LATIN:
        enc = R.choice(["cp1252", "cp1252", "utf-16", "utf-8-sig", "latin-1"])
    if enc == "utf-8-sig":
        return "\ufeff".encode("utf-8") + s.encode("utf-8")
    if enc == "utf-16":
        return s.encode("utf-16")  # with a BOM
    try:
        return s.encode(enc)
    except UnicodeEncodeError:
        return s.encode("utf-8")


DEF_ENCS = ("utf-8",) * 6 + ("utf-8-sig",) * 2


# ---------------------------------------------------------------- txt
def g_txt():
    lines = []
    for _ in range(R.randint(0, 25)):
        r = R.random()
        if r < 0.15:
            lines.append("")
        elif r < 0.25:
            lines.append(R.choice([" ", "\t", "   \t", "\u3000", "\u00a0"]))
        else:
            ln = R.choice(["", "  ", "\t", "    "]) + phrase(1, 10, 0.2)
            if R.random() < 0.08:
                ln += R.choice(["\x0b", "\x0c", "\x1c", "\x1d", "\x1e", "\x85", "\u2028", "\u2029"]) + plain(1, 3)
            lines.append(ln)
    return encode(join_lines(lines, eol_pick()), DEF_ENCS + ("cp1252", "utf-16"))


# ---------------------------------------------------------------- md
def md_inline():
    bits = []
    for _ in range(R.randint(1, 6)):
        r = R.random()
        if r < 0.45:
            bits.append(plain(1, 4))
        elif r < 0.55:
            bits.append("**" + plain(1, 2) + "**")
        elif r < 0.62:
            bits.append("*" + plain(1, 2) + "*")
        elif r < 0.68:
            bits.append("`" + R.choice(["code", "a < b", "x*y", "|"]) + "`")
        elif r < 0.76:
            bits.append("[" + plain(1, 2) + "](" + R.choice(["https://e.com", "./a.md", "#anchor", "<a b.md>"]) + R.choice(["", ' "Title"']) + ")")
        elif r < 0.8:
            bits.append("<" + R.choice(["https://e.com", "a@b.co"]) + ">")
        elif r < 0.84:
            bits.append(R.choice(["<b>x</b>", "<br>", "<span class=\"a\">s</span>", "&amp;", "&copy;", "\\*", "\\_"]))
        elif r < 0.88:
            bits.append("[" + plain(1, 2) + "][" + R.choice(["ref", "1", ""]) + "]")
        elif r < 0.92:
            bits.append("~~" + plain(1, 2) + "~~")
        else:
            bits.append(R.choice(PH))
    s = " ".join(bits)
    if R.random() < 0.08:
        s += R.choice(["  ", "\\"])  # hard breaks
    return s


def g_md():
    lines = []
    if R.random() < 0.3:
        if R.random() < 0.75:
            lines.append("---")
            for _ in range(R.randint(0, 5)):
                k = R.choice(["title", "description", "summary", "tags", "author", "date", "slug", "sidebar_label", "draft", "keywords", "linkTitle"])
                v = R.choice([plain(1, 4), '"' + plain(1, 3) + '"', "'" + plain(1, 2) + "'", "[a, b]", "true", "2024-01-01", "", "|", "> folded", plain(1, 2) + ": colon"])
                lines.append(f"{k}: {v}")
                if v in ("|", "> folded"):
                    lines.append("  " + plain(1, 3))
            lines.append(R.choice(["---", "---", "..."]))
        else:
            lines.append("+++")
            for _ in range(R.randint(0, 4)):
                k = R.choice(["title", "description", "summary", "draft", "weight"])
                v = R.choice(['"' + plain(1, 3) + '"', "'" + plain(1, 2) + "'", "true", "10", '"""multi"""'])
                lines.append(f"{k} = {v}")
            lines.append("+++")
    for _ in range(R.randint(0, 22)):
        r = R.random()
        if r < 0.1:
            lines.append("#" * R.randint(1, 7) + R.choice([" ", "", "  "]) + md_inline() + R.choice(["", " #", " ##"]))
        elif r < 0.15:
            lines.append(md_inline())
            lines.append(R.choice(["===", "---", "- - -", "=", "--"]))
        elif r < 0.3:
            ind = R.choice(["", "", "  ", "    ", "\t"])
            for _ in range(R.randint(1, 4)):
                lines.append(ind + R.choice(["- ", "* ", "+ ", "1. ", "2) ", "10. ", "- [ ] ", "- [x] ", "-"]) + md_inline())
        elif r < 0.37:
            for _ in range(R.randint(1, 3)):
                lines.append(R.choice(["> ", ">", "> > ", ">  - "]) + md_inline())
        elif r < 0.45:
            fence = R.choice(["```", "~~~", "````"])
            lines.append(R.choice(["", "  "]) + fence + R.choice(["", "python", "js title=\"a\"", " "]))
            for _ in range(R.randint(0, 3)):
                lines.append(R.choice(["print('hi')", "# not a heading", "", "  - x", "<b>"]))
            if R.random() < 0.9:
                lines.append(fence)
        elif r < 0.5:
            lines.append("")
            for _ in range(R.randint(1, 3)):
                lines.append("    " + R.choice(["code line", "x = 1", "# c"]))
        elif r < 0.56:
            cols = R.randint(1, 4)
            lines.append("| " + " | ".join(plain(1, 2) for _ in range(cols)) + " |")
            lines.append("|" + "|".join(R.choice(["---", ":--", "--:", ":-:"]) for _ in range(cols)) + "|")
            for _ in range(R.randint(0, 3)):
                lines.append("| " + " | ".join(R.choice([md_inline(), "a \\| b", ""]) for _ in range(cols)) + " |")
        elif r < 0.61:
            if R.random() < 0.5:
                lines.append("<!-- " + plain(1, 3) + " -->")
            else:
                lines.append("<!--")
                lines.append(plain(1, 3))
                lines.append("-->")
        elif r < 0.65:
            lines.append("[" + R.choice(["ref", "1", "Docs"]) + "]: " + R.choice(["https://e.com", "<./a b.md>", "/x"]) + R.choice(["", ' "Title"', " 'T'", " (Paren title)"]))
        elif r < 0.7:
            lines.append(R.choice(["", "  "]) + "![" + R.choice(["", plain(1, 3)]) + "](" + R.choice(["img.png", "a b.png", "https://e.com/i.svg"]) + R.choice(["", ' "Img title"']) + ")" + R.choice(["", " " + plain(1, 2)]))
        elif r < 0.74:
            lines.append(R.choice(["---", "***", "___", "- - -", "* * *"]))
        elif r < 0.78:
            lines.append(R.choice(["<div>", "<details>", "<p align=\"center\">", "</div>", "<summary>" + plain(1, 2) + "</summary>", "<img src=\"a.png\" alt=\"x\">"]))
        elif r < 0.85:
            lines.append("")
        elif r < 0.88:
            lines.append(R.choice([" ", "\t", "  "]))
        else:
            lines.append(R.choice(["", "", " ", "   "]) + md_inline())
            if R.random() < 0.3:
                lines.append(md_inline())
    return encode(join_lines(lines, eol_pick()), DEF_ENCS)


# ---------------------------------------------------------------- srt / vtt
def ts(sep):
    h, m, s, ms = R.randint(0, 2), R.randint(0, 59), R.randint(0, 59), R.randint(0, 999)
    return f"{h:02d}:{m:02d}:{s:02d}{sep}{ms:03d}"


def srt_text():
    bits = []
    for _ in range(R.randint(1, 5)):
        r = R.random()
        if r < 0.5:
            bits.append(plain(1, 4))
        elif r < 0.6:
            t = R.choice(["i", "b", "u", "s"])
            bits.append(f"<{t}>{plain(1, 2)}</{t}>")
        elif r < 0.65:
            bits.append(R.choice(["<i>", "</i>", "<b>"]))
        elif r < 0.72:
            bits.append('<font color="' + R.choice(["#ff0000", "red"]) + '">' + plain(1, 2) + "</font>")
        elif r < 0.82:
            bits.append(R.choice(["{\\an8}", "{\\i1}", "{\\i0}", "{\\b1}", "{\\pos(10,20)}", "{\\c&H00FF00&}", "{\\r}", "{\\i1}" + plain(1, 1) + "{\\i0}"]))
        elif r < 0.86:
            bits.append(R.choice(["&amp;", "&lt;", "&", "<", ">", "-->"]))
        else:
            bits.append(R.choice(PH))
    return " ".join(bits)


def g_srt():
    lines = []
    n = 1
    for _ in range(R.randint(0, 10)):
        if R.random() < 0.9:
            lines.append(str(n) if R.random() < 0.95 else R.choice(["", "x", str(n + 5)]))
        n += 1
        sep = R.choice([",", ",", ",", "."])
        lines.append(ts(sep) + R.choice([" --> ", " --> ", "-->", "  -->  "]) + ts(sep) + R.choice(["", "", " X1:10 X2:20"]))
        for _ in range(R.choice([0, 1, 1, 1, 2, 2, 3])):
            lines.append(R.choice(["", "- ", "  "]) + srt_text() + R.choice(["", "", " "]))
        for _ in range(R.choice([1, 1, 1, 1, 2, 0])):
            lines.append(R.choice(["", "", " "]))
    return encode(join_lines(lines, eol_pick()), DEF_ENCS + ("cp1252",))


def vtt_text():
    bits = []
    for _ in range(R.randint(1, 5)):
        r = R.random()
        if r < 0.45:
            bits.append(plain(1, 4))
        elif r < 0.55:
            t = R.choice(["i", "b", "u"])
            bits.append(f"<{t}>{plain(1, 2)}</{t}>")
        elif r < 0.62:
            bits.append("<c" + R.choice(["", ".loud", ".a.b"]) + ">" + plain(1, 2) + "</c>")
        elif r < 0.69:
            bits.append("<v" + R.choice(["", ".first"]) + " " + R.choice(["Roger", "Esme Smith", "A&amp;B"]) + ">" + plain(1, 2) + R.choice(["</v>", ""]))
        elif r < 0.73:
            bits.append("<ruby>" + plain(1, 1) + "<rt>" + plain(1, 1) + "</rt></ruby>")
        elif r < 0.76:
            bits.append("<lang " + R.choice(["fr", "en-US"]) + ">" + plain(1, 2) + "</lang>")
        elif r < 0.8:
            bits.append("<" + ts(".") + ">")
        elif r < 0.85:
            bits.append(R.choice(["&amp;", "&lt;", "&gt;", "&nbsp;", "&lrm;", "&", "<x>", "</i>", "<i>", "<b.x>"]))
        else:
            bits.append(R.choice(PH))
    return " ".join(bits)


def g_vtt():
    lines = [R.choice(["WEBVTT", "WEBVTT", "WEBVTT - Title", "\ufeffWEBVTT" if False else "WEBVTT", "WEBVTT\tx"])]
    for _ in range(R.randint(0, 3)):
        lines.append(R.choice(["Language: " + R.choice(["en", "en-US", "fr", ""]), "Kind: captions", "language:en", "X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:0"]))
    lines.append("")
    for _ in range(R.randint(0, 9)):
        r = R.random()
        if r < 0.08:
            lines.append("NOTE " + plain(1, 3))
            if R.random() < 0.5:
                lines.append(plain(1, 3))
            lines.append("")
            continue
        if r < 0.12:
            lines.append("STYLE")
            lines.append("::cue { color: red }")
            lines.append("")
            continue
        if r < 0.15:
            lines.append("REGION")
            lines.append("id:fred width:40%")
            lines.append("")
            continue
        if R.random() < 0.3:
            lines.append(R.choice(["cue-" + str(R.randint(1, 9)), str(R.randint(1, 9)), "intro"]))
        lines.append(ts(".") + " --> " + ts(".") + R.choice(["", "", " align:start position:10%", " line:0"]))
        for _ in range(R.choice([0, 1, 1, 1, 2, 2, 3])):
            lines.append(vtt_text())
        lines.append(R.choice(["", "", "", " "]))
    return encode(join_lines(lines, eol_pick()), DEF_ENCS)


# ---------------------------------------------------------------- po
def po_str(s):
    s = s.replace("\\", "\\\\").replace('"', '\\"').replace("\t", "\\t")
    if R.random() < 0.15:
        s += R.choice(["\\n", "\\r", "\\a"])
    return s


def po_lines(kw, s):
    if R.random() < 0.2 and len(s) > 6:
        cut = R.randint(1, len(s) - 1)
        while cut < len(s) and s[cut - 1] == "\\":
            cut += 1
        return [f'{kw} ""', f'"{s[:cut]}"', f'"{s[cut:]}"']
    return [f'{kw} "{s}"']


def g_po():
    lines = []
    if R.random() < 0.8:
        lines.append("# Translation file")
        lines.append('msgid ""')
        lines.append('msgstr ""')
        hdr = ['"Project-Id-Version: x 1.0\\n"', '"Content-Type: text/plain; charset=' + R.choice(["UTF-8", "utf-8", "CHARSET", "ISO-8859-1"]) + '\\n"']
        if R.random() < 0.5:
            hdr.append('"Language: ' + R.choice(["fr", "de", "", "pl_PL"]) + '\\n"')
        if R.random() < 0.6:
            hdr.append('"Plural-Forms: ' + R.choice(["nplurals=2; plural=(n != 1);", "nplurals=1; plural=0;", "nplurals=INTEGER; plural=EXPRESSION;"]) + '\\n"')
        R.shuffle(hdr)
        lines += hdr
        lines.append("")
    for _ in range(R.randint(0, 10)):
        if R.random() < 0.3:
            lines.append("#. " + plain(1, 3))
        if R.random() < 0.3:
            lines.append("#: src/a.py:" + str(R.randint(1, 99)))
        if R.random() < 0.2:
            lines.append("#, " + R.choice(["fuzzy", "python-format", "fuzzy, c-format", "no-wrap"]))
        if R.random() < 0.1:
            lines.append('#| msgid "' + po_str(plain(1, 2)) + '"')
        if R.random() < 0.05:
            lines.append("#~ msgid \"old\"")
            lines.append("#~ msgstr \"alt\"")
        if R.random() < 0.15:
            lines += po_lines("msgctxt", po_str(plain(1, 2)))
        mid = po_str(phrase(1, 6))
        lines += po_lines("msgid", mid)
        if R.random() < 0.25:
            lines += po_lines("msgid_plural", po_str(phrase(1, 6)))
            for k in range(R.choice([1, 2, 2, 3, 6])):
                lines += po_lines(f"msgstr[{k}]", po_str(plain(1, 3)) if R.random() < 0.4 else "")
        else:
            lines += po_lines("msgstr", po_str(plain(1, 3)) if R.random() < 0.3 else "")
        if R.random() < 0.85:
            lines.append(R.choice(["", "", "  "]))
    return encode(join_lines(lines, eol_pick()), DEF_ENCS)


# ---------------------------------------------------------------- json, json5, arb, resjson
def js_str(s, q='"'):
    out = []
    for ch in s:
        if ch == "\\":
            out.append("\\\\")
        elif ch == q:
            out.append("\\" + q)
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append(R.choice(["\\t", "\\u0009"]))
        elif ch < " ":
            out.append("\\u%04x" % ord(ch))
        elif ord(ch) > 127 and R.random() < 0.2 and ord(ch) < 0x10000:
            out.append("\\u%04x" % ord(ch))
        elif ch == "/" and R.random() < 0.3:
            out.append("\\/")
        else:
            out.append(ch)
    return q + "".join(out) + q


SAFE_KEYS = ["title", "label", "name", "text", "body", "help", "nav", "items", "a", "b", "x.y", "with space", "ünï", "ok", "count", "c_d", "key-1", "0"]


def j_value(depth, style):
    r = R.random()
    if depth < 4 and r < 0.18:
        return j_obj(depth + 1, style)
    if depth < 4 and r < 0.26:
        return ("arr", [j_value(depth + 1, style) for _ in range(R.randint(0, 4))])
    if r < 0.33:
        return ("lit", R.choice(["true", "false", "null", "0", "-1.5e3", "42", "1E2"] + (["NaN", "Infinity", "-Infinity"] if style == "json" and R.random() < 0.3 else [])))
    s = R.choice([phrase(1, 7), phrase(1, 4, 0.6), R.choice(PH), "", " ", "123", "https://example.com", "12:30", "!!!", "e.g.", plain(1, 2), "multi\nline", "#fff"])
    return ("str", s)


def j_obj(depth, style):
    items = []
    used = []
    for _ in range(R.randint(0, 6)):
        if used and R.random() < 0.08:
            k = R.choice(used)  # duplicate key
        else:
            k = R.choice(SAFE_KEYS) + (str(R.randint(0, 9)) if R.random() < 0.5 else "")
        used.append(k)
        items.append((k, j_value(depth, style)))
    return ("obj", items)


def j_render(v, style, ind, lvl):
    kind, x = v
    nl = "\n" if ind is not None else ""
    pad = (ind or "") * (lvl + 1)
    end = (ind or "") * lvl
    if kind == "lit":
        return x
    if kind == "str":
        return js_str(x, "'" if style == "json5" and R.random() < 0.3 else '"')
    if kind == "arr":
        if not x:
            return "[]"
        body = ("," + nl).join(pad + j_render(e, style, ind, lvl + 1) for e in x)
        trail = "," if style == "json5" and R.random() < 0.3 else ""
        return "[" + nl + body + trail + nl + end + "]"
    if not x:
        return "{}"
    parts = []
    for k, e in x:
        if style == "json5" and R.random() < 0.4 and k.replace("_", "a").isalnum() and not k[0].isdigit():
            key = k
        else:
            key = js_str(k)
        sep = R.choice([": ", ":", " : "]) if ind is not None else ":"
        c = ""
        if style in ("json5", "resjson") and ind is not None and R.random() < 0.1:
            c = pad + R.choice(["// note", "/* block */"]) + nl
        parts.append(c + pad + key + sep + j_render(e, style, ind, lvl + 1))
    trail = "," if style == "json5" and R.random() < 0.3 else ""
    return "{" + nl + ("," + nl).join(parts) + trail + nl + end + "}"


def g_json_like(style):
    ind = R.choice([None, "  ", "    ", "\t"])
    root = j_obj(0, style)
    if style == "json" and R.random() < 0.05:
        root = ("arr", [j_value(1, style) for _ in range(R.randint(0, 4))])
    s = j_render(root, style, ind, 0)
    if style == "json5" and R.random() < 0.3:
        s = "// header\n" + s
    if style == "resjson" and R.random() < 0.3:
        s = "/* resjson */\n" + s
    eol = R.choice(["\n", "\r\n"])
    s = s.replace("\n", eol) + (eol if R.random() < 0.7 else "")
    return encode(s, DEF_ENCS)


def g_json():
    return g_json_like("json")


def g_json5():
    return g_json_like("json5")


def g_resjson():
    # resjson: flat-ish string table with _key.comment entries
    ind = R.choice(["  ", "    ", "\t"])
    parts = []
    for _ in range(R.randint(0, 10)):
        k = ident()
        r = R.random()
        if r < 0.7:
            parts.append((k, js_str(R.choice([phrase(1, 6), phrase(1, 3, 0.6), R.choice(PH), "", "42"]))))
            if R.random() < 0.3:
                parts.append((f"_{k}.comment", js_str(plain(1, 3))))
            if R.random() < 0.1:
                parts.append((f"_{k}.maxLength", "40"))
        elif r < 0.85:
            inner = ", ".join(f"{js_str(ident())}: {js_str(phrase(1, 3))}" for _ in range(R.randint(0, 3)))
            parts.append((k, "{ " + inner + " }"))
        else:
            parts.append((k, R.choice(["true", "1", "null"])))
    lines = []
    for i, (k, v) in enumerate(parts):
        if R.random() < 0.1:
            lines.append(ind + R.choice(["// c", "/* c */"]))
        lines.append(ind + js_str(k) + R.choice([": ", " : "]) + v + ("," if i < len(parts) - 1 else ""))
    eol = R.choice(["\n", "\r\n"])
    s = "{" + eol + eol.join(lines) + eol + "}" + eol
    return encode(s, DEF_ENCS)


ICU = [
    "{count, plural, =0{No items} =1{One item} other{{count} items}}",
    "{count, plural, one{# file} other{# files}}",
    "{gender, select, male{He} female{She} other{They}} replied",
    "Hello {name}, you have {count, plural, one{one message} other{{count} messages}}",
    "{n, plural, offset:1 =0{nobody} one{{who}} other{{who} and # others}}",
    "'{literal}' braces",
    "{a}{b}",
    "{x, number} and {d, date, short}",
]


def g_arb():
    ind = R.choice(["  ", "    "])
    parts = []
    if R.random() < 0.8:
        parts.append(("@@locale", js_str(R.choice(["en", "en_US", "de"]))))
    if R.random() < 0.2:
        parts.append(("@@last_modified", js_str("2024-01-01T00:00:00Z")))
    for _ in range(R.randint(0, 9)):
        k = R.choice(["hello", "items", "title", "btnSave", "greet", "count", "msg"]) + str(R.randint(0, 99))
        v = R.choice([phrase(1, 5, 0.1), R.choice(ICU), "Hello {name}", "{a} of {b}", plain(1, 4), "", "{x}"])
        parts.append((k, js_str(v)))
        if R.random() < 0.6:
            meta = '{"description": ' + js_str(plain(1, 3))
            if "{" in v and R.random() < 0.8:
                meta += ', "placeholders": {"name": {"type": "String", "example": "Bob"}}'
            meta += "}"
            parts.append(("@" + k, meta))
    eol = R.choice(["\n", "\r\n"])
    s = "{" + eol + ("," + eol).join(ind + js_str(k) + ": " + v for k, v in parts) + eol + "}" + eol
    return encode(s, DEF_ENCS)


# ---------------------------------------------------------------- strings, properties
def strings_esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def g_strings():
    lines = []
    for _ in range(R.randint(0, 10)):
        r = R.random()
        if r < 0.15:
            lines.append(R.choice(["/* " + plain(1, 3) + " */", "// " + plain(1, 3), "/* multi\nline */", "/*\n * doc\n */"]))
        elif r < 0.2:
            lines.append("")
        k = ident()
        v = R.choice([phrase(1, 6), phrase(1, 3, 0.7), "%@", "%1$@ of %2$@", "", "Caf\\U00e9", "\\t tab", "line\\nline"])
        key = R.choice(['"' + k + '"', '"' + k + '"', k]) if k.replace(".", "").replace("_", "").replace("-", "").isalnum() else '"' + k + '"'
        val = '"' + (strings_esc(v) if "\\" not in v else v) + '"'
        lines.append(key + R.choice([" = ", "=", "  =  "]) + val + R.choice([";", ";", "; // trailing", " ;"]))
    eol = R.choice(["\n", "\r\n"])
    return encode(join_lines(lines, eol), ("utf-8",) * 4 + ("utf-8-sig", "utf-16", "utf-16"))


def prop_esc(s, latin):
    out = []
    for ch in s:
        if ch in "\\":
            out.append("\\\\")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ord(ch) > 126 and (not latin or ord(ch) > 255 or R.random() < 0.5):
            if ord(ch) > 0xFFFF:
                out.append(ch)
            else:
                out.append("\\u%04x" % ord(ch) if R.random() < 0.5 else "\\u%04X" % ord(ch))
        else:
            out.append(ch)
    return "".join(out)


def g_properties():
    latin = R.random() < 0.3 or LATIN
    lines = []
    for _ in range(R.randint(0, 12)):
        r = R.random()
        if r < 0.12:
            lines.append(R.choice(["# ", "! ", "#"]) + plain(1, 3))
            continue
        if r < 0.18:
            lines.append(R.choice(["", "  "]))
            continue
        k = ident().replace("-", ".")
        if R.random() < 0.1:
            k = k.replace("k", "k\\ e").replace("a", "a\\:")
        v = prop_esc(R.choice([phrase(1, 7), phrase(1, 3, 0.6), "{0} of {1}", "It''s {0}", "", "%s", "  lead"]), latin)
        sep = R.choice(["=", " = ", ":", " : ", " ", "\t"])
        if R.random() < 0.1 and len(v) > 4:
            cut = len(v) // 2
            while cut < len(v) and v[cut - 1] == "\\":
                cut += 1
            lines.append(R.choice(["", "  "]) + k + sep + v[:cut] + "\\")
            lines.append(R.choice(["    ", "\t", ""]) + v[cut:])
        else:
            lines.append(R.choice(["", "", "  "]) + k + sep + v)
    eol = R.choice(["\n", "\r\n"])
    s = join_lines(lines, eol)
    if latin:
        try:
            return s.encode("latin-1")
        except UnicodeEncodeError:
            return s.encode("utf-8")
    return encode(s, DEF_ENCS)


# ---------------------------------------------------------------- ftl
def ftl_text():
    bits = []
    for _ in range(R.randint(1, 5)):
        r = R.random()
        if r < 0.6:
            bits.append(plain(1, 3).replace("{", "").replace("}", "").replace("[", "(").replace("*", "x").replace(".", ""))
        elif r < 0.75:
            bits.append("{ $" + R.choice(["name", "count", "n"]) + " }")
        elif r < 0.82:
            bits.append("{ -brand-name }")
        elif r < 0.86:
            bits.append('{ "{" }')
        elif r < 0.9:
            bits.append("{ NUMBER($n, minimumFractionDigits: 2) }")
        elif r < 0.94:
            bits.append("{$x}")
        else:
            bits.append("{ msg-ref }")
    return " ".join(bits) or "text"


def g_ftl():
    lines = []
    if R.random() < 0.3:
        lines.append("### " + plain(1, 3))
        lines.append("")
    for _ in range(R.randint(0, 9)):
        r = R.random()
        if r < 0.1:
            lines.append(R.choice(["# ", "## "]) + plain(1, 3))
        k = R.choice(["", "", "", "-"]) + R.choice(["hello", "title", "btn-save", "emails", "brand-name", "a"]) + str(R.randint(0, 99))
        r = R.random()
        if r < 0.55:
            lines.append(f"{k} = " + ftl_text())
        elif r < 0.7:
            lines.append(f"{k} =")
            for _ in range(R.randint(1, 3)):
                lines.append("    " + ftl_text())
        elif r < 0.85:
            lines.append(f"{k} = {{ ${R.choice(['n', 'count'])} ->")
            lines.append("    [one] " + ftl_text())
            if R.random() < 0.5:
                lines.append("    [few] " + ftl_text())
            lines.append("   *[other] " + ftl_text())
            lines.append("}")
        else:
            lines.append(f"{k} =" + R.choice(["", " " + ftl_text()]))
        if R.random() < 0.3:
            for _ in range(R.randint(1, 2)):
                lines.append("    ." + R.choice(["title", "label", "accesskey", "placeholder"]) + " = " + ftl_text())
        if R.random() < 0.6:
            lines.append("")
    if R.random() < 0.05:
        lines.append("broken = { $x")
    eol = R.choice(["\n", "\r\n"])
    return encode(join_lines(lines, eol), DEF_ENCS)


# ---------------------------------------------------------------- dtd and the XML resource formats
def xml_text(s, attr=False):
    s = s.replace("&", "&amp;").replace("<", "&lt;")
    if R.random() < 0.3:
        s = s.replace(">", "&gt;")
    if attr:
        s = s.replace('"', "&quot;")
    return s


def xml_rich():
    """XML character data: escaped text with now and then a character reference or a CDATA section."""
    s = xml_text(phrase(1, 6, 0.4))
    r = R.random()
    if r < 0.1:
        s += " &#x41;&#169;"
    elif r < 0.15:
        s += " <![CDATA[" + R.choice(["<b>x</b>", "a & b", plain(1, 2)]) + "]]>"
    elif r < 0.2:
        s = "\n    " + s + "\n  "
    return s


def g_dtd():
    lines = []
    for _ in range(R.randint(0, 10)):
        r = R.random()
        if r < 0.12:
            lines.append("<!-- " + plain(1, 3).replace("--", "-") + " -->")
            continue
        if r < 0.17:
            lines.append("")
            continue
        n = ident().replace("-", ".")
        v = phrase(1, 6, 0.2).replace("&", "&amp;").replace("%", "&#37;")
        if R.random() < 0.2:
            v += " &brandShortName;"
        q = R.choice(['"', "'"])
        v = v.replace(q, "&quot;" if q == '"' else "&apos;")
        lines.append(f"<!ENTITY {n}{R.choice([' ', '  ', chr(10) + '  '])}{q}{v}{q}>")
    if R.random() < 0.1:
        lines.append('<!ENTITY % ext SYSTEM "x.dtd">')
    eol = R.choice(["\n", "\r\n"])
    return encode(join_lines(lines, eol), DEF_ENCS)


def g_resx():
    eol = R.choice(["\n", "\r\n"])
    out = ['<?xml version="1.0" encoding="utf-8"?>', "<root>", '  <resheader name="resmimetype"><value>text/microsoft-resx</value></resheader>']
    for _ in range(R.randint(0, 9)):
        n = ident()
        r = R.random()
        sp = R.choice([' xml:space="preserve"', ""])
        if r < 0.1:
            out.append(f'  <data name="{n}" type="System.Drawing.Size, System.Drawing"><value>10, 20</value></data>')
        elif r < 0.15:
            out.append(f'  <data name="{n}" mimetype="application/x-microsoft.net.object.binary.base64"><value>AAEAAAD/</value></data>')
        elif r < 0.2:
            out.append(f'  <data name="{n}"{sp}><value></value></data>')
        elif r < 0.23:
            out.append(f'  <data name="{n}"{sp} />')
        else:
            c = f"<comment>{xml_text(plain(1, 3))}</comment>" if R.random() < 0.3 else ""
            if R.random() < 0.5:
                out.append(f'  <data name="{n}"{sp}>')
                out.append(f"    <value>{xml_rich()}</value>")
                if c:
                    out.append("    " + c)
                out.append("  </data>")
            else:
                out.append(f'  <data name="{n}"{sp}><value>{xml_rich()}</value>{c}</data>')
        if R.random() < 0.1:
            out.append("  <!-- " + plain(1, 2).replace("--", "") + " -->")
    out.append("</root>")
    return encode(eol.join(out) + eol, DEF_ENCS)


def g_wxl():
    eol = R.choice(["\n", "\r\n"])
    v4 = R.random() < 0.3
    ns = "http://wixtoolset.org/schemas/v4/wxl" if v4 else "http://schemas.microsoft.com/wix/2006/localization"
    out = ['<?xml version="1.0" encoding="utf-8"?>', f'<WixLocalization Culture="{R.choice(["en-us", "en-US"])}" xmlns="{ns}">']
    for _ in range(R.randint(0, 9)):
        i = ident()
        txt = xml_rich() if R.random() < 0.8 else ""
        txt = txt.replace("[1]", "[ProductName]")
        ov = R.choice(["", ' Overridable="yes"', ' Localizable="no"'])
        if v4 and R.random() < 0.6:
            out.append(f'  <String Id="{i}"{ov} Value="{xml_text(phrase(1, 5, 0.3), True)}" />')
        else:
            out.append(f'  <String Id="{i}"{ov}>{txt}</String>')
        if R.random() < 0.1:
            out.append("  <!-- c -->")
        if R.random() < 0.05:
            out.append(f'  <UI Id="{i}" Text="{xml_text(plain(1, 2), True)}" />')
    out.append("</WixLocalization>")
    return encode(eol.join(out) + eol, DEF_ENCS)


def g_xaml():
    eol = R.choice(["\n", "\r\n"])
    if R.random() < 0.8:
        out = ['<?xml version="1.0" encoding="utf-8"?>',
               '<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"',
               '                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"',
               '                    xmlns:system="clr-namespace:System;assembly=mscorlib">']
        for _ in range(R.randint(0, 8)):
            k = ident()
            r = R.random()
            if r < 0.1:
                out.append(f'    <SolidColorBrush x:Key="{k}" Color="#FF0078D7"/>')
            elif r < 0.15:
                out.append(f'    <x:Double x:Key="{k}">12</x:Double>')
            else:
                v = R.choice([xml_rich(), "{Binding Name}", "{}{0} files", "", xml_text(phrase(1, 4, 0.5))])
                out.append(f'    <system:String x:Key="{k}">{v}</system:String>')
        out.append("</ResourceDictionary>")
    else:
        out = [f'<Window x:Class="App.Main" Title="{xml_text(plain(1, 3), True)}"',
               '        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"',
               '        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">',
               "  <StackPanel>"]
        for _ in range(R.randint(0, 8)):
            el = R.choice(["Button", "TextBlock", "Label", "CheckBox", "MenuItem", "TabItem", "Run"])
            at = R.choice(["Content", "Text", "ToolTip", "Header", "Title", "Tag", "Name", "PlaceholderText"])
            v = R.choice([xml_text(phrase(1, 4, 0.3), True), "{Binding X}", "{}{0} items", "42", "", "{StaticResource Ok}"])
            if R.random() < 0.2:
                out.append(f"    <{el}>{xml_text(plain(1, 3))}</{el}>")
            else:
                out.append(f'    <{el} {at}="{v}"/>')
        out.append("  </StackPanel>")
        out.append("</Window>")
    return encode(eol.join(out) + eol, DEF_ENCS)


GEN = {
    "txt": (g_txt, "txt"), "md": (g_md, "md"), "srt": (g_srt, "srt"), "vtt": (g_vtt, "vtt"), "po": (g_po, "po"),
    "json": (g_json, "json"), "json5": (g_json5, "json5"), "resjson": (g_resjson, "resjson"), "arb": (g_arb, "arb"),
    "strings": (g_strings, "strings"), "properties": (g_properties, "properties"), "ftl": (g_ftl, "ftl"),
    "dtd": (g_dtd, "dtd"), "resx": (g_resx, "resx"), "wxl": (g_wxl, "wxl"), "xaml": (g_xaml, "xaml"),
}

INJECT = {
    "srt": ["one\n\n99\n00:00:01,000 --> 00:00:02,000\ninjected cue", "a\r\n\r\nb", "-->", "x\n\n"],
    "vtt": ["one\n\n00:00:01.000 --> 00:00:02.000\ninjected cue", "<script>x</script>", "NOTE\n\nx", "a --> b", "<i>open"],
    "po": ['x"\nmsgid "evil"\nmsgstr "y', "a\nb", "\\", '"', "line\n\nmsgid \"z\""],
    "md": ["__保存__をクリック", "**「変更を保存」**をクリック", "参见https://example.com/a?b=1&c=2。", "见www.example.org了",
           "_x_y", "one\n\n# injected heading", "two\n- item", "```\ncode", "<!-- hide", "---\ntitle: x\n---", "a  \nb", "| a | b |", "1. x", "# h", "[x]: https://evil"],
    "txt": ["a\nb", "a\r\nb", "\u2028x"],
}
COMMON_INJECT = ['"; evil = "x', "</value><data name=\"evil\"><value>x", "]]>", "<!--", "&amp;&lt;", "\\u0041", "}\nevil = x",
                 "{ $var } and { -brand-name }", "{", "\n.attr = x", "⟦9⟧ forged ⟦/9⟧", "⟦x1⟧", "${'$'}", "a\nb"]


def golden(fmt, path):
    try:
        g = og.golden(fmt, path)
    except Exception as exc:  # LocOS refuses the file
        return {"format": fmt, "parse_error": f"{type(exc).__name__}: {exc}"}
    with open(path, "rb") as fh:
        data = fh.read()
    filt = og.get_filter(fmt)
    pool = INJECT.get(fmt, []) + COMMON_INJECT
    mapping = {str(u["p_index"]): R.choice(pool) for u in g["units"]}
    try:
        out = filt.build_localized(data, mapping, "de-DE")
        g["builds"].append({"name": "inject", "locale": "de-DE", "map": mapping, "out_b64": base64.b64encode(out).decode()})
    except Exception as exc:
        g["builds"].append({"name": "inject", "locale": "de-DE", "map": mapping, "error": f"{type(exc).__name__}: {exc}"})
    return g


def main():
    args = sys.argv[1:]
    out = args.pop(0)
    n, seed = 300, 1
    fmts = []
    while args:
        a = args.pop(0)
        if a == "--n":
            n = int(args.pop(0))
        elif a == "--seed":
            seed = int(args.pop(0))
        else:
            fmts.append(a)
    for fmt in fmts or sorted(GEN):
        gen, ext = GEN[fmt]
        d = os.path.join(out, fmt)
        os.makedirs(d, exist_ok=True)
        refused = 0
        global LATIN
        for k in range(n):
            R.seed(f"{seed}-{fmt}-{k}")
            LATIN = fmt in ("txt", "srt", "vtt", "po", "properties", "md") and R.random() < 0.25
            p = os.path.join(d, f"r{seed}_{k:04d}.{ext}")
            with open(p, "wb") as fh:
                fh.write(gen())
            g = golden(fmt, p)
            refused += "parse_error" in g
            with open(p + ".golden.json", "w", encoding="utf-8", newline="\n") as fh:
                json.dump(g, fh, ensure_ascii=False)
        print(fmt, n, "files", refused, "refused by LocOS", flush=True)


if __name__ == "__main__":
    main()
