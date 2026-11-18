//! What the unit accessors return while the fields are still `BTreeMap`s (unit model step 1 of the 0.10 work).
//!
//! [`Unit::codes`](crate::Unit::codes) returns `&MapShim<CodeData>`, and [`UnitContext::extra`](crate::UnitContext::extra)
//! and [`Unit::hidden_map`](crate::Unit::hidden_map) return `&TextShim`. Each is a view of the current field with exactly
//! the methods, argument types and item types of the type that will replace the field: [`MapShim<V>`] matches
//! [`SmallMap<V>`] (so `Codes`), and [`TextShim`] matches [`TextMap`]. When the fields become `Codes` and
//! `TextMap`, the accessors return `&Codes` and `&TextMap` and code written against the accessors compiles
//! unchanged. So:
//!
//! - never name `MapShim` or `TextShim` (or their iterator types) in a crate: call methods on what the accessor
//!   returns, and build owned maps as `Codes` and `TextMap`;
//! - for an owned copy, write `Codes::from(unit.codes())` or `TextMap::from(ctx.extra())`, not `.clone()`;
//! - to replace a whole map, call `set_codes` / `set_extra` / `set_hidden_map` with anything `Into<Codes>` or
//!   `Into<TextMap>` (a `BTreeMap` still works).
//!
//! The views are `#[repr(transparent)]` wrappers of the field; they go away when the storage changes.

use crate::smallmap::{SmallMap, TextMap};
use serde::ser::{Serialize, Serializer};
use std::collections::{btree_map, BTreeMap};
use std::fmt;
use std::iter::Map;
use std::ops::Index;

/// A view of a `BTreeMap<String, V>` field with the API of [`SmallMap<V>`]. See the [module docs](self).
#[repr(transparent)]
#[derive(PartialEq, Eq)]
pub struct MapShim<V>(BTreeMap<String, V>);

/// A view of a `BTreeMap<String, String>` field with the API of [`TextMap`]. See the [module docs](self).
#[repr(transparent)]
#[derive(PartialEq, Eq)]
pub struct TextShim(BTreeMap<String, String>);

/// Iterator over `(&str, &V)`, in key order.
pub type Iter<'a, V> = Map<btree_map::Iter<'a, String, V>, for<'x> fn((&'x String, &'x V)) -> (&'x str, &'x V)>;
/// Iterator over `(&str, &mut V)`, in key order.
pub type IterMut<'a, V> = Map<btree_map::IterMut<'a, String, V>, for<'x> fn((&'x String, &'x mut V)) -> (&'x str, &'x mut V)>;
/// Iterator over the keys, in order.
pub type Keys<'a, V> = Map<btree_map::Keys<'a, String, V>, for<'x> fn(&'x String) -> &'x str>;
/// Iterator over `(&str, &str)`, in key order.
pub type TextIter<'a> = Map<btree_map::Iter<'a, String, String>, for<'x> fn((&'x String, &'x String)) -> (&'x str, &'x str)>;
/// Iterator over the values as `&str`, in key order.
pub type TextValues<'a> = Map<btree_map::Values<'a, String, String>, for<'x> fn(&'x String) -> &'x str>;

#[allow(clippy::ptr_arg)] // the signature the Iter adapter needs
fn pair<'x, V>((k, v): (&'x String, &'x V)) -> (&'x str, &'x V) {
    (k, v)
}
#[allow(clippy::ptr_arg)]
fn pair_mut<'x, V>((k, v): (&'x String, &'x mut V)) -> (&'x str, &'x mut V) {
    (k, v)
}
#[allow(clippy::ptr_arg)]
fn text_pair<'x>((k, v): (&'x String, &'x String)) -> (&'x str, &'x str) {
    (k, v)
}
#[allow(clippy::ptr_arg)]
fn as_str(s: &String) -> &str {
    s
}

