//! LocOS's JSON dialect suite (`tests/verify_json_dialects_filter.py`) in Rust: Chrome, FormatJS, Structured JSON,
//! go-i18n, ARB-shaped files, i18next plural fill, and the hostile inputs. Each test works the way the LocOS
//! adapter does: extract under the `locos` profile, turn the units into LocOS text with its `⟦xN⟧` tokens, set
//! translations from LocOS text, merge with the profile's merge options.

use bilingua_json::JsonFilter;
use bilingua_model::profile::{self, tokens};
use bilingua_model::*;

const ARB: &str = r#"{
  "@@locale": "en",
  "@@last_modified": "2026-10-10T09:00:00",
  "title": "Hello {name}",
  "@title": {
    "description": "Greeting on the home page",
    "placeholders": {"name": {"type": "String", "example": "Bob"}}
  },
  "items": "{count, plural, =0{No items} one{# item} other{# items}}",
  "@items": {"description": "Shown under the cart"},
  "mixed": "You have {count, plural, one{# new message} other{# new messages}} from {who}",
  "sel": "{gender, select, male{He liked it} female{She liked it} other{They liked it}}",
  "rank": "{n, selectordinal, one{#st place} other{#th place}}",
  "quoted": "Use '{braces}' and it''s fine",
  "id": "4711"
}
"#;

const CHROME: &str = r#"{
  "extName": {
    "message": "Tab Saver",
    "description": "Name of the extension, shown in the store"
  },
  "greet": {
    "message": "Hello $USER$, you have $1 open tabs",
    "description": "Toolbar greeting",
    "placeholders": {
      "user": {"content": "$1", "example": "Cira"}
    }
  }
}
"#;

const FORMATJS: &str = r#"{
  "cart.count": {
    "defaultMessage": "{itemCount, plural, one{# item} other{# items}} in your cart",
    "description": "Cart badge"
  },
  "app.welcome": {
    "defaultMessage": "Welcome back, {name}!",
    "description": {"context": "home", "note": "keep it short"}
  }
}
"#;

const STRUCTURED: &str = r#"{
  "home": {
    "title": {
      "string": "Welcome",
      "context": "page header",
      "developer_comment": "Keep it short",
      "character_limit": 20
    },
    "files": {"string": "{n, plural, one{# file} other{# files}}"}
  }
}
"#;

const GO: &str = r#"{
  "PersonCats": {
    "description": "The number of cats a person has",
    "hash": "sha1-abc",
    "one": "{{.Name}} has {{.Count}} cat.",
    "other": "{{.Name}} has {{.Count}} cats."
  },
  "HelloPerson": "Hello {{.Name}}"
}
"#;

const GO_V1: &str = r#"[
  {"id": "d_days", "translation": {"one": "{{.Count}} day", "other": "{{.Count}} days"}},
  {"id": "hello", "translation": "Hello"}
]
"#;

const PLAIN: &str = r#"{"title": "Hello", "meta": {"description": "A description", "count": "{count}"}}"#;

fn locos_opts() -> ExtractOptions {
    profile::locos().extract_options()
}

/// What the adapter does after the filter: the profile's annotations.
fn extract_with(src: &str, opts: &ExtractOptions) -> Document {
    let mut d = JsonFilter.extract(src.as_bytes(), opts).unwrap();
    profile::annotate(&mut d, Some("locos"));
    d
}

fn extract(src: &str) -> Document {
    extract_with(src, &locos_opts())
}

struct U {
    p: usize,
    seg: String,
    text: String,
    map: tokens::LocosMap,
}

fn locos_units(d: &Document) -> Vec<U> {
    let mut v = Vec::new();
    for (i, u) in d.units.iter().enumerate() {
        for s in &u.segments {
            let (text, map) = tokens::to_locos_text_for("json", &s.source);
            let p = u.context.extra.get(profile::P_INDEX).and_then(|p| p.parse().ok()).unwrap_or(i);
            v.push(U { p, seg: s.id.clone(), text, map });
        }
    }
    v
}

fn units(src: &str) -> Vec<String> {
    locos_units(&extract(src)).into_iter().map(|u| u.text).collect()
}

