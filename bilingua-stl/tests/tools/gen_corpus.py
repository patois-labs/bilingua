"""Writes the made-up STL corpus files into bilingua/oracle/corpus/stl/ (pass the folder). Plain Python, no LocOS.

Files: news (table 00, double height, colours, extension blocks, comment and user data), plain (open subtitles with
italics, 30 fps), and one file per other table (Cyrillic, Arabic, Greek, Hebrew) holding bytes of that table.
"""

import os
import sys


def gsi(cct=b"00", dfc=b"STL25.01", tnb=0, title=b"Demo", mnc=b"40"):
    g = bytearray(b" " * 1024)
    g[0:3] = b"850"
    g[3:11] = dfc
    g[11:12] = b"1"
    g[12:14] = cct
    g[14:16] = b"09"
    g[16:48] = title.ljust(32)
    g[224:230] = b"261105"
    g[230:236] = b"261105"
    g[236:238] = b"01"
    g[238:243] = b"%05d" % tnb
    g[243:248] = b"%05d" % tnb
    g[248:251] = b"001"
    g[251:253] = mnc
    g[253:255] = b"23"
    g[255:256] = b"1"
    return bytes(g)


def tti(sn, tf, ebn=0xFF, cf=0, vp=20, tci=(10, 0, 1, 0), tco=(10, 0, 3, 0)):
    head = bytes([0]) + sn.to_bytes(2, "little") + bytes([ebn, 0, *tci, *tco, vp, 2, cf])
    return head + tf + b"\x8f" * (112 - len(tf))


DH = b"\x0d\x0b\x0b"
END = b"\x0a\x0a"


def write(folder, name, cct, blocks, dfc=b"STL25.01"):
    with open(os.path.join(folder, name), "wb") as fh:
        fh.write(gsi(cct, dfc, len(blocks)) + b"".join(blocks))


def main():
    folder = sys.argv[1]
    os.makedirs(folder, exist_ok=True)
    write(folder, "news.stl", b"00", [
        tti(1, DH + b"Good evening," + END + b"\x8a\x8a" + DH + b"and welcome." + END),
        tti(2, DH + b"Caf\xc2e cr\xc8eme \x01red\x07 white" + END, tci=(10, 0, 4, 0), tco=(10, 0, 6, 0)),
        tti(3, b"Editor note: check the spelling", cf=1),
        tti(3, b"user data", ebn=0xFE),
        tti(4, DH + b"A long subtitle that runs " + END, ebn=0x00, tci=(10, 0, 7, 0), tco=(10, 0, 9, 0)),
        tti(4, b"\x8a\x8a" + DH + b"over two blocks." + END, tci=(10, 0, 7, 0), tco=(10, 0, 9, 0)),
    ])
    write(folder, "plain30.stl", b"00", [
        tti(1, b"\x80Italic\x81 open text $5 #1"),
        tti(2, b"Second line, no boxing", tci=(10, 0, 4, 0), tco=(10, 0, 6, 0)),
    ], dfc=b"STL30.01")
    write(folder, "cyrillic.stl", b"01", [tti(1, DH + b"Hello \xbf\xe0\xd8\xd2\xd5\xe2" + END)])
    write(folder, "arabic.stl", b"02", [tti(1, DH + b"abc \xc7\xe4\xd3\xe4" + END)])
    write(folder, "greek.stl", b"03", [tti(1, DH + b"x \xe1\xe2\xe3 \xdc y" + END)])
    write(folder, "hebrew.stl", b"04", [tti(1, DH + b"a \xf9\xec\xe5\xed b" + END)])


main()
