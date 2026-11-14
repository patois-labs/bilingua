"""Writes src/cjk.bin: Python's cp932, gbk, cp949 and cp950 codecs as tables (what LocOS reads and writes DXF with).

    <LocOS venv python> tools/gen_cjk.py src/cjk.bin

Layout, little endian, for each codec in the order cp932, gbk, cp949, cp950:
  128 x u16   the character of the single byte 0x80..0xFF (0 = the codec rejects it)
  126*191 x u16  the character of the pair lead 0x81..0xFE, trail 0x40..0xFE (index (lead-0x81)*191 + trail-0x40; 0 = none)
  u32 n, then n x (u32 character, u16 code)  where the codec writes a character differently from "the lowest
       code that decodes to it" (code 0 = the codec cannot write it). ASCII is the identity in all four.
A code is the byte for a single byte (< 0x100) or lead << 8 | trail.
Read only use of Python; nothing in LocOS is touched.
"""

import struct
import sys

CODECS = ["cp932", "gbk", "cp949", "cp950"]
LEADS = range(0x81, 0xFF)
TRAILS = range(0x40, 0xFF)


def one(codec, raw):
    try:
        s = raw.decode(codec)
    except UnicodeDecodeError:
        return 0
    if len(s) != 1:
        return -1
    cp = ord(s)
    assert 0 < cp < 0x10000, (codec, raw, cp)
    return cp


def build(codec):
    for b in range(0x80):
        assert bytes([b]).decode(codec) == chr(b), (codec, b)
        assert chr(b).encode(codec) == bytes([b]), (codec, b)
    singles = []
    for b in range(0x80, 0x100):
        v = one(codec, bytes([b]))
        assert v >= 0, (codec, b)
        singles.append(v)
    pairs = []
    for lead in LEADS:
        for trail in TRAILS:
            v = one(codec, bytes([lead, trail]))
            if v == -1:
                v = 0  # two characters (a single-byte letter followed by an ASCII one): not a pair
            pairs.append(v)
    # a byte that reads alone is never also a lead byte with a valid pair
    for i, s in enumerate(singles):
        if s:
            lead = 0x80 + i
            if 0x81 <= lead <= 0xFE:
                row = pairs[(lead - 0x81) * len(TRAILS) : (lead - 0x80) * len(TRAILS)]
                assert not any(row), (codec, hex(lead))
    # default reverse: the lowest code that decodes to the character
    rev = {}
    for i, s in enumerate(singles):
        if s and s not in rev:
            rev[s] = 0x80 + i
    for li, lead in enumerate(LEADS):
        for ti, trail in enumerate(TRAILS):
            v = pairs[li * len(TRAILS) + ti]
            if v and v not in rev:
                rev[v] = lead << 8 | trail
    overrides = []
    for cp in range(0x80, 0x10000):
        if 0xD800 <= cp <= 0xDFFF:
            continue
        try:
            e = chr(cp).encode(codec)
            code = e[0] if len(e) == 1 else int.from_bytes(e, "big")
            assert len(e) <= 2, (codec, cp, e)
        except UnicodeEncodeError:
            code = 0
        if code != rev.get(cp, 0):
            overrides.append((cp, code))
    # supplementary planes are never writable
    for cp in range(0x10000, 0x30000):
        try:
            chr(cp).encode(codec)
            raise AssertionError((codec, hex(cp)))
        except UnicodeEncodeError:
            pass
    return singles, pairs, overrides


def main(path):
    out = bytearray()
    for codec in CODECS:
        singles, pairs, overrides = build(codec)
        out += struct.pack("<128H", *singles)
        out += struct.pack(f"<{len(pairs)}H", *pairs)
        out += struct.pack("<I", len(overrides))
        for cp, code in overrides:
            out += struct.pack("<IH", cp, code)
        print(codec, "pairs", sum(1 for p in pairs if p), "singles", sum(1 for s in singles if s), "overrides", len(overrides))
    with open(path, "wb") as fh:
        fh.write(out)
    print(len(out), "bytes")


main(sys.argv[1])
