"""Time the LocOS TOML filter on a file made by `cargo run --release -p bilingua-toml --example speed -- FILE`.
Run with the LocOS venv python (read-only use of LocOS)."""
import os, sys, tempfile, time
LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-bench-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
from app import config
config.MAX_DOCUMENT_SEGMENTS = 10_000_000  # the bench file has more strings than the default cap
config.MAX_SEGMENT_CHARS = 100_000
from app.filters import get_filter
data = open(sys.argv[1], "rb").read()
f = get_filter("toml")
t = time.perf_counter(); r = f.parse(data); t1 = time.perf_counter() - t
print(f"locos parse: {t1:.3f}s ({len(r.units)} units)")
t = time.perf_counter(); f.build(data, {}); print(f"locos build untouched: {time.perf_counter() - t:.3f}s")
tr = {str(u.p_index): "DE " + u.text for u in r.units}
t = time.perf_counter(); f.build(data, tr); print(f"locos build translated: {time.perf_counter() - t:.3f}s")
