//! Resource limit fixes from the security review (run 2): one subtitle group joined from many blocks is refused
//! before its text is decoded.

mod common;
mod peak;
use bilingua_model::*;
use bilingua_stl::Stl;
use common::*;

#[test]
fn one_huge_group_is_refused_before_decode() {
    // 20,000 text blocks of one subtitle (same number, extension block 0x00): the joined text is far over the
    // unit limit, so it must be refused while the blocks are joined, not after every byte became a decoded cell
    let n = 20_000;
    let mut file = gsi_default(n);
    let body = vec![b'a'; 112];
    for _ in 0..n {
        file.extend(tti(&Tti { sn: 1, ebn: 0x00, ..Default::default() }, &body));
    }
    let (r, bytes) = peak::peak(|| Stl.extract(&file, &opts(None)));
    assert!(matches!(r, Err(Error::Limit(ref e)) if e.kind == LimitKind::UnitChars), "{r:?}");
    assert!(bytes < 16 * file.len(), "{bytes} bytes held for a {} byte file", file.len());
    // a subtitle of a few blocks still reads
    let mut file = gsi_default(3);
    for ebn in [0x00, 0x01, 0xFF] {
        file.extend(tti(&Tti { sn: 1, ebn, ..Default::default() }, b"Hello"));
    }
    assert_eq!(Stl.extract(&file, &opts(None)).unwrap().units.len(), 1);
}
