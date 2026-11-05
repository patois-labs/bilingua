//! `cargo run -p bilingua-pdf --example pdf_dump -- FILE [PAGE]`: the page text as Bilingua reads it, and with a
//! page number (1-based) the text objects of that page.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).expect("read");
    match args.get(2) {
        Some(p) => {
            let n: usize = p.parse().expect("page");
            print!("{}", bilingua_pdf::debug_page(&bytes, n - 1).expect("page"));
            let pages = bilingua_pdf::page_texts(&bytes, &Default::default()).expect("pages");
            println!("{}", pages[n - 1].text.escape_debug());
        }
        None => {
            for p in bilingua_pdf::page_texts(&bytes, &Default::default()).expect("pages") {
                println!("--- page {} ocr {}\n{}", p.page + 1, p.ocr, p.text.escape_debug());
            }
        }
    }
}
