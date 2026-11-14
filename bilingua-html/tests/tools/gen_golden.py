"""Builds tests/data/locos_golden.json: HTML pages and what the LocOS HTML filter (app/filters/html_filter.py,
HtmlFilter) makes of them. Adapted from bilingua-serverpages/tests/tools/gen_golden.py.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    PYTHONIOENCODING=utf-8 <LocOS venv python> -I gen_golden.py OUT.json

Cases: random pages and XHTML documents in 11 languages, legacy code pages, byte mutations of random pages, the
hostile pages of LocOS's desync suite and more, .html pages with server code (LocOS routes them to the
server-page filters), and two filter variants: page_meta=False (EPUB, e-mail) and a template pattern.

For each page the golden holds the input bytes (hex), the filter variant, the units LocOS parses (text, style and
the inline map of each unit) or the refusal, and the bytes LocOS builds with four mappings: every unit translated
without tokens ("all"), every third unit ("third"), every unit's own text plus " Z" with tokens kept ("tok"), and
the tokens in reverse order ("swap"); plus build_localized of "tok" for two locales. Deterministic (seed 1106).
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-html-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.html_filter import HtmlFilter  # noqa: E402

TEMPLATE = r"\{\{.*?\}\}|\{%.*?%\}"

# 11 languages: (lang tag, words)
LANGS = {
    "en": ["Save", "Cancel", "Welcome back", "Hello, world!", "It's fine", 'He said "hi"', "Open the inbox", "Sentence one. Sentence two."],
    "fr": ["Caf\u00e9 cr\u00e8me", "N'arr\u00eatez pas", "\u00c0 bient\u00f4t", "Ouvrir la bo\u00eete"],
    "de": ["Gr\u00fc\u00dfe aus K\u00f6ln", "Stra\u00dfe", "\u00dcbersicht \u00f6ffnen"],
    "es": ["\u00bfD\u00f3nde est\u00e1?", "Ma\u00f1ana", "\u00a1Hola!"],
    "ru": ["\u041f\u0440\u0438\u0432\u0435\u0442, \u043c\u0438\u0440", "\u0421\u043e\u0445\u0440\u0430\u043d\u0438\u0442\u044c"],
    "ar": ["\u0645\u0631\u062d\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645", "\u062d\u0641\u0638"],
    "he": ["\u05e9\u05dc\u05d5\u05dd \u05e2\u05d5\u05dc\u05dd", "\u05e9\u05de\u05d5\u05e8"],
    "ja": ["\u65e5\u672c\u8a9e\u306e\u30c6\u30ad\u30b9\u30c8", "\u4fdd\u5b58\u3059\u308b", "\u3000\u5168\u89d2\u3000"],
    "zh": ["\u4e2d\u6587\u6587\u672c", "\u4fdd\u5b58"],
    "ko": ["\ud55c\uad6d\uc5b4 \ud14d\uc2a4\ud2b8", "\uc800\uc7a5"],
    "hi": ["\u0928\u092e\u0938\u094d\u0924\u0947 \u0926\u0941\u0928\u093f\u092f\u093e", "\u0938\u0939\u0947\u091c\u0947\u0902"],
}
EXTRA = [
    "Tom &amp; Jerry", "a &lt; b", "5 &gt; 3", "Price: 5 &euro;", "a < b", "R&D team", "Hello&nbsp;world", "x", "42", "---", "&#169; 2026",
    " emspace ", "emoji \U0001F600 here", " nbsp ", "tab\there", "x\u200by zero-width", "line one\nline two", "  padded  ", "&unknown; ref",
    "&amp;amp; twice", "&#x27;quoted&#39;", "a > b", "{{ user.name }}", "{% if a %}x{% endif %}", "Done",
]
ALTS = ["Picture of a cat", "Company logo", "12345", "Fotograf\u00eda", "A &amp; B", 'Say &quot;hi&quot;', "", "\u65e5\u672c", "  spaced  "]

INLINE = [
    "<b>{w}</b>", "<em>{w}</em>", "<strong>{w}</strong>", "<a href=\"/x\" title=\"Open the inbox\">{w}</a>", "<a href=\"/y\" aria-label=\"Go home\">{w}</a>",
    "<span class=\"notranslate\">{w}</span>", "<span translate=\"no\">{w}</span>", "<code>{w}</code>", "<kbd>Ctrl</kbd>", "<br>", "<br/>", "<br />",
    "<img src=\"a.png\" alt=\"{alt}\">", "<img src=\"b.png\">", "<img alt='{alt}' title=\"Image title\"/>", "<b>{w}", "</i>", "<i><b>{w}</i></b>",
    "<small>{w}</small>", "<abbr title=\"World Health Organization\">WHO</abbr>", "<wbr>", "<u>{w}</u>", "<sup>2</sup>", "<ruby>\u6f22<rt>kan</rt></ruby>",
    "<label>{w}</label>", "<input type=\"text\" placeholder=\"Type\">", "<strike>{w}</strike>", "<q>{w}</q>", "<span lang=\"fr\">{w}</span>",
    "<A HREF=\"/z\">{w}</A>", "<span title=\"a\"title=\"b\">{w}</span>", "<span data-x='{{{{ x }}}}'>{w}</span>",
]
BLOCK = [
    "<p>{c}</p>", "<h1>{c}</h1>", "<h2 class=\"t\">{c}</h2>", "<li>{c}</li>", "<div>{c}</div>", "<td>{c}</td>", "<th>{c}</th>", "<p>{c}",
    "<button>{c}</button>", "<p translate=\"no\">{c}</p>", "<div class=\"notranslate\"><p>{c}</p></div>",
    "<div translate=\"no\"><p translate=\"yes\">{c}</p></div>", "<pre>{c}</pre>", "<option>{c}</option>", "<blockquote>{c}</blockquote>",
    "<figure><figcaption>{c}</figcaption></figure>", "<details><summary>{c}</summary>{c}</details>", "<dl><dt>{c}<dd>{c}</dl>",
    "<section>{c}</section>", "<textarea>{c}</textarea>", "<P>{c}</P>",
]
OTHER = [
    "<script>var a = \"<p>x</p>\"; if (a<b) {{}}</script>", "<style>p {{ color: red }}</style>", "<!-- a comment -->", "<!-- <p>commented</p> -->",
    "<meta name=\"description\" content=\"A page about things\">", "<meta property=\"og:title\" content=\"Big title\">",
    "<meta name=\"keywords\" content=\"a, b, c\">", "<meta name=\"twitter:description\" content=\"Short &amp; sweet\">", "<meta charset=\"utf-8\">",
    "<meta http-equiv=\"content-language\" content=\"en\">", "<meta name=description content=\"Unquoted name\">", "<meta name=\"robots\" content=\"index\">",
    "<hr>", "<input type=\"submit\" value=\"Go\">", "<img src=\"c.png\" alt=\"{alt}\">", "<![CDATA[ x ]]>", "<svg><title>t</title><text>Svg text</text></svg>",
    "<a title=\"Only a title\"><img alt=\"Logo\"></a>", "<area alt=\"Area text\" href=\"#\">", "<table><tr><td>{w}</td><td>{w}</td></tr></table>",
    "<ul><li>{w}<li>{w}</ul>", "<select><option>{w}</option><option selected>{w}</option></select>", "<noscript><p>{w}</p></noscript>",
    "<template><p>{w}</p></template>", "<math><mi>x</mi></math>", "<iframe src=\"x\"></iframe>", "<?php-like processing instruction?>",
]
HOSTILE = [
    '<!--><a title="--> <p>Hi <img alt="cat"> there</p>',
    '<svg><style><a title="</style><img alt="cat"></svg>',
    '<xmp><a title="</xmp><img alt="cat">',
    '<!---><a title="--> <p>Hi <img alt="cat"> there</p>',
    '<!-- a --!><a title="--> <p>Hi <img alt="cat"> there</p>',
    '<p></ x <a title="> <img alt="cat"></p>',
    '<textarea><a title="</textarea><img alt="cat">',
    '<noscript><a title="</noscript><img alt="cat">',
    '<a title= = "x><img alt="cat">',
    '<![CDATA[><a title="]]><img alt="cat">',
    '<?x ><a title="?><img alt="cat">',
    '<math><style><a title="</style><img alt="cat"></math>',
    '<select><xmp><a title="</xmp><img alt="cat"></select>',
    "<p>Unclosed <b>bold and <i>italic",
    "<p>Text <!-- unclosed comment",
    "<p title=\"unclosed>Text</p><p>More</p>",
    "<plaintext><p>raw</p>",
    "<p>a</p></p></div></body></html><p>after</p>",
    "<script><!--<script></script>--></script><p>after</p>",
    "<p>" + "<b>" * 70 + "deep inline" + "</b>" * 70 + "</p>",
    "<div>" * 300 + "too deep" + "</div>" * 300,
    "<p>\x00nul</p>",
    "<p>" + "x" * 5000 + " long</p>",
    "<p a=" + "'" * 3 + ">Quotes</p>",
    "<p>\u27e6x1\u27e7 bracket text</p>",
    "<p>\ufeffinner bom</p>",
    "<p title='a\"b'>Mixed quotes</p>",
    "<img alt=\"x\" alt=\"Second alt\"><p>Dup</p>",
    "<p>a</b>b</i>c</p>",
    "<html><body><p>One<p>Two<li>Three<td>Four</body></html>",
    "<!DOCTYPE html><html lang=\"en\" dir=\"ltr\" xml:lang=\"en\"><head><meta http-equiv=\"Content-Language\" content=\"en\"></head><body><p>Lang</p></body></html>",
    "<p onclick=\"alert('x')\">Handler</p><a href=\"javascript:alert(1)\">Link</a>",
]
SERVER = [
    "<p>Hello <?php echo $name; ?>, welcome.</p>",
    "<html><body><h1><?= $title ?></h1><p>Body text</p></body></html>",
    "<p>Today is <%= date %> here</p>",
    "<%@ taglib uri=\"/x\" prefix=\"c\" %><p>Hello ${user} and <%= x %></p>",
    "<p>Mixed <?php echo 1; ?> and <% x %></p>",
    "<p>Open <?php but never closed</p>",
    "<p>Price <% if (a) { %>high<% } %></p>",
    "<p>Not code: a <? b</p>",
]


def word(rng, lang):
    if rng.random() < 0.25:
        return rng.choice(EXTRA)
    return rng.choice(LANGS[lang] + LANGS["en"][:2])


def content(rng, lang):
    c = ""
    for _ in range(rng.randint(1, 5)):
        q = rng.random()
        if q < 0.55:
            c += word(rng, lang) + rng.choice([" ", " ", ", ", ""])
        else:
            c += rng.choice(INLINE).format(w=word(rng, lang), alt=rng.choice(ALTS))
    return c.rstrip() if rng.random() < 0.7 else c


def make(rng, lang, xhtml=False):
    nl = rng.choice(["\n", "\n", "\r\n", ""])
    parts = []
    closing = ""
    shape = rng.random()
    if xhtml:
        decl = rng.choice(['<?xml version="1.0" encoding="UTF-8"?>' + nl, ""])
        dt = rng.choice(["<!DOCTYPE html>", '<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Strict//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd">', ""])
        parts.append(f'{decl}{dt}{nl}<html xmlns="http://www.w3.org/1999/xhtml" lang="{lang}" xml:lang="{lang}">{nl}<head>{nl}')
        parts.append(f"<title>{rng.choice(LANGS[lang])}</title>{nl}")
        if rng.random() < 0.5:
            parts.append('<meta name="description" content="' + rng.choice(LANGS[lang]) + '"/>' + nl)
        parts.append(f"</head>{nl}<body>{nl}")
        for _ in range(rng.randint(1, 7)):
            r = rng.random()
            w = rng.choice(LANGS[lang])
            if r < 0.5:
                parts.append(rng.choice(["<p>{c}</p>", "<h1>{c}</h1>", "<li>{c}</li>", "<div><p>{c}</p></div>", "<td>{c}</td>"]).format(
                    c=rng.choice([w, w + " <b>" + w + "</b>", w + " <br/> " + w, '<img src="a.png" alt="' + w + '"/> ' + w, "<em>" + w + "</em>.",
                                  w + " &amp; " + w, w + " &#169;", w + " &nbsp; " + w, '<a href="#" title="' + w + '">' + w + "</a>"])) + nl)
            elif r < 0.65:
                parts.append(rng.choice(["<script>var a = 1 &amp;&amp; 2;</script>", "<style>p {} </style>", "<!-- note -->",
                                         "<script><![CDATA[ if (a < b) {} ]]></script>", "<hr/>", "<br/>"]) + nl)
            elif r < 0.8:
                parts.append('<p><span xml:lang="fr" lang="fr">' + w + "</span> " + w + "</p>" + nl)
            else:
                parts.append("<ul>" + "".join("<li>" + rng.choice(LANGS[lang]) + "</li>" for _ in range(rng.randint(1, 3))) + "</ul>" + nl)
        parts.append(f"</body>{nl}</html>{nl}")
        return "".join(parts)
    if shape < 0.6:
        lang_attr = rng.choice([f' lang="{lang}"', f' lang="{lang}" dir="ltr"', "", f" LANG={lang}", f' lang="{lang}" xml:lang="{lang}"'])
        parts.append(rng.choice(["<!DOCTYPE html>", "<!doctype html>", ""]) + f"{nl}<html{lang_attr}>{nl}<head>{nl}")
        for _ in range(rng.randint(0, 4)):
            parts.append(rng.choice(OTHER[4:12]).format(alt="x", w="y") + nl)
        parts.append(f"<title>{content(rng, lang) if rng.random() < 0.3 else word(rng, lang)}</title>{nl}</head>{nl}<body>{nl}")
        closing = f"</body>{nl}</html>{nl}"
    for _ in range(rng.randint(1, 10)):
        r = rng.random()
        if r < 0.5:
            parts.append(rng.choice(BLOCK).format(c=content(rng, lang)) + nl)
        elif r < 0.75:
            parts.append(rng.choice(OTHER).format(alt=rng.choice(ALTS), w=word(rng, lang)) + nl)
        elif r < 0.85:
            parts.append(rng.choice(["Loose text ", "", "\u00a0", "  "]) + word(rng, lang) + nl)
        else:
            tag = rng.choice(['<a href="/x" title="{w}">{w}</a>', '<img src="i.png" alt="{w}" title="{w}">', '<div title="{w}">{w}</div>',
                              '<p aria-label="{w}">{w}</p>', '<area alt="{w}">', '<a aria-label="{w}" title="T">x</a>'])
            parts.append(tag.format(w=word(rng, lang)) + nl)
    parts.append(closing)
    return "".join(parts)


LEGACY = [
    ("windows-1252", "fr"), ("windows-1252", "de"), ("iso-8859-1", "es"), ("shift_jis", "ja"), ("euc-kr", "ko"), ("gb2312", "zh"),
    ("koi8-r", "ru"), ("windows-1256", "ar"), ("iso-8859-8", "he"), ("windows-1251", "ru"),
]


def make_legacy(rng, charset, lang):
    for _ in range(50):
        body = "".join(
            rng.choice(["<p>{c}</p>", "<h1>{c}</h1>", "<li>{c}</li>"]).format(c=rng.choice(LANGS[lang]) + rng.choice(["", " <b>x</b>", " &amp; y"]))
            + "\n"
            for _ in range(rng.randint(1, 5))
        )
        s = f'<html><head><meta charset="{charset}"><title>{rng.choice(LANGS[lang])}</title></head><body>\n{body}</body></html>\n'
        try:
            return s.encode(charset)
        except UnicodeEncodeError:
            continue
    return None


MUT = ["<", ">", '"', "'", "=", "/", "&", ";", "!", "-", " ", "\n", "<!--", "-->", "</", "<b>", "</p>", "<textarea>", "<xmp>", "<svg><style>",
       "</ x>", "<script>", "]]>", "<![CDATA[", "&amp;", "\u00e9", "\u65e5"]


def mutate(rng, s):
    s = list(s)
    for _ in range(rng.randint(1, 4)):
        if not s:
            break
        k = rng.randrange(len(s))
        op = rng.random()
        if op < 0.4:
            s.insert(k, rng.choice(MUT))
        elif op < 0.7:
            del s[k]
        else:
            s[k] = rng.choice(MUT)
    out = "".join(s)
    if rng.random() < 0.15:
        out = out[: rng.randrange(len(out) + 1)]
    return out


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
    allm = {str(i): 'T%d "q" \u00e9 <b> & &amp; ${x} {{ v }} \\' % i for i in range(n)}
    third = {str(i): "Zeta %d \u65e5 'it's' &nbsp;" % i for i in range(0, n, 3)}
    tok = {str(i): u["text"] + " Z" for i, u in enumerate(units)}
    swap = {}
    for i, u in enumerate(units):
        toks = TOK.findall(u["text"])
        if toks:
            swap[str(i)] = " ".join(reversed(toks)) + " Q"
    return {"all": allm, "third": third, "tok": tok, "swap": swap}


def make_filter(cfg):
    if cfg == "nometa":
        return HtmlFilter(page_meta=False)
    if cfg == "template":
        return HtmlFilter(template_pattern=re.compile(TEMPLATE, re.S))
    return HtmlFilter()


def run(cfg, data):
    f = make_filter(cfg)
    case = {"hex": data.hex(), "cfg": cfg}
    try:
        pr = f.parse(data)
    except Exception as e:  # noqa: BLE001
        case["error"] = type(e).__name__
        case["message"] = str(e)
        return case
    units = dump_units(pr)
    case["units"] = units
    case["server_code"] = pr.skeleton.get("server_code")
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
    rng = random.Random(1106)
    cases = []

    def add(name, data, cfg="default", kind="random"):
        c = run(cfg, data)
        c["name"] = name
        c["kind"] = kind
        cases.append(c)

    langs = list(LANGS)
    for i in range(330):
        lang = langs[i % len(langs)]
        data = make(rng, lang).encode("utf-8")
        if rng.random() < 0.1:
            data = b"\xef\xbb\xbf" + data
        add(f"random-{lang}-{i:03d}", data)
    for i in range(66):
        lang = langs[i % len(langs)]
        add(f"xhtml-{lang}-{i:03d}", make(rng, lang, xhtml=True).encode("utf-8"), kind="xhtml")
    for i in range(30):
        charset, lang = LEGACY[i % len(LEGACY)]
        data = make_legacy(rng, charset, lang)
        if data is not None:
            add(f"legacy-{charset}-{i:03d}", data, kind="legacy")
    for i in range(90):
        lang = langs[i % len(langs)]
        add(f"mutated-{i:03d}", mutate(rng, make(rng, lang, xhtml=rng.random() < 0.3)).encode("utf-8"), kind="mutated")
    for i, h in enumerate(HOSTILE):
        add(f"hostile-{i:02d}", h.encode("utf-8"), kind="hostile")
    for i, h in enumerate(SERVER):
        add(f"server-{i:02d}", h.encode("utf-8"), kind="server")
    for i in range(40):
        lang = langs[i % len(langs)]
        add(f"nometa-{lang}-{i:03d}", make(rng, lang, xhtml=rng.random() < 0.3).encode("utf-8"), cfg="nometa")
    for i in range(40):
        lang = langs[i % len(langs)]
        add(f"template-{lang}-{i:03d}", make(rng, lang).encode("utf-8"), cfg="template")
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    ok = sum(1 for c in cases if "error" not in c)
    print(f"{len(cases)} cases, {ok} parsed, {len(cases) - ok} refused")


main()
