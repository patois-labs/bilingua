//! Word counting.
//!
//! - A word is a run of letters and digits (and combining marks, such as Arabic diacritics).
//! - An apostrophe or hyphen between two letters or digits stays inside the word (`don't`, `well-known`);
//!   a dot or comma between two digits stays inside the number (`1.9`, `1,000`).
//! - Han ideographs, Hiragana and Katakana count one word per character (these languages write no spaces).
//!   The prolonged sound mark and the middle dot are not counted. Hangul is written with spaces, so it is
//!   counted by space-separated word like any other script.

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F // Han
        | 0x3041..=0x309F // Hiragana
        | 0x30A1..=0x30FA | 0x30FD..=0x30FF // Katakana without the middle dot and long mark
        | 0x31F0..=0x31FF | 0xFF66..=0xFF9F) // Katakana extensions and half-width
}

fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E8 | 0x0E31..=0x0E3A | 0x3099..=0x309A | 0x200D)
}

fn is_word_char(c: char) -> bool {
    !is_cjk(c) && (c.is_alphanumeric() || is_mark(c))
}

pub fn count_words(s: &str) -> usize {
    let cs: Vec<char> = s.chars().collect();
    let mut n = 0usize;
    let mut in_word = false;
    for (i, &c) in cs.iter().enumerate() {
        if c == '\u{30fc}' || c == '\u{30fb}' {
            continue; // prolonged sound mark and middle dot: part of a katakana word, not a word
        }
        if is_cjk(c) {
            n += 1;
            in_word = false;
        } else if is_word_char(c) {
            if !in_word {
                n += 1;
                in_word = true;
            }
        } else if in_word {
            let prev = cs[i - 1];
            let next = cs.get(i + 1).copied().unwrap_or(' ');
            let inner = match c {
                '\'' | '\u{2019}' | '-' | '\u{2011}' => is_word_char(prev) && is_word_char(next),
                '.' | ',' => prev.is_ascii_digit() && next.is_ascii_digit(),
                _ => false,
            };
            if !inner {
                in_word = false;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_by_language() {
        assert_eq!(count_words("Welcome to Larchmoor."), 3);
        assert_eq!(count_words("Don't use well-known 1.9 or 1,000 items"), 7);
        assert_eq!(count_words("\u{bf}C\u{f3}mo est\u{e1}s? \u{a1}Muy bien!"), 4);
        assert_eq!(count_words("l'eau et c'est la vie"), 5);
        assert_eq!(count_words("Die Flut ist \u{fc}ber."), 4);
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("  ...  "), 0);
        // Japanese and Chinese: one per character, punctuation not counted
        assert_eq!(count_words("\u{6f6e}\u{304c}\u{5f15}\u{3044}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}"), 8);
        assert_eq!(count_words("\u{6b22}\u{8fce}\u{6765}\u{5230}\u{62c9}\u{5947}\u{83ab}\u{5c14}\u{3002}"), 8);
        // katakana long mark and middle dot do not count; Latin inside Japanese counts as a word
        assert_eq!(count_words("\u{30e9}\u{30fc}\u{30e1}\u{30f3}"), 3);
        assert_eq!(count_words("Wi-Fi\u{3092}\u{4f7f}\u{3046}"), 4);
        // Korean by space-separated word
        assert_eq!(count_words("\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} \u{c800}\u{b294} \u{bbfc}\u{c218}\u{c785}\u{b2c8}\u{b2e4}."), 3);
        // Arabic, with and without diacritics
        assert_eq!(count_words("\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{644}\u{627}\u{631}\u{62a}\u{634}\u{645}\u{648}\u{631}\u{61f}"), 4);
        assert_eq!(count_words("\u{645}\u{64e}\u{631}\u{652}\u{62d}\u{64e}\u{628}\u{627}\u{b9} \u{628}\u{643}\u{645}"), 2);
    }
}
