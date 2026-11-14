r"""Writes src/tables.rs: the Python codecs XPress Tags files use.

 * mac_roman (<e0>) and cp1252 (<e1>): the bytes 0x80..=0xFF as code points (UNDEF = rejected).
 * cp932 (<e3>), big5 (<e6>), gbk (<e7>), cp949 (<e19>): which single bytes and which two-byte sequences the
   Python codec accepts. The Rust side decodes and encodes with encoding_rs (a superset in places) and refuses
   what Python refuses.

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe gen_tables.py ..\..\src\tables.rs
"""
import sys

out = ["// Generated from Python's codecs by tests/tools/gen_tables.py. Do not edit.",
       "// Single-byte tables map the bytes 0x80..=0xFF to a code point; UNDEF marks a byte the codec rejects.",
       "pub const UNDEF: u32 = 0xFFFF_FFFF;", ""]
for name, codec in (("MAC_ROMAN", "mac_roman"), ("CP1252", "cp1252")):
    vals = []
    for b in range(0x80, 0x100):
        try:
            vals.append("0x%04X" % ord(bytes([b]).decode(codec)))
        except UnicodeDecodeError:
            vals.append("UNDEF")
    out.append("pub static %s: [u32; 128] = [" % name)
    for i in range(0, 128, 16):
        out.append("    " + ", ".join(vals[i:i + 16]) + ",")
    out.append("];")
    out.append("")

NPAIR = 126 * 191  # lead 0x81..=0xFE, trail 0x40..=0xFE
out.append("/// A multi-byte code page: valid single bytes, valid lead bytes, valid (lead, trail) pairs.")
out.append("pub struct Mb {")
out.append("    pub single: [u32; 8],")
out.append("    pub lead: [u32; 8],")
out.append("    pub pairs: &'static [u32],")
out.append("}")
out.append("")
for name, codec in (("CP932", "cp932"), ("BIG5", "big5"), ("GBK", "gbk"), ("CP949", "cp949")):
    single = [0] * 8
    lead = [0] * 8
    pairs = [0] * ((NPAIR + 31) // 32)
    for b in range(256):
        try:
            bytes([b]).decode(codec)
            single[b >> 5] |= 1 << (b & 31)
        except UnicodeDecodeError:
            pass
    for l in range(0x81, 0xFF):
        if single[l >> 5] >> (l & 31) & 1:
            continue  # valid alone (cp932 0xA0, 0xFD..0xFF): not a lead
        for t in range(0x40, 0xFF):
            try:
                bytes([l, t]).decode(codec)
            except UnicodeDecodeError:
                continue
            k = (l - 0x81) * 191 + (t - 0x40)
            pairs[k >> 5] |= 1 << (k & 31)
            lead[l >> 5] |= 1 << (l & 31)
    seen = {}
    for l in range(0x81, 0xFF):
        if single[l >> 5] >> (l & 31) & 1:
            continue
        for t in range(0x40, 0xFF):
            try:
                ch = bytes([l, t]).decode(codec)
            except UnicodeDecodeError:
                continue
            seen[ch] = seen.get(ch, 0) + 1
    dups = sorted((ord(c), int.from_bytes(c.encode(codec), "big")) for c, n in seen.items() if n > 1)
    out.append("/// Characters with more than one spelling in the code page, and the one Python writes.")
    out.append("pub static DUP_%s: &[(u32, u16)] = &[" % name)
    for i in range(0, len(dups), 6):
        out.append("    " + ", ".join("(0x%04X, 0x%04X)" % d for d in dups[i:i + 6]) + ",")
    out.append("];")
    out.append("")
    out.append("pub static %s: Mb = Mb {" % name)
    out.append("    single: [%s]," % ", ".join("0x%08X" % w for w in single))
    out.append("    lead: [%s]," % ", ".join("0x%08X" % w for w in lead))
    out.append("    pairs: &[")
    for i in range(0, len(pairs), 8):
        out.append("        " + ", ".join("0x%08X" % w for w in pairs[i:i + 8]) + ",")
    out.append("    ],")
    out.append("};")
    out.append("")
open(sys.argv[1], "w", encoding="utf-8", newline="\n").write("\n".join(out))

# Python's Big5 differs from the WHATWG Big5 in about 260 mappings (and has no Hong Kong extension): the whole
# table, as 16-bit little-endian code points indexed by (lead - 0x81) * 191 + (trail - 0x40); 0 = invalid.
import struct

tab = bytearray()
for l in range(0x81, 0xFF):
    for t in range(0x40, 0xFF):
        try:
            ch = bytes([l, t]).decode("big5")
            assert len(ch) == 1 and ord(ch) < 0x10000
            tab += struct.pack("<H", ord(ch))
        except UnicodeDecodeError:
            tab += b"\0\0"
open(sys.argv[1].replace("tables.rs", "big5.bin"), "wb").write(bytes(tab))
