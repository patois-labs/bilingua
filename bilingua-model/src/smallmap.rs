//! Compact maps for the per-unit data: [`SmallMap`] (a sorted boxed slice) and [`TextMap`] (text to text).
//!
//! A `BTreeMap` allocates a whole node (room for 11 entries) on its first insert, about 1.3 KB for a unit's
//! inline codes even when it holds two of them. A unit holds a few entries, so a sorted boxed slice with
//! binary-search lookup holds the same data in a third of the memory and is just as fast to read.
//!
//! Both behave like `BTreeMap<String, _>` wherever a caller can tell: keys iterate in `str` order (which is
//! `String` order), a second insert of a key replaces the value, JSON is an object with its keys in that order,
//! and reading a JSON object with a repeated key keeps the last value. Two things differ: iteration yields
//! `(&str, &V)`, and the methods take `&str` keys.
//!
//! Building: an insert whose key sorts after every key already there appends; any other insert shifts the tail.
//! `collect`, `extend`, `From<BTreeMap>` and `From<[(K, V); N]>` sort once, so a map with many entries is
//! never built in quadratic time that way. Every insert or remove reallocates the slice to its exact length (a
//! boxed slice has no spare capacity): build a large map in bulk rather than one insert at a time.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::iter::Map;
use std::marker::PhantomData;
use std::ops::Index;

/// A map from text keys to `V`, kept as one boxed slice sorted by key. See the [module docs](self).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SmallMap<V> {
    entries: Box<[(Box<str>, V)]>,
}

/// Iterator over `(&str, &V)`, in key order.
pub type Iter<'a, V> = Map<std::slice::Iter<'a, (Box<str>, V)>, for<'x> fn(&'x (Box<str>, V)) -> (&'x str, &'x V)>;
/// Iterator over `(&str, &mut V)`, in key order.
pub type IterMut<'a, V> = Map<std::slice::IterMut<'a, (Box<str>, V)>, for<'x> fn(&'x mut (Box<str>, V)) -> (&'x str, &'x mut V)>;
/// Iterator over the keys, in order.
pub type Keys<'a, V> = Map<std::slice::Iter<'a, (Box<str>, V)>, for<'x> fn(&'x (Box<str>, V)) -> &'x str>;
/// Iterator over the values, in key order.
pub type Values<'a, V> = Map<std::slice::Iter<'a, (Box<str>, V)>, for<'x> fn(&'x (Box<str>, V)) -> &'x V>;
/// Iterator over the values, mutable, in key order.
pub type ValuesMut<'a, V> = Map<std::slice::IterMut<'a, (Box<str>, V)>, for<'x> fn(&'x mut (Box<str>, V)) -> &'x mut V>;
/// Owning iterator over `(Box<str>, V)`, in key order.
pub type IntoIter<V> = std::vec::IntoIter<(Box<str>, V)>;

fn entry_ref<V>(e: &(Box<str>, V)) -> (&str, &V) {
    (&e.0, &e.1)
}
fn entry_mut<V>(e: &mut (Box<str>, V)) -> (&str, &mut V) {
    (&e.0, &mut e.1)
}
fn entry_key<V>(e: &(Box<str>, V)) -> &str {
    &e.0
}
fn entry_value<V>(e: &(Box<str>, V)) -> &V {
    &e.1
}
fn entry_value_mut<V>(e: &mut (Box<str>, V)) -> &mut V {
    &mut e.1
}

