"""Builds the bilingua-pdf fixtures. Run with the LocOS venv Python (it needs typst and cryptography):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe -I gen_fixtures.py OUT_DATA_DIR OUT_CORPUS_DIR

Two families:
- `typst_*`: made by Localization OS's own PDF writer (`PdfFilter.build`, typst), the files the LocOS suite builds
  (`text_pdf`, the scanned page, pagination, the escape cases, the ten scripts) plus a few layouts typst makes easily.
- `raw_*`: hand-written PDFs (pdfgen.py) for the parser and the text-layer rules: kerning spaces, encodings,
  ToUnicode maps, object streams, incremental updates, broken and looping xrefs, forms, rotation, encryption.
Every file is deterministic except the encrypted ones (random IVs and salts).
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ.setdefault("LOC_TMS_DATA_DIR", os.path.join(os.environ.get("TEMP", "/tmp"), "bilingua-pdf-gen-data"))
sys.dont_write_bytecode = True
sys.path.insert(1, LOCOS)

from pdfgen import (Crypt, Name, Raw, Ref, Str, Stream, append_update, simple_doc, std_font,  # noqa: E402
                    tounicode_cmap, write)

DATA, CORPUS = sys.argv[1], sys.argv[2]
os.makedirs(DATA, exist_ok=True)
os.makedirs(CORPUS, exist_ok=True)

# The ten scripts of verify_pdf.py plus Latin languages: 11+ languages for the bar.
SCRIPTS = {
    "ru": "Быстрая коричневая лиса перепрыгивает через ленивую собаку.",
    "el": "Η γρήγορη καφέ αλεπού πηδάει πάνω από το τεμπέλικο σκυλί.",
    "ja": "素早い茶色の狐が怠け者の犬を飛び越えます。",
    "zh-Hans": "敏捷的棕色狐狸跳过了懒狗。",
    "ko": "빠른 갈색 여우가 게으른 개를 뛰어넘습니다.",
    "ar": "الثعلب البني السريع يقفز فوق الكلب الكسول.",
    "he": "השועל החום המהיר קופץ מעל הכלב העצלן.",
    "hi": "तेज़ भूरी लोमड़ी आलसी कुत्ते के ऊपर कूद जाती है।",
    "bn": "দ্রুত বাদামী শিয়াল অলস কুকুরের উপর দিয়ে লাফ দেয়।",
    "th": "สุนัขจิ้งจอกสีน้ำตาลว่องไวกระโดดข้ามสุนัขขี้เกียจ",
    "fr": "Le vif renard brun saute par-dessus le chien paresseux, à l'été déjà.",
    "de": "Der schnelle braune Fuchs springt über den faulen Hund. Größe: ß.",
    "pl": "Szybki brązowy lis przeskakuje nad leniwym psem. Zażółć gęślą jaźń.",
    "es": "El veloz zorro marrón salta sobre el perro perezoso. ¿Señor? ¡Sí!",
}

ESCAPES = [
    "Run the tool with --verbose --help to see all flags.",
    "Values in the range 10--20 are valid; see the docs...",
    "well--known --- ... -? co-?operate --> <-- 1--2",
    "typst meta: #call *bold* _emph_ `code` [group] $math$ @ref ~tilde // comment",
    "path/with/slashes and quotes 'x' \"y\" and a lone backslash \\ end",
    '<b>&</b> and #read("/x") #include "y" #import "z" stay literal',
    "- leading dash not a bullet",
    "+ leading plus not an enum",
    "= leading equals not a heading",
    "  - spaces then dash",
    "\t+ tab then plus",
    "  = spaced heading",
    "1. leading enum stays",
    "1.5 million (decimal)",
    "WORD1\tWORD2\tWORD3 must not fuse (tab -> space)",
    "col1\tcol2\tcol3 tabbed values stay separated",
]


def save(name, data, corpus=True):
    with open(os.path.join(DATA, name), "wb") as f:
        f.write(data)
    if corpus:
        with open(os.path.join(CORPUS, name), "wb") as f:
            f.write(data)


# ---------------------------------------------------------------- typst (LocOS's writer)

def typst_family():
    import typst
    from app.filters import pdf_filter
    from app.filters.pdf_filter import PdfFilter

    f = PdfFilter()
    save("typst_digital.pdf", f.build(b"", {"0": "Hello world. This is a digital PDF.", "1": "Second paragraph here."}))
    save("typst_pagination.pdf", f.build(b"", {str(i): f"Paragraph {i}: enough words on this line to consume vertical space." for i in range(120)}))
    save("typst_escape.pdf", f.build(b"", {str(i): s for i, s in enumerate(ESCAPES)}))
    for i, s in enumerate(ESCAPES):
        save(f"typst_escape_{i:02d}.pdf", f.build(b"", {"0": s}), corpus=False)
    save("typst_reparse.pdf", f.build(b"", {
        "0": "The quick brown fox jumps over the lazy dog.",
        "1": "A second paragraph with distinct wording for the check.",
        "2": "Troisieme paragraphe avec des accents: eleve, francais.",
    }))
    for code, text in SCRIPTS.items():
        save(f"typst_lang_{code}.pdf", f.build_localized(b"", {"0": text, "1": text + " " + text}, code))
    save("typst_multiscript.pdf", f.build(b"", {str(i): t for i, t in enumerate(SCRIPTS.values())}))
    save("typst_translated.pdf", f.build(b"", {"0": "Texte traduit.", "1": "Texte traduit."}))
    save("typst_long_token.pdf", f.build(b"", {"0": "hash " + "a1b2c3d4e5f6" * 40 + " end"}))

    def raw(src, name, corpus=True):
        save(name, typst.compile(src.encode("utf-8")), corpus)

    raw('#set page(paper: "a4", margin: 40pt)\n#set text(size: 11pt, lang: "en", hyphenate: true)\n#set par(justify: true)\n'
        + "#columns(2)[" + " ".join(["Internationalization and localization require extraordinarily comprehensive "
                                     "documentation, notwithstanding unquestionably straightforward implementations."] * 12) + "]\n",
        "typst_hyphen_columns.pdf")
    raw('#set page(paper: "a5", margin: 30pt)\n= A Heading\nSome *bold* and _italic_ words, `code`, and a #link("https://example.com")[link].\n\n'
        "- first item\n- second item with more words\n\n+ one\n+ two\n\n"
        "#table(columns: 3, [Name], [Qty], [Price], [Apple], [3], [1.20], [Pear], [10], [0.50])\n\n"
        "Footnote here#footnote[The note text.] and H#sub[2]O and x#super[2].\n",
        "typst_styles.pdf")
    raw('#set page(paper: "a4", margin: 40pt)\n#set text(size: 11pt)\n'
        + "\n\n".join(f"Paragraph {i} of a document whose pages each carry a few lines of plain prose." for i in range(6))
        + "\n#pagebreak()\nShort.\n#pagebreak()\n" + "Third page text that is long enough to count as a text layer.\n",
        "typst_short_page.pdf")
    raw('#set page(width: 300pt, height: 200pt, margin: 20pt, flipped: false)\n#rotate(90deg)[Rotated words here]\n'
        "#place(bottom + right)[Corner text]\nPlain line of text at the top.\n", "typst_rotate.pdf")
    raw('#set page(paper: "a6", margin: 20pt)\n#set text(dir: rtl, lang: "ar")\n' + SCRIPTS["ar"] + " 2024 (" + "نص" + ")\n\n"
        + "English inside عربي text.\n", "typst_rtl_mixed.pdf")

    # The scanned and mixed pages of verify_pdf.py (images only, so LocOS sends them to OCR).
    import tempfile

    import pypdfium2 as pdfium

    src = pdfium.PdfDocument(f.build(b"", {"0": "Hello world. This is a digital PDF.", "1": "Second paragraph here."}))
    page = src[0]
    pil = page.render(scale=150 / 72).to_pil().convert("RGB")
    page.close()
    src.close()
    with tempfile.TemporaryDirectory() as d:
        pil.save(os.path.join(d, "scan.png"))
        save("typst_scanned.pdf", typst.compile(b'#set page(paper: "a4", margin: 0pt)\n#image("scan.png", width: 100%, height: 100%)\n', root=d))
        save("typst_mixed_scan.pdf", typst.compile(
            ('#set page(paper: "a4", margin: 40pt)\nThis is page one with a real text layer and plenty of characters.\n'
             '#pagebreak()\n#set page(margin: 0pt)\n#image("scan.png", width: 100%, height: 100%)\n').encode(), root=d))
    _ = pdf_filter


# ---------------------------------------------------------------- raw

HELV_W = [278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278] + [556] * 10 + \
    [278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
     667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556,
     278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334,
     260, 334, 584]


def bt(*ops):
    return Raw(("\n".join(ops)).encode("latin-1"))


def raw_family():
    helv = std_font("Helvetica")
    helv_w = std_font("Helvetica", widths=HELV_W)
    times = std_font("Times-Roman")
    cour = std_font("Courier", encoding=None)

    # 1. The text operators on the standard 14 fonts (no Widths: the AFM metrics apply).
    c = b"""BT /F1 12 Tf 72 720 Td (Standard fonts: Tj, TJ, quote and double quote.) Tj
