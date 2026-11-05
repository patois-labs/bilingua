//! Tech 3264 Appendix 3 language codes (GSI LC), keyed by ISO 639 code, as Localization OS has them.

const CODES: &[(&str, u8)] = &[
    ("sq", 0x01), ("br", 0x02), ("ca", 0x03), ("hr", 0x04), ("cy", 0x05), ("cs", 0x06), ("da", 0x07),
    ("de", 0x08), ("en", 0x09), ("es", 0x0A), ("eo", 0x0B), ("et", 0x0C), ("eu", 0x0D), ("fo", 0x0E),
    ("fr", 0x0F), ("fy", 0x10), ("ga", 0x11), ("gd", 0x12), ("gl", 0x13), ("is", 0x14), ("it", 0x15),
    ("se", 0x16), ("la", 0x17), ("lv", 0x18), ("lb", 0x19), ("lt", 0x1A), ("hu", 0x1B), ("mt", 0x1C),
    ("nl", 0x1D), ("no", 0x1E), ("nb", 0x1E), ("nn", 0x1E), ("oc", 0x1F), ("pl", 0x20), ("pt", 0x21),
    ("ro", 0x22), ("rm", 0x23), ("sr", 0x24), ("sk", 0x25), ("sl", 0x26), ("fi", 0x27), ("sv", 0x28),
    ("tr", 0x29), ("wa", 0x2B),
    ("zu", 0x45), ("vi", 0x46), ("uz", 0x47), ("ur", 0x48), ("uk", 0x49), ("th", 0x4A), ("te", 0x4B),
    ("tt", 0x4C), ("ta", 0x4D), ("tg", 0x4E), ("sw", 0x4F), ("srn", 0x50), ("so", 0x51), ("si", 0x52),
    ("sn", 0x53), ("sh", 0x54), ("rue", 0x55), ("ru", 0x56), ("qu", 0x57), ("ps", 0x58), ("pa", 0x59),
    ("fa", 0x5A), ("pap", 0x5B), ("or", 0x5C), ("ne", 0x5D), ("nd", 0x5E), ("mr", 0x5F), ("ms", 0x61),
    ("mg", 0x62), ("mk", 0x63), ("lo", 0x64), ("ko", 0x65), ("km", 0x66), ("kk", 0x67), ("kn", 0x68),
    ("ja", 0x69), ("id", 0x6A), ("hi", 0x6B), ("he", 0x6C), ("ha", 0x6D), ("gn", 0x6E), ("gu", 0x6F),
    ("el", 0x70), ("ka", 0x71), ("ff", 0x72), ("prs", 0x73), ("cv", 0x74), ("zh", 0x75), ("my", 0x76),
    ("bg", 0x77), ("bn", 0x78), ("be", 0x79), ("bm", 0x7A), ("az", 0x7B), ("as", 0x7C), ("hy", 0x7D),
    ("ar", 0x7E), ("am", 0x7F),
];

/// The LC byte value for a BCP 47 locale (00 = unknown / not applicable).
pub fn language_code(locale: &str) -> u8 {
    let low = locale.replace('_', "-").to_lowercase();
    let parts: Vec<&str> = low.split('-').collect();
    let mut lang = parts[0];
    if lang == "nl" && parts[1..].contains(&"be") {
        return 0x2A; // Flemish
    }
    if lang == "iw" {
        lang = "he";
    }
    CODES.iter().find(|(l, _)| *l == lang).map_or(0, |(_, c)| *c)
}
