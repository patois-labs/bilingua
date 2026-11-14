"""Times the LocOS xcstrings and xcloc filters on the files tests/speed.rs writes (XC_SPEED_OUT). Read-only use of LocOS.

    python tools/time_locos.py LOCOS_DIR SPEED_DIR
"""
import os, sys, tempfile, time

locos, d = sys.argv[1], sys.argv[2]
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, locos)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

for ext, loc in (("xcstrings", "ru"), ("xcloc", "de")):
    data = open(os.path.join(d, "big." + ext), "rb").read()
    f = get_filter(ext)
    t = time.perf_counter(); r = f.parse(data); t1 = time.perf_counter() - t
    tr = {str(u.p_index): "\u00ab" + u.text + "\u00bb" for u in r.units}
    t = time.perf_counter(); out = f.build_localized(data, tr, loc); t2 = time.perf_counter() - t
    t = time.perf_counter(); same = f.build(data, {}); t3 = time.perf_counter() - t
    print(f"{ext}: {len(data)} bytes, {len(r.units)} units; parse {t1:.3f}s, build_localized {t2:.3f}s, untouched build {t3:.3f}s, identity {same == data}")
