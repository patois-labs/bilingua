#![allow(dead_code)]
//! Bundles built in memory for the .xcloc tests (the fixtures of the LocOS suite `verify_xcloc_filter.py`).
use bilingua_model::*;
use bilingua_xcstrings::Xcloc;
use bilingua_zip::{write_zip, EntryMeta};

pub const XLIFF: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <file original="MyApp/Localizable.xcstrings" source-language="en" target-language="fr"
        datatype="plaintext">
    <header>
      <tool tool-id="com.apple.dt.xcode" tool-name="Xcode" tool-version="15.0" build-num="15A240d"/>
    </header>
    <body>
      <trans-unit id="Hello %@" xml:space="preserve">
        <source>Hello %@</source>
        <target state="new">Hello %@</target>
        <note>Greeting on the home screen</note>
      </trans-unit>
      <trans-unit id="%lld items" xml:space="preserve">
        <source>%1$lld items in %2$@</source>
        <target state="new">%1$lld items in %2$@</target>
      </trans-unit>
      <trans-unit id="Bold" xml:space="preserve">
        <source>Tap <g id="1">Save</g> now</source>
      </trans-unit>
      <trans-unit id="Empty" xml:space="preserve">
        <source></source>
      </trans-unit>
    </body>
  </file>
</xliff>
"#;

pub const XLIFF2: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <file original="MyApp/InfoPlist.xcstrings" source-language="en" target-language="fr"
        datatype="plaintext">
    <body>
      <trans-unit id="CFBundleName" xml:space="preserve">
        <source>My App</source>
      </trans-unit>
    </body>
  </file>
</xliff>
"#;

pub const CONTENTS: &[u8] = br#"{
  "developmentRegion" : "en",
  "project" : "MyApp.xcodeproj",
  "targetLocale" : "fr",
  "toolInfo" : {
    "toolBuildNumber" : "15A240d",
    "toolID" : "com.apple.dt.xcode",
    "toolName" : "Xcode",
    "toolVersion" : "15.0"
  },
  "version" : "1.0"
}"#;

pub fn png_bytes() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    for _ in 0..4 {
        v.extend(0u8..=255);
    }
    v
}

fn meta(name: &str, stored: bool, dir: bool, t: [u16; 6]) -> EntryMeta {
    EntryMeta { name: name.into(), stored, dir, modified: Some(t) }
}

/// Entries as given, plus the archive comment.
pub fn zip_of(entries: Vec<(EntryMeta, Vec<u8>)>, comment: &[u8]) -> Vec<u8> {
    let mut z = write_zip(&entries).unwrap();
    if !comment.is_empty() {
        let i = z.len() - 22;
        z[i + 20..i + 22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
        z.extend_from_slice(comment);
    }
    z
}

pub struct Bundle<'a> {
    pub prefix: &'a str,
    pub extra: Vec<(&'a str, Vec<u8>)>,
    pub xliffs: Vec<(&'a str, &'a [u8])>,
    pub contents: &'a [u8],
    pub macosx: bool,
}

impl Default for Bundle<'_> {
    fn default() -> Self {
        Bundle { prefix: "fr.xcloc/", extra: vec![], xliffs: vec![("fr.xliff", XLIFF)], contents: CONTENTS, macosx: true }
    }
}

impl Bundle<'_> {
    pub fn build(&self) -> Vec<u8> {
        let p = self.prefix;
        let mut e: Vec<(EntryMeta, Vec<u8>)> = Vec::new();
        if !p.is_empty() {
            e.push((meta(p, true, true, [2024, 1, 2, 3, 4, 6]), vec![]));
        }
        e.push((meta(&format!("{p}contents.json"), false, false, [2024, 1, 2, 3, 4, 6]), self.contents.to_vec()));
        for (name, body) in &self.xliffs {
            e.push((meta(&format!("{p}Localized Contents/{name}"), false, false, [2024, 1, 2, 3, 4, 8]), body.to_vec()));
        }
        e.push((meta(&format!("{p}Localized Contents/fr.lproj/logo.png"), true, false, [2024, 1, 2, 3, 4, 8]), png_bytes()));
        e.push((
            meta(&format!("{p}Source Contents/MyApp/Localizable.xcstrings"), false, false, [2024, 1, 2, 3, 4, 10]),
            br#"{"sourceLanguage":"en","strings":{"Hello %@":{}}}"#.to_vec(),
        ));
        e.push((meta(&format!("{p}Notes/screen.txt"), true, false, [2024, 1, 2, 3, 4, 12]), b"note".to_vec()));
        if self.macosx {
            e.push((meta("__MACOSX/fr.xcloc/._contents.json", true, false, [2024, 1, 2, 3, 4, 12]), b"\x00\x05\x16\x07junk".to_vec()));
        }
        for (name, body) in &self.extra {
            e.push((meta(name, true, false, [2024, 1, 2, 3, 4, 12]), body.clone()));
        }
        zip_of(e, b"made by hand")
    }
}

pub fn members(z: &[u8]) -> Vec<(String, Option<[u16; 6]>, bool, Vec<u8>)> {
    bilingua_zip::read_zip("xcloc", z, &bilingua_zip::ZipLimits::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.name, e.meta.modified, e.meta.stored, e.data))
        .collect()
}

pub fn member(z: &[u8], name: &str) -> Vec<u8> {
    members(z).into_iter().find(|m| m.0 == name).unwrap_or_else(|| panic!("no member {name}")).3
}

pub fn comment(z: &[u8]) -> Vec<u8> {
    let mut i = z.len() - 22;
    while !z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
        i -= 1;
    }
    let n = u16::from_le_bytes([z[i + 20], z[i + 21]]) as usize;
    z[i + 22..i + 22 + n].to_vec()
}

pub fn ex(z: &[u8]) -> Document {
    Xcloc.extract(z, &ExtractOptions::default()).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn ex_with(z: &[u8], target: Option<&str>) -> Document {
    Xcloc.extract(z, &ExtractOptions { target_lang: target.map(str::to_string), ..Default::default() }).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

/// Text of a unit the way LocOS shows it (tokens for inline codes).
pub fn text(u: &Unit) -> String {
    locos::to_locos_text(&u.source()).0
}

/// Sets the target of the unit at position `pos` from LocOS token text.
pub fn put(d: &mut Document, pos: usize, t: &str) {
    let u = &mut d.units[pos];
    let (_, map) = locos::to_locos_text(&u.source());
    u.segments[0].target = locos::from_locos_text(t, &map);
    u.segments[0].state = SegState::Draft;
}

pub fn merge(d: &Document) -> Vec<u8> {
    Xcloc.merge(d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}
