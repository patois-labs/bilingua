r"""Builds tests/data/codec_golden.json: for every code page an XPress Tags file can select, the digest of Python's
decoding of every byte sequence it accepts (all valid single bytes, then all valid lead/trail pairs in order) and of
encoding that text back.

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe gen_codec_golden.py ..\data\codec_golden.json
"""
import json
import sys

CODECS = {"mac_roman": "mac_roman", "cp1252": "cp1252", "latin-1": "latin-1", "cp932": "cp932", "big5": "big5",
          "gbk": "gbk", "cp949": "cp949"}


def fnv(b):
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "%016x" % h


out = {}
for name, codec in CODECS.items():
    parts = []
    for b in range(256):
        try:
            bytes([b]).decode(codec)
            parts.append(bytes([b]))
        except UnicodeDecodeError:
            pass
    singles = set(parts)
    for l in range(0x81, 0xFF):
        if bytes([l]) in singles:
            continue  # a byte that is valid alone (cp932 0xA0, 0xFD..0xFF) is not a lead
        for t in range(0x40, 0xFF):
            try:
                bytes([l, t]).decode(codec)
                parts.append(bytes([l, t]))
            except UnicodeDecodeError:
                pass
    data = b"".join(parts) if name in ("cp932", "big5", "gbk", "cp949") else bytes(b for b in range(256) if (lambda x: True)(b))
    if name not in ("cp932", "big5", "gbk", "cp949"):
        ok = bytearray()
        for b in range(256):
            try:
                bytes([b]).decode(codec)
                ok.append(b)
            except UnicodeDecodeError:
                pass
        data = bytes(ok)
    text = data.decode(codec)
    re = text.encode(codec)
    out[name] = {"len": len(data), "chars": len(text), "text": fnv(text.encode("utf-8")), "reenc": fnv(re), "same": re == data}
json.dump(out, open(sys.argv[1], "w"), indent=1)
print(out)
