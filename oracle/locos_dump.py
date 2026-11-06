"""Run with the Localization OS venv Python (read-only use of its filters).

    python locos_dump.py OUT.json FILE [FILE ...]   (or @LIST: a file naming one path per line)

Writes {file: {"units": [text, ...]} or {"error": ...}} where text is the unit text as LocOS makes it, with
inline-code tokens left in place. Nothing in the LocOS tree is written; the data dir is a throwaway.
"""

import json
import os
import sys
import tempfile

# The Localization OS checkout (the oracle). Default: a `local-tms` folder next to the repository.
LOCOS = os.environ.get("LOCOS_DIR") or next(
    (os.path.join(up, "local-tms") for up in [os.path.dirname(os.path.abspath(__file__))] + [
        os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), *[".."] * n)) for n in range(1, 8)]
     if os.path.isdir(os.path.join(up, "local-tms", "app", "filters"))), "local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-oracle-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

EXT = {"docm": "docx", "dotx": "docx", "dotm": "docx", "htm": "html", "markdown": "md", "text": "txt"}


def inventory(out_path: str) -> None:
    """Every format the registry knows: canonical name, filter class and the extensions that resolve to it."""
    from app.filters import base

    rows = []
    for fmt in base.supported_formats():
        flt = base.get_filter(fmt)
        exts = {k for k, v in base._ALIASES.items() if v == fmt} | {k for k, v in base._LAZY.items() if v[1] == fmt}
        exts.add(fmt)
        rows.append({"fmt": fmt, "cls": type(flt).__name__ if flt else None, "exts": sorted(exts)})
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(rows, fh)


def dialect(fmt: str, data: bytes, pr) -> str:
    """The LocOS dialect a file was read as, when the registry format has several chosen by content: `.xml` by
    root element (Android strings, Java XML properties, ITS / DITA / DocBook, multilingual), `.json` by shape
    (chrome, formatjs, structured, go, arb), Storyline bilingual tables in `.docx`, Mozilla `.properties`, and
    `.ts` (Qt Linguist or a TypeScript module). "" = the format's plain reading."""
    sk = pr.skeleton if isinstance(pr.skeleton, dict) else {}
    try:
        if fmt == "xml":
            from app.filters.xml_resource import pick

            name = type(pick(data)).__name__
            if name == "AndroidStringsFilter":
                return "android"
            if name == "JavaXmlPropertiesFilter":
                return "java-xml-properties"
            d = sk.get("dialect") or "its"
            return "" if d == "its" else d
        if fmt == "json":
            d = sk.get("dialect") or "plain"
            return "" if d == "plain" else d
        if fmt == "docx":
            return "storyline-table" if "docx_bilingual_table" in json.dumps(sk.get("warnings", sk.get("file_warnings", "")), default=str) else ""
        if fmt == "properties":
            from app.filters import mozilla_properties

            return "mozilla" if mozilla_properties.looks_mozilla(data.decode("utf-8", "replace")) else ""
        if fmt == "js" and data.lstrip().startswith((b"<?xml", b"<TS", b"<!DOCTYPE TS")):
            return "qt"
    except Exception:  # noqa: BLE001
        return ""
    return ""


def main() -> None:
    if sys.argv[1] == "--inventory":
        inventory(sys.argv[2])
        return
    out_path, files = sys.argv[1], sys.argv[2:]
    if len(files) == 1 and files[0].startswith("@"):  # a list file, one path per line
        with open(files[0][1:], encoding="utf-8") as fh:
            files = [ln for ln in fh.read().splitlines() if ln]
    res = {}
    for f in files:
        ext = f.rsplit(".", 1)[-1].lower()
        ext = EXT.get(ext, ext)
        try:
            filt = get_filter(ext)
            if filt is None:
                res[f] = {"error": f"no LocOS filter for {ext}"}
                continue
            with open(f, "rb") as fh:
                data = fh.read()
            pr = filt.parse(data)
            opened = None
            try:
                if ext == "docx":
                    import docx

                    docx.Document(f)
                elif ext in ("xlsx", "xlsm"):
                    import warnings

                    import openpyxl

                    with warnings.catch_warnings():
                        warnings.simplefilter("ignore")
                        openpyxl.load_workbook(f).close()
            except Exception as e:  # noqa: BLE001
                opened = f"{type(e).__name__}: {str(e)[:150]}"
            res[f] = {"filter": filt.fmt, "dialect": dialect(filt.fmt, data, pr), "open_error": opened, "units": [
                {"i": u.p_index, "text": u.text, "style": u.style} for u in pr.units]}
        except Exception as e:  # noqa: BLE001
            res[f] = {"error": f"{type(e).__name__}: {str(e)[:200]}"}
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(res, fh, ensure_ascii=False)


main()
