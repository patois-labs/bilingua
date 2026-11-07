//! Python bindings for Bilingua. The Document crosses the boundary as plain Python dicts and lists, in its serde
//! JSON form, so new model fields flow through without changes here. It is written straight into Python objects and
//! read straight from them (`doc_to_py`, `doc_in`), with no `serde_json::Value` copy in between; the objects, key
//! order and errors are the ones the `Value` path gives.

mod pyde;

use engine::{Document, ExtractOptions, Granularity};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Number, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};

create_exception!(
    bilingua,
    BilinguaError,
    PyException,
    "Raised for every Bilingua failure. Has a `code` attribute and a `detail` attribute (the parser's own sentence, without the `not a <format> file:` prefix a `parse` error's message carries)."
);

/// An error with a stable machine code.
struct Fail {
    code: &'static str,
    msg: String,
    /// The parser's own sentence (`Error::Parse` without its prefix); `msg` for every other error.
    detail: Option<String>,
}

impl Fail {
    fn new(code: &'static str, msg: impl Into<String>) -> Fail {
        Fail { code, msg: msg.into(), detail: None }
    }
}

impl From<engine::Error> for Fail {
    fn from(e: engine::Error) -> Fail {
        use engine::Error as E;
        let code = match &e {
            E::Parse { .. } => "parse",
            E::Unsupported(_) => "unsupported",
            E::UnknownSegment(_) => "unknown_segment",
            E::Locked(_) => "locked",
            E::InventedTag(_) => "invented_tag",
            E::DuplicatedTag(_) => "duplicated_tag",
            E::ParserVersion { .. } => "parser_version",
            E::Write(_) => "write",
            E::Unwritable { .. } => "unwritable",
            E::Limit(_) => "limit",
            E::Encoding(_) => "encoding",
            E::Io(_) => "io",
            // the LocOS adapter maps this code to its own invalid_argument
            E::FormatOption { .. } | E::UnknownProfile(_) => "invalid_argument",
            _ => "error",
        };
        let detail = match &e {
            E::Parse { msg, .. } => msg.clone(),
            other => other.to_string(),
        };
        Fail { code, msg: e.to_string(), detail: Some(detail) }
    }
}

fn to_py_err(py: Python<'_>, f: Fail) -> PyErr {
    let detail = f.detail.unwrap_or_else(|| f.msg.clone());
    let err = BilinguaError::new_err(f.msg);
    // Setting an attribute on a fresh exception instance cannot fail in practice; ignore if it does.
    let _ = err.value(py).setattr("code", f.code);
    let _ = err.value(py).setattr("detail", detail);
    err
}

/// Runs `f` without the GIL and turns a panic into a `Fail` with code "internal".
fn guarded<T: Send>(py: Python<'_>, f: impl FnOnce() -> Result<T, Fail> + Send) -> PyResult<T> {
    let r = py.detach(|| catch_unwind(AssertUnwindSafe(f)));
    match r {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(f)) => Err(to_py_err(py, f)),
        Err(_) => Err(to_py_err(py, Fail::new("internal", "internal error in the Bilingua engine"))),
    }
}

fn to_py(py: Python<'_>, v: &Value) -> PyResult<Py<PyAny>> {
    Ok(match v {
        Value::Null => py.None(),
        Value::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any().unbind()
            } else if let Some(u) = n.as_u64() {
                u.into_pyobject(py)?.into_any().unbind()
            } else {
                PyFloat::new(py, n.as_f64().unwrap_or(0.0)).into_any().unbind()
            }
        }
        Value::String(s) => PyString::new(py, s).into_any().unbind(),
        Value::Array(a) => {
            let items = a.iter().map(|x| to_py(py, x)).collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, items)?.into_any().unbind()
        }
        Value::Object(m) => {
            let d = PyDict::new(py);
            for (k, x) in m {
                d.set_item(k, to_py(py, x)?)?;
            }
            d.into_any().unbind()
        }
    })
}