/// Sorts by key (stable) and keeps the last value of each repeated key, as a run of `BTreeMap::insert` would.
fn sorted_last_wins<V>(mut v: Vec<(Box<str>, V)>) -> Vec<(Box<str>, V)> {
    if v.windows(2).all(|w| w[0].0 < w[1].0) {
        return v;
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    // `dedup_by` hands (later, kept): move the later value into the kept slot, then drop the later entry
    v.dedup_by(|later, kept| {
        if later.0 == kept.0 {
            std::mem::swap(&mut later.1, &mut kept.1);
            true
        } else {
            false
        }
    });
    v
}

impl<V> Default for SmallMap<V> {
    fn default() -> SmallMap<V> {
        SmallMap { entries: Box::default() }
    }
}

impl<V> SmallMap<V> {
    /// An empty map (allocates nothing).
    pub fn new() -> SmallMap<V> {
        SmallMap::default()
    }

    fn from_sorted_vec(v: Vec<(Box<str>, V)>) -> SmallMap<V> {
        SmallMap { entries: v.into_boxed_slice() }
    }

    fn find(&self, key: &str) -> Result<usize, usize> {
        self.entries.binary_search_by(|(k, _)| (**k).cmp(key))
    }

    /// Edits the entries as a `Vec` and stores them back at their exact length.
    fn edit<R>(&mut self, f: impl FnOnce(&mut Vec<(Box<str>, V)>) -> R) -> R {
        let mut v = std::mem::take(&mut self.entries).into_vec();
        let r = f(&mut v);
        self.entries = v.into_boxed_slice();
        r
    }

    fn insert_at(&mut self, i: usize, key: Box<str>, value: V) {
        self.edit(|v| {
            v.reserve_exact(1);
            v.insert(i, (key, value));
        });
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value for `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.find(key).ok().map(|i| &self.entries[i].1)
    }

    /// The value for `key`, mutable.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.find(key).ok().map(|i| &mut self.entries[i].1)
    }

    /// The stored key and the value for `key`.
    pub fn get_key_value(&self, key: &str) -> Option<(&str, &V)> {
        self.find(key).ok().map(|i| entry_ref(&self.entries[i]))
    }

    /// True when `key` has a value.
    pub fn contains_key(&self, key: &str) -> bool {
        self.find(key).is_ok()
    }

    /// Sets the value for `key` and returns the one it replaces. Appends when `key` sorts after every key there.
    pub fn insert<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, value: V) -> Option<V> {
        let at = match self.entries.last() {
            Some((last, _)) if **last >= *key.as_ref() => self.find(key.as_ref()),
            _ => Err(self.entries.len()),
        };
        match at {
            Ok(i) => Some(std::mem::replace(&mut self.entries[i].1, value)),
            Err(i) => {
                self.insert_at(i, key.into(), value);
                None
            }
        }
    }

    /// Removes `key` and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let i = self.find(key).ok()?;
        Some(self.edit(|v| v.remove(i)).1)
    }

    /// Removes every entry (and frees the slice).
    pub fn clear(&mut self) {
        self.entries = Box::default();
    }

    /// Keeps only the entries for which `keep` returns true.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut V) -> bool) {
        self.edit(|v| v.retain_mut(|(k, val)| keep(k, val)));
    }

    /// The value for `key`, inserting `make()` first when there is none (what `entry(key).or_insert_with(make)`
    /// does on a `BTreeMap`). The key is only converted when it is inserted.
    pub fn get_or_insert_with<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, make: impl FnOnce() -> V) -> &mut V {
        let i = match self.find(key.as_ref()) {
            Ok(i) => i,
            Err(i) => {
                self.insert_at(i, key.into(), make());
                i
            }
        };
        &mut self.entries[i].1
    }

    /// The entries as `(&str, &V)`, in key order.
    pub fn iter(&self) -> Iter<'_, V> {
        self.entries.iter().map(entry_ref::<V> as _)
    }

    /// The entries as `(&str, &mut V)`, in key order.
    pub fn iter_mut(&mut self) -> IterMut<'_, V> {
        self.entries.iter_mut().map(entry_mut::<V> as _)
    }

    /// The keys, in order.
    pub fn keys(&self) -> Keys<'_, V> {
        self.entries.iter().map(entry_key::<V> as _)
    }

    /// The values, in key order.
    pub fn values(&self) -> Values<'_, V> {
        self.entries.iter().map(entry_value::<V> as _)
    }

    /// The values, mutable, in key order.
    pub fn values_mut(&mut self) -> ValuesMut<'_, V> {
        self.entries.iter_mut().map(entry_value_mut::<V> as _)
    }
}

impl<V: fmt::Debug> fmt::Debug for SmallMap<V> {
    /// Prints like a `BTreeMap`: `{"k": v, ...}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<V, Q: AsRef<str> + ?Sized> Index<&Q> for SmallMap<V> {
    type Output = V;
    /// The value for `key`. Panics when there is none, as a `BTreeMap` does.
    fn index(&self, key: &Q) -> &V {
        self.get(key.as_ref()).expect("no entry found for key")
    }
}

impl<'a, V> IntoIterator for &'a SmallMap<V> {
    type Item = (&'a str, &'a V);
    type IntoIter = Iter<'a, V>;
    fn into_iter(self) -> Iter<'a, V> {
        self.iter()
    }
}

impl<'a, V> IntoIterator for &'a mut SmallMap<V> {
    type Item = (&'a str, &'a mut V);
    type IntoIter = IterMut<'a, V>;
    fn into_iter(self) -> IterMut<'a, V> {
        self.iter_mut()
    }
}

impl<V> IntoIterator for SmallMap<V> {
    type Item = (Box<str>, V);
    type IntoIter = IntoIter<V>;
    fn into_iter(self) -> IntoIter<V> {
        self.entries.into_vec().into_iter()
    }
}

impl<K: Into<Box<str>>, V> FromIterator<(K, V)> for SmallMap<V> {
    /// Sorts once; for a repeated key the last value wins, as when collecting into a `BTreeMap`.
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> SmallMap<V> {
        let v: Vec<(Box<str>, V)> = iter.into_iter().map(|(k, v)| (k.into(), v)).collect();
        SmallMap::from_sorted_vec(sorted_last_wins(v))
    }
}

impl<K: Into<Box<str>>, V> Extend<(K, V)> for SmallMap<V> {
    /// Adds the entries in one merge (a new value replaces an old one for the same key; the last of repeated new
    /// keys wins).
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        let new = sorted_last_wins(iter.into_iter().map(|(k, v)| (k.into(), v)).collect::<Vec<_>>());
        if new.is_empty() {
            return;
        }
        let old = std::mem::take(&mut self.entries).into_vec();
        let mut out = Vec::with_capacity(old.len() + new.len());
        let (mut a, mut b) = (old.into_iter().peekable(), new.into_iter().peekable());
        loop {
            let take_old = match (a.peek(), b.peek()) {
                (Some(x), Some(y)) => match x.0.cmp(&y.0) {
                    std::cmp::Ordering::Less => true,
                    std::cmp::Ordering::Equal => {
                        a.next();
                        false
                    }
                    std::cmp::Ordering::Greater => false,
                },
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            out.extend(if take_old { a.next() } else { b.next() });
        }
        self.entries = out.into_boxed_slice();
    }
}

impl<V> From<BTreeMap<String, V>> for SmallMap<V> {
    /// Already in order: no sort.
    fn from(m: BTreeMap<String, V>) -> SmallMap<V> {
        SmallMap::from_sorted_vec(m.into_iter().map(|(k, v)| (k.into_boxed_str(), v)).collect())
    }
}

impl<V> From<SmallMap<V>> for BTreeMap<String, V> {
    fn from(m: SmallMap<V>) -> BTreeMap<String, V> {
        m.into_iter().map(|(k, v)| (k.into_string(), v)).collect()
    }
}

impl<V: Clone> From<&SmallMap<V>> for SmallMap<V> {
    /// A copy. `Codes::from(unit.codes())` takes an owned copy of a unit's codes, whatever `codes()` returns.
    fn from(m: &SmallMap<V>) -> SmallMap<V> {
        m.clone()
    }
}

impl<K: Into<Box<str>>, V, const N: usize> From<[(K, V); N]> for SmallMap<V> {
    fn from(a: [(K, V); N]) -> SmallMap<V> {
        a.into_iter().collect()
    }
}

impl<V: Serialize> Serialize for SmallMap<V> {
    /// A JSON object, keys in order: the bytes a `BTreeMap<String, V>` writes.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(self.iter())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for SmallMap<V> {
    /// From an object; for a repeated key the last value wins, as `serde_json` does into a `BTreeMap`.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<SmallMap<V>, D::Error> {
        struct V0<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for V0<V> {
            type Value = SmallMap<V>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<SmallMap<V>, A::Error> {
                // the hint comes from the input: cap it so a hostile length cannot reserve much up front
                let mut v = Vec::with_capacity(m.size_hint().unwrap_or(0).min(64));
                while let Some((k, val)) = m.next_entry::<Box<str>, V>()? {
                    v.push((k, val));
                }
                Ok(SmallMap::from_sorted_vec(sorted_last_wins(v)))
            }
        }
        d.deserialize_map(V0(PhantomData))
    }
}

