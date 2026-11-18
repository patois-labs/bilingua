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
//! **Two representations, one behaviour.** Up to [`SMALL_MAX`] entries the map is a sorted boxed slice, exact
//! size, so an insert or a remove reallocates it and shifts at most `SMALL_MAX` entries. Above that it moves its
//! entries into a boxed `BTreeMap` (one extra allocation for a map that is large anyway), where an insert costs
//! `O(log n)`: a unit with 200,000 inline codes inserted one at a time in any order is built in linear-log time,
//! not quadratic. It moves back to the slice when a remove or `retain` leaves [`SMALL_MIN`] entries or fewer (the
//! gap stops a map that hovers around the limit from switching on every call). Iteration order, JSON bytes,
//! `PartialEq`, `Eq`, `Hash`, `Ord`, `Debug` and the `Index` panic are the same in both representations; nothing
//! but [`SmallMap::is_compact`] tells them apart. Inline size: 16 bytes.
//!
//! Bulk building (`collect`, `extend` on a slice, `From<BTreeMap>`, `From<[(K, V); N]>`, reading JSON) sorts once
//! and picks the representation for the final size.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use std::cmp::Ordering;
use std::collections::{btree_map, BTreeMap};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::{FusedIterator, Map};
use std::marker::PhantomData;
use std::ops::Index;

/// Most entries a map keeps in its sorted slice; one more and it moves to a `BTreeMap`.
pub const SMALL_MAX: usize = 32;
/// A map in a `BTreeMap` moves back to a slice when a remove leaves this many entries or fewer.
pub const SMALL_MIN: usize = 16;

/// A map from text keys to `V`. See the [module docs](self).
#[derive(Clone)]
pub struct SmallMap<V> {
    repr: Repr<V>,
}

#[derive(Clone)]
enum Repr<V> {
    Small(Box<[(Box<str>, V)]>),
    Large(Box<BTreeMap<Box<str>, V>>),
}

/// One of two iterators with the same item.
#[derive(Clone)]
enum Either<A, B> {
    A(A),
    B(B),
}

impl<A: Iterator, B: Iterator<Item = A::Item>> Iterator for Either<A, B> {
    type Item = A::Item;
    fn next(&mut self) -> Option<A::Item> {
        match self {
            Either::A(a) => a.next(),
            Either::B(b) => b.next(),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Either::A(a) => a.size_hint(),
            Either::B(b) => b.size_hint(),
        }
    }
}

impl<A: DoubleEndedIterator, B: DoubleEndedIterator<Item = A::Item>> DoubleEndedIterator for Either<A, B> {
    fn next_back(&mut self) -> Option<A::Item> {
        match self {
            Either::A(a) => a.next_back(),
            Either::B(b) => b.next_back(),
        }
    }
}

