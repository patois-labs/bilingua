//! How many plural forms a Qt numerus message has in a language: the CLDR categories whole numbers reach, from the
//! shared table in `bilingua_model::cldr` (the data Localization OS reads through Babel), so a target language gets
//! exactly the forms LocOS would give it. Unlike the cardinal table, a category that only fractions reach (`other`
//! in Polish and Russian, `many` in Czech and in French, Spanish, Italian and Portuguese) does not count: a Qt
//! message counts whole items. Those 18 languages are listed here; every other language has the categories of the
//! shared table.

/// The Babel version the shared table was generated from.
pub const BABEL_VERSION: &str = "2.18.0";

/// Languages whose cardinal categories include one that no whole number reaches: the categories whole numbers
/// 0..=1000 do reach (bit 0 zero, 1 one, 2 two, 3 few, 4 many, 5 other). Sorted by code.
const WHOLE_NUMBERS: &[(&str, u8)] = &[
    ("be", 26),
    ("br", 46),
    ("ca", 34),
    ("cs", 42),
    ("es", 34),
    ("fr", 34),
    ("gv", 46),
    ("ht", 34),
    ("it", 34),
    ("lld", 34),
    ("lt", 42),
    ("pl", 26),
    ("pt", 34),
    ("ru", 26),
    ("scn", 34),
    ("sk", 42),
    ("uk", 26),
    ("vec", 34),
];

/// Number of plural forms for `locale`, or 2 (`one`, `other`) for an unknown or empty locale, as LocOS.
pub fn forms(locale: &str) -> usize {
    mask(locale).map(|m| m.count_ones() as usize).unwrap_or(2)
}

fn mask(locale: &str) -> Option<u8> {
    let lang = bilingua_model::cldr::language(locale);
    let bits = bilingua_model::cldr::bits_babel(&lang, false)?;
    let canon = bilingua_model::cldr::babel_language(&lang);
    Some(WHOLE_NUMBERS.binary_search_by(|(c, _)| (*c).cmp(canon)).map_or(bits, |i| WHOLE_NUMBERS[i].1))
}

#[cfg(test)]
#[allow(dead_code)]
mod old {
    include!("../tests/data/old_cldr.rs");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exceptions_are_sorted() {
        assert!(WHOLE_NUMBERS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    /// Every code of two or three letters, `root`, `und` and some malformed ones, bare and with a script or
    /// territory: the shared table answers exactly as the crate's own table did.
    #[test]
    fn shared_table_answers_like_the_old_private_one() {
        let mut langs: Vec<String> = vec!["root".into(), "und".into(), "".into(), " ".into(), "x1".into(), "abcd".into()];
        for a in b'a'..=b'z' {
            for b in b'a'..=b'z' {
                langs.push(format!("{}{}", a as char, b as char));
                for c in b'a'..=b'z' {
                    langs.push(format!("{}{}{}", a as char, b as char, c as char));
                }
            }
        }
        let (mut n, mut known) = (0usize, 0usize);
        for l in &langs {
            for s in ["", "-DE", "_Latn", "-Hant-TW", "-"] {
                for tag in [format!("{l}{s}"), format!("{}{s}", l.to_uppercase()), format!(" {l}{s} ")] {
                    let old_forms = old::forms(&tag);
                    assert_eq!(old_forms, forms(&tag), "{tag:?}");
                    n += 1;
                    known += usize::from(old_forms != 2);
                }
            }
        }
        assert!(n > 250_000 && known > 1000, "n={n} known={known}");
    }
}