fn from_py(obj: &Bound<'_, PyAny>, depth: usize) -> Result<Value, Fail> {
    let bad = |m: String| Fail::new("invalid_document", m);
    if depth > 200 {
        return Err(bad("document is nested too deeply".into()));
    }
    if obj.is_none() {
        return Ok(Value::Null);
    }
    // bool before int: bool is a subclass of int in Python.
    if let Ok(b) = obj.downcast::<PyBool>() {
        return Ok(Value::Bool(b.is_true()));
    }
    if obj.is_instance_of::<PyInt>() {
        if let Ok(i) = obj.extract::<i64>() {
            return Ok(Value::Number(i.into()));
        }
        if let Ok(u) = obj.extract::<u64>() {
            return Ok(Value::Number(u.into()));
        }
        return Err(bad("integer out of range".into()));
    }
    if obj.is_instance_of::<PyFloat>() {
        let f: f64 = obj.extract().map_err(|e| bad(e.to_string()))?;
        return Number::from_f64(f).map(Value::Number).ok_or_else(|| bad("NaN or infinity in document".into()));
    }
    if let Ok(s) = obj.downcast::<PyString>() {
        return Ok(Value::String(s.to_str().map_err(|e| bad(e.to_string()))?.to_string()));
    }
    if let Ok(d) = obj.downcast::<PyDict>() {
        let mut m = Map::new();
        for (k, v) in d.iter() {
            let key = k.downcast::<PyString>().map_err(|_| bad("dict keys must be strings".into()))?;
            let key = key.to_str().map_err(|e| bad(e.to_string()))?.to_string();
            m.insert(key, from_py(&v, depth + 1)?);
        }
        return Ok(Value::Object(m));
    }
    if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
        let mut out = Vec::new();
        for x in obj.try_iter().map_err(|e| bad(e.to_string()))? {
            out.push(from_py(&x.map_err(|e| bad(e.to_string()))?, depth + 1)?);
        }
        return Ok(Value::Array(out));
    }
    if let Ok(b) = obj.downcast::<PyBytes>() {
        // Raw zip entries may be passed as bytes; the model reads them as a base64 string.
        return Ok(Value::String(engine::b64::encode(b.as_bytes())));
    }
    let tn = obj.get_type().name().map(|n| n.to_string()).unwrap_or_default();
    Err(bad(format!("cannot use a {tn} in a document")))
}

/// All formats: a list of `{"id", "extensions", "bilingual"}`.
#[pyfunction]
fn formats(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let list: Vec<Value> = engine::filters()
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id(),
                "extensions": f.extensions(),
                "bilingual": f.bilingual(),
            })
        })
        .collect();
    to_py(py, &Value::Array(list))
}

/// The format id for a file name and its bytes, or None when no parser accepts it.
#[pyfunction]
fn detect(py: Python<'_>, name: &str, data: &[u8]) -> PyResult<Option<String>> {
    let (name, data) = (name.to_string(), data.to_vec());
    guarded(py, move || Ok(engine::detect(&name, &data).map(|f| f.id().to_string())))
}

fn parse_granularity(s: &str) -> Result<Granularity, Fail> {
    match s {
        "sentence" => Ok(Granularity::Sentence),
        "paragraph" => Ok(Granularity::Paragraph),
        "document" => Ok(Granularity::Document),
        other => Err(Fail::new(
            "invalid_argument",
            format!("granularity must be sentence, paragraph or document, not {other}"),
        )),
    }
}

/// `format_options` as text: a dict of str to str, bool or int.
fn format_option_map(v: Value) -> Result<std::collections::BTreeMap<String, String>, Fail> {
    let Value::Object(m) = v else {
        return Err(Fail::new("invalid_argument", "format_options must be a dict"));
    };
    let mut out = std::collections::BTreeMap::new();
    for (k, v) in m {
        let s = match v {
            Value::String(s) => s,
            Value::Bool(b) => b.to_string(),
            Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
            other => return Err(Fail::new("invalid_argument", format!("format_options[{k:?}] must be str, bool or int, not {other}"))),
        };
        out.insert(k, s);
    }
    Ok(out)
}

