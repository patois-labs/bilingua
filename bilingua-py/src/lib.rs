//! Python bindings for Bilingua. The Document crosses the boundary as plain Python dicts and lists, built from
//! its serde JSON form, so new model fields flow through without changes here.

use engine::{Document, ExtractOptions, Granularity};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Number, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};

create_exception!(bilingua, BilinguaError, PyException, "Raised for every Bilingua failure. Has a `code` attribute.");

/// An error with a stable machine code.
struct Fail {
    code: &'static str,
    msg: String,
}

impl Fail {
    fn new(code: &'static str, msg: impl Into<String>) -> Fail {
        Fail { code, msg: msg.into() }
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
            E::Io(_) => "io",
        };
        Fail::new(code, e.to_string())
    }
}

fn to_py_err(py: Python<'_>, f: Fail) -> PyErr {
    let err = BilinguaError::new_err(f.msg);
    // Setting an attribute on a fresh exception instance cannot fail in practice; ignore if it does.
    let _ = err.value(py).setattr("code", f.code);
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

/// Opens a file and returns the Document as dicts and lists.
#[pyfunction]
#[pyo3(signature = (name, data, *, source_lang=None, target_lang=None, granularity="paragraph"))]
fn extract(
    py: Python<'_>,
    name: &str,
    data: &[u8],
    source_lang: Option<String>,
    target_lang: Option<String>,
    granularity: &str,
) -> PyResult<Py<PyAny>> {
    let g = parse_granularity(granularity).map_err(|f| to_py_err(py, f))?;
    let (name, data) = (name.to_string(), data.to_vec());
    let value = guarded(py, move || {
        let opts = ExtractOptions { source_lang, target_lang, granularity: g, include_locked: false };
        let doc = engine::extract(&name, &data, &opts)?;
        serde_json::to_value(&doc).map_err(|e| Fail::new("internal", e.to_string()))
    })?;
    to_py(py, &value)
}

/// Writes a Document (as returned by `extract`, with targets filled in) back to file bytes.
#[pyfunction]
fn merge<'py>(py: Python<'py>, document: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let value = from_py(document, 0).map_err(|f| to_py_err(py, f))?;
    let bytes = guarded(py, move || {
        let doc: Document = serde_json::from_value(value).map_err(|e| Fail::new("invalid_document", e.to_string()))?;
        Ok(engine::merge(&doc)?)
    })?;
    Ok(PyBytes::new(py, &bytes))
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
    m.add("BilinguaError", py.get_type::<BilinguaError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