/// A public iterator: a newtype over `Either` of the slice and the tree iterator.
macro_rules! iterator {
    ($(#[$doc:meta])* $name:ident<$($lt:lifetime,)? V>, $item:ty, $small:ty, $large:ty) => {
        $(#[$doc])*
        pub struct $name<$($lt,)? V>(Either<$small, $large>);
        impl<$($lt,)? V> Iterator for $name<$($lt,)? V> {
            type Item = $item;
            fn next(&mut self) -> Option<$item> {
                self.0.next()
            }
            fn size_hint(&self) -> (usize, Option<usize>) {
                self.0.size_hint()
            }
        }
        impl<$($lt,)? V> DoubleEndedIterator for $name<$($lt,)? V> {
            fn next_back(&mut self) -> Option<$item> {
                self.0.next_back()
            }
        }
        impl<$($lt,)? V> ExactSizeIterator for $name<$($lt,)? V> {}
        impl<$($lt,)? V> FusedIterator for $name<$($lt,)? V> {}
    };
}

iterator!(
    /// Iterator over `(&str, &V)`, in key order.
    Iter<'a, V>, (&'a str, &'a V),
    Map<std::slice::Iter<'a, (Box<str>, V)>, for<'x> fn(&'x (Box<str>, V)) -> (&'x str, &'x V)>,
    Map<btree_map::Iter<'a, Box<str>, V>, for<'x> fn((&'x Box<str>, &'x V)) -> (&'x str, &'x V)>
);
iterator!(
    /// Iterator over `(&str, &mut V)`, in key order.
    IterMut<'a, V>, (&'a str, &'a mut V),
    Map<std::slice::IterMut<'a, (Box<str>, V)>, for<'x> fn(&'x mut (Box<str>, V)) -> (&'x str, &'x mut V)>,
    Map<btree_map::IterMut<'a, Box<str>, V>, for<'x> fn((&'x Box<str>, &'x mut V)) -> (&'x str, &'x mut V)>
);
iterator!(
    /// Iterator over the keys, in order.
    Keys<'a, V>, &'a str,
    Map<std::slice::Iter<'a, (Box<str>, V)>, for<'x> fn(&'x (Box<str>, V)) -> &'x str>,
    Map<btree_map::Keys<'a, Box<str>, V>, for<'x> fn(&'x Box<str>) -> &'x str>
);
iterator!(
    /// Iterator over the values, in key order.
    Values<'a, V>, &'a V,
    Map<std::slice::Iter<'a, (Box<str>, V)>, for<'x> fn(&'x (Box<str>, V)) -> &'x V>,
    btree_map::Values<'a, Box<str>, V>
);
iterator!(
    /// Iterator over the values, mutable, in key order.
    ValuesMut<'a, V>, &'a mut V,
    Map<std::slice::IterMut<'a, (Box<str>, V)>, for<'x> fn(&'x mut (Box<str>, V)) -> &'x mut V>,
    btree_map::ValuesMut<'a, Box<str>, V>
);
iterator!(
    /// Owning iterator over `(Box<str>, V)`, in key order.
    IntoIter<V>, (Box<str>, V),
    std::vec::IntoIter<(Box<str>, V)>,
    btree_map::IntoIter<Box<str>, V>
);

impl<V> Clone for Iter<'_, V> {
    fn clone(&self) -> Self {
        Iter(self.0.clone())
    }
}
impl<V> Clone for Keys<'_, V> {
    fn clone(&self) -> Self {
        Keys(self.0.clone())
    }
}
impl<V> Clone for Values<'_, V> {
    fn clone(&self) -> Self {
        Values(self.0.clone())
    }
}

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
#[allow(clippy::borrowed_box)] // the signatures the tree adapters need
fn tree_ref<'x, V>((k, v): (&'x Box<str>, &'x V)) -> (&'x str, &'x V) {
    (k, v)
}
#[allow(clippy::borrowed_box)]
fn tree_mut<'x, V>((k, v): (&'x Box<str>, &'x mut V)) -> (&'x str, &'x mut V) {
    (k, v)
}
#[allow(clippy::borrowed_box)]
fn tree_key(k: &Box<str>) -> &str {
    k
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
        SmallMap { repr: Repr::Small(Box::default()) }
    }
}

impl<V> SmallMap<V> {
    /// An empty map (allocates nothing).
    pub fn new() -> SmallMap<V> {
        SmallMap::default()
    }

    /// Entries sorted by key, no key twice: a slice up to `SMALL_MAX`, a tree above (built in bulk).
    pub(crate) fn from_sorted_vec(v: Vec<(Box<str>, V)>) -> SmallMap<V> {
        let repr = if v.len() > SMALL_MAX { Repr::Large(Box::new(v.into_iter().collect())) } else { Repr::Small(v.into_boxed_slice()) };
        SmallMap { repr }
    }

    /// True while the entries are kept in the sorted slice (at most [`SMALL_MAX`] of them, or a map that shrank to
    /// [`SMALL_MIN`]). For tests and measurements; behaviour is the same either way.
    #[doc(hidden)]
    pub fn is_compact(&self) -> bool {
        matches!(self.repr, Repr::Small(_))
    }

    fn find(s: &[(Box<str>, V)], key: &str) -> Result<usize, usize> {
        s.binary_search_by(|(k, _)| (**k).cmp(key))
    }

    /// Edits a slice as a `Vec` and stores it back at its exact length.
    fn edit<R>(s: &mut Box<[(Box<str>, V)]>, f: impl FnOnce(&mut Vec<(Box<str>, V)>) -> R) -> R {
        let mut v = std::mem::take(s).into_vec();
        let r = f(&mut v);
        *s = v.into_boxed_slice();
        r
    }

    /// Moves a slice that is full to a tree.
    fn grow(&mut self) -> &mut BTreeMap<Box<str>, V> {
        if let Repr::Small(s) = &mut self.repr {
            let tree: BTreeMap<Box<str>, V> = std::mem::take(s).into_vec().into_iter().collect();
            self.repr = Repr::Large(Box::new(tree));
        }
        match &mut self.repr {
            Repr::Large(t) => t,
            Repr::Small(_) => unreachable!("just grown"),
        }
    }

    /// Moves a tree that has shrunk back to a slice.
    fn shrink(&mut self) {
        if let Repr::Large(t) = &mut self.repr {
            if t.len() <= SMALL_MIN {
                let v: Vec<(Box<str>, V)> = std::mem::take(&mut **t).into_iter().collect();
                self.repr = Repr::Small(v.into_boxed_slice());
            }
        }
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        match &self.repr {
            Repr::Small(s) => s.len(),
            Repr::Large(t) => t.len(),
        }
    }

    /// True when the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The value for `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        match &self.repr {
            Repr::Small(s) => Self::find(s, key).ok().map(|i| &s[i].1),
            Repr::Large(t) => t.get(key),
        }
    }

    /// The value for `key`, mutable.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        match &mut self.repr {
            Repr::Small(s) => Self::find(s, key).ok().map(|i| &mut s[i].1),
            Repr::Large(t) => t.get_mut(key),
        }
    }

    /// The stored key and the value for `key`.
    pub fn get_key_value(&self, key: &str) -> Option<(&str, &V)> {
        match &self.repr {
            Repr::Small(s) => Self::find(s, key).ok().map(|i| entry_ref(&s[i])),
            Repr::Large(t) => t.get_key_value(key).map(tree_ref),
        }
    }

    /// True when `key` has a value.
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Sets the value for `key` and returns the one it replaces.
    pub fn insert<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, value: V) -> Option<V> {
        if let Repr::Small(s) = &mut self.repr {
            // an insert that sorts last skips the search
            let at = match s.last() {
                Some((last, _)) if **last >= *key.as_ref() => Self::find(s, key.as_ref()),
                _ => Err(s.len()),
            };
            match at {
                Ok(i) => return Some(std::mem::replace(&mut s[i].1, value)),
                Err(i) if s.len() < SMALL_MAX => {
                    Self::edit(s, |v| {
                        v.reserve_exact(1);
                        v.insert(i, (key.into(), value));
                    });
                    return None;
                }
                Err(_) => {}
            }
        }
        let t = self.grow();
        if let Some(slot) = t.get_mut(key.as_ref()) {
            return Some(std::mem::replace(slot, value));
        }
        t.insert(key.into(), value)
    }

    /// Removes `key` and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        match &mut self.repr {
            Repr::Small(s) => {
                let i = Self::find(s, key).ok()?;
                Some(Self::edit(s, |v| v.remove(i)).1)
            }
            Repr::Large(t) => {
                let out = t.remove(key);
                self.shrink();
                out
            }
        }
    }

    /// Removes every entry (and frees the storage).
    pub fn clear(&mut self) {
        *self = SmallMap::new();
    }

    /// Keeps only the entries for which `keep` returns true.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut V) -> bool) {
        match &mut self.repr {
            Repr::Small(s) => Self::edit(s, |v| v.retain_mut(|(k, val)| keep(k, val))),
            Repr::Large(t) => {
                t.retain(|k, v| keep(k, v));
                self.shrink();
            }
        }
    }

    /// The value for `key`, inserting `make()` first when there is none (what `entry(key).or_insert_with(make)`
    /// does on a `BTreeMap`). The key is only converted when it is inserted.
    pub fn get_or_insert_with<K: AsRef<str> + Into<Box<str>>>(&mut self, key: K, make: impl FnOnce() -> V) -> &mut V {
        if let Repr::Small(s) = &self.repr {
            match Self::find(s, key.as_ref()) {
                Ok(i) => {
                    let Repr::Small(s) = &mut self.repr else { unreachable!("small") };
                    return &mut s[i].1;
                }
                Err(i) if s.len() < SMALL_MAX => {
                    let Repr::Small(s) = &mut self.repr else { unreachable!("small") };
                    let (k, v) = (key.into(), make());
                    Self::edit(s, |e| {
                        e.reserve_exact(1);
                        e.insert(i, (k, v));
                    });
                    return &mut s[i].1;
                }
                Err(_) => {}
            }
        }
        let t = self.grow();
        if t.contains_key(key.as_ref()) {
            return t.get_mut(key.as_ref()).expect("present");
        }
        t.entry(key.into()).or_insert_with(make)
    }

    /// The entries as `(&str, &V)`, in key order.
    pub fn iter(&self) -> Iter<'_, V> {
        Iter(match &self.repr {
            Repr::Small(s) => Either::A(s.iter().map(entry_ref::<V> as _)),
            Repr::Large(t) => Either::B(t.iter().map(tree_ref::<V> as _)),
        })
    }

    /// The entries as `(&str, &mut V)`, in key order.
    pub fn iter_mut(&mut self) -> IterMut<'_, V> {
        IterMut(match &mut self.repr {
            Repr::Small(s) => Either::A(s.iter_mut().map(entry_mut::<V> as _)),
            Repr::Large(t) => Either::B(t.iter_mut().map(tree_mut::<V> as _)),
        })
    }

    /// The keys, in order.
    pub fn keys(&self) -> Keys<'_, V> {
        Keys(match &self.repr {
            Repr::Small(s) => Either::A(s.iter().map(entry_key::<V> as _)),
            Repr::Large(t) => Either::B(t.keys().map(tree_key as _)),
        })
    }

    /// The values, in key order.
    pub fn values(&self) -> Values<'_, V> {
        Values(match &self.repr {
            Repr::Small(s) => Either::A(s.iter().map(entry_value::<V> as _)),
            Repr::Large(t) => Either::B(t.values()),
        })
    }

    /// The values, mutable, in key order.
    pub fn values_mut(&mut self) -> ValuesMut<'_, V> {
        ValuesMut(match &mut self.repr {
            Repr::Small(s) => Either::A(s.iter_mut().map(entry_value_mut::<V> as _)),
            Repr::Large(t) => Either::B(t.values_mut()),
        })
    }
}

