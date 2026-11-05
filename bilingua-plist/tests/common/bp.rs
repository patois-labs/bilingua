//! A small, independent binary plist writer and reader for the tests (two-byte references, four-byte offsets).

#[derive(Debug, Clone, PartialEq)]
pub enum V {
    Str(String),
    Int(i64),
    Real(f64),
    Bool(bool),
    Date(f64),
    Data(Vec<u8>),
    Arr(Vec<V>),
    Dict(Vec<(String, V)>),
}

pub fn s(x: &str) -> V {
    V::Str(x.to_string())
}

pub fn dict(items: Vec<(&str, V)>) -> V {
    V::Dict(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn head(out: &mut Vec<u8>, hi: u8, n: usize) {
    if n < 15 {
        out.push(hi << 4 | n as u8);
    } else {
        out.push(hi << 4 | 0xF);
        out.push(0x11);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    }
}

fn emit(v: &V, objs: &mut Vec<Vec<u8>>) -> usize {
    let me = objs.len();
    objs.push(Vec::new());
    let mut o = Vec::new();
    match v {
        V::Str(x) if x.is_ascii() => {
            head(&mut o, 5, x.len());
            o.extend_from_slice(x.as_bytes());
        }
        V::Str(x) => {
            let u: Vec<u16> = x.encode_utf16().collect();
            head(&mut o, 6, u.len());
            for c in u {
                o.extend_from_slice(&c.to_be_bytes());
            }
        }
        V::Int(i) => {
            o.push(0x13);
            o.extend_from_slice(&i.to_be_bytes());
        }
        V::Real(r) => {
            o.push(0x23);
            o.extend_from_slice(&r.to_be_bytes());
        }
        V::Date(r) => {
            o.push(0x33);
            o.extend_from_slice(&r.to_be_bytes());
        }
        V::Bool(b) => o.push(if *b { 9 } else { 8 }),
        V::Data(d) => {
            head(&mut o, 4, d.len());
            o.extend_from_slice(d);
        }
        V::Arr(items) => {
            let refs: Vec<usize> = items.iter().map(|i| emit(i, objs)).collect();
            head(&mut o, 0xA, refs.len());
            for r in refs {
                o.extend_from_slice(&(r as u16).to_be_bytes());
            }
        }
        V::Dict(items) => {
            let keys: Vec<usize> = items.iter().map(|(k, _)| emit(&V::Str(k.clone()), objs)).collect();
            let vals: Vec<usize> = items.iter().map(|(_, v)| emit(v, objs)).collect();
            head(&mut o, 0xD, keys.len());
            for r in keys.into_iter().chain(vals) {
                o.extend_from_slice(&(r as u16).to_be_bytes());
            }
        }
    }
    objs[me] = o;
    me
}

pub fn write(v: &V) -> Vec<u8> {
    let mut objs = Vec::new();
    emit(v, &mut objs);
    let mut out = b"bplist00".to_vec();
    let mut offs = Vec::new();
    for o in &objs {
        offs.push(out.len() as u32);
        out.extend_from_slice(o);
    }
    let table = out.len() as u64;
    for o in offs {
        out.extend_from_slice(&o.to_be_bytes());
    }
    out.extend_from_slice(&[0; 6]);
    out.push(4);
    out.push(2);
    out.extend_from_slice(&(objs.len() as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&table.to_be_bytes());
    out
}

fn be(b: &[u8]) -> u64 {
    b.iter().fold(0, |a, x| a << 8 | *x as u64)
}

pub fn read(b: &[u8]) -> V {
    assert!(b.starts_with(b"bplist00"), "not a bplist: {:?}", &b[..b.len().min(8)]);
    let t = &b[b.len() - 32..];
    let (osz, rsz) = (t[6] as usize, t[7] as usize);
    let (top, table) = (be(&t[16..24]) as usize, be(&t[24..32]) as usize);
    obj(b, osz, rsz, table, top)
}

fn obj(b: &[u8], osz: usize, rsz: usize, table: usize, i: usize) -> V {
    let off = be(&b[table + i * osz..table + (i + 1) * osz]) as usize;
    let m = b[off];
    let (hi, lo) = (m >> 4, (m & 15) as usize);
    let (n, data) = if matches!(hi, 4 | 5 | 6 | 0xA | 0xD) && lo == 15 {
        let size = 1usize << (b[off + 1] & 15);
        (be(&b[off + 2..off + 2 + size]) as usize, off + 2 + size)
    } else {
        (lo, off + 1)
    };
    match hi {
        0 => V::Bool(m == 9),
        1 => {
            let size = 1usize << lo;
            let raw = &b[off + 1..off + 1 + size];
            V::Int(if size == 8 { i64::from_be_bytes(raw.try_into().unwrap()) } else { be(raw) as i64 })
        }
        2 => V::Real(if lo == 2 { f32::from_be_bytes(b[off + 1..off + 5].try_into().unwrap()) as f64 } else { f64::from_be_bytes(b[off + 1..off + 9].try_into().unwrap()) }),
        3 => V::Date(f64::from_be_bytes(b[off + 1..off + 9].try_into().unwrap())),
        4 => V::Data(b[data..data + n].to_vec()),
        5 => V::Str(String::from_utf8(b[data..data + n].to_vec()).unwrap()),
        6 => V::Str(String::from_utf16(&b[data..data + 2 * n].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()).unwrap()),
        0xA => V::Arr((0..n).map(|k| obj(b, osz, rsz, table, be(&b[data + k * rsz..data + (k + 1) * rsz]) as usize)).collect()),
        0xD => V::Dict(
            (0..n)
                .map(|k| {
                    let key = match obj(b, osz, rsz, table, be(&b[data + k * rsz..data + (k + 1) * rsz]) as usize) {
                        V::Str(s) => s,
                        other => panic!("key {other:?}"),
                    };
                    let v = obj(b, osz, rsz, table, be(&b[data + (n + k) * rsz..data + (n + k + 1) * rsz]) as usize);
                    (key, v)
                })
                .collect(),
        ),
        other => panic!("object type {other}"),
    }
}
