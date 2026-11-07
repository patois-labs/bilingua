use super::*;

const SRT: &str = "1\n00:00:01,000 --> 00:00:03,500\nWelcome to <b>Larchmoor</b>.\nThe kettle is <i>ready</i>.\n\n2\n00:00:04,000 --> 00:00:06,000 X1:100 X2:200\n{\\an8}Top line &amp; more\n\n3\n00:00:07,000 --> 00:00:09,250\n<font color=\"#ff0000\">Red <u>and underlined</u></font>\n\n\n\n4\n00:00:10,000 --> 00:00:11,000\nLast cue, no final newline";

const VTT: &str = "WEBVTT - Larchmoor demo\n\nSTYLE\n::cue(.loud) { color: red }\n\nNOTE This is a comment\nthat spans two lines\n\nintro\n00:00:01.000 --> 00:00:03.500 align:start position:10%\n<v Brent>Welcome to <b>Larchmoor</b> &amp; friends.\nSecond <i>line</i>.\n\n00:04.000 --> 00:06.000\n<c.loud>Shout</c.loud> at <00:00:05.000>five seconds\n\nREGION\nid:r1\n\n3\n00:07.000 --> 00:09.000\n<v Ana>Unclosed voice <b>bold\n";

fn srt(s: &str) -> Document {
    Srt.extract(s.as_bytes(), &Default::default()).unwrap()
}

fn vtt(s: &str) -> Document {
    Vtt.extract(s.as_bytes(), &Default::default()).unwrap()
}

fn text_of(d: &Document, id: &str) -> String {
    plain_text(&d.find(id).unwrap().1.source)
}

#[test]
fn srt_round_trip_and_units() {
    let d = srt(SRT);
    assert_eq!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap(), SRT);
    assert_eq!(d.units.len(), 4);
    let u = &d.units[0];
    assert_eq!(u.id, "c1");
    assert_eq!(u.context.kind.as_deref(), Some("subtitle"));
    assert_eq!(u.context.key.as_deref(), Some("1"));
    assert_eq!(u.context.extra["start"], "00:00:01,000");
    assert_eq!(u.context.extra["end"], "00:00:03,500");
    assert_eq!(u.context.extra["max_cpl"], "42");
    assert_eq!(plain_text(&u.segments[0].source), "Welcome to Larchmoor.The kettle is ready.");
    // b open/close, line break standalone, i open/close
    let shown: Vec<String> = u.segments[0]
        .source
        .iter()
        .filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None })
        .collect();
    assert_eq!(shown, ["<b>", "</b>", "\u{21b5}", "<i>", "</i>"]);
    assert_eq!(u.codes["1"].label.as_deref(), Some("bold"));
    assert_eq!(d.units[1].context.extra["settings"], "X1:100 X2:200");
    assert_eq!(text_of(&d, "c2"), "Top line &amp; more", "SRT text is not entity-decoded");
    assert_eq!(d.units[2].codes["1"].label.as_deref(), Some("font"));
    assert_eq!(d.units[3].context.key.as_deref(), Some("4"));
}

#[test]
fn srt_translation_keeps_tags_timing_and_breaks() {
    let mut d = srt(SRT);
    let mut t = d.units[0].segments[0].source.clone();
    for i in t.iter_mut() {
        if let Inline::Text(x) = i {
            *x = match x.as_str() {
                "Welcome to " => "Bienvenido a ".into(),
                "Larchmoor" => "Larchmoor".into(),
                "." => ".".into(),
                "The kettle is " => "La tetera está ".into(),
                "ready" => "lista".into(),
                o => o.into(),
            };
        }
    }
    d.set_target("c1", t).unwrap();
    d.set_target("c4", vec![Inline::text("Última, línea\ndos")]).unwrap();
    let out = String::from_utf8(Srt.merge(&d).unwrap()).unwrap();
    assert!(out.starts_with("1\n00:00:01,000 --> 00:00:03,500\nBienvenido a <b>Larchmoor</b>.\nLa tetera está <i>lista</i>.\n\n2\n"), "{out}");
    assert!(out.contains("\n\n\n\n4\n00:00:10,000 --> 00:00:11,000\nÚltima, línea\ndos"));
    assert!(out.contains("{\\an8}Top line &amp; more\n"));
}