/// Opens a file and returns the Document as dicts and lists.
#[pyfunction]
#[allow(clippy::too_many_arguments)] // one keyword argument per option
#[pyo3(signature = (name, data, *, source_lang=None, target_lang=None, granularity="paragraph", limits=None, format_options=None, profile=None, profile_version=None))]
fn extract(
    py: Python<'_>,
    name: &str,
    data: &[u8],
    source_lang: Option<String>,
    target_lang: Option<String>,
    granularity: &str,
    limits: Option<&Bound<'_, PyAny>>,
    format_options: Option<&Bound<'_, PyAny>>,
    profile: Option<String>,
    profile_version: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let g = parse_granularity(granularity).map_err(|f| to_py_err(py, f))?;
    let prof = resolve_profile(profile.as_deref(), profile_version).map_err(|f| to_py_err(py, f))?;
    // A dict of limit names (see `Limits`); anything left out keeps Localization OS's default.
    let limits: Option<engine::Limits> = match limits {
        None => None,
        Some(l) if l.is_none() => None,
        Some(l) => {
            let v = from_py(l, 0).map_err(|f| to_py_err(py, f))?;
            Some(serde_json::from_value(v).map_err(|e| to_py_err(py, Fail::new("invalid_argument", format!("limits: {e}"))))?)
        }
    };
    // Options only one format knows, e.g. {"ftl.translate_terms": True, "csv.columns": "source=1"}; values are
    // str, bool or int (sent to the engine as text: true/false, decimal).
    let format_options: std::collections::BTreeMap<String, String> = match format_options {
        None => Default::default(),
        Some(o) if o.is_none() => Default::default(),
        Some(o) => {
            let v = from_py(o, 0).map_err(|f| to_py_err(py, f))?;
            format_option_map(v).map_err(|f| to_py_err(py, f))?
        }
    };
    let (name, data) = (name.to_string(), data.to_vec());
    let value = guarded(py, move || {
        let opts = ExtractOptions::new()
            .with_source_lang(source_lang)
            .with_target_lang(target_lang)
            .with_granularity(g)
            .with_limits(limits)
            .with_format_options(format_options)
            .with_profile(prof.map(|p| p.name));
        let doc = engine::extract(&name, &data, &opts)?;
        let parser = parser_info(&doc, prof);
        Ok((doc, parser))
    })?;
    doc_to_py(py, &value.0, Some(&value.1))
}

/// Python dicts that come out with their keys sorted, as `to_py` writes a `Value` object (serde_json's map is a
/// BTreeMap here): the same objects in the same order as the old `to_value` + `to_py` path.
struct SortedDict;

impl pythonize::PythonizeMappingType for SortedDict {
    type Builder<'py> = (Python<'py>, Vec<(Bound<'py, PyString>, Bound<'py, PyAny>)>);

    fn builder<'py>(py: Python<'py>, len: Option<usize>) -> PyResult<Self::Builder<'py>> {
        Ok((py, Vec::with_capacity(len.unwrap_or(0))))
    }

    fn push_item<'py>(builder: &mut Self::Builder<'py>, key: Bound<'py, PyAny>, value: Bound<'py, PyAny>) -> PyResult<()> {
        // a key that is not a str fails the fast path; doc_to_py then takes the Value path
        builder.1.push((key.downcast_into::<PyString>()?, value));
        Ok(())
    }

    fn finish<'py>(builder: Self::Builder<'py>) -> PyResult<Bound<'py, pyo3::types::PyMapping>> {
        let (py, mut items) = builder;
        // byte order, as BTreeMap<String, _> orders; a struct never repeats a key. Every key was made from a
        // Rust &str, so to_str cannot fail; the check stays for safety.
        for (k, _) in &items {
            k.to_str()?;
        }
        items.sort_by(|a, b| a.0.to_str().unwrap_or("").cmp(b.0.to_str().unwrap_or("")));
        let d = PyDict::new(py);
        for (k, v) in items {
            d.set_item(k, v)?;
        }
        Ok(d.into_mapping())
    }
}

struct SortedTypes;

impl pythonize::PythonizeTypes for SortedTypes {
    type Map = SortedDict;
    type NamedMap = pythonize::PythonizeUnnamedMappingAdapter<SortedDict>;
    type List = PyList;
}

/// A Document with the parser record as one more top-level key (sorted in with the others).
#[derive(serde::Serialize)]
struct WithParser<'a> {
    #[serde(flatten)]
    doc: &'a Document,
    parser: &'a Value,
}