0 -16 Td [(Kerned) -250 (TJ) 120 (array)] TJ
14 TL T* (After T-star on the next line.) Tj
(Quote operator line.) '
2 1 (Double quote with spacing.) "
/F2 10 Tf 0 -30 Td (Times Roman at ten points, caf\\351 cr\\350me br\\373l\\351e.) Tj
/F3 9 Tf 0 -14 Td (Courier fixed pitch 1234567890) Tj
ET"""
    o, r = simple_doc(c, {"F1": helv, "F2": times, "F3": cour})
    save("raw_std14.pdf", write(o, r))

    # 2. LaTeX-style words: no space glyphs, the gaps are TJ kerning (PDFium makes the spaces).
    c = b"""BT /F1 11 Tf 72 700 Td
[(The)-333(quic)27(k)-333(bro)27(wn)-333(fo)-27(x)-333(jumps)-333(o)27(v)27(er)-333(the)-333(lazy)-333(dog.)]TJ
0 -14 Td [(Tight)-80(kerning)-120(stays)-160(joined)-200(or)-260(splits.)]TJ
0 -14 Td [(Line)-333(ending)-333(with)-333(a)-333(hyphen-)]TJ
0 -14 Td [(ated)-333(word)-333(continues)-333(here.)]TJ
ET"""
    o, r = simple_doc(c, {"F1": helv_w})
    save("raw_kerning.pdf", write(o, r))

    # 3. Gaps between separate text objects on one line, and objects drawn right to left on a line.
    c = b"""BT /F1 12 Tf 72 700 Td (Left) Tj ET