#[test]
fn srt_crlf_and_bom() {
    let crlf = SRT.replace('\n', "\r\n");
    let mut d = srt(&crlf);
    assert_eq!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap(), crlf);
    // A line break tag is written with the file's own ending.
    let mut t = d.units[0].segments[0].source.clone();
    if let Inline::Text(x) = &mut t[0] {
        *x = "Hola ".into();
    }
    d.set_target("c1", t).unwrap();
    let out = String::from_utf8(Srt.merge(&d).unwrap()).unwrap();
    assert!(out.contains("Hola <b>Larchmoor</b>.\r\nThe kettle is <i>ready</i>.\r\n\r\n2\r\n"), "{out:?}");
    assert!(!out.replace("\r\n", "").contains('\n'));

    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(SRT.as_bytes());
    let d = Srt.extract(&bytes, &Default::default()).unwrap();
    assert_eq!(Srt.merge(&d).unwrap(), bytes);
}

#[test]
fn vtt_round_trip_and_units() {
    let d = vtt(VTT);
    assert_eq!(String::from_utf8(Vtt.merge(&d).unwrap()).unwrap(), VTT);
    assert_eq!(d.units.len(), 3, "NOTE, STYLE, REGION and header are skeleton");
    let u = &d.units[0];
    assert_eq!(u.context.key.as_deref(), Some("intro"));
    assert_eq!(u.context.extra["start"], "00:00:01.000");
    assert_eq!(u.context.extra["settings"], "align:start position:10%");
    assert_eq!(plain_text(&u.segments[0].source), "Welcome to Larchmoor & friends.Second line.");
    let shown: Vec<String> = u.segments[0]
        .source
        .iter()
        .filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None })
        .collect();
    assert_eq!(shown, ["<v Brent>", "<b>", "</b>", "\u{21b5}", "<i>", "</i>"]);
    assert_eq!(u.codes["1"].label.as_deref(), Some("voice"));
    // No identifier: the cue number is the key. Class span and timestamp.
    let second = &d.units[1];
    assert_eq!(second.context.key.as_deref(), Some("2"));
    let labels: Vec<&str> = second.codes.values().filter_map(|c| c.label.as_deref()).collect();
    assert!(labels.contains(&"class") && labels.contains(&"timestamp"), "{labels:?}");
    // Unclosed voice and bold become standalone tags.
    let third = &d.units[2];
    let kinds: Vec<TagKind> = third.segments[0].source.iter().filter_map(|i| if let Inline::Tag { kind, .. } = i { Some(*kind) } else { None }).collect();
    assert_eq!(kinds, [TagKind::Standalone, TagKind::Standalone]);
}

