"""Tests for the bilingua wheel over the Bilingua oracle corpus. Bytes in, bytes out: nothing here writes files."""
import copy
import io
import zipfile
import json
import pathlib
import threading
import time

import pytest

import bilingua

CORPUS = pathlib.Path(__file__).resolve().parents[2] / "oracle" / "corpus"
FILES = sorted(p for p in CORPUS.rglob("*") if p.is_file() and p.name != "SOURCES.md")
IDS = [f"{p.parent.name}/{p.name}" for p in FILES]


def pseudo(doc, wrap=True):
    """Sets every translatable segment's target to its source in brackets, tags kept. Returns the count."""
    n = 0
    for unit in doc["units"]:
        if not unit["translate"]:
            continue
        for seg in unit["segments"]:
            t = [dict(i) for i in seg["source"]]
            if wrap:
                t = [{"text": "["}] + t + [{"text": "]"}]
            seg["target"] = t
            n += 1
    return n


def texts(doc):
    return [
        "".join(i["text"] for i in seg["source"] if "text" in i)
        for u in doc["units"]
        for seg in u["segments"]
    ]


def same_file(a, b):
    """Identical bytes; for zip containers, the same entries in the same order with the same content (the
    compressed bytes may differ, because the writer recompresses)."""
    if a == b:
        return True
    if not (a[:2] == b"PK" and b[:2] == b"PK"):
        return False
    za, zb = zipfile.ZipFile(io.BytesIO(a)), zipfile.ZipFile(io.BytesIO(b))
    if za.namelist() != zb.namelist():
        return False
    return all(za.read(n) == zb.read(n) for n in za.namelist())


def error_code(fn, *a, **k):
    with pytest.raises(bilingua.BilinguaError) as e:
        fn(*a, **k)
    return e.value.code


def test_corpus_found():
    assert len(FILES) >= 30


def test_version_and_formats():
    assert bilingua.version() == bilingua.__version__
    fmts = bilingua.formats()
    ids = {f["id"] for f in fmts}
    assert {"docx", "xlsx", "pptx", "html", "po", "srt", "vtt"} <= ids
    for f in fmts:
        assert isinstance(f["extensions"], list) and f["extensions"]
        assert isinstance(f["bilingual"], bool)
    assert next(f for f in fmts if f["id"] == "po")["bilingual"] is True
    assert next(f for f in fmts if f["id"] == "docx")["bilingual"] is False


@pytest.mark.parametrize("path", FILES, ids=IDS)
def test_detect(path):
    assert bilingua.detect(path.name, path.read_bytes()) is not None


@pytest.mark.parametrize("granularity", ["paragraph", "sentence", "document"])
@pytest.mark.parametrize("path", FILES, ids=IDS)
def test_untranslated_roundtrip_is_identical(path, granularity):
    data = path.read_bytes()
    doc = bilingua.extract(path.name, data, granularity=granularity)
    assert isinstance(doc, dict) and doc["format"] == bilingua.detect(path.name, data)
    assert same_file(bilingua.merge(doc), data)


@pytest.mark.parametrize("path", FILES, ids=IDS)
def test_pseudo_translation_roundtrip(path):
    data = path.read_bytes()
    doc = bilingua.extract(path.name, data, source_lang="en", target_lang="xx")
    n = pseudo(doc)
    out = bilingua.merge(doc)
    assert isinstance(out, bytes)
    if n == 0:
        assert same_file(out, data)
        return
    assert not same_file(out, data)
    assert len(bilingua.extract(path.name, out)["units"]) >= 1


@pytest.mark.parametrize("name", ["menu.xlsx", "launch.pptx", "manual.docx", "languages.docx"])
def test_pseudo_text_is_in_zip_output(name):
    path = next(p for p in FILES if p.name == name)
    doc = bilingua.extract(name, path.read_bytes())
    src = texts(doc)
    pseudo(doc)
    got = texts(bilingua.extract(name, bilingua.merge(doc)))
    assert len(got) == len(src)
    for s, g in zip(src, got):
        if s.strip():
            assert g == "[" + s + "]"


