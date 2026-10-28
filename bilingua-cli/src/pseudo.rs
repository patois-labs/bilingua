//! Pseudo-translation: accented letters, about 30% longer, wrapped in brackets, tags kept.

fn accent(c: char) -> char {
    let lower = match c.to_ascii_lowercase() {
        'a' => '\u{e1}',
        'b' => '\u{180}',
        'c' => '\u{e7}',
        'd' => '\u{10f}',
        'e' => '\u{e9}',
        'f' => '\u{192}',
        'g' => '\u{11d}',
        'h' => '\u{125}',
        'i' => '\u{ed}',
        'j' => '\u{135}',
        'k' => '\u{137}',
        'l' => '\u{13a}',
        'm' => '\u{1e41}',
        'n' => '\u{f1}',
        'o' => '\u{f6}',
        'p' => '\u{fe}',
        'q' => '\u{2a0}',
        'r' => '\u{155}',
        's' => '\u{161}',
        't' => '\u{163}',
        'u' => '\u{fc}',
        'v' => '\u{1e7d}',
        'w' => '\u{175}',
        'x' => '\u{1e8b}',
        'y' => '\u{fd}',
        'z' => '\u{17e}',
        _ => return c,
    };
    if c.is_ascii_uppercase() {
        lower.to_uppercase().next().unwrap_or(lower)
    } else {
        lower
    }
}

/// Maps the letters of `s` but leaves placeholders alone: `{...}`, `%s`-style codes and `<...>` markup.
pub fn accent_text(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() * 2);
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            '{' | '<' => {
                let close = if c == '{' { '}' } else { '>' };
                if let Some(j) = cs[i..].iter().position(|x| *x == close) {
                    out.extend(&cs[i..=i + j]);
                    i += j + 1;
                    continue;
                }
                out.push(c);
            }
            '%' => {
                // %s, %d, %1$d, %.2f, %%
                let mut j = i + 1;
                while j < cs.len() && j < i + 8 && (cs[j].is_ascii_digit() || matches!(cs[j], '$' | '.' | '-' | '+' | '#')) {
                    j += 1;
                }
                if j < cs.len() && (matches!(cs[j], 's' | 'd' | 'i' | 'f' | 'u' | 'x' | 'X' | 'o' | 'e' | 'E' | 'g' | 'G' | 'c' | 'p' | '@' | '%')) {
                    out.extend(&cs[i..=j]);
                    i = j + 1;
                    continue;
                }
                out.push(c);
            }
            c => out.push(accent(c)),
        }
        i += 1;
    }
    out
}

/// How many padding characters make `chars` about 30% longer.
pub fn padding(chars: usize) -> usize {
    (chars * 3).div_ceil(10).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_letters_and_keeps_placeholders() {
        assert_eq!(accent_text("Hello"), "\u{124}\u{e9}\u{13a}\u{13a}\u{f6}");
        assert_eq!(accent_text("Hi {name}, %s and %1$d %% <b>x</b>"), "\u{124}\u{ed} {name}, %s \u{e1}\u{f1}\u{10f} %1$d %% <b>\u{1e8b}</b>");
        assert_eq!(accent_text("\u{4e2d}\u{6587} 123 \u{645}\u{631}\u{62d}\u{628}\u{627}"), "\u{4e2d}\u{6587} 123 \u{645}\u{631}\u{62d}\u{628}\u{627}");
        assert_eq!(accent_text("100% sure {open"), "100% \u{161}\u{fc}\u{155}\u{e9} {\u{f6}\u{fe}\u{e9}\u{f1}");
    }

    #[test]
    fn padding_is_about_thirty_percent() {
        assert_eq!(padding(10), 3);
        assert_eq!(padding(1), 1);
        assert_eq!(padding(100), 30);
    }
}