impl<V> MapShim<V> {
    #[allow(unsafe_code)] // removed in unit model step 3, when the fields become Codes and TextMap
    pub(crate) fn wrap(m: &BTreeMap<String, V>) -> &MapShim<V> {
        // SAFETY: MapShim<V> is #[repr(transparent)] over BTreeMap<String, V>, so the two have the same layout;
        // the reference keeps its lifetime, and nothing else can reach the map while it is borrowed.
        unsafe { &*(m as *const BTreeMap<String, V> as *const MapShim<V>) }
    }
    #[allow(unsafe_code)] // removed in unit model step 3, when the fields become Codes and TextMap
    pub(crate) fn wrap_mut(m: &mut BTreeMap<String, V>) -> &mut MapShim<V> {
        // SAFETY: as in `wrap`; the exclusive borrow is carried over unchanged.
        unsafe { &mut *(m as *mut BTreeMap<String, V> as *mut MapShim<V>) }
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// True when the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// The value for `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.get(key)
    }
    /// The value for `key`, mutable.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.0.get_mut(key)
    }
    /// The stored key and the value for `key`.
    pub fn get_key_value(&self, key: &str) -> Option<(&str, &V)> {
        self.0.get_key_value(key).map(pair)
    }
    /// True when `key` has a value.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
    /// Sets the value for `key` and returns the one it replaces.
    pub fn insert<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, value: V) -> Option<V> {
        if let Some(slot) = self.0.get_mut(key.as_ref()) {
            return Some(std::mem::replace(slot, value));
        }
        self.0.insert(String::from(key.into()), value)
    }
    /// Removes `key` and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        self.0.remove(key)
    }
    /// Removes every entry.
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Keeps only the entries for which `keep` returns true.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut V) -> bool) {
        self.0.retain(|k, v| keep(k, v));
    }
    /// The value for `key`, inserting `make()` first when there is none.
    pub fn get_or_insert_with<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, make: impl FnOnce() -> V) -> &mut V {
        self.0.entry(String::from(key.into())).or_insert_with(make)
    }
    /// The entries as `(&str, &V)`, in key order.
    pub fn iter(&self) -> Iter<'_, V> {
        self.0.iter().map(pair::<V> as _)
    }
    /// The entries as `(&str, &mut V)`, in key order.
    pub fn iter_mut(&mut self) -> IterMut<'_, V> {
        self.0.iter_mut().map(pair_mut::<V> as _)
    }
    /// The keys, in order.
    pub fn keys(&self) -> Keys<'_, V> {
        self.0.keys().map(as_str as _)
    }
    /// The values, in key order.
    pub fn values(&self) -> btree_map::Values<'_, String, V> {
        self.0.values()
    }
    /// The values, mutable, in key order.
    pub fn values_mut(&mut self) -> btree_map::ValuesMut<'_, String, V> {
        self.0.values_mut()
    }
}

impl<V: fmt::Debug> fmt::Debug for MapShim<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<V, Q: AsRef<str> + ?Sized> Index<&Q> for MapShim<V> {
    type Output = V;
    /// The value for `key`. Panics when there is none.
    fn index(&self, key: &Q) -> &V {
        self.get(key.as_ref()).expect("no entry found for key")
    }
}

impl<'a, V> IntoIterator for &'a MapShim<V> {
    type Item = (&'a str, &'a V);
    type IntoIter = Iter<'a, V>;
    fn into_iter(self) -> Iter<'a, V> {
        self.iter()
    }
}

impl<'a, V> IntoIterator for &'a mut MapShim<V> {
    type Item = (&'a str, &'a mut V);
    type IntoIter = IterMut<'a, V>;
    fn into_iter(self) -> IterMut<'a, V> {
        self.iter_mut()
    }
}

impl<K: Into<Box<str>>, V> Extend<(K, V)> for MapShim<V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        self.0.extend(iter.into_iter().map(|(k, v)| (String::from(k.into()), v)));
    }
}

