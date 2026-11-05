"""Times the LocOS ODF filter on the file tests/speed.rs writes (ODF_SPEED_OUT). Read-only use of LocOS.

    python tools/time_locos.py LOCOS_DIR SPEED_DIR
"""
import os, sys, tempfile, time

locos, d = sys.argv[1], sys.argv[2]
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, locos)
from app import config  # noqa: E402
from app.filters.base import get_filter  # noqa: E402
from app import filters as _f  # noqa: E402,F401

data = open(os.path.join(d, "big.odt"), "rb").read()
f = get_filter("odt")
t = time.perf_counter(); r = f.parse(data); t1 = time.perf_counter() - t
tr = {str(u.p_index): "\u00ab" + u.text + "\u00bb" for u in r.units}
t = time.perf_counter(); out = f.build_localized(data, tr, "de-DE"); t2 = time.perf_counter() - t
t = time.perf_counter(); same = f.build(data, {}); t3 = time.perf_counter() - t
print(f"odt: {len(data)} bytes, {len(r.units)} units; parse {t1:.3f}s, build_localized {t2:.3f}s, untouched build {t3:.3f}s, "
      f"parse+build {t1 + t2:.3f}s, identity {same == data}")
