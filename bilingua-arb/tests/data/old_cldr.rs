// The crate's own CLDR table and lookup as they were before the switch to bilingua_model::cldr (item G),
// kept only so a test can compare old and new over every language code. Not compiled outside that test.


pub const ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// The categories the language uses, in CLDR order, always with `other`. `None` for a language the table does
/// not know (callers then leave the plural forms as they are).
pub fn categories(locale: &str, ordinal: bool) -> Option<Vec<&'static str>> {
    let mask = lookup(locale, ordinal)?;
    Some(ORDER.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, c)| *c).collect())
}

fn lookup(locale: &str, ordinal: bool) -> Option<u8> {
    let code = locale.trim().replace('-', "_");
    let mut parts = code.split('_');
    let lang = parts.next()?.to_ascii_lowercase();
    if lang.is_empty() || !lang.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let (mut script, mut terr) = (None, None);
    for p in parts {
        if p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut c = p.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            script = Some(c);
        } else if (p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic())) || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit())) {
            terr = Some(p.to_ascii_uppercase());
        }
    }
    let mut cands = Vec::new();
    if let (Some(s), Some(t)) = (&script, &terr) {
        cands.push(format!("{lang}_{s}_{t}"));
    }
    if let Some(t) = &terr {
        cands.push(format!("{lang}_{t}"));
    }
    if let Some(s) = &script {
        cands.push(format!("{lang}_{s}"));
    }
    cands.push(lang.clone());
    for c in cands {
        if let Ok(i) = TABLE.binary_search_by(|(k, _, _)| (*k).cmp(c.as_str())) {
            let (_, card, ord) = TABLE[i];
            return Some(if ordinal { ord } else { card });
        }
    }
    // Babel also resolves aliases (`deu`, `iw`, `tl`, `und`): LocOS fills plurals for them, so this crate does too.
    let i = ALIASES.binary_search_by(|(k, _, _)| (*k).cmp(lang.as_str())).ok()?;
    let (_, card, ord) = ALIASES[i];
    Some(if ordinal { ord } else { card })
}
// Generated from Babel 2.18.0 (CLDR plural and ordinal categories), the data Localization OS uses.
// (language or language_Script_TERRITORY, cardinal mask, ordinal mask); bit 0 zero, 1 one, 2 two, 3 few, 4 many, 5 other.
pub const BABEL_VERSION: &str = "2.18.0";
pub(crate) static TABLE: &[(&str, u8, u8)] = &[
    ("aa", 32, 32), ("ab", 32, 32), ("af", 34, 32), ("agq", 32, 32), ("ak", 34, 32), ("am", 34, 32), ("an", 34, 32),
    ("ann", 32, 32), ("apc", 32, 32), ("ar", 63, 32), ("arn", 32, 32), ("as", 34, 62), ("asa", 34, 32),
    ("ast", 34, 32), ("az", 34, 58), ("ba", 32, 32), ("bal", 34, 34), ("bas", 32, 32), ("be", 58, 40),
    ("bem", 34, 32), ("bew", 32, 32), ("bez", 34, 32), ("bg", 34, 32), ("bgc", 32, 32), ("bgn", 32, 32),
    ("bho", 34, 32), ("blo", 35, 43), ("blt", 32, 32), ("bm", 32, 32), ("bn", 34, 62), ("bo", 32, 32), ("br", 62, 32),
    ("brx", 34, 32), ("bs", 42, 32), ("bss", 32, 32), ("byn", 32, 32), ("ca", 50, 46), ("cad", 32, 32),
    ("cch", 32, 32), ("ccp", 32, 32), ("ce", 34, 32), ("ceb", 34, 32), ("cgg", 34, 32), ("cho", 32, 32),
    ("chr", 34, 32), ("cic", 32, 32), ("ckb", 34, 32), ("co", 32, 32), ("cop", 32, 32), ("cs", 58, 32),
    ("csw", 34, 32), ("cu", 32, 32), ("cv", 32, 32), ("cy", 63, 63), ("da", 34, 32), ("dav", 32, 32), ("de", 34, 32),
    ("dje", 32, 32), ("doi", 34, 32), ("dsb", 46, 32), ("dua", 32, 32), ("dv", 34, 32), ("dyo", 32, 32),
    ("dz", 32, 32), ("ebu", 32, 32), ("ee", 34, 32), ("el", 34, 32), ("en", 34, 46), ("eo", 34, 32), ("es", 50, 32),
    ("et", 34, 32), ("eu", 34, 32), ("ewo", 32, 32), ("fa", 34, 32), ("ff", 34, 32), ("fi", 34, 32), ("fil", 34, 34),
    ("fo", 34, 32), ("fr", 50, 34), ("frr", 32, 32), ("fur", 34, 32), ("fy", 34, 32), ("ga", 62, 34), ("gaa", 32, 32),
    ("gd", 46, 46), ("gez", 32, 32), ("gl", 34, 32), ("gn", 32, 32), ("gsw", 34, 32), ("gu", 34, 62), ("guz", 32, 32),
    ("gv", 62, 32), ("ha", 34, 32), ("haw", 34, 32), ("he", 38, 32), ("hi", 34, 62), ("hnj", 32, 32), ("hr", 42, 32),
    ("hsb", 46, 32), ("ht", 50, 34), ("hu", 34, 34), ("hy", 34, 34), ("ia", 34, 32), ("id", 32, 32), ("ie", 32, 32),
    ("ig", 32, 32), ("ii", 32, 32), ("io", 34, 32), ("is", 34, 32), ("it", 50, 48), ("iu", 38, 32), ("ja", 32, 32),
    ("jbo", 32, 32), ("jgo", 34, 32), ("jmc", 34, 32), ("jv", 32, 32), ("ka", 34, 50), ("kaa", 32, 32),
    ("kab", 34, 32), ("kaj", 34, 32), ("kam", 32, 32), ("kcg", 34, 32), ("kde", 32, 32), ("kea", 32, 32),
    ("ken", 32, 32), ("kgp", 32, 32), ("khq", 32, 32), ("ki", 32, 32), ("kk", 34, 48), ("kkj", 34, 32),
    ("kl", 34, 32), ("kln", 32, 32), ("km", 32, 32), ("kn", 34, 32), ("ko", 32, 32), ("kok", 32, 32), ("kpe", 32, 32),
    ("ks", 34, 32), ("ksb", 34, 32), ("ksf", 32, 32), ("ksh", 35, 32), ("ku", 34, 32), ("kw", 63, 50),
    ("kxv", 32, 32), ("ky", 34, 32), ("la", 32, 32), ("lag", 35, 32), ("lb", 34, 32), ("lg", 34, 32), ("lij", 34, 48),
    ("lkt", 32, 32), ("lld", 50, 48), ("lmo", 32, 32), ("ln", 34, 32), ("lo", 32, 34), ("lrc", 32, 32),
    ("lt", 58, 32), ("ltg", 32, 32), ("lu", 32, 32), ("luo", 32, 32), ("luy", 32, 32), ("lv", 35, 32),
    ("mai", 32, 32), ("mas", 34, 32), ("mdf", 32, 32), ("mer", 32, 32), ("mfe", 32, 32), ("mg", 34, 32),
    ("mgh", 32, 32), ("mgo", 34, 32), ("mhn", 32, 32), ("mi", 32, 32), ("mic", 32, 32), ("mk", 34, 54),
    ("ml", 34, 32), ("mn", 34, 32), ("mni", 32, 32), ("moh", 32, 32), ("mr", 34, 46), ("ms", 32, 34), ("mt", 62, 32),
    ("mua", 32, 32), ("mus", 32, 32), ("my", 32, 32), ("myv", 32, 32), ("mzn", 32, 32), ("naq", 38, 32),
    ("nb", 34, 32), ("nd", 34, 32), ("nds", 32, 32), ("ne", 34, 34), ("nl", 34, 32), ("nmg", 32, 32), ("nn", 34, 32),
    ("nnh", 34, 32), ("no", 34, 32), ("nqo", 32, 32), ("nr", 34, 32), ("nso", 34, 32), ("nus", 32, 32),
    ("nv", 32, 32), ("ny", 34, 32), ("nyn", 34, 32), ("oc", 32, 32), ("om", 34, 32), ("or", 34, 62), ("os", 34, 32),
    ("osa", 32, 32), ("pa", 34, 32), ("pap", 34, 32), ("pcm", 34, 32), ("pis", 32, 32), ("pl", 58, 32),
    ("prg", 35, 32), ("ps", 34, 32), ("pt", 50, 32), ("qu", 32, 32), ("quc", 32, 32), ("raj", 32, 32),
    ("rhg", 32, 32), ("rif", 32, 32), ("rm", 34, 32), ("rn", 32, 32), ("ro", 42, 34), ("rof", 34, 32), ("ru", 58, 32),
    ("rw", 32, 32), ("rwk", 34, 32), ("sa", 32, 32), ("sah", 32, 32), ("saq", 34, 32), ("sat", 38, 32),
    ("sbp", 32, 32), ("sc", 34, 48), ("scn", 50, 48), ("sd", 34, 32), ("sdh", 34, 32), ("se", 38, 32),
    ("seh", 34, 32), ("ses", 32, 32), ("sg", 32, 32), ("shi", 42, 32), ("shn", 32, 32), ("si", 34, 32),
    ("sid", 32, 32), ("sk", 58, 32), ("skr", 32, 32), ("sl", 46, 32), ("sma", 38, 32), ("smj", 38, 32),
    ("smn", 38, 32), ("sms", 38, 32), ("sn", 34, 32), ("so", 34, 32), ("sq", 34, 50), ("sr", 42, 32), ("ss", 34, 32),
    ("ssy", 34, 32), ("st", 34, 32), ("su", 32, 32), ("sv", 34, 34), ("sw", 34, 32), ("syr", 34, 32), ("szl", 32, 32),
    ("ta", 34, 32), ("te", 34, 32), ("teo", 34, 32), ("tg", 32, 32), ("th", 32, 32), ("ti", 34, 32), ("tig", 34, 32),
    ("tk", 34, 40), ("tn", 34, 32), ("to", 32, 32), ("tok", 32, 32), ("tpi", 32, 32), ("tr", 34, 32), ("trv", 32, 32),
    ("trw", 32, 32), ("ts", 34, 32), ("tt", 32, 32), ("twq", 32, 32), ("tyv", 32, 32), ("tzm", 34, 32),
    ("ug", 34, 32), ("uk", 58, 40), ("ur", 34, 32), ("uz", 34, 32), ("vai", 32, 32), ("ve", 34, 32), ("vec", 50, 48),
    ("vi", 32, 34), ("vmw", 32, 32), ("vo", 34, 32), ("vun", 34, 32), ("wa", 34, 32), ("wae", 34, 32),
    ("wal", 32, 32), ("wbp", 32, 32), ("wo", 32, 32), ("xh", 34, 32), ("xnr", 32, 32), ("xog", 34, 32),
    ("yav", 32, 32), ("yi", 34, 32), ("yo", 32, 32), ("yrl", 32, 32), ("yue", 32, 32), ("za", 32, 32),
    ("zgh", 32, 32), ("zh", 32, 32), ("zu", 34, 32),
];

