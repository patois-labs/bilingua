//! Bilingua segmentation: split units into sentences with SRX 2.0 rules, and join or split segments by hand.
//!
//! ```
//! use bilingua_model::*;
//! use bilingua_seg::{segment, Rules};
//!
//! let mut doc = Document {
//!     format: "txt".into(), name: None, source_lang: None, target_lang: None,
//!     granularity: Granularity::Sentence,
//!     units: vec![Unit::new("p1", vec![Inline::text("Hello there. How are you?")])],
//!     skeleton: Skeleton::Text { parts: vec![SkelPart::Unit(0)], encoding: "utf-8".into(), parser: None },
//!     warnings: vec![],
//! };
//! segment(&mut doc, "en", Granularity::Sentence, Rules::bundled());
//! assert_eq!(doc.units[0].segments.len(), 2);
//! assert_eq!(doc.units[0].segments[1].id, "p1#2");
//! ```

mod rules;

pub use rules::{Rules, SrxError};

use bilingua_model::*;
use std::collections::HashMap;

/// One character or one inline tag, so positions can be reasoned about without touching tag contents.
#[derive(Debug, Clone)]
enum Atom {
    Ch(char),
    Tag(Inline),
}

fn atoms(v: &[Inline]) -> Vec<Atom> {
    let mut out = Vec::new();
    for i in v {
        match i {
            Inline::Text(t) => out.extend(t.chars().map(Atom::Ch)),
            tag => out.push(Atom::Tag(tag.clone())),
        }
    }
    out
}

fn inlines(a: &[Atom]) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut buf = String::new();
    for x in a {
        match x {
            Atom::Ch(c) => buf.push(*c),
            Atom::Tag(t) => {
                if !buf.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut buf)));
                }
                out.push(t.clone());
            }
        }
    }
    if !buf.is_empty() {
        out.push(Inline::Text(buf));
    }
    out
}

fn text_of(a: &[Atom]) -> String {
    a.iter()
        .filter_map(|x| match x {
            Atom::Ch(c) => Some(*c),
            _ => None,
        })
        .collect()
}

fn is_ws(a: &Atom) -> bool {
    matches!(a, Atom::Ch(c) if c.is_whitespace())
}

/// Open/close pairs as (open index, close index) over `atoms`. A tag without a partner is not a pair.
fn pairs(a: &[Atom]) -> Vec<(usize, usize)> {
    let mut open: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut out = Vec::new();
    for (i, x) in a.iter().enumerate() {
        if let Atom::Tag(Inline::Tag { id, kind, .. }) = x {
            match kind {
                TagKind::Open => open.entry(id.as_str()).or_default().push(i),
                TagKind::Close => {
                    if let Some(o) = open.get_mut(id.as_str()).and_then(|v| v.pop()) {
                        out.push((o, i));
                    }
                }
                TagKind::Standalone => {}
            }
        }
    }
    out
}

/// True when a cut placed before `a[p]` would fall inside a tag pair.
fn inside_pair(pairs: &[(usize, usize)], p: usize) -> bool {
    pairs.iter().any(|(o, c)| *o < p && p <= *c)
}

/// Moves a cut past close and standalone tags that follow the previous character, so a closing tag stays with
/// the sentence it ends and an opening tag goes with the next one.
fn settle_cut(a: &[Atom], prev_char: usize) -> usize {
    let mut p = prev_char + 1;
    while p < a.len() {
        match &a[p] {
            Atom::Tag(Inline::Tag { kind: TagKind::Close | TagKind::Standalone, .. }) => p += 1,
            _ => break,
        }
    }
    p
}

fn renumber(unit: &mut Unit) {
    if unit.segments.len() == 1 {
        unit.segments[0].id = unit.id.clone();
    } else {
        for (i, s) in unit.segments.iter_mut().enumerate() {
            s.id = format!("{}#{}", unit.id, i + 1);
        }
    }
}