#[test]
fn vtt_translation_escapes_and_keeps_skeleton() {
    let mut d = vtt(VTT);
    let mut t = d.units[0].segments[0].source.clone();
    for i in t.iter_mut() {
        if let Inline::Text(x) = i {
            *x = match x.as_str() {
                "Welcome to " => "Bienvenue chez ".into(),
                " & friends." => " & les amis < tous.".into(),
                "Second " => "Deuxième ".into(),
                o => o.into(),
            };
        }
    }
    d.set_target("c1", t).unwrap();
    let out = String::from_utf8(Vtt.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<v Brent>Bienvenue chez <b>Larchmoor</b> &amp; les amis &lt; tous.\nDeuxième <i>line</i>.\n"), "{out}");
    assert!(out.starts_with("WEBVTT - Larchmoor demo\n\nSTYLE\n::cue(.loud) { color: red }\n\nNOTE This is a comment\nthat spans two lines\n\nintro\n00:00:01.000 --> 00:00:03.500 align:start position:10%\n"));
    assert!(out.contains("REGION\nid:r1\n"));
    // Foreign entities survive.
    let d2 = vtt("WEBVTT\n\n00:01.000 --> 00:02.000\nCaf&eacute; &lt;ok&gt;\n");
    assert_eq!(plain_text(&d2.units[0].segments[0].source), "Caf&eacute; <ok>");
    let mut d2 = d2;
    d2.set_target("c1", vec![Inline::text("Caf&eacute; <b>")]).unwrap();
    assert!(String::from_utf8(Vtt.merge(&d2).unwrap()).unwrap().contains("Caf&eacute; &lt;b>"));
}

#[test]
fn eleven_languages_round_trip_and_translate() {
    let lines = [
        ("en", "Welcome to the shop."),
        ("es", "Bienvenido a la tienda."),
        ("fr", "Bienvenue dans la boutique."),
        ("it", "Benvenuti nel negozio."),
        ("de", "Willkommen im Geschäft."),
        ("nl", "Welkom in de winkel."),
        ("pt", "Bem-vindo à loja."),
        ("ja", "お店へようこそ。"),
        ("zh", "欢迎光临本店。"),
        ("ko", "매장에 오신 것을 환영합니다."),
        ("ar", "مرحبا بكم في المتجر."),
    ];
    let mut srt_file = String::new();
    let mut vtt_file = String::from("WEBVTT\n\n");
    for (i, (_, l)) in lines.iter().enumerate() {
        srt_file.push_str(&format!("{}\n00:00:{:02},000 --> 00:00:{:02},500\n<i>{l}</i>\nSecond line\n\n", i + 1, i, i));
        vtt_file.push_str(&format!("00:{:02}.000 --> 00:{:02}.500\n<i>{l}</i>\nSecond line\n\n", i, i));
    }
    let d = srt(&srt_file);
    assert_eq!(d.units.len(), 11);
    assert_eq!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap(), srt_file);
    let d = vtt(&vtt_file);
    assert_eq!(d.units.len(), 11);
    assert_eq!(String::from_utf8(Vtt.merge(&d).unwrap()).unwrap(), vtt_file);
    for (i, (lang, l)) in lines.iter().enumerate() {
        assert_eq!(text_of(&d, &format!("c{}", i + 1)), format!("{l}Second line"), "{lang}");
    }
    // Translate every cue (swap the first text) and read it back.
    let mut d = d;
    for i in 0..11 {
        let id = format!("c{}", i + 1);
        let mut t = d.find(&id).unwrap().1.source.clone();
        for x in t.iter_mut() {
            if let Inline::Text(s) = x {
                s.push('!');
            }
        }
        d.set_target(&id, t).unwrap();
    }
    let again = vtt(&String::from_utf8(Vtt.merge(&d).unwrap()).unwrap());
    for (i, (_, l)) in lines.iter().enumerate() {
        assert_eq!(text_of(&again, &format!("c{}", i + 1)), format!("{l}!Second line!"));
    }
}

#[test]
fn odd_input() {
    // No cues at all.
    assert_eq!(srt("").units.len(), 0);
    assert_eq!(vtt("WEBVTT\n").units.len(), 0);
    // A cue with no text, a timing line without a number, and junk blocks are kept as skeleton.
    let s = "1\n00:00:01,000 --> 00:00:02,000\n\njunk block\n\n00:00:03,000 --> 00:00:04,000\nNo number\n";
    let d = srt(s);
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].context.key.as_deref(), Some("1"));
    assert_eq!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap(), s);
    // Not a WebVTT file.
    assert!(Vtt.extract(b"00:01.000 --> 00:02.000\nHi\n", &Default::default()).is_err());
    assert!(Srt.extract(&[0xff, 0xfe, 0x00], &Default::default()).is_err());
    // A literal less-than sign in SRT text is not a tag.
    let d = srt("1\n00:00:01,000 --> 00:00:02,000\nI <3 tea & 5 < 6\n");
    assert_eq!(text_of(&d, "c1"), "I <3 tea & 5 < 6");
    // Tags the model would reject are not invented by merge: dropping a tag drops its markup.
    let mut d = srt("1\n00:00:01,000 --> 00:00:02,000\nA <b>bold</b> word\n");
    d.set_target("c1", vec![Inline::text("Una palabra")]).unwrap();
    assert!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap().contains("\nUna palabra\n"));
    assert!(d.set_target("c1", vec![Inline::tag("9", TagKind::Open, "<b>")]).is_err());
}