/// The Document as Python dicts and lists, with the parser record when there is one. Written straight into Python
/// objects; if that ever fails, the old way (`to_value`, then `to_py`) gives the answer.
fn doc_to_py(py: Python<'_>, doc: &Document, parser: Option<&Value>) -> PyResult<Py<PyAny>> {
    let fast = match parser {
        Some(p) => pythonize::pythonize_custom::<SortedTypes, _>(py, &WithParser { doc, parser: p }),
        None => pythonize::pythonize_custom::<SortedTypes, _>(py, doc),
    };
    if let Ok(obj) = fast {
        return Ok(obj.unbind());
    }
    let mut v = serde_json::to_value(doc).map_err(|e| to_py_err(py, Fail::new("internal", e.to_string())))?;
    if let (Value::Object(m), Some(p)) = (&mut v, parser) {
        m.insert(PARSER_KEY.to_string(), p.clone());
    }
    to_py(py, &v)
}

/// A Document handed in by the caller: read already, or (the old path) a `Value` still to be read, so that a
/// malformed document fails at the same step, with the same error, as before.
enum DocIn {
    Ready(Box<Document>),
    Raw(Value),
}

impl DocIn {
    /// The document's `format` string ("" when it has none), as the old path read it from the `Value`.
    fn format(&self) -> String {
        match self {
            DocIn::Ready(d) => d.format.clone(),
            DocIn::Raw(v) => v.get("format").and_then(|f| f.as_str()).unwrap_or("").to_string(),
        }
    }

    fn into_document(self) -> Result<Document, Fail> {
        match self {
            DocIn::Ready(d) => Ok(*d),
            DocIn::Raw(v) => serde_json::from_value(v).map_err(|e| Fail::new("invalid_document", e.to_string())),
        }
    }
}

/// Reads a Document dict and the parser record `extract` added. The fast path reads the Python objects directly
/// (`pyde`); anything it does not take goes the old way, `from_py` (whose conversion errors are raised here, as
/// before) and a `Value` read later by `DocIn::into_document`.
fn doc_in(obj: &Bound<'_, PyAny>) -> Result<(DocIn, Option<Value>), Fail> {
    if let Ok(d) = obj.downcast::<PyDict>() {
        let stamped = match d.get_item(PARSER_KEY) {
            // the record sits one level down, as it did inside the whole document's from_py
            Ok(Some(p)) => from_py(&p, 1).ok().map(Some),
            Ok(None) => Some(None),
            Err(_) => None,
        };
        if let Some(stamped) = stamped {
            if let Ok(doc) = <Document as serde::Deserialize>::deserialize(pyde::De::new(obj.clone())) {
                return Ok((DocIn::Ready(Box::new(doc)), stamped));
            }
        }
    }
    let mut value = from_py(obj, 0)?;
    let stamped = value.as_object_mut().and_then(|m| m.remove(PARSER_KEY));
    Ok((DocIn::Raw(value), stamped))
}