BT /F1 12 Tf 110 700 Td (gap) Tj ET
BT /F1 12 Tf 131 700 Td (touching) Tj ET
BT /F1 12 Tf 400 650 Td (right part) Tj ET
BT /F1 12 Tf 72 650 Td (left part drawn second) Tj ET
BT /F1 12 Tf 72 600 Td (sup) Tj 0 4 Td /F1 7 Tf (2) Tj 0 -4 Td /F1 12 Tf ( after superscript) Tj ET
BT /F1 12 Tf 72 560 Td (fake bold) Tj ET
BT /F1 12 Tf 72.3 560 Td (fake bold) Tj ET
BT /F1 12 Tf 72 520 Td 3 Tr (invisible OCR layer text) Tj ET
BT /F1 12 Tf 72 480 Td 5 Ts (risen) Tj 0 Ts 150 Tz ( wide) Tj 100 Tz 2 Tc ( spaced) Tj 0 Tc 6 Tw ( word spaced text) Tj ET"""
    o, r = simple_doc(c, {"F1": helv_w})
    save("raw_layout.pdf", write(o, r))

    # 4. Differences, ligatures, uniXXXX names, unknown names; MacRoman and Standard encodings.
    diff = {"Type": Name("Encoding"), "BaseEncoding": Name("WinAnsiEncoding"),
            "Differences": [1, Name("fi"), Name("fl"), Name("uni0141"), Name("u1F600"), Name("g123"), Name("a.sc"),
                            Name("ff"), Name("emdash"), Name("quotedblleft"), Name("quotedblright")]}
    c = b"""BT /F1 12 Tf 72 700 Td (\\001nd \\002ow \\003\\363d\\272 \\006sc \\007ect \\010 \\011quoted\\012 glyph\\005) Tj
