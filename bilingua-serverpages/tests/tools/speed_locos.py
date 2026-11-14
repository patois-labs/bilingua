r"""Times the LocOS server-page filters on the large files the Rust speed test writes (read only use of LocOS).

    mkdir big && set BILINGUA_SP_SPEED_DIR=big
    cargo test -p bilingua-serverpages --release --test speed -- --nocapture
    <LocOS venv python> -I speed_locos.py big
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-sp-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000
config.MAX_UPLOAD_BYTES = 1 << 30

for ext in ("phtml", "jsp"):
    data = open(os.path.join(sys.argv[1], "big." + ext), "rb").read()
    f = get_filter(ext)
    t0 = time.perf_counter()
    pr = f.parse(data)
    t1 = time.perf_counter()
    mapping = {str(i): 'T%d "q" translated ' % i for i in range(len(pr.units))}
    f.build(data, mapping)
    t2 = time.perf_counter()
    assert f.build(data, {}) == data
    print(f"LocOS {ext}: {len(data) / 1e6:.1f} MB, {len(pr.units)} units, parse {1e3 * (t1 - t0):.0f} ms, "
          f"build {1e3 * (t2 - t1):.0f} ms, parse+build {1e3 * (t2 - t0):.0f} ms")
