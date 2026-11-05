//! SRX 2.0 reader and matcher on top of the `regex` crate.
//!
//! Each SRX rule has a "before the break" and an "after the break" expression. We compile one regex per rule,
//! `(?:BEFORE)(?P<bl_after>AFTER)`, and the break position is where the `bl_after` group starts. Matches are
//! searched from every start position, so overlapping candidates are all found. At one position the first rule
//! in the list decides whether there is a break (SRX semantics). Rule lists of all matching language maps are
//! cascaded in file order.

use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug)]
#[non_exhaustive]
pub enum SrxError {
    Xml(String),
    Regex(String),
    Missing(String),
}

impl std::fmt::Display for SrxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SrxError::Xml(m) => write!(f, "invalid SRX file: {m}"),
            SrxError::Regex(m) => write!(f, "invalid regular expression in SRX rule: {m}"),
            SrxError::Missing(m) => write!(f, "SRX file refers to a missing language rule: {m}"),
        }
    }
}

impl std::error::Error for SrxError {}

#[derive(Debug)]
struct Rule {
    brk: bool,
    re: Regex,
}

/// Compiled segmentation rules.
#[derive(Debug)]
pub struct Rules {
    sets: Vec<Vec<Rule>>,
    /// (language pattern, index into `sets`), in file order.
    maps: Vec<(Regex, usize)>,
}

static BUNDLED: OnceLock<Rules> = OnceLock::new();

