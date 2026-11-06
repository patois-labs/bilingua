//! CLDR plural and ordinal categories of a target language, from the shared table in `bilingua_model::cldr` (the
//! babel data Localization OS uses), so a target gets exactly the categories LocOS would give it. Babel also
//! resolves old and three-letter language codes (`deu`, `iw`, `tl`, `und`, `root`); LocOS fills plurals for those,
//! so this crate keeps babel's alias list for them (code to language only; the categories come from the table).

use bilingua_model::cldr;

/// The categories the language uses, in CLDR order, always with `other`. `None` for a language the table does
/// not know (callers then leave the plural forms as they are).
pub fn categories(locale: &str, ordinal: bool) -> Option<Vec<&'static str>> {
    let code = locale.trim().replace('-', "_");
    let lang = code.split('_').next()?.to_ascii_lowercase();
    if lang.is_empty() || !lang.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if lang == "root" {
        return Some(vec!["other"]);
    }
    let lang = match ALIASES.binary_search_by(|(k, _)| (*k).cmp(lang.as_str())) {
        Ok(i) if cldr::cardinal_bits(&lang).is_none() => ALIASES[i].1.to_string(),
        _ => lang,
    };
    let card = cldr::cardinal_bits(&lang)?;
    Some(cldr::names(if ordinal { cldr::ordinal_bits(&lang) } else { card }))
}

