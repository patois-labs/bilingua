r"""Times the LocOS CSV filter on the large file the Rust speed test writes (read only use of LocOS).

    BILINGUA_CSV_SPEED_OUT=big.csv cargo test -p bilingua-csv --release --test speed -- --nocapture
    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe speed_locos.py big.csv
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-csv-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000

data = open(sys.argv[1], "rb").read()
f = get_filter("csv")
t0 = time.perf_counter()
pr = f.parse(data, source_locale="en", target_locales=["de"])
t1 = time.perf_counter()
mapping = {str(i): 'T%d, "q" translated' % i for i in range(len(pr.units))}
out = f.build_localized(data, mapping, "de", table=pr.skeleton["table"])
t2 = time.perf_counter()
assert f.build_localized(data, {}, "de", table=pr.skeleton["table"]) == data
t3 = time.perf_counter()
print(f"LocOS csv: {len(data) / 1e6:.1f} MB, {len(pr.units)} units, parse {1e3 * (t1 - t0):.0f} ms, "
      f"build {1e3 * (t2 - t1):.0f} ms, untouched build {1e3 * (t3 - t2):.0f} ms, parse+build {1e3 * (t2 - t0):.0f} ms")
