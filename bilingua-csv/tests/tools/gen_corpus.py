r"""Writes the made-up CSV/TSV corpus in bilingua/oracle/corpus/csv (Lumen Bakery is a fictional company).

    python gen_corpus.py ../../../oracle/corpus/csv

Standard library only. Every file parses with the default options (no mapping, no source language), so the facade
contract test can read them; the legacy code pages other than Windows-1252 are tested from bytes made in the Rust
tests (they need the source language).
"""

import os
import sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "corpus-csv"
os.makedirs(OUT, exist_ok=True)


def put(name, data):
    if isinstance(data, str):
        data = data.encode("utf-8")
    with open(os.path.join(OUT, name), "wb") as fh:
        fh.write(data)


def lines(rows, delim=",", nl="\r\n", final=True):
    def q(v):
        if any(c in v for c in (delim, '"', "\r", "\n")):
            return '"' + v.replace('"', '""') + '"'
        return v

    body = nl.join(delim.join(q(c) for c in r) for r in rows)
    return body + (nl if final else "")


MENU = [
    ["key", "source", "target", "context", "max length"],
    ["btn.save", "Save", "", "button on the order form", "12"],
    ["btn.cancel", "Cancel", "", "button", "12"],
    ["msg.welcome", "Welcome back, baker!", "", "home screen", ""],
    ["msg.order", 'Your order "Sourdough Deluxe" is on its way.', "", "email", ""],
    ["msg.hours", "Open Monday to Friday,\n7 AM to 6 PM.", "", "footer, two lines", ""],
    ["msg.items", "%d items in your basket (%s)", "", "placeholders", ""],
    ["msg.tags", "<b>Fresh</b> today", "", "markup", ""],
    ["num.price", "12.50", "", "not text", ""],
    ["f.sum", "=SUM(A1:A3)", "", "formula", ""],
    ["f.text", '="0012"', "", "excel text number", ""],
    ["msg.two", "Fresh bread daily. Order before noon for same-day delivery.", "", "two sentences", ""],
    ["msg.minus", "-5 degrees inside the cooler", "", "starts with a minus", ""],
    ["msg.plus", "+1 free croissant", "", "starts with a plus", ""],
    ["msg.at", "@home delivery", "", "starts with at", ""],
]
put("menu_crlf.csv", lines(MENU))
put("menu_lf_nofinal.csv", lines(MENU, nl="\n", final=False))
put("menu_cr_only.csv", lines(MENU[:8], nl="\r"))

SENT = {
    "en": "Welcome to Lumen Bakery.",
    "fr": "Bienvenue chez Lumen Bakery, le pain du matin.",
    "de": "Willkommen bei Lumen Bakery: Brot für jeden Tag.",
    "es": "Bienvenido a Lumen Bakery, ¿qué desea hoy?",
    "ja": "ルーメンベーカリーへようこそ。",
    "zh": "欢迎光明面包店。",
    "ko": "루멘 베이커리에 오신 것을 환영합니다.",
    "ar": "مرحبا بكم في مخبز لومن.",
    "ru": "Добро пожаловать в пекарню Люмен.",
    "el": "Καλώς ήρθατε στο φούρνο.",
    "tr": "Lümen Fırınına hoş geldiniz.",
}
langs = list(SENT)
multi = [["key"] + langs + ["notes"]]
multi.append(["greeting"] + [SENT[l] for l in langs] + ["shown on the home page"])
multi.append(["bye"] + ["See you soon"] + [""] * (len(langs) - 1) + [""])
multi.append(["qty", "\"3 loaves\", please"] + [""] * (len(langs) - 1) + ["quoted comma"])
put("multilingual_semicolon_bom.csv", b"\xef\xbb\xbf" + lines(multi, ";").encode("utf-8"))
put("multilingual_tab.tsv", lines(multi, "\t", "\n"))

for lang in ("en", "fr", "de", "es", "ja", "zh", "ko", "ar", "ru", "el", "tr"):
    rows = [["id", "text"], ["1", SENT[lang]], ["2", SENT[lang] + " (" + lang + ")"], ["3", "7"]]
    put(f"strings_{lang}.csv", lines(rows, nl="\n"))

put("pipe_strings.csv", lines([["id", "text", "note"], ["1", "Open the oven, then wait", "step"], ["2", "Cool for 10 minutes", "step"]], "|", "\n"))
put("catalog.tsv", lines([["sku", "Name", "Description"], ["L-100", "Sourdough loaf", "Slow-fermented, 24 hours"], ["L-200", "Rye \"dark\"", "Caraway & molasses"]], "\t", "\r\n"))
put("glossary.tab", lines([["term", "translation"], ["crumb", "Krume"], ["proof", "Gare"], ["starter", "Sauerteig-Ansatz"]], "\t", "\n"))
put("no_header.csv", lines([["Open early", "Close late"], ["Fresh bread", "Warm rolls"], ["42", "3.5"]], nl="\n"))
put("blank_lines.csv", "source,target\n\nOne,\n\n\nTwo,\n")
put("empty_cells.csv", lines([["source", "target"], ["", ""], ["   ", ""], ["Text", ""], ["", "Done"]], nl="\n"))
put("mixed_endings.csv", "source,target\nOne,\r\nTwo,\rThree,\n")
put("quoted_multiline.csv", 'source,target\n"Line one\nline two\nline three",\n"He said ""stop"", then left",\n"Trailing quote""",\n')
put("formulas.csv", lines([["source", "target"], ["=1+1", ""], ["+1 call", ""], ["-dash start", ""], ["@mention", ""], ["Plain", ""]], nl="\n"))
put("wide.csv", ",".join(f"col{i}" for i in range(60)) + "\n" + ",".join(f"Value number {i}" for i in range(60)) + "\n")
put("quote_in_field.csv", 'source,target\nTall 6" loaf,\nSay "hi" to us,\n')
put("space_after_delimiter.csv", "source, target\nOne, \nTwo, \n")
put("pipe_in_text.csv", 'source,target\n"a | b",\n"c ; d",\n')

# encodings
put("utf16le.csv", b"\xff\xfe" + lines([["source", "target"], ["Hello, world", ""], ["Café", ""]], nl="\r\n").encode("utf-16-le"))
put("utf16be.tsv", b"\xfe\xff" + lines([["source", "target"], ["Hello world", ""], ["日本語", ""]], "\t", "\n").encode("utf-16-be"))
put("cp1252.csv", lines([["source", "target"], ["Café crème", ""], ["Crêpe – €4", ""], ["Naïve", ""]], nl="\r\n").encode("cp1252"))
put("cp1252.tsv", lines([["Name", "Preis"], ["Brötchen", "0,60 €"], ["Geräuchert", "3,10 €"]], "\t", "\n").encode("cp1252"))
print(len(os.listdir(OUT)), "files in", OUT)
