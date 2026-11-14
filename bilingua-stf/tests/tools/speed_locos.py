"""Times the LocOS STF filter on the large file the Rust speed test writes (read only use of LocOS).

    BILINGUA_STF_SPEED_OUT=big.stf cargo test -p bilingua-stf --release --test speed -- --nocapture
    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe speed_locos.py big.stf [FRACTION ...]

Parse is timed on the whole file. LocOS build() splices every changed cell into one growing string (a copy of the
whole file per unit, so quadratic); on the whole 5 MB file it runs for well over an hour, so build is timed on
prefixes of the file (default 1/20 and 1/10, cut at a line end) and the growth is printed.
"""

import os
import re
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-stf-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000
f = get_filter("stf")


def mapping_for(pr):
    m = {}
    for i, u in enumerate(pr.units):
        toks = "".join(re.findall("\u27e6x\\d+\u27e7", u.text))
        m[str(i)] = 'T%d "q" ' % i + toks
    return m


data = open(sys.argv[1], "rb").read()
t0 = time.perf_counter()
pr = f.parse(data)
t1 = time.perf_counter()
print(f"LocOS stf parse: {len(data) / 1e6:.1f} MB, {len(pr.units)} units, {1e3 * (t1 - t0):.0f} ms", flush=True)
fracs = [float(x) for x in sys.argv[2:]] or [0.05, 0.1]
for fr in fracs:
    cut = data.rfind(b"\n", 0, int(len(data) * fr)) + 1
    part = data[:cut]
    p = f.parse(part)
    m = mapping_for(p)
    t2 = time.perf_counter()
    f.build_localized(part, m, "de-DE")
    t3 = time.perf_counter()
    print(f"LocOS stf build_localized: {len(part) / 1e6:.2f} MB, {len(p.units)} units, {1e3 * (t3 - t2):.0f} ms", flush=True)
