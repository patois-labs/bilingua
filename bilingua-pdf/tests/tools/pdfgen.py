"""A tiny PDF writer for the bilingua-pdf fixtures (standard library only, plus `cryptography` for AES).

Each fixture is a list of objects; `write()` lays them out with a classic xref table, an xref stream with object
streams, or no usable xref at all, and can encrypt them with the standard security handler (RC4 40/128, AES-128,
AES-256 R6). Nothing here is clever: it writes exactly what the test asks for so PDFium's reading of it can be
recorded as the golden.
"""

import hashlib
import os
import struct
import zlib

PAD = bytes.fromhex("28BF4E5E4E758A4164004E56FFFA01082E2E00B6D0683E802F0CA9FE6453697A")


class Ref:
    def __init__(self, n, g=0):
        self.n, self.g = n, g


class Name(str):
    pass


class Raw(bytes):
    """Bytes written as they are (a content operator sequence, a pre-encoded string)."""


class Str(bytes):
    """A PDF string, written as a literal with escapes (or hex when `hexed`)."""

    def __new__(cls, b, hexed=False):
        o = bytes.__new__(cls, b if isinstance(b, (bytes, bytearray)) else b.encode("latin-1"))
        o.hexed = hexed
        return o


class Stream:
    def __init__(self, dict_, data, compress=False, filters=None):
        self.dict = dict(dict_)
        self.data = data
        if compress:
            self.data = zlib.compress(data)
            self.dict["Filter"] = Name("FlateDecode")
        if filters:
            self.dict["Filter"] = filters


def lit(b):
    out = bytearray(b"(")
    for c in b:
        if c in b"()\\":
            out += b"\\" + bytes([c])
        elif c == 13:
            out += b"\\r"
        elif c == 10:
            out += b"\\n"
        else:
            out.append(c)
    return bytes(out + b")")


def ser(o, crypt=None, num=None):
    if o is None:
        return b"null"
    if o is True:
        return b"true"
    if o is False:
        return b"false"
    if isinstance(o, Ref):
        return b"%d %d R" % (o.n, o.g)
    if isinstance(o, Name):
        return b"/" + o.encode("latin-1")
    if isinstance(o, Raw):
        return bytes(o)
    if isinstance(o, Str):
        b = bytes(o)
        if crypt and num is not None:
            b = crypt.encrypt(num, b)
        return b"<" + b.hex().encode() + b">" if (o.hexed or crypt) else lit(b)
    if isinstance(o, bytes):
        return lit(o)
    if isinstance(o, str):
        return lit(o.encode("latin-1"))
    if isinstance(o, bool):
        return b"true" if o else b"false"
    if isinstance(o, int):
        return b"%d" % o
    if isinstance(o, float):
        s = ("%.4f" % o).rstrip("0").rstrip(".")
        return s.encode() if s not in ("", "-") else b"0"
    if isinstance(o, (list, tuple)):
        return b"[" + b" ".join(ser(x, crypt, num) for x in o) + b"]"
    if isinstance(o, dict):
        return b"<<" + b"".join(b"/" + k.encode("latin-1") + b" " + ser(v, crypt, num) for k, v in o.items()) + b">>"
    raise TypeError(type(o))


def ser_obj(o, crypt=None, num=None):
    if isinstance(o, Stream):
        data = o.data
        if crypt and num is not None and not o.dict.get("_plain"):
            data = crypt.encrypt(num, data)
        d = {k: v for k, v in o.dict.items() if not k.startswith("_")}
        d["Length"] = len(data)
        return ser(d, crypt, num) + b"\nstream\n" + data + b"\nendstream"
    return ser(o, crypt, num)


# ---------------------------------------------------------------- encryption

