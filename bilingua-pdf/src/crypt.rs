//! The PDF standard security handler, read side only: a file whose user password is empty (an "owner password
//! only" file, which every viewer opens) is decrypted; a file that needs a password is refused, as PDFium (and so
//! Localization OS) refuses it.
//!
//! The primitives (MD5, SHA-256/384/512, AES-128/256, RC4) are small textbook implementations checked against the
//! standard test vectors below. They decrypt content anyone can open; they are not a security boundary, which
//! is why they live here instead of pulling in a crypto stack.

use crate::obj::{Dict, Obj};

const PAD: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08, 0x2e, 0x2e, 0x00, 0xb6,
    0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    None,
    Rc4,
    Aes,
}

#[derive(Debug, Clone)]
pub struct Handler {
    pub cipher: Cipher,
    key: Vec<u8>,
    pub encrypt_metadata: bool,
}

/// Why a file cannot be decrypted.
#[derive(Debug, PartialEq, Eq)]
pub enum CryptError {
    /// The file needs a password (PDFium: "Incorrect password error").
    Password,
    /// A security handler or settings this reader does not know (PDFium: "Unsupported security scheme").
    Unsupported,
}

fn get_int(d: &Dict, k: &[u8], resolve: &dyn Fn(&Obj) -> Obj) -> Option<i64> {
    d.get(k).map(resolve).and_then(|o| o.int())
}
fn get_bytes(d: &Dict, k: &[u8], resolve: &dyn Fn(&Obj) -> Obj) -> Vec<u8> {
    match d.get(k).map(resolve) {
        Some(Obj::Str(s)) => s.to_vec(),
        Some(Obj::Name(s)) => s.to_vec(),
        _ => Vec::new(),
    }
}

impl Handler {
    /// Opens the encryption dictionary with the empty password.
    pub fn open(enc: &Dict, file_id: &[u8], resolve: &dyn Fn(&Obj) -> Obj) -> Result<Handler, CryptError> {
        if get_bytes(enc, b"Filter", resolve) != b"Standard" {
            return Err(CryptError::Unsupported);
        }
        let v = get_int(enc, b"V", resolve).unwrap_or(0);
        let r = get_int(enc, b"R", resolve).unwrap_or(0);
        let p = get_int(enc, b"P", resolve).unwrap_or(-1) as i32 as u32;
        let encrypt_metadata = !matches!(enc.get(b"EncryptMetadata").map(resolve), Some(Obj::Bool(false)));
        let (cipher, key_len) = Self::crypt_info(enc, v, resolve)?;
        if cipher == Cipher::None {
            return Ok(Handler { cipher, key: vec![], encrypt_metadata });
        }
        let o = get_bytes(enc, b"O", resolve);
        let u = get_bytes(enc, b"U", resolve);
        if r >= 5 {
            let key = aes256_key(enc, &o, &u, p, resolve).ok_or(CryptError::Password)?;
            return Ok(Handler { cipher, key, encrypt_metadata });
        }
        for ignore_meta in [false, true] {
            let key = calc_key(b"", &o, p, file_id, r, key_len, ignore_meta || encrypt_metadata);
            if check_user(&key, &u, r, file_id) {
                return Ok(Handler { cipher, key, encrypt_metadata });
            }
        }
        Err(CryptError::Password)
    }

