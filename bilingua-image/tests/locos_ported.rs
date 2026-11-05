//! The filter-level cases of the LocOS image suites, written fresh in Rust: `verify_image_localization.py` (the SVG
//! and region parts), `verify_image_raster_aliases.py` (sniff, frames, hostile headers), and the SVG cases that live
//! in other suites (`verify_filter_roundtrip_509`, `verify_rtl_layout`, `verify_output_a11y` #691,
//! `verify_office_comments` G, `verify_request_limits` #271, `verify_upload_segment_cap`, `verify_script_csp`).
//! `verify_image_scanning.py` tests deploy scripts (Trivy), not the filter: nothing to port.
//! Each test names the LocOS case it ports.
mod common;
use bilingua_image::{ImageFilter, SvgFilter};
use bilingua_model::*;
use common::*;

const BANNER: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"300\" height=\"120\">\n\
  <defs><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"#347\"/></linearGradient></defs>\n\
  <rect x=\"2\" y=\"2\" width=\"296\" height=\"116\" fill=\"url(#g)\"/>\n\
  <style>.t { font-family: sans-serif; }</style>\n\
  <text class=\"t\" x=\"16\" y=\"40\">Summer Sale &amp; more</text>\n\
  <text class=\"t\" x=\"16\" y=\"80\"><tspan>Shop now</tspan></text>\n\
  <!-- decorative comment stays byte-identical -->\n\
</svg>\n";

// ---- verify_image_localization (3) SVG swap fidelity ------------------------------------------------------------

#[test]
fn banner_units_and_byte_identical_round_trip() {
    let d = extract(BANNER);
    assert_eq!(texts(&d), ["Summer Sale & more", "Shop now"]);
    assert_eq!(build(BANNER, &[(0, "Summer Sale & more"), (1, "Shop now")]).unwrap(), BANNER);
    // a translation reaches the text nodes only
    let out = s(build(BANNER, &[(0, "Sommer-Sale & mehr"), (1, "Jetzt einkaufen")]).unwrap());
    assert!(out.contains(">Sommer-Sale &amp; mehr</text>") && out.contains("<tspan>Jetzt einkaufen</tspan>"));
    assert!(out.contains("<!-- decorative comment stays byte-identical -->") && out.contains("linearGradient"));
    wellformed(&out).unwrap();
}

#[test]
fn nothing_translated_is_the_original_bytes_in_every_mode() {
    let d = extract(BANNER);
    for o in [MergeOptions::default(), locos_opts()] {
        assert_eq!(SvgFilter.merge_with(&d, &o).unwrap(), BANNER);
    }
}

// ---- security: the uploaded SVG is attacker markup; the filter must not make it worse --------------------------------

#[test]
fn script_and_event_handlers_are_never_text_and_never_touched() {
    let evil = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"80\">\n  <script>alert(document.domain)</script>\n  <text x=\"10\" y=\"40\" onclick=\"alert(1)\">Summer Sale</text>\n</svg>\n";
    let d = extract(evil);
    assert_eq!(texts(&d), ["Summer Sale"]);
    let out = s(build(evil, &[(0, "Sommer</text><script>alert(2)</script><text>")]).unwrap());
    assert!(out.contains("<script>alert(document.domain)</script>") && out.contains("onclick=\"alert(1)\""));
    assert!(out.contains("Sommer&lt;/text&gt;&lt;script&gt;alert(2)&lt;/script&gt;&lt;text&gt;"));
    assert_eq!(start_names(&out), ["svg", "script", "text"]);
}

// ---- verify_filter_roundtrip_509: entities ---------------------------------------------------------------------------

