"""Writes the LocOS-profile goldens (Track D): run with the Localization OS venv Python, read-only.

    <local-tms>/.venv/Scripts/python.exe oracle/locos_golden.py [FORMAT ...]

For every input under `bilingua/tests/locos/<format>/` (any file not ending in `.golden.json`), LocOS's filter
parses it and builds it under a few mappings; the result goes next to the input as `<file>.golden.json`:

    {"format": "txt", "units": [{"p_index": 0, "text": "...", "style": ""}],
     "builds": [{"name": "none", "locale": null, "map": {}, "out_b64": "..."}, ...]}

A build of a zip package (docx, pptx, xlsx, the XLIFF packages) is written per entry instead of `out_b64`, in the
order of LocOS's archive: `"zip": [{"name": "word/document.xml", "sha1": "..."}, {"name": "[Content_Types].xml",
"same": true}, ...]`, where `same` means the entry's bytes are those of the input entry of that name (so the golden
does not repeat the parts nothing changed) and the bytes of a rewritten entry are the member named by its SHA-1 in
the sidecar `<file>.golden.zip` (deflated, shared by every build of the file). The Rust side compares entries by
name: an untouched entry byte for byte, a rewritten one as canonical XML (zip compression, dates and LocOS's own XML
serialiser are not the point).

A `<file>.maps.json` next to an input lists extra builds made the same way (`[{"name": ..., "locale": ...,
"map": {...}}]`): hostile targets, written by `oracle/gen_locos_pptx.py` for the PPTX decks it makes.

`bilingua/tests/locos_golden.rs` reads them, extracts the same file through the facade under the LocOS profile,
turns the units into LocOS text with `profile::tokens` (as the adapter does) and compares, then sets each mapping
as targets and compares the merged bytes. Nothing in the LocOS tree is written; its data dir is a throwaway.
"""

import base64
import hashlib
import json
import os
import re
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(os.path.dirname(HERE), "bilingua", "tests", "locos")
LOCOS = os.environ.get("LOCOS_DIR") or r"C:\Users\brent\code\local-tms"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-golden-")
os.environ.pop("LOC_TMS_BILINGUA_FORMATS", None)
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

TOKEN = re.compile("(\u27e6/?[a-z]?\\d+\u27e7)")
# Locales the localized builds use, per format (default: de-DE). PO also tries a CLDR plural language.
LOCALES = {"po": ["de-DE", "pl-PL", "ar-EG", "ja-JP", "tl", "id-ID", "ta"], "vtt": ["de-DE", "fr"], "arb": ["de-DE", "pl-PL"], "ftl": ["de-DE", "pl-PL"], "json": ["de-DE", "pl-PL", "ar-EG", "ja-JP"]}
# Track D group B: the Office formats also build for a right-to-left complex-script target (w:bCs, w:rtl,
# rightToLeft); the XLIFF family also for a legacy underscore locale (LocOS declares `es-ES`).
for _fmt in ("docx", "pptx", "xlsx"):
    LOCALES[_fmt] = ["de-DE", "ar-SA"]
for _fmt in ("xliff", "sdlxliff", "mqxliff", "mxliff"):
    LOCALES[_fmt] = ["de-DE", "es_ES"]


def zip_entries(data: bytes):
    """The entries of a zip as (name, bytes) in archive order, or None when `data` is not a zip."""
    if not data.startswith(b"PK"):
        return None
    try:
        import io

        with zipfile.ZipFile(io.BytesIO(data)) as zf:
            return [(i.filename, zf.read(i)) for i in zf.infolist() if not i.is_dir()]
    except (zipfile.BadZipFile, ValueError, OSError):
        return None


def pseudo(text: str, tag: str) -> str:
    """The text with every run outside the tokens changed, tokens kept in place."""
    parts = TOKEN.split(text)
    out = []
    for i, p in enumerate(parts):
        if i % 2 == 1 or not p.strip():
            out.append(p)
        elif tag == "prefix":
            out.append("\u00dc " + p)
        else:
            out.append(p + " & <\"'>")
    return "".join(out)


def golden(fmt: str, path: str, blobs: dict | None = None) -> dict:
    with open(path, "rb") as fh:
        data = fh.read()
    filt = get_filter(fmt)
    pr = filt.parse(data)
    units = [{"p_index": u.p_index, "text": u.text, "style": u.style or ""} for u in pr.units]
    builds = []
    original = dict(zip_entries(data) or [])
    blobs = blobs if blobs is not None else {}

    def add(name, locale, mapping):
        try:
            if locale is None:
                out = filt.build(data, mapping)
            else:
                out = filt.build_localized(data, mapping, locale)
            entries = zip_entries(out) if original else None
            if entries is None:
                builds.append({"name": name, "locale": locale, "map": mapping, "out_b64": base64.b64encode(out).decode()})
            else:
                z = []
                for n, b in entries:
                    if original.get(n) == b:
                        z.append({"name": n, "same": True})
                    else:
                        sha = hashlib.sha1(b).hexdigest()
                        blobs[sha] = b
                        z.append({"name": n, "sha1": sha})
                builds.append({"name": name, "locale": locale, "map": mapping, "zip": z})
        except Exception as exc:  # recorded, the Rust side skips it
            builds.append({"name": name, "locale": locale, "map": mapping, "error": f"{type(exc).__name__}: {exc}"})

    add("none", None, {})
    add("prefix", None, {str(u["p_index"]): pseudo(u["text"], "prefix") for u in units})
    for loc in LOCALES.get(fmt, ["de-DE"]):
        half = {str(u["p_index"]): pseudo(u["text"], "escape") for k, u in enumerate(units) if k % 2 == 0}
        add(f"escape-{loc}", loc, half)
        add(f"none-{loc}", loc, {})
    # hostile mappings of a file, when it has a `<file>.maps.json` (a list of {"name", "locale", "map"})
    if os.path.exists(path + ".maps.json"):
        with open(path + ".maps.json", encoding="utf-8") as fh:
            for m in json.load(fh):
                add(m["name"], m["locale"], m["map"])
    return {"format": fmt, "units": units, "builds": builds}


def main() -> None:
    want = set(sys.argv[1:])
    for fmt in sorted(os.listdir(ROOT)):
        d = os.path.join(ROOT, fmt)
        if not os.path.isdir(d) or (want and fmt not in want):
            continue
        for name in sorted(os.listdir(d)):
            if name.endswith((".golden.json", ".golden.zip", ".maps.json")):
                continue
            blobs: dict = {}
            g = golden(fmt, os.path.join(d, name), blobs)
            with open(os.path.join(d, name + ".golden.json"), "w", encoding="utf-8", newline="\n") as fh:
                json.dump(g, fh, ensure_ascii=False, indent=1)
                fh.write("\n")
            side = os.path.join(d, name + ".golden.zip")
            if blobs:
                with zipfile.ZipFile(side, "w") as zf:
                    for sha in sorted(blobs):
                        zf.writestr(zipfile.ZipInfo(sha, (1980, 1, 1, 0, 0, 0)), blobs[sha], zipfile.ZIP_DEFLATED, 9)
            elif os.path.exists(side):
                os.remove(side)
            print(fmt, name, len(g["units"]), "units", sum("error" in b for b in g["builds"]), "build errors")


if __name__ == "__main__":
    main()