    fn crypt_info(enc: &Dict, v: i64, resolve: &dyn Fn(&Obj) -> Obj) -> Result<(Cipher, usize), CryptError> {
        let mut cipher = Cipher::Rc4;
        let keylen: i64;
        if v >= 4 {
            let stmf = get_bytes(enc, b"StmF", resolve);
            let strf = get_bytes(enc, b"StrF", resolve);
            if stmf != strf {
                return Err(CryptError::Unsupported);
            }
            let Some(Obj::Dict(cf)) = enc.get(b"CF").map(resolve) else { return Err(CryptError::Unsupported) };
            if strf == b"Identity" {
                return Ok((Cipher::None, 0));
            }
            let Some(Obj::Dict(def)) = cf.get(&strf).map(resolve) else { return Err(CryptError::Unsupported) };
            let mut bits = if v == 4 {
                match get_int(&def, b"Length", resolve).unwrap_or(0) {
                    0 => get_int(enc, b"Length", resolve).unwrap_or(128),
                    n => n,
                }
            } else {
                get_int(enc, b"Length", resolve).unwrap_or(256)
            };
            if bits < 0 {
                return Err(CryptError::Unsupported);
            }
            if bits < 40 {
                bits *= 8;
            }
            keylen = bits / 8;
            let cfm = get_bytes(&def, b"CFM", resolve);
            if cfm == b"AESV2" || cfm == b"AESV3" {
                cipher = Cipher::Aes;
            }
        } else {
            keylen = if v > 1 { get_int(enc, b"Length", resolve).unwrap_or(40) / 8 } else { 5 };
        }
        let ok = match cipher {
            Cipher::Aes => matches!(keylen, 16 | 24 | 32),
            Cipher::Rc4 => (5..=16).contains(&keylen),
            Cipher::None => true,
        };
        if !ok {
            return Err(CryptError::Unsupported);
        }
        Ok((cipher, keylen as usize))
    }

    /// Decrypts one string or stream of object `num` `gen`.
    pub fn decrypt(&self, num: u32, gen: u16, data: &[u8]) -> Vec<u8> {
        match self.cipher {
            Cipher::None => data.to_vec(),
            Cipher::Rc4 => rc4(&self.obj_key(num, gen), data),
            Cipher::Aes => {
                let key = if self.key.len() == 32 { self.key.clone() } else { self.obj_key(num, gen) };
                aes_cbc_decrypt(&key, data)
            }
        }
    }

    fn obj_key(&self, num: u32, gen: u16) -> Vec<u8> {
        let mut k = self.key.clone();
        k.extend_from_slice(&num.to_le_bytes()[..3]);
        k.extend_from_slice(&gen.to_le_bytes());
        if self.cipher == Cipher::Aes {
            k.extend_from_slice(b"sAlT");
        }
        let d = md5(&k);
        d[..(self.key.len() + 5).min(16)].to_vec()
    }
}

fn calc_key(password: &[u8], o: &[u8], p: u32, file_id: &[u8], r: i64, key_len: usize, include_meta: bool) -> Vec<u8> {
    let mut pass = [0u8; 32];
    let n = password.len().min(32);
    pass[..n].copy_from_slice(&password[..n]);
    pass[n..].copy_from_slice(&PAD[..32 - n]);
    let mut m = Vec::with_capacity(32 + o.len() + 4 + file_id.len() + 4);
    m.extend_from_slice(&pass);
    m.extend_from_slice(o);
    m.extend_from_slice(&p.to_le_bytes());
    m.extend_from_slice(file_id);
    if r >= 3 && !include_meta {
        m.extend_from_slice(&[0xff; 4]);
    }
    let mut d = md5(&m);
    let len = key_len.min(16);
    if r >= 3 {
        for _ in 0..50 {
            d = md5(&d[..len]);
        }
    }
    d[..len].to_vec()
}

fn check_user(key: &[u8], u: &[u8], r: i64, file_id: &[u8]) -> bool {
    if u.len() < 16 {
        return false;
    }
    if r == 2 {
        return rc4(key, &PAD)[..16] == u[..16];
    }
    let mut test = u[..u.len().min(32)].to_vec();
    for i in (0..=19u8).rev() {
        let k: Vec<u8> = key.iter().map(|b| b ^ i).collect();
        test = rc4(&k, &test);
    }
    let mut m = PAD.to_vec();
    m.extend_from_slice(file_id);
    md5(&m)[..16] == test[..16]
}

