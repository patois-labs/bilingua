//! Prints the units of a wiki file, one per line, with every inline tag replaced by one marker (U+00A4) and white
//! space collapsed, the way the oracle compares texts: `cargo run -p bilingua-wiki --example wiki_dump -- FILE [wiki|dokuwiki]`
use bilingua_model::{ExtractOptions, Filter, Inline};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = std::fs::read(&args[0]).expect("read file");
    let dialect = args.get(1).cloned().unwrap_or_else(|| if args[0].ends_with(".wiki") || args[0].ends_with(".mediawiki") { "wiki".into() } else { "dokuwiki".into() });
    let f: Box<dyn Filter> = if dialect == "wiki" { Box::new(bilingua_wiki::MediaWiki::default()) } else { Box::new(bilingua_wiki::DokuWiki) };
    let doc = f.extract(&data, &ExtractOptions::default()).expect("extract");
    for u in &doc.units {
        let mut s = String::new();
        for i in u.source() {
            match i {
                Inline::Text(t) => s.push_str(&t),
                Inline::Tag { .. } => s.push('\u{a4}'),
            }
        }
        let norm: Vec<&str> = s.split_whitespace().collect();
        println!("{}", norm.join(" "));
    }
}
