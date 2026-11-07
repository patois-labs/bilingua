//! `DocumentHandle`: a Document kept on the Rust side (BILI-18, speed). The dict path moves the whole Document across
//! the boundary on every call (extract writes it out, `locos_units`, `locos_set_targets` and `merge` read it back);
//! a handle never does, unless the caller asks for the dict (`to_dict`).
//!
//! A handle is immutable: `locos_set_targets` returns a new handle and leaves the one it was called on as it was,
//! as the dict function returns a new dict. So a handle needs no lock: any thread may use it at any time, and the
//! calls that do the work (`locos_units`, `locos_set_targets`, `merge`) release the GIL. The Document is freed
//! when the last handle that holds it is.
//!
//! Every answer is the one the dict path gives for the same Document (pinned by `tests/test_handle.py` over the
//! oracle corpus and the LocOS goldens): the dict path reads back exactly what it wrote, so working on the Document
//! itself skips a round trip, not a step.

use crate::{
    apply_locos_targets, doc_in, doc_to_py, guarded, merge_options, targets_from_py, to_py, to_py_err, units_to_py, Fail, SortedTypes,
};
use engine::Document;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use serde_json::Value;
use std::sync::Arc;

/// An extracted Document held by Bilingua. Made by `extract_handle` (or `DocumentHandle.from_dict`); read and
/// written with its methods, which give what the functions of the same name give for `to_dict()`.
#[pyclass(frozen, module = "bilingua", name = "DocumentHandle")]
pub struct DocumentHandle {
    doc: Arc<Document>,
    /// The parser record `extract` adds to the dict (`None` for a handle made from a dict without one).
    parser: Option<Value>,
}

impl DocumentHandle {
    pub fn new(doc: Document, parser: Option<Value>) -> DocumentHandle {
        DocumentHandle { doc: Arc::new(doc), parser }
    }
}

#[pymethods]
impl DocumentHandle {
    /// A handle for a Document dict (as `extract` returns it, or as stored by the caller): read once, with the
    /// checks and errors `merge` and `locos_units` give for a malformed one; its `parser` record is kept.
    #[staticmethod]
    fn from_dict(py: Python<'_>, document: &Bound<'_, PyAny>) -> PyResult<DocumentHandle> {
        let (doc, stamped) = doc_in(document).map_err(|f| to_py_err(py, f))?;
        let doc = guarded(py, move || doc.into_document())?;
        Ok(DocumentHandle::new(doc, stamped))
    }

    /// The Document as dicts and lists, exactly as `extract` returns it (or `locos_set_targets` for a handle it
    /// made), parser record included.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        doc_to_py(py, &self.doc, self.parser.as_ref())
    }

    /// `locos_units(handle.to_dict())`, without the dict.
    fn locos_units(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = Arc::clone(&self.doc);
        let units = guarded(py, move || Ok(engine::profile::locos_units(&doc)))?;
        units_to_py(py, &units)
    }

    /// `locos_set_targets(handle.to_dict(), targets)`, without the dicts: returns `(new_handle, problems)`. This
    /// handle is left as it was; on an error nothing changes.
    fn locos_set_targets(&self, py: Python<'_>, targets: &Bound<'_, PyAny>) -> PyResult<(DocumentHandle, Py<PyAny>)> {
        let targets = targets_from_py(py, targets)?;
        let doc = Arc::clone(&self.doc);
        let (doc, problems) = guarded(py, move || {
            let mut doc = Document::clone(&doc);
            let problems = apply_locos_targets(&mut doc, targets)?;
            Ok((doc, problems))
        })?;
        Ok((DocumentHandle::new(doc, self.parser.clone()), to_py(py, &problems)?))
    }

    /// `merge(handle.to_dict(), ...)`, without the dict: the same keywords, the same defaults (the profile version
    /// the Document was extracted under, from its parser record).
    #[pyo3(signature = (*, output_encoding=None, stamp_target_always=None, profile=None, format_options=None, profile_version=None))]
    fn merge<'py>(
        &self,
        py: Python<'py>,
        output_encoding: Option<&str>,
        stamp_target_always: Option<bool>,
        profile: Option<String>,
        format_options: Option<&Bound<'py, PyAny>>,
        profile_version: Option<u32>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let opts = merge_options(py, &self.doc.format, self.parser.as_ref(), output_encoding, stamp_target_always, profile, format_options, profile_version)?;
        let doc = Arc::clone(&self.doc);
        let bytes = guarded(py, move || Ok(engine::merge_with(&doc, &opts)?))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// The Document's `format` (`to_dict()["format"]`).
    #[getter]
    fn format(&self) -> &str {
        &self.doc.format
    }

    /// The parser record (`to_dict()["parser"]`), or None.
    #[getter]
    fn parser(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.parser {
            Some(p) => to_py(py, p),
            None => Ok(py.None()),
        }
    }

    /// `to_dict().get("warnings", [])`: the reader's notices.
    #[getter]
    fn warnings(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        pythonize::pythonize_custom::<SortedTypes, _>(py, &self.doc.warnings).map(Bound::unbind).map_err(|e| to_py_err(py, Fail::new("internal", e.to_string())))
    }

    /// `to_dict().get("source_encoding")`: the encoding a text file was decoded from when it was not UTF-8.
    #[getter]
    fn source_encoding(&self) -> Option<&str> {
        self.doc.source_encoding.as_deref()
    }

    /// The number of units (every unit, translatable or not; `len(to_dict()["units"])`).
    fn __len__(&self) -> usize {
        self.doc.units.len()
    }

    /// The `context.extra` of every unit that has one, in file order: a list of `{"unit": unit id, "segments":
    /// [segment ids], "translate": bool, "extra": {name: value}}`. Reads what an adapter looks up per unit (an
    /// image inventory, a dialect name) without the whole Document. With `names`, only those extras are listed,
    /// and a unit with none of them is left out.
    #[pyo3(signature = (names=None))]
    fn unit_extras(&self, py: Python<'_>, names: Option<Vec<String>>) -> PyResult<Py<PyAny>> {
        let out = PyList::empty(py);
        for u in &self.doc.units {
            let extra = PyDict::new(py);
            for (k, v) in &u.context.extra {
                if names.as_ref().is_none_or(|n| n.contains(k)) {
                    extra.set_item(k, v)?;
                }
            }
            if extra.is_empty() {
                continue;
            }
            let d = PyDict::new(py);
            d.set_item("extra", extra)?;
            d.set_item("segments", u.segments.iter().map(|s| s.id.as_str()).collect::<Vec<_>>())?;
            d.set_item("translate", u.translate)?;
            d.set_item("unit", &u.id)?;
            out.append(d)?;
        }
        Ok(out.into_any().unbind())
    }

    fn __repr__(&self) -> String {
        format!("<bilingua.DocumentHandle format={:?} units={}>", self.doc.format, self.doc.units.len())
    }
}
