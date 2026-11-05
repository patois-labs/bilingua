"""Exhaustive fingerprints of Python's codecs (strict encode of every code point, strict decode of every 1- and
2-byte input) for the dxf gate. Prints Rust constants."""

import struct

CODECS = [
    "cp874", "cp1250", "cp1251", "cp1252", "cp1253", "cp1254", "cp1255", "cp1256", "cp1257", "cp1258",
    "cp437", "cp850", "cp852", "cp855", "cp857", "cp860", "cp861", "cp863", "cp864", "cp865", "cp866", "cp869",
    "cp932", "gbk", "cp949", "cp950",
]
M = (1 << 64) - 1


class Fnv:
    def __init__(self):
        self.h = 0xCBF29CE484222325

    def feed(self, b):
        h = self.h
        for x in b:
            h ^= x
            h = (h * 0x100000001B3) & M
        self.h = h


def enc_hash(codec):
    f = Fnv()
    for cp in range(0x110000):
        if 0xD800 <= cp <= 0xDFFF:
            continue
        try:
            e = chr(cp).encode(codec)
        except UnicodeEncodeError:
            continue
        f.feed(struct.pack("<IB", cp, len(e)) + e)
    return f.h


def dec_one(f, codec, raw):
    try:
        s = raw.decode(codec)
        f.feed(raw + b"\x01" + s.encode("utf-8") + b"\xff")
    except UnicodeDecodeError as exc:
        f.feed(raw + b"\x00" + struct.pack("<I", exc.start))


def dec_hash(codec):
    f = Fnv()
    for a in range(256):
        dec_one(f, codec, bytes([a]))
    for a in range(256):
        for b in range(256):
            dec_one(f, codec, bytes([a, b]))
    return f.h


if __name__ == "__main__":
    print("const PY_FINGERPRINTS: &[(&str, u64, u64)] = &[")
    for c in CODECS:
        print(f'    ("{c}", 0x{enc_hash(c):016X}, 0x{dec_hash(c):016X}),')
    print("];")
