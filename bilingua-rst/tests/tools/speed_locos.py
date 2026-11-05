"""Times the LocOS rst filter on the file the Rust speed test generates (read only use of LocOS).

    BILINGUA_RST_SPEED_OUT=big.rst cargo test -p bilingua-rst --release --test speed
    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe speed_locos.py big.rst

Parses the file, then builds it with every unit translated to "T<n> " + its text, as the Rust test does.
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-rst-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000
for name in ("MAX_UPLOAD_BYTES", "MAX_UPLOAD_LINES"):
    if hasattr(config, name):
        setattr(config, name, 10**12)

data = open(sys.argv[1], "rb").read()
F = get_filter("rst")
t0 = time.perf_counter()
r = F.parse(data)
t1 = time.perf_counter()
mp = {str(i): f"T{i} " + u.text for i, u in enumerate(r.units)}
out = F.build(data, mp)
t2 = time.perf_counter()
print(f"LocOS rst speed: {len(data) / 1e6:.1f} MB, {len(r.units)} units; parse {t1 - t0:.2f} s, translated build {t2 - t1:.2f} s, out {len(out)} bytes")
