"""Writes tests/data/lang_*.ts: one made-up Qt Linguist file per language, some in legacy encodings, UTF-16 and with
a BOM and CRLF. Plain Python 3, no dependencies:  python gen_fixtures.py  (run from anywhere)."""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")

# language code, Qt language attribute, forms of "%n file(s)" the file carries, strings
LANGS = {
    "en": ("en_US", ["Open file", "Saved %1 of %2 pages", "%n file(s) deleted", "Settings"]),
    "de": ("de_DE", ["Datei öffnen", "%1 von %2 Seiten gespeichert", "%n Datei(en) gelöscht", "Einstellungen"]),
    "fr": ("fr_FR", ["Ouvrir le fichier", "%1 pages sur %2 enregistrées", "%n fichier(s) supprimé(s)", "Paramètres"]),
    "es": ("es_ES", ["Abrir archivo", "Guardadas %1 de %2 páginas", "%n archivo(s) eliminado(s)", "Configuración"]),
    "pt": ("pt_BR", ["Abrir arquivo", "%1 de %2 páginas salvas", "%n arquivo(s) excluído(s)", "Configurações"]),
    "ru": ("ru_RU", ["Открыть файл", "Сохранено %1 из %2 страниц", "Удалено файлов: %n", "Настройки"]),
    "ar": ("ar_SA", ["فتح الملف", "تم حفظ %1 من %2 صفحات", "تم حذف %n ملف", "الإعدادات"]),
    "ja": ("ja_JP", ["ファイルを開く", "%2ページ中%1ページを保存しました", "%n個のファイルを削除しました", "設定"]),
    "zh": ("zh_CN", ["打开文件", "已保存 %1 / %2 页", "已删除 %n 个文件", "设置"]),
    "ko": ("ko_KR", ["파일 열기", "%2페이지 중 %1페이지 저장됨", "파일 %n개 삭제됨", "설정"]),
    "tr": ("tr_TR", ["Dosyayı aç", "%2 sayfanın %1 tanesi kaydedildi", "%n dosya silindi", "Ayarlar"]),
}


def doc(qt_lang, s, enc="utf-8", nl="\n", numerus_forms=2):
    forms = "".join(f"            <numerusform>{s[2]}</numerusform>{nl}" for _ in range(numerus_forms))
    lines = [
        f'<?xml version="1.0" encoding="{enc}"?>',
        "<!DOCTYPE TS>",
        f'<TS version="2.1" language="{qt_lang}" sourcelanguage="en_US">',
        "<context>",
        "    <name>MainWindow</name>",
        "    <message>",
        '        <location filename="../src/mainwindow.cpp" line="42"/>',
        f"        <source>&amp;{LANGS['en'][1][0]}...</source>",
        "        <comment>menu entry</comment>",
        f'        <translation type="unfinished">&amp;{s[0]}...</translation>',
        "    </message>",
        "    <message>",
        f"        <source>{LANGS['en'][1][1]}</source>",
        "        <extracomment>%1 and %2 are page counts</extracomment>",
        '        <translation type="unfinished"></translation>',
        "    </message>",
        '    <message numerus="yes">',
        f"        <source>{LANGS['en'][1][2]}</source>",
        '        <translation type="unfinished">',
    ]
    out = nl.join(lines) + nl + forms + nl.join(
        [
            "        </translation>",
            "    </message>",
            "</context>",
            "<context>",
            "    <name>Preferences</name>",
            "    <message>",
            f"        <source>{LANGS['en'][1][3]}</source>",
            f"        <translation>{s[3]}</translation>",
            "    </message>",
            "    <message>",
            '        <source>Tab<byte value="x9"/>separated</source>',
            '        <translation type="unfinished"/>',
            "    </message>",
            "</context>",
            "</TS>",
            "",
        ]
    )
    return out


def write(name, text, codec, bom=b""):
    with open(os.path.join(OUT, name), "wb") as fh:
        fh.write(bom + text.encode(codec))


def main():
    for k, (q, s) in LANGS.items():
        write(f"lang_{k}.ts", doc(q, s, numerus_forms={"ru": 3, "ar": 6, "ja": 1, "zh": 1, "ko": 1, "tr": 2}.get(k, 2)), "utf-8")
    q, s = LANGS["en"]
    write("lang_en_utf8bom_crlf.ts", doc(q, s, nl="\r\n"), "utf-8", b"\xef\xbb\xbf")
    for k, codec, label in [
        ("de", "latin-1", "ISO-8859-1"), ("fr", "cp1252", "windows-1252"), ("ru", "cp1251", "windows-1251"),
        ("ru", "koi8-r", "KOI8-R"), ("ar", "cp1256", "windows-1256"), ("tr", "cp1254", "windows-1254"),
        ("ja", "shift_jis", "Shift_JIS"), ("ja", "euc_jp", "EUC-JP"), ("ko", "euc_kr", "EUC-KR"),
        ("zh", "gbk", "GBK"), ("zh", "gb2312", "GB2312"),
    ]:
        q, s = LANGS[k]
        suffix = {"latin-1": "latin1", "cp1252": "cp1252", "cp1251": "cp1251", "koi8-r": "koi8r", "cp1256": "cp1256", "cp1254": "cp1254",
                  "shift_jis": "shiftjis", "euc_jp": "eucjp", "euc_kr": "euckr", "gbk": "gbk", "gb2312": "gb2312"}[codec]
        write(f"lang_{k}_{suffix}.ts", doc(q, s, enc=label), codec)
    for k, codec, label in [("ar", "utf-16-le", "utf-16"), ("ko", "utf-16-be", "utf-16"), ("zh", "utf-16-le", "utf-16")]:
        q, s = LANGS[k]
        bom = b"\xff\xfe" if codec.endswith("le") else b"\xfe\xff"
        write(f"lang_{k}_{codec.replace('-', '')}.ts", doc(q, s, enc=label, numerus_forms=6 if k == "ar" else 1), codec, bom)


main()