impl<V: PartialEq> PartialEq for SmallMap<V> {
    /// Same entries, whichever representation each side has.
    fn eq(&self, other: &SmallMap<V>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<V: Eq> Eq for SmallMap<V> {}

impl<V: PartialOrd> PartialOrd for SmallMap<V> {
    /// Entry by entry, as a `BTreeMap` compares.
    fn partial_cmp(&self, other: &SmallMap<V>) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<V: Ord> Ord for SmallMap<V> {
    fn cmp(&self, other: &SmallMap<V>) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<V: Hash> Hash for SmallMap<V> {
    /// The length, then every entry in order: equal maps hash the same in either representation.
    fn hash<H: Hasher>(&self, h: &mut H) {
        h.write_usize(self.len());
        for (k, v) in self {
            k.hash(h);
            v.hash(h);
        }
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
        IntoIter(match self.repr {
            Repr::Small(s) => Either::A(s.into_vec().into_iter()),
            Repr::Large(t) => Either::B(t.into_iter()),
        })
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
    /// A new value replaces an old one for the same key; of repeated new keys the last wins. A slice merges the
    /// new entries in one pass; a tree inserts them one by one.
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        let s = match &mut self.repr {
            Repr::Large(t) => {
                for (k, v) in iter {
                    t.insert(k.into(), v);
                }
                return;
            }
            Repr::Small(s) => s,
        };
        let new = sorted_last_wins(iter.into_iter().map(|(k, v)| (k.into(), v)).collect::<Vec<_>>());
        if new.is_empty() {
            return;
        }
        let old = std::mem::take(s).into_vec();
        let mut out = Vec::with_capacity(old.len() + new.len());
        let (mut a, mut b) = (old.into_iter().peekable(), new.into_iter().peekable());
        loop {
            let take_old = match (a.peek(), b.peek()) {
                (Some(x), Some(y)) => match x.0.cmp(&y.0) {
                    Ordering::Less => true,
                    Ordering::Equal => {
                        a.next();
                        false
                    }
                    Ordering::Greater => false,
                },
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            out.extend(if take_old { a.next() } else { b.next() });
        }
        *self = SmallMap::from_sorted_vec(out);
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

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    #[test]
    fn a_full_slice_moves_to_a_tree_and_back() {
        let mut m = SmallMap::new();
        for i in 0..SMALL_MAX {
            m.insert(format!("k{i:02}"), i);
        }
        assert!(m.is_compact());
        m.insert("k99", 99);
        assert!(!m.is_compact());
        assert_eq!(m.len(), SMALL_MAX + 1);
        // removes down to SMALL_MIN + 1 keep the tree, the next one goes back to the slice
        let keys: Vec<String> = m.keys().map(str::to_string).collect();
        for k in &keys[..SMALL_MAX - SMALL_MIN] {
            assert!(m.remove(k).is_some());
        }
        assert_eq!(m.len(), SMALL_MIN + 1);
        assert!(!m.is_compact());
        m.remove(&keys[SMALL_MAX - SMALL_MIN]);
        assert!(m.is_compact());
        assert_eq!(m.len(), SMALL_MIN);
        // get_or_insert_with, extend, collect and retain cross the limit too
        let mut g: SmallMap<usize> = (0..SMALL_MAX).map(|i| (format!("{i}"), i)).collect();
        assert!(g.is_compact());
        *g.get_or_insert_with("new", || 0) += 1;
        assert!(!g.is_compact());
        assert_eq!(g["new"], 1);
        g.retain(|k, _| k.len() == 1);
        assert!(g.is_compact());
        assert_eq!(g.len(), 10);
        g.extend((0..SMALL_MAX).map(|i| (format!("x{i}"), i)));
        assert!(!g.is_compact());
        assert_eq!(g.len(), 10 + SMALL_MAX);
        let c: SmallMap<u8> = (0..=SMALL_MAX).map(|i| (format!("{i}"), 0)).collect();
        assert!(!c.is_compact());
        g.clear();
        assert!(g.is_compact() && g.is_empty());
    }

    #[test]
    fn both_representations_look_the_same() {
        let big: SmallMap<u32> = (0..40).map(|i| (format!("k{i}"), i)).collect();
        let mut tree = big.clone();
        for i in 20..40 {
            tree.remove(&format!("k{i}"));
        }
        let slice: SmallMap<u32> = (0..20).map(|i| (format!("k{i}"), i)).collect();
        assert!(!tree.is_compact() && slice.is_compact());
        assert_eq!(tree, slice);
        assert_eq!(slice, tree);
        assert_eq!(hash_of(&tree), hash_of(&slice));
        assert_eq!(tree.cmp(&slice), Ordering::Equal);
        assert_eq!(tree.partial_cmp(&slice), Some(Ordering::Equal));
        assert_eq!(format!("{tree:?}"), format!("{slice:?}"));
        assert_eq!(serde_json::to_string(&tree).unwrap(), serde_json::to_string(&slice).unwrap());
        assert_eq!(tree.iter().collect::<Vec<_>>(), slice.iter().collect::<Vec<_>>());
        assert_eq!(tree.iter().rev().collect::<Vec<_>>(), slice.iter().rev().collect::<Vec<_>>());
        assert_eq!(tree.keys().len(), 20);
        assert_eq!(tree.values().sum::<u32>(), slice.values().sum::<u32>());
        assert_eq!(tree.get_key_value("k3"), slice.get_key_value("k3"));
        assert_eq!(tree.clone().into_iter().collect::<Vec<_>>(), slice.clone().into_iter().collect::<Vec<_>>());
        assert_eq!(BTreeMap::from(tree.clone()), BTreeMap::from(slice.clone()));
        let mut other = slice.clone();
        other.insert("k0", 100);
        assert_eq!(tree.cmp(&other), slice.cmp(&other));
        assert_ne!(tree, other);
        let t = TextMap::from_iter((0..40).map(|i| (format!("k{i}"), "v")));
        assert_eq!(serde_json::to_string(&t).unwrap(), serde_json::to_string(&BTreeMap::from(t.clone())).unwrap());
    }

    #[test]
    #[should_panic(expected = "no entry found for key")]
    fn index_panics_the_same_on_a_tree() {
        let m: SmallMap<u8> = (0..100).map(|i| (format!("{i}"), 0)).collect();
        assert!(!m.is_compact());
        let _ = m["x"];
    }

    #[test]
    fn json_picks_the_representation_by_size() {
        let body: Vec<String> = (0..50).rev().map(|i| format!("\"{i}\":{i}")).collect();
        let m: SmallMap<u32> = serde_json::from_str(&format!("{{{}}}", body.join(","))).unwrap();
        assert!(!m.is_compact());
        let bt: BTreeMap<String, u32> = serde_json::from_str(&format!("{{{}}}", body.join(","))).unwrap();
        assert_eq!(serde_json::to_string(&m).unwrap(), serde_json::to_string(&bt).unwrap());
        assert!(SmallMap::from(BTreeMap::from([("a".to_string(), 1)])).is_compact());
        assert!(!SmallMap::from(bt).is_compact());
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
