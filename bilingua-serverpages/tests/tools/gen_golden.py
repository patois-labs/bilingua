"""Builds tests/data/locos_golden.json: random server pages of every dialect and what the LocOS filters make of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    PYTHONIOENCODING=utf-8 <LocOS venv python> -I gen_golden.py OUT.json [FIXTURE_FILE ...]

For each page the golden holds the input bytes (hex), the units LocOS parses (text, style and the inline map of
each unit: open/close/point markup by token number) or the refusal, and the bytes LocOS builds with four
mappings: every unit translated without tokens ("all"), every third unit ("third"), every unit's own text plus
" Z" with tokens kept ("tok"), and the tokens in reverse order ("swap"); plus build_localized for two locales.
Deterministic (seed 1105).
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-sp-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Hello, world!", "It's fine", 'He said "hi"', "Caf\u00e9 cr\u00e8me", "\u65e5\u672c\u8a9e\u306e\u30c6\u30ad\u30b9\u30c8",
    "\u0645\u0631\u062d\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645", "\ud55c\uad6d\uc5b4 \ud14d\uc2a4\ud2b8", "\u4e2d\u6587\u6587\u672c", "Tom &amp; Jerry", "a &lt; b", "5 &gt; 3",
    "Price: 5 &euro;", "a < b", "R&D team", "Hello&nbsp;world", "x", "42", "---", "Sentence one. Sentence two.", "Click here", "Open the inbox",
    "Total", "&#169; 2026", "　全角　", " emspace ", "emoji 😀 here", " nbsp ", "tab	here", "x​y zero-width", "line one\nline two", "  padded  ", "Done",
]
ALTS = ["Picture of a cat", "Company logo", "12345", "Fotograf\u00eda", "A &amp; B", "Say &quot;hi&quot;", "", "\u65e5\u672c"]

HTML_INLINE = [
    "<b>{w}</b>", "<em>{w}</em>", "<a href=\"/x\" title=\"Open the inbox\">{w}</a>", "<a href=\"/y\" aria-label=\"Go home\">{w}</a>",
    "<span class=\"notranslate\">{w}</span>", "<span translate=\"no\">{w}</span>", "<code>{w}</code>", "<br>", "<br/>",
    "<img src=\"a.png\" alt=\"{alt}\">", "<img src=\"b.png\">", "<b>{w}", "</i>", "<i><b>{w}</i></b>", "<small>{w}</small>",
    "<abbr title=\"World Health Organization\">WHO</abbr>", "<wbr>", "<u>{w}</u>",
]
HTML_BLOCK = [
    "<p>{c}</p>", "<h1>{c}</h1>", "<li>{c}</li>", "<div>{c}</div>", "<td>{c}</td>", "<p>{c}", "<h2 class=\"t\">{c}</h2>", "<button>{c}</button>",
    "<p translate=\"no\">{c}</p>", "<div class=\"notranslate\"><p>{c}</p></div>", "<div translate=\"no\"><p translate=\"yes\">{c}</p></div>",
    "<label>{c}</label>", "<pre>{c}</pre>", "<option>{c}</option>", "<title>{c}</title>",
]
HTML_OTHER = [
    "<script>var a = \"<p>x</p>\"; if (a<b) {{}}</script>", "<style>p {{ color: red }}</style>", "<!-- a comment -->", "<!DOCTYPE html>",
    "<meta name=\"description\" content=\"A page about things\">", "<meta property=\"og:title\" content=\"Big title\">",
    "<meta name=\"keywords\" content=\"a, b, c\">", "<meta charset=\"utf-8\">", "<meta http-equiv=\"content-language\" content=\"en\">",
    "<meta name=viewport content=\"width=device-width\">", "<hr>", "<input type=\"submit\" value=\"Go\">", "<img src=\"c.png\" alt=\"{alt}\">",
    "<![CDATA[ x ]]>", "<textarea placeholder=\"Say\">hello</textarea>", "<svg><title>t</title></svg>", "<a title=\"Only a title\"><img alt=\"Logo\"></a>",
    "<area alt=\"Area text\">", "<table><tr><td>{w}</td><td>{w}</td></tr></table>", "<ul><li>{w}<li>{w}</ul>",
]

PHP_OUT = ["<?= $n ?>", "<?php echo $x; ?>", "<?php print \"a\"; ?>", "<?=$a?>", "<?php echo htmlspecialchars($u) ?>", "<?PHP ECHO $y ?>"]
PHP_STMT = [
    "<?php if ($a): ?>", "<?php endif; ?>", "<?php foreach ($l as $i) { ?>", "<?php } ?>", "<?php // note ?>", "<?php\n$b = <<<EOT\n<p>x ?></p>\nEOT;\n?>",
    "<? $short = 1 ?>", "<?php # hash ?>", "<?php /* c ?> */ ?>", "<?php $s = 'a ?> b'; ?>", "<?php else: ?>", "<?php\nfoo();\n?>", "<?xml version=\"1.0\"?>",
    "<?php $x = \"<div\"; echo '>'; ?>",
]
PHP_ATTR = ['href="<?php echo url("a\\"b>"); ?>"', 'alt="<?= $alt ?>"', 'class="<?= $c ?> x"', 'title="<?php echo $t ?>"', "data-x='<?= $d ?>'", 'id=<?= $i ?>']
PHP_BAD = ["<<?= $tag ?>>x</<?= $tag ?>>", "<div <?php echo $attrs ?>>x</div>", "<p>Hi <?php echo 'oops ?></p>", "<p><?php /* never ?></p>", "<?php $x = <<<EOT\nno end\n",
           "<p>\ufdd0x</p>", '<?php echo "<div"; ?> class="x">Hello</div>']

JSP_OUT = ["<%= date %>", "${user.name}", "#{bean.x}", "<jsp:expression>x</jsp:expression>", "<%=a%>", "${a['b']}", "${f(\"}\")}"]
JSP_STMT = [
    "<% for (int i = 0; i < 3; i++) { %>", "<% } %>", "<%@ page contentType=\"text/html; charset=UTF-8\" %>", "<%-- page comment with <p>markup</p> --%>",
    "<jsp:scriptlet>String s = \"<p>no</p>\";</jsp:scriptlet>", "<%! int x; %>", "<%@ taglib uri=\"/x\" prefix=\"c\" %>", "<jsp:declaration>int y;</jsp:declaration>",
    "<c:if test=\"${a}\">", "</c:if>", "<%@ include file=\"h.jsp\" %>",
]
JSP_ATTR = ['class="${cls}"', 'href="<%= u %>"', "title='${t}'", 'alt="<%= a %> pic"', 'value="${x} y"']
JSP_BAD = ["<p>Hi <% int x = 1; </p>", "<p>Hi ${user.name</p>", "<jsp:scriptlet>int x;", "<input <%= disabled %> value=\"x\">", "<<%= t %>>x"]

ASPX_OUT = ["<%= x %>", "<%: Model.Name %>", "<%# Eval(\"Title\") %>", "<%$ Resources:Strings, Hello %>", "<%=x%>"]
ASPX_STMT = [
    "<% if (show) { %>", "<% } %>", "<%@ Page Language=\"C#\" AutoEventWireup=\"true\" %>", "<%-- comment --%>", "<asp:Label runat=\"server\" Text=\"Server label\" />",
    "<asp:Button ID=\"b\" runat=\"server\" Text=\"Go\" />", "<%@ Register TagPrefix=\"x\" %>", "<% foreach (var i in items) { %>",
]
ASPX_ATTR = ['value="<%= x %>"', 'title="<%# Eval("T") %>"', 'class="a <%: c %>"', 'alt="<%= a %>"']
ASPX_BAD = ["<input <%= disabled %> value=\"x\">", "<p>Hi <%-- never closed</p>", "<p>x <% y</p>"]

ERB_OUT = ["<%= t('.title') %>", "<%== raw %>", "<%= item.price %>", "<%= link_to 'a', x %>", "<%%"]
ERB_STMT = ["<% @items.each do |item| -%>", "<% end -%>", "<%# a comment %>", "<% if a %>", "<% else %>", "<%- x -%>", "<%= a %%> b %>"]
ERB_ATTR = ['href="<%= url %>"', 'alt="<%= a %>"', 'class="<%= c %> z"', 'title="<%= t %>"']
ERB_BAD = ["<p>Hi <% x</p>", "<div <%= a %>>x</div>"]

SSI_OUT = ['<!--#echo var="LAST_MODIFIED" -->', '<!--#fsize file="a.zip" -->', '<!--#flastmod virtual="/x" -->']
SSI_STMT = ['<!--#include virtual="/header.html" -->', '<!--#config timefmt="%Y" -->', '<!--#set var="a" value="b" -->', '<!--#exec cmd="ls" -->']
SSI_ATTR = ['href="<!--#echo var="U" -->"', 'alt="<!--#echo var="A" -->"']
SSI_BAD = ["<p><!--#echo var='x' </p>", "<p>\ufdd1</p>"]

DIALECTS = {
    "phtml": (PHP_OUT, PHP_STMT, PHP_ATTR, PHP_BAD),
    "jsp": (JSP_OUT, JSP_STMT, JSP_ATTR, JSP_BAD),
    "aspx": (ASPX_OUT, ASPX_STMT, ASPX_ATTR, ASPX_BAD),
    "erb": (ERB_OUT, ERB_STMT, ERB_ATTR, ERB_BAD),
    "shtml": (SSI_OUT, SSI_STMT, SSI_ATTR, SSI_BAD),
}


def make(rng, fmt):
    out_i, stmt_i, attr_i, bad = DIALECTS[fmt]
    nl = rng.choice(["\n", "\n", "\r\n"])
    parts = []
    if rng.random() < 0.5:
        lang = rng.choice(['lang="en"', 'lang="<?= $l ?>"' if fmt == "phtml" else 'lang="${l}"' if fmt == "jsp" else 'lang="en-US" xml:lang="en-US"', 'dir="ltr" lang="en"', ""])
        parts.append(f"<!DOCTYPE html>{nl}<html {lang}>{nl}<head>{nl}")
        for _ in range(rng.randint(0, 3)):
            parts.append(rng.choice(HTML_OTHER[:12]).format(alt="x", w="y", c="z") + nl)
        parts.append(f"<title>{rng.choice(WORDS)}</title>{nl}</head>{nl}<body>{nl}")
        closing = f"</body>{nl}</html>{nl}"
    else:
        closing = ""
    for _ in range(rng.randint(1, 9)):
        r = rng.random()
        if r < 0.40:
            n = rng.randint(1, 5)
            c = ""
            for _ in range(n):
                q = rng.random()
                if q < 0.4:
                    c += rng.choice(WORDS) + " "
                elif q < 0.6:
                    c += rng.choice(HTML_INLINE).format(w=rng.choice(WORDS), alt=rng.choice(ALTS))
                elif q < 0.8:
                    c += rng.choice(out_i) + rng.choice([" ", ", ", "", " and "])
                else:
                    c += rng.choice(stmt_i) + " " if rng.random() < 0.4 else rng.choice(WORDS) + " "
            parts.append(rng.choice(HTML_BLOCK).format(c=c.rstrip() if rng.random() < 0.7 else c, w="w") + nl)
        elif r < 0.55:
            parts.append(rng.choice(stmt_i) + nl)
        elif r < 0.70:
            parts.append(rng.choice(HTML_OTHER).format(alt=rng.choice(ALTS), w=rng.choice(WORDS)) + nl)
        elif r < 0.85:
            a = rng.choice(attr_i)
            tag = rng.choice(["<a {a}>{w}</a>", "<img src=\"i.png\" {a}>", "<div {a}>{w}</div>", "<p {a} title=\"Plain title\">{w}</p>", "<input {a} type=\"text\">"])
            parts.append(tag.format(a=a, w=rng.choice(WORDS)) + nl)
        elif r < 0.93:
            parts.append(rng.choice(["Loose text ", "", "\u00a0", "  "]) + rng.choice(out_i) + " tail" + nl)
        else:
            parts.append(f"<p>{rng.choice(WORDS)}" + rng.choice(["</p>", "", "<br>", "<i>"]) + nl)
    if rng.random() < 0.12:
        parts.append(rng.choice(bad) + nl)
    parts.append(closing)
    s = "".join(parts)
    data = s.encode("utf-8")
    if rng.random() < 0.15:
        data = b"\xef\xbb\xbf" + data
    return data


LOCALES = ["fr-FR", "ar"]


def dump_units(pr):
    out = []
    for u in pr.units:
        m = {}
        for k, v in (u.inline_map or {}).items():
            m[k] = {a: (str(b) if b is not None else None) for a, b in v.items()}
        out.append({"text": u.text, "style": u.style, "map": m})
    return out


TOK = re.compile("\u27e6/?[a-z]?\\d+\u27e7")


def mappings(units):
    n = len(units)
    allm = {str(i): 'T%d "q" \u00e9 <b> & &amp; ${x} #{y} \\' % i for i in range(n)}
    third = {str(i): "Zeta %d \u65e5 'it's'" % i for i in range(0, n, 3)}
    tok = {str(i): u["text"] + " Z" for i, u in enumerate(units)}
    swap = {}
    for i, u in enumerate(units):
        toks = TOK.findall(u["text"])
        if toks:
            swap[str(i)] = " ".join(reversed(toks)) + " Q"
    return {"all": allm, "third": third, "tok": tok, "swap": swap}


def run(f, data):
    case = {"hex": data.hex()}
    try:
        pr = f.parse(data)
    except Exception as e:  # noqa: BLE001
        case["error"] = type(e).__name__
        case["message"] = str(e)
        return case
    units = dump_units(pr)
    case["units"] = units
    case["a11y_images"] = pr.skeleton.get("a11y_images")
    case["builds"] = {}
    for label, m in mappings(units).items():
        try:
            case["builds"][label] = f.build(data, m).hex()
        except Exception as e:  # noqa: BLE001
            case["builds"][label] = {"error": type(e).__name__, "message": str(e)}
    case["loc"] = {}
    for loc in LOCALES:
        try:
            case["loc"][loc] = f.build_localized(data, mappings(units)["tok"], loc).hex()
        except Exception as e:  # noqa: BLE001
            case["loc"][loc] = {"error": type(e).__name__, "message": str(e)}
    return case


def main():
    out_path = sys.argv[1]
    fixtures = []
    for p in sys.argv[2:]:
        base = os.path.basename(p)
        fixtures.append((base, base.rsplit(".", 1)[-1], open(p, "rb").read()))
    cases = []
    for name, ext, data in fixtures:
        f = get_filter(ext)
        c = run(f, data)
        c["name"] = name
        c["fmt"] = f.fmt
        c["fixture"] = True
        cases.append(c)
    rng = random.Random(1105)
    for fmt in DIALECTS:
        f = get_filter(fmt)
        for i in range(150):
            data = make(rng, fmt)
            c = run(f, data)
            c["name"] = f"random-{fmt}-{i:03d}"
            c["fmt"] = fmt
            cases.append(c)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    ok = sum(1 for c in cases if "error" not in c)
    print(f"{len(cases)} cases, {ok} parsed, {len(cases) - ok} refused")


main()