// Codes Babel 2.18.0 resolves through its language aliases and likely subtags (ISO 639-2 `deu`, legacy `iw`,
// `tl`, `und`, `root`, ...), with the masks of the locale Babel picks; tried when TABLE has no entry.
pub(crate) static ALIASES: &[(&str, u8, u8)] = &[
    ("aar", 32, 32), ("abk", 32, 32), ("adp", 32, 32), ("afr", 34, 32), ("ajp", 32, 32), ("aka", 34, 32),
    ("alb", 34, 50), ("als", 34, 50), ("amh", 34, 32), ("ara", 63, 32), ("arb", 63, 32), ("arg", 34, 32),
    ("arm", 34, 34), ("asm", 34, 62), ("aze", 34, 58), ("azj", 34, 58), ("bak", 32, 32), ("bam", 32, 32),
    ("baq", 34, 32), ("bcc", 34, 34), ("bel", 58, 40), ("ben", 34, 62), ("bh", 34, 32), ("bih", 34, 32),
    ("bod", 32, 32), ("bos", 42, 32), ("bre", 62, 32), ("bul", 34, 32), ("bur", 32, 32), ("bxk", 32, 32),
    ("cat", 50, 46), ("ces", 58, 32), ("che", 34, 32), ("chi", 32, 32), ("chu", 32, 32), ("chv", 32, 32),
    ("cld", 34, 32), ("cls", 32, 32), ("cmn", 32, 32), ("cnr", 42, 32), ("cor", 63, 50), ("cos", 32, 32),
    ("cym", 63, 63), ("cze", 58, 32), ("dan", 34, 32), ("deu", 34, 32), ("dgo", 34, 32), ("div", 34, 32),
    ("drh", 34, 32), ("dut", 34, 32), ("dzo", 32, 32), ("ekk", 34, 32), ("ell", 34, 32), ("eng", 34, 46),
    ("epo", 34, 32), ("est", 34, 32), ("eus", 34, 32), ("ewe", 34, 32), ("fao", 34, 32), ("fas", 34, 32),
    ("fat", 34, 32), ("fin", 34, 32), ("fra", 50, 34), ("fre", 50, 34), ("fry", 34, 32), ("fuc", 34, 32),
    ("ful", 34, 32), ("gaz", 34, 32), ("geo", 34, 50), ("ger", 34, 32), ("gla", 46, 46), ("gle", 62, 34),
    ("glg", 34, 32), ("glv", 62, 32), ("gom", 32, 32), ("gre", 34, 32), ("grn", 32, 32), ("gug", 32, 32),
    ("guj", 34, 62), ("hat", 50, 34), ("hau", 34, 32), ("heb", 38, 32), ("hin", 34, 62), ("hrv", 42, 32),
    ("hun", 34, 34), ("hye", 34, 34), ("ibo", 32, 32), ("ice", 34, 32), ("ido", 34, 32), ("iii", 32, 32),
    ("ike", 38, 32), ("iku", 38, 32), ("ile", 32, 32), ("in", 32, 32), ("ina", 34, 32), ("ind", 32, 32),
    ("isl", 34, 32), ("ita", 50, 48), ("iw", 38, 32), ("jav", 32, 32), ("ji", 34, 32), ("jpn", 32, 32),
    ("jw", 32, 32), ("kal", 34, 32), ("kan", 34, 32), ("kas", 34, 32), ("kat", 34, 50), ("kaz", 34, 48),
    ("khk", 34, 32), ("khm", 32, 32), ("kik", 32, 32), ("kin", 32, 32), ("kir", 34, 32), ("kmr", 34, 32),
    ("kor", 32, 32), ("kur", 34, 32), ("lao", 32, 34), ("lat", 32, 32), ("lav", 35, 32), ("lin", 34, 32),
    ("lit", 58, 32), ("ltz", 34, 32), ("lub", 32, 32), ("lug", 34, 32), ("lvs", 35, 32), ("mac", 34, 54),
    ("mal", 34, 32), ("mao", 32, 32), ("mar", 34, 46), ("may", 32, 34), ("mkd", 34, 54), ("mlg", 34, 32),
    ("mlt", 62, 32), ("mo", 42, 34), ("mol", 42, 34), ("mon", 34, 32), ("mri", 32, 32), ("msa", 32, 34),
    ("mup", 32, 32), ("mya", 32, 32), ("nav", 32, 32), ("nbl", 34, 32), ("nde", 34, 32), ("nep", 34, 34),
    ("nld", 34, 32), ("nno", 34, 32), ("nob", 34, 32), ("nor", 34, 32), ("npi", 34, 34), ("nya", 34, 32),
    ("oci", 32, 32), ("ori", 34, 62), ("orm", 34, 32), ("ory", 34, 62), ("oss", 34, 32), ("pan", 34, 32),
    ("pbu", 34, 32), ("per", 34, 32), ("pes", 34, 32), ("plt", 34, 32), ("pol", 58, 32), ("por", 50, 32),
    ("prp", 34, 62), ("pus", 34, 32), ("que", 32, 32), ("quz", 32, 32), ("roh", 34, 32), ("ron", 42, 34),
    ("root", 32, 32), ("rum", 42, 34), ("run", 32, 32), ("rus", 58, 32), ("sag", 32, 32), ("san", 32, 32),
    ("scc", 42, 32), ("scr", 42, 32), ("sin", 34, 32), ("slk", 58, 32), ("slo", 58, 32), ("slv", 46, 32),
    ("sme", 38, 32), ("sna", 34, 32), ("snd", 34, 32), ("som", 34, 32), ("sot", 34, 32), ("spa", 50, 32),
    ("spy", 32, 32), ("sqi", 34, 50), ("src", 34, 48), ("srd", 34, 48), ("srp", 42, 32), ("ssw", 34, 32),
    ("sun", 32, 32), ("swa", 34, 32), ("swe", 34, 34), ("swh", 34, 32), ("tam", 34, 32), ("tat", 32, 32),
    ("tel", 34, 32), ("tgk", 32, 32), ("tgl", 34, 34), ("tha", 32, 32), ("tib", 32, 32), ("tir", 34, 32),
    ("tl", 34, 34), ("ton", 32, 32), ("tsn", 34, 32), ("tso", 34, 32), ("tuk", 34, 40), ("tur", 34, 32),
    ("tw", 34, 32), ("twi", 34, 32), ("uig", 34, 32), ("ukr", 58, 40), ("und", 34, 46), ("urd", 34, 32),
    ("uzb", 34, 32), ("uzn", 34, 32), ("ven", 34, 32), ("vie", 32, 34), ("vol", 34, 32), ("wel", 63, 63),
    ("wln", 34, 32), ("wol", 32, 32), ("xho", 34, 32), ("xpe", 32, 32), ("ydd", 34, 32), ("yid", 34, 32),
    ("yor", 32, 32), ("zha", 32, 32), ("zho", 32, 32), ("zsm", 32, 34), ("zul", 34, 32), ("zyb", 32, 32),
];