def test_arabic_and_japanese_text():
    ar = "مرحبا بالعالم. هذا اختبار.\n"
    ja = "こんにちは世界。これはテストです。\n"
    for name, text in [("a.txt", ar), ("j.txt", ja)]:
        doc = bilingua.extract(name, text.encode("utf-8"))
        assert texts(doc) == [text.strip()]
        assert bilingua.merge(doc) == text.encode("utf-8")
        doc["units"][0]["segments"][0]["target"] = [{"text": "traducido " + text.strip()}]
        assert bilingua.merge(doc).decode("utf-8") == "traducido " + text


def test_japanese_and_arabic_in_json():
    data = json.dumps({"greet": "こんにちは。お元気ですか。", "ar": "مرحبا. كيف حالك؟"}, ensure_ascii=False).encode()
    doc = bilingua.extract("s.json", data, source_lang="ja", granularity="sentence")
    assert doc["source_lang"] == "ja"
    assert bilingua.merge(copy.deepcopy(doc)) == data
    pseudo(doc)
    out = json.loads(bilingua.merge(doc))
    assert out["greet"].startswith("[") and "お元気ですか" in out["greet"]
    assert out["ar"].startswith("[") and "كيف حالك" in out["ar"]


def test_corpus_languages_docx_has_cjk_and_arabic():
    path = next(p for p in FILES if p.name == "languages.docx")
    joined = "".join(texts(bilingua.extract(path.name, path.read_bytes())))
    assert any("؀" <= c <= "ۿ" for c in joined)
    assert any("぀" <= c <= "ヿ" or "一" <= c <= "鿿" for c in joined)


def test_options_flow_through():
    doc = bilingua.extract("a.txt", b"One. Two.\n", source_lang="en", target_lang="es-ES", granularity="sentence")
    assert doc["source_lang"] == "en" and doc["target_lang"] == "es-ES"
    assert doc["granularity"] == "sentence"
    assert len(doc["units"][0]["segments"]) == 2
    assert bilingua.extract("a.txt", b"One. Two.\n")["granularity"] == "paragraph"


def test_error_is_an_exception_with_code():
    assert issubclass(bilingua.BilinguaError, Exception)
    assert error_code(bilingua.extract, "notes.xyz", b"a: 1\n") == "unsupported"


def test_bad_inputs_give_parse_errors():
    assert error_code(bilingua.extract, "x.docx", b"this is not a zip") == "parse"
    assert error_code(bilingua.extract, "x.json", b"{not json") == "parse"


def test_bad_granularity():
    assert error_code(bilingua.extract, "a.txt", b"hi", granularity="word") == "invalid_argument"


def test_detect_unknown_is_none():
    assert bilingua.detect("x.xyz", b"a: 1") is None


def test_wrong_types():
    with pytest.raises(TypeError):
        bilingua.extract("a.txt", "not bytes")
    with pytest.raises(TypeError):
        bilingua.detect(1, b"")


def test_merge_invalid_documents():
    for bad in [{}, {"format": 3}, {1: 2}, object(), {"x": float("nan")}]:
        assert error_code(bilingua.merge, bad) == "invalid_document"
    deep = cur = []
    for _ in range(500):
        nxt = []
        cur.append(nxt)
        cur = nxt
    assert error_code(bilingua.merge, deep) == "invalid_document"


def test_merge_unknown_format():
    doc = bilingua.extract("a.txt", b"hi\n")
    doc["format"] = "nope"
    assert error_code(bilingua.merge, doc) == "unsupported"


def test_locked_and_invented_tag_errors_do_not_panic():
    doc = bilingua.extract("a.html", b"<p>Hello <b>world</b></p>")
    doc["units"][0]["segments"][0]["target"] = [{"tag": {"id": "99", "kind": "open", "display": "<i>"}}]
    try:
        bilingua.merge(doc)
    except bilingua.BilinguaError as e:
        assert e.code


