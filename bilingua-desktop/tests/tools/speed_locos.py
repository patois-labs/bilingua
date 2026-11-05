r"""Times the LocOS desktop and catkeys filters on the files `speed.rs` saves (DESKTOP_SPEED_OUT).

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe -I speed_locos.py DIR

Per file: parse, an untouched build, and build_localized with every unit translated (de-DE), the same work the
Rust test times. Read only use of the LocOS filters; the data dir is a throwaway.
"""
import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-desktop-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 1_000_000  # the speed files are within the defaults; no cap needed

for fmt in ("desktop", "catkeys"):
    data = open(os.path.join(sys.argv[1], f"speed.{fmt}"), "rb").read()
    f = get_filter(fmt)
    t0 = time.perf_counter()
    pr = f.parse(data)
    t_parse = time.perf_counter() - t0
    n = len(pr.units)
    t1 = time.perf_counter()
    same = f.build(data, {})
    t_same = time.perf_counter() - t1
    assert same == data
    tr = {str(i): "\u00ab" + u.text + "\u00bb" for i, u in enumerate(pr.units)}
    t2 = time.perf_counter()
    out = f.build_localized(data, tr, "de-DE")
    t_loc = time.perf_counter() - t2
    print(f"{fmt} LocOS: {len(data)} bytes, {n} units; parse {t_parse:.3f}s, untouched build {t_same:.3f}s, "
          f"translated (de-DE) build_localized {t_loc:.3f}s; parse+build {t_parse + t_loc:.3f}s")