/F2 12 Tf 0 -20 Td (MacRoman: \\216t\\216 \\212 na\\226ve \\247) Tj
/F3 12 Tf 0 -20 Td (Standard: \\252quoted\\272 \\341 \\361 fi\\256) Tj
/F4 12 Tf 0 -20 Td (abc) Tj
ET"""
    o, r = simple_doc(c, {
        "F1": std_font("Helvetica", encoding=diff, widths=[500] * 224, first=1),
        "F2": std_font("Times-Roman", encoding="MacRomanEncoding"),
        "F3": std_font("Helvetica", encoding={"Type": Name("Encoding")}, extra={}),
        "F4": std_font("Symbol", encoding=None),
    })
    save("raw_encodings.pdf", write(o, r))

    # 5. A Type0 font with Identity-H and a ToUnicode map (bfchar and bfrange, a ligature, a surrogate pair).
    tu = tounicode_cmap(
        [(b"\x00\x01", "H"), (b"\x00\x02", "e"), (b"\x00\x03", "l"), (b"\x00\x04", "o"), (b"\x00\x05", " "),
         (b"\x00\x06", "ffi"), (b"\x00\x07", "\U0001F600"), (b"\x00\x08", "\u65e5"), (b"\x00\x09", "\u672c")],
        ranges=[(b"\x00\x10", b"\x00\x29", "a")])
    desc = {"Type": Name("FontDescriptor"), "FontName": Name("NotoSans"), "Flags": 32, "FontBBox": [-500, -300, 1200, 1000],
            "ItalicAngle": 0, "Ascent": 900, "Descent": -250, "CapHeight": 700, "StemV": 80}
    cid = {"Type": Name("Font"), "Subtype": Name("CIDFontType2"), "BaseFont": Name("NotoSans"),
           "CIDSystemInfo": {"Registry": "Adobe", "Ordering": "Identity", "Supplement": 0},
           "FontDescriptor": Ref(30), "DW": 600, "W": [1, [700, 550, 250, 560, 280, 900, 1000, 1000, 1000], 16, 41, 520]}
    t0 = {"Type": Name("Font"), "Subtype": Name("Type0"), "BaseFont": Name("NotoSans"), "Encoding": Name("Identity-H"),
          "DescendantFonts": [Ref(31)], "ToUnicode": Ref(32)}
    c = b"""BT /F1 14 Tf 72 700 Td <0001000200030003000400050006000500070005000800090005> Tj