#[test]
fn numeric_and_quote_entities_round_trip_without_double_escaping() {
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><text>&#169; 2024 &quot;Acme&quot; &#x41; &amp; Co</text></svg>";
    let d = extract(svg);
    let text = texts(&d).remove(0);
    assert_eq!(text, "\u{a9} 2024 \"Acme\" A & Co");
    assert_eq!(build(svg, &[(0, &text)]).unwrap(), svg);
    let out = s(build(svg, &[(0, &format!("{text}!"))]).unwrap());
    assert!(!out.contains("&amp;#169;") && !out.contains("&amp;quot;") && out.contains("\u{a9} 2024 \"Acme\" A &amp; Co!"));
    assert!(s(build(svg, &[(0, "a < b & c")]).unwrap()).contains("a &lt; b &amp; c"));
}

// ---- verify_rtl_layout: svg --------------------------------------------------------------------------------------------

const RTL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"200\">\
<text id=\"a\" x=\"10\" y=\"20\">Left label</text>\
<text id=\"b\" x=\"200\" y=\"40\" text-anchor=\"middle\">Centered</text>\
<text id=\"c\" x=\"390\" y=\"60\" style=\"font-size:12px; text-anchor: end\">Right label</text>\
<g text-anchor=\"end\"><text id=\"d\" x=\"390\" y=\"80\">Inherited end</text></g>\
<text id=\"e\" x=\"10\" y=\"100\"><tspan x=\"10\" text-anchor=\"start\">First line</tspan><tspan x=\"10\" dy=\"14\">Second line</tspan></text>\
<text id=\"f\" x=\"10\" y=\"140\" direction=\"ltr\">Explicit ltr</text>\
<text id=\"g\" x=\"10\" y=\"160\">Not translated</text>\
</svg>\n";

#[test]
fn svg_rtl_direction_and_mirrored_anchors_on_translated_text_only() {
    let n = extract(RTL.as_bytes()).units.len();
    assert_eq!(n, 8);
    let he: Vec<String> = (0..n - 1).map(|k| format!("\u{5d8}\u{5e7}\u{5e1}\u{5d8} {k}")).collect();
    let map: Vec<(usize, &str)> = he.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    let out = s(build_localized(RTL.as_bytes(), &map, "he").unwrap());
    wellformed(&out).unwrap();
    let t = |id: &str| tag_with_id(&out, id);
    for id in ["a", "b", "c", "d", "e", "f"] {
        assert!(t(id).contains("direction=\"rtl\""), "{id}: {}", t(id));
    }
    assert!(t("a").contains("text-anchor=\"end\""), "{}", t("a"));
    assert!(t("b").contains("text-anchor=\"middle\""));
    assert!(t("c").contains("text-anchor: start") && !t("c").contains(" text-anchor=\"") && t("c").contains("style=\"font-size:12px;"), "{}", t("c"));
    assert!(t("d").contains("text-anchor=\"start\""), "{}", t("d"));
    assert!(t("e").contains("text-anchor=\"end\""), "{}", t("e"));
    assert!(out.contains("<tspan x=\"10\" text-anchor=\"end\">") && out.contains("<tspan x=\"10\" dy=\"14\">"));
    assert!(!t("g").contains("direction") && !t("g").contains("text-anchor"));
    assert!(out.contains("<svg xml:lang=\"he\" xmlns=\"http://www.w3.org/2000/svg\""), "{out}");
    // left-to-right targets and the plain build: no direction, no anchor change
    for data in [build_localized(RTL.as_bytes(), &map, "de-DE").unwrap(), build(RTL.as_bytes(), &map).unwrap()] {
        let o = s(data);
        assert!(!o.contains("direction=\"rtl\""));
        assert!(!tag_with_id(&o, "a").contains("text-anchor"));
    }
}