/// Writes a Document (as returned by `extract`, with targets filled in) back to file bytes.
///
/// Merge options are keyword-only, like extract's: `output_encoding` ("keep", the default, or "utf-8", which
/// writes UTF-8 and makes charset declarations say so) and `stamp_target_always` (bool: stamp the target language
/// and fill plurals even when nothing is translated, as Localization OS does).
#[pyfunction]
#[pyo3(signature = (document, *, output_encoding=None, stamp_target_always=None, profile=None, format_options=None, profile_version=None))]
fn merge<'py>(
    py: Python<'py>,
    document: &Bound<'py, PyAny>,
    output_encoding: Option<&str>,
    stamp_target_always: Option<bool>,
    profile: Option<String>,
    format_options: Option<&Bound<'py, PyAny>>,
    profile_version: Option<u32>,
) -> PyResult<Bound<'py, PyBytes>> {
    // the parser record extract adds is information for the caller, not part of the model's document
    let (doc, stamped) = doc_in(document).map_err(|f| to_py_err(py, f))?;
    // a profile gives the defaults (LocOS: UTF-8 out except ini, strings, dxf, txt, properties, xaml and resjson, stamp always);
    // keywords override them. Without a version in the call, the version the document was extracted under is used.
    let format = doc.format();
    let base = profile.as_deref().map(|p| p.split('@').next().unwrap_or(p));
    let stamped_version = stamped
        .as_ref()
        .filter(|p| base.is_some() && p.get("profile").and_then(Value::as_str) == base)
        .and_then(|p| p.get("profile_version"))
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok());
    let spec_has_version = profile.as_deref().is_some_and(|p| p.contains('@'));
    let want = profile_version.or(if spec_has_version { None } else { stamped_version });
    let prof = resolve_profile(profile.as_deref(), want).map_err(|f| to_py_err(py, f))?;
    let mut opts = match prof {
        None => engine::MergeOptions::new(),
        Some(p) => p.merge_options(&format),
    };
    if let Some(o) = format_options.filter(|o| !o.is_none()) {
        let v = from_py(o, 0).map_err(|f| to_py_err(py, f))?;
        opts = opts.with_format_options(format_option_map(v).map_err(|f| to_py_err(py, f))?);
    }
    if let Some(e) = output_encoding {
        opts = opts.with_output_encoding(parse_output_encoding(e).map_err(|f| to_py_err(py, f))?);
    }
    if let Some(b) = stamp_target_always {
        opts = opts.with_stamp_target_always(b);
    }
    let bytes = guarded(py, move || {
        let doc = doc.into_document()?;
        Ok(engine::merge_with(&doc, &opts)?)
    })?;
    Ok(PyBytes::new(py, &bytes))
}

fn parse_output_encoding(s: &str) -> Result<engine::OutputEncoding, Fail> {
    match s.to_ascii_lowercase().as_str() {
        "keep" => Ok(engine::OutputEncoding::Keep),
        "utf-8" | "utf8" => Ok(engine::OutputEncoding::Utf8),
        other => Err(Fail::new("invalid_argument", format!("output_encoding must be \"keep\" or \"utf-8\", not {other:?}"))),
    }
}

/// The key of the parser record `extract` adds to the Document dict (`merge` and the `locos_*` calls ignore it).
const PARSER_KEY: &str = "parser";

/// A profile name (`"locos"`, `"locos@1"`) and an optional version to the profile Bilingua will use: an unknown
/// name is `invalid_argument`, a version this release cannot produce is `parser_version`.
fn resolve_profile(spec: Option<&str>, version: Option<u32>) -> Result<Option<engine::profile::Profile>, Fail> {
    match spec {
        None if version.is_some() => Err(Fail::new("invalid_argument", "profile_version needs a profile")),
        None => Ok(None),
        Some(s) => Ok(Some(engine::profile::resolve(s, version)?)),
    }
}

/// What made a document, for the caller to stamp: the Bilingua release, the format crate, its version and its
/// skeleton schema, the profile and its version.
fn parser_info(doc: &Document, prof: Option<engine::profile::Profile>) -> Value {
    let p = doc.skeleton.parser();
    serde_json::json!({
        "bilingua": env!("CARGO_PKG_VERSION"),
        "format": doc.format,
        "crate": p.map(|p| p.name.clone()),
        "crate_version": p.map(|p| p.version.clone()),
        "schema": p.map(|p| p.schema),
        "profile": prof.map(|p| p.name),
        "profile_version": prof.map(|p| p.version),
    })
}

fn inlines_from_py(obj: &Bound<'_, PyAny>) -> Result<Vec<engine::Inline>, Fail> {
    let v = from_py(obj, 0)?;
    serde_json::from_value(v).map_err(|e| Fail::new("invalid_argument", format!("inlines: {e}")))
}

fn locos_map_from_py(obj: &Bound<'_, PyAny>) -> Result<engine::profile::tokens::LocosMap, Fail> {
    let v = from_py(obj, 0)?;
    serde_json::from_value(v).map_err(|e| Fail::new("invalid_argument", format!("token map: {e}")))
}

fn problem_value(p: &engine::profile::tokens::TokenProblem) -> Value {
    use engine::profile::tokens::TokenProblem as P;
    match p {
        P::Stray { at } => serde_json::json!({"kind": "stray", "at": at}),
        P::Unknown { token } => serde_json::json!({"kind": "unknown", "token": token}),
        P::KindMismatch { token } => serde_json::json!({"kind": "kind_mismatch", "token": token}),
    }
}

