r"""Makes the oracle corpus of the image crate: bilingua/oracle/corpus/svg/*.svg and bilingua/oracle/corpus/image/*.png.

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe gen_corpus.py OUT_ROOT     (OUT_ROOT = .../bilingua/oracle/corpus)

Pillow (from the LocOS venv, read only) draws the PNGs and embeds the region sidecar the way LocOS's own tests do
(`loc_tms_regions`). Everything else is plain bytes written here. Files are small and made up; the suite SVGs are the
LocOS test fixtures (Apache 2.0).
"""

import io
import json
import os
import sys

from PIL import Image, ImageDraw
from PIL.PngImagePlugin import PngInfo

root = sys.argv[1]
svg_dir = os.path.join(root, "svg")
img_dir = os.path.join(root, "image")
os.makedirs(svg_dir, exist_ok=True)
os.makedirs(img_dir, exist_ok=True)

NS = 'xmlns="http://www.w3.org/2000/svg"'


def put(name, data):
    path = os.path.join(svg_dir if name.endswith(".svg") else img_dir, name)
    with open(path, "wb") as fh:
        fh.write(data if isinstance(data, bytes) else data.encode("utf-8"))


put("banner.svg", f"""<?xml version="1.0" encoding="UTF-8"?>
<svg {NS} width="300" height="120">
  <defs><linearGradient id="g"><stop offset="0" stop-color="#347"/></linearGradient></defs>
  <rect x="2" y="2" width="296" height="116" fill="url(#g)"/>
  <style>.t {{ font-family: sans-serif; }}</style>
  <text class="t" x="16" y="40">Summer Sale &amp; more</text>
  <text class="t" x="16" y="80"><tspan>Shop now</tspan></text>
  <!-- decorative comment stays byte-identical -->
</svg>
""")
put("entities.svg", f'<svg {NS}><text>&#169; 2024 &quot;Acme&quot; &#x41; &amp; Co</text></svg>')
put("rtl.svg", f"""<?xml version="1.0" encoding="UTF-8"?>
<svg {NS} width="400" height="200"><text id="a" x="10" y="20">Left label</text><text id="b" x="200" y="40" text-anchor="middle">Centered</text><text id="c" x="390" y="60" style="font-size:12px; text-anchor: end">Right label</text><g text-anchor="end"><text id="d" x="390" y="80">Inherited end</text></g><text id="e" x="10" y="100"><tspan x="10" text-anchor="start">First line</tspan><tspan x="10" dy="14">Second line</tspan></text><text id="f" x="10" y="140" direction="ltr">Explicit ltr</text><text id="g" x="10" y="160">Not translated</text></svg>
""")
put("rtl_edge.svg", f'<svg {NS}><text id="z" x="1" y="1"/><text id="a" x="10" y="20">Hello</text><text id="m" x="50" y="40" transform="rotate(30 50 40)" text-anchor="middle"><tspan x="50">One</tspan><tspan x="50" dy="12" text-anchor="start">Two</tspan></text></svg>')
put("lang_root.svg", f'<?xml version="1.0"?>\n<!-- c -->\n<svg {NS} xml:lang="en"><text x="1" y="2">Hello</text><text xml:lang="la">Lorem</text></svg>')
put("lang_plain.svg", "<svg lang='en' xmlns=\"http://www.w3.org/2000/svg\"><text>Hi</text></svg>")
put("lang_both.svg", '<svg xml:lang="en" lang="en"><text>Hi</text></svg>')
put("lang_tricky.svg", '<svg aria-label="set lang=en" data-n=\' xml:lang=x\' xml:lang="en"><text>Hi</text></svg>')
put("a11y.svg", f'<svg {NS}><title>Sales &amp; returns</title><desc> Bars per quarter </desc><metadata><title>Not announced</title></metadata><text>Q1 <tspan>bar</tspan></text><title>Q1 <tspan>bar</tspan></title><text>Q2 total</text></svg>')
put("chart.svg", f"""<?xml version="1.0" encoding="UTF-8"?>
<svg {NS} viewBox="0 0 400 220" role="img" aria-labelledby="t d">
  <title id="t">Quarterly revenue</title>
  <desc id="d">Bar chart of revenue per quarter in thousands of dollars</desc>
  <style>text {{ font: 12px sans-serif; }} .axis {{ stroke: #999; }}</style>
  <line class="axis" x1="40" y1="180" x2="380" y2="180"/>
  <rect x="60" y="80" width="40" height="100" fill="#4a7"/><text x="60" y="198">Q1</text>
  <rect x="140" y="60" width="40" height="120" fill="#4a7"/><text x="140" y="198">Q2</text>
  <rect x="220" y="40" width="40" height="140" fill="#4a7"/><text x="220" y="198">Q3</text>
  <rect x="300" y="20" width="40" height="160" fill="#4a7"/><text x="300" y="198">Q4</text>
  <text x="200" y="14" text-anchor="middle">Revenue grew <tspan font-weight="bold">every</tspan> quarter</text>
  <text x="4" y="110">Sales (k$)</text>
</svg>
""")
put("edge.svg", f"""<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg {NS} xmlns:xlink="http://www.w3.org/1999/xlink" width="200" height="100">
  <?editor-note keep me?>
  <!-- a < b > c: a comment with markup characters -->
  <script type="text/javascript"><![CDATA[ if (a < b && c > d) {{ document.title = "x"; }} ]]></script>
  <style>/* <text>not text</text> */ text {{ fill: red; }}</style>
  <defs><text id="in-defs">Defined text</text></defs>
  <foreignObject width="100" height="50"><p xmlns="http://www.w3.org/1999/xhtml">HTML inside</p></foreignObject>
  <a xlink:href="https://example.com/?a=1&amp;b=2"><text x="5" y="20" title="a&gt;b">Link text</text></a>
  <text x="5" y="40"><![CDATA[CDATA text is not translated]]></text>
  <text x="5" y="60">   padded   text   </text>
  <text x="5" y="80">&lt;b&gt; escaped &amp; kept</text>
</svg>
""")

