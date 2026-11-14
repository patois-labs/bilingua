"""Times the LocOS email filter on a message made by `cargo test --release -p bilingua-email --test speed` with
EMAIL_SPEED_OUT set.

    <LocOS venv python> -B speed_locos.py FILE.eml

Prints seconds for parse, an untouched build, and a build with every unit translated (read only use of LocOS).
"""

import os
import sys
import tempfile
import time

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-email-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

EML = get_filter("eml")
data = open(sys.argv[1], "rb").read()
t = time.perf_counter()
res = EML.parse(data)
t_parse = time.perf_counter() - t
n = len(res.units)
t = time.perf_counter()
same = EML.build(data, {})
t_same = time.perf_counter() - t
assert same == data
texts = {str(i): "\u00ab" + u.text + "\u00bb" for i, u in enumerate(res.units)}
t = time.perf_counter()
out = EML.build(data, texts)
t_build = time.perf_counter() - t
print(f"locos email speed: {len(data)} bytes, {n} units; parse {t_parse:.2f}s, untouched build {t_same:.2f}s, translated build {t_build:.2f}s; parse+build {t_parse + t_build:.2f}s")
