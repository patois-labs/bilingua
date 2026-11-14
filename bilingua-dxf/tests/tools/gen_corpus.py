"""Writes the DXF corpus: tests/fixtures/corpus (all files) from invented Lumen Bakery drawings.

    python gen_corpus.py ../fixtures/corpus

Hand-built text, no third-party content. Languages: English, German, French, Spanish, Arabic, Japanese, Chinese
(simplified and traditional), Korean, Russian, Greek, Hebrew, Thai; AutoCAD 2018 (UTF-8) and older releases in their
code pages, CRLF, LF and CR line breaks, a BOM, long MTEXT, MTEXT format codes.
"""

import os
import sys

LANGS = {
    "en": ("Ground floor plan", "Bakery counter and display cases", "Fire exit"),
    "de": ("Grundriss Erdgeschoss", "Backtheke und Vitrinen", "Notausgang"),
    "fr": ("Plan du rez-de-chaussée", "Comptoir de boulangerie et vitrines", "Sortie de secours"),
    "es": ("Plano de la planta baja", "Mostrador de panadería y vitrinas", "Salida de emergencia"),
    "ar": ("مخطط الطابق الأرضي", "منضدة المخبز وخزائن العرض", "مخرج الطوارئ"),
    "ja": ("1階平面図", "ベーカリーカウンターとショーケース", "非常口"),
    "zh": ("一层平面图", "面包柜台和展示柜", "紧急出口"),
    "zh-tw": ("一樓平面圖", "麵包櫃檯和展示櫃", "緊急出口"),
    "ko": ("1층 평면도", "베이커리 카운터와 진열장", "비상구"),
    "ru": ("План первого этажа", "Прилавок пекарни и витрины", "Запасной выход"),
    "el": ("Κάτοψη ισογείου", "Πάγκος αρτοποιείου και βιτρίνες", "Έξοδος κινδύνου"),
    "he": ("תוכנית קומת קרקע", "דלפק מאפייה וויטרינות", "יציאת חירום"),
    "th": ("แปลนชั้นล่าง", "เคาน์เตอร์เบเกอรี่และตู้โชว์", "ทางออกฉุกเฉิน"),
}

BS = "\\"


def dxf(pairs, nl="\r\n", enc="utf-8", bom=False):
    out = "".join(f"{code:>3}{nl}{val}{nl}" for code, val in pairs).encode(enc)
    return (b"\xef\xbb\xbf" + out) if bom else out


