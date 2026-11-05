//! Hostile files built to make the reader do far more work than the file is large: page-tree DAGs, Type 3 fonts
//! that draw with themselves, filter bombs, huge predictor rows, huge dictionaries, streams without an end, form
//! fan-out, one line of many objects, CFF glyphs that loop through subroutines. Each must end quickly with a
//! result or a typed error. Run in release (`cargo test --release -p bilingua-pdf --test gate_attacks`); the time
//! bounds are generous for a debug build.

mod common;
use bilingua_model::*;
use bilingua_pdf::Pdf;
use common::*;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(20);

fn run(name: &str, b: &[u8]) -> Result<Document, Error> {
    run_with(name, b, &ExtractOptions::default())
}

fn run_with(name: &str, b: &[u8], o: &ExtractOptions) -> Result<Document, Error> {
    let t = Instant::now();
    let r = Pdf.extract(b, o);
    let took = t.elapsed();
    eprintln!("{name}: {:?} in {took:?}", r.as_ref().map(|d| d.units.len()).map_err(|e| e.to_string()));
    assert!(took < BOUND, "{name} took {took:?}");
    if let Err(e) = &r {
        assert!(matches!(e, Error::Limit(_) | Error::Parse { .. } | Error::Unsupported(_)), "{name}: {e:?}");
    }
    r
}

fn page_with(resources: &str, content: &[u8], extra: Vec<O>) -> Vec<u8> {
    let mut objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain(format!("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources {resources} /Contents 4 0 R>>")),
        O::Stream(String::new(), content.to_vec()),
    ];
    objs.extend(extra);
    pdf(&objs)
}

#[test]
fn page_tree_dag_is_not_walked_exponentially() {
    // Every branch lists the next branch twice: 2^60 paths to an empty leaf branch.
    let mut objs = vec![O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()), O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into())];
    for k in 3..63 {
        objs.push(O::Plain(format!("<</Type /Pages /Kids [{} 0 R {} 0 R]>>", k + 1, k + 1)));
    }
    objs.push(O::Plain("<</Type /Pages /Kids []>>".into()));
    let _ = run("page_dag", &pdf(&objs));
}

#[test]
fn type3_fonts_drawing_with_each_other_do_not_explode() {
    // 40 Type 3 fonts; every glyph of every font is a d0 glyph that shows text in all 40 fonts.
    let n = 40;
    let first_font = 5;
    let glyph = first_font + n;
    let mut fonts = String::new();
    for i in 0..n {
        fonts.push_str(&format!("/T{i} {} 0 R ", first_font + i));
    }
    let mut g = String::from("0 0 d0 BT ");
    for i in 0..n {
        g.push_str(&format!("/T{i} 1 Tf (abc) Tj "));
    }
    g.push_str("ET");
    let diffs: String = (0..256).map(|c| format!("/g{c}")).collect::<Vec<_>>().join(" ");
    let procs: String = (0..256).map(|c| format!("/g{c} {glyph} 0 R")).collect::<Vec<_>>().join(" ");
    let mut extra = Vec::new();
    for _ in 0..n {
        extra.push(O::Plain(format!(
            "<</Type /Font /Subtype /Type3 /FontBBox [0 0 0 0] /FontMatrix [0.001 0 0 0.001 0 0] /FirstChar 0 /LastChar 255 \
             /Encoding <</Type /Encoding /Differences [0 {diffs}]>> /CharProcs <<{procs}>> /Resources <</Font <<{fonts}>>>>>>"
        )));
    }
    extra.push(O::Stream(String::new(), g.into_bytes()));
    let b = page_with(&format!("<</Font <<{fonts}>>>>"), b"BT /T0 12 Tf 72 700 Td (Some text in a Type 3 font) Tj ET", extra);
    let _ = run("type3_web", &b);
}