/// A map from text to text (a unit's hidden values, a context's extras), stored as a [`SmallMap`] of `Box<str>`.
/// It reads and writes `&str`: [`TextMap::get`] gives `Option<&str>` and iteration gives `(&str, &str)`, so
/// callers never see how the text is stored. JSON is an object of strings, as a `BTreeMap<String, String>`.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextMap(SmallMap<Box<str>>);

/// Iterator over a [`TextMap`]'s `(&str, &str)`, in key order.
pub type TextIter<'a> = Map<Iter<'a, Box<str>>, for<'x> fn((&'x str, &'x Box<str>)) -> (&'x str, &'x str)>;
/// Iterator over a [`TextMap`]'s values as `&str`, in key order.
pub type TextValues<'a> = Map<Values<'a, Box<str>>, for<'x> fn(&'x Box<str>) -> &'x str>;
/// Owning iterator over a [`TextMap`]'s `(Box<str>, Box<str>)`, in key order.
pub type TextIntoIter = IntoIter<Box<str>>;

#[allow(clippy::borrowed_box)] // the signature the TextIter adapter needs
fn text_pair<'x>((k, v): (&'x str, &'x Box<str>)) -> (&'x str, &'x str) {
    (k, v)
}
#[allow(clippy::borrowed_box)]
fn text_value(v: &Box<str>) -> &str {
    v
}