/// Puts a unit back to one segment. The target is kept only when every segment had one.
fn collapse(unit: &mut Unit) {
    if unit.segments.len() <= 1 {
        return;
    }
    let all_translated = unit.segments.iter().all(|s| !s.target.is_empty());
    let source = unit.source();
    let target = if all_translated { unit.output() } else { vec![] };
    let state = unit.segments.iter().map(|s| s.state.rank()).min().map(SegState::from_rank).unwrap_or(SegState::New);
    // The origin survives only when every part has the same one.
    let origin = match unit.segments.first().map(|s| &s.origin) {
        Some(first) if all_translated && unit.segments.iter().all(|s| &s.origin == first) => first.clone(),
        _ => None,
    };
    unit.segments = vec![Segment {
        id: unit.id.clone(),
        source,
        target,
        state: if all_translated { state } else { SegState::New },
        origin,
    }];
    unit.separators.clear();
}

/// Splits a unit's single segment at the given atom cuts. Each cut is an index into the atoms: the new segment
/// starts there (after whitespace is moved to the separator).
fn split_at_cuts(unit: &mut Unit, a: &[Atom], cuts: &[usize]) {
    let pr = pairs(a);
    let mut segs: Vec<Vec<Atom>> = Vec::new();
    let mut seps: Vec<String> = Vec::new();
    let mut start = 0usize;
    for &p in cuts {
        if p <= start || p >= a.len() {
            continue;
        }
        let mut left = p;
        while left > start && is_ws(&a[left - 1]) {
            left -= 1;
        }
        let mut right = p;
        while right < a.len() && is_ws(&a[right]) {
            right += 1;
        }
        if left <= start || right >= a.len() {
            continue; // an empty segment on either side: not a real boundary
        }
        if inside_pair(&pr, left) || inside_pair(&pr, right) {
            continue;
        }
        segs.push(a[start..left].to_vec());
        seps.push(text_of(&a[left..right]));
        start = right;
    }
    if segs.is_empty() {
        return;
    }
    segs.push(a[start..].to_vec());
    unit.segments = segs
        .into_iter()
        .map(|s| Segment { id: String::new(), source: inlines(&s), target: vec![], state: SegState::New, origin: None })
        .collect();
    unit.separators = seps;
    renumber(unit);
}

/// Applies a granularity to a document.
///
/// `Sentence` splits every translatable unit into sentences. `Paragraph` and `Document` put each unit back to one
/// segment. Calling it again on a segmented document first rejoins each unit; targets are kept only when every
/// segment of the unit had one. `lang` is the source language (a tag such as "en" or "ja-JP").
pub fn segment(doc: &mut Document, lang: &str, mode: Granularity, rules: &Rules) {
    doc.granularity = mode;
    for unit in doc.units.iter_mut() {
        if !unit.translate {
            continue;
        }
        collapse(unit);
        if mode != Granularity::Sentence || unit.segments.len() != 1 {
            continue;
        }
        let a = atoms(&unit.segments[0].source);
        let mut s = String::new();
        // byte offset in `s` -> atom index of the character at that offset
        let mut at: Vec<(usize, usize)> = Vec::new();
        for (i, x) in a.iter().enumerate() {
            if let Atom::Ch(c) = x {
                at.push((s.len(), i));
                s.push(*c);
            }
        }
        let mut cuts = Vec::new();
        for b in rules.breaks(lang, &s) {
            // atom index of the character that starts the new sentence, and of the one before it
            let Ok(k) = at.binary_search_by_key(&b, |(off, _)| *off) else { continue };
            if k == 0 {
                continue;
            }
            cuts.push(settle_cut(&a, at[k - 1].1));
        }
        cuts.sort_unstable();
        cuts.dedup();
        split_at_cuts(unit, &a, &cuts);
    }
}

fn locate<'a>(doc: &'a mut Document, seg_id: &str) -> Result<(&'a mut Unit, usize), Error> {
    for u in doc.units.iter_mut() {
        if let Some(i) = u.segments.iter().position(|s| s.id == seg_id) {
            return Ok((u, i));
        }
    }
    Err(Error::UnknownSegment(seg_id.to_string()))
}

