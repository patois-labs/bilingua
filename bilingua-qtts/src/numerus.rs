//! Qt's own numerus forms per language (bilingua#23): how many `<numerusform>` elements Qt Linguist and lrelease
//! expect for a language and in which order, from the rule table of qttools (`src/linguist/shared/numerus.cpp`,
//! `getNumerusInfo`). Qt's rules are not CLDR's: Polish and Russian have three forms (no fraction form), Czech and
//! Slovak three, Arabic six, Japanese, Chinese, Korean, Turkish and Hungarian one. A language Qt has no rule for
//! falls back to the CLDR categories whole numbers reach (`cldr::forms`, the count LocOS writes).
//!
//! Each form has a CLDR category as a hint where the form covers the whole numbers of exactly one category; a form
//! that mixes categories (Latvian "Plural", Welsh 2 to 5, Macedonian) has none.

/// The numerus forms of one language: the rule set's name (two languages with the same name number their forms
/// alike) and a category hint per form, in Qt's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forms {
    pub rule: String,
    pub hints: Vec<Option<&'static str>>,
}

impl Forms {
    /// Number of forms.
    pub fn len(&self) -> usize {
        self.hints.len()
    }

    /// The token of form `k` in an added unit's mark (`polish/2`).
    pub fn token(&self, k: usize) -> String {
        format!("{}/{k}", self.rule)
    }

    /// The tokens of every form, the list an added unit's mark names.
    pub fn tokens(&self) -> Vec<String> {
        (0..self.len()).map(|k| self.token(k)).collect()
    }
}