def test_empty_and_odd_inputs_do_not_crash():
    for name in ["a.txt", "a.json", "a.html", "a.md", "a.srt", "a.vtt", "a.po", "a.docx", "a.xlsx", "a.pptx"]:
        for data in [b"", b"\xff\xfe\x00", b"\x00" * 64, "😀".encode()]:
            try:
                bilingua.merge(bilingua.extract(name, data))
            except bilingua.BilinguaError as e:
                assert e.code


def test_truncated_zip_does_not_crash():
    data = next(p for p in FILES if p.name == "manual.docx").read_bytes()
    for cut in (10, len(data) // 2, len(data) - 5):
        try:
            bilingua.extract("m.docx", data[:cut])
        except bilingua.BilinguaError as e:
            assert e.code


def test_tuple_in_document_accepted():
    doc = bilingua.extract("a.txt", b"hi\n")
    doc["units"] = tuple(doc["units"])
    assert bilingua.merge(doc) == b"hi\n"


def test_threads_run_in_parallel_safely():
    data = next(p for p in FILES if p.name == "manual.docx").read_bytes()
    results = []

    def work():
        for _ in range(5):
            results.append(same_file(bilingua.merge(bilingua.extract("m.docx", data)), data))

    ts = [threading.Thread(target=work) for _ in range(4)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    assert results == [True] * 20


def test_no_file_system_access(monkeypatch):
    def boom(*a, **k):
        raise AssertionError("file system used")

    monkeypatch.setattr("builtins.open", boom)
    doc = bilingua.extract("a.txt", b"hello\n")
    assert bilingua.merge(doc) == b"hello\n"


def test_timing_largest_docx(capsys):
    path = max((p for p in FILES if p.suffix == ".docx"), key=lambda p: p.stat().st_size)
    data = path.read_bytes()
    t0 = time.perf_counter()
    doc = bilingua.extract(path.name, data)
    t1 = time.perf_counter()
    out = bilingua.merge(doc)
    t2 = time.perf_counter()
    assert same_file(out, data)
    with capsys.disabled():
        print(f"\n[timing] {path.name} {len(data)} bytes: extract {(t1 - t0) * 1000:.1f} ms, merge {(t2 - t1) * 1000:.1f} ms")


def zip_doc():
    path = next(p for p in FILES if p.name == "manual.docx")
    data = path.read_bytes()
    return bilingua.extract(path.name, data), data


def test_skeleton_carries_parser_and_base64_zip_bytes():
    doc, _ = zip_doc()
    skel = doc["skeleton"]["zip"]
    assert skel["parser"]["name"] == "bilingua-docx"
    raws = [e["data"]["raw"] for e in skel["entries"] if "raw" in e["data"]]
    assert raws and all(isinstance(r, str) for r in raws)


def test_old_skeleton_forms_still_merge():
    doc, data = zip_doc()
    import base64

    old = copy.deepcopy(doc)
    del old["skeleton"]["zip"]["parser"]
    for e in old["skeleton"]["zip"]["entries"]:
        if "raw" in e["data"]:
            e["data"]["raw"] = list(base64.b64decode(e["data"]["raw"]))
    assert bilingua.merge(old) == bilingua.merge(doc)
    as_bytes = copy.deepcopy(doc)
    for e in as_bytes["skeleton"]["zip"]["entries"]:
        if "raw" in e["data"]:
            e["data"]["raw"] = base64.b64decode(e["data"]["raw"])
    assert bilingua.merge(as_bytes) == bilingua.merge(doc)
    assert same_file(bilingua.merge(doc), data)


def test_skeleton_from_another_parser_or_a_newer_schema_is_refused():
    doc, _ = zip_doc()
    parser = doc["skeleton"]["zip"]["parser"]
    assert isinstance(parser["schema"], int) and parser["schema"] >= 1
    newer = copy.deepcopy(doc)
    newer["skeleton"]["zip"]["parser"]["schema"] = parser["schema"] + 1
    assert error_code(bilingua.merge, newer) == "parser_version"
    doc["skeleton"]["zip"]["parser"] = {"name": "bilingua-xlsx", "version": bilingua.version()}
    assert error_code(bilingua.merge, doc) == "parser_version"


def test_a_crate_version_bump_without_a_schema_change_still_merges():
    doc, data = zip_doc()
    for version in ("0.2.0", "1.0.0", "9.9.9"):
        bumped = copy.deepcopy(doc)
        bumped["skeleton"]["zip"]["parser"]["version"] = version
        assert bilingua.merge(bumped) == bilingua.merge(doc)


def test_bad_base64_is_an_invalid_document():
    doc, _ = zip_doc()
    e = next(e for e in doc["skeleton"]["zip"]["entries"] if "raw" in e["data"])
    e["data"]["raw"] = "not base64!"
    assert error_code(bilingua.merge, doc) == "invalid_document"


def test_bilingual_flag_comes_from_the_formats():
    assert {f["id"] for f in bilingua.formats() if f["bilingual"]} == {"po", "xliff", "sdlxliff", "mqxliff", "mxliff", "sdlppx", "mqxlz", "xcloc"}


def test_limits_are_refused_with_the_limit_code():
    e = error_code(bilingua.extract, "a.txt", b"x" * 60, limits={"max_input_bytes": 50})
    assert e == "limit"
    assert bilingua.extract("a.txt", b"x" * 50, limits={"max_input_bytes": 50})["units"]
    # an unknown limit name is an invalid argument, not silently ignored
    assert error_code(bilingua.extract, "a.txt", b"x", limits={"max_inputs": 1}) == "invalid_argument"


def test_entity_bomb_in_xliff_is_a_limit_error():
    bomb = (
        b'<?xml version="1.0"?><!DOCTYPE x [<!ENTITY a "lol"><!ENTITY b "&a;&a;&a;">]>'
        b'<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="a" source-language="en" '
        b'target-language="es" datatype="plaintext"><body><trans-unit id="1"><source>&b;</source></trans-unit></body></file></xliff>'
    )
    assert error_code(bilingua.extract, "a.xlf", bomb) == "limit"


def test_legacy_encoding_round_trips_and_garbled_text_warns():
    text = "Αυτό είναι ένα δοκιμαστικό κείμενο.\n"
    raw = (text * 2).encode("cp1253")
    doc = bilingua.extract("a.txt", raw, source_lang="el")
    assert "warnings" not in doc
    assert bilingua.merge(doc) == raw
    # no source language: read as Windows-1252, it looks wrong, so the Document carries a warning, not an error
    doc = bilingua.extract("a.txt", raw)
    assert doc["warnings"][0]["kind"] == "text_encoding"
    assert bilingua.merge(doc) == raw
    assert error_code(bilingua.extract, "a.txt", b"a\xff\x00\x01b") == "encoding"


def test_merge_options_output_encoding_and_stamp_target_always():
    src = '<html><head><meta charset="windows-1252"></head><body><p>café</p></body></html>'.encode("latin-1")
    doc = bilingua.extract("a.html", src, target_lang="es")
    assert bilingua.merge(doc) == src
    assert bilingua.merge(doc, None) == src
    out = bilingua.merge(doc, {"output_encoding": "utf-8"})
    assert "café".encode("utf-8") in out and b'charset="UTF-8"' in out
    xliff = (
        b'<?xml version="1.0"?><xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="a" '
        b'source-language="en" datatype="plaintext"><body><trans-unit id="1"><source>Hi</source></trans-unit></body></file></xliff>'
    )
    doc = bilingua.extract("a.xlf", xliff, target_lang="fr-FR")
    assert bilingua.merge(doc) == xliff
    assert b'target-language="fr-FR"' in bilingua.merge(doc, {"stamp_target_always": True})
    assert error_code(bilingua.merge, doc, {"stamp_always": True}) == "invalid_argument"
    assert error_code(bilingua.merge, doc, {"output_encoding": "latin-9"}) == "invalid_argument"


def test_format_options_reach_the_format():
    ftl = b"-brand = Firefox\nhello = Hello\n"
    assert len(bilingua.extract("a.ftl", ftl)["units"]) == 1
    assert len(bilingua.extract("a.ftl", ftl, format_options={"ftl.translate_terms": "true"})["units"]) == 2
    assert error_code(bilingua.extract, "a.ftl", ftl, format_options={"x": 1}) == "invalid_argument"
