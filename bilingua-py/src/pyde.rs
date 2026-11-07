//! Reading a Document straight from Python objects, without building a `serde_json::Value` first (T8, BILI-18).
//!
//! The old way was `from_py` (Python objects to a `Value`, checking types and depth) and then
//! `serde_json::from_value`. This deserializer gives the same answer without the copy: it accepts exactly the
//! Python objects `from_py` accepts (same type order, same depth limit, bytes read as their base64 string) and
//! reads them the way `serde_json`'s `Value` deserializer reads the `Value` that `from_py` would have built. A value
//! the target type ignores (an unknown key) is still checked with `from_py` itself.
//!
//! It is only ever a fast path: on any error the caller goes the old way, so the error a caller sees (code and
//! message) is the old one.

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde::de::{self, DeserializeSeed, IntoDeserializer, Visitor};
use std::borrow::Cow;
use std::fmt;

/// Any failure; the caller falls back to the old path, which produces the real error.
#[derive(Debug)]
pub struct DeError(String);

impl fmt::Display for DeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DeError {}

impl de::Error for DeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DeError(msg.to_string())
    }
}

fn fail<T>(m: &str) -> Result<T, DeError> {
    Err(DeError(m.to_string()))
}

/// The `Value` kind `from_py` would turn the object into.
enum Kind<'a> {
    Null,
    Bool(bool),
    /// An integer `from_py` reads as i64 (a negative one stays i64; serde_json's Number keeps the others as u64).
    I64(i64),
    U64(u64),
    F64(f64),
    Str(Cow<'a, str>),
    Dict,
    Seq,
}

/// One Python object at a nesting depth (the top object is depth 0, as in `from_py`).
pub struct De<'py> {
    obj: Bound<'py, PyAny>,
    depth: usize,
}