0 -20 Td <0010001100120005001300140015> Tj 0 -20 Td [<0001> -500 <0002000300030004>] TJ ET"""
    o, r = simple_doc(c, {"F1": Ref(33)}, extra_objs={30: desc, 31: cid, 32: tu, 33: t0})
    save("raw_type0_tounicode.pdf", write(o, r))
    save("raw_objstm.pdf", write(dict(o), r, mode="objstm"))

    # 6. Legacy CJK CMaps with no ToUnicode: Shift-JIS, GBK, Big5, EUC-KR, and UniJIS UCS-2.
    def cjk(cmap, ordering, base):
        d = {"Type": Name("FontDescriptor"), "FontName": Name(base), "Flags": 4, "FontBBox": [0, -120, 1000, 880],
             "ItalicAngle": 0, "Ascent": 880, "Descent": -120, "CapHeight": 700, "StemV": 80}
        return d, {"Type": Name("Font"), "Subtype": Name("CIDFontType0"), "BaseFont": Name(base),
                   "CIDSystemInfo": {"Registry": "Adobe", "Ordering": ordering, "Supplement": 2}, "DW": 1000}, \
            {"Type": Name("Font"), "Subtype": Name("Type0"), "BaseFont": Name(base), "Encoding": Name(cmap)}
    fonts = {}
    extra = {}
    n = 40
    for name, (cmap, ordering, base) in {"J": ("90ms-RKSJ-H", "Japan1", "MS-Mincho"), "G": ("GBK-EUC-H", "GB1", "SimSun"),
                                         "B": ("ETen-B5-H", "CNS1", "MingLiU"), "K": ("KSCms-UHC-H", "Korea1", "Batang"),
                                         "U": ("UniJIS-UCS2-H", "Japan1", "MS-Gothic")}.items():
        d, cf, t0 = cjk(cmap, ordering, base)
        extra[n], extra[n + 1], extra[n + 2] = d, cf, t0
        cf["FontDescriptor"] = Ref(n)
        t0["DescendantFonts"] = [Ref(n + 1)]
        fonts[name] = Ref(n + 2)
        n += 3

    def hx(s, enc):
        return ("<" + s.encode(enc).hex() + ">").encode()
    c = b"BT /J 14 Tf 72 700 Td " + hx("日本語のテキスト ABC", "shift_jis") + b" Tj\n/G 14 Tf 0 -24 Td " + hx("简体中文文本", "gbk") + \
        b" Tj\n/B 14 Tf 0 -24 Td " + hx("繁體中文文字", "big5") + b" Tj\n/K 14 Tf 0 -24 Td " + hx("한국어 텍스트", "cp949") + \
        b" Tj\n/U 14 Tf 0 -24 Td " + hx("ユニコード", "utf-16-be") + b" Tj ET"
    o, r = simple_doc(c, fonts, extra_objs=extra)
    save("raw_cjk_legacy.pdf", write(o, r))

    # 7. Incremental update (the second revision replaces the page content), and a /Prev loop.
    o, r = simple_doc(b"BT /F1 12 Tf 72 700 Td (First revision text.) Tj ET", {"F1": helv})
    base = write(o, r)
    content_num = [k for k, v in o.items() if isinstance(v, Stream)][0]
    upd = {content_num: Stream({}, b"BT /F1 12 Tf 72 700 Td (Second revision replaces the first.) Tj ET", compress=True)}
    save("raw_incremental.pdf", append_update(base, upd, r, size=max(o) + 1))
    save("raw_prev_loop.pdf", append_update(base, upd, r, prev_loop=True, size=max(o) + 1))

    # 8. Damaged cross references: wrong offsets, no xref at all, truncated, garbage before the header.
    o, r = simple_doc(b"BT /F1 12 Tf 72 700 Td (Repaired cross reference table text.) Tj ET", {"F1": helv})
    good = write(o, r)
    save("raw_bad_offsets.pdf", write(dict(o), r, mode="badoffsets"))
    save("raw_no_xref.pdf", write(dict(o), r, mode="noxref"))
    save("raw_junk_prefix.pdf", b"junk before the header\r\n" * 3 + good)
    save("raw_truncated_half.pdf", good[: len(good) // 2], corpus=False)
    save("raw_truncated_tail.pdf", good[: good.rfind(b"startxref")], corpus=False)
    save("raw_not_pdf.pdf", b"\x89PNG\r\n\x1a\n" + b"\x00" * 64, corpus=False)
    save("raw_empty.pdf", b"", corpus=False)

    # 9. Form XObjects (nested, with a Matrix), a rotated page, several content streams, inline image.
    form2 = Stream({"Type": Name("XObject"), "Subtype": Name("Form"), "BBox": [0, 0, 300, 50],
                    "Resources": {"Font": {"F1": Ref(61)}}},
                   b"BT /F1 10 Tf 0 10 Td (Inner nested form text.) Tj ET")
    form1 = Stream({"Type": Name("XObject"), "Subtype": Name("Form"), "BBox": [0, 0, 400, 100], "Matrix": [1, 0, 0, 1, 10, 0],
                    "Resources": {"Font": {"F1": Ref(61)}, "XObject": {"X2": Ref(63)}}},
                   b"BT /F1 12 Tf 0 40 Td (Outer form text.) Tj ET q 1 0 0 1 0 -20 cm /X2 Do Q")
    c1 = b"BT /F1 12 Tf 72 720 Td (Page text before the form.) Tj ET\nq 1 0 0 1 72 600 cm /X1 Do Q\n"
    c2 = b"BI /W 4 /H 2 /BPC 8 /CS /G ID \x00EI\xffEI\x00\x01 EI\nBT /F1 12 Tf 72 500 Td (Text after an inline image in a second stream.) Tj ET"
    o, r = simple_doc([c1, c2], {"F1": Ref(61)},
                      page_extra={"_res": {"XObject": {"X1": Ref(62)}}},
                      extra_objs={61: helv, 62: form1, 63: form2})
    save("raw_forms.pdf", write(o, r))
    o, r = simple_doc(b"BT /F1 12 Tf 72 720 Td (Text on a page rotated ninety degrees.) Tj ET\n"
                      b"BT /F1 12 Tf 0 1 -1 0 300 300 Tm (Text drawn vertically with a matrix.) Tj ET",
                      {"F1": helv}, page_extra={"Rotate": 90})
    save("raw_rotated.pdf", write(o, r))

    # 10. Marked content with ActualText, and a Type3 font.
    c = b"""BT /F1 12 Tf 72 700 Td /Span <</ActualText (replacement text)>> BDC (shown glyphs) Tj EMC ( and the rest.) Tj ET