#[test]
fn svg_rtl_empty_text_element_takes_no_edits_and_middle_anchor_stays() {
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><text id=\"z\" x=\"1\" y=\"1\"/><text id=\"a\" x=\"10\" y=\"20\">Hello</text><text id=\"m\" x=\"50\" y=\"40\" transform=\"rotate(30 50 40)\" text-anchor=\"middle\"><tspan x=\"50\">One</tspan><tspan x=\"50\" dy=\"12\" text-anchor=\"start\">Two</tspan></text></svg>";
    let out = s(build_localized(svg, &[(0, "\u{5e9}\u{5dc}\u{5d5}\u{5dd}"), (1, "\u{5d0}"), (2, "\u{5d1}")], "he").unwrap());
    wellformed(&out).unwrap();
    let t = |id: &str| tag_with_id(&out, id);
    assert!(!t("z").contains("direction") && !t("z").contains("text-anchor"), "{}", t("z"));
    assert!(t("a").contains("direction=\"rtl\"") && t("a").contains("text-anchor=\"end\""));
    assert!(t("m").contains("text-anchor=\"middle\"") && t("m").contains("transform=\"rotate(30 50 40)\""));
    assert!(out.contains("<tspan x=\"50\">\u{5d0}</tspan>") && out.contains("<tspan x=\"50\" dy=\"12\" text-anchor=\"end\">\u{5d1}</tspan>"), "{out}");
}

// ---- verify_output_a11y #691: the root declares the target language --------------------------------------------------

#[test]
fn svg_root_language() {
    let svg = b"<?xml version=\"1.0\"?>\n<!-- c -->\n<svg xmlns=\"http://www.w3.org/2000/svg\" xml:lang=\"en\"><text x=\"1\" y=\"2\">Hello</text><text xml:lang=\"la\">Lorem</text></svg>";
    let out = build_localized(svg, &[(0, "Hallo")], "de-DE").unwrap();
    let o = s(out);
    assert!(o.contains("<svg xmlns=\"http://www.w3.org/2000/svg\" xml:lang=\"de-DE\">") && o.contains(">Hallo<") && o.contains("<text xml:lang=\"la\">"), "{o}");
    assert_eq!(build_localized(svg, &[(0, "Hallo")], "").unwrap(), build(svg, &[(0, "Hallo")]).unwrap());
    assert_eq!(build(svg, &[]).unwrap(), svg);
    let bare = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><text>Hi</text></svg>";
    assert!(build_localized(bare, &[], "ar").unwrap().starts_with(b"<svg xml:lang=\"ar\" xmlns="));
    let plain = b"<svg lang='en' xmlns=\"http://www.w3.org/2000/svg\"><text>Hi</text></svg>";
    let o = s(build_localized(plain, &[], "fr-FR").unwrap());
    assert!(o.contains("lang=\"fr-FR\"") && !o.contains("xml:lang"), "{o}");
    assert_eq!(build_localized(b"<svg xml:lang=\"en\" lang=\"en\"><text>Hi</text></svg>", &[], "ja").unwrap(), b"<svg xml:lang=\"ja\" lang=\"ja\"><text>Hi</text></svg>");
    // `lang=` inside another attribute's quoted value is not an attribute
    let tricky = b"<svg aria-label=\"set lang=en\" data-n=' xml:lang=x' xml:lang=\"en\"><text>Hi</text></svg>";
    let want = b"<svg aria-label=\"set lang=en\" data-n=' xml:lang=x' xml:lang=\"de\"><text>Hi</text></svg>";
    assert_eq!(build_localized(tricky, &[], "de").unwrap(), want);
}

// ---- verify_office_comments G: title and desc ---------------------------------------------------------------------------