/// Joins two neighbouring segments of one unit into one. The merged segment keeps the text between them (the
/// separator) and its target is the two targets joined when both exist, else empty. Segment ids of the unit are
/// renumbered. Returns the id of the merged segment.
pub fn join(doc: &mut Document, seg_a: &str, seg_b: &str) -> Result<String, Error> {
    let (unit, i) = locate(doc, seg_a)?;
    if !unit.translate || unit.lock_reason.is_some() || unit.segments[i].state == SegState::Locked {
        return Err(Error::Locked(seg_a.to_string()));
    }
    if unit.segments.get(i + 1).map(|s| s.id.as_str()) != Some(seg_b) {
        return Err(Error::Unsupported(format!("{seg_a} and {seg_b} are not neighbours in one unit")));
    }
    if unit.segments[i + 1].state == SegState::Locked {
        return Err(Error::Locked(seg_b.to_string()));
    }
    let sep = unit.separators.get(i).cloned().unwrap_or_default();
    let b = unit.segments.remove(i + 1);
    unit.separators.remove(i);
    let a = &mut unit.segments[i];
    let both = !a.target.is_empty() && !b.target.is_empty();
    let state = if both { SegState::from_rank(a.state.rank().min(b.state.rank())) } else { SegState::New };
    let join_with_sep = |x: &[Inline], y: &[Inline]| -> Vec<Inline> {
        let mut v = x.to_vec();
        if !sep.is_empty() {
            v.push(Inline::Text(sep.clone()));
        }
        v.extend(y.iter().cloned());
        merge_text(v)
    };
    a.source = join_with_sep(&a.source, &b.source);
    a.target = if both { join_with_sep(&a.target, &b.target) } else { vec![] };
    a.state = state;
    if !both || a.origin != b.origin {
        a.origin = None;
    }
    renumber(unit);
    Ok(unit.segments[i].id.clone())
}