#[test]
fn run_length_and_ascii85_bombs_stay_in_the_budget() {
    let mut o = ExtractOptions::default();
    o.format_options.insert("pdf.max_stream_bytes".into(), "4000000".into());
    // 4 MB of RunLength pairs unpack to 512 MB; 4 MB of 'z' to 16 MB; flate in front makes the input tiny.
    let rle: Vec<u8> = [0x81u8, b' '].repeat(2_000_000);
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Contents 4 0 R>>".into()),
        O::Stream("/Filter /RunLengthDecode".into(), rle.clone()),
    ];
    assert!(matches!(run_with("rle_bomb", &pdf(&objs), &o), Err(Error::Limit(_))));
    let z = deflate(&rle);
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Contents 4 0 R>>".into()),
        O::Stream("/Filter [/FlateDecode /RunLengthDecode]".into(), z),
    ];
    assert!(matches!(run_with("flate_rle_bomb", &pdf(&objs), &o), Err(Error::Limit(_))));
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Contents 4 0 R>>".into()),
        O::Stream("/Filter [/FlateDecode /ASCII85Decode]".into(), deflate(&vec![b'z'; 3_000_000])),
    ];
    assert!(matches!(run_with("flate_a85_bomb", &pdf(&objs), &o), Err(Error::Limit(_))));
}

#[test]
fn huge_predictor_rows_do_not_allocate_per_stream() {
    let refs: String = (0..3000).map(|_| "5 0 R").collect::<Vec<_>>().join(" ");
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Contents 4 0 R>>".into()),
        O::Plain(format!("[{refs}]")),
        O::Stream("/Filter /FlateDecode /DecodeParms <</Predictor 12 /Columns 250000000>>".into(), deflate(b"\x00BT ET")),
    ];
    let _ = run("predictor_rows", &pdf(&objs));
}

#[test]
fn huge_dictionaries_are_not_quadratic() {
    let n = 300_000;
    let keys: String = (0..n).map(|i| format!("/k{i} 1 ")).collect();
    let fonts: String = (0..n).map(|i| format!("/F{i} 5 0 R ")).collect();
    let content = "/F0 1 Tf ".repeat(n);
    let objs = vec![
        O::Plain(format!("<</Type /Catalog /Pages 2 0 R /X <<{keys}>>>>")),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain(format!("<</Type /Page /Parent 2 0 R /Resources <</Font <<{fonts}>>>> /Contents 4 0 R>>")),
        O::Stream(String::new(), format!("BT {content} 72 700 Td (A line of text long enough.) Tj ET").into_bytes()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>".into()),
    ];
    let _ = run("huge_dicts", &pdf(&objs));
}

#[test]
fn streams_without_an_end_are_not_copied_again_and_again() {
    // 6000 stream objects with a wrong /Length and no endstream or endobj, all used as page content.
    let n = 6000;
    let mut b = b"%PDF-1.7\n".to_vec();
    let total = 5 + n;
    let mut offs = vec![0usize; total];
    offs[1] = b.len();
    b.extend_from_slice(b"1 0 obj <</Type /Catalog /Pages 2 0 R>> endobj\n");
    offs[2] = b.len();
    b.extend_from_slice(b"2 0 obj <</Type /Pages /Kids [3 0 R] /Count 1>> endobj\n");
    offs[3] = b.len();
    let refs: String = (0..n).map(|i| format!("{} 0 R ", 5 + i)).collect();
    b.extend_from_slice(format!("3 0 obj <</Type /Page /Parent 2 0 R /Contents [{refs}]>> endobj\n").as_bytes());
    for (k, off) in offs.iter_mut().enumerate().skip(5) {
        *off = b.len();
        b.extend_from_slice(format!("{k} 0 obj <</Length 99999999>> stream\nBT ET {}\n", "%".repeat(80)).as_bytes());
    }
    let at = b.len();
    b.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    for &o in &offs {
        if o == 0 {
            b.extend_from_slice(b"0000000000 65535 f \n");
        } else {
            b.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
    }
    b.extend_from_slice(format!("trailer\n<</Size {total} /Root 1 0 R>>\nstartxref\n{at}\n%%EOF\n").as_bytes());
    let _ = run("no_endstream", &b);
}

#[test]
fn form_fan_out_ends() {
    // Form k draws form k+1 four times, 30 levels: 4^30 form runs if nothing stops it.
    let levels = 30;
    let mut objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</XObject <</X 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), b"/X Do".to_vec()),
    ];
    for k in 0..levels {
        let next = 6 + k;
        objs.push(O::Stream(
            format!("/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources <</XObject <</X {next} 0 R>>>>"),
            b"/X Do /X Do /X Do /X Do".to_vec(),
        ));
    }
    objs.push(O::Stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10]".into(), b"0 0 m 1 1 l S".to_vec()));
    let _ = run("form_fan_out", &pdf(&objs));
}

