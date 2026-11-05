"""Writes the made-up ASS / SSA corpus to bilingua/oracle/corpus/ass/ (Lumen Bakery is fictional). Stdlib only.

    python gen_corpus.py ../../../oracle/corpus/ass
"""

import os
import sys

# title, greeting, a line with a style switch, a two-line sign, a closing line
LANGS = {
    "en": ("Fresh bread every morning", "Welcome to Lumen Bakery, friends!", "Our {\\i1}sourdough{\\i0} is ready.", "OPEN\\NSeven days", "See you tomorrow, early."),
    "de": ("Jeden Morgen frisches Brot", "Willkommen in der Lumen-Bäckerei, Freunde!", "Unser {\\i1}Sauerteig{\\i0} ist fertig.", "OFFEN\\NSieben Tage", "Bis morgen, früh."),
    "fr": ("Du pain frais chaque matin", "Bienvenue à la boulangerie Lumen, les amis !", "Notre {\\i1}pain au levain{\\i0} est prêt.", "OUVERT\\NSept jours", "À demain, tôt."),
    "es": ("Pan fresco cada mañana", "¡Bienvenidos a la panadería Lumen, amigos!", "Nuestra {\\i1}masa madre{\\i0} está lista.", "ABIERTO\\NSiete días", "Hasta mañana, temprano."),
    "ja": ("毎朝焼きたてのパン", "ルーメンベーカリーへようこそ、皆さん！", "当店の{\\i1}サワードウ{\\i0}ができました。", "営業中\\N無休", "また明日、早いうちに。"),
    "zh": ("每天早晨新鲜面包", "欢迎来到鲁梅面包房，朋友们！", "我们的{\\i1}酸面包{\\i0}好了。", "营业中\\N全周", "明天见，记得早点。"),
    "ko": ("매일 아침 신선한 빵", "루멘 베이커리에 오신 것을 환영합니다!", "저희 {\\i1}사워도우{\\i0}가 준비됐어요.", "영업 중\\N일주일", "내일 일찍 만나요."),
    "ar": ("خبز طازج كل صباح", "مرحباً بكم في مخبز لومن، أصدقائي!", "{\\i1}العجين الحامض{\\i0} جاهز.", "مفتوح\\Nسبعة أيام", "أراكم غداً، باكراً."),
    "ru": ("Свежий хлеб каждое утро", "Добро пожаловать в пекарню Люмен, друзья!", "Наша {\\i1}закваска{\\i0} готова.", "ОТКРЫТО\\NСемь дней", "До завтра, рано."),
    "pt": ("Pão fresco toda manhã", "Bem-vindos à padaria Lumen, amigos!", "O nosso {\\i1}pão de fermentação natural{\\i0} está pronto.", "ABERTO\\NSete dias", "Até amanhã, cedo."),
    "tr": ("Her sabah taze ekmek", "Lumen Fırını'na hoş geldiniz, dostlar!", "{\\i1}Ekşi mayalı{\\i0} ekmeğimiz hazır.", "AÇIK\\NYedi gün", "Yarın görüşmek üzere, erkenden."),
}

STYLES = (
    "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, "
    "Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
    "Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n"
)
EVENTS_FMT = "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"


def ass(lang):
    t, a, b, c, d = LANGS[lang]
    return (
        f"[Script Info]\n; Lumen Bakery ({lang})\nTitle: {t}\nScriptType: v4.00+\nPlayResX: 1920\n\n{STYLES}\n"
        f"[Events]\n{EVENTS_FMT}"
        f"Dialogue: 0,0:00:01.00,0:00:04.00,Default,Anna,0,0,0,,{a}\n"
        f"Comment: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,Note for the editor: keep it short\n"
        f"Dialogue: 0,0:00:05.00,0:00:07.50,Default,,0,0,0,,{{\\an8}}{b}\n"
        f"Dialogue: 1,0:00:08.00,0:00:10.00,Sign,,0,0,0,,{{\\pos(960,200)}}{c}\n"
        f"Dialogue: 2,0:00:10.00,0:00:11.00,Default,,0,0,0,,{{\\p1}}m 0 0 l 100 0 100 100 0 100{{\\p0}}\n"
        f"Dialogue: 0,0:00:12.00,0:00:14.00,Default,,0,0,0,,{d}\\N{{\\i1}}{a}{{\\i0}}\n"
    )


