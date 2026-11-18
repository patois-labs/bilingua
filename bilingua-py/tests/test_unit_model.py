"""The 0.10 unit model keys in the Python dicts: context `role` and `level`, code `type` and `attrs`, unit `plural`.
They appear outside the LocOS profile and are missing under `profile="locos"`, so a LocOS dict keeps its old shape."""
import pathlib

import bilingua

CORPUS = pathlib.Path(__file__).resolve().parents[2] / "oracle" / "corpus"
FEATURES = (CORPUS / "docx" / "features.docx").read_bytes()

ARB = (
    b'{\n  "@@locale": "en",\n'
    b'  "files": "{count, plural, one{# file} other{# files}}",\n'
    b'  "hi": "Hello"\n}\n'
)

NEW_CONTEXT = {"role", "level"}
NEW_CODE = {"type", "attrs"}


def new_keys(doc):
    """Every new key found in the dict, as (where, key) pairs."""
    found = []
    for u in doc["units"]:
        if "plural" in u:
            found.append(("unit", "plural"))
        found += [("context", k) for k in NEW_CONTEXT & set(u.get("context", {}))]
        for c in u.get("codes", {}).values():
            found += [("code", k) for k in NEW_CODE & set(c)]
    return found


def test_docx_has_roles_and_code_types():
    doc = bilingua.extract("features.docx", FEATURES)
    roles = [(u["context"].get("role"), u["context"].get("level")) for u in doc["units"]]
    assert ("title", None) in roles
    assert ("heading", 1) in roles
    # a unit without a role has neither key
    assert all("level" not in u["context"] for u in doc["units"] if "role" not in u["context"])
    types = {c["type"]: c for u in doc["units"] for c in u["codes"].values() if "type" in c}
    assert "bold" in types and "link" in types
    assert types["link"]["attrs"]["href"]
    # the four old keys are still written
    assert {"open", "close", "standalone", "label"} <= set(types["bold"])
    # the dict goes back through merge unchanged
    assert bilingua.merge(doc)[:2] == b"PK"


def test_arb_polish_target_has_plural_slots_and_added_units():
    doc = bilingua.extract("app_en.arb", ARB, source_lang="en", target_lang="pl")
    slots = [u["plural"] for u in doc["units"] if "plural" in u]
    assert {s["category"] for s in slots} == {"one", "few", "many", "other"}
    assert {s["group"] for s in slots} == {slots[0]["group"]}
    assert all(s["kind"] == "cardinal" and s["arg"] == "count" for s in slots)
    added = sorted(s["category"] for s in slots if s.get("added"))
    assert added == ["few", "many"]
    # `added` is missing (not False) on the source's own forms; the message without a plural has no slot
    assert all("added" not in s for s in slots if s["category"] in ("one", "other"))
    hi = next(u for u in doc["units"] if u["context"].get("key") == "hi")
    assert "plural" not in hi
    # without a target nothing is added
    plain = bilingua.extract("app_en.arb", ARB, source_lang="en")
    assert len(doc["units"]) == len(plain["units"]) + 2
    assert not any(u["plural"].get("added") for u in plain["units"] if "plural" in u)
    # untranslated, the added units write the source `other` form, as the fill did (a merge with nothing
    # translated gives the file back as it was, so translate another message)
    hi["segments"][0]["target"] = [{"text": "Cześć"}]
    out = bilingua.merge(doc).decode()
    assert "few{# files}" in out and "many{# files}" in out


def test_locos_profile_dicts_have_none_of_the_new_keys():
    docx = bilingua.extract("features.docx", FEATURES, profile="locos")
    arb = bilingua.extract("app_en.arb", ARB, source_lang="en", target_lang="pl", profile="locos")
    assert new_keys(docx) == []
    assert new_keys(arb) == []
    # and the LocOS arb has no added units
    assert len(arb["units"]) == len(bilingua.extract("app_en.arb", ARB, source_lang="en", profile="locos")["units"])
    # the same files outside the profile do have them
    assert new_keys(bilingua.extract("features.docx", FEATURES))
    assert new_keys(bilingua.extract("app_en.arb", ARB, source_lang="en", target_lang="pl"))


def test_a_changed_target_has_its_own_error():
    doc = bilingua.extract("app_en.arb", ARB, source_lang="en", target_lang="pl")
    many = next(u for u in doc["units"] if u.get("plural", {}).get("category") == "many")
    doc["target_lang"] = "sl"
    try:
        bilingua.merge(doc)
    except bilingua.BilinguaError as e:
        assert e.code == "target_changed"
        assert str(e) == (
            f"unit {many['id']} is a plural form added for another target language; extract the file again for this target"
        )
    else:
        raise AssertionError("merged for another target")