impl<'py> De<'py> {
    pub fn new(obj: Bound<'py, PyAny>) -> De<'py> {
        De { obj, depth: 0 }
    }

    /// `from_py`'s checks, in its order.
    fn kind(&self) -> Result<Kind<'_>, DeError> {
        let obj = &self.obj;
        if self.depth > 200 {
            return fail("document is nested too deeply");
        }
        if obj.is_none() {
            return Ok(Kind::Null);
        }
        if let Ok(b) = obj.downcast::<PyBool>() {
            return Ok(Kind::Bool(b.is_true()));
        }
        if obj.is_instance_of::<PyInt>() {
            if let Ok(i) = obj.extract::<i64>() {
                return Ok(if i < 0 { Kind::I64(i) } else { Kind::U64(i as u64) });
            }
            if let Ok(u) = obj.extract::<u64>() {
                return Ok(Kind::U64(u));
            }
            return fail("integer out of range");
        }
        if obj.is_instance_of::<PyFloat>() {
            let f: f64 = obj.extract().map_err(|e| DeError(e.to_string()))?;
            return if f.is_finite() { Ok(Kind::F64(f)) } else { fail("NaN or infinity in document") };
        }
        if let Ok(s) = obj.downcast::<PyString>() {
            return Ok(Kind::Str(Cow::Borrowed(s.to_str().map_err(|e| DeError(e.to_string()))?)));
        }
        if obj.is_instance_of::<PyDict>() {
            return Ok(Kind::Dict);
        }
        if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
            return Ok(Kind::Seq);
        }
        if let Ok(b) = obj.downcast::<PyBytes>() {
            return Ok(Kind::Str(Cow::Owned(engine::b64::encode(b.as_bytes()))));
        }
        fail("unsupported type")
    }

    fn child(&self, obj: Bound<'py, PyAny>) -> De<'py> {
        De { obj, depth: self.depth + 1 }
    }

    /// The items of a list or tuple, iterated the way `from_py` iterates them (a plain list or tuple directly:
    /// its iterator is the built-in one; a subclass through `iter()`, which may be overridden).
    fn items(&self) -> Result<Vec<De<'py>>, DeError> {
        if let Ok(l) = self.obj.downcast_exact::<PyList>() {
            return Ok(l.iter().map(|x| self.child(x)).collect());
        }
        if let Ok(t) = self.obj.downcast_exact::<PyTuple>() {
            return Ok(t.iter().map(|x| self.child(x)).collect());
        }
        let mut out = Vec::new();
        for x in self.obj.try_iter().map_err(|e| DeError(e.to_string()))? {
            out.push(self.child(x.map_err(|e| DeError(e.to_string()))?));
        }
        Ok(out)
    }

    /// The entries of a dict; keys must be str (valid UTF-8), as in `from_py`.
    fn entries(&self) -> Result<Vec<(Bound<'py, PyString>, De<'py>)>, DeError> {
        let d = self.obj.downcast::<PyDict>().map_err(|e| DeError(e.to_string()))?;
        let mut out = Vec::with_capacity(d.len());
        for (k, v) in d.iter() {
            let key = k.downcast_into::<PyString>().map_err(|_| DeError("dict keys must be strings".into()))?;
            key.to_str().map_err(|e| DeError(e.to_string()))?;
            out.push((key, self.child(v)));
        }
        Ok(out)
    }

    fn visit_seq<'de, V: Visitor<'de>>(&self, visitor: V) -> Result<V::Value, DeError> {
        let items = self.items()?;
        let len = items.len();
        let mut seq = Seq { iter: items.into_iter() };
        let out = visitor.visit_seq(&mut seq)?;
        if seq.iter.len() == 0 {
            Ok(out)
        } else {
            Err(de::Error::invalid_length(len, &"fewer elements in array"))
        }
    }

    fn visit_map<'de, V: Visitor<'de>>(&self, visitor: V) -> Result<V::Value, DeError> {
        let entries = self.entries()?;
        let len = entries.len();
        let mut map = MapIn { iter: entries.into_iter(), value: None };
        let out = visitor.visit_map(&mut map)?;
        if map.iter.len() == 0 {
            Ok(out)
        } else {
            Err(de::Error::invalid_length(len, &"fewer elements in map"))
        }
    }
}

fn number<'de, V: Visitor<'de>>(k: &Kind<'_>, visitor: V) -> Result<V::Value, DeError> {
    match *k {
        Kind::I64(i) => visitor.visit_i64(i),
        Kind::U64(u) => visitor.visit_u64(u),
        Kind::F64(f) => visitor.visit_f64(f),
        _ => fail("invalid type: expected a number"),
    }
}

macro_rules! numbers {
    ($de:lifetime: $($m:ident)*) => {$(
        fn $m<V: Visitor<$de>>(self, visitor: V) -> Result<V::Value, DeError> {
            number(&self.kind()?, visitor)
        }
    )*};
}

