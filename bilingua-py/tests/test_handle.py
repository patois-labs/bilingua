"""DocumentHandle and extract(..., locos_units=True) (BILI-18, speed): the same answers as the dict functions.

The differential part runs every file of the oracle corpus and every LocOS golden input both ways, natively and under
the LocOS profile: the dict, the units, the targets' problems, the merged bytes and every error (code, message,
detail) must be identical. More files (the wild sets) can be added at run time with BILI_HANDLE_DIFF_DIRS, a list of
directories separated by os.pathsep.
"""
import json
import os
import pathlib
import threading

import pytest

import bilingua

HERE = pathlib.Path(__file__).resolve()
CORPUS = HERE.parents[2] / "oracle" / "corpus"
GOLDEN_ROOT = HERE.parents[2] / "bilingua" / "tests" / "locos"
SIDECARS = (".golden.json", ".golden.zip", ".maps.json", ".opts.json")


def _files():
    out = [p for p in CORPUS.rglob("*") if p.is_file() and p.name != "SOURCES.md"]
    out += [p for p in GOLDEN_ROOT.glob("*/*") if p.is_file() and not p.name.endswith(SIDECARS)]
    for d in filter(None, os.environ.get("BILI_HANDLE_DIFF_DIRS", "").split(os.pathsep)):
        out += [p for p in pathlib.Path(d).rglob("*") if p.is_file()]
    return sorted(set(out))


FILES = _files()
IDS = [f"{p.parent.name}/{p.name}" for p in FILES]
VARIANTS = {
    "native": {"source_lang": "en"},
    "locos": {"profile": "locos", "granularity": "paragraph", "target_lang": "de-DE"},
}


def outcome(fn, *a, **k):
    try:
        return ("ok", fn(*a, **k))
    except bilingua.BilinguaError as e:
        return ("error", e.code, str(e), e.detail)


def pseudo(units):
    return {u["segment_id"]: u["text"] + " ~x" for u in units}


def compare(name, data, kw, merge_kw):
    """Every step both ways; returns how far it got (for the summary test)."""
    d = outcome(bilingua.extract, name, data, **kw)
    h = outcome(bilingua.extract_handle, name, data, **kw)
    if d[0] == "error":
        assert h == d
        assert outcome(bilingua.extract, name, data, locos_units=True, **kw) == d
        return "refused"
    assert h[0] == "ok", h
    doc, handle = d[1], h[1]
    as_dict = handle.to_dict()
    assert as_dict == doc
    assert json.dumps(as_dict) == json.dumps(doc)  # same key order
    assert handle.format == doc["format"]
    assert handle.parser == doc["parser"]
    assert handle.warnings == doc.get("warnings", [])
    assert handle.source_encoding == doc.get("source_encoding")
    assert len(handle) == len(doc["units"])
    extras = [
        {"unit": u["id"], "segments": [s["id"] for s in u["segments"]], "translate": u["translate"], "extra": u["context"]["extra"]}
        for u in doc["units"]
        if u.get("context", {}).get("extra")
    ]
    assert handle.unit_extras() == extras
    # the units: dict path, handle, and extract with locos_units=True
    units = outcome(bilingua.locos_units, doc)
    assert outcome(handle.locos_units) == units
    both = outcome(bilingua.extract, name, data, locos_units=True, **kw)
    if units[0] == "ok":
        assert both == ("ok", (doc, units[1]))
    # untouched merge
    assert outcome(handle.merge, **merge_kw) == outcome(bilingua.merge, doc, **merge_kw)
    if units[0] != "ok":
        return "no units"
    # targets: problems, the new document, and its merge
    targets = pseudo(units[1])
    sd = outcome(bilingua.locos_set_targets, doc, targets)
    sh = outcome(handle.locos_set_targets, targets)
    if sd[0] == "error":
        assert sh == sd
        return "targets refused"
    (doc2, probs), (h2, probs2) = sd[1], sh[1]
    assert probs2 == probs
    assert h2.to_dict() == doc2
    assert json.dumps(h2.to_dict()) == json.dumps(doc2)
    assert outcome(h2.merge, **merge_kw) == outcome(bilingua.merge, doc2, **merge_kw)
    # the handle the targets were set on is unchanged
    assert handle.to_dict() == doc
    # a handle made from the dict is the same handle
    h3 = bilingua.DocumentHandle.from_dict(doc2)
    assert h3.to_dict() == doc2 and outcome(h3.merge, **merge_kw) == outcome(bilingua.merge, doc2, **merge_kw)
    return "built"