#[test]
fn sniffing_and_ids() {
    assert!(Srt.sniff(SRT.as_bytes()));
    assert!(!Srt.sniff(VTT.as_bytes()));
    assert!(Vtt.sniff(VTT.as_bytes()));
    assert!(Vtt.sniff("\u{feff}WEBVTT".as_bytes()));
    assert!(!Vtt.sniff(SRT.as_bytes()));
    assert_eq!((Srt.id(), Vtt.id()), ("srt", "vtt"));
    assert_eq!(Srt.extensions(), ["srt"]);
    assert_eq!(Vtt.extensions(), ["vtt"]);
    assert_eq!(srt(SRT).format, "srt");
    assert_eq!(vtt(VTT).format, "vtt");
}

#[test]
fn every_prefix_of_a_file_is_safe() {
    for full in [SRT, VTT] {
        for end in 0..=full.len() {
            if !full.is_char_boundary(end) {
                continue;
            }
            let part = &full[..end];
            let (d, f): (Result<Document, Error>, &dyn Filter) = if full == SRT {
                (Srt.extract(part.as_bytes(), &Default::default()), &Srt)
            } else {
                (Vtt.extract(part.as_bytes(), &Default::default()), &Vtt)
            };
            if let Ok(d) = d {
                assert_eq!(f.merge(&d).unwrap(), part.as_bytes(), "prefix {end}");
            }
        }
    }
}

#[test]
fn cr_cr_lf_line_endings_keep_a_cue_in_one_piece() {
    // a file saved twice has CR CR LF endings; a cue is still one cue, and a longer translation stays in it
    let s = "1\r\r\n00:00:01,000 --> 00:00:03,000\r\r\nFirst line\r\r\nsecond line\r\r\n\r\r\n2\r\r\n00:00:04,000 --> 00:00:05,000\r\r\nOnly\r\r\n";
    let mut d = srt(s);
    assert_eq!(d.units.len(), 2);
    assert_eq!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap(), s);
    let id = d.units[0].segments[0].id.clone();
    let mut t = d.units[0].segments[0].source.clone();
    t.insert(0, Inline::text("Zq"));
    t.push(Inline::text("qZ"));
    d.set_target(&id, t).unwrap();
    let out = Srt.merge(&d).unwrap();
    let back = srt(&String::from_utf8(out).unwrap());
    assert_eq!(back.units.len(), 2);
    assert!(plain_text(&back.units[0].segments[0].source).starts_with("ZqFirst line"));
}

#[test]
fn srt_word_timing_marks_are_codes_not_text() {
    // YouTube-style auto captions: <hh:mm:ss.mmm> before each word
    let s = "1\n00:00:04,160 --> 00:00:08,070\nWe are <00:00:04.480>ourselves.\n\n2\n00:00:09,000 --> 00:00:10,000\nStays <1:2> text and <b>bold</b>\n";
    let mut d = srt(s);
    assert_eq!(String::from_utf8(Srt.merge(&d).unwrap()).unwrap(), s);
    assert_eq!(text_of(&d, "c1"), "We are ourselves.");
    let stamp = d.units[0].codes.values().find(|c| c.label.as_deref() == Some("timestamp")).expect("a timestamp code");
    assert_eq!(stamp.standalone.as_deref(), Some("<00:00:04.480>"));
    // something that only looks like a stamp stays text
    assert_eq!(text_of(&d, "c2"), "Stays <1:2> text and bold");
    let mut t = d.units[0].segments[0].source.clone();
    for i in t.iter_mut() {
        if let Inline::Text(x) = i {
            if x == "We are " {
                *x = "Wir sind ".into();
            }
        }
    }
    d.set_target("c1", t).unwrap();
    let out = String::from_utf8(Srt.merge(&d).unwrap()).unwrap();
    assert!(out.contains("Wir sind <00:00:04.480>ourselves."), "{out}");
}