def header(ver, cp):
    h = [(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, ver)]
    if cp:
        h += [(9, "$DWGCODEPAGE"), (3, cp)]
    return [*h, (9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]


def drawing(lang, ver="AC1032", cp="ANSI_1252"):
    title, note, exit_ = LANGS[lang]
    long_note = (note + ". ") * 12
    mt = "{" + BS + "fArial|b1|i0;" + note + "}" + BS + "P" + BS + "H2.5x;" + exit_ + BS + "~ " + BS + "S1/2;"
    return [
        *header(ver, cp),
        (0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "LAYER"), (0, "LAYER"), (5, "10"), (2, "Walls Text"), (70, "0"),
        (0, "ENDTAB"), (0, "ENDSEC"),
        (0, "SECTION"), (2, "BLOCKS"),
        (0, "BLOCK"), (5, "20"), (8, "0"), (2, "TITLEBLOCK"), (70, "2"),
        (0, "ATTDEF"), (5, "21"), (8, "0"), (1, title), (3, "Drawing title"), (2, "TITLE"),
        (0, "TEXT"), (5, "22"), (8, "0"), (1, "Lumen Bakery"),
        (0, "ENDBLK"), (5, "23"), (0, "ENDSEC"),
        (0, "SECTION"), (2, "ENTITIES"),
        (0, "TEXT"), (5, "30"), (8, "Walls Text"), (10, "1.0"), (1, title), (7, "Standard"),
        (0, "TEXT"), (5, "31"), (8, "Walls Text"), (1, "1200"),
        (0, "TEXT"), (5, "32"), (8, "Walls Text"), (1, "%%c50 " + exit_ + ", 45%%d"),
        (0, "MTEXT"), (5, "33"), (8, "Notes"), (3, long_note[:250]), (1, long_note[250:]), (7, "Standard"),
        (1001, "ACAD"), (1000, "do not translate me"),
        (0, "MTEXT"), (5, "34"), (8, "Notes"), (1, mt),
        (0, "INSERT"), (5, "35"), (2, "TITLEBLOCK"), (66, "1"),
        (0, "ATTRIB"), (5, "36"), (1, title), (2, "TITLE"),
        (0, "SEQEND"), (5, "37"),
        (0, "DIMENSION"), (5, "38"), (2, "*D1"), (1, "<> " + exit_), (3, "Standard"),
        (0, "MULTILEADER"), (5, "3A"), (300, "CONTEXT_DATA{"), (304, exit_ + BS + "P" + note), (301, "}"),
        (0, "ENDSEC"),
        (0, "SECTION"), (2, "OBJECTS"), (0, "DICTIONARY"), (5, "C"), (3, "ACAD_GROUP"), (350, "D"), (0, "ENDSEC"),
        (0, "EOF"),
    ]


LEGACY = [
    ("en", "ANSI_1252", "cp1252"), ("de", "ANSI_1252", "cp1252"), ("fr", "ANSI_1252", "cp1252"),
    ("es", "ANSI_1252", "cp1252"), ("ru", "ANSI_1251", "cp1251"), ("el", "ANSI_1253", "cp1253"),
    ("ar", "ANSI_1256", "cp1256"), ("he", "ANSI_1255", "cp1255"), ("th", "ANSI_874", "cp874"),
    ("ja", "ANSI_932", "cp932"), ("zh", "ANSI_936", "gbk"), ("ko", "ANSI_949", "cp949"), ("zh-tw", "ANSI_950", "cp950"),
]


def main(out):
    os.makedirs(out, exist_ok=True)

    def w(name, data):
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data)

    for lang in LANGS:
        w(f"utf8_{lang}.dxf", dxf(drawing(lang)))
    for lang, cp, codec in LEGACY:
        w(f"ac1015_{lang}_{cp.lower()}.dxf", dxf(drawing(lang, "AC1015", cp), enc=codec))
    w("ac1021_bom_crlf.dxf", dxf(drawing("de", "AC1021", "ANSI_1252"), bom=True))
    w("ac1027_lf.dxf", dxf(drawing("fr", "AC1027"), nl="\n"))
    w("ac1015_cr.dxf", dxf(drawing("es", "AC1015", "ANSI_1252"), nl="\r", enc="cp1252"))
    body = [(0, "SECTION"), (2, "ENTITIES"), (0, "TEXT"), (8, "0"), (1, "Größe der Küche"), (0, "ENDSEC"), (0, "EOF")]
    w("r12_no_header_cp1252.dxf", dxf(body, enc="cp1252"))
    w("r12_no_header_utf8.dxf", dxf(body))
    w("ac1032_no_final_newline.dxf", dxf(drawing("en", "AC1032"), nl="\n").rstrip(b"\n"))
    w("escapes.dxf", dxf([
        *header("AC1032", "ANSI_1252"), (0, "SECTION"), (2, "ENTITIES"),
        (0, "TEXT"), (1, "Caf" + BS + "U+00E9 " + BS + "U+4E2D " + BS + "U+D83D" + BS + "U+DE00 done"),
        (0, "MTEXT"), (1, "Plan" + BS + "M+18A8Aend {unbalanced " + BS + "Pline2 " + BS),
        (0, "ENDSEC"), (0, "EOF")]))
    w("long_mtext.dxf", dxf([
        *header("AC1032", "ANSI_1252"), (0, "SECTION"), (2, "ENTITIES"), (0, "MTEXT"),
        *([(3, "The contractor shall verify all dimensions on site before fabrication. " * 3 + "x" * 31)] * 4),
        (1, "End of the long note."), (0, "ENDSEC"), (0, "EOF")]))


if __name__ == "__main__":
    main(sys.argv[1])