LANGS = {
    "de": "Gr\u00fc\u00dfe aus K\u00f6ln",
    "ar": "\u0645\u0631\u062d\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645",
    "he": "\u05e9\u05dc\u05d5\u05dd \u05e2\u05d5\u05dc\u05dd",
    "ja": "\u3053\u3093\u306b\u3061\u306f\u4e16\u754c",
    "zh": "\u4f60\u597d\uff0c\u4e16\u754c",
    "ko": "\uc548\ub155\ud558\uc138\uc694 \uc138\uacc4",
    "ru": "\u041f\u0440\u0438\u0432\u0435\u0442, \u043c\u0438\u0440",
    "th": "\u0e2a\u0e27\u0e31\u0e2a\u0e14\u0e35\u0e0a\u0e32\u0e27\u0e42\u0e25\u0e01",
    "el": "\u0393\u03b5\u03b9\u03b1 \u03c3\u03bf\u03c5 \u03ba\u03cc\u03c3\u03bc\u03b5",
}
for code, text in LANGS.items():
    put(f"lang_{code}.svg", f'<svg {NS} xml:lang="{code}"><title>{text}</title><text x="4" y="20">{text}</text><text x="4" y="40"><tspan>{text}</tspan> 1</text></svg>')

base = f'<?xml version="1.0" encoding="{{enc}}"?>\n<svg {NS}><text x="4" y="20">{{t}}</text><text x="4" y="40">Plain &amp; simple</text></svg>\n'
put("utf8_bom.svg", b"\xef\xbb\xbf" + base.format(enc="UTF-8", t=LANGS["de"]).encode("utf-8"))
put("utf16le.svg", base.format(enc="UTF-16", t=LANGS["de"]).encode("utf-16"))
put("utf16be.svg", b"\xfe\xff" + base.format(enc="UTF-16", t=LANGS["de"]).encode("utf-16-be"))
put("cp1252.svg", base.format(enc="windows-1252", t="caf\u00e9 cr\u00e8me").encode("cp1252"))
put("latin1.svg", base.format(enc="ISO-8859-1", t="Gr\u00fc\u00dfe").encode("latin-1"))
put("cp1251.svg", base.format(enc="windows-1251", t=LANGS["ru"]).encode("cp1251"))
put("sjis.svg", base.format(enc="Shift_JIS", t=LANGS["ja"]).encode("shift_jis"))
put("gbk.svg", base.format(enc="GBK", t=LANGS["zh"].replace("\uff0c", ",")).encode("gbk"))
put("euckr.svg", base.format(enc="EUC-KR", t=LANGS["ko"]).encode("euc-kr"))
put("cp1256.svg", base.format(enc="windows-1256", t=LANGS["ar"]).encode("cp1256"))
put("crlf.svg", base.format(enc="UTF-8", t="Line").replace("\n", "\r\n").encode("utf-8"))

# raster images with the LocOS region sidecar


def png(regions, name, kind="tEXt", size=(220, 110), draw=True):
    im = Image.new("RGB", size, (238, 240, 244))
    if draw:
        d = ImageDraw.Draw(im)
        for r in regions:
            x, y, w, h = r["bbox"]
            d.rectangle([x, y, x + w, y + h], outline=(120, 120, 120))
    meta = PngInfo()
    if regions is not None:
        j = json.dumps(regions, ensure_ascii=(kind != "iTXt"))
        if kind == "tEXt":
            meta.add_text("loc_tms_regions", j)
        elif kind == "zTXt":
            meta.add_text("loc_tms_regions", j, zip=True)
        else:
            meta.add_itxt("loc_tms_regions", j, lang="en", tkey="regions")
    buf = io.BytesIO()
    im.save(buf, format="PNG", pnginfo=meta)
    put(name, buf.getvalue())


png([{"text": "Summer Sale", "bbox": [30, 20, 160, 24], "confidence": 0.97}, {"text": "Shop now", "bbox": [30, 60, 100, 20], "confidence": 0.5},
     {"text": "Big deals. Small prices.", "bbox": [30, 84, 180, 16], "confidence": 0.92}], "banner.png")
png([{"text": t, "bbox": [10, 8 + 22 * i, 190, 18]} for i, t in enumerate(["Welcome", LANGS["ar"], LANGS["ja"], LANGS["zh"], LANGS["ko"]])], "multilang.png", size=(220, 130))
png([{"text": t, "bbox": [10, 8 + 22 * i, 190, 18]} for i, t in enumerate([LANGS["ru"], LANGS["he"], LANGS["th"]])], "itxt.png", kind="iTXt")
png([{"text": "Compressed sidecar", "bbox": [10, 10, 190, 20], "polygon": [[10, 10], [200, 10], [200, 30], [10, 30]], "angle_deg": 0.0}], "ztxt.png", kind="zTXt")
print("written to", root)
