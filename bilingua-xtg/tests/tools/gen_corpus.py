r"""Writes the made-up XPress Tags corpus (oracle/corpus/xtg): the same small document in eleven languages and
every encoding the format selects with <eN>. No Python codec is needed beyond the standard library.

    python gen_corpus.py ..\..\..\oracle\corpus\xtg
"""
import os
import sys

# (file stem, extension, <e> number, python codec, bom/extra, heading, bold word, rest)
DOC = (
    "{head}\r\n"
    '@Normal=[S"",""]<*L*h"Standard"f"Helvetica"z12>\r\n'
    '@Heading=[S"","Body"]<*C><B>\r\n'
    "@Heading:<*C>{h1}\r\n"
    "@Body:{p1} <B>{bold}<P> {p2}<\\n>{p3}\r\n"
    "{p4} <\\@> 3 <\\<> 4, <\\\\><\\i>.\r\n"
    '<f"Arial"><z10>{p5}<f$><z$>\r\n'
    "@$:<XI,\"{ix}\",\"\">{p6} <I>{ital}<I$> {p7}.\r\n"
    "\r\n"
    "<\\#13><*R>\r\n"
    '<*t(36,0,"1 ")>{p8}<\\t>{p9} <c"Red">{p10}<c$>\r\n'
)

LANGS = {
    "en": ("Getting started", "Hello", "bold", "world", "second line", "Mail us", "Leading text", "Entry", "italic", "again", "Name", "Value", "Red"),
    "de": ("Erste Schritte", "Hallo", "fett", "Welt", "zweite Zeile", "Schreiben Sie uns", "Vorspann", "Eintrag", "kursiv", "nochmals", "Name", "Wert", "Rot"),
    "fr": ("Pour commencer", "Bonjour", "gras", "monde", "deuxième ligne", "Écrivez-nous", "Texte initial", "Entrée", "italique", "encore", "Nom", "Valeur", "Rouge"),
    "es": ("Primeros pasos", "Hola", "negrita", "mundo", "segunda línea", "Escríbanos", "Texto inicial", "Entrada", "cursiva", "otra vez", "Nombre", "Valor", "Rojo"),
    "it": ("Per iniziare", "Ciao", "grassetto", "mondo", "seconda riga", "Scrivici", "Testo iniziale", "Voce", "corsivo", "ancora", "Nome", "Valore", "Rosso"),
    "ru": ("С чего начать", "Привет", "жирный", "мир", "вторая строка", "Напишите нам", "Вступление", "Запись", "курсив", "ещё", "Имя", "Значение", "Красный"),
    "ar": ("البدء", "مرحبا", "غامق", "بالعالم", "السطر الثاني", "راسلنا", "نص تمهيدي", "مدخل", "مائل", "مرة أخرى", "الاسم", "القيمة", "أحمر"),
    "ja": ("はじめに", "こんにちは", "太字", "世界", "2行目", "お問い合わせ", "先頭のテキスト", "項目", "斜体", "もう一度", "名前", "値", "赤"),
    "ko": ("시작하기", "안녕하세요", "굵게", "세계", "둘째 줄", "문의하세요", "앞부분 텍스트", "항목", "기울임", "다시", "이름", "값", "빨강"),
    "zh-Hans": ("入门", "你好", "粗体", "世界", "第二行", "联系我们", "开头文字", "条目", "斜体", "再次", "名称", "值", "红色"),
    "zh-Hant": ("入門", "你好", "粗體", "世界", "第二行", "聯絡我們", "開頭文字", "條目", "斜體", "再次", "名稱", "值", "紅色"),
}


def doc(lang, e, head=None):
    h1, p1, bold, p2, p3, p4, p5, ix, ital, p7, n8, n9, red = LANGS[lang]
    return DOC.format(head=head if head is not None else "<v19.00><e%d>" % e, h1=h1, p1=p1, bold=bold, p2=p2, p3=p3, p4=p4, p5=p5, ix=ix,
                      p6=p5, ital=ital, p7=p7, p8=n8, p9=n9, p10=red)


FILES = [
    ("en_utf8", "xtg", "en", 9, "utf-8", b""),
    ("de_utf8bom", "xtg", "de", 9, "utf-8", b"\xef\xbb\xbf"),
    ("fr_macroman", "xtg", "fr", 0, "mac_roman", b""),
    ("es_cp1252", "tag", "es", 1, "cp1252", b""),
    ("it_latin1", "tag", "it", 2, "latin-1", b""),
    ("ru_utf8", "xtg", "ru", 9, "utf-8", b""),
    ("ar_utf8", "xtg", "ar", 9, "utf-8", b""),
    ("ar_utf16le", "xtg", "ar", 8, "utf-16-le", b"\xff\xfe"),
    ("ja_cp932", "xtg", "ja", 3, "cp932", b""),
    ("ja_utf16be", "tag", "ja", 8, "utf-16-be", b"\xfe\xff"),
    ("ko_cp949", "xtg", "ko", 19, "cp949", b""),
    ("zh-Hans_gbk", "xtg", "zh-Hans", 7, "gbk", b""),
    ("zh-Hant_big5", "xtg", "zh-Hant", 6, "big5", b""),
]


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for stem, ext, lang, e, codec, bom in FILES:
        text = doc(lang, e)
        nl = "\r" if codec == "mac_roman" else ("\n" if stem.startswith("it_") else "\r\n")
        text = text.replace("\r\n", nl)
        with open(os.path.join(out, "%s.%s" % (stem, ext)), "wb") as fh:
            fh.write(bom + text.encode(codec))
    # no <e> tag: Windows-1252 and UTF-8 through the shared decode
    t = doc("de", 1, head="").lstrip("\r\n")
    with open(os.path.join(out, "de_noheader_cp1252.tag"), "wb") as fh:
        fh.write(t.encode("cp1252"))
    # UTF-16 LE without a BOM
    with open(os.path.join(out, "en_utf16le_nobom.xtg"), "wb") as fh:
        fh.write(doc("en", 8).encode("utf-16-le"))
    # attribute pairs
    pairs = (
        "<v13.00><e9>\r\n@Heading:Account <B>settings<B$>\r\n@B:<*C><B>Account<B$> settings\r\n"
        "@B:x <B>a <I>b<I$><B$> c\r\n@B:<B>a <I>b<$>\r\n"
        '@B:x <@Emph>a<@$> <BI>c<P> d <B>e<B> <f"Arial">g<f$> <c"Red">h<c$>\r\n'
        '@B:<f"Arial"><z10>Whole<f$><z$>\r\n'
        "@B:x <z12>a<z14>b<z$> d\r\n"
    )
    with open(os.path.join(out, "pairs_utf8.xtg"), "wb") as fh:
        fh.write(pairs.encode("utf-8"))


main()
