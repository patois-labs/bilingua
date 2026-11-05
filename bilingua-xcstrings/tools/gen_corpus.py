"""Writes the made-up oracle corpus (Lumen Bakery is a fictional company) into oracle/corpus/xcstrings and
oracle/corpus/xcloc. Standard library only.

    python tools/gen_corpus.py OUT_CORPUS_DIR
"""
import io, json, os, sys, zipfile

out = sys.argv[1]
os.makedirs(os.path.join(out, "xcstrings"), exist_ok=True)
os.makedirs(os.path.join(out, "xcloc"), exist_ok=True)


def su(v, state="translated"):
    return {"stringUnit": {"state": state, "value": v}}


bakery = {
    "sourceLanguage": "en",
    "strings": {
        "Add to basket": {"comment": "Button on a product page", "localizations": {"de": su("In den Warenkorb"), "en": su("Add to basket")}},
        "%lld loaves": {
            "comment": "Basket line count",
            "localizations": {
                "en": {"variations": {"plural": {"one": su("%lld loaf"), "other": su("%lld loaves")}}},
                "fr": {"variations": {"plural": {"one": su("%lld pain"), "other": su("%lld pains")}}},
            },
        },
        "Pickup": {"localizations": {"en": {"variations": {"device": {"iphone": su("Pick up on your way"), "ipad": su("Pick up at the counter"), "mac": su("Pick up in store")}}}}},
        "order_summary": {
            "comment": "%1$@ is the customer name",
            "localizations": {
                "en": {
                    **su("%1$@, your order has %#@items@ and %#@loaves@."),
                    "substitutions": {
                        "items": {"argNum": 2, "formatSpecifier": "lld", "variations": {"plural": {"one": su("%arg item"), "other": su("%arg items")}}},
                        "loaves": {"argNum": 3, "formatSpecifier": "lld", "variations": {"plural": {"zero": su("no loaves"), "one": su("%arg loaf"), "other": su("%arg loaves")}}},
                    },
                }
            },
        },
        "Welcome back, %@!": {},
        "100%% butter": {"comment": "Literal percent"},
        "Café menu – today": {"localizations": {"en": su("Café menu – today")}},
        "Old promo": {"extractionState": "stale", "localizations": {"en": su("Spring promo")}},
        "SKU-%@": {"shouldTranslate": False},
        "Version": {"extractionState": "manual", "localizations": {"en": su("  Version %@ (build %@)  ")}},
        "•": {},
        "https://example.com/bakery": {"shouldTranslate": False},
    },
    "version": "1.0",
}
open(os.path.join(out, "xcstrings", "bakery.xcstrings"), "wb").write(json.dumps(bakery, indent=2, separators=(",", " : "), ensure_ascii=False).encode("utf-8"))

compact = {"sourceLanguage": "en", "strings": {"Bread/Rolls": {"comment": "menu"}, "Crème brûlée": {}, "Open 7/7": {"localizations": {"en": su("Open 7/7")}}}, "version": "1.0"}
text = json.dumps(compact, ensure_ascii=True, separators=(",", ":")).replace("/", chr(92) + "/")
open(os.path.join(out, "xcstrings", "compact_bom.xcstrings"), "wb").write(b"\xef\xbb\xbf" + text.encode("ascii") + b"\n")

crlf = json.dumps(bakery, indent=2, separators=(",", " : "), ensure_ascii=False).replace("\n", "\r\n")
open(os.path.join(out, "xcstrings", "bakery_crlf.xcstrings"), "wb").write(crlf.encode("utf-8"))

XLIFF = """<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <file original="Lumen/Localizable.xcstrings" source-language="en" target-language="fr" datatype="plaintext">
    <header>
      <tool tool-id="com.apple.dt.xcode" tool-name="Xcode" tool-version="15.0" build-num="15A240d"/>
    </header>
    <body>
      <trans-unit id="Add to basket" xml:space="preserve">
        <source>Add to basket</source>
        <target state="new">Add to basket</target>
        <note>Button on a product page</note>
      </trans-unit>
      <trans-unit id="%lld loaves" xml:space="preserve">
        <source>%lld loaves</source>
        <target state="new">%lld loaves</target>
      </trans-unit>
      <trans-unit id="order_summary" xml:space="preserve">
        <source>%1$@, your order has %2$lld items.</source>
        <target state="needs-translation">%1$@, your order has %2$lld items.</target>
        <note>%1$@ is the customer name</note>
      </trans-unit>
      <trans-unit id="rich" xml:space="preserve">
        <source>Fresh <g id="1">daily</g> from <x id="2"/> ovens</source>
      </trans-unit>
    </body>
  </file>
</xliff>
"""
XLIFF2 = """<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <file original="Lumen/InfoPlist.xcstrings" source-language="en" target-language="fr" datatype="plaintext">
    <body>
      <trans-unit id="CFBundleDisplayName" xml:space="preserve">
        <source>Lumen Bakery</source>
        <target state="new">Lumen Bakery</target>
      </trans-unit>
      <trans-unit id="NSCameraUsageDescription" xml:space="preserve">
        <source>Scan a loaf label to reorder it.</source>
        <target state="new">Scan a loaf label to reorder it.</target>
      </trans-unit>
    </body>
  </file>
</xliff>
"""
CONTENTS = """{
  "developmentRegion" : "en",
  "project" : "LumenBakery.xcodeproj",
  "targetLocale" : "fr",
  "toolInfo" : {
    "toolBuildNumber" : "15A240d",
    "toolID" : "com.apple.dt.xcode",
    "toolName" : "Xcode",
    "toolVersion" : "15.0"
  },
  "version" : "1.0"
}"""


def bundle(path, xliffs, prefix="fr.xcloc/"):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr(zipfile.ZipInfo(prefix, (2026, 10, 2, 9, 30, 0)), b"")
        z.writestr(zipfile.ZipInfo(prefix + "contents.json", (2026, 10, 2, 9, 30, 0)), CONTENTS, compress_type=zipfile.ZIP_DEFLATED)
        for name, body in xliffs:
            z.writestr(zipfile.ZipInfo(prefix + "Localized Contents/" + name, (2026, 10, 2, 9, 30, 0)), body, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr(zipfile.ZipInfo(prefix + "Source Contents/Lumen/Localizable.xcstrings", (2026, 10, 2, 9, 30, 0)), '{"sourceLanguage":"en","strings":{}}', compress_type=zipfile.ZIP_DEFLATED)
        z.writestr(zipfile.ZipInfo(prefix + "Notes/readme.txt", (2026, 10, 2, 9, 30, 0)), "Notes for translators")
    open(path, "wb").write(buf.getvalue())


bundle(os.path.join(out, "xcloc", "lumen_fr.xcloc"), [("fr.xliff", XLIFF)])
bundle(os.path.join(out, "xcloc", "lumen_two_files.xcloc"), [("fr.xliff", XLIFF), ("fr-info.xliff", XLIFF2)])