/// Merges with `map` (p_index to LocOS text), for the target locale, as the adapter does.
fn build(src: &str, map: &[(usize, String)], locale: Option<&str>) -> String {
    let opts = locos_opts().with_target_lang(locale.map(|l| l.to_string()));
    let mut d = extract_with(src, &opts);
    for u in locos_units(&d) {
        if let Some((_, t)) = map.iter().find(|(p, _)| *p == u.p) {
            profile::locos_set_target(&mut d, &u.seg, tokens::from_locos_text(t, &u.map)).unwrap();
        }
    }
    String::from_utf8(bilingua_model::Filter::merge_with(&JsonFilter, &d, &profile::locos().merge_options("json")).unwrap()).unwrap()
}

fn all_tr(src: &str, prefix: &str) -> Vec<(usize, String)> {
    locos_units(&extract(src)).into_iter().map(|u| (u.p, format!("{prefix}{}", u.text))).collect()
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

#[test]
fn detection_is_strict() {
    // a partial shape stays the generic walk: every string is a unit, descriptions included
    let generic = |s: &str| {
        let d = extract(s);
        d.units.iter().all(|u| !u.context.extra.contains_key("json.dialect"))
    };
    let dialect = |s: &str| -> Option<String> { extract(s).units.first().and_then(|u| u.context.extra.get("json.dialect").cloned()) };
    assert_eq!(dialect(CHROME).as_deref(), Some("chrome"));
    assert_eq!(dialect(FORMATJS).as_deref(), Some("formatjs"));
    assert_eq!(dialect(STRUCTURED).as_deref(), Some("structured"));
    assert_eq!(dialect(GO).as_deref(), Some("go"));
    assert_eq!(dialect(GO_V1).as_deref(), Some("go"));
    assert_eq!(dialect(ARB).as_deref(), Some("arb"));
    for s in [PLAIN, r#"{"a": "Hello", "b": "World"}"#, r#"["a", "b"]"#, r#"{"a": {"message": "x"}, "b": "y"}"#, r#"{"a": {"message": "x", "foo": "y"}}"#, "\"just a string\""] {
        assert!(generic(s), "{s}");
    }
}

#[test]
fn arb_units_skip_metadata() {
    let got = units(ARB);
    assert_eq!(
        got,
        vec![
            "Hello {name}",
            "No items",
            "\u{27E6}x1\u{27E7} item",
            "\u{27E6}x1\u{27E7} items",
            "You have \u{27E6}x1\u{27E7} from {who}",
            "\u{27E6}x1\u{27E7} new message",
            "\u{27E6}x1\u{27E7} new messages",
            "He liked it",
            "She liked it",
            "They liked it",
            "\u{27E6}x1\u{27E7}st place",
            "\u{27E6}x1\u{27E7}th place",
            "Use '{braces}' and it''s fine",
            "4711",
        ]
    );
    let joined = got.join(" ");
    for meta in ["Greeting on the home page", "Shown under the cart", "Bob", "2026-10-10"] {
        assert!(!joined.contains(meta), "{meta}");
    }
}

#[test]
fn untouched_files_merge_to_their_own_bytes() {
    for src in [ARB, CHROME, FORMATJS, STRUCTURED, GO, GO_V1, PLAIN] {
        let d = extract(src);
        let out = Filter::merge(&JsonFilter, &d).unwrap();
        assert_eq!(out, src.as_bytes());
        // translations equal to the source are byte-identical too
        let same: Vec<(usize, String)> = locos_units(&d).into_iter().map(|u| (u.p, u.text)).collect();
        assert_eq!(build(src, &same, None), src, "same text");
        assert_eq!(build(src, &[], None), src);
    }
    let bom = format!("\u{feff}{CHROME}");
    let d = extract(&bom);
    assert_eq!(Filter::merge(&JsonFilter, &d).unwrap(), bom.as_bytes());
    let out = build(&bom, &[(0, "Tab Retter".into())], None);
    assert!(out.starts_with('\u{feff}') && out.contains("Tab Retter"));
}

#[test]
fn arb_translated_and_locale() {
    let tr = all_tr(ARB, "T:");
    let ru = json(&build(ARB, &tr, Some("ru-RU")));
    assert_eq!(ru["@@locale"], "ru_RU");
    assert_eq!(ru["@@last_modified"], "2026-10-10T09:00:00");
    assert_eq!(ru["@title"]["description"], "Greeting on the home page");
    assert_eq!(ru["title"], "T:Hello {name}");
    assert_eq!(ru["items"], "{count, plural, =0{T:No items} one{T:# item} few{T:# items} many{T:# items} other{T:# items}}");
    assert_eq!(
        ru["mixed"],
        "T:You have {count, plural, one{T:# new message} few{T:# new messages} many{T:# new messages} other{T:# new messages}} from {who}"
    );
    assert_eq!(ru["sel"], "{gender, select, male{T:He liked it} female{T:She liked it} other{T:They liked it}}");
    assert_eq!(ru["id"], "T:4711");
    let ja = json(&build(ARB, &tr, Some("ja")));
    assert_eq!(ja["items"], "{count, plural, =0{T:No items} other{T:# items}}");
    assert_eq!(ja["rank"], "{n, selectordinal, other{T:#th place}}");
    assert_eq!(ja["@@locale"], "ja");
    let en = json(&build(ARB, &tr, Some("en")));
    assert_eq!(en["rank"], "{n, selectordinal, one{T:#st place} two{T:#th place} few{T:#th place} other{T:#th place}}");
    let ar = json(&build(ARB, &tr, Some("ar")));
    assert_eq!(ar["items"].as_str().unwrap().matches("{T:").count(), 7);
    // no locale: no switch, no category change
    let plain = json(&build(ARB, &tr, None));
    assert_eq!(plain["@@locale"], "en");
    assert_eq!(plain["items"], "{count, plural, =0{T:No items} one{T:# item} other{T:# items}}");
}

#[test]
fn arb_locale_is_added_as_the_first_entry_and_forced_arb_needs_the_shape() {
    // a file without @@locale is generic JSON to LocOS; forcing the ARB dialect on it is refused
    let src = "{
    \"hello\": \"Hello\",
    \"@hello\": {\"description\": \"d\"}
}
";
    let err = JsonFilter.extract(src.as_bytes(), &locos_opts().with_format_option("json.dialect", "arb"));
    assert!(matches!(err, Err(Error::FormatOption { .. })));
    // an ARB-shaped file whose @@locale is the only key still gets the target
    let only = "{
  \"@@locale\": \"en\"
}
";
    assert_eq!(build(only, &[], Some("de-DE")), "{
  \"@@locale\": \"de_DE\"
}
");
}

