//! A guess for a subtitle file saved in Simplified Chinese (GB2312 / GBK) with no byte order mark, when no source
//! language says which code page to read it in (GitHub bilingua#11). Without it such a file is read as
//! Windows-1252 and every Chinese line turns into accented Latin letters.
//!
//! The guess is narrow on purpose: Windows-1252 text with accents is often also a valid GBK byte stream (an `é`
//! before an ASCII letter is a GBK pair), so a valid GBK reading alone proves nothing. Chinese text in GB2312 is
//! made of runs of two-byte characters whose both bytes are 0xA1 or above; Latin text has single accented letters
//! between ASCII ones. Only a file whose bytes look like the first is read as GBK, and the reader says so in a
//! warning. Native mode only: the LocOS profile reads such a file as Localization OS does.

/// Whether `data` (not UTF-8, no byte order mark) reads as GB2312 / GBK text: every byte at or above 0x80 starts
/// a valid GBK pair, at least 8 pairs, at least 95 % of them in the GB2312 range (both bytes 0xA1 to 0xFE), and the
/// runs of non-ASCII bytes are on average at least two characters long.
pub fn looks_gbk(data: &[u8]) -> bool {
    let (mut pairs, mut strong, mut runs, mut run_bytes) = (0usize, 0usize, 0usize, 0usize);
    let mut in_run = false;
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            in_run = false;
            i += 1;
            continue;
        }
        let Some(&t) = data.get(i + 1) else { return false };
        if !(0x81..=0xFE).contains(&b) || !(0x40..=0xFE).contains(&t) || t == 0x7F {
            return false;
        }
        pairs += 1;
        if b >= 0xA1 && t >= 0xA1 {
            strong += 1;
        }
        if !in_run {
            runs += 1;
            in_run = true;
        }
        run_bytes += 2;
        // a pair whose second byte is ASCII ends the run (it is one accented letter before a letter in Latin text)
        if t < 0x80 {
            in_run = false;
        }
        i += 2;
    }
    pairs >= 8 && strong * 100 >= pairs * 95 && run_bytes >= runs * 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_reads_as_gbk_and_latin_does_not() {
        // "Universe guardian" and a second line, in GB2312
        let gb: &[u8] = b"1\r\n00:00:01,000 --> 00:00:02,000\r\n\xd3\xee\xd6\xe6\xca\xd8\xbb\xa4\xc9\xf1\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n\xc4\xe3\xba\xc3\xa3\xac\xca\xc0\xbd\xe7\r\n";
        assert!(looks_gbk(gb));
        // French and German in Windows-1252, with accents before letters and two accents in a row
        let fr: &[u8] = b"\xc9l\xe8ve, caf\xe9, \xe0 l'\xe9cole, d\xe9j\xe0 vu, na\xefve, \xe9t\xe9, \xe9l\xe9ments, \xc0 bient\xf4t, f\xeate";
        assert!(!looks_gbk(fr));
        let de: &[u8] = b"Gr\xfc\xdfe aus M\xfcnchen, sch\xf6ne Gr\xfc\xdfe, \xc4rger, \xd6l, \xdcbel, Stra\xdfe, gro\xdf, s\xfc\xdf";
        assert!(!looks_gbk(de));
        // too little to tell
        assert!(!looks_gbk(b"\xc4\xe3\xba\xc3"));
    }
}
