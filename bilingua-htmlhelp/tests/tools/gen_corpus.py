"""Writes the made-up HTML Help corpus into tests/fixtures/corpus (run once; the files are committed).

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_corpus.py ../fixtures/corpus

The oracle corpus (oracle/corpus/htmlhelp) is a copy of the files a Spanish pseudo-translation fits: every project
file, the English, German and French contents and index, and the UTF-8 (BOM) and UTF-16 contents files.

Twelve languages (English, German, French, Russian, Arabic, Japanese, Chinese simplified and traditional, Korean,
Greek, Vietnamese, Polish), each as a contents, an index and a project file in the Windows ANSI code page of the
language (what HTML Help Workshop writes), plus a UTF-8 (BOM) and a UTF-16 contents file. The only use of the LocOS
tree is `_cp1258_text`, to write Vietnamese the way Windows-1258 does (base letter + combining tone mark).
"""

import os
import sys

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

sys.dont_write_bytecode = True
sys.path.insert(0, os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms"))
os.environ.setdefault("LOC_TMS_DATA_DIR", os.path.join(os.environ.get("TEMP", "."), "locos-hh-corpus"))
from app.filters.htmlhelp_filter import _cp1258_text, target_lcid  # noqa: E402

# code, python codec, meta charset, GDI charset, Language= line, [started, order, delivery, faq, title, window]
LANGS = [
    ("en", "cp1252", "windows-1252", 0, "0x409 English (United States)",
     ["Getting started", "Order bread online", "Delivery and pickup", "Frequently asked questions", "Bakery Handbook", "Bakery Help"]),
    ("de", "cp1252", "windows-1252", 0, "0x407 German (Germany)",
     ["Erste Schritte", "Brot online bestellen", "Lieferung und Abholung", "H\u00e4ufig gestellte Fragen", "B\u00e4ckerei-Handbuch", "B\u00e4ckerei-Hilfe"]),
    ("fr", "cp1252", "windows-1252", 0, "0x40c French (France)",
     ["Premiers pas", "Commander du pain en ligne", "Livraison et retrait", "Questions fr\u00e9quentes", "Manuel de la boulangerie", "Aide de la boulangerie"]),
    ("ru", "cp1251", "windows-1251", 204, "0x419 Russian",
     ["\u041d\u0430\u0447\u0430\u043b\u043e \u0440\u0430\u0431\u043e\u0442\u044b", "\u0417\u0430\u043a\u0430\u0437 \u0445\u043b\u0435\u0431\u0430 \u043e\u043d\u043b\u0430\u0439\u043d",
      "\u0414\u043e\u0441\u0442\u0430\u0432\u043a\u0430 \u0438 \u0441\u0430\u043c\u043e\u0432\u044b\u0432\u043e\u0437",
      "\u0427\u0430\u0441\u0442\u043e \u0437\u0430\u0434\u0430\u0432\u0430\u0435\u043c\u044b\u0435 \u0432\u043e\u043f\u0440\u043e\u0441\u044b",
      "\u0421\u043f\u0440\u0430\u0432\u043e\u0447\u043d\u0438\u043a \u043f\u0435\u043a\u0430\u0440\u043d\u0438", "\u0421\u043f\u0440\u0430\u0432\u043a\u0430 \u043f\u0435\u043a\u0430\u0440\u043d\u0438"]),
    ("ar", "cp1256", "windows-1256", 178, "0x401 Arabic (Saudi Arabia)",
     ["\u0627\u0644\u0628\u062f\u0621", "\u0637\u0644\u0628 \u0627\u0644\u062e\u0628\u0632 \u0639\u0628\u0631 \u0627\u0644\u0625\u0646\u062a\u0631\u0646\u062a",
      "\u0627\u0644\u062a\u0648\u0635\u064a\u0644 \u0648\u0627\u0644\u0627\u0633\u062a\u0644\u0627\u0645", "\u0627\u0644\u0623\u0633\u0626\u0644\u0629 \u0627\u0644\u0634\u0627\u0626\u0639\u0629",
      "\u062f\u0644\u064a\u0644 \u0627\u0644\u0645\u062e\u0628\u0632", "\u0645\u0633\u0627\u0639\u062f\u0629 \u0627\u0644\u0645\u062e\u0628\u0632"]),
    ("ja", "cp932", "shift_jis", 128, "0x411 Japanese (Japan)",
     ["\u306f\u3058\u3081\u306b", "\u30d1\u30f3\u3092\u30aa\u30f3\u30e9\u30a4\u30f3\u3067\u6ce8\u6587", "\u914d\u9054\u3068\u53d7\u3051\u53d6\u308a",
      "\u3088\u304f\u3042\u308b\u8cea\u554f", "\u30d9\u30fc\u30ab\u30ea\u30fc\u30cf\u30f3\u30c9\u30d6\u30c3\u30af", "\u30d9\u30fc\u30ab\u30ea\u30fc\u30d8\u30eb\u30d7"]),
    ("zh", "gbk", "gb2312", 134, "0x804 Chinese (Simplified, China)",
     ["\u5165\u95e8", "\u5728\u7ebf\u8ba2\u8d2d\u9762\u5305", "\u914d\u9001\u4e0e\u81ea\u53d6", "\u5e38\u89c1\u95ee\u9898", "\u9762\u5305\u5e97\u624b\u518c", "\u9762\u5305\u5e97\u5e2e\u52a9"]),
    ("zh-tw", "cp950", "big5", 136, "0x404 Chinese (Traditional, Taiwan)",
     ["\u5165\u9580", "\u7dda\u4e0a\u8a02\u8cfc\u9eb5\u5305", "\u914d\u9001\u8207\u81ea\u53d6", "\u5e38\u898b\u554f\u984c", "\u9eb5\u5305\u5e97\u624b\u518a", "\u9eb5\u5305\u5e97\u8aaa\u660e"]),
    ("ko", "cp949", "ks_c_5601-1987", 129, "0x412 Korean (Korea)",
     ["\uc2dc\uc791\ud558\uae30", "\uc628\ub77c\uc778\uc73c\ub85c \ube75 \uc8fc\ubb38", "\ubc30\uc1a1 \ubc0f \ud53d\uc5c5", "\uc790\uc8fc \ubb3b\ub294 \uc9c8\ubb38",
      "\ubca0\uc774\ucee4\ub9ac \uc548\ub0b4\uc11c", "\ubca0\uc774\ucee4\ub9ac \ub3c4\uc6c0\ub9d0"]),
    ("el", "cp1253", "windows-1253", 161, "0x408 Greek (Greece)",
     ["\u039e\u03b5\u03ba\u03b9\u03bd\u03ce\u03bd\u03c4\u03b1\u03c2", "\u03a0\u03b1\u03c1\u03b1\u03b3\u03b3\u03b5\u03bb\u03af\u03b1 \u03c8\u03c9\u03bc\u03b9\u03bf\u03cd online",
      "\u03a0\u03b1\u03c1\u03ac\u03b4\u03bf\u03c3\u03b7 \u03ba\u03b1\u03b9 \u03c0\u03b1\u03c1\u03b1\u03bb\u03b1\u03b2\u03ae", "\u03a3\u03c5\u03c7\u03bd\u03ad\u03c2 \u03b5\u03c1\u03c9\u03c4\u03ae\u03c3\u03b5\u03b9\u03c2",
      "\u0395\u03b3\u03c7\u03b5\u03b9\u03c1\u03af\u03b4\u03b9\u03bf \u03b1\u03c1\u03c4\u03bf\u03c0\u03bf\u03b9\u03b5\u03af\u03bf\u03c5", "\u0392\u03bf\u03ae\u03b8\u03b5\u03b9\u03b1 \u03b1\u03c1\u03c4\u03bf\u03c0\u03bf\u03b9\u03b5\u03af\u03bf\u03c5"]),
    ("vi", "cp1258", "windows-1258", 163, "0x42a Vietnamese (Vietnam)",
     ["B\u1eaft \u0111\u1ea7u", "\u0110\u1eb7t b\u00e1nh m\u00ec tr\u1ef1c tuy\u1ebfn", "Giao h\u00e0ng v\u00e0 nh\u1eadn t\u1ea1i ti\u1ec7m", "C\u00e2u h\u1ecfi th\u01b0\u1eddng g\u1eb7p",
      "S\u1ed5 tay ti\u1ec7m b\u00e1nh", "Tr\u1ee3 gi\u00fap ti\u1ec7m b\u00e1nh"]),
    ("pl", "cp1250", "windows-1250", 238, "0x415 Polish (Poland)",
     ["Pierwsze kroki", "Zam\u00f3w chleb online", "Dostawa i odbi\u00f3r", "Najcz\u0119\u015bciej zadawane pytania", "Podr\u0119cznik piekarni", "Pomoc piekarni"]),
]


TAGS = {"en": "en-US", "de": "de-DE", "fr": "fr-FR", "ru": "ru-RU", "ar": "ar-SA", "ja": "ja-JP", "zh": "zh-CN", "zh-tw": "zh-Hant",
        "ko": "ko-KR", "el": "el-GR", "vi": "vi-VN", "pl": "pl-PL"}
# Language= the way HTML Help Workshop (and the LocOS merge) writes it for the language
LANGS = [(c, k, ch, g, "0x%x %s" % target_lcid(TAGS[c]), p) for c, k, ch, g, _l, p in LANGS]


def enc(text, codec):
    if codec == "cp1258":
        text = _cp1258_text(text)
    return text.encode(codec)


def esc(s):
    return s.replace("&", "&amp;").replace('"', "&quot;")


def toc(code, charset, gdi, ph):
    started, order, delivery, faq, title, window = ph
    return (
        '<!DOCTYPE HTML PUBLIC "-//IETF//DTD HTML//EN">\r\n<HTML>\r\n<HEAD>\r\n'
        f'<meta http-equiv="Content-Type" content="text/html; charset={charset}">\r\n'
        '<meta name="GENERATOR" content="Microsoft&reg; HTML Help Workshop 4.1">\r\n<!-- Sitemap 1.0 -->\r\n</HEAD><BODY>\r\n'
        '<OBJECT type="text/site properties">\r\n\t<param name="ImageType" value="Folder">\r\n'
        f'\t<param name="Font" value="Tahoma,8,{gdi}">\r\n</OBJECT>\r\n<UL>\r\n'
        f'\t<LI> <OBJECT type="text/sitemap">\r\n\t\t<param name="Name" value="{esc(started)}">\r\n'
        f'\t\t<param name="Local" value="html/{code}/start.htm">\r\n\t\t<param name="ImageNumber" value="1">\r\n\t\t</OBJECT>\r\n'
        f'\t<LI> <OBJECT type="text/sitemap">\r\n\t\t<param name="Name" value="{esc(order)}">\r\n'
        f'\t\t<param name="Local" value="html/{code}/order.htm">\r\n\t\t</OBJECT>\r\n'
        "\t<UL>\r\n"
        f'\t\t<LI> <OBJECT type="text/sitemap">\r\n\t\t\t<param name="Name" value="{esc(delivery)}">\r\n'
        f'\t\t\t<param name="Local" value="html/{code}/delivery.htm#top">\r\n\t\t\t</OBJECT>\r\n'
        "\t</UL>\r\n"
        f"\t<LI> <OBJECT type=\"text/sitemap\">\r\n\t\t<PARAM NAME=Name VALUE='{faq}'>\r\n"
        f'\t\t<param name="Local" value="html/{code}/faq.htm">\r\n\t\t<param name="Comment" value="not translated">\r\n\t\t</OBJECT>\r\n'
        "</UL>\r\n</BODY></HTML>\r\n"
    )


def index(code, charset, ph):
    started, order, delivery, faq, title, window = ph
    return (
        '<!DOCTYPE HTML PUBLIC "-//IETF//DTD HTML//EN">\n<HTML><HEAD>'
        f'<meta http-equiv="Content-Type" content="text/html; charset={charset}">\n<!-- Sitemap 1.0 -->\n</HEAD><BODY>\n<UL>\n'
        f'\t<LI> <OBJECT type="text/sitemap">\n\t\t<param name="Name" value="{esc(order)}">\n'
        f'\t\t<param name="Name" value="{esc(started)}">\n\t\t<param name="Local" value="html/{code}/start.htm">\n'
        f'\t\t<param name="Name" value="{esc(delivery)}">\n\t\t<param name="Local" value="html/{code}/delivery.htm">\n\t\t</OBJECT>\n'
        f'\t<LI> <OBJECT type="text/sitemap">\n\t\t<param name="Keyword" value="{esc(faq)}">\n\t\t<param name="Local" value="html/{code}/faq.htm">\n\t\t</OBJECT>\n'
        f'\t<LI> <OBJECT type="text/sitemap">\n\t\t<param name="Name" value="{esc(title)}">\n\t\t<param name="See Also" value="{esc(order)}">\n\t\t</OBJECT>\n'
        "</UL>\n</BODY></HTML>\n"
    )


def project(code, lang, ph):
    started, order, delivery, faq, title, window = ph
    return (
        "[OPTIONS]\r\nCompatibility=1.1 or later\r\nCompiled file=manual.chm\r\nContents file=toc.hhc\r\n"
        f"Default topic=html/{code}/start.htm\r\nDisplay compile progress=No\r\nIndex file=index.hhk\r\n"
        f"Language={lang}\r\nTitle={title}\r\n\r\n[WINDOWS]\r\n"
        f'main="{window}","toc.hhc","index.hhk","html/{code}/start.htm","html/{code}/start.htm",,,,,0x23520,,0x387e,,,,,,,,0\r\n\r\n'
        f"[FILES]\r\nhtml/{code}/start.htm\r\n; Title=not an option here\r\n\r\n[INFOTYPES]\r\n"
    )


for code, codec, charset, gdi, lang, ph in LANGS:
    with open(os.path.join(OUT, f"toc_{code}.hhc"), "wb") as fh:
        fh.write(enc(toc(code, charset, gdi, ph), codec))
    with open(os.path.join(OUT, f"index_{code}.hhk"), "wb") as fh:
        fh.write(enc(index(code, charset, ph), codec))
    # HTML Help Workshop writes the project in the ANSI code page too, but a project file declares no charset, so only
    # the Western ones are written that way; the others are UTF-8 (readable by every filter without a hint).
    with open(os.path.join(OUT, f"project_{code}.hhp"), "wb") as fh:
        fh.write(enc(project(code, lang, ph), codec) if codec == "cp1252" else project(code, lang, ph).encode("utf-8"))

# Unicode files: UTF-8 with a BOM (German) and UTF-16 little endian (Japanese)
de = next(x for x in LANGS if x[0] == "de")
ja = next(x for x in LANGS if x[0] == "ja")
with open(os.path.join(OUT, "toc_de_utf8bom.hhc"), "wb") as fh:
    fh.write(b"\xef\xbb\xbf" + toc("de", "utf-8", 0, de[5]).encode("utf-8"))
with open(os.path.join(OUT, "toc_ja_utf16le.hhc"), "wb") as fh:
    fh.write(b"\xff\xfe" + toc("ja", "utf-16", 128, ja[5]).encode("utf-16-le"))
print(len(os.listdir(OUT)), "files")