#[test]
fn chrome() {
    let got = units(CHROME);
    assert_eq!(got, vec!["Tab Saver", "Hello \u{27E6}x1\u{27E7}, you have \u{27E6}x2\u{27E7} open tabs"]);
    let out = build(CHROME, &[(0, "Tab-Retter".into()), (1, "Hallo \u{27E6}x1\u{27E7}, du hast \u{27E6}x2\u{27E7} offene Tabs".into())], None);
    let v = json(&out);
    assert_eq!(v["greet"]["message"], "Hallo $USER$, du hast $1 offene Tabs");
    assert_eq!(v["greet"]["description"], "Toolbar greeting");
    assert_eq!(v["greet"]["placeholders"]["user"], json(r#"{"content": "$1", "example": "Cira"}"#));
    assert_eq!(out.replace("Tab-Retter", "Tab Saver").replace("Hallo $USER$, du hast $1 offene Tabs", "Hello $USER$, you have $1 open tabs"), CHROME);
    // an invented token falls back to the source message
    let bad = json(&build(CHROME, &[(1, "Hallo \u{27E6}x9\u{27E7}".into())], None));
    assert_eq!(bad["greet"]["message"], "Hello $USER$, you have $1 open tabs");
}

#[test]
fn formatjs() {
    let got = units(FORMATJS);
    assert_eq!(got, vec!["\u{27E6}x1\u{27E7} in your cart", "\u{27E6}x1\u{27E7} item", "\u{27E6}x1\u{27E7} items", "Welcome back, {name}!"]);
    let tr = vec![
        (0, "\u{412} \u{43A}\u{43E}\u{440}\u{437}\u{438}\u{43D}\u{435}: \u{27E6}x1\u{27E7}".to_string()),
        (1, "\u{27E6}x1\u{27E7} \u{442}\u{43E}\u{432}\u{430}\u{440}".to_string()),
        (2, "\u{27E6}x1\u{27E7} \u{442}\u{43E}\u{432}\u{430}\u{440}\u{43E}\u{432}".to_string()),
        (3, "\u{421} \u{432}\u{43E}\u{437}\u{432}\u{440}\u{430}\u{449}\u{435}\u{43D}\u{438}\u{435}\u{43C}, {name}!".to_string()),
    ];
    let out = json(&build(FORMATJS, &tr, Some("ru")));
    assert_eq!(
        out["cart.count"]["defaultMessage"],
        "\u{412} \u{43A}\u{43E}\u{440}\u{437}\u{438}\u{43D}\u{435}: {itemCount, plural, one{# \u{442}\u{43E}\u{432}\u{430}\u{440}} few{# \u{442}\u{43E}\u{432}\u{430}\u{440}\u{43E}\u{432}} many{# \u{442}\u{43E}\u{432}\u{430}\u{440}\u{43E}\u{432}} other{# \u{442}\u{43E}\u{432}\u{430}\u{440}\u{43E}\u{432}}}"
    );
    assert_eq!(out["cart.count"]["description"], "Cart badge");
    assert_eq!(out["app.welcome"]["description"], json(r#"{"context": "home", "note": "keep it short"}"#));
    // the outer unit dropped the plural token: the source outer text is kept (no lost branch)
    let lost = json(&build(FORMATJS, &[(0, "In der Karre".into())], None));
    assert!(lost["cart.count"]["defaultMessage"].as_str().unwrap().starts_with("{itemCount, plural,"));
    // unbalanced braces in a translation would break ICU: source kept
    let brk = json(&build(FORMATJS, &[(3, "Willkommen {name".into())], None));
    assert_eq!(brk["app.welcome"]["defaultMessage"], "Welcome back, {name}!");
}

#[test]
fn structured() {
    let got = units(STRUCTURED);
    assert_eq!(got, vec!["Welcome", "\u{27E6}x1\u{27E7} file", "\u{27E6}x1\u{27E7} files"]);
    let out = json(&build(STRUCTURED, &[(0, "Bienvenue".into())], None));
    assert_eq!(out["home"]["title"], json(r#"{"string": "Bienvenue", "context": "page header", "developer_comment": "Keep it short", "character_limit": 20}"#));
}

#[test]
fn go_i18n() {
    assert_eq!(units(GO), vec!["\u{27E6}x1\u{27E7} has \u{27E6}x2\u{27E7} cat.", "\u{27E6}x1\u{27E7} has \u{27E6}x2\u{27E7} cats.", "Hello \u{27E6}x1\u{27E7}"]);
    let tr = vec![
        (0, "\u{423} \u{27E6}x1\u{27E7} \u{27E6}x2\u{27E7} \u{43A}\u{43E}\u{448}\u{43A}\u{430}.".to_string()),
        (1, "\u{423} \u{27E6}x1\u{27E7} \u{27E6}x2\u{27E7} \u{43A}\u{43E}\u{448}\u{435}\u{43A}.".to_string()),
        (2, "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{27E6}x1\u{27E7}".to_string()),
    ];
    let out = build(GO, &tr, Some("ru"));
    assert_eq!(
        out,
        "{\n  \"PersonCats\": {\n    \"description\": \"The number of cats a person has\",\n    \"hash\": \"sha1-abc\",\n    \"one\": \"\u{423} {{.Name}} {{.Count}} \u{43A}\u{43E}\u{448}\u{43A}\u{430}.\",\n    \"few\": \"\u{423} {{.Name}} {{.Count}} \u{43A}\u{43E}\u{448}\u{435}\u{43A}.\",\n    \"many\": \"\u{423} {{.Name}} {{.Count}} \u{43A}\u{43E}\u{448}\u{435}\u{43A}.\",\n    \"other\": \"\u{423} {{.Name}} {{.Count}} \u{43A}\u{43E}\u{448}\u{435}\u{43A}.\"\n  },\n  \"HelloPerson\": \"\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}, {{.Name}}\"\n}\n"
    );
    let ja = json(&build(GO, &tr, Some("ja")));
    let keys: std::collections::BTreeSet<&str> = ja["PersonCats"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["description", "hash", "other"].into_iter().collect());
    // v1 array form
    assert_eq!(units(GO_V1), vec!["\u{27E6}x1\u{27E7} day", "\u{27E6}x1\u{27E7} days", "Hello"]);
    let v1 = json(&build(GO_V1, &[(0, "\u{27E6}x1\u{27E7} d".into()), (1, "\u{27E6}x1\u{27E7} ds".into()), (2, "Hi".into())], Some("ru")));
    assert_eq!(v1[0]["translation"], json(r#"{"one": "{{.Count}} d", "few": "{{.Count}} ds", "many": "{{.Count}} ds", "other": "{{.Count}} ds"}"#));
    assert_eq!(v1[1], json(r#"{"id": "hello", "translation": "Hi"}"#));
}

#[test]
fn plain_json_is_unchanged_and_legacy_stamp_works() {
    let d = extract(PLAIN);
    let texts: Vec<String> = locos_units(&d).into_iter().map(|u| u.text).collect();
    assert_eq!(texts, vec!["Hello", "A description", "{count}"]);
    assert_eq!(build(PLAIN, &[(1, "Une description".into())], None), PLAIN.replace("A description", "Une description"));
    assert_eq!(build(PLAIN, &[], Some("ru")), PLAIN);
    // a document read as generic JSON before dialects: the Chrome-shaped file keeps its descriptions as units
    let legacy = ExtractOptions::new().with_profile("locos").with_format_option("json.dialect", "plain");
    let mut d = extract_with(CHROME, &legacy);
    assert_eq!(d.units.len(), 6);
    let seg = d.units[1].segments[0].id.clone();
    profile::locos_set_target(&mut d, &seg, vec![Inline::text("Name")]).unwrap();
    let out = Filter::merge_with(&JsonFilter, &d, &profile::locos().merge_options("json").with_format_option("json.dialect", "chrome")).unwrap();
    assert_eq!(json(std::str::from_utf8(&out).unwrap())["extName"]["description"], "Name");
    // deep generic JSON (deeper than any dialect) keeps the old walk
    let deep = format!("{}\"deep text\"{}", "[".repeat(150), "]".repeat(150));
    assert_eq!(units(&deep), vec!["deep text"]);
}

const I18NEXT: &str = "{\n  \"files\": {\n    \"deleted_one\": \"{{count}} file was deleted\",\n    \"deleted_other\": \"{{count}} files were deleted\",\n    \"show_other\": \"Show other\"\n  },\n  \"place_ordinal_one\": \"{{count}}st\",\n  \"place_ordinal_other\": \"{{count}}th\"\n}";

#[test]
fn i18next_plural_fill() {
    let tr = vec![(0, "S-pl".to_string()), (1, "P-pl".into()), (2, "Pokaz".into()), (3, "O1".into()), (4, "O-other".into())];
    let pl = json(&build(I18NEXT, &tr, Some("pl-PL")));
    let text = build(I18NEXT, &tr, Some("pl-PL"));
    let at: Vec<usize> = ["\"deleted_one\"", "\"deleted_few\"", "\"deleted_many\"", "\"deleted_other\"", "\"show_other\""].iter().map(|k| text.find(k).unwrap()).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{text}");
    assert_eq!(pl["files"]["deleted_few"], "P-pl");
    assert_eq!(pl["files"]["deleted_many"], "P-pl");
    assert_eq!(pl["files"]["deleted_one"], "S-pl");
    let ar = json(&build(I18NEXT, &tr, Some("ar-SA")));
    let got: std::collections::BTreeSet<&str> = ar["files"].as_object().unwrap().keys().filter(|k| k.starts_with("deleted_")).map(String::as_str).collect();
    assert_eq!(got, ["deleted_zero", "deleted_one", "deleted_two", "deleted_few", "deleted_many", "deleted_other"].into_iter().collect());
    // English ordinals use one/two/few/other
    let en = json(&build(I18NEXT, &tr, Some("en-GB")));
    assert_eq!(en["place_ordinal_two"], "O-other");
    assert_eq!(en["place_ordinal_few"], "O-other");
    assert!(en["files"].get("deleted_few").is_none());
    // Japanese needs only _other: nothing added, nothing removed; no locale never fills
    let ja = json(&build(I18NEXT, &tr, Some("ja-JP")));
    let keys: Vec<&str> = ja["files"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["deleted_one", "deleted_other", "show_other"]);
    assert!(json(&build(I18NEXT, &tr, None))["files"].get("deleted_few").is_none());
}

#[test]
fn i18next_fill_is_off_without_the_option_and_keeps_the_layout_when_asked() {
    let d = |lang: &str| {
        let mut d = JsonFilter.extract(I18NEXT.as_bytes(), &ExtractOptions::new().with_target_lang(lang.to_string())).unwrap();
        d.set_target("u2", vec![Inline::text("Plik")]).unwrap();
        d
    };
    // no option, no profile: the file keeps what it has
    let out = Filter::merge(&JsonFilter, &d("pl")).unwrap();
    assert!(!String::from_utf8(out).unwrap().contains("deleted_few"));
    // the option without the profile splices the keys into the text: the layout stays
    let on = MergeOptions::new().with_format_option("json.i18next_plurals", true);
    let out = String::from_utf8(Filter::merge_with(&JsonFilter, &d("pl"), &on).unwrap()).unwrap();
    assert!(out.contains("    \"deleted_one\": \"{{count}} file was deleted\",\n    \"deleted_few\": \"Plik\",\n    \"deleted_many\": \"Plik\",\n    \"deleted_other\": \"Plik\","), "{out}");
    json(&out);
    // the profile with the option off
    let off = profile::locos().merge_options("json").with_format_option("json.i18next_plurals", false);
    let out = String::from_utf8(Filter::merge_with(&JsonFilter, &d("pl"), &off).unwrap()).unwrap();
    assert!(!out.contains("deleted_few"));
}

#[test]
fn arb_suffix_plural_fill() {
    let src = "{\n  \"@@locale\": \"en\",\n  \"itemCountOne\": \"{count} item\",\n  \"itemCountOther\": \"{count} items\",\n  \"@itemCountOther\": {\"placeholders\": {\"count\": {}}},\n  \"showOther\": \"Show other\",\n  \"dayZero\": \"no days\",\n  \"dayFew\": \"OWN few\",\n  \"dayOther\": \"{n} days\"\n}\n";
    let tr: Vec<(usize, String)> = ["{count} polozka", "{count} polozek", "Jine", "Z", "F", "D"].iter().enumerate().map(|(i, s)| (i, s.to_string())).collect();
    let cs = build(src, &tr, Some("cs-CZ"));
    let v = json(&cs);
    assert_eq!(v["itemCountFew"], "{count} polozek");
    assert_eq!(v["itemCountMany"], "{count} polozek");
    assert_eq!(v["dayFew"], "F");
    assert_eq!(v["dayMany"], "D");
    assert_eq!(v["dayOne"], "D");
    let at = |k: &str| cs.find(&format!("\"{k}\"")).unwrap();
    assert!(at("itemCountOne") < at("itemCountFew") && at("itemCountFew") < at("itemCountOther"));
    assert!(cs.contains("  \"itemCountFew\": \"{count} polozek\",\n  \"itemCountMany\""));
    assert!(cs.contains("\"@itemCountOther\": {\"placeholders\": {\"count\": {}}},"));
    assert!(!cs.contains("showOtherFew") && !cs.contains("showFew"));
    assert_eq!(v["@@locale"], "cs_CZ");
    let ja = json(&build(src, &tr, Some("ja-JP")));
    assert!(ja.get("itemCountFew").is_none() && ja.get("itemCountOne").is_some());
    assert!(!build(src, &tr, None).contains("itemCountFew"));
}

#[test]
fn options_are_checked() {
    let bad = ExtractOptions::new().with_format_option("json.dialect", "yaml");
    assert!(matches!(JsonFilter.extract(b"[\"a b\"]", &bad), Err(Error::FormatOption { .. })));
    // a forced dialect the file does not fit
    let chrome = ExtractOptions::new().with_format_option("json.dialect", "chrome");
    assert!(matches!(JsonFilter.extract(PLAIN.as_bytes(), &chrome), Err(Error::FormatOption { .. })));
    assert!(matches!(JsonFilter.extract(GO.as_bytes(), &chrome), Err(Error::FormatOption { .. })));
    // without the option and without the profile a Chrome-shaped file is still the generic walk
    let d = JsonFilter.extract(CHROME.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.len(), 5);
    // with it (no profile) only the messages are units and the placeholders are Bilingua tags
    let d = JsonFilter.extract(CHROME.as_bytes(), &chrome).unwrap();
    assert_eq!(d.units.len(), 2);
    assert_eq!(d.units[1].context.key.as_deref(), Some("/greet/message"));
    assert_eq!(d.units[1].context.notes, vec!["Toolbar greeting".to_string()]);
    assert_eq!(Filter::merge(&JsonFilter, &d).unwrap(), CHROME.as_bytes());
    let mut d = d;
    let src = d.units[1].source();
    let (a, b) = (src[1].clone(), src[3].clone());
    d.set_target("u2", vec![Inline::text("Hallo "), a, Inline::text(", "), b]).unwrap();
    let out = String::from_utf8(Filter::merge(&JsonFilter, &d).unwrap()).unwrap();
    assert_eq!(json(&out)["greet"]["message"], "Hallo $USER$, $1");
    // a merge takes the same words
    let junk = MergeOptions::new().with_format_option("json.dialect", "nope");
    assert!(Filter::merge_with(&JsonFilter, &d, &junk).is_err());
    assert!(Filter::merge_with(&JsonFilter, &d, &MergeOptions::new().with_format_option("json.dialect", "plain")).is_ok());
}

#[test]
fn non_profile_dialect_merge_stamps_only_when_translated() {
    let opts = ExtractOptions::new().with_format_option("json.dialect", "auto").with_target_lang("ru".to_string());
    let mut d = JsonFilter.extract(FORMATJS.as_bytes(), &opts).unwrap();
    // nothing translated: the original bytes
    assert_eq!(Filter::merge(&JsonFilter, &d).unwrap(), FORMATJS.as_bytes());
    d.set_target("u4", vec![Inline::text("Hi")]).unwrap();
    let out = String::from_utf8(Filter::merge(&JsonFilter, &d).unwrap()).unwrap();
    assert!(out.contains("few{# items}"), "{out}");
}

#[test]
fn hostile_inputs() {
    let arb = locos_opts().with_format_option("json.dialect", "arb");
    for bad in ["[\"not\", \"an\", \"object\"]", "\"str\""] {
        assert!(JsonFilter.extract(bad.as_bytes(), &arb).is_err(), "{bad}");
    }
    assert!(JsonFilter.extract(b"{\"a\": \"caf\xe9\"}", &arb).is_err());
    let deep = format!("{{{}{}}}", "\"a\": {".repeat(300), "}".repeat(300));
    assert!(JsonFilter.extract(deep.as_bytes(), &locos_opts()).is_err());
    // malformed ICU: one opaque unit, rebuilt verbatim
    let src = r#"{"bad": "{count, plural, one{# x} other{# y}", "@@locale": "en"}"#;
    assert_eq!(units(src), vec!["{count, plural, one{# x} other{# y}"]);
    assert_eq!(build(src, &[], None), src);
    // deeply nested ICU: opaque, never a crash
    let icu = format!("{}x{}", "{a, select, other{".repeat(60), "}".repeat(60));
    let doc = format!("{{\"d\": \"{icu}\", \"@@locale\": \"en\"}}");
    assert_eq!(units(&doc).len(), 1);
    // NaN and Infinity read, as json.loads does
    for raw in ["{\"a\": NaN, \"b\": \"hello there\"}", "[1, -Infinity, Infinity, \"x y\"]"] {
        assert_eq!(extract(raw).units.len(), 1, "{raw}");
    }
    // a lone surrogate message is left alone
    let lone = r#"{"@@locale": "en", "a": "bad \ud800 here", "b": "good one"}"#;
    assert_eq!(units(lone), vec!["good one"]);
    assert_eq!(build(lone, &[], None), lone);
}

#[test]
fn big_arb_rebuild_is_linear() {
    let mut s = String::from("{\n  \"@@locale\": \"en\"");
    for i in 0..20000 {
        s.push_str(&format!(",\n  \"k{i}\": \"Message number {i} with some text\""));
    }
    s.push_str("\n}\n");
    let tr: Vec<(usize, String)> = (0..20000).map(|i| (i, format!("XMessage number {i} with some text"))).collect();
    let out = build(&s, &tr, Some("de"));
    assert!(out.contains("\"k19999\": \"XMessage number 19999 with some text\""));
    assert!(out.contains("\"@@locale\": \"de\""));
}

#[test]
fn mutated_files_never_panic() {
    let seeds = [ARB, CHROME, FORMATJS, STRUCTURED, GO, GO_V1, I18NEXT];
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for seed in seeds {
        for _ in 0..300 {
            let mut b = seed.as_bytes().to_vec();
            for _ in 0..(1 + next() % 4) {
                let at = (next() as usize) % b.len();
                match next() % 3 {
                    0 => b[at] = (next() % 128) as u8,
                    1 => {
                        b.remove(at);
                    }
                    _ => b.insert(at, b"{}[]\",:\\#'"[(next() % 10) as usize]),
                }
            }
            let opts = locos_opts().with_target_lang("pl".to_string());
            if let Ok(mut d) = JsonFilter.extract(&b, &opts) {
                for u in 0..d.units.len() {
                    let id = d.units[u].segments[0].id.clone();
                    let _ = d.set_target(&id, vec![Inline::text("T")]);
                }
                let _ = Filter::merge_with(&JsonFilter, &d, &profile::locos().merge_options("json"));
            }
        }
    }
}

#[test]
fn gate_crlf_file_gets_lf_lines_where_locos_splices_them() {
    // LocOS writes the go-i18n object it rebuilds with LF breaks inside a CRLF file (no newline normalising)
    let src = "{\r\n  \"files\": {\r\n    \"one\": \"{{.N}} file\",\r\n    \"other\": \"{{.N}} files\"\r\n  }\r\n}\r\n";
    let want = "{\r\n  \"files\": {\n    \"one\": \"{{.N}} file\",\n    \"few\": \"{{.N}} files\",\n    \"many\": \"{{.N}} files\",\n    \"other\": \"{{.N}} files\"\n  }\r\n}\r\n";
    assert_eq!(build(src, &[], Some("pl-PL")), want);
}

#[test]
fn gate_non_token_brackets_are_words_and_hand_built_units_merge() {
    // `⟦x⟧` is not a LocOS token, so its `x` is a letter (LocOS `_translatable`); `⟦x1⟧` alone is a token
    let src = "{\"@@locale\": \"en\", \"a\": \"\u{27e6}x\u{27e7}\", \"b\": \"\u{27e6}x1\u{27e7}\"}";
    assert_eq!(units(src), vec!["\u{27e6}x\u{27e7}".to_string()]);
    // a plain document whose units lost their original token still merges (as before the dialects)
    let mut d = JsonFilter.extract(b"{\"t\": \"Hello there\"}", &ExtractOptions::new()).unwrap();
    for u in &mut d.units {
        u.codes.clear();
    }
    let out = bilingua_model::Filter::merge_with(&JsonFilter, &d, &MergeOptions::default()).unwrap();
    assert_eq!(out, b"{\"t\": \"Hello there\"}");
}
