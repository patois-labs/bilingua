//! CLDR plural categories per language (generated from the CLDR data the LocOS filter reads through babel).

pub const ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

const TABLE: &[(&[&str], &str)] = &[
    (&["other"], "aa ab agq ann apc arn ba bas bew bgc bgn blt bm bo bss byn cad cch ccp cho cic co cop cu cv dav dje dua dyo dz ebu ewo frr gaa gez gn guz hnj id ie ig ii ja jbo jv kaa kam kde kea ken kgp khq ki kln km ko kok kpe ksf kxv la lkt lmo lo lrc ltg lu luo luy mai mdf mer mfe mgh mhn mi mic mni moh ms mua mus my myv mzn nds nmg nqo nus nv oc osa pis qu quc raj rhg rif rn rw sa sah sbp ses sg shn sid skr su szl tg th to tok tpi trv trw tt twq tyv vai vi vmw wal wbp wo xnr yav yo yrl yue za zgh zh"),
    (&["one", "other"], "af ak am an as asa ast az bal bem bez bg bho bn brx ce ceb cgg chr ckb csw da de doi dv ee el en eo et eu fa ff fi fil fo fur fy gl gsw gu ha haw hi hu hy ia io is jgo jmc ka kab kaj kcg kk kkj kl kn ks ksb ku ky lb lg lij ln mas mg mgo mk ml mn mr nb nd ne nl nn nnh no nr nso ny nyn om or os pa pap pcm ps rm rof rwk saq sc sd sdh seh si sn so sq ss ssy st sv sw syr ta te teo ti tig tk tn tr ts tzm ug ur uz ve vo vun wa wae xh xog yi zu"),
    (&["zero", "one", "two", "few", "many", "other"], "ar cy kw"),
    (&["one", "few", "many", "other"], "be cs lt pl ru sk uk"),
    (&["zero", "one", "other"], "blo ksh lag lv prg"),
    (&["one", "two", "few", "many", "other"], "br ga gv mt"),
    (&["one", "few", "other"], "bs hr ro shi sr"),
    (&["one", "many", "other"], "ca es fr ht it lld pt scn vec"),
    (&["one", "two", "few", "other"], "dsb gd hsb sl"),
    (&["one", "two", "other"], "he iu naq sat se sma smj smn sms"),
];

/// The CLDR plural categories a language uses, `other` always included, in CLDR order. The language is the
/// part of the tag before the first `-` or `_`. `None` when the language is not known (callers then leave the
/// forms as they are).
pub fn categories(locale: &str) -> Option<Vec<&'static str>> {
    let code = locale.trim().replace('-', "_");
    let lang = code.split('_').next()?.to_ascii_lowercase();
    if lang.is_empty() {
        return None;
    }
    TABLE.iter().find(|(_, langs)| langs.split(' ').any(|l| l == lang)).map(|(cats, _)| cats.to_vec())
}
