"""Writes tests/data/golden_stamps.json: what the LocOS filter writes for the target language. For every corpus file
and Okapi package, every unit translated, 12 target locales: the language and direction attributes of content.xml and
styles.xml (sorted "name=value" lists) and the meta.xml dc:language. Read-only use of LocOS; run with its venv python:

    python tools/gen_golden.py LOCOS_DIR CORPUS_DIR OKAPI_DIR OUT.json
"""
import io, json, os, sys, tempfile, zipfile

locos, corpus, okapi, out = sys.argv[1:5]
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, locos)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402
from lxml import etree  # noqa: E402

LANGS = ["de-DE", "es-ES", "fr-FR", "it-IT", "nl-NL", "pt-BR", "ja-JP", "zh-CN", "ko-KR", "ar-SA", "th-TH", "he-IL"]
KEEP = {"language", "country", "script", "language-asian", "country-asian", "script-asian", "language-complex",
        "country-complex", "script-complex", "writing-mode"}
FO = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
ST = "urn:oasis:names:tc:opendocument:xmlns:style:1.0"


def stamps(data, part):
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        if part not in z.namelist():
            return []
        root = etree.fromstring(z.read(part))
    res = []
    for el in root.iter():
        for k, v in el.attrib.items():
            ns, _, local = k[1:].partition("}") if k.startswith("{") else ("", "", k)
            if ns in (FO, ST) and local in KEEP:
                res.append(f"{local}={v}")
    return sorted(res)


def dc_lang(data):
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        if "meta.xml" not in z.namelist():
            return None
        root = etree.fromstring(z.read("meta.xml"))
    el = root.find(".//{http://purl.org/dc/elements/1.1/}language")
    return None if el is None else el.text


res = {}
for tag, d in (("corpus", os.path.join(corpus, "odf")), ("okapi", okapi)):
    for name in sorted(os.listdir(d)):
        ext = name.rsplit(".", 1)[-1]
        if ext not in ("odt", "ods", "odp", "odg"):
            continue
        data = open(os.path.join(d, name), "rb").read()
        f = get_filter(ext)
        pr = f.parse(data)
        tr = {str(u.p_index): "\u00ab" + u.text + "\u00bb" for u in pr.units}
        ent = {}
        for lang in LANGS:
            o = f.build_localized(data, tr, lang)
            ent[lang] = {"content": stamps(o, "content.xml"), "styles": stamps(o, "styles.xml"), "meta": dc_lang(o)}
        res[f"{tag}/{name}"] = ent
open(out, "w", encoding="utf-8").write(json.dumps(res, ensure_ascii=False, indent=0))
print(len(res), "files")