/// Babel's language aliases (from `babel.core.get_global("language_aliases")` through `Locale.parse`, babel
/// 2.18.0), sorted: the old or three-letter code and the language whose plural rules it takes.
static ALIASES: &[(&str, &str)] = &[
    ("aar", "aa"), ("abk", "ab"), ("adp", "dz"), ("afr", "af"), ("ajp", "apc"), ("aka", "ak"), ("alb", "sq"),
    ("als", "sq"), ("amh", "am"), ("ara", "ar"), ("arb", "ar"), ("arg", "an"), ("arm", "hy"), ("asm", "as"),
    ("aze", "az"), ("azj", "az"), ("bak", "ba"), ("bam", "bm"), ("baq", "eu"), ("bcc", "bal"), ("bel", "be"),
    ("ben", "bn"), ("bh", "bho"), ("bih", "bho"), ("bod", "bo"), ("bos", "bs"), ("bre", "br"), ("bul", "bg"),
    ("bur", "my"), ("bxk", "luy"), ("cat", "ca"), ("ces", "cs"), ("che", "ce"), ("chi", "zh"), ("chu", "cu"),
    ("chv", "cv"), ("cld", "syr"), ("cls", "sa"), ("cmn", "zh"), ("cnr", "sr"), ("cor", "kw"), ("cos", "co"),
    ("cym", "cy"), ("cze", "cs"), ("dan", "da"), ("deu", "de"), ("dgo", "doi"), ("div", "dv"), ("drh", "mn"),
    ("dut", "nl"), ("dzo", "dz"), ("ekk", "et"), ("ell", "el"), ("eng", "en"), ("epo", "eo"), ("est", "et"),
    ("eus", "eu"), ("ewe", "ee"), ("fao", "fo"), ("fas", "fa"), ("fat", "ak"), ("fin", "fi"), ("fra", "fr"),
    ("fre", "fr"), ("fry", "fy"), ("fuc", "ff"), ("ful", "ff"), ("gaz", "om"), ("geo", "ka"), ("ger", "de"),
    ("gla", "gd"), ("gle", "ga"), ("glg", "gl"), ("glv", "gv"), ("gom", "kok"), ("gre", "el"), ("grn", "gn"),
    ("gug", "gn"), ("guj", "gu"), ("hat", "ht"), ("hau", "ha"), ("heb", "he"), ("hin", "hi"), ("hrv", "hr"),
    ("hun", "hu"), ("hye", "hy"), ("ibo", "ig"), ("ice", "is"), ("ido", "io"), ("iii", "ii"), ("ike", "iu"),
    ("iku", "iu"), ("ile", "ie"), ("in", "id"), ("ina", "ia"), ("ind", "id"), ("isl", "is"), ("ita", "it"),
    ("iw", "he"), ("jav", "jv"), ("ji", "yi"), ("jpn", "ja"), ("jw", "jv"), ("kal", "kl"), ("kan", "kn"),
    ("kas", "ks"), ("kat", "ka"), ("kaz", "kk"), ("khk", "mn"), ("khm", "km"), ("kik", "ki"), ("kin", "rw"),
    ("kir", "ky"), ("kmr", "ku"), ("kor", "ko"), ("kur", "ku"), ("lao", "lo"), ("lat", "la"), ("lav", "lv"),
    ("lin", "ln"), ("lit", "lt"), ("ltz", "lb"), ("lub", "lu"), ("lug", "lg"), ("lvs", "lv"), ("mac", "mk"),
    ("mal", "ml"), ("mao", "mi"), ("mar", "mr"), ("may", "ms"), ("mkd", "mk"), ("mlg", "mg"), ("mlt", "mt"),
    ("mo", "ro"), ("mol", "ro"), ("mon", "mn"), ("mri", "mi"), ("msa", "ms"), ("mup", "raj"), ("mya", "my"),
    ("nav", "nv"), ("nbl", "nr"), ("nde", "nd"), ("nep", "ne"), ("nld", "nl"), ("nno", "nn"), ("nob", "nb"),
    ("nor", "no"), ("npi", "ne"), ("nya", "ny"), ("oci", "oc"), ("ori", "or"), ("orm", "om"), ("ory", "or"),
    ("oss", "os"), ("pan", "pa"), ("pbu", "ps"), ("per", "fa"), ("pes", "fa"), ("plt", "mg"), ("pol", "pl"),
    ("por", "pt"), ("prp", "gu"), ("pus", "ps"), ("que", "qu"), ("quz", "qu"), ("roh", "rm"), ("ron", "ro"),
    ("rum", "ro"), ("run", "rn"), ("rus", "ru"), ("sag", "sg"), ("san", "sa"), ("scc", "sr"), ("scr", "hr"),
    ("sin", "si"), ("slk", "sk"), ("slo", "sk"), ("slv", "sl"), ("sme", "se"), ("sna", "sn"), ("snd", "sd"),
    ("som", "so"), ("sot", "st"), ("spa", "es"), ("spy", "kln"), ("sqi", "sq"), ("src", "sc"), ("srd", "sc"),
    ("srp", "sr"), ("ssw", "ss"), ("sun", "su"), ("swa", "sw"), ("swe", "sv"), ("swh", "sw"), ("tam", "ta"),
    ("tat", "tt"), ("tel", "te"), ("tgk", "tg"), ("tgl", "fil"), ("tha", "th"), ("tib", "bo"), ("tir", "ti"),
    ("tl", "fil"), ("ton", "to"), ("tsn", "tn"), ("tso", "ts"), ("tuk", "tk"), ("tur", "tr"), ("tw", "ak"),
    ("twi", "ak"), ("uig", "ug"), ("ukr", "uk"), ("und", "en"), ("urd", "ur"), ("uzb", "uz"), ("uzn", "uz"),
    ("ven", "ve"), ("vie", "vi"), ("vol", "vo"), ("wel", "cy"), ("wln", "wa"), ("wol", "wo"), ("xho", "xh"),
    ("xpe", "kpe"), ("ydd", "yi"), ("yid", "yi"), ("yor", "yo"), ("zha", "za"), ("zho", "zh"), ("zsm", "ms"),
    ("zul", "zu"), ("zyb", "za"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_sorted_and_resolve() {
        assert!(ALIASES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(ALIASES.iter().all(|(_, l)| cldr::cardinal_bits(l).is_some()));
        assert_eq!(categories("pl-PL", false).unwrap(), ["one", "few", "many", "other"]);
        assert_eq!(categories("deu", false), categories("de", false));
        assert_eq!(categories("und", true).unwrap(), ["one", "two", "few", "other"]);
        assert_eq!(categories("root", false).unwrap(), ["other"]);
        assert_eq!(categories("en_US", true).unwrap(), ["one", "two", "few", "other"]);
        assert!(categories("xx", false).is_none());
        assert!(categories("", false).is_none());
    }
}
