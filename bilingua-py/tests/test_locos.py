"""The Localization OS calls of the wheel: token text, numbering, targets, profile versions and the parser record.

The golden part replays `bilingua/tests/locos/` (written by `oracle/locos_golden.py` from LocOS's own filters) the
way the LocOS adapter will: extract under the profile, `locos_units`, `locos_set_targets`, `merge`. It is the
Python twin of `bilingua/tests/locos_golden.rs`; both must agree with LocOS.
"""
import base64
import io
import json
import pathlib
import zipfile
from xml.etree import ElementTree

import pytest

import bilingua

OPEN, CLOSE = "⟦", "⟧"
GOLDEN_ROOT = pathlib.Path(__file__).resolve().parents[2] / "bilingua" / "tests" / "locos"
GOLDENS = sorted(GOLDEN_ROOT.glob("*/*.golden.json"))
# Same lists as locos_golden.rs (KNOWN, KNOWN_BUILDS): files or builds that still differ from LocOS on purpose.
KNOWN = {}
KNOWN_BUILDS = {
    "hostile_tails.pptx": ("prefix", "hostile", "hostile-ar-SA", "hostile-de-DE"),
    # a pair named by a point token in a plain XLIFF: LocOS copies the source element with its source text
    "docx_po_fidelity_1.xliff": ("repair-point", "repair-point-de-DE"),
    "xliff_package_filter_1.xliff": ("repair-point", "repair-point-de-DE"),
}


def tag(id_, kind, display, free=False):
    t = {"id": id_, "kind": kind, "display": display}
    if free:
        t["free"] = True
    return {"tag": t}


def tok(s):
    return s.replace("[", OPEN).replace("]", CLOSE)


def code(fn, *a, **k):
    with pytest.raises(bilingua.BilinguaError) as e:
        fn(*a, **k)
    return e.value.code


# --- token text ---------------------------------------------------------------------------------------------------


def test_positional_tokens_and_map():
    inl = [{"text": "a "}, tag("b", "open", "<b>"), {"text": "bold"}, tag("b", "close", "</b>"), tag("br", "standalone", "<br/>"), tag("n", "standalone", "{n}", True)]
    text, m = bilingua.to_locos_text("html5-not-a-format", inl)
    assert text == tok("a [g1]bold[/g1][x2][p3]")
    assert m["1"] == {"id": "b", "letter": "g", "open": "<b>", "close": "</b>", "free": False}
    assert m["2"]["letter"] == "x" and m["3"]["letter"] == "p" and m["3"]["free"] is True
    back, problems = bilingua.from_locos_text(text, m)
    assert back == inl and problems == []


def test_docx_numbers_by_tag_id_with_r_letter():
    inl = [tag("1", "open", "<r1>"), {"text": "a"}, tag("1", "close", "</r1>"), tag("3", "standalone", "<x3/>"), tag("2", "open", "<r2>"), {"text": "b"}, tag("2", "close", "</r2>")]
    text, m = bilingua.to_locos_text("docx", inl)
    assert text == tok("[r1]a[/r1][x3][r2]b[/r2]")
    assert sorted(m) == ["1", "2", "3"]
    assert bilingua.from_locos_text(tok("[r2]b[/r2][r1]a[/r1][x3]"), m)[1] == []


def test_html_pairs_by_id_even_when_crossing():
    inl = [tag("a", "open", "<a>"), tag("b", "open", "<b>"), {"text": "x"}, tag("a", "close", "</a>"), tag("b", "close", "</b>")]
    html, _ = bilingua.to_locos_text("html", inl)
    plain, _ = bilingua.to_locos_text("md", inl)
    assert html == tok("[g1][g2]x[/g1][/g2]")
    # elsewhere the inner side of crossing pairs becomes point codes
    assert plain != html and tok("[x") in plain


def test_look_alike_text_keeps_its_number_and_stays_text():
    inl = [{"text": tok("see [x1] ")}, tag("b", "standalone", "<br/>")]
    text, m = bilingua.to_locos_text("md", inl)
    assert text == tok("see [x1] [x2]") and list(m) == ["2"]
    back, problems = bilingua.from_locos_text(text, m)
    assert back == inl
    assert problems == [{"kind": "unknown", "token": tok("[x1]")}]


def test_damaged_tokens_are_kept_as_text_and_listed():
    _, m = bilingua.to_locos_text("md", [tag("b", "open", "<b>"), {"text": "x"}, tag("b", "close", "</b>"), tag("c", "standalone", "<c/>")])
    back, problems = bilingua.from_locos_text(tok("[g9] [/x2] [g1]y[/g1] [x") + " " + CLOSE, m)
    kinds = [p["kind"] for p in problems]
    assert kinds == ["unknown", "kind_mismatch", "stray", "stray"]
    assert "".join(i.get("text", "") for i in back).startswith(tok("[g9] [/x2] "))
    assert code(bilingua.to_locos_text, "md", [{"nope": 1}]) == "invalid_argument"
    assert code(bilingua.from_locos_text, "x", {"1": {"letter": 5}}) == "invalid_argument"


