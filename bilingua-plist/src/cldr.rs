//! CLDR cardinal plural categories per language (the data Localization OS reads through babel), from the shared
//! table in `bilingua_model::cldr`, used to fill the forms a `.stringsdict` lacks when it is merged for a target
//! language.

/// The plural categories a language uses, always with `other`, in CLDR order. Only the language part of the tag
/// counts; `None` when the language is unknown (the forms then stay as they are). The old codes babel still
/// resolves (`iw`, `tl`, ...) count as the language they stand for.
pub(crate) fn categories(locale: &str) -> Option<Vec<&'static str>> {
    let lang = bilingua_model::cldr::language(locale);
    bilingua_model::cldr::cardinal_bits_aliased(&lang).map(bilingua_model::cldr::names)
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
    fn known_languages_match_cldr() {
        assert_eq!(categories("ru").unwrap(), ["one", "few", "many", "other"]);
        assert_eq!(categories("ja").unwrap(), ["other"]);
        assert_eq!(categories("ar-EG").unwrap(), ["zero", "one", "two", "few", "many", "other"]);
        assert_eq!(categories("pl").unwrap(), ["one", "few", "many", "other"]);
        assert_eq!(categories("en_US").unwrap(), ["one", "other"]);
        assert_eq!(categories("zh-Hant").unwrap(), ["other"]);
        assert_eq!(categories("ko").unwrap(), ["other"]);
        assert_eq!(categories("es").unwrap(), ["one", "many", "other"]);
        // old codes babel still resolves
        assert_eq!(categories("iw").unwrap(), ["one", "two", "other"]);
        assert_eq!(categories("tl-PH").unwrap(), ["one", "other"]);
        assert!(categories("xx").is_none() && categories("").is_none());
    }

    /// Every code of one to four letters (and some malformed ones), bare and with a region or script: the shared
    /// table answers exactly as the crate's own table did.
    #[test]
    fn shared_table_answers_like_the_old_private_one() {
        let mut langs: Vec<String> = vec!["".into(), " ".into(), "x1".into(), "root".into(), "und".into(), "abcde".into()];
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
                    let (o, w) = (old::categories(&tag), categories(&tag));
                    assert_eq!(o, w, "{tag:?}");
                    n += 1;
                    known += usize::from(o.is_some());
                }
            }
        }
        assert!(n > 250_000 && known > 2000, "n={n} known={known}");
    }
}
