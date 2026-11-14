"""Writes tests/data/locos_golden.json: what the Localization OS `.xml` resource filters (Android strings and Java
XML properties, app/filters/xml_resource.py) give for a set of inputs. Run with the LocOS venv Python, read only:

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe tests/tools/gen_golden.py

Per case: the unit texts LocOS shows (with its ⟦g1⟧ ⟦x1⟧ tokens), the translation used for each unit, the build
with those translations, and for Android the localized builds for pl and ja (plural fill).
"""

import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="xmlres-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app.filters.base import get_filter  # noqa: E402

A = '<?xml version="1.0" encoding="utf-8"?>\n<resources xmlns:xliff="urn:oasis:names:tc:xliff:document:1.2">\n'
Z = "</resources>\n"
J = '<?xml version="1.0" encoding="UTF-8" standalone="no"?>\n<!DOCTYPE properties SYSTEM "http://java.sun.com/dtd/properties.dtd">\n<properties>\n'

CASES = {
    "strings.xml": (DATA / "strings.xml").read_bytes(),
    "messages.xml": (DATA / "messages.xml").read_bytes(),
    "breaks_706.xml": (
        b'<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
        b'    <string name="two">Your download is complete.\\nOpen the file.</string>\n'
        b'    <string name="kept">One\\nTwo</string>\n'
        b'    <string name="plain">No break here.</string>\n'
        b'    <plurals name="left">\n'
        b'        <item quantity="one">%d file left.\\nKeep going.</item>\n'
        b'        <item quantity="other">%d files left.\\nKeep going.</item>\n'
        b"    </plurals>\n</resources>\n"
    ),
    "crlf_plurals.xml": (
        A + '  <plurals name="apples">\n    <item quantity="one">%d apple</item>\n    <item quantity="other">%d apples</item>\n'
        '  </plurals>\n  <plurals name="off" translatable="false">\n    <item quantity="one">x</item>\n'
        '    <item quantity="other">xs</item>\n  </plurals>\n'
        '  <plurals name="inline"><item quantity="one">one</item><item quantity="two">two</item><item quantity="other">many</item></plurals>\n'
        + Z
    ).replace("\n", "\r\n").encode("utf-8"),
    "inline.xml": (
        A
        + '    <string name="nested">Use <b>bold <i>and italic</i></b> here.</string>\n'
        + '    <string name="br">Line<br/>break</string>\n'
        + '    <string name="lead_ph"><xliff:g id="n">%1$d</xliff:g> items left</string>\n'
        + '    <string name="comment">Before <!-- a note --> after</string>\n'
        + '    <string name="only_tag"><b> </b></string>\n'
        + '    <string name="link">Read <a href="https://example.com/?a=1&amp;b=2">the terms</a>.</string>\n'
        + Z
    ).encode("utf-8"),
    "escapes.xml": (
        A
        + '    <string name="amp">Fish &amp; chips &lt;b&gt; &#233;t&#xE9;</string>\n'
        + '    <string name="pad">   padded value   </string>\n'
        + '    <string name="quoted_pad">"  quoted, with spaces  "</string>\n'
        + '    <string name="q_cdata"><![CDATA["Don\'t stop"]]></string>\n'
        + '    <string name="at">\\@start and \\?q</string>\n'
        + '    <string name="tail_bs">ends with a backslash \\\\</string>\n'
        + '    <string name="unicode">\\u00e9 stays as typed \\t tab</string>\n'
        + '    <string name="ref2">@android:string/ok</string>\n'
        + '    <string name="digits">12345</string>\n'
        + '    <string name="multi">first line\n        second line</string>\n'
        + Z
    ).encode("utf-8"),
    "arrays.xml": (
        A
        + "    <!-- Planet names -->\n"
        + '    <string-array name="planets"><item>Mercury</item><item>Venus</item><item>@string/earth</item></string-array>\n'
        + '    <string-array name="codes" translatable="false"><item>MRC</item></string-array>\n'
        + '    <string name="after">After the arrays</string>\n'
        + Z
    ).encode("utf-8"),
    "ja_utf8_bom.xml": ("\ufeff" + A + '    <string name="hi">こんにちは、%1$s さん</string>\n' + Z).encode("utf-8"),
    "java_more.xml": (
        J
        + '<entry key="plain">Don\'t worry</entry>\n'
        + '<entry key="args">{0} can\'\'t open {1}</entry>\n'
        + '<entry key="cdata"><![CDATA[Save & <close>]]></entry>\n'
        + '<entry key="empty"></entry>\n'
        + '<entry key="pad">  spaced  </entry>\n'
        + "</properties>\n"
    ).encode("utf-8"),
}


def tr_for(text: str) -> str:
    """The translation used for a unit: the LocOS text with characters that need escaping around it."""
    return "Zq'\"& " + text + " <ok>"


def main():
    f = get_filter("xml")
    out = {}
    for name, data in CASES.items():
        pr = f.parse(data)
        units = [{"i": u.p_index, "text": u.text} for u in pr.units]
        tr = {str(u["i"]): tr_for(u["text"]) for u in units}
        case = {
            "input": data.decode("utf-8"),
            "units": [u["text"] for u in units],
            "translations": [tr_for(u["text"]) for u in units],
            "built": f.build(data, tr).decode("utf-8"),
            "identity": f.build(data, {}) == data,
        }
        root = data.lstrip(b"\xef\xbb\xbf")
        if b"<resources" in root:
            case["built_pl"] = f.build_localized(data, tr, "pl").decode("utf-8")
            case["built_ja"] = f.build_localized(data, tr, "ja").decode("utf-8")
        out[name] = case
    (DATA / "locos_golden.json").write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {len(out)} cases")


main()