impl<V: PartialEq> PartialEq<SmallMap<V>> for MapShim<V> {
    fn eq(&self, other: &SmallMap<V>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<V: PartialEq> PartialEq<MapShim<V>> for SmallMap<V> {
    fn eq(&self, other: &MapShim<V>) -> bool {
        other == self
    }
}

impl<V: Serialize> Serialize for MapShim<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<V: Clone> From<&MapShim<V>> for SmallMap<V> {
    /// An owned copy: `Codes::from(unit.codes())`.
    fn from(m: &MapShim<V>) -> SmallMap<V> {
        SmallMap::from(m.0.clone())
    }
}

impl TextShim {
    #[allow(unsafe_code)] // removed in unit model step 3, when the fields become Codes and TextMap
    pub(crate) fn wrap(m: &BTreeMap<String, String>) -> &TextShim {
        // SAFETY: TextShim is #[repr(transparent)] over BTreeMap<String, String>, so the two have the same layout;
        // the reference keeps its lifetime.
        unsafe { &*(m as *const BTreeMap<String, String> as *const TextShim) }
    }
    #[allow(unsafe_code)] // removed in unit model step 3, when the fields become Codes and TextMap
    pub(crate) fn wrap_mut(m: &mut BTreeMap<String, String>) -> &mut TextShim {
        // SAFETY: as in `wrap`; the exclusive borrow is carried over unchanged.
        unsafe { &mut *(m as *mut BTreeMap<String, String> as *mut TextShim) }
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// True when the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// The text for `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
    /// True when `key` has a value.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
    /// Sets the text for `key` and returns the text it replaces.
    pub fn insert<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, value: impl Into<Box<str>>) -> Option<Box<str>> {
        let value = String::from(value.into());
        if let Some(slot) = self.0.get_mut(key.as_ref()) {
            return Some(std::mem::replace(slot, value).into_boxed_str());
        }
        self.0.insert(String::from(key.into()), value).map(String::into_boxed_str)
    }
    /// Removes `key` and returns its text.
    pub fn remove(&mut self, key: &str) -> Option<Box<str>> {
        self.0.remove(key).map(String::into_boxed_str)
    }
    /// Removes every entry.
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Keeps only the entries for which `keep` returns true.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &str) -> bool) {
        self.0.retain(|k, v| keep(k, v));
    }
    /// The text for `key`, inserting `make()` first when there is none.
    pub fn get_or_insert_with<K: AsRef<str> + Into<Box<str>>, T: Into<Box<str>>>(&mut self, key: K, make: impl FnOnce() -> T) -> &str {
        self.0.entry(String::from(key.into())).or_insert_with(|| String::from(make().into()))
    }
    /// The entries as `(&str, &str)`, in key order.
    pub fn iter(&self) -> TextIter<'_> {
        self.0.iter().map(text_pair as _)
    }
    /// The keys, in order.
    pub fn keys(&self) -> Keys<'_, String> {
        self.0.keys().map(as_str as _)
    }
    /// The values, in key order.
    pub fn values(&self) -> TextValues<'_> {
        self.0.values().map(as_str as _)
    }
}

impl fmt::Debug for TextShim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<Q: AsRef<str> + ?Sized> Index<&Q> for TextShim {
    type Output = str;
    /// The text for `key`. Panics when there is none.
    fn index(&self, key: &Q) -> &str {
        self.get(key.as_ref()).expect("no entry found for key")
    }
}

impl<'a> IntoIterator for &'a TextShim {
    type Item = (&'a str, &'a str);
    type IntoIter = TextIter<'a>;
    fn into_iter(self) -> TextIter<'a> {
        self.iter()
    }
}

impl<K: Into<Box<str>>, T: Into<Box<str>>> Extend<(K, T)> for TextShim {
    fn extend<I: IntoIterator<Item = (K, T)>>(&mut self, iter: I) {
        self.0.extend(iter.into_iter().map(|(k, v)| (String::from(k.into()), String::from(v.into()))));
    }
}

impl PartialEq<TextMap> for TextShim {
    fn eq(&self, other: &TextMap) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl PartialEq<TextShim> for TextMap {
    fn eq(&self, other: &TextShim) -> bool {
        other == self
    }
}

impl Serialize for TextShim {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl From<&TextShim> for TextMap {
    /// An owned copy: `TextMap::from(ctx.extra())`.
    fn from(m: &TextShim) -> TextMap {
        TextMap::from(m.0.clone())
    }
}