impl Rules {
    /// The rules shipped with Bilingua (`rules/default.srx`).
    pub fn bundled() -> &'static Rules {
        BUNDLED.get_or_init(|| Rules::from_srx(include_str!("../rules/default.srx")).expect("bundled SRX is valid"))
    }

    /// Reads an SRX 2.0 document.
    pub fn from_srx(xml: &str) -> Result<Rules, SrxError> {
        let mut reader = Reader::from_str(xml);
        let mut names: HashMap<String, usize> = HashMap::new();
        let mut sets: Vec<Vec<Rule>> = Vec::new();
        let mut raw_maps: Vec<(String, String)> = Vec::new();

        let mut cur_set: Option<usize> = None;
        let mut cur_brk = true;
        let mut before = String::new();
        let mut after = String::new();
        #[derive(PartialEq)]
        enum In {
            None,
            Before,
            After,
        }
        let mut inside = In::None;

        loop {
            let ev = reader.read_event().map_err(|e| SrxError::Xml(e.to_string()))?;
            match ev {
                Event::Start(ref e) | Event::Empty(ref e) => {
                    let is_empty = matches!(ev, Event::Empty(_));
                    let local = e.local_name();
                    let name = std::str::from_utf8(local.as_ref()).unwrap_or("").to_string();
                    let attr = |key: &str| -> Option<String> {
                        e.attributes().flatten().find(|a| a.key.as_ref() == key.as_bytes()).and_then(|a| {
                            a.unescape_value().ok().map(|v| v.into_owned())
                        })
                    };
                    match name.as_str() {
                        "languagerule" => {
                            let n = attr("languagerulename").unwrap_or_default();
                            sets.push(Vec::new());
                            names.insert(n, sets.len() - 1);
                            cur_set = if is_empty { None } else { Some(sets.len() - 1) };
                        }
                        "rule" => {
                            cur_brk = attr("break").map(|b| b != "no").unwrap_or(true);
                            before.clear();
                            after.clear();
                            if is_empty {
                                push_rule(&mut sets, cur_set, cur_brk, "", "")?;
                            }
                        }
                        "beforebreak" => inside = if is_empty { In::None } else { In::Before },
                        "afterbreak" => inside = if is_empty { In::None } else { In::After },
                        "languagemap" => {
                            raw_maps.push((
                                attr("languagepattern").unwrap_or_default(),
                                attr("languagerulename").unwrap_or_default(),
                            ));
                        }
                        _ => {}
                    }
                }
                Event::End(ref e) => {
                    let local = e.local_name();
                    match local.as_ref() {
                        b"beforebreak" | b"afterbreak" => inside = In::None,
                        b"rule" => push_rule(&mut sets, cur_set, cur_brk, &before, &after)?,
                        b"languagerule" => cur_set = None,
                        _ => {}
                    }
                }
                Event::Text(ref t) => {
                    let s = t.decode().map_err(|e| SrxError::Xml(e.to_string()))?;
                    match inside {
                        In::Before => before.push_str(&s),
                        In::After => after.push_str(&s),
                        In::None => {}
                    }
                }
                Event::CData(ref c) => {
                    let s = c.decode().map_err(|e| SrxError::Xml(e.to_string()))?;
                    match inside {
                        In::Before => before.push_str(&s),
                        In::After => after.push_str(&s),
                        In::None => {}
                    }
                }
                Event::GeneralRef(ref r) => {
                    let name = r.decode().map_err(|e| SrxError::Xml(e.to_string()))?;
                    let ch = match r.resolve_char_ref().map_err(|e| SrxError::Xml(e.to_string()))? {
                        Some(c) => c.to_string(),
                        None => match name.as_ref() {
                            "lt" => "<".into(),
                            "gt" => ">".into(),
                            "amp" => "&".into(),
                            "quot" => "\"".into(),
                            "apos" => "'".into(),
                            other => return Err(SrxError::Xml(format!("unknown entity &{other};"))),
                        },
                    };
                    match inside {
                        In::Before => before.push_str(&ch),
                        In::After => after.push_str(&ch),
                        In::None => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }

        let mut maps = Vec::new();
        for (pat, rule_name) in raw_maps {
            let idx = *names.get(&rule_name).ok_or_else(|| SrxError::Missing(rule_name.clone()))?;
            let re = Regex::new(&format!("(?i)^(?:{pat})$")).map_err(|e| SrxError::Regex(e.to_string()))?;
            maps.push((re, idx));
        }
        Ok(Rules { sets, maps })
    }

    /// True when at least one language map matches `lang`.
    pub fn has_rules_for(&self, lang: &str) -> bool {
        let lang = norm_lang(lang);
        self.maps.iter().any(|(re, _)| re.is_match(&lang))
    }

    /// Byte offsets inside `text` where a new sentence starts. Uses the SRX rules when the language has
    /// any, otherwise ICU sentence breaking.
    pub fn breaks(&self, lang: &str, text: &str) -> Vec<usize> {
        let lang = norm_lang(lang);
        let mut rules: Vec<&Rule> = Vec::new();
        let mut any = false;
        for (re, idx) in &self.maps {
            if re.is_match(&lang) {
                any = true;
                rules.extend(self.sets[*idx].iter());
            }
        }
        if !any {
            return icu_breaks(text);
        }
        srx_breaks(&rules, text)
    }
}

fn norm_lang(lang: &str) -> String {
    lang.trim().replace('_', "-")
}

fn push_rule(sets: &mut [Vec<Rule>], cur: Option<usize>, brk: bool, before: &str, after: &str) -> Result<(), SrxError> {
    let Some(i) = cur else { return Ok(()) };
    let src = format!("(?:{before})(?P<bl_after>{after})");
    let re = Regex::new(&src).map_err(|e| SrxError::Regex(format!("{e}")))?;
    sets[i].push(Rule { brk, re });
    Ok(())
}

fn srx_breaks(rules: &[&Rule], text: &str) -> Vec<usize> {
    // position -> (rule index, break?) ; the lowest rule index wins.
    let mut decided: HashMap<usize, (usize, bool)> = HashMap::new();
    for (idx, rule) in rules.iter().enumerate() {
        let mut start = 0usize;
        while start <= text.len() {
            let Some(caps) = rule.re.captures_at(text, start) else { break };
            let m = caps.get(0).expect("group 0");
            let pos = caps.name("bl_after").map(|g| g.start()).unwrap_or(m.end());
            match decided.get(&pos) {
                Some((i, _)) if *i <= idx => {}
                _ => {
                    decided.insert(pos, (idx, rule.brk));
                }
            }
            start = match text[m.start()..].chars().next() {
                Some(c) => m.start() + c.len_utf8(),
                None => break,
            };
        }
    }
    let mut out: Vec<usize> = decided
        .into_iter()
        .filter(|(pos, (_, brk))| *brk && *pos > 0 && *pos < text.len() && text.is_char_boundary(*pos))
        .map(|(pos, _)| pos)
        .collect();
    out.sort_unstable();
    out
}

fn icu_breaks(text: &str) -> Vec<usize> {
    let segmenter = icu_segmenter::SentenceSegmenter::new(Default::default());
    segmenter.segment_str(text).filter(|b| *b > 0 && *b < text.len()).collect()
}