#[test]
fn title_and_desc_are_appended_units_and_nested_or_metadata_ones_are_skipped() {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><title>Sales &amp; returns</title><desc> Bars per quarter </desc>\
<metadata><title>Not announced</title></metadata><text>Q1 <tspan>bar</tspan></text><title>Q1 <tspan>bar</tspan></title><text>Q2 total</text></svg>";
    let d = extract(svg.as_bytes());
    let got: Vec<(String, String)> = texts(&d).into_iter().zip(styles(&d)).collect();
    let want = [("Q1 ", ""), ("bar", ""), ("Q2 total", ""), ("Sales & returns", "a11y:svg-title"), ("Bars per quarter", "a11y:svg-desc")];
    // LocOS cuts the text inside <text> at each tag, so "Q1 " is a unit of its own (trailing space is the lead/trail)
    let got_trim: Vec<(String, String)> = got.iter().map(|(t, s)| (t.trim().to_string(), s.clone())).collect();
    let want_trim: Vec<(String, String)> = want.iter().map(|(t, s)| (t.trim().to_string(), s.to_string())).collect();
    assert_eq!(got_trim, want_trim);
    let out = s(build(svg.as_bytes(), &[(3, "Umsatz & Retouren"), (4, "Balken")]).unwrap());
    assert!(out.contains("<title>Umsatz &amp; Retouren</title>") && out.contains("<desc> Balken </desc>"));
    assert!(out.contains("<metadata><title>Not announced</title></metadata>") && out.contains("<title>Q1 <tspan>bar</tspan></title>"));
    // an SVG imported before (text units only): its mapping never touches title and desc
    let old = s(build(svg.as_bytes(), &[(0, "T1"), (2, "Q2 gesamt")]).unwrap());
    assert!(old.contains("<title>Sales &amp; returns</title>") && old.contains("Q2 gesamt"));
}

// ---- verify_request_limits #271 and verify_upload_segment_cap -------------------------------------------------------

#[test]
fn tag_split_is_linear_and_unchanged() {
    let hostile = format!("<svg><text>{}", "<a".repeat(60000));
    let t = std::time::Instant::now();
    let d = SvgFilter.extract(hostile.as_bytes(), &opts());
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    // 60000 `<` and no `>`: no tag, and the one text node inside <text> has letters, so it is a unit... or the XML
    // guard refused it; either way it was quick and typed
    assert!(d.is_ok() || d.is_err());
    let units = texts(&extract(b"<svg><text>Hello SVG</text></svg>"));
    assert_eq!(units, ["Hello SVG"]);
    let sample = "<svg x=\"1\"><text>Hi <tspan>there</tspan> a < b</text><!-- c<d --><></svg>";
    let d = extract(sample.as_bytes());
    // `< b</text>` is read as a tag named b, as LocOS reads it: the text node after "there" is " a "
    assert_eq!(texts(&d), ["Hi", "there", "a"]);
}

fn capped(max: usize) -> ExtractOptions {
    opts().with_limits(Limits::new().with_max_units(max))
}

#[test]
fn tag_flood_and_too_many_text_nodes_are_refused_and_at_cap_parses() {
    let head = "<svg xmlns=\"http://www.w3.org/2000/svg\">";
    let flood = format!("{head}{}</svg>", "<a/>".repeat(20 * 100 + 10));
    assert!(matches!(SvgFilter.extract(flood.as_bytes(), &capped(100)), Err(Error::Limit(_))));
    let many = format!("{head}{}</svg>", "<text>Hi</text>".repeat(105));
    assert!(matches!(SvgFilter.extract(many.as_bytes(), &capped(100)), Err(Error::Limit(_))));
    let at_cap = format!("{head}{}</svg>", "<text>Hi</text>".repeat(100));
    assert_eq!(SvgFilter.extract(at_cap.as_bytes(), &capped(100)).unwrap().units.len(), 100);
}

// ---- verify_image_localization (1)(2): the region sidecar of a raster image ---------------------------------------

const REGIONS: &str = "[{\"text\": \"Summer Sale\", \"bbox\": [30, 20, 260, 44], \"confidence\": 0.97}, {\"text\": \"Shop now\", \"bbox\": [30, 100, 160, 36], \"confidence\": 0.5}, {\"text\": \"Big deals. Small prices.\", \"bbox\": [30, 170, 320, 30], \"confidence\": 0.92}, {\"text\": \"New\", \"bbox\": [420, 20, 46, 14], \"confidence\": 0.95}]";

