// The crate's own CLDR table and lookup as they were before the switch to bilingua_model::cldr (item G),
// kept only so a test can compare old and new over every language code. Not compiled outside that test.


/// Number of plural forms for `locale`, or 2 (`one`, `other`) for an unknown or empty locale, as LocOS.
pub fn forms(locale: &str) -> usize {
    lookup(locale).map(|m| m.count_ones() as usize).unwrap_or(2)
}

fn lookup(locale: &str) -> Option<u8> {
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
        if let Ok(i) = TABLE.binary_search_by(|(k, _)| (*k).cmp(c.as_str())) {
            return Some(TABLE[i].1);
        }
    }
    let i = ALIASES.binary_search_by(|(k, _)| (*k).cmp(lang.as_str())).ok()?;
    Some(ALIASES[i].1)
}
// Generated from Babel 2.18.0 by tests/tools/gen_cldr.py.
// (language or language_Script_TERRITORY, categories whole numbers 0..=1000 reach); bit 0 zero, 1 one, 2 two, 3 few, 4 many, 5 other.
pub const BABEL_VERSION: &str = "2.18.0";
pub(crate) static TABLE: &[(&str, u8)] = &[
    ("aa", 32), ("ab", 32), ("af", 34), ("agq", 32), ("ak", 34), ("am", 34), ("an", 34), ("ann", 32), ("apc", 32),
    ("ar", 63), ("arn", 32), ("as", 34), ("asa", 34), ("ast", 34), ("az", 34), ("ba", 32), ("bal", 34), ("bas", 32),
    ("be", 26), ("bem", 34), ("bew", 32), ("bez", 34), ("bg", 34), ("bgc", 32), ("bgn", 32), ("bho", 34), ("blo", 35),
    ("blt", 32), ("bm", 32), ("bn", 34), ("bo", 32), ("br", 46), ("brx", 34), ("bs", 42), ("bss", 32), ("byn", 32),
    ("ca", 34), ("cad", 32), ("cch", 32), ("ccp", 32), ("ce", 34), ("ceb", 34), ("cgg", 34), ("cho", 32), ("chr", 34),
    ("cic", 32), ("ckb", 34), ("co", 32), ("cop", 32), ("cs", 42), ("csw", 34), ("cu", 32), ("cv", 32), ("cy", 63),
    ("da", 34), ("dav", 32), ("de", 34), ("dje", 32), ("doi", 34), ("dsb", 46), ("dua", 32), ("dv", 34), ("dyo", 32),
    ("dz", 32), ("ebu", 32), ("ee", 34), ("el", 34), ("en", 34), ("eo", 34), ("es", 34), ("et", 34), ("eu", 34),
    ("ewo", 32), ("fa", 34), ("ff", 34), ("fi", 34), ("fil", 34), ("fo", 34), ("fr", 34), ("frr", 32), ("fur", 34),
    ("fy", 34), ("ga", 62), ("gaa", 32), ("gd", 46), ("gez", 32), ("gl", 34), ("gn", 32), ("gsw", 34), ("gu", 34),
    ("guz", 32), ("gv", 46), ("ha", 34), ("haw", 34), ("he", 38), ("hi", 34), ("hnj", 32), ("hr", 42), ("hsb", 46),
    ("ht", 34), ("hu", 34), ("hy", 34), ("ia", 34), ("id", 32), ("ie", 32), ("ig", 32), ("ii", 32), ("io", 34),
    ("is", 34), ("it", 34), ("iu", 38), ("ja", 32), ("jbo", 32), ("jgo", 34), ("jmc", 34), ("jv", 32), ("ka", 34),
    ("kaa", 32), ("kab", 34), ("kaj", 34), ("kam", 32), ("kcg", 34), ("kde", 32), ("kea", 32), ("ken", 32),
    ("kgp", 32), ("khq", 32), ("ki", 32), ("kk", 34), ("kkj", 34), ("kl", 34), ("kln", 32), ("km", 32), ("kn", 34),
    ("ko", 32), ("kok", 32), ("kpe", 32), ("ks", 34), ("ksb", 34), ("ksf", 32), ("ksh", 35), ("ku", 34), ("kw", 63),
    ("kxv", 32), ("ky", 34), ("la", 32), ("lag", 35), ("lb", 34), ("lg", 34), ("lij", 34), ("lkt", 32), ("lld", 34),
    ("lmo", 32), ("ln", 34), ("lo", 32), ("lrc", 32), ("lt", 42), ("ltg", 32), ("lu", 32), ("luo", 32), ("luy", 32),
    ("lv", 35), ("mai", 32), ("mas", 34), ("mdf", 32), ("mer", 32), ("mfe", 32), ("mg", 34), ("mgh", 32), ("mgo", 34),
    ("mhn", 32), ("mi", 32), ("mic", 32), ("mk", 34), ("ml", 34), ("mn", 34), ("mni", 32), ("moh", 32), ("mr", 34),
    ("ms", 32), ("mt", 62), ("mua", 32), ("mus", 32), ("my", 32), ("myv", 32), ("mzn", 32), ("naq", 38), ("nb", 34),
    ("nd", 34), ("nds", 32), ("ne", 34), ("nl", 34), ("nmg", 32), ("nn", 34), ("nnh", 34), ("no", 34), ("nqo", 32),
    ("nr", 34), ("nso", 34), ("nus", 32), ("nv", 32), ("ny", 34), ("nyn", 34), ("oc", 32), ("om", 34), ("or", 34),
    ("os", 34), ("osa", 32), ("pa", 34), ("pap", 34), ("pcm", 34), ("pis", 32), ("pl", 26), ("prg", 35), ("ps", 34),
    ("pt", 34), ("qu", 32), ("quc", 32), ("raj", 32), ("rhg", 32), ("rif", 32), ("rm", 34), ("rn", 32), ("ro", 42),
    ("rof", 34), ("ru", 26), ("rw", 32), ("rwk", 34), ("sa", 32), ("sah", 32), ("saq", 34), ("sat", 38), ("sbp", 32),
    ("sc", 34), ("scn", 34), ("sd", 34), ("sdh", 34), ("se", 38), ("seh", 34), ("ses", 32), ("sg", 32), ("shi", 42),
    ("shn", 32), ("si", 34), ("sid", 32), ("sk", 42), ("skr", 32), ("sl", 46), ("sma", 38), ("smj", 38), ("smn", 38),
    ("sms", 38), ("sn", 34), ("so", 34), ("sq", 34), ("sr", 42), ("ss", 34), ("ssy", 34), ("st", 34), ("su", 32),
    ("sv", 34), ("sw", 34), ("syr", 34), ("szl", 32), ("ta", 34), ("te", 34), ("teo", 34), ("tg", 32), ("th", 32),
    ("ti", 34), ("tig", 34), ("tk", 34), ("tn", 34), ("to", 32), ("tok", 32), ("tpi", 32), ("tr", 34), ("trv", 32),
    ("trw", 32), ("ts", 34), ("tt", 32), ("twq", 32), ("tyv", 32), ("tzm", 34), ("ug", 34), ("uk", 26), ("ur", 34),
    ("uz", 34), ("vai", 32), ("ve", 34), ("vec", 34), ("vi", 32), ("vmw", 32), ("vo", 34), ("vun", 34), ("wa", 34),
    ("wae", 34), ("wal", 32), ("wbp", 32), ("wo", 32), ("xh", 34), ("xnr", 32), ("xog", 34), ("yav", 32), ("yi", 34),
    ("yo", 32), ("yrl", 32), ("yue", 32), ("za", 32), ("zgh", 32), ("zh", 32), ("zu", 34),
];
pub(crate) static ALIASES: &[(&str, u8)] = &[
    ("aar", 32), ("abk", 32), ("adp", 32), ("afr", 34), ("ajp", 32), ("aka", 34), ("alb", 34), ("als", 34),
    ("amh", 34), ("ara", 63), ("arb", 63), ("arg", 34), ("arm", 34), ("asm", 34), ("aze", 34), ("azj", 34),
    ("bak", 32), ("bam", 32), ("baq", 34), ("bcc", 34), ("bel", 26), ("ben", 34), ("bh", 34), ("bih", 34),
    ("bod", 32), ("bos", 42), ("bre", 46), ("bul", 34), ("bur", 32), ("bxk", 32), ("cat", 34), ("ces", 42),
    ("che", 34), ("chi", 32), ("chu", 32), ("chv", 32), ("cld", 34), ("cls", 32), ("cmn", 32), ("cnr", 42),
    ("cor", 63), ("cos", 32), ("cym", 63), ("cze", 42), ("dan", 34), ("deu", 34), ("dgo", 34), ("div", 34),
    ("drh", 34), ("dut", 34), ("dzo", 32), ("ekk", 34), ("ell", 34), ("eng", 34), ("epo", 34), ("est", 34),
    ("eus", 34), ("ewe", 34), ("fao", 34), ("fas", 34), ("fat", 34), ("fin", 34), ("fra", 34), ("fre", 34),
    ("fry", 34), ("fuc", 34), ("ful", 34), ("gaz", 34), ("geo", 34), ("ger", 34), ("gla", 46), ("gle", 62),
    ("glg", 34), ("glv", 46), ("gom", 32), ("gre", 34), ("grn", 32), ("gug", 32), ("guj", 34), ("hat", 34),
    ("hau", 34), ("heb", 38), ("hin", 34), ("hrv", 42), ("hun", 34), ("hye", 34), ("ibo", 32), ("ice", 34),
    ("ido", 34), ("iii", 32), ("ike", 38), ("iku", 38), ("ile", 32), ("in", 32), ("ina", 34), ("ind", 32),
    ("isl", 34), ("ita", 34), ("iw", 38), ("jav", 32), ("ji", 34), ("jpn", 32), ("jw", 32), ("kal", 34), ("kan", 34),
    ("kas", 34), ("kat", 34), ("kaz", 34), ("khk", 34), ("khm", 32), ("kik", 32), ("kin", 32), ("kir", 34),
    ("kmr", 34), ("kor", 32), ("kur", 34), ("lao", 32), ("lat", 32), ("lav", 35), ("lin", 34), ("lit", 42),
    ("ltz", 34), ("lub", 32), ("lug", 34), ("lvs", 35), ("mac", 34), ("mal", 34), ("mao", 32), ("mar", 34),
    ("may", 32), ("mkd", 34), ("mlg", 34), ("mlt", 62), ("mo", 42), ("mol", 42), ("mon", 34), ("mri", 32),
    ("msa", 32), ("mup", 32), ("mya", 32), ("nav", 32), ("nbl", 34), ("nde", 34), ("nep", 34), ("nld", 34),
    ("nno", 34), ("nob", 34), ("nor", 34), ("npi", 34), ("nya", 34), ("oci", 32), ("ori", 34), ("orm", 34),
    ("ory", 34), ("oss", 34), ("pan", 34), ("pbu", 34), ("per", 34), ("pes", 34), ("plt", 34), ("pol", 26),
    ("por", 34), ("prp", 34), ("pus", 34), ("que", 32), ("quz", 32), ("roh", 34), ("ron", 42), ("root", 32),
    ("rum", 42), ("run", 32), ("rus", 26), ("sag", 32), ("san", 32), ("scc", 42), ("scr", 42), ("sin", 34),
    ("slk", 42), ("slo", 42), ("slv", 46), ("sme", 38), ("sna", 34), ("snd", 34), ("som", 34), ("sot", 34),
    ("spa", 34), ("spy", 32), ("sqi", 34), ("src", 34), ("srd", 34), ("srp", 42), ("ssw", 34), ("sun", 32),
    ("swa", 34), ("swe", 34), ("swh", 34), ("tam", 34), ("tat", 32), ("tel", 34), ("tgk", 32), ("tgl", 34),
    ("tha", 32), ("tib", 32), ("tir", 34), ("tl", 34), ("ton", 32), ("tsn", 34), ("tso", 34), ("tuk", 34),
    ("tur", 34), ("tw", 34), ("twi", 34), ("uig", 34), ("ukr", 26), ("und", 34), ("urd", 34), ("uzb", 34),
    ("uzn", 34), ("ven", 34), ("vie", 32), ("vol", 34), ("wel", 63), ("wln", 34), ("wol", 32), ("xho", 34),
    ("xpe", 32), ("ydd", 34), ("yid", 34), ("yor", 32), ("zha", 32), ("zho", 32), ("zsm", 32), ("zul", 34),
    ("zyb", 32),
];
