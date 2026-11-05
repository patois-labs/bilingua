r"""Times the LocOS wiki filters on the large files the Rust speed test writes (read only use of LocOS).

    mkdir out && BILINGUA_WIKI_SPEED_OUT=out cargo test -p bilingua-wiki --release --test speed -- --nocapture
    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe speed_locos.py out/speed.wiki out/speed.dokuwiki
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-wiki-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000
config.MAX_SEGMENT_CHARS = 1_000_000

for path in sys.argv[1:]:
    data = open(path, "rb").read()
    f = get_filter("dokuwiki" if path.endswith(".dokuwiki") else "wiki")
    t0 = time.perf_counter()
    pr = f.parse(data)
    t1 = time.perf_counter()
    mapping = {str(i): "T%d %s" % (i, u.text) for i, u in enumerate(pr.units)}
    out = f.build(data, mapping)
    t2 = time.perf_counter()
    assert f.build(data, {}) == data
    t3 = time.perf_counter()
    print(f"LocOS {os.path.basename(path)}: {len(data) / 1e6:.1f} MB, {len(pr.units)} units, parse {1e3 * (t1 - t0):.0f} ms, "
          f"build {1e3 * (t2 - t1):.0f} ms, untouched build {1e3 * (t3 - t2):.0f} ms, parse+build {1e3 * (t2 - t0):.0f} ms")