fn aes256_key(enc: &Dict, o: &[u8], u: &[u8], p: u32, resolve: &dyn Fn(&Obj) -> Obj) -> Option<Vec<u8>> {
    if o.len() < 48 || u.len() < 48 {
        return None;
    }
    let r = get_int(enc, b"R", resolve).unwrap_or(0);
    let hash = |salt: &[u8]| -> [u8; 32] {
        if r >= 6 {
            hash_r6(b"", salt, None)
        } else {
            sha256(salt)
        }
    };
    if hash(&u[32..40]) != u[..32] {
        return None;
    }
    let ik = hash(&u[40..48]);
    let ue = get_bytes(enc, b"UE", resolve);
    if ue.len() < 32 {
        return None;
    }
    let aes = Aes::new(&ik);
    let mut key = vec![0u8; 32];
    let mut prev = [0u8; 16];
    for (i, block) in ue[..32].chunks(16).enumerate() {
        let mut b = [0u8; 16];
        b.copy_from_slice(block);
        let d = aes.decrypt_block(&b);
        for j in 0..16 {
            key[i * 16 + j] = d[j] ^ prev[j];
        }
        prev = b;
    }
    let perms = get_bytes(enc, b"Perms", resolve);
    if perms.is_empty() {
        return None;
    }
    let mut pb = [0u8; 16];
    let n = perms.len().min(16);
    pb[..n].copy_from_slice(&perms[..n]);
    let buf = Aes::new(&key).decrypt_block(&pb);
    if &buf[9..12] != b"adb" || u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) != p {
        return None;
    }
    Some(key)
}

/// ISO 32000-2 algorithm 2.B (the revision 6 password hash).
fn hash_r6(password: &[u8], salt: &[u8], udata: Option<&[u8]>) -> [u8; 32] {
    let mut m = password.to_vec();
    m.extend_from_slice(salt);
    if let Some(u) = udata {
        m.extend_from_slice(u);
    }
    let mut k: Vec<u8> = sha256(&m).to_vec();
    let mut i = 0u32;
    loop {
        let mut k1 = Vec::new();
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            if let Some(u) = udata {
                k1.extend_from_slice(u);
            }
        }
        let aes = Aes::new(&k[..16]);
        let mut iv = [0u8; 16];
        iv.copy_from_slice(&k[16..32]);
        let mut e = Vec::with_capacity(k1.len());
        for block in k1.chunks(16) {
            let mut b = [0u8; 16];
            for j in 0..16 {
                b[j] = block[j] ^ iv[j];
            }
            iv = aes.encrypt_block(&b);
            e.extend_from_slice(&iv);
        }
        let modv = e[..16].iter().map(|x| *x as u32).sum::<u32>() % 3;
        k = match modv {
            0 => sha256(&e).to_vec(),
            1 => sha512_variant(&e, true),
            _ => sha512_variant(&e, false),
        };
        i += 1;
        if i >= 64 && (*e.last().unwrap_or(&0) as u32) <= i - 32 {
            break;
        }
        if i > 10_000 {
            break;
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&k[..32]);
    out
}

// ---------------------------------------------------------------- RC4

pub fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    let mut s: [u8; 256] = core::array::from_fn(|i| i as u8);
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|c| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            c ^ s[(s[i as usize].wrapping_add(s[j as usize])) as usize]
        })
        .collect()
}

// ---------------------------------------------------------------- MD5

pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4,
        11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: [u32; 64] = core::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32);
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    for chunk in msg.chunks(64) {
        let m: [u32; 16] = core::array::from_fn(|i| u32::from_le_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]));
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let t = d;
            d = c;
            c = b;
            b = b.wrapping_add(a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g]).rotate_left(S[i]));
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for i in 0..4 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_le_bytes());
    }
    out
}

// ---------------------------------------------------------------- SHA-2

pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
        0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
        0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