BT /F2 1 Tf 72 660 Td (abc) Tj ET"""
    t3 = {"Type": Name("Font"), "Subtype": Name("Type3"), "FontBBox": [0, 0, 750, 750], "FontMatrix": [0.012, 0, 0, 0.012, 0, 0],
          "CharProcs": {"a": Ref(71), "b": Ref(71), "c": Ref(71)}, "Encoding": {"Type": Name("Encoding"), "Differences": [97, Name("a"), Name("b"), Name("c")]},
          "FirstChar": 97, "LastChar": 99, "Widths": [800, 800, 800], "Resources": {}}
    o, r = simple_doc(c, {"F1": helv, "F2": Ref(70)}, extra_objs={70: t3, 71: Stream({}, b"800 0 0 0 750 750 d1 0 0 750 750 re f")})
    save("raw_actualtext_type3.pdf", write(o, r))

    # 11. Filters: ASCII85, ASCIIHex, LZW, RunLength, and a chain.
    import base64
    text = b"BT /F1 12 Tf 72 700 Td (Text through the %s filter.) Tj ET"
    pages = []
    a85 = base64.a85encode(text % b"ASCII85") + b"~>"
    pages.append((None, Stream({}, a85, filters=Name("ASCII85Decode"))))
    pages.append((None, Stream({}, (text % b"ASCIIHex").hex().encode() + b">", filters=Name("ASCIIHexDecode"))))
    pages.append((None, Stream({}, lzw(text % b"LZW"), filters=Name("LZWDecode"))))
    pages.append((None, Stream({}, runlength(text % b"RunLength"), filters=Name("RunLengthDecode"))))
    import zlib
    pages.append((None, Stream({}, base64.a85encode(zlib.compress(text % b"chained")) + b"~>", filters=[Name("ASCII85Decode"), Name("FlateDecode")])))
    objs = {1: {"Type": Name("Catalog"), "Pages": Ref(2)}}
    kids = []
    for i, (_, stm) in enumerate(pages):
        objs[20 + i] = stm
        objs[30 + i] = {"Type": Name("Page"), "Parent": Ref(2), "MediaBox": [0, 0, 612, 792], "Resources": {"Font": {"F1": Ref(10)}}, "Contents": Ref(20 + i)}
        kids.append(Ref(30 + i))
    objs[2] = {"Type": Name("Pages"), "Kids": kids, "Count": len(kids)}
    objs[10] = helv
    save("raw_filters.pdf", write(objs, 1))

    # 12. Page tree inheritance (MediaBox and Resources on the parent), an empty page, a page with only a few chars.
    objs = {1: {"Type": Name("Catalog"), "Pages": Ref(2)},
            2: {"Type": Name("Pages"), "Kids": [Ref(3), Ref(4)], "Count": 3, "MediaBox": [0, 0, 400, 400], "Resources": {"Font": {"F1": Ref(10)}}},
            3: {"Type": Name("Pages"), "Parent": Ref(2), "Kids": [Ref(5), Ref(6)], "Count": 2},
            4: {"Type": Name("Page"), "Parent": Ref(2), "Contents": Ref(22)},
            5: {"Type": Name("Page"), "Parent": Ref(3), "Contents": Ref(20)},
            6: {"Type": Name("Page"), "Parent": Ref(3)},
            10: helv,
            20: Stream({}, b"BT /F1 12 Tf 20 360 Td (Inherited resources and media box on page one.) Tj ET", compress=True),
            22: Stream({}, b"BT /F1 12 Tf 20 360 Td (Page 3) Tj ET", compress=True)}
    save("raw_page_tree.pdf", write(objs, 1))

    # 13. Arabic in a simple TrueType font with a ToUnicode map, glyphs in visual order, as many producers write it.
    word = "مرحبا بالعالم"
    vis = list(reversed(word))
    codes = {}
    for ch in vis:
        codes.setdefault(ch, 33 + len(codes))
    tu = tounicode_cmap([(bytes([v]), k) for k, v in codes.items()], codespace=(b"\x00", b"\xff"))
    f = {"Type": Name("Font"), "Subtype": Name("TrueType"), "BaseFont": Name("ArialMT"), "FirstChar": 32, "LastChar": 60,
         "Widths": [278] + [500] * 28, "ToUnicode": Ref(81), "Encoding": Name("WinAnsiEncoding")}
    s = bytes(codes[ch] for ch in vis)
    c = b"BT /F1 16 Tf 200 700 Td " + b"<" + s.hex().encode() + b"> Tj ET\nBT /F1 16 Tf 72 650 Td (Hello) Tj ET"
    o, r = simple_doc(c, {"F1": Ref(80)}, extra_objs={80: f, 81: tu})
    save("raw_arabic_visual.pdf", write(o, r))

    # 14. Encryption: empty user password (opens), and a user password (refused).
    o, r = simple_doc(b"BT /F1 12 Tf 72 700 Td (Owner password only, the text still opens.) Tj ET",
                      {"F1": helv}, extra_objs={})
    o[3] = {"Title": Str(b"Encrypted title")}
    for kind in ("rc4-40", "rc4-128", "aes-128", "aes-256"):
        save(f"raw_encrypted_{kind.replace('-', '')}.pdf", write(dict(o), r, crypt=Crypt(kind), info=3, header=b"%PDF-1.7"))
    save("raw_encrypted_userpw.pdf", write(dict(o), r, crypt=Crypt("rc4-128", user=b"secret")), corpus=False)
    save("raw_encrypted_userpw_aes256.pdf", write(dict(o), r, crypt=Crypt("aes-256", user=b"secret")), corpus=False)

    # 15. Several pages with blank lines inside the text (PDFium joins lines; LocOS splits on blank lines).
    c = b"""BT /F1 12 Tf 72 700 Td (A first line) Tj 0 -14 Td (a second line of the same paragraph) Tj
