//! Python bindings for Bilingua. The Document crosses the boundary as plain Python dicts and lists, built from
//! its serde JSON form, so new model fields flow through without changes here.

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
    let err = BilinguaError::new_err(f.msg);
    // Setting an attribute on a fresh exception instance cannot fail in practice; ignore if it does.
    let _ = err.value(py).setattr("code", f.code);
    let _ = err.value(py).setattr("detail", f.detail.as_deref().unwrap_or(&f.msg));
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
        let mut v = serde_json::to_value(&doc).map_err(|e| Fail::new("internal", e.to_string()))?;
        if let Value::Object(m) = &mut v {
            m.insert(PARSER_KEY.to_string(), parser_info(&doc, prof));
        }
        Ok(v)
    })?;
    to_py(py, &value)
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
    let mut value = from_py(document, 0).map_err(|f| to_py_err(py, f))?;
    // the parser record extract adds is information for the caller, not part of the model's document
    let stamped = value.as_object_mut().and_then(|m| m.remove(PARSER_KEY));
    // a profile gives the defaults (LocOS: UTF-8 out except ini, strings, dxf, txt, properties, xaml and resjson, stamp always);
    // keywords override them. Without a version in the call, the version the document was extracted under is used.
    let format = value.get("format").and_then(|f| f.as_str()).unwrap_or("").to_string();
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
        let doc: Document = serde_json::from_value(value).map_err(|e| Fail::new("invalid_document", e.to_string()))?;
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

/// A Document from its dict, the parser record dropped; also returns that record.
fn document_from_value(mut value: Value) -> Result<(Document, Option<Value>), Fail> {
    let stamped = value.as_object_mut().and_then(|m| m.remove(PARSER_KEY));
    let doc = serde_json::from_value(value).map_err(|e| Fail::new("invalid_document", e.to_string()))?;
    Ok((doc, stamped))
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

/// A segment's inline list as Localization OS text and its token map, numbered by the LocOS rule of `format`
/// (pairs by id for html, tag ids as numbers for docx, pptx and xlsx, positional otherwise; look-alike text in the
/// source keeps its number). The map is keyed by token number: `{id, letter, open, close, free, unpaired?}`.
#[pyfunction]
fn to_locos_text(py: Python<'_>, format: &str, inlines: &Bound<'_, PyAny>) -> PyResult<(String, Py<PyAny>)> {
    let inl = inlines_from_py(inlines).map_err(|f| to_py_err(py, f))?;
    let (text, map) = engine::profile::tokens::to_locos_text_for(format, &inl);
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
    let value = from_py(document, 0).map_err(|f| to_py_err(py, f))?;
    let units = guarded(py, move || {
        let (doc, _) = document_from_value(value)?;
        serde_json::to_value(engine::profile::locos_units(&doc)).map_err(|e| Fail::new("internal", e.to_string()))
    })?;
    to_py(py, &units)
}

/// Sets Localization OS translations on a Document extracted with `profile="locos"`: `targets` maps a segment id to
/// LocOS text (tokens as `to_locos_text` writes them for that segment's source). Each target is set the way LocOS
/// means it (an empty text is a translation to nothing, white space only is kept, a standalone placeholder may
/// repeat). Returns `(document, problems)`: the Document with the targets set (its parser record kept) and, per
/// segment id, the tokens kept as text (see `from_locos_text`; for TOML and JS a token of the wrong kind that
/// names a code of the unit is dropped, as LocOS drops it, and still listed); a segment with none is left out. A target the
/// document cannot take raises BilinguaError (`unknown_segment`, `locked`, `invented_tag`, `duplicated_tag`) naming
/// the segment.
#[pyfunction]
fn locos_set_targets(py: Python<'_>, document: &Bound<'_, PyAny>, targets: &Bound<'_, PyAny>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
    let value = from_py(document, 0).map_err(|f| to_py_err(py, f))?;
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
        let (mut doc, stamped) = document_from_value(value)?;
        let maps: std::collections::HashMap<String, engine::profile::tokens::LocosMap> =
            engine::profile::locos_units(&doc).into_iter().map(|u| (u.segment_id, u.map)).collect();
        let empty = engine::profile::tokens::LocosMap::new();
        let mut problems = Map::new();
        for (seg, text) in targets {
            // a segment the numbering does not list (not translatable, or no such id) still goes to the model's
            // checks, with an empty map: they name the problem
            let map = maps.get(&seg).unwrap_or(&empty);
            // the format's LocOS token rules (TOML and JS drop a wrong-kind token naming a code, #814.1)
            let (inl, found) = engine::profile::tokens::parse_locos_text_for(&doc.format, &text, map);
            engine::profile::locos_set_target(&mut doc, &seg, inl).map_err(|e| {
                let f = Fail::from(e);
                Fail::new(f.code, format!("segment {seg}: {}", f.msg))
            })?;
            if !found.is_empty() {
                problems.insert(seg, Value::Array(found.iter().map(problem_value).collect()));
            }
        }
        let mut v = serde_json::to_value(&doc).map_err(|e| Fail::new("internal", e.to_string()))?;
        if let (Value::Object(m), Some(p)) = (&mut v, stamped) {
            m.insert(PARSER_KEY.to_string(), p);
        }
        Ok((v, Value::Object(problems)))
    })?;
    Ok((to_py(py, &doc)?, to_py(py, &problems)?))
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
