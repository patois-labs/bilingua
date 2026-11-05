"""Writes the made-up INI corpus to bilingua/oracle/corpus/ini/ (Lumen Bakery is fictional). Stdlib only.

    python gen_corpus.py ../../../oracle/corpus/ini
"""

import os
import sys

LANGS = {
    "en": ("Log in", "Click \\\"Save\\\" to continue", "%d loaves in %s", "Fresh bread every morning"),
    "de": ("Anmelden", "Klicken Sie auf \\\"Speichern\\\"", "%d Brote in %s", "Jeden Morgen frisches Brot"),
    "fr": ("Se connecter", "Cliquez sur \\\"Enregistrer\\\"", "%d pains dans %s", "Du pain frais chaque matin"),
    "es": ("Iniciar sesión", "Pulse \\\"Guardar\\\"", "%d panes en %s", "Pan fresco cada mañana"),
    "ja": ("ログイン", "\\\"保存\\\"をクリック", "%sに%d個のパン", "毎朝焼きたてのパン"),
    "zh": ("登录", "点击\\\"保存\\\"继续", "%s中有%d个面包", "每天早晨新鲜面包"),
    "ko": ("로그인", "\\\"저장\\\"을 눌러주세요", "%s에 빵 %d개", "매일 아침 신선한 빵"),
    "ar": ("تسجيل الدخول", "انقر \\\"حفظ\\\" للمتابعة", "%d خبز في %s", "خبز طازج كل صباح"),
    "ru": ("Войти", "Нажмите \\\"Сохранить\\\"", "%d хлеба в %s", "Свежий хлеб каждое утро"),
    "pt": ("Entrar", "Clique em \\\"Salvar\\\"", "%d pães em %s", "Pão fresco toda manhã"),
    "tr": ("Giriş yap", "\\\"Kaydet\\\"e tıklayın", "%s içinde %d ekmek", "Her sabah taze ekmek"),
}


EXTRA_FR = "[Extra]" + chr(13) + chr(10) + "note = Caf" + chr(0xe9) + " " + chr(0xe0) + " la cr" + chr(0xe8) + "me " + chr(0x2013) + " " + chr(0xe9) + "t" + chr(0xe9) + chr(13) + chr(10)


def joomla(lang):
    a, b, c, d = LANGS[lang]
    return (
        f"; Lumen Bakery language file ({lang})\n; Note : All ini files need to be saved as UTF-8\n\n"
        f"; Shown on the login button\nCOM_LUMEN_LOGIN=\"{a}\"\nCOM_LUMEN_SAVE=\"{b}\"\n"
        f"COM_LUMEN_COUNT=\"{c}\"\nCOM_LUMEN_TAGLINE=\"{d}\"\nCOM_LUMEN_EMPTY=\"\"\n"
        f"COM_LUMEN_HTML=\"<strong>{a}</strong> {d}\"\n"
    )


def generic(lang):
    a, b, c, d = LANGS[lang]
    return (
        f"[General]\r\n# Window caption\r\ntitle = {a}\r\nversion=1.2.3\r\njust a stray line\r\n\r\n"
        f"[Messages]\r\ntitle = {d}\r\nwelcome =   {b}   \r\ncount = {c}\r\n"
    )


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)

    def put(name, data):
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data)

    for lang in LANGS:
        put(f"joomla_{lang}.ini", joomla(lang).encode("utf-8"))
    for lang in ("en", "ja", "ar"):
        put(f"generic_{lang}.ini", generic(lang).encode("utf-8"))
    put("generic_de_utf8bom.ini", b"\xef\xbb\xbf" + generic("de").encode("utf-8"))
    put("generic_zh_utf16le.ini", b"\xff\xfe" + generic("zh").encode("utf-16-le"))
    put("generic_ko_utf16be.ini", b"\xfe\xff" + generic("ko").encode("utf-16-be"))
    put("generic_fr_cp1252.ini", (generic("fr") + EXTRA_FR).encode("cp1252"))
    put("legacy_qq.ini", 'COM_A="Press "_QQ_"Go"_QQ_" now"\nCOM_B="Say \\"hi\\""\n'.encode())
    put("edge.ini", b"=\n= =\n[]\n[unclosed\n;\n[A]\ndup=one\n[B]\ndup=two\ndup=three\nk=\"\"\nq=\"x\nlonely\n")
    put("mixed_newlines.ini", b"[S]\r\na=One\nb=Two\rc=Three\r\n")


if __name__ == "__main__":
    main()
