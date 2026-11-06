# bilingua (Python)

Python bindings for [Bilingua](https://github.com/patois-labs/bilingua), the file parsers for translation: open a
source file in any of 88 formats (Word, Excel, PowerPoint, PO, XLIFF, subtitles, software strings and more), get its
translatable text as segments with the formatting kept as inline tags, set the translations, and get the file back
in its original format. Bytes in, bytes out: it never touches the file system.

    pip install bilingua

One wheel (`cp311-abi3`) works for every Python from 3.11 on. Type stubs are included.

```python
import bilingua

data = open("messages.po", "rb").read()
doc = bilingua.extract("messages.po", data, source_lang="en", target_lang="es", granularity="sentence")
for unit in doc["units"]:
    if not unit["translate"]:
        continue
    for seg in unit["segments"]:
        print(seg["id"], seg["source"])
        seg["target"] = [{"text": "..."}]      # a list of inlines: {"text": ...} or {"tag": {...}}
open("messages.es.po", "wb").write(bilingua.merge(doc))
```

## API

- `formats() -> list[dict]`: `{"id", "extensions", "bilingual"}` per format. `bilingual` is true for formats that
  carry source and target (PO).
- `detect(name: str, data: bytes) -> str | None`: the format id, or `None` when no parser accepts the file.
- `extract(name, data, *, source_lang=None, target_lang=None, granularity="paragraph") -> dict`: the Document as
  dicts and lists, the same shape as its JSON. `granularity` is `sentence`, `paragraph` or `document`.
- `merge(document: dict, *, output_encoding="keep", stamp_target_always=False) -> bytes`: writes the (possibly
  translated) Document back in its original format. Set a segment's `target` to a list of inlines (`{"text": ".."}`
  or `{"tag": {...}}`); an empty target keeps the source, unless the segment has `"empty_target": true` (translated
  to nothing on purpose: written empty). Merge options are keyword-only: `output_encoding` (`"keep"` or `"utf-8"`,
  which writes UTF-8 and makes charset declarations say so) and `stamp_target_always` (stamp the target language and
  fill plurals even when nothing is translated, as Localization OS does).
- `extract(..., format_options=None)`: a dict of `"<format>.<name>"` options to str, bool or int values, e.g.
  `{"ftl.translate_terms": True, "csv.columns": "source=1, target.de=2"}`. A key no format declares, or a value of the
  wrong kind, raises `invalid_argument`.
- `extract(..., limits=None)`: a dict of limit names to override (see `bilingua-model/README.md`); the rest keep the
  defaults. The Document has a `warnings` list (e.g. `text_encoding`) when a file was read but looks garbled.
- Type stubs: `bilingua.pyi` ships in the wheel (with `py.typed`), so editors and type checkers see the API.
- `version() -> str`.

Every failure raises `bilingua.BilinguaError` with a `code` attribute: `parse`, `unsupported`, `unknown_segment`,
`locked`, `invented_tag`, `duplicated_tag`, `parser_version`, `write`, `unwritable` (the translation cannot be written in this format), `limit` (over a safety limit or unsafe XML/zip), `encoding` (unreadable text), `io`, `invalid_argument`, `invalid_document`, `internal`.
Panics inside the engine become `internal`. The GIL is released while parsing and merging, so threads run in
parallel. The Document is converted through its serde JSON form, so new model fields appear in Python with no change
here.

Notes: with no targets set, `merge` returns the same bytes for text formats. For zip formats (DOCX, XLSX, PPTX) the
entries and their contents are identical but the compressed bytes may differ.

## Example: a filter adapter in the Localization OS style (an example, not the real adapter)

    import bilingua

    def parse(name, data, source_lang, target_lang):
        doc = bilingua.extract(name, data, source_lang=source_lang, target_lang=target_lang, granularity="sentence")
        segs = [(s["id"], s["source"]) for u in doc["units"] if u["translate"] for s in u["segments"]]
        return doc, segs

    def build(doc, translations):  # translations: {segment id: list of inlines}
        for u in doc["units"]:
            for s in u["segments"]:
                s["target"] = translations.get(s["id"], s["target"])
        return bilingua.merge(doc)

## Not done

Preview (`preview`, `render_segment`) and `resegment` are not exposed. `bilingual` comes from each format's `Filter::bilingual()`. Wheels are built for Windows x64 and Linux x86_64 (manylinux); on other platforms pip builds from the source
distribution, which needs a Rust toolchain (1.88 or later).

## Building from source

Use a venv of your own, never a shared one.

Windows (PowerShell):

    cd bilingua\bilingua-py
    py -3.11 -m venv .venv
    .venv\Scripts\python -m pip install maturin pytest
    .venv\Scripts\maturin develop --release     # installs into the venv
    .venv\Scripts\maturin build --release -o dist   # builds dist\bilingua-*.whl
    .venv\Scripts\python -m pytest

Linux:

    cd bilingua/bilingua-py
    python3 -m venv .venv
    .venv/bin/python -m pip install maturin pytest
    .venv/bin/maturin develop --release
    .venv/bin/maturin build --release -o dist
    .venv/bin/python -m pytest

One wheel (`cp311-abi3`) works for every Python from 3.11 on. Plain `cargo test` and `cargo clippy` in `bilingua/`
also cover this crate (they need a Python install to link against).