fn repair_value(r: &engine::profile::tokens::TagRepair) -> Value {
    use engine::profile::tokens::TagRepair as R;
    let (kind, token) = match r {
        R::Repeated { token } => ("repeated", token.clone()),
        R::StrayClose { token } => ("stray_close", token.clone()),
        R::Unclosed { token } => ("unclosed", token.clone()),
        R::PairAsPoint { token } => ("pair_as_point", token.clone()),
        other => ("repaired", format!("{other:?}")),
    };
    serde_json::json!({"kind": kind, "token": token})
}

/// A segment's inline list as Localization OS text and its token map, numbered by the LocOS rule of `format`
/// (pairs by id for html, tag ids as numbers for docx, pptx and xlsx, positional otherwise; look-alike text in the
/// source keeps its number). The map is keyed by token number: `{id, letter, open, close, free, unpaired?}`.
/// `codes` (keyword, optional): the unit's `codes` dict; for idml and icml a pair whose code is labelled `run` is
/// then an `r` pair, as LocOS writes it and as `locos_units` gives it (without `codes`, every pair is `g`).
#[pyfunction]
#[pyo3(signature = (format, inlines, *, codes=None))]
fn to_locos_text(py: Python<'_>, format: &str, inlines: &Bound<'_, PyAny>, codes: Option<&Bound<'_, PyAny>>) -> PyResult<(String, Py<PyAny>)> {
    let inl = inlines_from_py(inlines).map_err(|f| to_py_err(py, f))?;
    let (text, map) = match codes.filter(|c| !c.is_none()) {
        None => engine::profile::tokens::to_locos_text_for(format, &inl),
        Some(c) => {
            let v = from_py(c, 0).map_err(|f| to_py_err(py, f))?;
            let codes: std::collections::BTreeMap<String, engine::CodeData> =
                serde_json::from_value(v).map_err(|e| to_py_err(py, Fail::new("invalid_argument", format!("codes: {e}"))))?;
            engine::profile::tokens::to_locos_text_coded(format, &inl, &codes)
        }
    };
    let m = serde_json::to_value(&map).map_err(|e| to_py_err(py, Fail::new("internal", e.to_string())))?;
    Ok((text, to_py(py, &m)?))
}

/// Localization OS text back to an inline list, with the map `to_locos_text` gave. A token that is malformed,
/// names no code in the map, or has the wrong kind stays in the text as it is (LocOS keeps such tokens as text);
/// each one is listed in the second item as `{"kind": "stray", "at"}`, `{"kind": "unknown", "token"}` or
/// `{"kind": "kind_mismatch", "token"}`.
#[pyfunction]
fn from_locos_text(py: Python<'_>, text: &str, map: &Bound<'_, PyAny>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
    let map = locos_map_from_py(map).map_err(|f| to_py_err(py, f))?;
    let (inl, problems) = engine::profile::tokens::parse_locos_text(text, &map);
    let inl = serde_json::to_value(&inl).map_err(|e| to_py_err(py, Fail::new("internal", e.to_string())))?;
    let problems = Value::Array(problems.iter().map(problem_value).collect());
    Ok((to_py(py, &inl)?, to_py(py, &problems)?))
}

/// The translatable segments of a Document extracted with `profile="locos"`, numbered the way Localization OS
/// numbers its units: a list of `{p_index, segment_id, text, map, style}` (text with LocOS tokens, map as from
/// `to_locos_text`).
#[pyfunction]
fn locos_units(py: Python<'_>, document: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let (doc, _) = doc_in(document).map_err(|f| to_py_err(py, f))?;
    let units = guarded(py, move || {
        let doc = doc.into_document()?;
        Ok(engine::profile::locos_units(&doc))
    })?;
    // written straight into Python objects with sorted keys (the same dicts and lists as `to_value` + `to_py`, without
    // the `Value` copy: W4 speed); if that ever fails, the old way gives the answer
    if let Ok(obj) = pythonize::pythonize_custom::<SortedTypes, _>(py, &units) {
        return Ok(obj.unbind());
    }
    let v = serde_json::to_value(&units).map_err(|e| to_py_err(py, Fail::new("internal", e.to_string())))?;
    to_py(py, &v)
}