0 -60 Td (After a large vertical gap) Tj ET"""
    o, r = simple_doc(c, {"F1": helv})
    save("raw_vertical_gap.pdf", write(o, r))


def lzw(data):
    """LZW encoder with EarlyChange 1 (the PDF default)."""
    table = {bytes([i]): i for i in range(256)}
    nxt, width, out, bits, nbits = 258, 9, bytearray(), 0, 0

    def emit(code):
        nonlocal bits, nbits
        bits = (bits << width) | code
        nbits += width
        while nbits >= 8:
            nbits -= 8
            out.append((bits >> nbits) & 255)

    emit(256)
    w = b""
    for c in data:
        wc = w + bytes([c])
        if wc in table:
            w = wc
            continue
        emit(table[w])
        table[wc] = nxt
        nxt += 1
        if nxt + 1 >= (1 << width) and width < 12:
            width += 1
        w = bytes([c])
    if w:
        emit(table[w])
    emit(257)
    if nbits:
        out.append((bits << (8 - nbits)) & 255)
    return bytes(out)


def runlength(data):
    out = bytearray()
    for i in range(0, len(data), 100):
        chunk = data[i : i + 100]
        out.append(len(chunk) - 1)
        out += chunk
    out.append(128)
    return bytes(out)


if __name__ == "__main__":
    typst_family()
    raw_family()
    print("fixtures written")
