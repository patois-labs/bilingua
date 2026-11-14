"""The Document crosses the PyO3 boundary without a serde_json::Value copy (T8). These tests pin that the objects,
their key order and the errors are the ones the Value path gave: sorted keys at every level, the same refusals for
the same bad input (the fast reader falls back to the old path on anything it does not take)."""
import copy
import json
import pathlib

import pytest

import bilingua

CORPUS = pathlib.Path(__file__).resolve().parents[2] / "oracle" / "corpus"
FILES = sorted(p for p in CORPUS.rglob("*") if p.is_file() and p.name != "SOURCES.md")
IDS = [f"{p.parent.name}/{p.name}" for p in FILES]


def keys_sorted(x):
    """True when every dict in x has its keys in sorted order (the order serde_json's BTreeMap gave)."""
    if isinstance(x, dict):
        return list(x) == sorted(x) and all(keys_sorted(v) for v in x.values())
    if isinstance(x, list):
        return all(keys_sorted(v) for v in x)
    return True


def only_plain(x):
    """True when x holds only dict, list, str, int, bool and None (no tuples, bytes or floats)."""
    if isinstance(x, dict):
        return all(type(k) is str and only_plain(v) for k, v in x.items())
    if isinstance(x, list):
        return all(only_plain(v) for v in x)
    return x is None or type(x) in (str, int, bool)


def txt_doc(text=b"Hello there. Second one.\n"):
    return bilingua.extract("a.txt", text, source_lang="en")


def error(fn, *args):
    with pytest.raises(bilingua.BilinguaError) as e:
        fn(*args)
    return e.value.code, str(e.value)


@pytest.mark.parametrize("path", FILES, ids=IDS)
def test_extract_gives_sorted_plain_objects_that_survive_json(path):
    try:
        doc = bilingua.extract(path.name, path.read_bytes(), source_lang="en")
    except bilingua.BilinguaError:
        return
    assert keys_sorted(doc), path
    assert only_plain(doc), path
    # json.dumps without sort_keys gives the sorted text, as with the Value path
    assert json.dumps(doc) == json.dumps(doc, sort_keys=True)
    again = json.loads(json.dumps(doc))
    assert outcome(bilingua.merge, again) == outcome(bilingua.merge, doc)


def outcome(fn, *args):
    try:
        return fn(*args)
    except bilingua.BilinguaError as e:
        return (e.code, str(e))


def test_parser_record_sorts_in_with_the_document_keys():
    doc = txt_doc()
    assert list(doc) == sorted(doc)
    assert "parser" in doc and keys_sorted(doc["parser"])


def test_locos_set_targets_keeps_order_and_parser():
    doc = bilingua.extract("a.txt", b"Hello there.\n", source_lang="en", profile="locos")
    units = bilingua.locos_units(doc)
    out, problems = bilingua.locos_set_targets(doc, {u["segment_id"]: "Hola." for u in units})
    assert keys_sorted(out) and problems == {}
    assert out["parser"] == doc["parser"]
    assert list(out) == list(doc)
    assert b"Hola." in bilingua.merge(out)


def test_tuples_read_like_lists():
    doc = txt_doc()
    tup = copy.deepcopy(doc)
    tup["units"] = tuple(tup["units"])
    assert bilingua.merge(tup) == bilingua.merge(doc)


def test_an_int_is_not_a_bool():
    # a truthy value in a bool field is refused, as serde_json refused it
    doc = txt_doc()
    doc["units"][0]["translate"] = 1
    code, msg = error(bilingua.merge, doc)
    assert code == "invalid_document" and "expected a boolean" in msg


def test_an_unknown_key_is_still_checked():
    doc = txt_doc()
    doc["extra"] = {1, 2}
    assert error(bilingua.merge, doc) == ("invalid_document", "cannot use a set in a document")
    doc["extra"] = {1: "x"}
    assert error(bilingua.merge, doc) == ("invalid_document", "dict keys must be strings")
    doc["extra"] = float("nan")
    assert error(bilingua.locos_units, doc) == ("invalid_document", "NaN or infinity in document")


def test_deep_nesting_in_an_unknown_key_is_refused_not_a_crash():
    doc = txt_doc()
    deep = []
    for _ in range(100_000):
        deep = [deep]
    doc["extra"] = deep
    assert error(bilingua.merge, doc) == ("invalid_document", "document is nested too deeply")
    assert error(bilingua.locos_units, doc) == ("invalid_document", "document is nested too deeply")


def test_bytes_still_read_as_their_base64_text():
    # bytes in a str field became their base64 string on the Value path; they still do (b"txt" is "dHh0")
    doc = txt_doc()
    doc["format"] = b"txt"
    code, msg = error(bilingua.merge, doc)
    assert code == "unsupported" and "dHh0" in msg


def test_bad_parser_record_and_bad_profile_fail_as_before():
    doc = txt_doc()
    doc["parser"] = {"profile": {1, 2}}
    assert error(bilingua.merge, doc) == ("invalid_document", "cannot use a set in a document")
    # a document the model refuses still loses to a bad profile argument, which merge checks first
    doc = txt_doc()
    doc["units"][0]["translate"] = "yes"
    assert error(lambda d: bilingua.merge(d, profile="nope"), doc)[0] == "invalid_argument"
