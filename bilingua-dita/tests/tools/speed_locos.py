"""Times the LocOS `dita` filter on the large file the Rust tool writes (read only use of LocOS).

    cargo run -p bilingua-dita --release --example dita_dump -- gen big.dita 6
    cargo run -p bilingua-dita --release --example dita_dump -- time big.dita
    the LocOS venv python speed_locos.py big.dita
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-dita-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 50_000_000
for name in ("MAX_SEGMENT_CHARS", "MAX_UPLOAD_BYTES"):
    if hasattr(config, name):
        setattr(config, name, 1 << 40)

data = open(sys.argv[1], "rb").read()
f = get_filter("dita")
t0 = time.perf_counter()
pr = f.parse(data)
t1 = time.perf_counter()
mapping = {str(i): "[" + u.text + "]" for i, u in enumerate(pr.units)}
out = f.build(data, mapping)
t2 = time.perf_counter()
assert f.build(data, {}) == data
t3 = time.perf_counter()
print(f"LocOS dita: {len(data) / 1e6:.1f} MB, {len(pr.units)} units, parse {1e3 * (t1 - t0):.0f} ms, "
      f"build {1e3 * (t2 - t1):.0f} ms, untouched build {1e3 * (t3 - t2):.0f} ms, parse+build {1e3 * (t2 - t0):.0f} ms")
