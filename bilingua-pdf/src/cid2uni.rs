//! PDFium's CID to Unicode tables of the Adobe CJK collections (`CPDF_CID2UnicodeMap`), used for a Type 0 font with
//! an Identity CMap and no ToUnicode entry for the code. The data is PDFium's (BSD-3-Clause, see NOTICE), made into
//! `cid2uni.bin` by `tools/gen-pdf-cid2uni.py` (format described there): about 75 KB compiled in, inflated once
//! (about 166 KB) the first time a font needs it.

use std::io::Read;
use std::sync::OnceLock;

static BIN: &[u8] = include_bytes!("cid2uni.bin");

/// An Adobe character collection PDFium has a table for (its `CIDSet`, without `UCS`, which has no table).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CidSet {
    Gb1 = 0,
    Cns1 = 1,
    Japan1 = 2,
    Korea1 = 3,
}

impl CidSet {
    /// `CPDF_CMapParser::CharsetFromOrdering` (the Registry is not checked, as in PDFium).
    pub fn from_ordering(ordering: &[u8]) -> Option<CidSet> {
        match ordering {
            b"GB1" => Some(CidSet::Gb1),
            b"CNS1" => Some(CidSet::Cns1),
            b"Japan1" => Some(CidSet::Japan1),
            b"Korea1" => Some(CidSet::Korea1),
            _ => None,
        }
    }

    /// The table, indexed by CID (0 = no character).
    pub fn table(self) -> &'static [u16] {
        static TABLES: OnceLock<[Vec<u16>; 4]> = OnceLock::new();
        &TABLES.get_or_init(decode)[self as usize]
    }

    /// `CPDF_CID2UnicodeMap::UnicodeFromCID`.
    pub fn unicode(self, cid: u16) -> u16 {
        self.table().get(cid as usize).copied().unwrap_or(0)
    }
}

fn decode() -> [Vec<u16>; 4] {
    let mut raw = Vec::new();
    // The data is compiled in and checked by the tests: a failure here would leave the tables empty, never panic.
    let _ = flate2::read::ZlibDecoder::new(BIN).read_to_end(&mut raw);
    let mut words = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
    let mut out: [Vec<u16>; 4] = Default::default();
    for t in out.iter_mut() {
        let n = words.next().unwrap_or(0) as usize;
        t.reserve(n);
        let mut prev = 0u16;
        for d in words.by_ref().take(n) {
            prev = prev.wrapping_add(d).wrapping_add(1);
            t.push(prev);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_decode_with_pdfium_sizes_and_values() {
        assert_eq!(CidSet::Gb1.table().len(), 30284);
        assert_eq!(CidSet::Cns1.table().len(), 19088);
        assert_eq!(CidSet::Japan1.table().len(), 15444);
        assert_eq!(CidSet::Korea1.table().len(), 18352);
        for s in [CidSet::Gb1, CidSet::Cns1, CidSet::Japan1, CidSet::Korea1] {
            // CID 1 is the space in every Adobe CJK collection; CID 0 is .notdef.
            assert_eq!(s.unicode(1), 0x20, "{s:?}");
            assert_eq!(s.unicode(0), 0xFFFD, "{s:?}");
        }
        // Adobe-Japan1: CIDs 1-95 are ASCII (34 = A); 842, 843 are hiragana small a, a; 1125 is the first kanji.
        assert_eq!(CidSet::Japan1.unicode(34), u16::from(b'A'));
        assert_eq!(CidSet::Japan1.unicode(842), 0x3041);
        assert_eq!(CidSet::Japan1.unicode(843), 0x3042);
        assert_eq!(CidSet::Japan1.unicode(1125), 0x4E9C);
        // Adobe-GB1: 940 is U+554A (the first GB 2312 hanzi); Adobe-CNS1: 595 is U+4E00.
        assert_eq!(CidSet::Gb1.unicode(940), 0x554A);
        assert_eq!(CidSet::Cns1.unicode(595), 0x4E00);
        // Adobe-Korea1: 1086 is U+AC00 (the first hangul syllable).
        assert_eq!(CidSet::Korea1.unicode(1086), 0xAC00);
        assert_eq!(CidSet::Japan1.unicode(60000), 0);
        assert_eq!(CidSet::from_ordering(b"Identity"), None);
    }
}