# --- parser record and profile versions ----------------------------------------------------------------------------


def test_parser_record():
    doc = bilingua.extract("a.md", b"# Hi\n\nText.\n", profile="locos")
    p = doc["parser"]
    assert p["bilingua"] == bilingua.version() and p["format"] == "markdown"
    assert p["crate"] == "bilingua-markdown" and p["crate_version"] and isinstance(p["schema"], int)
    assert p["profile"] == "locos" and p["profile_version"] == bilingua.profiles()["locos"]
    plain = bilingua.extract("a.md", b"# Hi\n")["parser"]
    assert plain["profile"] is None and plain["profile_version"] is None
    # merge ignores the record
    assert bilingua.merge(doc, profile="locos") == b"# Hi\n\nText.\n"


def test_profile_version_selection():
    data = b"Hello\n"
    current = bilingua.profiles()["locos"]
    for kw in ({"profile": "locos@1"}, {"profile": "locos", "profile_version": 1}, {"profile": "locos@1", "profile_version": 1}):
        d = bilingua.extract("a.txt", data, **kw)
        assert d["parser"]["profile_version"] == 1
        assert bilingua.merge(d, profile=kw["profile"]) == data
    assert code(bilingua.extract, "a.txt", data, profile="locos", profile_version=current + 1) == "parser_version"
    assert code(bilingua.extract, "a.txt", data, profile="locos@0") == "parser_version"
    assert code(bilingua.extract, "a.txt", data, profile="locos@1", profile_version=2) == "invalid_argument"
    assert code(bilingua.extract, "a.txt", data, profile_version=1) == "invalid_argument"
    assert code(bilingua.extract, "a.txt", data, profile="locos@") == "invalid_argument"
    doc = bilingua.extract("a.txt", data, profile="locos")
    # a document stamped with a profile version this release cannot produce is refused at merge
    doc["parser"]["profile_version"] = current + 1
    assert code(bilingua.merge, doc, profile="locos") == "parser_version"
    # an explicit version wins over the record
    assert bilingua.merge(doc, profile="locos", profile_version=current) == data
    assert bilingua.merge(doc, profile="locos@1") == data
    assert code(bilingua.merge, doc, profile_version=1) == "invalid_argument"


# --- units and targets -----------------------------------------------------------------------------------------------


def test_locos_units_and_set_targets():
    data = b"<html><body><h1>Title</h1><p>Some <b>bold</b> text.</p></body></html>"
    doc = bilingua.extract("a.html", data, profile="locos", target_lang="de")
    units = bilingua.locos_units(doc)
    assert [u["p_index"] for u in units] == list(range(len(units)))
    body = next(u for u in units if "bold" in u["text"])
    assert OPEN in body["text"] and body["map"]
    targets = {u["segment_id"]: u["text"].replace("bold", "fett").replace("Title", "Titel").replace("text", "Text") for u in units}
    new, problems = bilingua.locos_set_targets(doc, targets)
    assert problems == {} and new["parser"] == doc["parser"]
    out = bilingua.merge(new, profile="locos").decode("utf-8")
    assert "Titel" in out and "<b>fett</b>" in out
    # a look-alike typed by the translator stays text and is listed
    seg = body["segment_id"]
    new, problems = bilingua.locos_set_targets(doc, {seg: body["text"] + tok(" [x9]")})
    assert problems == {seg: [{"kind": "unknown", "token": tok("[x9]")}]}
    assert tok("[x9]") in bilingua.merge(new, profile="locos").decode("utf-8")
    # empty is a translation to nothing; white space only is kept
    new, _ = bilingua.locos_set_targets(doc, {seg: ""})
    assert "bold" not in bilingua.merge(new, profile="locos").decode("utf-8")
    new, _ = bilingua.locos_set_targets(doc, {seg: "  "})
    assert "bold" not in bilingua.merge(new, profile="locos").decode("utf-8")
    assert code(bilingua.locos_set_targets, doc, {"no-such-segment": "x"}) == "unknown_segment"
    assert code(bilingua.locos_set_targets, doc, {seg: 3}) == "invalid_argument"
    assert code(bilingua.locos_set_targets, doc, ["x"]) == "invalid_argument"
    with pytest.raises(bilingua.BilinguaError, match="segment no-such-segment"):
        bilingua.locos_set_targets(doc, {"no-such-segment": "x"})


def test_repeated_placeholder_is_accepted_like_locos():
    data = json.dumps({"k": "{n} items"}).encode()
    doc = bilingua.extract("a.json", data, profile="locos")
    (u,) = bilingua.locos_units(doc)
    new, problems = bilingua.locos_set_targets(doc, {u["segment_id"]: "{n} Dinge, {n}"})
    assert problems == {}
    assert json.loads(bilingua.merge(new, profile="locos"))["k"] == "{n} Dinge, {n}"


