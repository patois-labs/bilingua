"""Times the Localization OS ARB filter on the same generated file that tests/speed.rs times in Rust.

    LOCOS_DIR=<local-tms checkout> <local-tms venv python> -I locos_speed.py [OUT.arb]

Read-only use of LocOS (the data dir is a throwaway). Prints one line per step in seconds. The generator below
must stay in step with `generate()` in speed.rs: same messages, same size.
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

N = 30000  # messages (about 6 MB); the cap is lifted below, as it is a product limit


def generate(n: int = N) -> str:
    out = ['{\n  "@@locale": "en"']
    for i in range(n):
        k = i % 4
        if k == 0:
            m = f"Welcome to Larchmoor {{name}}, your order number {i} is ready for pickup at the harbour bakery counter today."
        elif k == 1:
            m = (f"{{count, plural, =0{{No loaves in basket {i}}} one{{# loaf in basket {i}}} other{{# loaves in basket {i}}}}} "
                 "baked fresh this morning")
        elif k == 2:
            m = f"{{gender, select, female{{She ordered item {i}}} male{{He ordered item {i}}} other{{They ordered item {i}}}}}"
        else:
            m = f"Opening hours for branch {i}: every day from 7 AM to 6 PM, closed on public holidays and Sundays in winter."
        out.append(f',\n  "message{i}": "{m}",\n  "@message{i}": {{"description": "Shown on screen number {i}"}}')
    out.append("\n}\n")
    return "".join(out)


def main() -> None:
    text = generate()
    data = text.encode("utf-8")
    if len(sys.argv) > 1:
        with open(sys.argv[1], "wb") as fh:
            fh.write(data)
    print(f"file: {len(data)} bytes, {N} messages")
    config.MAX_DOCUMENT_SEGMENTS = 10**6
    filt = get_filter("arb")
    t = time.perf_counter()
    pr = filt.parse(data)
    t_parse = time.perf_counter() - t
    texts = {str(u.p_index): "T:" + u.text for u in pr.units}
    t = time.perf_counter()
    out = filt.build(data, {})
    t_plain_untouched = time.perf_counter() - t
    t = time.perf_counter()
    out = filt.build_localized(data, texts, "ru")
    t_build = time.perf_counter() - t
    print(f"units: {len(pr.units)}; output {len(out)} bytes")
    print(f"locos parse: {t_parse:.3f} s")
    print(f"locos build (nothing translated): {t_plain_untouched:.3f} s")
    print(f"locos build_localized ru (all translated): {t_build:.3f} s")
    print(f"locos total (parse + untouched build + localized build): {t_parse + t_plain_untouched + t_build:.3f} s")


if __name__ == "__main__":
    main()
