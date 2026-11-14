"""Times the Localization OS PDF filter on the speed file written by `PDF_SPEED_OUT=... cargo test --test speed`.

    python -I speed_locos.py FILE      (the LocOS venv Python)
"""

import os
import sys
import tempfile
import time

os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="bilingua-pdf-speed-")
sys.dont_write_bytecode = True
sys.path.insert(0, os.environ.get("LOCOS_DIR", os.path.join("C:" + os.sep, "Users", "brent", "code", "local-tms")))

from app.filters.pdf_filter import PdfFilter  # noqa: E402

data = open(sys.argv[1], "rb").read()
t = time.perf_counter()
res = PdfFilter().parse(data)
took = time.perf_counter() - t
print(f"locos pdf: {len(data)} bytes, {len(res.units)} units, {sum(len(u.text) for u in res.units)} chars; parse {took:.3f} s")