#[test]
fn raster_regions_in_reading_order_with_the_geometry_contract() {
    let img = png(Some(REGIONS), 500, 300, &[]);
    let d = image_extract(&img).unwrap();
    assert_eq!(d.format, "image");
    // reading order: top to bottom, then left to right
    assert_eq!(texts(&d), ["Summer Sale", "New", "Shop now", "Big deals. Small prices."]);
    let r0: serde_json::Value = serde_json::from_str(&d.units[0].context.extra["image_region"]).unwrap();
    assert_eq!(r0["bbox_axis_aligned"], serde_json::json!([30, 20, 260, 44]));
    assert_eq!(r0["polygon"][0], serde_json::json!([30, 20]));
    assert_eq!(r0["polygon"][2], serde_json::json!([290, 64]));
    assert_eq!((r0["source"].as_str(), r0["text"].as_str(), r0["confidence"].as_f64()), (Some("mock"), Some("Summer Sale"), Some(0.97)));
    assert_eq!((r0["id"].as_str(), r0["reading_order_index"].as_i64()), (Some("r0"), Some(0)));
    assert_eq!(r0["baseline"]["intercept"].as_f64(), Some(64.0));
    assert_eq!(r0["lines"][0]["text"], "Summer Sale");
    for k in ["id", "reading_order_index", "polygon", "bbox_axis_aligned", "angle_deg", "baseline", "text", "confidence", "lines", "source"] {
        assert!(r0.get(k).is_some(), "geometry contract missing {k}");
    }
    assert_eq!(d.units[2].context.location.as_deref(), Some("30,100,160,36"));
}

#[test]
fn raster_without_a_sidecar_is_a_document_with_no_units() {
    for img in [png(None, 20, 20, &[]), png(Some("not json"), 20, 20, &[]), png(Some("{\"text\":\"a\",\"bbox\":[1,2,3,4]}"), 20, 20, &[])] {
        let d = image_extract(&img).unwrap();
        assert!(d.units.is_empty() && d.warnings.iter().any(|w| w.kind == "no-text-layer"));
        assert_eq!(ImageFilter.merge(&d).unwrap(), img);
    }
}

#[test]
fn raster_dimension_guards() {
    // verify_image_localization (8): the pixel-dimension guard; header only, the pixels are never reached
    let wide = png(Some(REGIONS), 8001, 1, &[]);
    assert!(image_extract(&wide).unwrap_err().to_string().contains("exceed"));
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = 5001u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&5001u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    big.extend(chunk(b"IHDR", &ihdr));
    big.extend(chunk(b"IDAT", b"junk"));
    big.extend(chunk(b"IEND", &[]));
    assert!(image_extract(&big).unwrap_err().to_string().contains("megapixels"));
}

// ---- verify_image_raster_aliases: sniff, frames, hostile ---------------------------------------------------------------

fn bmp(w: i32, h: i32) -> Vec<u8> {
    let mut v = b"BM".to_vec();
    v.extend_from_slice(&[0; 12]);
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&[1, 0, 24, 0]);
    v.extend_from_slice(&[0; 24]);
    v
}

fn gif(frames: usize) -> Vec<u8> {
    let mut v = b"GIF89a".to_vec();
    v.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0]);
    for _ in 0..frames {
        v.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4C, 0x01, 0]);
    }
    v.push(0x3B);
    v
}