impl TextMap {
    /// An empty map (allocates nothing).
    pub fn new() -> TextMap {
        TextMap::default()
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
        self.0.get(key).map(|v| &**v)
    }
    /// True when `key` has a value.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
    /// Sets the text for `key` and returns the text it replaces.
    pub fn insert<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, value: impl Into<Box<str>>) -> Option<Box<str>> {
        self.0.insert(key, value.into())
    }
    /// Removes `key` and returns its text.
    pub fn remove(&mut self, key: &str) -> Option<Box<str>> {
        self.0.remove(key)
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
        self.0.get_or_insert_with(key, || make().into())
    }
    /// The entries as `(&str, &str)`, in key order.
    pub fn iter(&self) -> TextIter<'_> {
        self.0.iter().map(text_pair as _)
    }
    /// The keys, in order.
    pub fn keys(&self) -> Keys<'_, Box<str>> {
        self.0.keys()
    }
    /// The values, in key order.
    pub fn values(&self) -> TextValues<'_> {
        self.0.values().map(text_value as _)
    }
}

impl fmt::Debug for TextMap {
    /// Prints like a `BTreeMap`: `{"k": "v", ...}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<Q: AsRef<str> + ?Sized> Index<&Q> for TextMap {
    type Output = str;
    /// The text for `key`. Panics when there is none, as a `BTreeMap` does.
    fn index(&self, key: &Q) -> &str {
        self.get(key.as_ref()).expect("no entry found for key")
    }
}

impl<'a> IntoIterator for &'a TextMap {
    type Item = (&'a str, &'a str);
    type IntoIter = TextIter<'a>;
    fn into_iter(self) -> TextIter<'a> {
        self.iter()
    }
}

impl IntoIterator for TextMap {
    type Item = (Box<str>, Box<str>);
    type IntoIter = TextIntoIter;
    fn into_iter(self) -> TextIntoIter {
        self.0.into_iter()
    }
}

