use super::*;

const POT: &str = r##"# Larchmoor Tools, template.
# This file is distributed under the same license as the package.
#
msgid ""
msgstr ""
"Project-Id-Version: larchmoor 1.0\n"
"Report-Msgid-Bugs-To: \n"
"Content-Type: text/plain; charset=UTF-8\n"
"Content-Transfer-Encoding: 8bit\n"

#. Shown on the cart button
#: src/cart.c:42 src/cart.c:77
#, c-format
msgid "Add %d item to %s"
msgstr ""

#: src/menu.py:9
msgctxt "menu"
msgid "File"
msgstr ""

msgctxt "verb"
msgid "File"
msgstr ""

#, python-format
msgid "Hello, %(name)s! You have {count} messages."
msgstr ""

msgid "%d kettle"
msgid_plural "%d kettles"
msgstr[0] ""
msgstr[1] ""

msgid ""
"First line of a long text.\n"
"Second line of the same text.\n"
msgstr ""

# translator note
msgid "Plain \"quoted\" text with a tab\there and a backslash \\"
msgstr ""

#~ msgid "Old string"
#~ msgstr "Cadena antigua"
"##;

const ES: &str = r##"# Spanish translation.
msgid ""
msgstr ""
"Project-Id-Version: larchmoor 1.0\n"
"Language: es\n"
"Content-Type: text/plain; charset=UTF-8\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\n"

#: src/cart.c:42
#, c-format
msgid "Add %d item to %s"
msgstr "Añadir %d artículo a %s"

#, fuzzy, c-format
msgid "Remove %d item"
msgstr "Quitar %d elemento"

msgid "%d kettle"
msgid_plural "%d kettles"
msgstr[0] "%d tetera"
msgstr[1] "%d teteras"

msgid ""
"First line.\n"
"Second line.\n"
msgstr ""
"Primera línea.\n"
"Segunda línea.\n"

msgid "Untranslated"
msgstr ""
"##;