/// Sets Localization OS translations on a Document extracted with `profile="locos"`: `targets` maps a segment id to
/// LocOS text (tokens as `to_locos_text` writes them for that segment's source). Each target is set the way LocOS
/// means it (an empty text is a translation to nothing, white space only is kept, a standalone placeholder may
/// repeat). Returns `(document, problems)`: the Document with the targets set (its parser record kept) and, per
/// segment id, the tokens kept as text (see `from_locos_text`; for TOML and JS a token of the wrong kind that
/// names a code of the unit is dropped, as LocOS drops it, and still listed) and, for the XLIFF family, TXML, TTX
/// and the packages, each tag repair LocOS's writers make (`repeated`, `stray_close`, `unclosed`,
/// `pair_as_point`: the tag dropped, closed at the end or written empty, the text kept); a segment with none is left out. A target the
/// document cannot take raises BilinguaError (`unknown_segment`, `locked`, `invented_tag`, `duplicated_tag`) naming
/// the segment.
#[pyfunction]
fn locos_set_targets(py: Python<'_>, document: &Bound<'_, PyAny>, targets: &Bound<'_, PyAny>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
    let (doc, stamped) = doc_in(document).map_err(|f| to_py_err(py, f))?;
    let targets = match from_py(targets, 0).map_err(|f| to_py_err(py, f))? {
        Value::Object(m) => {
            let mut out = Vec::with_capacity(m.len());
            for (k, v) in m {
                match v {
                    Value::String(s) => out.push((k, s)),
                    other => return Err(to_py_err(py, Fail::new("invalid_argument", format!("targets[{k:?}] must be str, not {other}")))),
                }
            }
            out
        }
        _ => return Err(to_py_err(py, Fail::new("invalid_argument", "targets must be a dict of segment id to text"))),
    };
    let (doc, problems) = guarded(py, move || {
        let mut doc = doc.into_document()?;
        let maps: std::collections::HashMap<String, engine::profile::tokens::LocosMap> =
            engine::profile::locos_units(&doc).into_iter().map(|u| (u.segment_id, u.map)).collect();
        let empty = engine::profile::tokens::LocosMap::new();
        let mut problems = Map::new();
        for (seg, text) in targets {
            // a segment the numbering does not list (not translatable, or no such id) still goes to the model's
            // checks, with an empty map: they name the problem
            let map = maps.get(&seg).unwrap_or(&empty);
            // the format's LocOS token rules (TOML and JS drop a wrong-kind token naming a code, #814.1)
            // the XLIFF family, TXML, TTX and the packages repair a repeated, stray or unclosed tag as LocOS's
            // writers do; each repair is listed with the problems
            let (inl, found, repairs) = engine::profile::tokens::repair_locos_text_for(&doc.format, &text, map);
            engine::profile::locos_set_target(&mut doc, &seg, inl).map_err(|e| {
                let f = Fail::from(e);
                Fail::new(f.code, format!("segment {seg}: {}", f.msg))
            })?;
            if !found.is_empty() || !repairs.is_empty() {
                let all = found.iter().map(problem_value).chain(repairs.iter().map(repair_value)).collect();
                problems.insert(seg, Value::Array(all));
            }
        }
        Ok((doc, Value::Object(problems)))
    })?;
    Ok((doc_to_py(py, &doc, stamped.as_ref())?, to_py(py, &problems)?))
}

/// The profiles Bilingua knows, as `{name: version}` (`{"locos": 1}`).
#[pyfunction]
fn profiles(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let m: Map<String, Value> = engine::profile::profiles().iter().map(|p| (p.name.to_string(), Value::from(p.version))).collect();
    to_py(py, &Value::Object(m))
}

/// The Bilingua version.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule(name = "bilingua")]
fn init_module(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(formats, m)?)?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(merge, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(profiles, m)?)?;
    m.add_function(wrap_pyfunction!(to_locos_text, m)?)?;
    m.add_function(wrap_pyfunction!(from_locos_text, m)?)?;
    m.add_function(wrap_pyfunction!(locos_units, m)?)?;
    m.add_function(wrap_pyfunction!(locos_set_targets, m)?)?;
    m.add("BilinguaError", py.get_type::<BilinguaError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