/// SHA-384 (`short`) or SHA-512.
pub fn sha512_variant(data: &[u8], short: bool) -> Vec<u8> {
    const K: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538, 0x59f111f1b605d019,
        0x923f82a4af194f9b, 0xab1c5ed5da6d8118, 0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694, 0xe49b69c19ef14ad2, 0xefbe4786384f25e3,
        0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65, 0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725,
        0x06ca6351e003826f, 0x142929670a0e6e70, 0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b, 0xa2bfe8a14cf10364, 0xa81a664bbc423001,
        0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb,
        0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3, 0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b, 0xca273eceea26619c, 0xd186b8c721c0c207,
        0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a,
        0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];
    let mut h: [u64; 8] = if short {
        [
            0xcbbb9d5dc1059ed8, 0x629a292a367cd507, 0x9159015a3070dd17, 0x152fecd8f70e5939, 0x67332667ffc00b31, 0x8eb44a8768581511,
            0xdb0c2e0d64f98fa7, 0x47b5481dbefa4fa4,
        ]
    } else {
        [
            0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1, 0x510e527fade682d1, 0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
        ]
    };
    let mut msg = data.to_vec();
    let bits = (data.len() as u128).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 128 != 112 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&chunk[8 * i..8 * i + 8]);
            w[i] = u64::from_be_bytes(b);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..80 {
            let s1 = v[4].rotate_right(14) ^ v[4].rotate_right(18) ^ v[4].rotate_right(41);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(28) ^ v[0].rotate_right(34) ^ v[0].rotate_right(39);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = Vec::with_capacity(64);
    for x in h {
        out.extend_from_slice(&x.to_be_bytes());
    }
    out.truncate(if short { 48 } else { 64 });
    out
}

// ---------------------------------------------------------------- AES

struct Aes {
    rk: Vec<[u8; 16]>,
    s: [u8; 256],
    inv: [u8; 256],
}

fn sbox() -> ([u8; 256], [u8; 256]) {
    let mut s = [0u8; 256];
    let mut inv = [0u8; 256];
    let (mut p, mut q) = (1u8, 1u8);
    loop {
        // p * 3 in GF(2^8)
        p = p ^ (p << 1) ^ if p & 0x80 != 0 { 0x1b } else { 0 };
        // q / 3
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4) ^ 0x63;
        s[p as usize] = x;
        if p == 1 {
            break;
        }
    }
    s[0] = 0x63;
    for i in 0..256 {
        inv[s[i] as usize] = i as u8;
    }
    (s, inv)
}

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0 }
}
fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    p
}

impl Aes {
    fn new(key: &[u8]) -> Aes {
        let (s, inv) = sbox();
        let nk = key.len() / 4;
        let nr = nk + 6;
        let mut w: Vec<[u8; 4]> = key.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        let mut rcon = 1u8;
        for i in nk..4 * (nr + 1) {
            let mut t = w[i - 1];
            if i % nk == 0 {
                t = [s[t[1] as usize] ^ rcon, s[t[2] as usize], s[t[3] as usize], s[t[0] as usize]];
                rcon = xtime(rcon);
            } else if nk > 6 && i % nk == 4 {
                t = [s[t[0] as usize], s[t[1] as usize], s[t[2] as usize], s[t[3] as usize]];
            }
            let p = w[i - nk];
            w.push([p[0] ^ t[0], p[1] ^ t[1], p[2] ^ t[2], p[3] ^ t[3]]);
        }
        let rk = w
            .chunks(4)
            .map(|c| {
                let mut r = [0u8; 16];
                for j in 0..4 {
                    r[4 * j..4 * j + 4].copy_from_slice(&c[j]);
                }
                r
            })
            .collect();
        Aes { rk, s, inv }
    }

    fn encrypt_block(&self, input: &[u8; 16]) -> [u8; 16] {
        let s = &self.s;
        let nr = self.rk.len() - 1;
        let mut st = *input;
        xor(&mut st, &self.rk[0]);
        for round in 1..=nr {
            for b in st.iter_mut() {
                *b = s[*b as usize];
            }
            shift_rows(&mut st, false);
            if round != nr {
                mix_columns(&mut st, false);
            }
            xor(&mut st, &self.rk[round]);
        }
        st
    }