/// Splits a segment after `at_char` characters of its source text (tags do not count). Whitespace at the split
/// moves into the separator. Refuses to cut inside a tag pair, at the very start or end, or in a locked unit.
/// Both new segments start untranslated. Returns the two new segment ids.
pub fn split(doc: &mut Document, seg_id: &str, at_char: usize) -> Result<(String, String), Error> {
    let (unit, i) = locate(doc, seg_id)?;
    if !unit.translate || unit.lock_reason.is_some() || unit.segments[i].state == SegState::Locked {
        return Err(Error::Locked(seg_id.to_string()));
    }
    let a = atoms(&unit.segments[i].source);
    let mut count = 0usize;
    let mut prev = None;
    let mut next = None;
    for (k, x) in a.iter().enumerate() {
        if let Atom::Ch(_) = x {
            if count + 1 == at_char {
                prev = Some(k);
            }
            if count == at_char {
                next = Some(k);
                break;
            }
            count += 1;
        }
    }
    let (Some(prev), Some(_next)) = (prev, next) else {
        return Err(Error::Unsupported("split position is outside the text".into()));
    };
    let p = settle_cut(&a, prev);
    let pr = pairs(&a);
    let mut left = p;
    while left > 0 && is_ws(&a[left - 1]) {
        left -= 1;
    }
    let mut right = p;
    while right < a.len() && is_ws(&a[right]) {
        right += 1;
    }
    if left == 0 || right >= a.len() || text_of(&a[..left]).trim().is_empty() || text_of(&a[right..]).trim().is_empty() {
        return Err(Error::Unsupported("a segment cannot be empty".into()));
    }
    if inside_pair(&pr, left) || inside_pair(&pr, right) {
        return Err(Error::Unsupported("cannot split inside a tag pair".into()));
    }
    let first = Segment { id: String::new(), source: inlines(&a[..left]), target: vec![], state: SegState::New, origin: None };
    let second = Segment { id: String::new(), source: inlines(&a[right..]), target: vec![], state: SegState::New, origin: None };
    unit.segments.splice(i..=i, [first, second]);
    unit.separators.insert(i, text_of(&a[left..right]));
    renumber(unit);
    Ok((unit.segments[i].id.clone(), unit.segments[i + 1].id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_of(units: Vec<Vec<Inline>>) -> Document {
        let n = units.len();
        Document {
            format: "test".into(),
            name: None,
            source_lang: None,
            target_lang: None,
            granularity: Granularity::Sentence,
            units: units.into_iter().enumerate().map(|(i, s)| Unit::new(format!("u{}", i + 1), s)).collect(),
            skeleton: Skeleton::Text { parts: (0..n).map(SkelPart::Unit).collect(), encoding: "utf-8".into(), parser: None },
            warnings: vec![],
        }
    }

    fn segs(lang: &str, text: &str) -> Vec<String> {
        let mut d = doc_of(vec![vec![Inline::text(text)]]);
        segment(&mut d, lang, Granularity::Sentence, Rules::bundled());
        // round trip: the unit's output is exactly the original text
        assert_eq!(plain_text(&d.units[0].output()), text, "round trip for {text:?}");
        d.units[0].segments.iter().map(|s| plain_text(&s.source)).collect()
    }

    #[test]
    fn english_basic_and_abbreviations() {
        assert_eq!(segs("en", "Hello there. How are you? Fine!"), vec!["Hello there.", "How are you?", "Fine!"]);
        assert_eq!(segs("en", "Dr. Smith met Mr. Jones at 5. Then they left."), vec!["Dr. Smith met Mr. Jones at 5.", "Then they left."]);
        assert_eq!(segs("en", "Bring fruit, e.g. apples. Bring bread."), vec!["Bring fruit, e.g. apples.", "Bring bread."]);
        assert_eq!(segs("en", "See Fig. 3 for details. It is clear."), vec!["See Fig. 3 for details.", "It is clear."]);
        assert_eq!(segs("en", "Order No. 42 shipped. Thanks."), vec!["Order No. 42 shipped.", "Thanks."]);
        assert_eq!(segs("en-US", "Acme S.A. sells widgets. Buy one."), vec!["Acme S.A. sells widgets.", "Buy one."]);
    }

    #[test]
    fn numbers_and_ellipses() {
        assert_eq!(segs("en", "Version 1.9 is out. Version 2.0 follows."), vec!["Version 1.9 is out.", "Version 2.0 follows."]);
        assert_eq!(segs("en", "It costs 3.50 euros. Pay now."), vec!["It costs 3.50 euros.", "Pay now."]);
        assert_eq!(segs("en", "Wait... then go. Done."), vec!["Wait... then go.", "Done."]);
        assert_eq!(segs("en", "He paused... Then he spoke."), vec!["He paused...", "Then he spoke."]);
        assert_eq!(segs("en", "Wait\u{2026} Then go."), vec!["Wait\u{2026}", "Then go."]);
        assert_eq!(segs("en", "1. Introduction to the product"), vec!["1. Introduction to the product"]);
    }

    #[test]
    fn quotes_after_the_stop() {
        assert_eq!(segs("en", "He said \"Go.\" Then he left."), vec!["He said \"Go.\"", "Then he left."]);
        assert_eq!(segs("en", "\"Why?\" she asked. He did not answer."), vec!["\"Why?\" she asked.", "He did not answer."]);
        assert_eq!(segs("en", "She said \u{201c}Stop.\u{201d} He stopped."), vec!["She said \u{201c}Stop.\u{201d}", "He stopped."]);
        assert_eq!(segs("fr", "Il a dit \u{ab} Va. \u{bb} Puis il est parti."), vec!["Il a dit \u{ab} Va. \u{bb}", "Puis il est parti."]);
        assert_eq!(segs("en", "(It works.) Next one."), vec!["(It works.)", "Next one."]);
    }

    #[test]
    fn spanish() {
        assert_eq!(segs("es-ES", "\u{bf}Qu\u{e9} hora es? \u{a1}Es tarde! Vamos."), vec!["\u{bf}Qu\u{e9} hora es?", "\u{a1}Es tarde!", "Vamos."]);
        assert_eq!(segs("es", "El Sr. L\u{f3}pez llega hoy. Bienvenido."), vec!["El Sr. L\u{f3}pez llega hoy.", "Bienvenido."]);
        assert_eq!(segs("es", "Hola, \u{bf}qu\u{e9} tal? Bien."), vec!["Hola, \u{bf}qu\u{e9} tal?", "Bien."]);
    }

    #[test]
    fn french_italian_german_dutch_portuguese() {
        assert_eq!(segs("fr", "Bonjour M. Dupont. Comment allez-vous ? Tr\u{e8}s bien !"), vec!["Bonjour M. Dupont.", "Comment allez-vous ?", "Tr\u{e8}s bien !"]);
        assert_eq!(segs("fr", "Utilisez un outil, p. ex. un marteau. Merci."), vec!["Utilisez un outil, p. ex. un marteau.", "Merci."]);
        assert_eq!(segs("it", "Il Sig. Rossi \u{e8} qui. Benvenuto."), vec!["Il Sig. Rossi \u{e8} qui.", "Benvenuto."]);
        assert_eq!(segs("de", "Das gilt z. B. f\u{fc}r Kinder. Weitere Infos folgen."), vec!["Das gilt z. B. f\u{fc}r Kinder.", "Weitere Infos folgen."]);
        assert_eq!(segs("de", "Wir treffen uns am 3. Mai in Berlin. Bis dann."), vec!["Wir treffen uns am 3. Mai in Berlin.", "Bis dann."]);
        assert_eq!(segs("de", "Siehe Nr. 5 und vgl. Abbildung 2. Ende."), vec!["Siehe Nr. 5 und vgl. Abbildung 2.", "Ende."]);
        assert_eq!(segs("nl", "Dhr. De Vries komt morgen. Tot dan."), vec!["Dhr. De Vries komt morgen.", "Tot dan."]);
        assert_eq!(segs("pt-BR", "O Dr. Silva chegou. Bem-vindo!"), vec!["O Dr. Silva chegou.", "Bem-vindo!"]);
    }

    #[test]
    fn japanese_and_chinese_have_no_space_after_the_stop() {
        assert_eq!(segs("ja", "\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3067}\u{3059}\u{3002}\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3067}\u{3059}\u{3002}"), vec!["\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3067}\u{3059}\u{3002}", "\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3067}\u{3059}\u{3002}"]);
        assert_eq!(segs("zh-CN", "\u{4f60}\u{597d}\u{ff01}\u{4f60}\u{5728}\u{54ea}\u{91cc}\u{ff1f}\u{6211}\u{5728}\u{8fd9}\u{91cc}\u{3002}"), vec!["\u{4f60}\u{597d}\u{ff01}", "\u{4f60}\u{5728}\u{54ea}\u{91cc}\u{ff1f}", "\u{6211}\u{5728}\u{8fd9}\u{91cc}\u{3002}"]);
        // closing quote stays with its sentence; a stop inside a quote does not end the outer sentence early
        assert_eq!(segs("ja", "\u{300c}\u{884c}\u{304f}\u{3002}\u{300d}\u{3068}\u{8a00}\u{3063}\u{305f}\u{3002}\u{5f7c}\u{306f}\u{51fa}\u{305f}\u{3002}"), vec!["\u{300c}\u{884c}\u{304f}\u{3002}\u{300d}", "\u{3068}\u{8a00}\u{3063}\u{305f}\u{3002}", "\u{5f7c}\u{306f}\u{51fa}\u{305f}\u{3002}"]);
        // ASCII text inside Japanese is not broken at a version number
        assert_eq!(segs("ja", "\u{30d0}\u{30fc}\u{30b8}\u{30e7}\u{30f3}1.9\u{3092}\u{4f7f}\u{3046}\u{3002}\u{7d42}\u{308f}\u{308a}\u{3002}"), vec!["\u{30d0}\u{30fc}\u{30b8}\u{30e7}\u{30f3}1.9\u{3092}\u{4f7f}\u{3046}\u{3002}", "\u{7d42}\u{308f}\u{308a}\u{3002}"]);
    }

    #[test]
    fn korean_and_arabic() {
        assert_eq!(segs("ko", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}. \u{c800}\u{b294} \u{bbfc}\u{c218}\u{c785}\u{b2c8}\u{b2e4}. \u{c5b4}\u{b514}\u{c5d0} \u{acc4}\u{c138}\u{c694}?"), vec!["\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}.", "\u{c800}\u{b294} \u{bbfc}\u{c218}\u{c785}\u{b2c8}\u{b2e4}.", "\u{c5b4}\u{b514}\u{c5d0} \u{acc4}\u{c138}\u{c694}?"]);
        // Arabic question mark U+061F; the Arabic comma U+060C does not end a sentence
        assert_eq!(
            segs("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627}\u{60c} \u{643}\u{64a}\u{641} \u{62d}\u{627}\u{644}\u{643}\u{61f} \u{623}\u{646}\u{627} \u{628}\u{62e}\u{64a}\u{631}."),
            vec!["\u{645}\u{631}\u{62d}\u{628}\u{627}\u{60c} \u{643}\u{64a}\u{641} \u{62d}\u{627}\u{644}\u{643}\u{61f}", "\u{623}\u{646}\u{627} \u{628}\u{62e}\u{64a}\u{631}."]
        );
        assert_eq!(segs("ar-SA", "\u{645}\u{631}\u{62d}\u{628}\u{627}\u{60c} \u{62b}\u{645} \u{645}\u{639}\u{627}\u{60c} \u{634}\u{643}\u{631}\u{627}."), vec!["\u{645}\u{631}\u{62d}\u{628}\u{627}\u{60c} \u{62b}\u{645} \u{645}\u{639}\u{627}\u{60c} \u{634}\u{643}\u{631}\u{627}."]);
    }

    #[test]
    fn language_without_rules_falls_back_to_icu() {
        assert!(!Rules::bundled().has_rules_for("pl"));
        assert_eq!(segs("pl", "To jest zdanie. To jest drugie zdanie."), vec!["To jest zdanie.", "To jest drugie zdanie."]);
    }

    #[test]
    fn paragraph_and_document_modes_keep_one_segment() {
        for mode in [Granularity::Paragraph, Granularity::Document] {
            let mut d = doc_of(vec![vec![Inline::text("One. Two. Three.")]]);
            segment(&mut d, "en", mode, Rules::bundled());
            assert_eq!(d.units[0].segments.len(), 1);
            assert_eq!(d.units[0].segments[0].id, "u1");
            assert_eq!(d.granularity, mode);
        }
    }

    #[test]
    fn ids_and_separators() {
        let mut d = doc_of(vec![vec![Inline::text("One.  Two.\nThree.")], vec![Inline::text("Only one")]]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        let u = &d.units[0];
        assert_eq!(u.segments.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["u1#1", "u1#2", "u1#3"]);
        assert_eq!(u.separators, vec!["  ", "\n"]);
        assert_eq!(d.units[1].segments[0].id, "u2");
        assert_eq!(plain_text(&u.output()), "One.  Two.\nThree.");
    }

    #[test]
    fn leading_and_trailing_space_stay_in_the_unit() {
        assert_eq!(segs("en", "  First. Second.  "), vec!["  First.", "Second.  "]);
    }

    fn tag(id: &str, kind: TagKind) -> Inline {
        Inline::tag(id, kind, if kind == TagKind::Close { "</b>" } else { "<b>" })
    }

    #[test]
    fn paired_tag_across_a_boundary_stays_in_one_segment() {
        let src = vec![
            Inline::text("Start. "),
            tag("1", TagKind::Open),
            Inline::text("Bold one. Bold two."),
            tag("1", TagKind::Close),
            Inline::text(" End."),
        ];
        let mut d = doc_of(vec![src.clone()]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        let u = &d.units[0];
        assert_eq!(u.segments.len(), 3);
        assert_eq!(u.segments[1].source, vec![tag("1", TagKind::Open), Inline::text("Bold one. Bold two."), tag("1", TagKind::Close)]);
        assert_eq!(u.output(), src);
    }

    #[test]
    fn closing_tag_stays_with_its_sentence_and_standalone_tags_stay_put() {
        let src = vec![
            tag("1", TagKind::Open),
            Inline::text("Done."),
            tag("1", TagKind::Close),
            Inline::text(" Next "),
            Inline::tag("2", TagKind::Standalone, "{name}"),
            Inline::text(" here."),
        ];
        let mut d = doc_of(vec![src.clone()]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        let u = &d.units[0];
        assert_eq!(u.segments.len(), 2);
        assert_eq!(u.segments[0].source, vec![tag("1", TagKind::Open), Inline::text("Done."), tag("1", TagKind::Close)]);
        assert_eq!(u.segments[1].source, vec![Inline::text("Next "), Inline::tag("2", TagKind::Standalone, "{name}"), Inline::text(" here.")]);
        assert_eq!(u.output(), src);
    }

    #[test]
    fn tag_at_unit_start_is_in_the_first_segment() {
        let src = vec![Inline::tag("1", TagKind::Standalone, "<br/>"), Inline::text("A. B.")];
        let mut d = doc_of(vec![src.clone()]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        assert_eq!(d.units[0].segments[0].source, vec![Inline::tag("1", TagKind::Standalone, "<br/>"), Inline::text("A.")]);
        assert_eq!(d.units[0].output(), src);
    }

    #[test]
    fn locked_units_are_not_split() {
        let mut d = doc_of(vec![vec![Inline::text("One. Two.")]]);
        d.units[0].translate = false;
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        assert_eq!(d.units[0].segments.len(), 1);
    }

    #[test]
    fn resegmenting_changes_granularity() {
        let mut d = doc_of(vec![vec![Inline::text("One. Two.")]]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        assert_eq!(d.units[0].segments.len(), 2);
        segment(&mut d, "en", Granularity::Paragraph, Rules::bundled());
        assert_eq!(d.units[0].segments.len(), 1);
        assert_eq!(plain_text(&d.units[0].segments[0].source), "One. Two.");
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        assert_eq!(d.units[0].segments.len(), 2);
    }

    #[test]
    fn join_and_split() {
        let mut d = doc_of(vec![vec![Inline::text("One.  Two. Three.")]]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        let merged = join(&mut d, "u1#1", "u1#2").unwrap();
        assert_eq!(merged, "u1#1");
        let u = &d.units[0];
        assert_eq!(u.segments.len(), 2);
        assert_eq!(plain_text(&u.segments[0].source), "One.  Two.");
        assert_eq!(plain_text(&u.output()), "One.  Two. Three.");
        // split it again after "One."
        let (a, b) = split(&mut d, "u1#1", 4).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("u1#1", "u1#2"));
        assert_eq!(d.units[0].segments.len(), 3);
        assert_eq!(d.units[0].separators, vec!["  ", " "]);
        assert_eq!(plain_text(&d.units[0].output()), "One.  Two. Three.");
        // join everything: the id goes back to the unit id
        join(&mut d, "u1#1", "u1#2").unwrap();
        join(&mut d, "u1#1", "u1#2").unwrap();
        assert_eq!(d.units[0].segments.len(), 1);
        assert_eq!(d.units[0].segments[0].id, "u1");
        assert_eq!(plain_text(&d.units[0].output()), "One.  Two. Three.");
    }

    #[test]
    fn join_and_split_errors() {
        let mut d = doc_of(vec![vec![Inline::text("One. Two. Three.")], vec![Inline::text("Other.")]]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        assert!(matches!(join(&mut d, "u1#1", "u1#3"), Err(Error::Unsupported(_))));
        assert!(matches!(join(&mut d, "u1#1", "u2"), Err(Error::Unsupported(_))));
        assert!(matches!(join(&mut d, "nope", "u1#2"), Err(Error::UnknownSegment(_))));
        assert!(matches!(split(&mut d, "u2", 0), Err(Error::Unsupported(_))));
        assert!(matches!(split(&mut d, "u2", 99), Err(Error::Unsupported(_))));
    }

    #[test]
    fn join_and_split_refuse_locked_segments_and_units() {
        let mut d = doc_of(vec![vec![Inline::text("One. Two. Three.")]]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        d.units[0].segments[1].state = SegState::Locked;
        // Locked second, locked first, and a locked middle segment all refuse; nothing changes.
        assert!(matches!(join(&mut d, "u1#1", "u1#2"), Err(Error::Locked(id)) if id == "u1#2"));
        assert!(matches!(join(&mut d, "u1#2", "u1#3"), Err(Error::Locked(id)) if id == "u1#2"));
        assert!(matches!(split(&mut d, "u1#2", 2), Err(Error::Locked(_))));
        assert_eq!(d.units[0].segments.len(), 3);
        assert_eq!(d.units[0].segments[1].state, SegState::Locked);
        // A unit with a lock reason refuses both operations too.
        d.units[0].segments[1].state = SegState::New;
        d.units[0].lock_reason = Some("context match".into());
        assert!(matches!(join(&mut d, "u1#1", "u1#2"), Err(Error::Locked(_))));
        assert!(matches!(split(&mut d, "u1#1", 2), Err(Error::Locked(_))));
        assert_eq!(d.units[0].segments.len(), 3);
        // Unlocked again, it works.
        d.units[0].lock_reason = None;
        assert!(join(&mut d, "u1#1", "u1#2").is_ok());
    }

    #[test]
    fn split_refuses_to_cut_a_tag_pair_and_join_keeps_targets() {
        let src = vec![tag("1", TagKind::Open), Inline::text("Bold words here"), tag("1", TagKind::Close)];
        let mut d = doc_of(vec![src]);
        assert!(matches!(split(&mut d, "u1", 5), Err(Error::Unsupported(_))));

        let mut d = doc_of(vec![vec![Inline::text("Uno. Dos.")]]);
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        d.set_target("u1#1", vec![Inline::text("One.")]).unwrap();
        d.set_target("u1#2", vec![Inline::text("Two.")]).unwrap();
        join(&mut d, "u1#1", "u1#2").unwrap();
        assert_eq!(plain_text(&d.units[0].segments[0].target), "One. Two.");
    }

    #[test]
    fn all_primary_languages_have_rules() {
        for l in ["en", "es", "fr", "it", "de", "nl", "pt", "ja", "zh", "ko", "ar", "pt-BR", "zh-TW", "en_GB"] {
            assert!(Rules::bundled().has_rules_for(l), "{l}");
        }
    }

    #[test]
    fn custom_srx_and_errors() {
        let xml = r#"<srx version="2.0"><header/><body><languagerules><languagerule languagerulename="x">
            <rule break="yes"><beforebreak>;</beforebreak><afterbreak>\s*</afterbreak></rule></languagerule></languagerules>
            <maprules><languagemap languagepattern=".*" languagerulename="x"/></maprules></body></srx>"#;
        let r = Rules::from_srx(xml).unwrap();
        let mut d = doc_of(vec![vec![Inline::text("a; b; c")]]);
        segment(&mut d, "xx", Granularity::Sentence, &r);
        assert_eq!(d.units[0].segments.len(), 3);
        assert!(Rules::from_srx("<srx><body><maprules><languagemap languagepattern='.*' languagerulename='nope'/></maprules></body></srx>").is_err());
        let bad = xml.replace("[;]", "").replace(">;<", ">(<");
        assert!(Rules::from_srx(&bad).is_err());
    }

    #[test]
    fn long_text_is_fast_enough() {
        let text = "This is a sentence with Dr. Who. ".repeat(2000);
        let mut d = doc_of(vec![vec![Inline::text(text.clone())]]);
        let t = std::time::Instant::now();
        segment(&mut d, "en", Granularity::Sentence, Rules::bundled());
        assert!(t.elapsed().as_secs() < 5);
        assert_eq!(plain_text(&d.units[0].output()), text);
    }
}