const O: Option<&str> = None;
const fn c(s: &'static str) -> Option<&'static str> {
    Some(s)
}

/// Qt's rule sets: name and hints (the order of `numerus.cpp`'s `...Forms` lists).
const JAPANESE: (&str, &[Option<&str>]) = ("japanese", &[c("other")]);
const ENGLISH: (&str, &[Option<&str>]) = ("english", &[c("one"), c("other")]);
const FRENCH: (&str, &[Option<&str>]) = ("french", &[c("one"), c("other")]);
const ICELANDIC: (&str, &[Option<&str>]) = ("icelandic", &[c("one"), c("other")]);
// Singular, Plural (n != 0), Nullar: CLDR `zero` is also 10 to 20, which Qt puts under Plural
const LATVIAN: (&str, &[Option<&str>]) = ("latvian", &[c("one"), O, O]);
// Singular, Dual, Plural: CLDR Irish has `few` and `many` inside Qt's Plural
const IRISH: (&str, &[Option<&str>]) = ("irish", &[c("one"), c("two"), O]);
const GAELIC: (&str, &[Option<&str>]) = ("gaelic", &[c("one"), c("two"), c("few"), c("other")]);
const SLOVAK: (&str, &[Option<&str>]) = ("slovak", &[c("one"), c("few"), c("other")]);
// n % 10 == 1, n % 10 == 2, else: CLDR Macedonian has one and other, 11 and 12 differ
const MACEDONIAN: (&str, &[Option<&str>]) = ("macedonian", &[O, O, O]);
const LITHUANIAN: (&str, &[Option<&str>]) = ("lithuanian", &[c("one"), c("few"), c("other")]);
// the same rule for every Russian-style language; the last form is CLDR `many` where the language has one
const RUSSIAN: (&str, &[Option<&str>]) = ("russian", &[c("one"), c("few"), c("many")]);
const RUSSIAN_NO_MANY: (&str, &[Option<&str>]) = ("russian", &[c("one"), c("few"), c("other")]);
const POLISH: (&str, &[Option<&str>]) = ("polish", &[c("one"), c("few"), c("many")]);
const ROMANIAN: (&str, &[Option<&str>]) = ("romanian", &[c("one"), c("few"), c("other")]);
const SLOVENIAN: (&str, &[Option<&str>]) = ("slovenian", &[c("one"), c("two"), c("few"), c("other")]);
// Singular, Paucal (0, 2 to 10), Greater Paucal (11 to 19), Plural: CLDR splits Qt's Paucal into two and few
const MALTESE: (&str, &[Option<&str>]) = ("maltese", &[c("one"), O, c("many"), c("other")]);
// Nullar, Singular, Dual (2 to 5), Sexal (6), Plural: CLDR splits Qt's Dual into two and few
const WELSH: (&str, &[Option<&str>]) = ("welsh", &[c("zero"), c("one"), O, c("many"), c("other")]);
const ARABIC: (&str, &[Option<&str>]) = ("arabic", &[c("zero"), c("one"), c("two"), c("few"), c("many"), c("other")]);

type Rule = (&'static str, &'static [Option<&'static str>]);

/// The languages of `numerus.cpp` (qttools dev, checked 2026-11-18), by code (Qt's `QLocale` languages), sorted.
/// Filipino (`fil`, and `tl`, which `QLocale` reads as Filipino) is in both the French-style list and the Tagalog
/// rule; Qt takes the first table entry that matches, the French style. Catalan, Lingala and Occitan are English
/// style.
const LANGS: &[(&str, Rule)] = &[
    ("aa", ENGLISH),
    ("ab", ENGLISH),
    ("af", ENGLISH),
    ("am", ENGLISH),
    ("ar", ARABIC),
    ("as", ENGLISH),
    ("ay", ENGLISH),
    ("az", ENGLISH),
    ("ba", ENGLISH),
    ("be", RUSSIAN),
    ("bg", ENGLISH),
    ("bi", JAPANESE),
    ("bn", ENGLISH),
    ("bo", JAPANESE),
    ("br", FRENCH),
    ("bs", RUSSIAN_NO_MANY),
    ("ca", ENGLISH),
    ("co", ENGLISH),
    ("cs", SLOVAK),
    ("cy", WELSH),
    ("da", ENGLISH),
    ("de", ENGLISH),
    ("dv", IRISH),
    ("dz", JAPANESE),
    ("el", ENGLISH),
    ("en", ENGLISH),
    ("eo", ENGLISH),
    ("es", ENGLISH),
    ("et", ENGLISH),
    ("eu", ENGLISH),
    ("fa", JAPANESE),
    ("fi", ENGLISH),
    ("fil", FRENCH),
    ("fj", JAPANESE),
    ("fo", ENGLISH),
    ("fr", FRENCH),
    ("fur", ENGLISH),
    ("fy", ENGLISH),
    ("ga", IRISH),
    ("gd", GAELIC),
    ("gl", ENGLISH),
    ("gn", JAPANESE),
    ("gu", ENGLISH),
    ("gv", IRISH),
    ("ha", ENGLISH),
    ("he", ENGLISH),
    ("hi", ENGLISH),
    ("hr", RUSSIAN_NO_MANY),
    ("hu", JAPANESE),
    ("hy", FRENCH),
    ("ia", ENGLISH),
    ("id", JAPANESE),
    ("ie", ENGLISH),
    ("ik", IRISH),
    ("is", ICELANDIC),
    ("it", ENGLISH),
    ("iu", IRISH),
    ("ja", JAPANESE),
    ("jv", JAPANESE),
    ("ka", ENGLISH),
    ("kk", ENGLISH),
    ("kl", ENGLISH),
    ("km", ENGLISH),
    ("kn", ENGLISH),
    ("ko", JAPANESE),
    ("ks", ENGLISH),
    ("ku", ENGLISH),
    ("kw", ENGLISH),
    ("ky", ENGLISH),
    ("la", ENGLISH),
    ("lb", ENGLISH),
    ("lg", ENGLISH),
    ("ln", ENGLISH),
    ("lo", ENGLISH),
    ("lt", LITHUANIAN),
    ("lv", LATVIAN),
    ("mg", ENGLISH),
    ("mi", IRISH),
    ("mk", MACEDONIAN),
    ("ml", ENGLISH),
    ("mn", ENGLISH),
    ("mr", ENGLISH),
    ("ms", JAPANESE),
    ("mt", MALTESE),
    ("my", JAPANESE),
    ("na", JAPANESE),
    ("nb", ENGLISH),
    ("ne", ENGLISH),
    ("nl", ENGLISH),
    ("nn", ENGLISH),
    ("no", ENGLISH),
    ("nso", ENGLISH),
    ("oc", ENGLISH),
    ("om", JAPANESE),
    ("or", ENGLISH),
    ("pa", ENGLISH),
    ("pl", POLISH),
    ("ps", ENGLISH),
    ("pt", ENGLISH),
    ("qu", ENGLISH),
    ("rm", ENGLISH),
    ("rn", ENGLISH),
    ("ro", ROMANIAN),
    ("ru", RUSSIAN),
    ("rw", ENGLISH),
    ("sa", IRISH),
    ("sd", ENGLISH),
    ("se", IRISH),
    ("si", ENGLISH),
    ("sk", SLOVAK),
    ("sl", SLOVENIAN),
    ("sm", IRISH),
    ("sn", ENGLISH),
    ("so", ENGLISH),
    ("sq", ENGLISH),
    ("sr", RUSSIAN_NO_MANY),
    ("ss", ENGLISH),
    ("ssy", ENGLISH),
    ("st", ENGLISH),
    ("su", JAPANESE),
    ("sv", ENGLISH),
    ("sw", ENGLISH),
    ("ta", ENGLISH),
    ("te", ENGLISH),
    ("tg", ENGLISH),
    ("th", JAPANESE),
    ("ti", FRENCH),
    ("tk", ENGLISH),
    ("tl", FRENCH),
    ("tn", ENGLISH),
    ("to", ENGLISH),
    ("tr", JAPANESE),
    ("ts", ENGLISH),
    ("tt", JAPANESE),
    ("ug", ENGLISH),
    ("uk", RUSSIAN),
    ("ur", ENGLISH),
    ("uz", ENGLISH),
    ("vi", JAPANESE),
    ("vo", ENGLISH),
    ("wa", FRENCH),
    ("wo", ENGLISH),
    ("xh", ENGLISH),
    ("yi", ENGLISH),
    ("yo", JAPANESE),
    ("za", JAPANESE),
    ("zh", JAPANESE),
    ("zu", ENGLISH),
];

fn qt_rule(lang: &str) -> Option<Rule> {
    LANGS.binary_search_by(|(c, _)| (*c).cmp(lang)).ok().map(|i| LANGS[i].1)
}

/// The numerus forms of a target language tag (`pl`, `pt-BR`, `sr_Latn`), or `None` for a tag neither Qt's table
/// nor CLDR knows.
pub fn forms(tag: &str) -> Option<Forms> {
    let lang = bilingua_model::cldr::language(tag);
    if lang.is_empty() {
        return None;
    }
    // Qt numbers Brazilian Portuguese like French (`n <= 1` is the singular)
    let region = tag.trim().split(['-', '_']).skip(1).find(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()));
    let rule = if lang == "pt" && region.is_some_and(|r| r.eq_ignore_ascii_case("br")) {
        Some(FRENCH)
    } else {
        qt_rule(&lang).or_else(|| qt_rule(bilingua_model::cldr::babel_language(&lang)))
    };
    if let Some((name, hints)) = rule {
        return Some(Forms { rule: name.to_string(), hints: hints.to_vec() });
    }
    // no Qt rule: the CLDR categories whole numbers reach, the forms the merge writes for it
    let cats = crate::cldr::categories(tag)?;
    Some(Forms { rule: format!("cldr-{}", cats.join("-")), hints: cats.into_iter().map(Some).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_sorted() {
        assert!(LANGS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn qt_counts_and_order() {
        let n = |t: &str| forms(t).map(|f| f.len());
        assert_eq!(n("pl"), Some(3));
        assert_eq!(n("ru-RU"), Some(3));
        assert_eq!(n("cs"), Some(3));
        assert_eq!(n("ar"), Some(6));
        assert_eq!(n("en"), Some(2));
        assert_eq!(n("ja"), Some(1));
        assert_eq!(n("hu"), Some(1));
        assert_eq!(n("ga"), Some(3));
        assert_eq!(n("cy"), Some(5));
        assert_eq!(n("sl"), Some(4));
        assert_eq!(forms("pl").unwrap().hints, [Some("one"), Some("few"), Some("many")]);
        assert_eq!(forms("hr").unwrap().hints, [Some("one"), Some("few"), Some("other")]);
        assert_eq!(forms("pt-BR").unwrap().rule, "french");
        assert_eq!(forms("pt-PT").unwrap().rule, "english");
        // babel aliases and three-letter codes
        assert_eq!(forms("pol").unwrap().rule, "polish");
        assert_eq!(forms("iw").unwrap().rule, "english");
        // a language Qt has no rule for takes CLDR's whole-number categories
        let f = forms("ksh").unwrap();
        assert_eq!((f.rule.as_str(), f.hints.as_slice()), ("cldr-zero-one-other", &[Some("zero"), Some("one"), Some("other")][..]));
        assert_eq!(forms(""), None);
        assert_eq!(forms("x1"), None);
        assert_eq!(forms("pl").unwrap().tokens(), ["polish/0", "polish/1", "polish/2"]);
    }
}