impl<K: Into<Box<str>>, T: Into<Box<str>>> FromIterator<(K, T)> for TextMap {
    /// Sorts once; for a repeated key the last value wins.
    fn from_iter<I: IntoIterator<Item = (K, T)>>(iter: I) -> TextMap {
        TextMap(iter.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

impl<K: Into<Box<str>>, T: Into<Box<str>>> Extend<(K, T)> for TextMap {
    /// Adds the entries in one merge; a new value replaces an old one for the same key.
    fn extend<I: IntoIterator<Item = (K, T)>>(&mut self, iter: I) {
        self.0.extend(iter.into_iter().map(|(k, v)| (k, v.into())));
    }
}

impl From<BTreeMap<String, String>> for TextMap {
    /// Already in order: no sort.
    fn from(m: BTreeMap<String, String>) -> TextMap {
        TextMap(SmallMap::from_sorted_vec(m.into_iter().map(|(k, v)| (k.into_boxed_str(), v.into_boxed_str())).collect()))
    }
}

impl From<TextMap> for BTreeMap<String, String> {
    fn from(m: TextMap) -> BTreeMap<String, String> {
        m.into_iter().map(|(k, v)| (k.into_string(), v.into_string())).collect()
    }
}

impl From<&TextMap> for TextMap {
    /// A copy. `TextMap::from(ctx.extra())` takes an owned copy, whatever `extra()` returns.
    fn from(m: &TextMap) -> TextMap {
        m.clone()
    }
}

impl<K: Into<Box<str>>, T: Into<Box<str>>, const N: usize> From<[(K, T); N]> for TextMap {
    fn from(a: [(K, T); N]) -> TextMap {
        a.into_iter().collect()
    }
}

impl Serialize for TextMap {
    /// A JSON object of strings, keys in order.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for TextMap {
    /// From an object of strings; for a repeated key the last value wins.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<TextMap, D::Error> {
        SmallMap::deserialize(d).map(TextMap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FusedIterator;

    fn keys<V>(m: &SmallMap<V>) -> Vec<&str> {
        m.keys().collect()
    }

    #[test]
    fn new_and_default_allocate_nothing_and_are_empty() {
        let m: SmallMap<u8> = SmallMap::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert_eq!(m, SmallMap::default());
        assert_eq!(std::mem::size_of::<SmallMap<u8>>(), 16);
        assert_eq!(serde_json::to_string(&m).unwrap(), "{}");
    }

    #[test]
    fn insert_keeps_order_and_replaces() {
        let mut m = SmallMap::new();
        assert_eq!(m.insert("b", 2), None);
        assert_eq!(m.insert("d", 4), None); // appends
        assert_eq!(m.insert("a", 1), None); // front
        assert_eq!(m.insert(String::from("c"), 3), None); // middle
        assert_eq!(m.insert("b", 20), Some(2)); // replace
        assert_eq!(keys(&m), ["a", "b", "c", "d"]);
        assert_eq!(m.get("b"), Some(&20));
        assert_eq!(m["d"], 4);
        assert_eq!(m.len(), 4);
        assert!(m.contains_key("c") && !m.contains_key("e"));
        assert_eq!(m.get_key_value("c"), Some(("c", &3)));
    }

    #[test]
    fn index_takes_str_string_and_box() {
        let m = SmallMap::from([("k", 7)]);
        let s = String::from("k");
        let b: Box<str> = "k".into();
        assert_eq!((m["k"], m[&s], m[&*s], m[&b]), (7, 7, 7, 7));
    }

    #[test]
    #[should_panic(expected = "no entry found for key")]
    fn index_panics_on_a_missing_key() {
        let m: SmallMap<u8> = SmallMap::new();
        let _ = m["x"];
    }

    #[test]
    fn remove_clear_retain() {
        let mut m: SmallMap<i32> = (0..10).map(|i| (format!("k{i}"), i)).collect();
        assert_eq!(m.remove("k3"), Some(3));
        assert_eq!(m.remove("k3"), None);
        m.retain(|k, v| {
            *v += 1;
            k != "k5"
        });
        assert_eq!(m.values().copied().collect::<Vec<_>>(), [1, 2, 3, 5, 7, 8, 9, 10]);
        m.clear();
        assert!(m.is_empty());
    }

    #[test]
    fn get_or_insert_with_inserts_once() {
        let mut m: SmallMap<Vec<u8>> = SmallMap::new();
        m.get_or_insert_with("x", Vec::new).push(1);
        m.get_or_insert_with(String::from("x"), || panic!("present")).push(2);
        m.get_or_insert_with("a", Vec::new).push(9);
        assert_eq!(m["x"], [1, 2]);
        assert_eq!(keys(&m), ["a", "x"]);
    }

    #[test]
    fn mutable_access() {
        let mut m = SmallMap::from([("a", 1), ("b", 2)]);
        *m.get_mut("a").unwrap() += 10;
        for (_, v) in m.iter_mut() {
            *v *= 2;
        }
        for v in m.values_mut() {
            *v += 1;
        }
        for (k, v) in &mut m {
            if k == "b" {
                *v = 0;
            }
        }
        assert_eq!(m.iter().collect::<Vec<_>>(), [("a", &23), ("b", &0)]);
        assert!(m.get_mut("z").is_none());
    }

    #[test]
    fn collect_and_from_sort_and_last_wins() {
        let m: SmallMap<u8> = vec![("b", 1), ("a", 2), ("b", 3), ("c", 4), ("a", 5)].into_iter().collect();
        assert_eq!(m.iter().collect::<Vec<_>>(), [("a", &5), ("b", &3), ("c", &4)]);
        let bt: BTreeMap<String, u8> = vec![("b", 1), ("a", 2), ("b", 3), ("c", 4), ("a", 5)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(SmallMap::from(bt.clone()), m);
        assert_eq!(BTreeMap::from(m.clone()), bt);
        assert_eq!(SmallMap::from(&m), m);
    }

    #[test]
    fn extend_merges_and_new_wins() {
        let mut m = SmallMap::from([("b", 1), ("d", 2)]);
        m.extend([("a", 10), ("d", 20), ("e", 30), ("a", 11)]);
        assert_eq!(m.iter().collect::<Vec<_>>(), [("a", &11), ("b", &1), ("d", &20), ("e", &30)]);
        m.extend(Vec::<(String, i32)>::new());
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn owned_iteration_and_into_iter_ref() {
        let m = SmallMap::from([("b", 2), ("a", 1)]);
        let mut seen = vec![];
        for (k, v) in &m {
            seen.push((k.to_string(), *v));
        }
        assert_eq!(seen, [("a".to_string(), 1), ("b".to_string(), 2)]);
        let owned: Vec<(Box<str>, i32)> = m.into_iter().rev().collect();
        assert_eq!(&*owned[0].0, "b");
    }

    #[test]
    fn iterators_are_exact_and_double_ended() {
        let m = SmallMap::from([("a", 1), ("b", 2), ("c", 3)]);
        assert_eq!(m.iter().len(), 3);
        assert_eq!(m.keys().rev().collect::<Vec<_>>(), ["c", "b", "a"]);
        assert_eq!(m.values().next_back(), Some(&3));
        fn fused<I: FusedIterator>(_: I) {}
        fused(m.iter());
    }

    #[test]
    fn debug_matches_btreemap() {
        let m = SmallMap::from([("b", "x"), ("a", "y")]);
        let bt: BTreeMap<String, &str> = [("b".to_string(), "x"), ("a".to_string(), "y")].into();
        assert_eq!(format!("{m:?}"), format!("{bt:?}"));
        let t = TextMap::from([("b", "x"), ("a", "y")]);
        assert_eq!(format!("{t:?}"), format!("{bt:?}"));
    }

    #[test]
    fn serde_is_a_sorted_object_and_last_wins() {
        let m = SmallMap::from([("z", 1), ("a", 2)]);
        assert_eq!(serde_json::to_string(&m).unwrap(), r#"{"a":2,"z":1}"#);
        let back: SmallMap<i32> = serde_json::from_str(r#"{"z":1,"a":2,"z":3}"#).unwrap();
        assert_eq!(back.iter().collect::<Vec<_>>(), [("a", &2), ("z", &3)]);
        let bt: BTreeMap<String, i32> = serde_json::from_str(r#"{"z":1,"a":2,"z":3}"#).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), serde_json::to_string(&bt).unwrap());
        // through a Value too (the path a dict from Python takes is the same visitor)
        let v = serde_json::json!({"b": 1, "a": 2});
        let from_value: SmallMap<i32> = serde_json::from_value(v).unwrap();
        assert_eq!(keys(&from_value), ["a", "b"]);
        assert!(serde_json::from_str::<SmallMap<i32>>("[1]").is_err());
        assert!(serde_json::from_str::<SmallMap<i32>>(r#"{"a":"x"}"#).is_err());
    }

    #[test]
    fn keys_sort_in_str_order_like_string() {
        // byte order, not char or locale order: uppercase before lowercase, `~` and `_` where ASCII puts them,
        // multi-byte after ASCII
        let ks = ["b", "B", "_o", "~x", "$raw", "a1", "a10", "a2", "é", "z", "", "\u{10000}"];
        let m: SmallMap<u8> = ks.iter().map(|k| (*k, 0)).collect();
        let bt: BTreeMap<String, u8> = ks.iter().map(|k| (k.to_string(), 0)).collect();
        assert_eq!(m.keys().collect::<Vec<_>>(), bt.keys().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(serde_json::to_string(&m).unwrap(), serde_json::to_string(&bt).unwrap());
    }

    #[test]
    fn insert_in_order_appends_without_search() {
        let mut m = SmallMap::new();
        for i in 0..2000 {
            assert_eq!(m.insert(format!("k{i:05}"), i), None);
        }
        assert_eq!(m.len(), 2000);
        assert!(m.keys().zip(m.keys().skip(1)).all(|(a, b)| a < b));
        for i in (0..2000).rev() {
            assert_eq!(m.insert(format!("k{i:05}"), i + 1), Some(i));
        }
    }

    #[test]
    fn bulk_build_of_many_entries() {
        // reverse order: one sort, not 50,000 shifts
        let m: SmallMap<u32> = (0..50_000u32).rev().map(|i| (format!("{i:06}"), i)).collect();
        assert_eq!(m.len(), 50_000);
        assert_eq!(m["000123"], 123);
        let mut e = SmallMap::from([("000001", 0u32)]);
        e.extend((0..50_000u32).rev().map(|i| (format!("{i:06}"), i)));
        assert_eq!(e, m);
    }

    #[test]
    fn text_map_reads_and_writes_str() {
        let mut t = TextMap::new();
        assert!(t.is_empty());
        assert_eq!(t.insert("part", "slide1.xml"), None);
        assert_eq!(t.insert(String::from("cells"), String::from("A1")), None);
        assert_eq!(t.insert("part", "slide2.xml").as_deref(), Some("slide1.xml"));
        assert_eq!(t.get("part"), Some("slide2.xml"));
        assert_eq!(&t["cells"], "A1");
        assert_eq!(t.len(), 2);
        assert!(t.contains_key("cells"));
        assert_eq!(t.iter().collect::<Vec<_>>(), [("cells", "A1"), ("part", "slide2.xml")]);
        assert_eq!(t.keys().collect::<Vec<_>>(), ["cells", "part"]);
        assert_eq!(t.values().collect::<Vec<_>>(), ["A1", "slide2.xml"]);
        assert_eq!(t.get_or_insert_with("cells", || "B2"), "A1");
        assert_eq!(t.get_or_insert_with("new", String::new), "");
        t.retain(|k, _| k != "new");
        t.extend([("a", "1")]);
        assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"a":"1","cells":"A1","part":"slide2.xml"}"#);
        assert_eq!(t.remove("a").as_deref(), Some("1"));
        let bt = BTreeMap::from(t.clone());
        assert_eq!(TextMap::from(bt), t);
        assert_eq!(TextMap::from(&t), t);
        let back: TextMap = serde_json::from_str(r#"{"b":"x","a":"y","b":"z"}"#).unwrap();
        assert_eq!(back.iter().collect::<Vec<_>>(), [("a", "y"), ("b", "z")]);
        let mut n = 0;
        for (_, v) in &back {
            n += v.len();
        }
        assert_eq!(n, 2);
        t.clear();
        assert!(t.is_empty());
        assert_eq!(back.into_iter().count(), 2);
    }
}
