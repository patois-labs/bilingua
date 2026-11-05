//! Developer tool (not shipped).
//!   icml_dump dump FILE                 units as JSON: [{"p": n, "text": "LocOS token text"}]
//!   icml_dump pseudo FILE OUT [LANG]    merge with every unit as "[" + source + "]" (optional target language)
//!   icml_dump gen PATH MB               a large generated ICML file
//!   icml_dump time FILE                 times extract, untouched merge and a pseudo-translated merge

use bilingua_icml::Icml;
use bilingua_model::*;
use std::time::Instant;

fn tok_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let run = u.codes.get(id).and_then(|c| c.label.as_deref()) == Some("run");
                let l = if run { 'r' } else { 'g' };
                match kind {
                    TagKind::Open => s.push_str(&format!("\u{27E6}{l}{id}\u{27E7}")),
                    TagKind::Close => s.push_str(&format!("\u{27E6}/{l}{id}\u{27E7}")),
                    TagKind::Standalone => s.push_str(&format!("\u{27E6}x{id}\u{27E7}")),
                }
            }
        }
    }
    s
}

fn big() -> Limits {
    Limits { max_units: 50_000_000, max_unit_chars: 10_000_000, max_input_bytes: 2 << 30, max_lines: 100_000_000, max_xml_elements: 100_000_000, ..Limits::default() }
}

fn pseudo(d: &mut Document) {
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opts = ExtractOptions { granularity: Granularity::Paragraph, ..Default::default() };
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match Icml.extract_with(&bytes, &opts, &big()) {
                Ok(d) => {
                    let v: Vec<_> = d
                        .units
                        .iter()
                        .map(|u| serde_json::json!({"p": u.codes["~p"].standalone.clone().unwrap_or_default().parse::<u64>().unwrap_or(0), "text": tok_text(u, &u.source())}))
                        .collect();
                    println!("{}", serde_json::to_string(&v).unwrap());
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("pseudo") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let o = ExtractOptions { target_lang: args.get(4).cloned(), ..opts };
            let mut d = Icml.extract_with(&bytes, &o, &big()).expect("extract");
            pseudo(&mut d);
            std::fs::write(&args[3], Icml.merge(&d).expect("merge")).expect("write");
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            std::fs::write(&args[2], generate(mb * 1_000_000)).expect("write");
        }
        Some("time") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let t = Instant::now();
            let mut d = Icml.extract_with(&bytes, &opts, &big()).expect("extract");
            let t_ext = t.elapsed();
            let t = Instant::now();
            let same = Icml.merge(&d).expect("merge") == bytes;
            let t_same = t.elapsed();
            pseudo(&mut d);
            let t = Instant::now();
            let out = Icml.merge(&d).expect("merge");
            let t_ps = t.elapsed();
            println!("bytes {} units {} extract {:?} untouched-merge {:?} (identical {same}) pseudo-merge {:?} out {}", bytes.len(), d.units.len(), t_ext, t_same, t_ps, out.len());
        }
        _ => eprintln!("usage: icml_dump dump|pseudo|gen|time ..."),
    }
}

/// A made-up magazine story of at least `min` bytes: headings, styled paragraphs, links, footnotes, tables.
pub fn generate(min: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<?aid style=\"50\" type=\"snippet\" readerVersion=\"6.0\" featureSet=\"513\" product=\"20.0(73)\" ?>\n<?aid SnippetType=\"InCopyInterchange\"?>\n<Document DOMVersion=\"20.0\" Self=\"d\">\n\t<RootCharacterStyleGroup Self=\"rcsg\"><CharacterStyle Self=\"CharacterStyle/Bold\" Name=\"Bold\"/></RootCharacterStyleGroup>\n\t<Story Self=\"u1\" AppliedTOCStyle=\"n\" TrackChanges=\"false\" StoryTitle=\"$ID/\" AppliedNamedGrid=\"n\">\n\t\t<StoryPreference OpticalMarginAlignment=\"false\" FrameType=\"TextFrameType\"/>\n",
    );
    let mut i = 0usize;
    while s.len() < min {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        let ps = "<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\">";
        match i % 4 {
            0 => s.push_str(&format!("\t\t{ps}\n\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Properties><AppliedFont type=\"string\">Minion Pro</AppliedFont></Properties><Content>Our {} {} is ready, order #{i} today </Content></CharacterStyleRange>\n\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Bold\"><Content>{}</Content></CharacterStyleRange>\n\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content> &amp; more.</Content><Br/></CharacterStyleRange>\n\t\t</ParagraphStyleRange>\n", w(1), w(2), w(3))),
            1 => s.push_str(&format!("\t\t{ps}\n\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>Caf\u{e9} {}\t\u{65e5}\u{672c} </Content><HyperlinkTextSource Self=\"h{i}\" Name=\"l{i}\"><Content>{}</Content></HyperlinkTextSource><Content> now</Content><PageReference Self=\"pr{i}\" PageReferenceType=\"CurrentPage\"/><Br/></CharacterStyleRange>\n\t\t</ParagraphStyleRange>\n", w(4), w(5))),
            2 => s.push_str(&format!("\t\t{ps}\n\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>Heading {} {}</Content><Br/><Content>Second line of {}<?ACE 7?> here</Content><Br/></CharacterStyleRange>\n\t\t</ParagraphStyleRange>\n", w(6), w(7), w(8))),
            _ => s.push_str(&format!("\t\t{ps}\n\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Table Self=\"t{i}\" BodyRowCount=\"1\" ColumnCount=\"2\"><Cell Self=\"t{i}a\" Name=\"0:0\"><ParagraphStyleRange><CharacterStyleRange><Content>Cell {}</Content></CharacterStyleRange></ParagraphStyleRange></Cell><Cell Self=\"t{i}b\" Name=\"1:0\"><ParagraphStyleRange><CharacterStyleRange><Content>Cell {}</Content></CharacterStyleRange></ParagraphStyleRange></Cell></Table><Br/></CharacterStyleRange>\n\t\t</ParagraphStyleRange>\n", w(9), w(1))),
        }
        i += 1;
    }
    s.push_str("\t</Story>\n</Document>\n");
    s
}