#[test]
fn many_objects_on_one_line_are_not_quadratic() {
    // 300 000 one-character text objects on one baseline, right to left: each would be inserted at the front.
    let n = 300_000;
    let mut c = String::from("BT /F1 1 Tf\n");
    for i in 0..n {
        c.push_str(&format!("1 0 0 1 {} 700 Tm (a) Tj\n", n - i));
    }
    c.push_str("ET");
    let b = pages(&[c.into_bytes()], true);
    let _ = run("one_line", &b);
}

/// A CID-keyed CFF whose one glyph calls a chain of global subroutines ten deep, each calling the next ten times.
fn looping_cff() -> Vec<u8> {
    fn index(items: &[Vec<u8>]) -> Vec<u8> {
        let mut out = (items.len() as u16).to_be_bytes().to_vec();
        if items.is_empty() {
            return out;
        }
        let total: usize = items.iter().map(|i| i.len()).sum::<usize>() + 1;
        let osz = if total < 256 { 1 } else { 2 };
        out.push(osz as u8);
        let mut off = 1usize;
        let put = |out: &mut Vec<u8>, v: usize| {
            if osz == 1 {
                out.push(v as u8)
            } else {
                out.extend_from_slice(&(v as u16).to_be_bytes())
            }
        };
        put(&mut out, off);
        for i in items {
            off += i.len();
            put(&mut out, off);
        }
        for i in items {
            out.extend_from_slice(i);
        }
        out
    }
    let mut gsubrs = Vec::new();
    for i in 0..10u8 {
        let mut s = Vec::new();
        for _ in 0..10 {
            s.push(139 + i + 1 - 107);
            s.push(29);
        }
        s.push(11);
        gsubrs.push(s);
    }
    gsubrs.push(vec![139 + 5, 139 + 5, 5, 11]);
    let name = index(&[b"A".to_vec()]);
    let strings = index(&[]);
    let gs = index(&gsubrs);
    let glyphs: Vec<Vec<u8>> = (0..2).map(|_| vec![139 - 107, 29, 14]).collect();
    let cs = index(&glyphs);
    // Top DICT: ROS (12 30) and CharStrings (17) with a 5-byte offset; its size is fixed, so the offset is known.
    let top_len = 3 + 2 + 5 + 1;
    let top_index_len = 2 + 1 + 2 + top_len;
    let cs_off = 4 + name.len() + top_index_len + strings.len() + gs.len();
    let mut top = vec![139, 139, 139, 12, 30, 29];
    top.extend_from_slice(&(cs_off as i32).to_be_bytes());
    top.push(17);
    let mut out = vec![1, 0, 4, 1];
    out.extend(name);
    out.extend(index(&[top]));
    out.extend(strings);
    out.extend(gs);
    out.extend(cs);
    out
}

#[test]
fn cff_glyphs_that_loop_through_subroutines_are_bounded() {
    let hex: String = (0..65536u32).map(|c| format!("{c:04x}")).collect();
    let extra = vec![
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /A /Encoding /Identity-H /DescendantFonts [6 0 R]>>".into()),
        O::Plain("<</Type /Font /Subtype /CIDFontType0 /BaseFont /A /CIDSystemInfo <</Registry (Adobe) /Ordering (Identity) /Supplement 0>> /FontDescriptor 7 0 R /DW 500>>".into()),
        O::Plain("<</Type /FontDescriptor /FontName /A /Flags 4 /FontBBox [0 0 0 0] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile3 8 0 R>>".into()),
        O::Stream("/Subtype /CIDFontType0C".into(), looping_cff()),
    ];
    let b = page_with("<</Font <</A 5 0 R>>>>", format!("BT /A 1 Tf 72 700 Td <{hex}> Tj ET").as_bytes(), extra);
    let _ = run("cff_loops", &b);
}
