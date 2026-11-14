//! CLDR plural and ordinal categories per language, from the shared table in `bilingua_model::cldr` (the data
//! Localization OS reads through Babel), so a target language gets exactly the categories LocOS would give it.

/// The Babel version the shared table was generated from.
pub const BABEL_VERSION: &str = "2.18.0";

/// The categories the language uses, in CLDR order, always with `other`. `None` for a language the table does
/// not know (callers then leave the plural forms as they are). Babel's aliases (`deu`, `iw`, `tl`, `und`) resolve,
/// as in LocOS.
pub fn categories(locale: &str, ordinal: bool) -> Option<Vec<&'static str>> {
    let lang = bilingua_model::cldr::language(locale);
    bilingua_model::cldr::bits_babel(&lang, ordinal).map(bilingua_model::cldr::names)
}

#[cfg(test)]
#[allow(dead_code)]
mod old {
    include!("../tests/data/old_cldr.rs");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every language code of two or three letters, `root`, `und` and some malformed ones, each bare and with a
    /// script or territory, in both modes: the shared table answers exactly as the crate's own table did.
    #[test]
    fn shared_table_answers_like_the_old_private_one() {
        let mut langs: Vec<String> = vec!["root".into(), "und".into(), "".into(), " ".into(), "x1".into(), "e".into(), "abcd".into(), "pt".into()];
        for a in b'a'..=b'z' {
            for b in b'a'..=b'z' {
                langs.push(format!("{}{}", a as char, b as char));
                for c in b'a'..=b'z' {
                    langs.push(format!("{}{}{}", a as char, b as char, c as char));
                }
            }
        }
        let suffixes = ["", "-DE", "_Latn", "-Hant-TW", "-419", "_x", "-"];
        let mut n = 0usize;
        let mut known = 0usize;
        for l in &langs {
            for s in suffixes {
                for tag in [format!("{l}{s}"), format!("{}{s}", l.to_uppercase()), format!(" {l}{s} ")] {
                    for ord in [false, true] {
                        let (o, w) = (old::categories(&tag, ord), categories(&tag, ord));
                        assert_eq!(o, w, "{tag:?} ordinal={ord}");
                        n += 1;
                        known += usize::from(o.is_some());
                    }
                }
            }
        }
        assert!(n > 700_000 && known > 5000, "n={n} known={known}");
    }
}