class Crypt:
    """Standard security handler. `kind` is rc4-40, rc4-128, aes-128 or aes-256."""

    def __init__(self, kind, user=b"", owner=b"owner", perms=-4, file_id=b"0123456789abcdef"):
        self.kind, self.id = kind, file_id
        self.P = perms
        if kind == "aes-256":
            self._r6(user, owner)
            return
        self.R = {"rc4-40": 2, "rc4-128": 3, "aes-128": 4}[kind]
        self.V = {"rc4-40": 1, "rc4-128": 2, "aes-128": 4}[kind]
        self.n = 5 if self.R == 2 else 16
        o = hashlib.md5((owner + PAD)[:32]).digest()
        if self.R >= 3:
            for _ in range(50):
                o = hashlib.md5(o[: self.n]).digest()
        k = o[: self.n]
        O = rc4(k, (user + PAD)[:32])
        if self.R >= 3:
            for i in range(1, 20):
                O = rc4(bytes(b ^ i for b in k), O)
        self.O = O
        h = hashlib.md5((user + PAD)[:32] + O + struct.pack("<i", self.P) + file_id)
        key = h.digest()
        if self.R >= 3:
            for _ in range(50):
                key = hashlib.md5(key[: self.n]).digest()
        self.key = key[: self.n]
        if self.R == 2:
            self.U = rc4(self.key, PAD)
        else:
            u = rc4(self.key, hashlib.md5(PAD + file_id).digest())
            for i in range(1, 20):
                u = rc4(bytes(b ^ i for b in self.key), u)
            self.U = u + b"\x00" * 16

    def _r6(self, user, owner):
        self.R, self.V = 6, 5
        self.key = os.urandom(32)
        uvs, uks = os.urandom(8), os.urandom(8)
        self.U = hash_r6(user, uvs) + uvs + uks
        self.UE = aes_cbc_nopad(hash_r6(user, uks), self.key)
        ovs, oks = os.urandom(8), os.urandom(8)
        self.O = hash_r6(owner, ovs, self.U) + ovs + oks
        self.OE = aes_cbc_nopad(hash_r6(owner, oks, self.U), self.key)
        perms = struct.pack("<i", self.P) + b"\xff\xff\xff\xff" + b"T" + b"adb" + os.urandom(4)
        from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

        e = Cipher(algorithms.AES(self.key), modes.ECB()).encryptor()
        self.Perms = e.update(perms) + e.finalize()

    def obj_key(self, num, gen=0):
        if self.R == 6:
            return self.key
        k = self.key + struct.pack("<I", num)[:3] + struct.pack("<I", gen)[:2]
        if self.kind == "aes-128":
            k += b"sAlT"
        return hashlib.md5(k).digest()[: min(len(self.key) + 5, 16)]

    def encrypt(self, num, data):
        k = self.obj_key(num)
        if self.kind.startswith("rc4"):
            return rc4(k, data)
        from cryptography.hazmat.primitives import padding
        from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

        iv = os.urandom(16)
        p = padding.PKCS7(128).padder()
        data = p.update(data) + p.finalize()
        e = Cipher(algorithms.AES(k), modes.CBC(iv)).encryptor()
        return iv + e.update(data) + e.finalize()

    def dict(self):
        if self.R == 6:
            cf = {"StdCF": {"CFM": Name("AESV3"), "AuthEvent": Name("DocOpen"), "Length": 32}}
            return {"Filter": Name("Standard"), "V": 5, "R": 6, "Length": 256, "CF": cf, "StmF": Name("StdCF"),
                    "StrF": Name("StdCF"), "O": Str(self.O, True), "U": Str(self.U, True), "OE": Str(self.OE, True),
                    "UE": Str(self.UE, True), "Perms": Str(self.Perms, True), "P": self.P}
        d = {"Filter": Name("Standard"), "V": self.V, "R": self.R, "O": Str(self.O, True), "U": Str(self.U, True), "P": self.P}
        if self.R >= 3:
            d["Length"] = self.n * 8
        if self.V == 4:
            d["CF"] = {"StdCF": {"CFM": Name("AESV2"), "AuthEvent": Name("DocOpen"), "Length": 16}}
            d["StmF"] = Name("StdCF")
            d["StrF"] = Name("StdCF")
        return d


def rc4(key, data):
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) & 255
        s[i], s[j] = s[j], s[i]
    out = bytearray()
    i = j = 0
    for c in data:
        i = (i + 1) & 255
        j = (j + s[i]) & 255
        s[i], s[j] = s[j], s[i]
        out.append(c ^ s[(s[i] + s[j]) & 255])
    return bytes(out)


def aes_cbc_nopad(key, data, iv=b"\x00" * 16):
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

    e = Cipher(algorithms.AES(key), modes.CBC(iv)).encryptor()
    return e.update(data) + e.finalize()


