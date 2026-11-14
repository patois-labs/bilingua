"""Writes the made-up STF corpus to bilingua/oracle/corpus/stf/ (Lumen Bakery is fictional). Stdlib only.

    python gen_corpus.py ../../../oracle/corpus/stf
"""

import os
import sys

T = "\t"
# (language code in Salesforce form, four labels)
LANGS = {
    "en": ("en_US", ("Log in", 'Click "Save" to continue', "Items in your basket", "Fresh bread every morning")),
    "de": ("de", ("Anmelden", 'Klicken Sie auf "Speichern"', "Artikel in Ihrem Warenkorb", "Jeden Morgen frisches Brot")),
    "fr": ("fr", ("Se connecter", 'Cliquez sur "Enregistrer"', "Articles dans votre panier", "Du pain frais chaque matin")),
    "es": ("es_MX", ("Iniciar sesión", 'Pulse "Guardar"', "Artículos en su cesta", "Pan fresco cada mañana")),
    "ja": ("ja", ("ログイン", "「保存」をクリック", "カートの商品", "毎朝焼きたてのパン")),
    "zh": ("zh_CN", ("登录", "点击“保存”继续", "购物车中的商品", "每天早晨新鲜面包")),
    "ko": ("ko", ("로그인", "저장을 눌러주세요", "장바구니 품목", "매일 아침 신선한 빵")),
    "ar": ("ar", ("تسجيل الدخول", "انقر حفظ للمتابعة", "العناصر في سلتك", "خبز طازج كل صباح")),
    "ru": ("ru", ("Войти", "Нажмите «Сохранить»", "Товары в корзине", "Свежий хлеб каждое утро")),
    "pt": ("pt_BR", ("Entrar", 'Clique em "Salvar"', "Itens no seu carrinho", "Pão fresco toda manhã")),
    "he": ("iw", ("התחברות", "לחץ על שמור", "פריטים בעגלה", "לחם טרי כל בוקר")),
}


def bilingual(lang, nl="\n"):
    code, (a, b, c, d) = LANGS[lang]
    rows = [
        "# Use the Bilingual file to review translations and add translations.",
        "# - The TRANSLATED section of the file contains text that has been translated.",
        "# - The UNTRANSLATED section of the file contains text that hasn't been translated.",
        f"Language code: {code}",
        "Type: Bilingual",
        "Translation type: Metadata",
        "",
        "------------------TRANSLATED-------------------",
        "",
        f"# KEY{T}LABEL{T}TRANSLATION{T}OUT OF DATE",
        "",
        "# CustomField",
        f"CustomField.Account.Region.FieldLabel{T}{a}{T}{a}{T}-",
        f"CustomField.Account.Region.HelpText{T}{b}\\n{d}{T}{b}{T}*",
        "",
        "------------------UNTRANSLATED-----------------",
        "",
        f"# KEY{T}LABEL",
        "",
        "# CustomLabel",
        f"CustomLabel.Welcome{T}{a}, {{0}}! {c} {{!$User.FirstName}}.",
        f"CustomLabel.Count{T}{{0,number}} {c}",
        f"PicklistValue.Lumen.Bakery.Fresh{T}{d}",
        f"CustomLabel.Blank{T}",
        f"CustomLabel.Escapes{T}\\n",
        f"CustomLabel.Tabbed{T}{a}\\t{d}\\\\done",
    ]
    return nl.join(rows) + nl


def source(lang, nl="\r\n"):
    code, (a, b, c, d) = LANGS[lang]
    rows = [
        "# Use the Source file to translate labels for the first time.",
        f"Language code: {code}",
        "Type: Source",
        "Translation type: Metadata",
        "",
        f"# KEY{T}LABEL",
        "",
        f"ButtonOrLink.Account.Call{T}{a}",
        f"LayoutSection.Account.Account Layout.Address{T}{c}",
        f"CustomLabel.Tagline{T}{d} {{0}}",
    ]
    return nl.join(rows) + nl


def outdated(lang):
    code, (a, b, c, d) = LANGS[lang]
    rows = [
        f"Language code: {code}",
        "Type: Outdated and untranslated",
        "",
        "------------------OUTDATED AND UNTRANSLATED-----------------",
        "",
        f"# KEY{T}LABEL",
        f"CustomLabel.Save{T}{b}",
        f"ValidationFormula.Account.Rule1.ErrorMessage{T}{c}",
    ]
    return "\n".join(rows) + "\n"


EDGE = (
    "Language code:\tde\n"
    "Type: Source\n"
    f"# KEY{T}LABEL\n"
    f"\torphan label\n"
    f"CustomLabel.A{T}Alpha second\x85third\x0cfourth fifth\n"
    f"CustomLabel.B{T}{{0}}\n"
    f"#CustomLabel.C{T}Commented\n"
    "stray line without a tab\n"
    f"CustomLabel.D{T}Last row without a final line break"
)


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)

    def put(name, data):
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data)

    for lang in LANGS:
        put(f"bilingual_{lang}.stf", bilingual(lang).encode("utf-8"))
    for lang in ("en", "ja", "ar"):
        put(f"source_{lang}.stf", source(lang).encode("utf-8"))
    for lang in ("fr", "ko"):
        put(f"outdated_{lang}.stf", outdated(lang).encode("utf-8"))
    put("bilingual_de_utf8bom.stf", b"\xef\xbb\xbf" + bilingual("de").encode("utf-8"))
    put("bilingual_zh_utf16le.stf", b"\xff\xfe" + bilingual("zh").encode("utf-16-le"))
    put("bilingual_ko_utf16be.stf", b"\xfe\xff" + bilingual("ko").encode("utf-16-be"))
    put("bilingual_fr_cp1252.stf", (bilingual("fr") + f"CustomLabel.Cafe{T}Café crème – été\n").encode("cp1252"))
    put("source_mixed_newlines.stf", source("en", "\n").replace("Type: Source\n", "Type: Source\r\n").encode("utf-8"))
    put("edge.stf", EDGE.encode("utf-8"))


if __name__ == "__main__":
    main()
