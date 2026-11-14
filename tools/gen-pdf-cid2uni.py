"""Make or check bilingua-pdf/src/cid2uni.bin: PDFium's Adobe CID to Unicode tables (`CPDF_CID2UnicodeMap`).

PDFium (BSD-3-Clause, see bilingua/NOTICE) maps the CID of a Type 0 font with no ToUnicode through a built-in table
of the font's Adobe collection: `kGB1CID2Unicode_5`, `kCNS1CID2Unicode_5`, `kJapan1CID2Unicode_4` and
`kKorea1CID2Unicode_2`, in `core/fpdfapi/cmaps/<collection>/Adobe-<collection>-UCS2_<n>.inc`. Those files are only
read here as data (nothing from them is compiled or run).

    python -I bilingua/tools/gen-pdf-cid2uni.py PDFIUM_SRC_DIR [--write]

PDFIUM_SRC_DIR is a PDFium checkout (or a folder holding just those four files under the same paths). Without
--write it checks that the committed file holds the same tables and exits 1 if not.

File format (read by bilingua-pdf/src/cid2uni.rs): a zlib stream of little-endian u16 words. For each collection in
PDFium's CIDSet order (GB1, CNS1, Japan1, Korea1): the entry count, then one word per CID: the code unit minus the
previous one minus 1 (wrapping; the first entry of each table counts from 0), so a run of consecutive code points is
a run of zeros and the tables compress to under half their size.
"""

import os
import re
import struct
import sys
import zlib

SETS = [("GB1", "Adobe-GB1-UCS2_5.inc", "kGB1CID2Unicode_5"),
        ("CNS1", "Adobe-CNS1-UCS2_5.inc", "kCNS1CID2Unicode_5"),
        ("Japan1", "Adobe-Japan1-UCS2_4.inc", "kJapan1CID2Unicode_4"),
        ("Korea1", "Adobe-Korea1-UCS2_2.inc", "kKorea1CID2Unicode_2")]
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "bilingua-pdf", "src", "cid2uni.bin")


def table(src, coll, fname, cname):
    with open(os.path.join(src, "core", "fpdfapi", "cmaps", coll, fname), encoding="latin-1") as f:
        text = f.read()
    m = re.search(r"\b" + cname + r"\[\]\s*=\s*\{(.*?)\};", text, re.S)
    if not m:
        raise SystemExit("missing " + cname)
    body = re.sub(r"//[^\n]*", "", m.group(1))
    vals = [int(t, 16) for t in (x.strip() for x in body.split(",")) if t]
    assert all(0 <= v <= 0xFFFF for v in vals), cname
    assert len(vals) < 0x10000, cname
    return vals


def payload(tables):
    words = []
    for vals in tables:
        words.append(len(vals))
        prev = 0
        for v in vals:
            words.append((v - prev - 1) & 0xFFFF)
            prev = v
    return struct.pack("<%dH" % len(words), *words)


def decode(data):
    raw = zlib.decompress(data)
    words = struct.unpack("<%dH" % (len(raw) // 2), raw)
    out, i = [], 0
    for _ in SETS:
        n = words[i]
        i += 1
        vals, prev = [], 0
        for d in words[i:i + n]:
            prev = (prev + d + 1) & 0xFFFF
            vals.append(prev)
        out.append(vals)
        i += n
    assert i == len(words)
    return out


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        raise SystemExit(__doc__)
    tables = [table(args[0], *s) for s in SETS]
    data = zlib.compress(payload(tables), 9)
    assert decode(data) == tables
    if "--write" in sys.argv:
        with open(OUT, "wb") as f:
            f.write(data)
        print("wrote", os.path.normpath(OUT), len(data), "bytes;", ", ".join(
            "%s %d CIDs" % (s[0], len(t)) for s, t in zip(SETS, tables)))
        return
    with open(OUT, "rb") as f:
        same = decode(f.read()) == tables
    print("cid2uni.bin is", "up to date" if same else "OUT OF DATE (run with --write)")
    sys.exit(0 if same else 1)


if __name__ == "__main__":
    main()
