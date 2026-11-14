# A package with per-member comments, extra fields, DOS attributes and data descriptors (unseekable writer).
import io, struct, sys, zipfile
WS = open(sys.argv[1], 'rb').read()
class Unseekable(io.RawIOBase):
    def __init__(self): self.buf = bytearray()
    def writable(self): return True
    def write(self, b): self.buf += b; return len(b)
out = Unseekable()
with zipfile.ZipFile(out, 'w') as zf:
    for name, body, method, comment in [("content.xlf", WS, zipfile.ZIP_DEFLATED, b"xliff member"),
                                         ("content.skl", b"<skl>{1}</skl>" * 20, zipfile.ZIP_DEFLATED, b"skeleton"),
                                         ("raw.bin", bytes(range(256)), zipfile.ZIP_STORED, b"")]:
        info = zipfile.ZipInfo(name, date_time=(2023, 1, 2, 3, 4, 6))
        info.compress_type = method
        info.comment = comment
        info.extra = struct.pack('<HH', 0xCAFE, 4) + b"\x01\x02\x03\x04"
        info.external_attr = 0x20  # DOS archive bit
        info.create_system = 0
        with zf.open(info, 'w') as fh:
            fh.write(body)
    zf.comment = b"package comment"
open(sys.argv[2], 'wb').write(bytes(out.buf))
