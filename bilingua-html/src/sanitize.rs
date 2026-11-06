//! `sanitize_html`: scripts, event handlers, script URLs, `<base>` and meta refresh removed from HTML (the
//! preview and any other caller that shows translated HTML in a browser).

use crate::tok::*;
use bilingua_htmlcore::pyx::unescape;

fn apply_edits(mut s: String, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (a, b, r) in edits {
        if a <= b && b <= s.len() && s.is_char_boundary(a) && s.is_char_boundary(b) {
            s.replace_range(a..b, &r);
        }
    }
    s
}

const PREVIEW_STYLE: &str = "<style>[data-bl-seg]{outline:none}[data-bl-seg].bl-active{outline:2px solid #3b82f6;outline-offset:2px}</style>";

fn unsafe_url(v: &str) -> bool {
    let d: String = unescape(v).chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    d.starts_with("javascript:") || d.starts_with("vbscript:") || d.starts_with("data:text/html")
}

/// Removes scripts, event handler attributes, script URLs, `<base>` and meta refresh from HTML. With
/// `inject`, also adds the preview `<style>`.
pub fn sanitize_html(html: &str, inject: bool) -> String {
    let toks = tokenize(html);
    let mut insert_at: Option<usize> = None; // token index to insert before
    if inject {
        let mut after_html: Option<usize> = None;
        let mut first_content: Option<usize> = None;
        for (i, t) in toks.iter().enumerate() {
            match t {
                Tok::End { name, .. } if name == "head" && insert_at.is_none() => insert_at = Some(i),
                Tok::Start { name, .. } if name == "html" && after_html.is_none() => after_html = Some(i + 1),
                Tok::Start { .. } | Tok::Text(..) if first_content.is_none() => first_content = Some(i),
                _ => {}
            }
        }
        insert_at = insert_at.or(after_html).or(first_content).or(Some(0));
    }
    let mut out = String::with_capacity(html.len() + 200);
    let mut in_script = false;
    for (i, t) in toks.iter().enumerate() {
        if insert_at == Some(i) {
            out.push_str(PREVIEW_STYLE);
        }
        let (a, b) = t.range();
        match t {
            Tok::Start { name, attrs, .. } => {
                if name == "script" {
                    in_script = true;
                    continue;
                }
                if name == "base" {
                    continue;
                }
                if name == "meta" && attr_value(html, attrs, "http-equiv").is_some_and(|v| v.trim().eq_ignore_ascii_case("refresh")) {
                    continue;
                }
                let mut cuts: Vec<(usize, usize, String)> = Vec::new();
                for at in attrs {
                    let bad_name = (at.name.starts_with("on") && at.name.len() > 2) || at.name == "srcdoc";
                    let bad_url = matches!(at.name.as_str(), "href" | "src" | "action" | "formaction" | "xlink:href" | "data" | "poster")
                        && at.val.is_some_and(|(x, y)| unsafe_url(&html[x..y]));
                    if bad_name || bad_url {
                        let end = at.full.map(|f| f.1).unwrap_or(at.name_end);
                        cuts.push((at.name_start - a, end - a, String::new()));
                    }
                }
                out.push_str(&apply_edits(html[a..b].to_string(), cuts));
            }
            Tok::End { name, .. } if name == "script" => {
                in_script = false;
            }
            Tok::Raw(..) | Tok::Text(..) if in_script => {}
            Tok::End { name, .. } if name == "base" => {}
            _ => {
                if !in_script {
                    out.push_str(&html[a..b]);
                }
            }
        }
    }
    out
}