@pytest.mark.parametrize("variant", list(VARIANTS))
@pytest.mark.parametrize("path", FILES, ids=IDS)
def test_handle_matches_the_dict_path(path, variant):
    kw = dict(VARIANTS[variant])
    merge_kw = {"profile": "locos"} if variant == "locos" else {}
    side = pathlib.Path(str(path) + ".opts.json")
    if variant == "locos" and side.exists():
        fo = json.loads(side.read_text(encoding="utf-8")).get("format_options")
        if fo:
            kw["format_options"] = fo
            merge_kw["format_options"] = fo
    compare(path.name, path.read_bytes(), kw, merge_kw)


GOLDENS = sorted(GOLDEN_ROOT.glob("*/*.golden.json"))


@pytest.mark.parametrize("golden", GOLDENS, ids=[f"{g.parent.name}/{g.name[:-12]}" for g in GOLDENS])
def test_handle_builds_every_golden_mapping_like_the_dict_path(golden):
    # the mappings LocOS's goldens use (hostile ones included), set through both paths: identical bytes or errors
    g = json.loads(golden.read_text(encoding="utf-8"))
    inp = pathlib.Path(str(golden)[: -len(".golden.json")])
    side = pathlib.Path(str(inp) + ".opts.json")
    fo = json.loads(side.read_text(encoding="utf-8")).get("format_options") if side.exists() else None
    for b in g["builds"]:
        loc = b["locale"].replace("_", "-") if b["locale"] else None
        kw = {"profile": "locos", "granularity": "paragraph", "target_lang": loc, "format_options": fo}
        try:
            doc, units = bilingua.extract(inp.name, inp.read_bytes(), locos_units=True, **kw)
        except bilingua.BilinguaError:
            continue
        h = bilingua.extract_handle(inp.name, inp.read_bytes(), **kw)
        by_p = {str(u["p_index"]): u["segment_id"] for u in units}
        targets = {by_p[k]: v for k, v in b["map"].items() if k in by_p}
        sd = outcome(bilingua.locos_set_targets, doc, targets)
        sh = outcome(h.locos_set_targets, targets)
        if sd[0] == "error":
            assert sh == sd, b["name"]
            continue
        assert sh[1][1] == sd[1][1], b["name"]
        assert outcome(sh[1][0].merge, profile="locos") == outcome(bilingua.merge, sd[1][0], profile="locos"), b["name"]


def test_enough_files():
    assert len(FILES) > 1000


# --- errors -------------------------------------------------------------------------------------------------------


def err(fn, *a, **k):
    with pytest.raises(bilingua.BilinguaError) as e:
        fn(*a, **k)
    return e.value.code, str(e.value), e.value.detail


def test_extract_handle_refuses_like_extract():
    for a, k in [
        (("a.nope", b"x"), {}),
        (("a.txt", b"Hi."), {"profile": "nope"}),
        (("a.txt", b"Hi."), {"granularity": "word"}),
        (("a.txt", b"Hi."), {"profile_version": 1}),
        (("a.txt", b"Hi."), {"format_options": {"nope.x": 1}}),
        (("a.txt", b"Hi."), {"limits": {"max_units": "x"}}),
        (("a.json", b"{"), {}),
    ]:
        assert err(bilingua.extract_handle, *a, **k) == err(bilingua.extract, *a, **k), (a, k)


def test_from_dict_refuses_like_merge():
    doc = bilingua.extract("a.txt", b"Hello there.\n")
    for bad in [{"units": 1}, dict(doc, extra={1, 2}), dict(doc, units=[{"id": 3}]), {}]:
        assert err(bilingua.DocumentHandle.from_dict, bad)[0] == err(bilingua.merge, bad)[0], bad
        assert err(bilingua.DocumentHandle.from_dict, bad)[1] == err(bilingua.merge, bad)[1], bad
    with pytest.raises(bilingua.BilinguaError):
        bilingua.DocumentHandle.from_dict(42)


def test_targets_refused_like_the_dict_path_and_the_handle_kept():
    doc = bilingua.extract("a.txt", b"Hello there.\n", profile="locos")
    h = bilingua.extract_handle("a.txt", b"Hello there.\n", profile="locos")
    seg = h.locos_units()[0]["segment_id"]
    for t in [{"nope": "x"}, {seg: 3}, [("a", "b")], {seg: "ok", "nope": "x"}, {seg: "⟦g9⟧x⟦/g9⟧"}]:
        have, want = outcome(h.locos_set_targets, t), outcome(bilingua.locos_set_targets, doc, t)
        if want[0] == "ok":
            assert have[0] == "ok" and have[1][0].to_dict() == want[1][0] and have[1][1] == want[1][1], t
        else:
            assert have == want, t
    assert h.to_dict() == doc
    assert err(h.merge, profile="nope")[0] == "invalid_argument"
    assert err(h.merge, output_encoding="latin-1")[0] == "invalid_argument"


