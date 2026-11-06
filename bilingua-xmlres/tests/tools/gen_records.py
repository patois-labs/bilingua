"""Writes tests/data/locos_records.json: what Localization OS's `get_filter("xml").records(data)` (the
XmlResourceDispatch, app/filters/xml_resource.py) gives for a set of inputs and the two fixtures. Run with the LocOS
venv Python, read only:

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe -I tests/tools/gen_records.py
"""

import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="xmlres-records-")
sys.dont_write_bytecode = True
os.chdir(LOCOS)
sys.path.insert(0, LOCOS)

from app.filters import get_filter  # noqa: E402

F = get_filter("xml")

A = '<?xml version="1.0" encoding="utf-8"?>\n<resources xmlns:xliff="urn:oasis:names:tc:xliff:document:1.2">\n'
Z = "\n</resources>\n"
J = '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE properties SYSTEM "http://java.sun.com/dtd/properties.dtd">\n<properties>\n'
JZ = "\n</properties>\n"

CASES = {
    "ws_and_escapes": A + '<string name="a">  Hello \\\'x\\\'  </string>\n<string name="b">  @string/x</string>\n<string name="c"> ?attr/y </string>' + Z,
    "ref_with_child": A + '<string name="a">@string/a<!-- c --></string>\n<string name="b">@x <b>bold</b></string>\n<string name="c"><b>@x</b></string>' + Z,
    "only_tokens": A + '<string name="a"><xliff:g id="n">%1$s</xliff:g></string>\n<string name="b"><b></b></string>\n<string name="c">%s</string>\n<string name="d">--</string>\n<string name="e">   </string>\n<string name="f">\\n</string>' + Z,
    "comments": A + '<!-- one -->\n<!-- two -->\n<string name="a">A</string>\n<string name="b">B</string>\n<!--   spaced   -->\n<color name="c">#fff</color>\n<string name="d">D</string>\n<!-- arr -->\n<string-array name="e"><item>x</item><!-- in --><item>y</item></string-array>\n<!-- pl -->\n<plurals name="p" translatable="false"><item quantity="one">1</item><item>noq</item></plurals>' + Z,
    "quotes_cdata": A + '<string name="a">"  quoted with \\"inner\\"  "</string>\n<string name="b"><![CDATA["cdata quoted"]]></string>\n<string name="c">a &amp; b &lt;c&gt;</string>\n<string name="d">tab\\tand\\u0041</string>\n<string name="e">"a" "b"</string>' + Z,
    "missing_name": A + '<string>no name</string>\n<plurals><item quantity="other">x</item></plurals>\n<string name="dup">1</string><string name="dup">2</string>' + Z,
    "java": J + '<comment>c</comment>\n<entry key="a">It\'\'s {0}</entry>\n<entry key="b">  spaced  </entry>\n<entry key="c"></entry>\n<entry key="d">@ref</entry>\n<entry>nokey</entry>\n<entry key="e"><![CDATA[<b>x</b>]]></entry>\n<entry key="f">it\'s</entry>' + JZ,
}


def rows(data: bytes):
    return [
        {k: getattr(r, k) for k in ("key", "value", "comment", "plural", "index", "translatable")}
        for r in F.records(data)
    ]


def main():
    out = {}
    for name, text in CASES.items():
        out[name] = {"input": text, "records": rows(text.encode("utf-8"))}
    for fx in ("strings.xml", "messages.xml"):
        out[fx] = {"file": fx, "records": rows((DATA / fx).read_bytes())}
    (DATA / "locos_records.json").write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