def hash_r6(pw, salt, udata=b""):
    """ISO 32000-2 algorithm 2.B."""
    k = hashlib.sha256(pw + salt + udata).digest()
    i = 0
    while True:
        k1 = (pw + k + udata) * 64
        e = aes_cbc_nopad(k[:16], k1, k[16:32])
        m = sum(e[:16]) % 3
        k = [hashlib.sha256, hashlib.sha384, hashlib.sha512][m](e).digest()
        i += 1
        if i >= 64 and e[-1] <= i - 32:
            break
    return k[:32]


# ---------------------------------------------------------------- layout

def write(objs, root, mode="classic", crypt=None, info=None, extra_trailer=None, header=b"%PDF-1.7"):
    """objs: {num: object}. mode: classic | xrefstream | objstm | noxref | badoffsets."""
    out = bytearray(header + b"\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    trailer = {"Size": max(objs) + 1, "Root": Ref(root)}
    if info:
        trailer["Info"] = Ref(info)
    if crypt:
        trailer["ID"] = [Str(crypt.id, True), Str(crypt.id, True)]
    if extra_trailer:
        trailer.update(extra_trailer)
    enc_num = None
    if crypt:
        enc_num = max(objs) + 1
        objs = dict(objs)
        objs[enc_num] = crypt.dict()
        trailer["Size"] = enc_num + 1
        trailer["Encrypt"] = Ref(enc_num)

    if mode == "objstm":
        inner = [n for n, o in objs.items() if not isinstance(o, Stream) and n != enc_num]
        outer = [n for n in objs if n not in inner]
        body = bytearray()
        heads = []
        for n in inner:
            heads.append(b"%d %d" % (n, len(body)))
            body += ser_obj(objs[n]) + b"\n"
        head = b" ".join(heads) + b"\n"
        stm_num = max(objs) + 1
        objs = dict(objs)
        objs[stm_num] = Stream({"Type": Name("ObjStm"), "N": len(inner), "First": len(head)}, head + bytes(body), compress=True)
        outer.append(stm_num)
        for n in outer:
            offsets[n] = len(out)
            out += b"%d 0 obj\n" % n + ser_obj(objs[n], crypt if n != enc_num else None, n) + b"\nendobj\n"
        xref_num = stm_num + 1
        size = xref_num + 1
        rows = []
        for n in range(size):
            if n in offsets:
                rows.append(struct.pack(">BIH", 1, offsets[n], 0))
            elif n in inner:
                rows.append(struct.pack(">BIH", 2, stm_num, inner.index(n)))
            elif n == xref_num:
                rows.append(struct.pack(">BIH", 1, len(out), 0))
            else:
                rows.append(struct.pack(">BIH", 0, 0, 65535))
        raw = predict_png_up(b"".join(rows), 7)
        d = dict(trailer)
        d.update({"Type": Name("XRef"), "Size": size, "W": [1, 4, 2], "DecodeParms": {"Predictor": 12, "Columns": 7}})
        xs = Stream(d, raw, compress=True)
        xs.dict["DecodeParms"] = {"Predictor": 12, "Columns": 7}
        at = len(out)
        out += b"%d 0 obj\n" % xref_num + ser_obj(xs) + b"\nendobj\nstartxref\n%d\n%%%%EOF\n" % at
        return bytes(out)

    for n in sorted(objs):
        offsets[n] = len(out)
        c = crypt if n != enc_num else None
        out += b"%d 0 obj\n" % n + ser_obj(objs[n], c, n) + b"\nendobj\n"
    if mode == "noxref":
        out += b"trailer\n" + ser(trailer) + b"\n%%EOF\n"
        return bytes(out)
    at = len(out)
    size = max(objs) + 1
    out += b"xref\n0 %d\n0000000000 65535 f \n" % size
    for n in range(1, size):
        off = offsets.get(n)
        if off is None:
            out += b"0000000000 65535 f \n"
        else:
            if mode == "badoffsets":
                off += 7
            out += b"%010d 00000 n \n" % off
    out += b"trailer\n" + ser(trailer) + b"\nstartxref\n%d\n%%%%EOF\n" % at
    return bytes(out)


def append_update(base, objs, root, prev_loop=False, size=None):
    """An incremental update: new objects and a new xref section whose /Prev points at the old one (or at itself)."""
    import re

    m = list(re.finditer(rb"startxref\s+(\d+)", base))[-1]
    prev = int(m.group(1))
    out = bytearray(base)
    offsets = {}
    for n in sorted(objs):
        offsets[n] = len(out)
        out += b"%d 0 obj\n" % n + ser_obj(objs[n]) + b"\nendobj\n"
    at = len(out)
    out += b"xref\n"
    for n in sorted(objs):
        out += b"%d 1\n%010d 00000 n \n" % (n, offsets[n])
    t = {"Size": size or max(max(objs) + 1, 2), "Root": Ref(root), "Prev": at if prev_loop else prev}
    out += b"trailer\n" + ser(t) + b"\nstartxref\n%d\n%%%%EOF\n" % at
    return bytes(out)


def predict_png_up(data, columns):
    out = bytearray()
    prev = bytes(columns)
    for i in range(0, len(data), columns):
        row = data[i : i + columns]
        out.append(2)
        out += bytes((row[j] - prev[j]) & 255 for j in range(len(row)))
        prev = row
    return bytes(out)


# ---------------------------------------------------------------- page helpers

def simple_doc(content, fonts, page_extra=None, extra_objs=None, media=(0, 0, 612, 792), pages=None):
    """A document with one page (or several: `pages` is a list of (content, fonts, page_extra)).

    fonts: {resource name: font dict or Stream objects already numbered via extra_objs}.
    Returns (objs, root)."""
    objs = {}
    n = [10]

    def add(o):
        n[0] += 1
        objs[n[0]] = o
        return Ref(n[0])

    specs = pages or [(content, fonts, page_extra)]
    kids = []
    for content, fonts, page_extra in specs:
        res = {"Font": {k: (v if isinstance(v, Ref) else add(v)) for k, v in fonts.items()}}
        if page_extra and "_res" in page_extra:
            res.update(page_extra["_res"])
        cs = [add(Stream({}, c, compress=True)) for c in (content if isinstance(content, list) else [content])]
        page = {"Type": Name("Page"), "Parent": Ref(2), "MediaBox": list(media), "Resources": res,
                "Contents": cs[0] if len(cs) == 1 else cs}
        if page_extra:
            page.update({k: v for k, v in page_extra.items() if k != "_res"})
        kids.append(add(page))
    objs[1] = {"Type": Name("Catalog"), "Pages": Ref(2)}
    objs[2] = {"Type": Name("Pages"), "Kids": kids, "Count": len(kids)}
    if extra_objs:
        objs.update(extra_objs)
    return objs, 1


def std_font(base="Helvetica", encoding="WinAnsiEncoding", widths=None, first=32, subtype="Type1", extra=None):
    f = {"Type": Name("Font"), "Subtype": Name(subtype), "BaseFont": Name(base)}
    if encoding is not None:
        f["Encoding"] = Name(encoding) if isinstance(encoding, str) else encoding
    if widths is not None:
        f["FirstChar"] = first
        f["LastChar"] = first + len(widths) - 1
        f["Widths"] = widths
    if extra:
        f.update(extra)
    return f


def tounicode_cmap(pairs, ranges=(), codespace=(b"\x00\x00", b"\xff\xff")):
    lo, hi = codespace
    body = [b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap",
            b"/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def",
            b"/CMapName /Adobe-Identity-UCS def /CMapType 2 def",
            b"1 begincodespacerange <%s> <%s> endcodespacerange" % (lo.hex().encode(), hi.hex().encode())]
    if pairs:
        body.append(b"%d beginbfchar" % len(pairs))
        for code, text in pairs:
            body.append(b"<%s> <%s>" % (code.hex().encode(), text.encode("utf-16-be").hex().encode()))
        body.append(b"endbfchar")
    if ranges:
        body.append(b"%d beginbfrange" % len(ranges))
        for a, b, start in ranges:
            body.append(b"<%s> <%s> <%s>" % (a.hex().encode(), b.hex().encode(), start.encode("utf-16-be").hex().encode()))
        body.append(b"endbfrange")
    body.append(b"endcmap CMapName currentdict /CMap defineresource pop end end")
    return Stream({}, b"\n".join(body), compress=True)