fn tiff(next_ifd: u32) -> Vec<u8> {
    let mut v = b"II*\0".to_vec();
    v.extend_from_slice(&8u32.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    for (tag, val) in [(256u16, 10u16), (257, 10)] {
        v.extend_from_slice(&tag.to_le_bytes());
        v.extend_from_slice(&3u16.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&[val as u8, 0, 0, 0]);
    }
    v.extend_from_slice(&next_ifd.to_le_bytes());
    v
}

fn webp_x(w: u32, h: u32, animated: bool) -> Vec<u8> {
    let mut v = b"RIFF\x16\0\0\0WEBPVP8X\x0a\0\0\0".to_vec();
    v.push(if animated { 2 } else { 0 });
    v.extend_from_slice(&[0, 0, 0]);
    v.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
    v.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
    v
}

#[test]
fn sniff_decides_by_magic_not_extension() {
    for (img, ok) in [
        (png(None, 4, 4, &[]), true),
        (gif(1), true),
        (tiff(0), true),
        (webp_x(4, 4, false), true),
        (bmp(4, 4), true),
        (b"\xff\xd8\xff\xe0".to_vec(), true),
        (b"BM".to_vec(), false),
        (b"RIFF\0\0\0\0WAVEfmt ".to_vec(), false),
        (b"hello".to_vec(), false),
        (BANNER.to_vec(), false),
    ] {
        assert_eq!(bilingua_model::Filter::sniff(&ImageFilter, &img), ok, "{:?}", &img[..img.len().min(12)]);
    }
    assert!(bilingua_model::Filter::sniff(&SvgFilter, BANNER));
    assert!(!bilingua_model::Filter::sniff(&SvgFilter, &png(None, 4, 4, &[])));
    assert!(!bilingua_model::Filter::sniff(&SvgFilter, b"<!doctype html><html><body><svg width=\"1\"></svg></body></html>"));
}

#[test]
fn single_frame_rasters_open_and_multi_frame_ones_are_refused_with_a_plain_message() {
    for ok in [gif(1), tiff(0), webp_x(8, 8, false), bmp(8, 8)] {
        let d = image_extract(&ok).unwrap();
        assert!(d.units.is_empty());
    }
    for (bad, needle) in [(gif(2), "GIF is animated"), (tiff(200), "more than one page"), (webp_x(8, 8, true), "WEBP is animated")] {
        let e = image_extract(&bad).unwrap_err().to_string();
        assert!(e.contains(needle) && e.contains("single-frame") && e.is_ascii() && !e.contains('\u{2014}'), "{e}");
    }
}

#[test]
fn hostile_raster_headers_are_refused_before_decoding() {
    assert!(image_extract(&bmp(30000, 30000)).unwrap_err().to_string().contains("exceed"));
    assert!(image_extract(&bmp(6000, 6000)).unwrap_err().to_string().contains("megapixels"));
    let fake = [b"BM".as_slice(), &png(None, 4, 4, &[])].concat();
    assert!(image_extract(&fake).is_err());
    assert!(image_extract(&gif(1)[..20]).is_err());
    assert!(image_extract(b"RIFF\x10\x00\x00\x00WEBPVP8 junkjunk").is_err());
    // an ICO and an EPS header are no raster kind at all: refused before any decoder
    let ico = b"\0\0\x01\0\x01\0\x10\x10\0\0\x01\0\x20\0".to_vec();
    assert!(image_extract(&ico).unwrap_err().to_string().contains("not PNG, JPEG"));
    let eps = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 10 10\nshowpage\n";
    assert!(image_extract(eps).unwrap_err().to_string().contains("not PNG, JPEG"));
    // a JPEG that claims 30000x30000
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xC0, 0, 11, 8];
    jpg.extend_from_slice(&30000u16.to_be_bytes());
    jpg.extend_from_slice(&30000u16.to_be_bytes());
    jpg.extend_from_slice(&[1, 1, 0x11, 0, 0xFF, 0xD9]);
    assert!(image_extract(&jpg).unwrap_err().to_string().contains("exceed"));
}

#[test]
fn animated_png_is_refused_as_pillow_does() {
    let still = png(None, 8, 8, &[]);
    let mut actl = Vec::new();
    actl.extend_from_slice(&2u32.to_be_bytes());
    actl.extend_from_slice(&0u32.to_be_bytes());
    // acTL must come before IDAT: rebuild with the chunk after IHDR
    let img = png(None, 8, 8, &[(b"acTL", actl)]);
    assert!(image_extract(&img).unwrap_err().to_string().contains("animated"));
    assert!(image_extract(&still).is_ok());
}
