"""Times the LocOS Qt Linguist filter (read only) on the file the speed test saves.

    set QTTS_SPEED_OUT=C:/tmp/big.ts & cargo test --release -p bilingua-qtts --test speed
    <local-tms venv python> -I speed_locos.py C:/tmp/big.ts
"""
import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
from app import config  # noqa: E402
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10**6
data = open(sys.argv[1], "rb").read()
f = get_filter("ts")
t0 = time.perf_counter()
pr = f.parse(data)
t1 = time.perf_counter()
tr = {str(u.p_index): "\u00ab" + u.text + "\u00bb" for u in pr.units}
t2 = time.perf_counter()
out = f.build_localized(data, tr, "pl-PL")
t3 = time.perf_counter()
print(f"locos: {len(data)} bytes, {len(pr.units)} units; parse {t1 - t0:.2f}s, build_localized {t3 - t2:.2f}s; total {t3 - t0:.2f}s")
