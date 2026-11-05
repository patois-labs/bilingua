"""Writes the made-up DTD corpus to bilingua/oracle/corpus/dtd/ (Lumen Bakery is fictional). Stdlib only.

    python gen_corpus.py ../../../oracle/corpus/dtd [OKAPI_DTD_DIR]

OKAPI_DTD_DIR (optional) is the folder with Okapi's Test01.dtd (Apache 2.0); it is copied in.
"""

import os
import shutil
import sys

LANGS = {
    "en": ("Lumen Bakery", "Log in", "Save & close", 'Click "Save" to continue', "Fresh bread every morning"),
    "de": ("Lumen Bäckerei", "Anmelden", "Speichern & schließen", 'Klicken Sie auf "Speichern"', "Jeden Morgen frisches Brot"),
    "fr": ("Boulangerie Lumen", "Se connecter", "Enregistrer et fermer", "Cliquez sur « Enregistrer »", "Du pain frais chaque matin"),
    "es": ("Panadería Lumen", "Iniciar sesión", "Guardar y cerrar", 'Pulse "Guardar" para continuar', "Pan fresco cada mañana"),
    "ja": ("ルーメン・ベーカリー", "ログイン", "保存して閉じる", "「保存」をクリックしてください", "毎朝焼きたてのパン"),
    "zh": ("流明面包店", "登录", "保存并关闭", "点击“保存”继续", "每天早晨新鲜面包"),
    "ko": ("루멘 베이커리", "로그인", "저장 후 닫기", "'저장'을 눌러주세요", "매일 아침 신선한 빵"),
    "ar": ("مخبز لومن", "تسجيل الدخول", "حفظ وإغلاق", "انقر على \"حفظ\" للمتابعة", "خبز طازج كل صباح"),
    "ru": ("Пекарня Люмен", "Войти", "Сохранить и закрыть", "Нажмите «Сохранить»", "Свежий хлеб каждое утро"),
    "pt": ("Padaria Lumen", "Entrar", "Salvar e fechar", 'Clique em "Salvar"', "Pão fresco toda manhã"),
    "tr": ("Lumen Fırın", "Giriş yap", "Kaydet ve kapat", "\"Kaydet\"e tıklayın", "Her sabah taze ekmek"),
}


def esc(s):
    return s.replace("&", "&amp;").replace('"', "&quot;").replace("%", "&#037;")


def browser(lang):
    brand, login, save, click, tag = LANGS[lang]
    return (
        "<!-- This Source Code Form is subject to the terms of the Mozilla Public\n"
        "   - License, v. 2.0. If a copy of the MPL was not distributed with this\n"
        "   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->\n\n"
        '<!ENTITY % brandDTD SYSTEM "chrome://branding/locale/brand.dtd">\n%brandDTD;\n\n'
        f"<!-- LOCALIZATION NOTE (window.title): &brandShortName; is the product name ({lang}). -->\n"
        f'<!ENTITY window.title "&brandShortName; - {esc(brand)}">\n'
        f'<!ENTITY login.label "{esc(login)}">\n<!ENTITY login.accesskey "{login[0]}">\n'
        f"<!ENTITY save.label '{esc(save)}'>\n"
        f'<!ENTITY click.text "{esc(click)}">\n'
        f'<!ENTITY tagline.text "<html:b>{esc(tag)}</html:b> &#169; 100&#037;">\n'
        '<!ENTITY ref.only "&brandShortName;">\n'
        f'<!ENTITY escaped.markup "&lt;i>{esc(tag)}&lt;/i> &lt;now&gt;">\n'
    )


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)

    def put(name, data):
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data if isinstance(data, bytes) else data.encode("utf-8"))

    for lang in LANGS:
        put(f"browser_{lang}.dtd", browser(lang))
    put(
        "locos_src.dtd",
        b"""<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. -->

<!ENTITY % brandDTD SYSTEM "chrome://branding/locale/brand.dtd">
%brandDTD;

<!-- LOCALIZATION NOTE (window.title): &brandShortName; is the product name. -->
<!ENTITY window.title "&brandShortName; Settings">
<!ENTITY save.label 'Save &amp; close'>
<!ENTITY save.accesskey "S">
<!ENTITY quote.text "Say &quot;hi&quot; to &#169; 100&#037;">
<!ENTITY markup.text "Click <html:b>here</html:b> &lt;now&gt;">
<!ENTITY only.ref "&brandShortName;">
<!ENTITY   spaced.label
     "Spaced out"  >
<!ENTITY ext.file SYSTEM "http://example.invalid/evil.dtd">
""",
    )
    put("bom_crlf.dtd", b"\xef\xbb\xbf" + b'<!-- c -->\r\n<!ENTITY a "One">\r\n\r\n<!-- note b -->\r\n<!ENTITY b "Two">\r\n')
    put(
        "edge.dtd",
        b'<?xml version="1.0" encoding="UTF-8"?>\n<!ELEMENT r (#PCDATA)>\n<!ATTLIST r a CDATA "x>y">\n'
        b'<!ENTITY a "Alpha">\n<!ENTITY pub PUBLIC "-//X//EN" "x.dtd">\n<!ENTITY % pe "<!ENTITY inner \'Hidden\'>">\n'
        b'<!ENTITY lol "lol">\n<!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;">\n<!ENTITY num "42">\n<!ENTITY sym "---">\n'
        b'<!ENTITY multi "line one\nline two">\n<!ENTITY empty "">\n<!ENTITY pad "  padded  ">\n',
    )
    if len(sys.argv) > 2:
        for n in ("Test01.dtd",):  # Test02.dtd has no entity value; it lives in tests/data
            shutil.copyfile(os.path.join(sys.argv[2], n), os.path.join(out, "okapi_" + n.lower()))


if __name__ == "__main__":
    main()