def ssa(lang):
    t, a, b, c, d = LANGS[lang]
    return (
        f"[Script Info]\nTitle: {t}\nScriptType: v4.00\n\n"
        "[V4 Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, "
        "BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\n"
        "Style: Default,Arial,20,65535,65535,65535,-2147483640,-1,0,1,3,0,2,30,30,30,0,0\n\n"
        "[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
        f"Dialogue: Marked=0,0:00:01.00,0:00:04.00,*Default,NTP,0000,0000,0000,!Effect,{a}\n"
        f"Dialogue: Marked=0,0:00:05.00,0:00:07.50,*Default,NTP,0000,0000,0000,,{b}\n"
    )


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)

    def put(name, data):
        with open(os.path.join(out, name), "wb") as fh:
            fh.write(data)

    for lang in LANGS:
        put(f"lumen_{lang}.ass", ass(lang).encode("utf-8"))
    for lang in ("en", "ja", "ar"):
        put(f"lumen_{lang}.ssa", ssa(lang).encode("utf-8"))
    put("lumen_de_utf8bom_crlf.ass", b"\xef\xbb\xbf" + ass("de").replace("\n", "\r\n").encode("utf-8"))
    put("lumen_zh_utf16le.ass", b"\xff\xfe" + ass("zh").encode("utf-16-le"))
    put("lumen_ko_utf16be.ass", b"\xfe\xff" + ass("ko").encode("utf-16-be"))
    put("lumen_fr_cp1252.ass", ass("fr").replace("’", "'").encode("cp1252"))
    put("lumen_pt_cp1252.ssa", ssa("pt").encode("cp1252"))
    put("lumen_ja_cr.ass", ass("ja").replace("\n", "\r").encode("utf-8"))
    put(
        "pairs.ass",
        (
            "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
            "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Account {\\i1}settings{\\i0}\n"
            "Dialogue: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,{\\an8}{\\b1}Account{\\b0} settings\n"
            "Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\b1}a {\\i1}b{\\r} c\n"
            "Dialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,Say {\\i1}very {\\b1}{\\c&H0000FF&}loud{\\r} now\n"
            "Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,x {\\b1}a{\\i1}b{\\b0}c{\\i0} d\n"
        ).encode("utf-8"),
    )
    put(
        "edge.ass",
        (
            "﻿[script info]\nTITLE:   <Untitled>  \n\n[Events]\nformat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
            "Dialogue:0,0:00:01.00,0:00:02.00,Default,,0,0,0,,No space after the colon, kept\n"
            "Dialogue: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,Hi {there\n"
            "Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,One two\x0cthree four\n"
            "Dialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,   \n"
            "Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,{\\an8}\\N\\h\n"
            "Dialogue: 0,0:00:06.00,0:00:07.00\n"
            "dialogue: 0,0:00:07.00,0:00:08.00,Default,,0,0,0,,lower case is not an event\n"
            "Dialogue: 0,0:00:08.00,0:00:09.00,Default,,0,0,0,,  padded  \\N  more  \n"
            "Dialogue: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,⟦x9⟧ literal brackets\n"
        ).encode("utf-8"),
    )
    put(
        "short_format.ass",
        b"[Events]\nFormat: Start, End, Text\nDialogue: 0:00:01.00,0:00:02.00,Short, form, text\nDialogue: 0:00:02.00,0:00:03.00,{\\i1}Second{\\i0}\n",
    )
    put("mixed_newlines.ass", b"[Script Info]\r\nTitle: Mixed\n[Events]\rDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,One\r\nDialogue: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,Two\n")
    put("language_header.ass", ass("de").replace("ScriptType", "Language: en\nScriptType").encode("utf-8"))


if __name__ == "__main__":
    main()