impl<'de, 'py> de::Deserializer<'de> for De<'py> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Null => visitor.visit_unit(),
            Kind::Bool(b) => visitor.visit_bool(b),
            k @ (Kind::I64(_) | Kind::U64(_) | Kind::F64(_)) => number(&k, visitor),
            Kind::Str(s) => visitor.visit_str(&s),
            Kind::Seq => self.visit_seq(visitor),
            Kind::Dict => self.visit_map(visitor),
        }
    }

    numbers!('de: deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64 deserialize_i128
             deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64 deserialize_u128
             deserialize_f32 deserialize_f64);

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        if matches!(self.kind()?, Kind::Null) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        let (variant, value) = match self.kind()? {
            Kind::Dict => {
                let mut entries = self.entries()?;
                if entries.len() != 1 {
                    return fail("map with a single key");
                }
                let (k, v) = entries.pop().expect("one entry");
                (k.to_str().map_err(|e| DeError(e.to_string()))?.to_owned(), Some(v))
            }
            Kind::Str(s) => (s.into_owned(), None),
            _ => return fail("invalid type: expected string or map"),
        };
        visitor.visit_enum(Enum { variant, value })
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Bool(b) => visitor.visit_bool(b),
            _ => fail("invalid type: expected a boolean"),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Str(s) => visitor.visit_str(&s),
            _ => fail("invalid type: expected a string"),
        }
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Str(s) => visitor.visit_str(&s),
            Kind::Seq => self.visit_seq(visitor),
            _ => fail("invalid type: expected a string or an array"),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Null => visitor.visit_unit(),
            _ => fail("invalid type: expected null"),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Seq => self.visit_seq(visitor),
            _ => fail("invalid type: expected an array"),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(self, _name: &'static str, _len: usize, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Dict => self.visit_map(visitor),
            _ => fail("invalid type: expected a map"),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        match self.kind()? {
            Kind::Seq => self.visit_seq(visitor),
            Kind::Dict => self.visit_map(visitor),
            _ => fail("invalid type: expected a map"),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_string(visitor)
    }

    /// The target type skips this value, but `from_py` would still have refused a bad one: check it the same way.
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        crate::from_py(&self.obj, self.depth).map_err(|f| DeError(f.msg))?;
        visitor.visit_unit()
    }
}

struct Seq<'py> {
    iter: std::vec::IntoIter<De<'py>>,
}

impl<'de, 'py> de::SeqAccess<'de> for Seq<'py> {
    type Error = DeError;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, DeError> {
        match self.iter.next() {
            Some(x) => seed.deserialize(x).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct MapIn<'py> {
    iter: std::vec::IntoIter<(Bound<'py, PyString>, De<'py>)>,
    value: Option<De<'py>>,
}

impl<'de, 'py> de::MapAccess<'de> for MapIn<'py> {
    type Error = DeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, DeError> {
        match self.iter.next() {
            Some((k, v)) => {
                self.value = Some(v);
                let s = k.to_str().map_err(|e| DeError(e.to_string()))?;
                let key: de::value::StrDeserializer<'_, DeError> = s.into_deserializer();
                seed.deserialize(key).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, DeError> {
        match self.value.take() {
            Some(v) => seed.deserialize(v),
            None => fail("value is missing"),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct Enum<'py> {
    variant: String,
    value: Option<De<'py>>,
}

impl<'de, 'py> de::EnumAccess<'de> for Enum<'py> {
    type Error = DeError;
    type Variant = Variant<'py>;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Variant<'py>), DeError> {
        let name: de::value::StringDeserializer<DeError> = self.variant.into_deserializer();
        seed.deserialize(name).map(|v| (v, Variant { value: self.value }))
    }
}

struct Variant<'py> {
    value: Option<De<'py>>,
}

impl<'de, 'py> de::VariantAccess<'de> for Variant<'py> {
    type Error = DeError;

    fn unit_variant(self) -> Result<(), DeError> {
        match self.value {
            Some(v) => <() as de::Deserialize>::deserialize(v),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, DeError> {
        match self.value {
            Some(v) => seed.deserialize(v),
            None => fail("invalid type: unit variant, expected newtype variant"),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, DeError> {
        match self.value {
            Some(v) => match v.kind()? {
                Kind::Seq => {
                    if v.obj.len().map_err(|e| DeError(e.to_string()))? == 0 {
                        visitor.visit_unit()
                    } else {
                        v.visit_seq(visitor)
                    }
                }
                _ => fail("invalid type: expected tuple variant"),
            },
            None => fail("invalid type: unit variant, expected tuple variant"),
        }
    }

    fn struct_variant<V: Visitor<'de>>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, DeError> {
        match self.value {
            Some(v) => match v.kind()? {
                Kind::Dict => v.visit_map(visitor),
                _ => fail("invalid type: expected struct variant"),
            },
            None => fail("invalid type: unit variant, expected struct variant"),
        }
    }
}