def test_merge_keywords_and_profile_version_follow_the_dict_path():
    data = "[s]\nk = Café\n".encode("cp1252")
    doc = bilingua.extract("a.ini", data, profile="locos@1")
    h = bilingua.extract_handle("a.ini", data, profile="locos@1")
    for k in [{}, {"profile": "locos"}, {"profile": "locos", "output_encoding": "utf-8"}, {"stamp_target_always": True}, {"profile": "locos", "profile_version": 1}, {"profile": "locos@1"}]:
        assert outcome(h.merge, **k) == outcome(bilingua.merge, doc, **k), k


def test_repr_and_type():
    h = bilingua.extract_handle("a.txt", b"One.\n\nTwo.\n")
    assert isinstance(h, bilingua.DocumentHandle)
    assert repr(h) == f'<bilingua.DocumentHandle format="txt" units={len(h)}>'
    assert h.unit_extras(["no.such"]) == []
    with pytest.raises(TypeError):
        bilingua.DocumentHandle()
    with pytest.raises(AttributeError):
        h.format = "x"


# --- threads and memory -------------------------------------------------------------------------------------------


def test_one_handle_used_from_many_threads():
    data = "".join(f"Line {i} has some text.\n\n" for i in range(300)).encode()
    h = bilingua.extract_handle("a.txt", data, profile="locos")
    units = h.locos_units()
    targets = pseudo(units)
    want_merge = h.merge(profile="locos")
    want_built = h.locos_set_targets(targets)[0].merge(profile="locos")
    bad = []
    barrier = threading.Barrier(8)

    def work():
        barrier.wait()
        for _ in range(25):
            if h.locos_units() != units or h.merge(profile="locos") != want_merge:
                bad.append("read")
            if h.locos_set_targets(targets)[0].merge(profile="locos") != want_built:
                bad.append("built")

    ts = [threading.Thread(target=work) for _ in range(8)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    assert bad == []


def _rss_kb():
    try:
        with open("/proc/self/statm") as fh:
            return int(fh.read().split()[1]) * (os.sysconf("SC_PAGE_SIZE") // 1024)
    except (OSError, ValueError, AttributeError):
        return None


@pytest.mark.skipif(_rss_kb() is None, reason="needs /proc/self/statm (Linux)")
def test_handles_are_freed():
    # 300 KB of text per round: a handle (and the one locos_set_targets makes) that leaked would add about
    # 1 GB over the loop; the bound leaves room for the allocator
    data = "".join(f"Paragraph {i} with a few words in it.\n\n" for i in range(8000)).encode()

    def round_trip():
        h = bilingua.extract_handle("a.txt", data, profile="locos")
        h2, _ = h.locos_set_targets(pseudo(h.locos_units()))
        h2.merge(profile="locos")

    for _ in range(20):
        round_trip()
    before = _rss_kb()
    for _ in range(300):
        round_trip()
    grown = _rss_kb() - before
    assert grown < 64 * 1024, f"RSS grew {grown} kB over 300 rounds"


def _notes_via_dict(doc):
    """The notes an adapter reads from the dict: unit notes keyed by the first segment's id."""
    out = []
    for unit in doc["units"]:
        notes = (unit.get("context") or {}).get("notes") or []
        if notes:
            out.append({"unit": unit["id"], "segments": [s["id"] for s in unit["segments"]], "notes": list(notes)})
    return out


@pytest.mark.parametrize(
    "name,data,expect_notes",
    [
        ("a.xmb", b"<messagebundle><msg id='1' desc='button' meaning='verb'>Save</msg><msg id='2'>Plain</msg></messagebundle>", True),
        ("a.tsv", b"key\tsource\tcomment\nk1\tHello\tbutton\nk2\tBye\t\n", False),
        ("a.txt", b"One.\n\nTwo.\n", False),
    ],
)
@pytest.mark.parametrize("profile", [None, "locos"])
def test_unit_notes_match_the_dict_path(name, data, expect_notes, profile):
    h = bilingua.extract_handle(name, data, profile=profile)
    want = _notes_via_dict(h.to_dict())
    assert h.unit_notes() == want
    if expect_notes:
        assert want and want[0]["notes"]
