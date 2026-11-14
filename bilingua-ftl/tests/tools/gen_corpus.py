"""Writes the made-up Fluent corpus to bilingua/oracle/corpus/ftl/ (Lumen Bakery is fictional). Stdlib only.

    python gen_corpus.py ../../../oracle/corpus/ftl
"""

import os
import sys

# lang: (login, one loaf, many loaves, tagline, tooltip, brand)
LANGS = {
    "en": ("Log in", "one loaf", "loaves", "Fresh bread every morning", "Click to log in", "Lumen Bakery"),
    "de": ("Anmelden", "ein Brot", "Brote", "Jeden Morgen frisches Brot", "Zum Anmelden klicken", "Lumen Backstube"),
    "fr": ("Se connecter", "un pain", "pains", "Du pain frais chaque matin", "Cliquez pour vous connecter", "Boulangerie Lumen"),
    "es": ("Iniciar sesión", "un pan", "panes", "Pan fresco cada mañana", "Pulse para iniciar sesión", "Panadería Lumen"),
    "ja": ("ログイン", "パン1個", "個のパン", "毎朝焼きたてのパン", "クリックしてログイン", "ルーメンベーカリー"),
    "zh": ("登录", "一个面包", "个面包", "每天早晨新鲜面包", "点击登录", "流明面包店"),
    "ko": ("로그인", "빵 한 개", "개의 빵", "매일 아침 신선한 빵", "로그인하려면 클릭", "루멘 베이커리"),
    "ar": ("تسجيل الدخول", "رغيف واحد", "أرغفة", "خبز طازج كل صباح", "انقر لتسجيل الدخول", "مخبز لومن"),
    "ru": ("Войти", "один хлеб", "хлебов", "Свежий хлеб каждое утро", "Нажмите, чтобы войти", "Пекарня Люмен"),
    "pt": ("Entrar", "um pão", "pães", "Pão fresco toda manhã", "Clique para entrar", "Padaria Lumen"),
    "tr": ("Giriş yap", "bir ekmek", "ekmek", "Her sabah taze ekmek", "Giriş yapmak için tıklayın", "Lumen Fırın"),
}


def app(lang):
    a, one, many, tag, tip, brand = LANGS[lang]
    return (
        f"### Lumen Bakery ({lang})\n\n"
        f"# Shown on the login button\nlogin = {a}\n    .title = {tip}\n    .accesskey = L\n\n"
        f"## Shop\n\n-brand-name = {brand}\n    .gender = feminine\n\n"
        f"welcome = {{ $name }}, {{ -brand-name }}: {tag}\n\n"
        f"loaves = {{ $count ->\n    [one] {{ $count }} {one}\n   *[other] {{ $count }} {many}\n}}\n\n"
        f"tagline =\n    {tag}\n    {{ -brand-name }}\n\n"
        f"price = {{ NUMBER($amount, minimumFractionDigits: 2) }}\n"
    )


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)

    def put(name, data):
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data)

    for lang in LANGS:
        put(f"app_{lang}.ftl", app(lang).encode("utf-8"))
    put("app_de_crlf_bom.ftl", b"\xef\xbb\xbf" + app("de").replace("\n", "\r\n").encode("utf-8"))
    put(
        "edge.ftl",
        (
            "# comment without a blank line after it\nfirst = First\n\n"
            "broken = { $x\nref = { first }\nempty =\n\n"
            "braces = Use { \"{\" } and { \"}\" } literally\n"
            "multi =\n    Line one\n      indented more\n    last { $v }\n\n"
            "shared = { $gender ->\n    [male] { $n ->\n        [one] He shared one photo.\n       *[other] He shared { $n } photos.\n    }\n   *[other] They shared photos.\n}\n\n"
            "place = { NUMBER($n, type: \"ordinal\") ->\n    [one] { $n }st\n   *[other] { $n }th\n}\n\n"
            "num = { $n ->\n    [0] none\n    [1] one\n   *[other] many\n}\n\n"
            "-term = Term value\n    .case = nominative\n\n"
            "only-attrs =\n    .placeholder = Search the web\n    .aria-label = Search\n\n"
            "trailing = Spaces after   \nlast = Last one"
        ).encode("utf-8"),
    )
    put("plain_ascii.ftl", b"hello = Hello\nbye = Goodbye\n")


main()
