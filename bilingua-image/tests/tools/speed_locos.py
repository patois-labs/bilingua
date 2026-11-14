r"""Times the LocOS image filter (SVG parse and build) on a file made by the Rust speed test.

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe speed_locos.py FILE.svg

Read-only use of the LocOS tree; mirrors the Rust test: every unit translated into Arabic, target ar-SA (rtl).
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR") or r"C:\Users\brent\code\local-tms"
path = os.path.abspath(sys.argv[1])
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-speed-")
os.environ["MAX_DOCUMENT_SEGMENTS"] = "10000000"
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
os.chdir(LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

f = get_filter("svg")
data = open(path, "rb").read()
t0 = time.perf_counter()
res = f._parse_svg(data)
t_parse = time.perf_counter() - t0
n = len(res.units)
mapping = {str(i): "\u0645\u0631\u062d\u0628\u0627 %d & x" % i for i in range(n)}
t1 = time.perf_counter()
out = f.build_localized(data, mapping, "ar-SA")
t_build = time.perf_counter() - t1
t2 = time.perf_counter()
out2 = f.build(data, mapping)
t_plain = time.perf_counter() - t2
print(f"locos svg speed: {len(data) / 1e6:.1f} MB, {n} units, parse {t_parse * 1e3:.0f} ms, "
      f"build_localized (ar-SA, rtl) {t_build * 1e3:.0f} ms, plain build {t_plain * 1e3:.0f} ms, parse+build {(t_parse + t_build) * 1e3:.0f} ms")