    fn decrypt_block(&self, input: &[u8; 16]) -> [u8; 16] {
        let inv = &self.inv;
        let nr = self.rk.len() - 1;
        let mut st = *input;
        xor(&mut st, &self.rk[nr]);
        for round in (0..nr).rev() {
            shift_rows(&mut st, true);
            for b in st.iter_mut() {
                *b = inv[*b as usize];
            }
            xor(&mut st, &self.rk[round]);
            if round != 0 {
                mix_columns(&mut st, true);
            }
        }
        st
    }
}

fn xor(a: &mut [u8; 16], b: &[u8; 16]) {
    for i in 0..16 {
        a[i] ^= b[i];
    }
}

fn shift_rows(st: &mut [u8; 16], inverse: bool) {
    let old = *st;
    for c in 0..4 {
        for r in 0..4 {
            let src = if inverse { (c + 4 - r) % 4 } else { (c + r) % 4 };
            st[4 * c + r] = old[4 * src + r];
        }
    }
}

fn mix_columns(st: &mut [u8; 16], inverse: bool) {
    for c in 0..4 {
        let a = [st[4 * c], st[4 * c + 1], st[4 * c + 2], st[4 * c + 3]];
        let m: [u8; 4] = if inverse { [14, 11, 13, 9] } else { [2, 3, 1, 1] };
        for r in 0..4 {
            st[4 * c + r] = gmul(a[r], m[0]) ^ gmul(a[(r + 1) % 4], m[1]) ^ gmul(a[(r + 2) % 4], m[2]) ^ gmul(a[(r + 3) % 4], m[3]);
        }
    }
}

/// AES-CBC with the IV in the first 16 bytes and PKCS#5 padding removed when it is valid (PDFium keeps the bytes
/// when the padding is not).
pub fn aes_cbc_decrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    if data.len() < 16 || !matches!(key.len(), 16 | 24 | 32) {
        return Vec::new();
    }
    let aes = Aes::new(key);
    let mut prev = [0u8; 16];
    prev.copy_from_slice(&data[..16]);
    let mut out = Vec::with_capacity(data.len());
    for block in data[16..].chunks(16) {
        if block.len() < 16 {
            break;
        }
        let mut b = [0u8; 16];
        b.copy_from_slice(block);
        let d = aes.decrypt_block(&b);
        for j in 0..16 {
            out.push(d[j] ^ prev[j]);
        }
        prev = b;
    }
    if let Some(&pad) = out.last() {
        if (1..=16).contains(&pad) && out.len() >= pad as usize {
            out.truncate(out.len() - pad as usize);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn vectors() {
        assert_eq!(md5(b"").to_vec(), hex("d41d8cd98f00b204e9800998ecf8427e"));
        assert_eq!(md5(b"The quick brown fox jumps over the lazy dog").to_vec(), hex("9e107d9d372bb6826bd81d3542a419d6"));
        assert_eq!(sha256(b"abc").to_vec(), hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        assert_eq!(
            sha512_variant(b"abc", true),
            hex("cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7")
        );
        assert_eq!(
            sha512_variant(b"abc", false),
            hex("ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f")
        );
        assert_eq!(rc4(b"Key", b"Plaintext"), hex("bbf316e8d940af0ad3"));
        // FIPS-197 appendix C.
        let pt: [u8; 16] = hex("00112233445566778899aabbccddeeff").try_into().unwrap();
        let k128 = hex("000102030405060708090a0b0c0d0e0f");
        let ct = Aes::new(&k128).encrypt_block(&pt);
        assert_eq!(ct.to_vec(), hex("69c4e0d86a7b0430d8cdb78070b4c55a"));
        assert_eq!(Aes::new(&k128).decrypt_block(&ct), pt);
        let k256 = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let ct = Aes::new(&k256).encrypt_block(&pt);
        assert_eq!(ct.to_vec(), hex("8ea2b7ca516745bfeafc49904b496089"));
        assert_eq!(Aes::new(&k256).decrypt_block(&ct), pt);
    }
}
