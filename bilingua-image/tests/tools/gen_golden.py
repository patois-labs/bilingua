r"""Makes tests/data/locos_golden.json: what the LocOS image filter does with ~700 SVG files and ~150 PNG files.

Run with the LocOS venv (read only use of the LocOS tree):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json

SVG case: {"kind": "svg", "name", "hex", "units": [[text, style]...] | "error", "builds": {label: digest},
"localized": {locale|label: digest}}.  PNG case: {"kind": "png", "name", "hex", "units": [text...] | "error"}.
A digest is "len:fnv64" of the output bytes, or "ERR:..." when LocOS refuses.
"""

import io
import json
import os
import random
import re
import struct
import sys
import tempfile
import zlib

LOCOS = os.environ.get("LOCOS_DIR") or r"C:\Users\brent\code\local-tms"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-golden-")
os.environ["LOC_TMS_IMAGE_DETECTOR"] = "mock"
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
os.chdir(LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.db import init_db  # noqa: E402

init_db()
from app.filters.base import get_filter  # noqa: E402
from PIL import Image  # noqa: E402
from PIL.PngImagePlugin import PngInfo  # noqa: E402

FILT = get_filter("image")
rnd = random.Random(20261105)


def fnv(b: bytes) -> str:
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{len(b)}:{h:016x}"


WORDS = ["Hello", "World", "Sale", "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\u041f\u0440\u0438\u0432\u0435\u0442",
         "\ud55c\uad6d\uc5b4", "\u4e2d\u6587", "na\u00efve", "42", "Q1", "a&amp;b", "x &lt; y", "&quot;quoted&quot;", "&#169; 2024",
         "&#x41;", "\u03b1\u03b2\u03b3", "\u05e9\u05dc\u05d5\u05dd", "price: 5 &gt; 3", "&apos;tick&apos;", "\u0e2a\u0e27\u0e31\u0e2a\u0e14\u0e35"]
FILLERS = ["", " ", "\n", "\n  ", "\t", "\r\n"]


def words(n=None):
    n = n or rnd.randint(1, 4)
    return " ".join(rnd.choice(WORDS) for _ in range(n))


def text_inner(depth=0):
    out = [rnd.choice(FILLERS) + words() + rnd.choice(FILLERS)]
    if depth < 2 and rnd.random() < 0.5:
        for _ in range(rnd.randint(1, 3)):
            tag = rnd.choice(["tspan", "tspan", "textPath", "TSPAN"])
            attrs = rnd.choice(["", ' x="1" dy="2"', ' text-anchor="start"', ' text-anchor="end"', ' style="text-anchor:middle"', " "])
            out.append(f"<{tag}{attrs}>{text_inner(depth + 1)}</{tag}>")
            if rnd.random() < 0.5:
                out.append(words(1))
    return "".join(out)


TEXT_ATTRS = ["", ' x="10" y="20"', ' text-anchor="middle"', ' style="font-size:12px; text-anchor: end"', " style='text-anchor:start;fill:red'",
              ' direction="ltr"', ' text-anchor = "end"', ' class="t" text-anchor=end', ' STYLE="TEXT-ANCHOR: END"', ' style=""',
              ' style="text-anchor:"', ' style="text-anchor: end !important"', ' aria-label="a>b"'.replace(">", "&gt;")]


def element(depth=0):
    r = rnd.random()
    if r < 0.45:
        a = rnd.choice(TEXT_ATTRS)
        if rnd.random() < 0.08:
            return f"<text{a}/>"
        return f"<text{a}>{text_inner()}</text>"
    if r < 0.55:
        return f"<title>{words()}</title>" if rnd.random() < 0.8 else "<title>Chart <tspan>x</tspan></title>"
    if r < 0.62:
        return f"<desc>{rnd.choice(FILLERS)}{words()}{rnd.choice(FILLERS)}</desc>"
    if r < 0.68:
        return rnd.choice(["<style>.t { font-family: sans-serif; }</style>", "<script>var a = 1 < 2;</script>",
                           "<metadata><title>Not announced</title></metadata>", "<defs><text>In defs</text></defs>",
                           "<defs><title>T</title></defs>", "<style><title>x</title></style>"])
    if r < 0.76:
        return rnd.choice(["<rect x=\"1\" y=\"2\" width=\"3\" height=\"4\"/>", "<path d=\"M0 0 L10 10\"/>", "<!-- note -->", "<!-- a<b -->",
                           "<![CDATA[ data ]]>", "<?pi target?>", "<circle r=\"5\" />", "<image href=\"a.png\"/>"])
    if r < 0.9 and depth < 3:
        a = rnd.choice(["", ' text-anchor="end"', ' transform="rotate(30)"', ' style="text-anchor:middle"'])
        return f"<g{a}>" + "".join(element(depth + 1) for _ in range(rnd.randint(1, 4))) + "</g>"
    return rnd.choice(["<foreignObject><p>Hello html</p></foreignObject>", "<a href=\"x\"><text>Link text</text></a>", "<switch><text>Sw</text></switch>"])


def svg_doc():
    root_attrs = rnd.choice(
        [' xmlns="http://www.w3.org/2000/svg"', ' xmlns="http://www.w3.org/2000/svg" xml:lang="en"', ' lang="en" xmlns="http://www.w3.org/2000/svg"',
         ' xml:lang="en" lang="en"', "", ' aria-label="set lang=en" data-n=\' xml:lang=x\' xml:lang="en"', ' xmlns="http://www.w3.org/2000/svg" lang',
         ' xml:lang = "en"', " xml:lang=en", ' LANG="x"'])
    body = "".join(rnd.choice(FILLERS) + element() for _ in range(rnd.randint(1, 9)))
    tag = rnd.choice(["svg", "svg", "svg", "SVG"])
    pre = rnd.choice(["", '<?xml version="1.0" encoding="UTF-8"?>\n', '<?xml version="1.0"?>\n<!-- c -->\n',
                      '<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">\n', "\n  "])
    return f"{pre}<{tag}{root_attrs}>{body}</{tag}>{rnd.choice(['', chr(10), chr(13) + chr(10)])}"


def encode(doc: str):
    k = rnd.random()
    if k < 0.55:
        return doc.encode("utf-8")
    if k < 0.65:
        return b"\xef\xbb\xbf" + doc.encode("utf-8")
    if k < 0.72:
        return doc.encode("utf-16")  # BOM
    if k < 0.78:
        return doc.encode("utf-16-be") if False else b"\xfe\xff" + doc.encode("utf-16-be")
    if k < 0.9:
        # a declared legacy charset the text fits in
        label, codec = rnd.choice([("windows-1252", "cp1252"), ("iso-8859-1", "latin-1"), ("windows-1251", "cp1251"), ("shift_jis", "shift_jis"),
                                   ("gbk", "gbk"), ("euc-kr", "euc-kr"), ("windows-1256", "cp1256")])
        d = doc.replace('encoding="UTF-8"', f'encoding="{label}"')
        if "encoding=" not in d:
            d = f'<?xml version="1.0" encoding="{label}"?>\n' + d
        try:
            return d.encode(codec)
        except UnicodeEncodeError:
            return d.encode("ascii", "xmlcharrefreplace")
    return doc.encode("cp1252", "replace")


def mutate(b: bytes) -> bytes:
    b = bytearray(b)
    for _ in range(rnd.randint(1, 3)):
        k = rnd.random()
        if not b:
            break
        i = rnd.randrange(len(b))
        if k < 0.3:
            del b[i : i + rnd.randint(1, 20)]
        elif k < 0.6:
            b[i] = rnd.randrange(256)
        elif k < 0.8:
            b[i:i] = rnd.choice([b"<", b">", b"<>", b"&", b"&#", b'"', b"<!--", b"]]>"])
        else:
            b = b[:i]
    return bytes(b)


MAPS = {
    "all": lambda i, t: "FR " + t,
    "third": lambda i, t: ("Zeta %d \u65e5\u672c" % i) if i % 3 == 0 else None,
    "plain": lambda i, t: "Plain%d" % i,
    "cjk": lambda i, t: "\u65e5\u672c\u8a9e%d" % i,
    "ar": lambda i, t: "\u0645\u0631\u062d\u0628\u0627 %d" % i,
    "sym": lambda i, t: 'A & "B" <c> \'d\' ]]> %s' % t,
    "pad": lambda i, t: "  " + t + "  ",
}
LOCALES = ["de-DE", "ar", "he-IL", "ja-JP", "fa_IR", "ku-Arab", "ug-Latn", "zh-Hant-TW", "", "!!", "x" * 70, "ar-u-nu-latn"]


def svg_case(name, data, mapped=True):
    from app.services.image_localization import sniff_kind

    c = {"kind": "svg", "name": name, "hex": data.hex(), "sniff_svg": sniff_kind(data) == "svg"}

    probe = data.decode("utf-16", "replace") if data[:2] in (bytes([255, 254]), bytes([254, 255])) else data.decode("latin-1")
    m = re.search(r"""<[?]xml[^>]{0,200}?(encoding\s*=\s*(["'])([^"']+)""" + chr(92) + """2)""", probe[:400])
    decl = m.group(1) if m and m.group(3).lower().replace("_", "-") not in ("utf-8", "utf8") else ""
    c["decl"] = decl
    try:
        res = FILT._parse_svg(data)
    except Exception as e:  # noqa: BLE001
        c["units"] = "error"
        c["error"] = f"{type(e).__name__}: {str(e)[:120]}"
        return c
    c["units"] = [[u.text, u.style or ""] for u in res.units]
    if not mapped:
        return c
    c["builds"], c["localized"] = {}, {}
    for label, fn in MAPS.items():
        mapping = {}
        for i, u in enumerate(res.units):
            v = fn(i, u.text)
            if v is not None:
                mapping[str(i)] = v
        try:
            if c["sniff_svg"]:
                c["builds"][label] = fnv(FILT.build(data, mapping))  # the public call, with the untouched-bytes guard
            else:
                c["builds"][label] = fnv(FILT._build_svg(data, mapping) if mapping else data)
        except Exception as e:  # noqa: BLE001
            c["builds"][label] = "ERR:" + type(e).__name__
        if label in ("all", "third", "ar", "sym"):
            for loc in LOCALES:
                try:
                    c["localized"][f"{loc}|{label}"] = fnv(FILT.build_localized(data, mapping, loc))
                except Exception as e:  # noqa: BLE001
                    c["localized"][f"{loc}|{label}"] = "ERR:" + type(e).__name__
    return c


def chunk(ty, data):
    c = struct.pack(">I", len(data)) + ty + data
    return c + struct.pack(">I", zlib.crc32(ty + data) & 0xFFFFFFFF)


def png_with(regions_json, size=(60, 40), kind="tEXt", extra=None):
    im = Image.new("RGB", size, (240, 240, 240))
    meta = PngInfo()
    if regions_json is not None:
        if kind == "tEXt":
            meta.add_text("loc_tms_regions", regions_json)
        elif kind == "zTXt":
            meta.add_text("loc_tms_regions", regions_json, zip=True)
        elif kind == "iTXt":
            meta.add_itxt("loc_tms_regions", regions_json, lang="en", tkey="regions")
    for k, v in (extra or []):
        meta.add_text(k, v)
    buf = io.BytesIO()
    im.save(buf, format="PNG", pnginfo=meta)
    return buf.getvalue()


def png_case(name, data):
    c = {"kind": "png", "name": name, "hex": data.hex()}
    try:
        res = FILT._parse_raster(data, None)
        c["units"] = [u.text for u in res.units]
        c["geom"] = [list(u.inline_map["image_region"]["bbox_axis_aligned"]) + [u.inline_map["image_region"]["polygon"]] for u in res.units]
    except Exception as e:  # noqa: BLE001
        c["units"] = "error"
        c["error"] = f"{type(e).__name__}: {str(e)[:120]}"
    return c


def rows(n):
    out = []
    for i in range(n):
        out.append({"text": words(rnd.randint(1, 3)).replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", '"').replace("&#169;", "\u00a9").replace("&#x41;", "A").replace("&apos;", "'"),
                    "bbox": [rnd.randint(0, 50), rnd.randint(0, 30), rnd.randint(1, 40), rnd.randint(1, 20)], "confidence": rnd.random()})
    return out


def main():
    cases = []
    for i in range(520):
        data = encode(svg_doc())
        cases.append(svg_case(f"svg{i:03d}", data))
    for i in range(120):
        base = encode(svg_doc())
        cases.append(svg_case(f"svgmut{i:03d}", mutate(base), mapped=True))
    # the LocOS suites' own SVGs
    suite = {
        "banner": b'<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="300" height="120">\n  <defs><linearGradient id="g"><stop offset="0" stop-color="#347"/></linearGradient></defs>\n  <rect x="2" y="2" width="296" height="116" fill="url(#g)"/>\n  <style>.t { font-family: sans-serif; }</style>\n  <text class="t" x="16" y="40">Summer Sale &amp; more</text>\n  <text class="t" x="16" y="80"><tspan>Shop now</tspan></text>\n  <!-- decorative comment stays byte-identical -->\n</svg>\n',
        "entities": b'<svg xmlns="http://www.w3.org/2000/svg"><text>&#169; 2024 &quot;Acme&quot; &#x41; &amp; Co</text></svg>',
        "rtl": ('<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200"><text id="a" x="10" y="20">Left label</text><text id="b" x="200" y="40" text-anchor="middle">Centered</text><text id="c" x="390" y="60" style="font-size:12px; text-anchor: end">Right label</text><g text-anchor="end"><text id="d" x="390" y="80">Inherited end</text></g><text id="e" x="10" y="100"><tspan x="10" text-anchor="start">First line</tspan><tspan x="10" dy="14">Second line</tspan></text><text id="f" x="10" y="140" direction="ltr">Explicit ltr</text><text id="g" x="10" y="160">Not translated</text></svg>\n').encode(),
        "rtl2": b'<svg xmlns="http://www.w3.org/2000/svg"><text id="z" x="1" y="1"/><text id="a" x="10" y="20">Hello</text><text id="m" x="50" y="40" transform="rotate(30 50 40)" text-anchor="middle"><tspan x="50">One</tspan><tspan x="50" dy="12" text-anchor="start">Two</tspan></text></svg>',
        "lang1": b'<?xml version="1.0"?>\n<!-- c -->\n<svg xmlns="http://www.w3.org/2000/svg" xml:lang="en"><text x="1" y="2">Hello</text><text xml:lang="la">Lorem</text></svg>',
        "lang2": b'<svg xmlns="http://www.w3.org/2000/svg"><text>Hi</text></svg>',
        "lang3": b"<svg lang='en' xmlns=\"http://www.w3.org/2000/svg\"><text>Hi</text></svg>",
        "lang4": b'<svg xml:lang="en" lang="en"><text>Hi</text></svg>',
        "lang5": b'<svg aria-label="set lang=en" data-n=\' xml:lang=x\' xml:lang="en"><text>Hi</text></svg>',
        "a11y": b'<svg xmlns="http://www.w3.org/2000/svg"><title>Sales &amp; returns</title><desc> Bars per quarter </desc><metadata><title>Not announced</title></metadata><text>Q1 <tspan>bar</tspan></text><title>Q1 <tspan>bar</tspan></title><text>Q2 total</text></svg>',
        "hostile_split": ("<svg><text>" + "<a" * 3000).encode(),
        "split_sample": b'<svg x="1"><text>Hi <tspan>there</tspan> a < b</text><!-- c<d --><></svg>',
        "hello": b"<svg><text>Hello SVG</text></svg>",
        "empty": b"<svg/>",
        "notext": b'<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>',
    }
    for k, v in suite.items():
        cases.append(svg_case("suite_" + k, v))
    # PNG sidecars
    for i in range(60):
        r = rows(rnd.randint(1, 8))
        cases.append(png_case(f"png_text{i:02d}", png_with(json.dumps(r, ensure_ascii=True))))
    for i in range(20):
        r = rows(rnd.randint(1, 5))
        cases.append(png_case(f"png_itxt{i:02d}", png_with(json.dumps(r, ensure_ascii=False), kind="iTXt")))
    for i in range(15):
        r = rows(rnd.randint(1, 5))
        cases.append(png_case(f"png_ztxt{i:02d}", png_with(json.dumps(r, ensure_ascii=True), kind="zTXt")))
    odd = {
        "none": png_with(None),
        "notjson": png_with("not json"),
        "notlist": png_with('{"text": "a", "bbox": [1,2,3,4]}'),
        "emptylist": png_with("[]"),
        "badrows": png_with('[1, "x", null, {"text": "", "bbox": [1,2,3,4]}, {"text": "a"}, {"text": "b", "bbox": [1,2,3]}, {"text": "c", "bbox": ["a",2,3,4]}, {"text": "ok", "bbox": ["1","2",3.5,true]}]'),
        "strbbox": png_with('[{"text": "ok", "bbox": [1.5, 2.5, 3.5, 4.5]}, {"text": "ok2", "bbox": [0.5, 1.5, 2.5, 3.5]}]'),
        "poly": png_with('[{"text": "p", "bbox": [1,2,3,4], "polygon": [[0,0],[5,0],[5,5],[0,5]]}, {"text": "q", "bbox": [1,3,3,4], "polygon": []}, {"text": "r", "bbox": [1,4,3,4], "polygon": [[1,2,3]]}, {"text": "s", "bbox": [1,5,3,4], "polygon": "ab"}]'),
        "conf": png_with('[{"text": "a", "bbox": [1,2,3,4], "confidence": null}, {"text": "b", "bbox": [1,3,3,4], "confidence": "0.5"}, {"text": "c", "bbox": [1,4,3,4], "angle_deg": "x"}, {"text": "d", "bbox": [1,5,3,4], "angle_deg": 45}]'),
        "nan": png_with('[{"text": "a", "bbox": [NaN,2,3,4]}, {"text": "b", "bbox": [1,3,3,4]}]'),
        "inf": png_with('[{"text": "a", "bbox": [Infinity,2,3,4]}]'),
        "nonstr": png_with('[{"text": 5, "bbox": [1,2,3,4]}, {"text": "s", "bbox": [1,3,3,4]}]'),
        "dup": png_with('[{"text": "first", "bbox": [1,2,3,4]}]', extra=[("loc_tms_regions", '[{"text": "second", "bbox": [1,2,3,4]}]')]),
        "order": png_with('[{"text": "c", "bbox": [5,9,1,1]}, {"text": "b", "bbox": [9,1,1,1]}, {"text": "a", "bbox": [1,1,1,1]}, {"text": "d", "bbox": [1,9,1,1]}]'),
        "many": png_with(json.dumps([{"text": "t%d" % i, "bbox": [i % 50, i // 5, 3, 3]} for i in range(130)])),
        "wide": png_with('[{"text": "w", "bbox": [1,2,3,4]}]', size=(9000, 4)),
        "huge": png_with('[{"text": "w", "bbox": [1,2,3,4]}]', size=(5001, 5001)) if False else None,
        "tabs": png_with(json.dumps([{"text": "  spaced \n text  ", "bbox": [1, 2, 3, 4]}])),
    }
    for k, v in odd.items():
        if v is not None:
            cases.append(png_case("png_" + k, v))
    base = png_with('[{"text": "a", "bbox": [1,2,3,4]}]')
    cases.append(png_case("png_badcrc", base[:40] + bytes([base[40] ^ 1]) + base[41:]))
    cases.append(png_case("png_trunc", base[:-20]))
    cases.append(png_case("png_trunc_idat", base[:60]))
    cases.append(png_case("png_noiend", base[:-12]))
    # an animated PNG (acTL with 2 frames) built by hand around a valid still
    def with_actl(png, n):
        sig, rest = png[:8], png[8:]
        ihdr_len = struct.unpack(">I", rest[:4])[0] + 12
        return sig + rest[:ihdr_len] + chunk(b"acTL", struct.pack(">II", n, 0)) + rest[ihdr_len:]

    cases.append(png_case("png_apng2", with_actl(base, 2)))
    cases.append(png_case("png_apng1", with_actl(base, 1)))
    cases.append(png_case("png_apng0", with_actl(base, 0)))
    # other rasters: LocOS parses them to zero regions (mock detector) or refuses
    for fmt in ("JPEG", "GIF", "BMP", "TIFF", "WEBP"):
        b = io.BytesIO()
        Image.new("RGB", (30, 20), (200, 10, 10)).save(b, format=fmt)
        cases.append(png_case("raster_" + fmt.lower(), b.getvalue()))
    for fmt in ("GIF", "WEBP", "TIFF"):
        frames = [Image.new("RGB", (30, 20), c) for c in ((255, 0, 0), (0, 0, 255))]
        b = io.BytesIO()
        frames[0].save(b, format=fmt, save_all=True, append_images=frames[1:])
        cases.append(png_case("raster_multi_" + fmt.lower(), b.getvalue()))
    cases.append(png_case("raster_garbage", b"\x00\x01\x02hello"))
    with open(sys.argv[1], "w", encoding="utf-8") as fh:
        json.dump(cases, fh, ensure_ascii=True)
    print(len(cases), "cases")


main()
