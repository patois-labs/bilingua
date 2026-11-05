"""Times the LocOS generic XML filter on the large file the Rust tool writes (read only use of LocOS).

    cargo run -p bilingua-xml --release --example dump -- gen big.xml 6
    cargo run -p bilingua-xml --release --example dump -- time big.xml
    the LocOS venv python speed_locos.py big.xml
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-xml-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 50_000_000
import app.filters.base as base  # noqa: E402

for name in ("MAX_SEGMENT_CHARS", "MAX_UPLOAD_BYTES"):
    if hasattr(config, name):
        setattr(config, name, 1 << 40)

data = open(sys.argv[1], "rb").read()
f = get_filter("xml")
t0 = time.perf_counter()
pr = f.parse(data)
t1 = time.perf_counter()
mapping = {str(i): "[" + u.text + "]" for i, u in enumerate(pr.units)}
out = f.build(data, mapping)
t2 = time.perf_counter()
assert f.build(data, {}) == data
t3 = time.perf_counter()
print(f"LocOS xml: {len(data) / 1e6:.1f} MB, {len(pr.units)} units, parse {1e3 * (t1 - t0):.0f} ms, "
      f"build {1e3 * (t2 - t1):.0f} ms, untouched build {1e3 * (t3 - t2):.0f} ms, parse+build {1e3 * (t2 - t0):.0f} ms")
