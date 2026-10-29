# bilingua-py

Python wheel for the Bilingua engine (PyO3, abi3, Python 3.11 and up). Module name: `bilingua`. Bytes in, bytes out:
it never touches the file system.

## Build

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

## API

- `formats() -> list[dict]`: `{"id", "extensions", "bilingual"}` per format. `bilingual` is true for formats that
  carry source and target (PO).
- `detect(name: str, data: bytes) -> str | None`: the format id, or `None` when no parser accepts the file.
- `extract(name, data, *, source_lang=None, target_lang=None, granularity="paragraph") -> dict`: the Document as
  dicts and lists, the same shape as its JSON. `granularity` is `sentence`, `paragraph` or `document`.
- `merge(document: dict) -> bytes`: writes the (possibly translated) Document back in its original format. Set a
  segment's `target` to a list of inlines (`{"text": ".."}` or `{"tag": {...}}`); an empty target keeps the source.
- `version() -> str`.

Every failure raises `bilingua.BilinguaError` with a `code` attribute: `parse`, `unsupported`, `unknown_segment`,
`locked`, `invented_tag`, `duplicated_tag`, `write`, `io`, `invalid_argument`, `invalid_document`, `internal`.
Panics inside the engine become `internal`. The GIL is released while parsing and merging, so threads run in
parallel. The Document is converted through its serde JSON form, so new model fields appear in Python with no change
here.

Notes: with no targets set, `merge` returns the same bytes for text formats. For zip formats (DOCX, XLSX, PPTX) the
entries and their contents are identical but the compressed bytes may differ.

## Example: a LocOS-style filter (example only, not the real adapter)

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

Preview (`preview`, `render_segment`) and `resegment` are not exposed. The `bilingual` flag is a fixed list in this
crate until the model reports it. Linux and macOS wheels are untested here (built on Windows only).