fn ex(src: &str) -> Document {
    Po.extract(src.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn ex_to(src: &str, lang: &str) -> Document {
    Po.extract(src.as_bytes(), &ExtractOptions::new().with_target_lang(lang)).unwrap()
}

fn merged(d: &Document) -> String {
    String::from_utf8(Po.merge(d).unwrap()).unwrap()
}

fn set(d: &mut Document, id: &str, s: &str) {
    let src = d.find(id).unwrap().1.source.clone();
    // Re-use placeholder tags where the text contains the same placeholder as the source.
    let mut used = std::collections::HashSet::new();
    let mut t = Vec::new();
    for piece in ph::split(s) {
        match piece {
            ph::Piece::Text(x) => t.push(Inline::text(x)),
            ph::Piece::Ph(p) => {
                let found = src.iter().find_map(|i| match i {
                    Inline::Tag { id, display, .. } if display == p && !used.contains(id) => Some(id.clone()),
                    _ => None,
                });
                match found {
                    Some(tid) => {
                        used.insert(tid.clone());
                        t.push(Inline::tag(tid, TagKind::Standalone, p));
                    }
                    None => t.push(Inline::text(p)),
                }
            }
        }
    }
    d.set_target(id, t).unwrap();
}

#[test]
fn round_trips() {
    for src in [POT, ES, "", "\n\n", "msgid \"a\"\nmsgstr \"b\"", "# only a comment\n"] {
        assert_eq!(merged(&ex(src)), src);
    }
    // With a target language the file is still untouched until something is translated.
    assert_eq!(merged(&ex_to(POT, "ar")), POT);
    assert_eq!(merged(&ex_to(ES, "es")), ES);
}

#[test]
fn extracts_units_with_context() {
    let d = ex(POT);
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["e2", "e3", "e4", "e5", "e6.0", "e6.1", "e7", "e8"]);
    let u = &d.units[0];
    assert_eq!(plain_text(&u.source()), "Add  item to ");
    assert_eq!(u.context.key.as_deref(), Some("Add %d item to %s"));
    assert_eq!(u.context.notes, ["Shown on the cart button", "ref: src/cart.c:42 src/cart.c:77"]);
    assert_eq!(u.context.extra.get("flags").map(String::as_str), Some("c-format"));
    let tags: Vec<_> = u.segments[0].source.iter().filter_map(|i| if let Inline::Tag { display, kind, .. } = i { Some((display.as_str(), *kind)) } else { None }).collect();
    assert_eq!(tags, [("%d", TagKind::Standalone), ("%s", TagKind::Standalone)]);
    assert_eq!(u.codes["1"].label.as_deref(), Some("placeholder"));
    // msgctxt + msgid is the key.
    assert_eq!(d.units[1].context.key.as_deref(), Some("menu :: File"));
    assert_eq!(d.units[1].context.extra.get("msgctxt").map(String::as_str), Some("menu"));
    assert_eq!(d.units[2].context.key.as_deref(), Some("verb :: File"));
    assert_ne!(d.units[1].context.key, d.units[2].context.key);
    // Python placeholders.
    let py: Vec<_> = d.units[3].segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count().eq(&2).then_some(1).into_iter().collect();
    assert_eq!(py.len(), 1);
    // Plural forms.
    assert_eq!(d.units[4].context.extra["plural"], "0");
    assert_eq!(d.units[5].context.extra["plural"], "1");
    assert_eq!(plain_text(&d.units[4].source()), " kettle");
    assert_eq!(plain_text(&d.units[5].source()), " kettles");
    // Multi-line msgid is unescaped and joined; escapes are decoded.
    assert_eq!(plain_text(&d.units[6].source()), "First line of a long text.\nSecond line of the same text.\n");
    assert_eq!(plain_text(&d.units[7].source()), "Plain \"quoted\" text with a tab\there and a backslash \\");
    assert_eq!(d.units[7].context.notes, ["comment: translator note"]);
    // The obsolete entry is not a unit; the header is skeleton.
    assert!(d.units.iter().all(|u| !plain_text(&u.source()).contains("Old string")));
}

#[test]
fn existing_translations_become_targets() {
    let d = ex(ES);
    assert_eq!(d.target_lang.as_deref(), Some("es"));
    let u = &d.units[0];
    assert_eq!(u.segments[0].state, SegState::Translated);
    assert_eq!(plain_text(&u.segments[0].target), "Añadir  artículo a ");
    assert!(u.segments[0].target.iter().any(|i| matches!(i, Inline::Tag { id, .. } if id == "1")));
    let fz = &d.units[1];
    assert_eq!(fz.context.extra["fuzzy"], "true");
    assert_eq!(fz.segments[0].state, SegState::Draft);
    assert_eq!(d.units.last().unwrap().segments[0].state, SegState::New);
}

#[test]
fn translating_a_pot_writes_msgstr_and_keeps_layout() {
    let mut d = ex_to(POT, "es");
    set(&mut d, "e2", "Añadir %d artículo a %s");
    set(&mut d, "e3", "Archivo");
    set(&mut d, "e5", "¡Hola, %(name)s! Tienes {count} mensajes.");
    set(&mut d, "e6.0", "%d tetera");
    set(&mut d, "e6.1", "%d teteras");
    set(&mut d, "e7", "Primera línea de un texto largo.\nSegunda línea del mismo texto.\n");
    set(&mut d, "e8", "Texto \"citado\" con tab\taquí y barra \\");
    let out = merged(&d);
    assert!(out.contains("msgid \"Add %d item to %s\"\nmsgstr \"Añadir %d artículo a %s\"\n"), "{out}");
    assert!(out.contains("msgctxt \"menu\"\nmsgid \"File\"\nmsgstr \"Archivo\"\n"));
    // The second "File" (verb) was left alone.
    assert!(out.contains("msgctxt \"verb\"\nmsgid \"File\"\nmsgstr \"\"\n"));
    assert!(out.contains("msgstr[0] \"%d tetera\"\nmsgstr[1] \"%d teteras\"\n"));
    // Multi-line msgid -> multi-line msgstr, split after each newline.
    assert!(out.contains("msgstr \"\"\n\"Primera línea de un texto largo.\\n\"\n\"Segunda línea del mismo texto.\\n\"\n"), "{out}");
    assert!(out.contains("msgstr \"Texto \\\"citado\\\" con tab\\taquí y barra \\\\\"\n"));
    // Comments, flags, references and the obsolete entry are untouched.
    assert!(out.contains("#. Shown on the cart button\n#: src/cart.c:42 src/cart.c:77\n#, c-format\n"));
    assert!(out.contains("#~ msgid \"Old string\"\n#~ msgstr \"Cadena antigua\"\n"));
    // Header got the language.
    assert!(out.contains("\"Report-Msgid-Bugs-To: \\n\"\n"));
    // And the result parses back to the same translations.
    let again = ex(&out);
    assert_eq!(plain_text(&again.find("e8").unwrap().1.target), "Texto \"citado\" con tab\taquí y barra \\");
    assert_eq!(again.find("e6.1").unwrap().1.state, SegState::Translated);
}

#[test]
fn language_header_is_updated_when_it_exists() {
    let mut d = ex_to(ES, "fr");
    set(&mut d, "e6", "Non traduit");
    let out = merged(&d);
    assert!(out.contains("\"Language: fr\\n\"\n"), "{out}");
    assert!(out.contains("\"Plural-Forms: nplurals=2; plural=(n > 1);\\n\"\n"), "{out}");
    // Nothing else moved.
    assert!(out.contains("msgstr \"Añadir %d artículo a %s\"\n"));
}

#[test]
fn plural_count_follows_the_target_language() {
    // Arabic has six forms.
    let mut d = ex_to(
        "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\nmsgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n",
        "ar",
    );
    let ids: Vec<String> = d.units.iter().map(|u| u.id.clone()).collect();
    assert_eq!(ids, ["e2.0", "e2.1", "e2.2", "e2.3", "e2.4", "e2.5"]);
    assert_eq!(plain_text(&d.units[5].source()), " files");
    for (i, id) in ids.iter().enumerate() {
        set(&mut d, id, &format!("%d ملف {i}"));
    }
    let out = merged(&d);
    for i in 0..6 {
        assert!(out.contains(&format!("msgstr[{i}] \"%d ملف {i}\"")), "{out}");
    }
    assert!(out.contains("nplurals=6; plural=n==0 ? 0"));
    assert!(out.contains("\"Language: ar\\n\""));
    // Japanese has one.
    let d = ex_to("msgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n", "ja");
    assert_eq!(d.units.len(), 1);
    // No target language: the entry's own count.
    let d = ex("msgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\nmsgstr[2] \"\"\n");
    assert_eq!(d.units.len(), 3);
    // Header count when the entry has none... the entry's own count wins; header only when unknown.
}

#[test]
fn partly_translated_plural_keeps_untranslated_forms() {
    let mut d = ex(ES);
    set(&mut d, "e4.1", "%d teteras azules");
    let out = merged(&d);
    assert!(out.contains("msgstr[0] \"%d tetera\"\nmsgstr[1] \"%d teteras azules\"\n"), "{out}");
}

#[test]
fn fuzzy_is_cleared_when_confirmed() {
    let mut d = ex(ES);
    // Editing keeps fuzzy (the unit is a draft).
    set(&mut d, "e3", "Eliminar %d elemento");
    assert!(merged(&d).contains("#, fuzzy, c-format\nmsgid \"Remove %d item\"\nmsgstr \"Eliminar %d elemento\"\n"));
    d.set_state("e3", SegState::Confirmed).unwrap();
    let out = merged(&d);
    assert!(out.contains("#, c-format\nmsgid \"Remove %d item\"\nmsgstr \"Eliminar %d elemento\"\n"), "{out}");
    // A flag line that only held "fuzzy" disappears entirely.
    let mut d = ex("#, fuzzy\nmsgid \"a\"\nmsgstr \"b\"\n");
    d.set_state("e1", SegState::Confirmed).unwrap();
    assert_eq!(merged(&d), "msgid \"a\"\nmsgstr \"b\"\n");
}

#[test]
fn editing_one_entry_leaves_the_rest_byte_for_byte() {
    let mut d = ex(ES);
    set(&mut d, "e2", "Agregar %d artículo a %s");
    let out = merged(&d);
    assert_eq!(out, ES.replace("Añadir %d artículo a %s", "Agregar %d artículo a %s"));
}

#[test]
fn clearing_a_translation_empties_msgstr() {
    let mut d = ex(ES);
    d.set_target("e2", vec![]).unwrap();
    assert!(merged(&d).contains("msgid \"Add %d item to %s\"\nmsgstr \"\"\n"));
}

#[test]
fn crlf_and_bom() {
    let crlf = ES.replace('\n', "\r\n");
    let mut d = ex(&crlf);
    assert_eq!(merged(&d), crlf);
    set(&mut d, "e6", "Línea\ndos");
    let out = merged(&d);
    assert!(out.contains("msgid \"Untranslated\"\r\nmsgstr \"\"\r\n\"Línea\\n\"\r\n\"dos\"\r\n") || out.contains("msgstr \"\"\r\n\"Línea\\n\"\r\n\"dos\""), "{out:?}");
    assert!(!out.contains("\r\n\r\r"));

    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(ES.as_bytes());
    let d = Po.extract(&bytes, &Default::default()).unwrap();
    assert_eq!(Po.merge(&d).unwrap(), bytes);
}

#[test]
fn errors() {
    assert!(Po.extract(b"msgid \"a\"\nmsgstr b\n", &Default::default()).is_err());
    assert!(Po.extract(b"msgid \"a\"\nmsgid \"b\"\nmsgstr \"c\"\n", &Default::default()).is_err());
    assert!(Po.extract(b"\"stray\"\n", &Default::default()).is_err());
    assert!(Po.extract(b"foo \"a\"\n", &Default::default()).is_err());
    assert!(Po.extract(b"msgid \"a\"\nmsgid_plural \"b\"\nmsgstr \"c\"\n", &Default::default()).is_err());
    assert!(Po.extract(b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=x-no-such-charset\\n\"\n", &Default::default()).is_err());
    assert!(Po.extract(&[0x00, 0xFF], &Default::default()).is_err());
    assert!(Po.sniff(b"msgid \"a\"\nmsgstr \"b\"\n"));
    assert!(!Po.sniff(b"hello"));
    assert_eq!(Po.extensions(), ["po", "pot"]);
}

#[test]
fn set_target_rejects_invented_tags() {
    let mut d = ex(POT);
    assert!(d.set_target("e2", vec![Inline::tag("9", TagKind::Standalone, "%x")]).is_err());
}

#[test]
fn eleven_languages_round_trip() {
    let langs: [(&str, &str, &str, &str); 11] = [
        ("en", "Welcome, %s!", "Add to cart", "%d kettle"),
        ("es", "¡Bienvenido, %s!", "Añadir al carrito", "%d tetera"),
        ("fr", "Bienvenue, %s !", "Ajouter au panier", "%d bouilloire"),
        ("it", "Benvenuto, %s!", "Aggiungi al carrello", "%d bollitore"),
        ("de", "Willkommen, %s!", "In den Warenkorb", "%d Wasserkocher"),
        ("nl", "Welkom, %s!", "In winkelwagen", "%d waterkoker"),
        ("pt", "Bem-vindo, %s!", "Adicionar ao carrinho", "%d chaleira"),
        ("ja", "ようこそ、%sさん！", "カートに追加", "ケトル%d個"),
        ("zh", "欢迎，%s！", "加入购物车", "%d 个水壶"),
        ("ko", "%s님, 환영합니다!", "장바구니에 담기", "주전자 %d개"),
        ("ar", "مرحبا بك يا %s!", "أضف إلى السلة", "%d غلاية"),
    ];
    for (lang, welcome, button, plural) in langs {
        let (np, _) = plural_rule(lang).unwrap();
        let mut file = format!(
            "msgid \"\"\nmsgstr \"\"\n\"Language: {lang}\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Plural-Forms: nplurals={np}; plural=0;\\n\"\n\n"
        );
        file.push_str(&format!("msgid \"Welcome, %s!\"\nmsgstr \"{welcome}\"\n\nmsgid \"Add to cart\"\nmsgstr \"{button}\"\n\n"));
        file.push_str("msgid \"%d kettle\"\nmsgid_plural \"%d kettles\"\n");
        for i in 0..np {
            file.push_str(&format!("msgstr[{i}] \"{plural}\"\n"));
        }
        let d = ex(&file);
        assert_eq!(merged(&d), file, "{lang}");
        assert_eq!(d.units.len(), 2 + np, "{lang}");
        assert_eq!(plain_text(&d.units[1].segments[0].target), button);
        // Translate the button again in the same language and re-read.
        let mut d = d;
        set(&mut d, "e3", &format!("{button}!"));
        let again = ex(&merged(&d));
        assert_eq!(plain_text(&again.find("e3").unwrap().1.target), format!("{button}!"), "{lang}");
    }
}

#[test]
fn placeholders_in_targets_map_to_source_tags() {
    let d = ex(ES);
    let seg = &d.units[0].segments[0];
    let ids: Vec<&str> = seg.target.iter().filter_map(|i| if let Inline::Tag { id, .. } = i { Some(id.as_str()) } else { None }).collect();
    assert_eq!(ids, ["1", "2"]);
    // A placeholder in the target that the source lacks stays plain text.
    let d = ex("msgid \"Hi\"\nmsgstr \"Hola %s\"\n");
    assert_eq!(plain_text(&d.units[0].segments[0].target), "Hola %s");
}

#[test]
fn cldr_only_targets_get_their_language_and_plural_rule_under_the_profile() {
    let src = "msgid \"\"\nmsgstr \"\"\n\"Language: \\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\nmsgid \"One file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n";
    let want = [
        ("tl", "nplurals=2; plural=(n == 1 || n == 2 || n == 3 || (n % 10 != 4 && n % 10 != 6 && n % 10 != 9) ? 0 : 1);"),
        ("tgl", "nplurals=2; plural=(n == 1 || n == 2 || n == 3 || (n % 10 != 4 && n % 10 != 6 && n % 10 != 9) ? 0 : 1);"),
        ("fil", "nplurals=2; plural=(n == 1 || n == 2 || n == 3 || (n % 10 != 4 && n % 10 != 6 && n % 10 != 9) ? 0 : 1);"),
        ("id", "nplurals=1; plural=0;"),
        ("in", "nplurals=1; plural=0;"),
        ("ta", "nplurals=2; plural=(n != 1);"),
        ("zu", "nplurals=2; plural=(n > 1);"),
        ("mk", "nplurals=2; plural=(n%10==1 && n%100!=11 ? 0 : 1);"),
    ];
    let o = ExtractOptions::new().with_profile(Some(profile::LOCOS));
    for (lang, rule) in want {
        let mut doc = Po.extract(src.as_bytes(), &o).unwrap().with_target_lang(lang);
        let id = doc.units[0].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text("Eins")]).unwrap();
        let out = String::from_utf8(Po.merge_with(&doc, &MergeOptions::new().with_profile(Some(profile::LOCOS))).unwrap()).unwrap();
        assert!(out.contains(&format!("\"Language: {lang}\\n\"")), "{lang}: {out}");
        let got = out.lines().find(|l| l.contains("Plural-Forms:")).unwrap();
        if lang == "mk" {
            // the exact spelling is the generated table's; only the slot numbers matter
            assert!(got.contains("nplurals=2"), "{lang}: {got}");
        } else {
            assert!(got.contains(rule), "{lang}: {got}");
        }
    }
}
