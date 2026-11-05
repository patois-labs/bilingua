"""Writes the fixtures in tests/data: the LocOS suite's two example files byte for byte, the two rows of
`verify_keys_bridge.py`, and one .desktop and one .catkeys per language (11 languages, source text in the language).

    python gen_fixtures.py ../data
"""

import os
import sys

DESKTOP = (
    "# Launcher for the example editor\n"
    "[Desktop Entry]\n"
    "Type=Application\n"
    "Version=1.5\n"
    "# Shown in menus\n"
    "Name=Text Editor\n"
    "Name[de]=Texteditor\n"
    "GenericName = Editor\n"
    "Comment=Edit\\stext files;\\nquickly\\\\safely\n"
    "Keywords=text;editor;plain\\;simple;\n"
    "Exec=example-editor %F\n"
    "Icon=accessories-text-editor\n"
    "Categories=Utility;TextEditor;\n"
    "Actions=new-window;\n"
    "\n"
    "[Desktop Action new-window]\n"
    "Name=New Window\n"
    "Exec=example-editor --new-window\n"
    "\n"
    "[X-Vendor Settings]\n"
    "Name=Not a launcher string\n"
)

CATKEYS = (
    "1\tenglish\tx-vnd.Example-App\t123456789\n"
    "Open\tMainWindow\t\tOpen\n"
    "Save %s?\tMainWindow\tfile name\tSave %s?\n"
    "(\tCalcView\t\t(\n"
    'Line one\\nLine "two"\tAbout\t\tLine one\\nLine "two"\n'
    "Quit\tMainWindow\t\tBeenden\n"
)

# language -> (name, comment, keywords, action name, haiku open, save, quit)
LANGS = {
    "en": ("Notes", "Take quick notes", "notes;memo;text", "New note", "Open", "Save %s?", "Quit"),
    "de": ("Notizen", "Schnell Notizen machen", "Notizen;Memo;Text", "Neue Notiz", "Öffnen", "%s speichern?", "Beenden"),
    "fr": ("Notes", "Prendre des notes rapidement", "notes;mémo;texte", "Nouvelle note", "Ouvrir", "Enregistrer %s ?", "Quitter"),
    "es": ("Notas", "Toma notas rápidas", "notas;memo;texto", "Nueva nota", "Abrir", "¿Guardar %s?", "Salir"),
    "ru": ("Заметки", "Быстрые заметки", "заметки;текст", "Новая заметка", "Открыть", "Сохранить %s?", "Выход"),
    "ar": ("ملاحظات", "دوّن ملاحظات سريعة", "ملاحظات;نص", "ملاحظة جديدة", "فتح", "حفظ %s؟", "خروج"),
    "ja": ("メモ帳", "すばやくメモを取る", "メモ;テキスト", "新しいメモ", "開く", "%s を保存しますか？", "終了"),
    "zh": ("笔记", "快速记笔记", "笔记;文本", "新笔记", "打开", "保存 %s？", "退出"),
    "ko": ("메모", "빠르게 메모하기", "메모;텍스트", "새 메모", "열기", "%s 저장할까요?", "종료"),
    "tr": ("Notlar", "Hızlı not alın", "notlar;metin", "Yeni not", "Aç", "%s kaydedilsin mi?", "Çık"),
    "hi": ("नोट्स", "जल्दी नोट्स लिखें", "नोट्स;पाठ", "नया नोट", "खोलें", "%s सहेजें?", "बाहर"),
}


def w(d, name, data):
    with open(os.path.join(d, name), "wb") as fh:
        fh.write(data if isinstance(data, bytes) else data.encode("utf-8"))


def main():
    d = sys.argv[1]
    w(d, "locos_example.desktop", DESKTOP)
    w(d, "locos_example.catkeys", CATKEYS)
    w(d, "bridge_app.desktop", b"[Desktop Entry]\nType=Application\nName=Notes\nComment=Take notes\nExec=notes\n")
    w(d, "bridge_en.catkeys", b"1\tenglish\tx-vnd.App\t12345\nOpen\tMenu\t\tOpen\n")
    w(d, "crlf_bom.desktop", b"\xef\xbb\xbf" + DESKTOP.replace("\n", "\r\n").encode("utf-8"))
    w(d, "crlf_bom.catkeys", b"\xef\xbb\xbf" + CATKEYS.replace("\n", "\r\n").encode("utf-8"))
    w(d, "no_final_newline.desktop", b"[Desktop Entry]\nType=Application\nName=Clock")
    for code, (name, comment, kw, act, op, sv, qt) in LANGS.items():
        w(
            d,
            f"lang_{code}.desktop",
            f"[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nKeywords={kw};\nExec=notes %F\n"
            f"Actions=new;\n\n[Desktop Action new]\nName={act}\nExec=notes --new\n",
        )
        w(
            d,
            f"lang_{code}.catkeys",
            f"1\t{code}\tx-vnd.Notes\t4711\n{op}\tMainWindow\t\t{op}\n{sv}\tMainWindow\tfile name\t{sv}\n{qt}\tMainWindow\t\t{qt}\n"
            f"{name}\tAbout\t\t{name}\n",
        )
    w(d, ".gitattributes", "* -text\n")


if __name__ == "__main__":
    main()
