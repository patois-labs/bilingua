"""Writes the LocOS golden files used by tests/golden.rs. Run with the LocOS venv python (read only use):

    python tools/gen_golden.py LOCOS_DIR OUT_DIR CATALOG [CATALOG ...]
"""
import json, os, sys, tempfile

locos, out_dir, catalogs = sys.argv[1], sys.argv[2], sys.argv[3:]
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, locos)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

PREFIX = {"de": "[de] ", "fr": "[fr] ", "es": "[es] ", "ru": "\u043f\u0435\u0440\u0435\u0432\u043e\u0434 ", "pl": "[pl] ",
          "ar": "\u062a\u0631\u062c\u0645\u0629 ", "ja": "\u7ffb\u8a33 ", "zh-Hans": "\u7ffb\u8bd1 ",
          "ko": "\ubc88\uc5ed ", "pt_BR": "[pt] ", "tr": "[tr] "}
f = get_filter("xcstrings")
for cat in catalogs:
    data = open(cat, "rb").read()
    name = os.path.basename(cat).split(".")[0]
    texts = [u.text for u in f.parse(data).units]
    for loc in list(PREFIX) + [None]:
        for mode in ("full", "half"):
            pre = PREFIX.get(loc, "[src] ")
            tr = {str(i): pre + t for i, t in enumerate(texts) if mode == "full" or i % 2 == 0}
            out = f.build_localized(data, tr, loc) if loc else f.build(data, tr)
            fn = f"{name}.{mode}.{loc or 'source'}.xcstrings"
            open(os.path.join(out_dir, fn), "wb").write(out)
            print(fn, len(out), out == data)