def test_cat_formats_repair_tags_like_locos():
    # W3 B-SIDE 8: a repeated, stray or unclosed pair token (common in MT output) is repaired as LocOS's CAT writers
    # do and each repair is listed, where it used to raise duplicated_tag
    data = (
        b'<?xml version="1.0" encoding="UTF-8"?><xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">'
        b'<file original="a" source-language="en" datatype="plaintext"><body>'
        b'<trans-unit id="1"><source>Click <g id="1">here</g> now.</source></trans-unit></body></file></xliff>'
    )
    doc = bilingua.extract("a.xlf", data, profile="locos", target_lang="de")
    (u,) = bilingua.locos_units(doc)
    seg = u["segment_id"]
    g1, c1 = tok("[g1]"), tok("[/g1]")
    new, problems = bilingua.locos_set_targets(doc, {seg: f"bitte {g1}Klick{c1} hier {g1}doppelt{c1} jetzt"})
    assert problems == {seg: [{"kind": "repeated", "token": g1}]}
    assert '<target>bitte <g id="1">Klick</g> hier doppelt jetzt</target>' in bilingua.merge(new, profile="locos").decode()
    new, problems = bilingua.locos_set_targets(doc, {seg: f"{c1}bitte {g1}Klick"})
    assert [p["kind"] for p in problems[seg]] == ["stray_close", "unclosed"]
    assert '<target>bitte <g id="1">Klick</g></target>' in bilingua.merge(new, profile="locos").decode()


# --- the LocOS goldens ---------------------------------------------------------------------------------------------


def zip_entries(data):
    if not data.startswith(b"PK"):
        return None
    with zipfile.ZipFile(io.BytesIO(data)) as zf:
        return [(i.filename, zf.read(i)) for i in zf.infolist() if not i.is_dir()]


def canonical(data):
    try:
        return ElementTree.canonicalize(data.decode("utf-8-sig"), with_comments=True)
    except (ElementTree.ParseError, UnicodeDecodeError, ValueError):
        return None


def same_zip(golden_path, input_zip, want, have):
    entries = zip_entries(have)
    assert entries is not None, "Bilingua did not write a zip"
    have = dict(entries)
    side = pathlib.Path(str(golden_path).replace(".golden.json", ".golden.zip"))
    blobs = dict(zip_entries(side.read_bytes()) or []) if side.exists() else {}
    assert set(have) == {e["name"] for e in want}
    orig = dict(input_zip)
    for e in want:
        h = have[e["name"]]
        if e.get("same"):
            assert h == orig.get(e["name"]), e["name"]
            continue
        w = blobs[e["sha1"]]
        if w != h:
            a, b = canonical(w), canonical(h)
            assert a is not None and a == b, e["name"]


def extract(name, data, locale=None):
    return bilingua.extract(name, data, profile="locos", granularity="paragraph", target_lang=locale.replace("_", "-") if locale else None)


@pytest.mark.parametrize("golden", GOLDENS, ids=[f"{g.parent.name}/{g.name[:-12]}" for g in GOLDENS])
def test_locos_golden(golden):
    g = json.loads(golden.read_text(encoding="utf-8"))
    inp = pathlib.Path(str(golden)[: -len(".golden.json")])
    name, data = inp.name, inp.read_bytes()
    if name in KNOWN:
        pytest.skip(KNOWN[name])
    if g.get("parse_error"):
        try:
            doc = extract(name, data)
        except bilingua.BilinguaError:
            return
        assert g["parse_error"].startswith("EmptyDocumentError") and not bilingua.locos_units(doc)
        return
    units = bilingua.locos_units(extract(name, data))
    assert [(u["p_index"], u["text"], u["style"]) for u in units] == [(u["p_index"], u["text"], u["style"]) for u in g["units"]]
    input_zip = zip_entries(data) or []
    for b in g["builds"]:
        if "error" in b or b["name"] in KNOWN_BUILDS.get(name, ()):
            continue
        mapping, locale, want_zip = b["map"], b["locale"], b.get("zip")
        want = base64.b64decode(b["out_b64"]) if "out_b64" in b else None
        # LocOS's byte guard: a build that changes nothing returns the upload without calling the parser
        identity = mapping and g["format"] != "po" and all(any(str(u["p_index"]) == k and u["text"] == v for u in g["units"]) for k, v in mapping.items())
        if locale is None and (not mapping or identity):
            untouched = len(want_zip) == len(input_zip) and all(e.get("same") for e in want_zip) if want_zip is not None else want == data
            if untouched:
                continue
        doc = extract(name, data, locale)
        by_p = {str(u["p_index"]): u["segment_id"] for u in bilingua.locos_units(doc)}
        doc, _ = bilingua.locos_set_targets(doc, {by_p[k]: v for k, v in mapping.items() if k in by_p})
        out = bilingua.merge(doc, profile="locos")
        if want_zip is not None:
            same_zip(golden, input_zip, want_zip, out)
        else:
            assert out == want, b["name"]


def test_goldens_found():
    assert len(GOLDENS) > 100
