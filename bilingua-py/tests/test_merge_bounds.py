"""BILI-105: merge bounds what a Document dict can make it do. The dict is the caller's data; these are changed in
Python the way a holder of it could change them, and each must be refused with a typed error."""
import copy
import pathlib

import pytest

import bilingua

FIXTURES = pathlib.Path(__file__).resolve().parents[2] / "bilingua" / "tests" / "fixtures"


def refusal(fn, *args, **kw):
    with pytest.raises(bilingua.BilinguaError) as e:
        fn(*args, **kw)
    return e.value


def text_parts(doc):
    return doc["skeleton"]["text"]["parts"]


def test_one_unit_written_many_times_is_refused():
    doc = bilingua.extract("a.json", b'{"k": "' + b"word " * 60 + b'end"}\n', source_lang="en")
    parts = text_parts(doc)
    slot = next(p["unit"] for p in parts if "unit" in p)
    have = sum(1 for p in parts if p.get("unit") == slot)
    ok = copy.deepcopy(doc)
    text_parts(ok).extend({"unit": slot} for _ in range(4 - have))
    assert bilingua.merge(ok)
    amp = copy.deepcopy(doc)
    text_parts(amp).extend({"unit": slot} for _ in range(100_000))
    e = refusal(bilingua.merge, amp)
    assert e.code == "limit" and e.limit == "skeleton", (e.code, e.limit, str(e))
    # a slot for a unit the document does not have
    missing = copy.deepcopy(doc)
    text_parts(missing).append({"unit": 99})
    assert refusal(bilingua.merge, missing).limit == "skeleton"


def test_the_extract_limits_apply_at_merge():
    doc = bilingua.extract("a.txt", b"One.\n\nTwo.\n\nThree.\n", source_lang="en", granularity="paragraph")
    e = refusal(bilingua.merge, doc, limits={"max_units": 2})
    assert e.code == "limit" and e.limit == "units"
    long = copy.deepcopy(doc)
    long["units"][0]["segments"][0]["target"] = [{"text": "y" * 401}]
    assert refusal(bilingua.merge, long, limits={"max_unit_chars": 100}).limit == "unit_chars"
    # an ordinary error has no limit
    bad = copy.deepcopy(doc)
    bad["format"] = "nope"
    assert refusal(bilingua.merge, bad).limit is None


@pytest.mark.parametrize("name", ["../x.xml", "/abs.xml", "word\\x.xml", "a\u0001b.xml", "C:x.xml", "./word/x.xml", "", "dup"])
def test_an_unsafe_or_repeated_zip_name_is_not_written(name):
    doc = bilingua.extract("page.docx", (FIXTURES / "page.docx").read_bytes(), source_lang="en")
    entries = doc["skeleton"]["zip"]["entries"]
    # the archive's own entries (a format's records start with "\0bilingua-" and are never written)
    real = [i for i, e in enumerate(entries) if not e["name"].startswith("\0")]
    entries[real[-1]]["name"] = entries[real[0]]["name"] if name == "dup" else name
    e = refusal(bilingua.merge, doc)
    assert e.code == "limit" and e.limit == "zip_malformed", (name, e.code, str(e))


def test_targets_set_in_the_dict_get_the_tag_check():
    doc = bilingua.extract("a.html", b"<p>Hello <b>bold</b> world.</p>", source_lang="en")
    seg = doc["units"][0]["segments"][0]
    open_ = next(i for i in seg["source"] if "tag" in i and i["tag"]["kind"] == "open")
    close = next(i for i in seg["source"] if "tag" in i and i["tag"]["kind"] == "close")
    good = copy.deepcopy(doc)
    good["units"][0]["segments"][0]["target"] = [{"text": "Hallo "}, open_, {"text": "fett"}, close, {"text": " Welt."}]
    assert b"Hallo <b>fett</b> Welt." in bilingua.merge(good)
    invented = copy.deepcopy(doc)
    invented["units"][0]["segments"][0]["target"] = [{"tag": {"id": "zz", "kind": "open", "display": "<i>"}}, {"text": "Hallo"}]
    assert refusal(bilingua.merge, invented).code == "invented_tag"
    twice = copy.deepcopy(doc)
    twice["units"][0]["segments"][0]["target"] = [open_, {"text": "a"}, close, open_, {"text": "b"}, close]
    assert refusal(bilingua.merge, twice).code == "duplicated_tag"
    # a standalone code the unit does not have
    point = copy.deepcopy(doc)
    point["units"][0]["segments"][0]["target"] = [{"tag": {"id": "zz", "kind": "standalone", "display": "<x>"}}]
    assert refusal(bilingua.merge, point).code == "invented_tag"


def test_a_bilingual_file_gets_the_tag_check_too():
    x = (b'<?xml version="1.0"?>\n<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file source-language="en" '
         b'target-language="de" datatype="plaintext" original="f"><body><trans-unit id="a"><source>Hi <g id="1">bold</g></source>'
         b'<target>Hallo <g id="1">fett</g></target></trans-unit></body></file></xliff>\n')
    doc = bilingua.extract("a.xlf", x, source_lang="en")
    assert bilingua.merge(doc) == x
    doc["units"][0]["segments"][0]["target"] = [{"text": "Hallo "}, {"tag": {"id": "99", "kind": "standalone", "display": "<x/>"}}]
    assert refusal(bilingua.merge, doc).code == "invented_tag"
