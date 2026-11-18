//! Bilingua filter for JavaScript and TypeScript locale modules (`.js .cjs .mjs .ts .mts .cts`): files whose only
//! content is a static message object (vue-i18n, i18next resources, hand-rolled translation tables). See the README.
//!
//! The file is tokenised, never executed. Every string value with a letter or digit outside its placeholders is a
//! unit, in source order; keys never. Code, types, comments, numbers and untranslatable strings stay in the
//! skeleton byte for byte, and only the literal of a changed unit is rewritten, in its own quote style.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod enc;
mod icu_units;
mod lex;
mod parse;
mod ph;

use bilingua_model::*;

const ID: &str = "js";
/// The one hidden code of a unit: `standalone` = the original literal (quotes included, a concatenation whole),
/// `open` = the quote character, `close` = "1" when the literal is a multi-line template.
const HIDDEN: &str = "$js";
/// bilingua#23: hidden values of the first unit of an ICU string split into one unit per branch: how many units the
/// string has (they follow each other) and the string's value (the merge plans it again).
const ICU_N: &str = "icu_units";
const ICU_V: &str = "icu_value";

/// Size limits (the Localization OS defaults). `Filter::extract` takes them from `ExtractOptions::limits`
/// ([`Limits::from_model`]); `extract_with` takes them as given.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Limits {
    /// String values in one file (and the base of the token, quote and line budgets).
    pub max_units: usize,
    /// Characters in one value.
    pub max_value_chars: usize,
    /// Bytes in one file.
    pub max_bytes: usize,
    /// Key bytes copied into the units, all together (`bilingua_model::Limits::max_context_bytes`): one long key
    /// over many values is refused before every unit holds a copy of it.
    pub max_context_bytes: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_units: 50_000,
            max_value_chars: 100_000,
            max_bytes: 128 * 1024 * 1024,
            max_context_bytes: bilingua_model::Limits::default().max_context_bytes,
        }
    }
}

impl Limits {
    /// The crate's limits from the call's `ExtractOptions::limits` (LocOS's `MAX_DOCUMENT_SEGMENTS`,
    /// `MAX_SEGMENT_CHARS` and upload size), so the budgets follow the caller's numbers.
    pub fn from_model(l: &bilingua_model::Limits) -> Limits {
        let d = Limits::default();
        Limits {
            max_units: l.max_units,
            max_value_chars: l.max_unit_chars,
            max_bytes: bilingua_model::guard::own_cap_bytes(d.max_bytes, bilingua_model::Limits::new().max_input_bytes, l.max_input_bytes),
            max_context_bytes: l.max_context_bytes,
        }
    }
}

/// Why a file was not read.
#[derive(Debug)]
pub(crate) enum ModuleError {
    /// Not a static locale module (code, a variable, a spread...), with the line.
    Refused(String),
    /// Over a size limit.
    Limit(String),
    /// Over the context limit (key text copied into the units).
    Context(String),
}

impl From<ModuleError> for Error {
    fn from(e: ModuleError) -> Error {
        match e {
            ModuleError::Refused(msg) => Error::Parse {
                format: ID.into(),
                msg,
            },
            // LocOS InputLimitError: code `limit`
            ModuleError::Limit(msg) => {
                let kind = if msg.contains("nested") { LimitKind::Depth } else { LimitKind::Units };
                LimitError::new(kind, msg).into()
            }
            ModuleError::Context(msg) => LimitError::new(LimitKind::Context, msg).into(),
        }
    }
}

/// One translatable string as a key-based project sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    /// Dotted key path (the default export's own name is dropped).
    pub key: String,
    /// The decoded value.
    pub value: String,
    /// Index of the unit.
    pub unit_index: usize,
}

/// The JavaScript / TypeScript locale module filter.
pub struct JsModule;

/// A file that is XML (a Qt Linguist `.ts` file) is not a JavaScript module.
fn is_xml(data: &[u8]) -> bool {
    let mut head: Vec<u8> = data[..data.len().min(512)].to_vec();
    for bom in [&[0xEF, 0xBB, 0xBF][..], &[0xFF, 0xFE], &[0xFE, 0xFF]] {
        if head.starts_with(bom) {
            head = head[bom.len()..]
                .iter()
                .copied()
                .filter(|&b| b != 0)
                .collect();
            break;
        }
    }
    let first = head
        .iter()
        .find(|&&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'));
    first == Some(&b'<')
}

struct Loaded {
    text: String,
    encoding: String,
    warnings: Vec<bilingua_model::Warning>,
}

fn load(bytes: &[u8], limits: &Limits, source_lang: Option<&str>) -> Result<Loaded, Error> {
    if bytes.len() > limits.max_bytes {
        return Err(LimitError::new(
            LimitKind::InputBytes,
            format!("this file is larger than {} bytes; split it into smaller files", limits.max_bytes),
        )
        .into());
    }
    if is_xml(bytes) {
        return Err(Error::Parse {
            format: ID.into(),
            msg: "This file is XML (a Qt Linguist .ts file?), not a JavaScript/TypeScript locale module.".into(),
        });
    }
    let (text, name, warnings) = enc::decode(bytes, source_lang)?;
    // a code page whose bytes do not come back from the text (double-mapped sequences) keeps the file's own bytes
    let encoding = if matches!(name, enc::UTF8 | enc::UTF8_BOM | enc::UTF16LE | enc::UTF16BE | enc::CP1252) {
        name.to_string()
    } else {
        bilingua_model::enc::keep_original(name, bytes, enc::encode(&text, name).ok().as_deref())
    };
    let lim = limits.max_units;
    let (mut lines, mut quotes, mut values) = (0usize, 0usize, 0usize);
    for &b in text.as_bytes() {
        match b {
            b'\n' | b'\r' => lines += 1,
            b'\'' | b'"' | b'`' => quotes += 1,
            b',' | b'{' | b'[' | b'(' => values += 1,
            _ => {}
        }
    }
    // LocOS `check_line_budget` and `_budget` (#571), code `limit`, LocOS's sentences
    if lines > 20 * lim {
        return Err(LimitError::new(
            LimitKind::Lines,
            format!("This file has more than {} lines (20 x MAX_DOCUMENT_SEGMENTS). Split it into smaller files.", 20 * lim),
        )
        .into());
    }
    if quotes > 40 * lim {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("This file has far more strings than MAX_DOCUMENT_SEGMENTS allows ({lim}). Split it into smaller files."),
        )
        .into());
    }
    if values > 20 * lim {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("This file has far more values than MAX_DOCUMENT_SEGMENTS allows ({lim}). Split it into smaller files."),
        )
        .into());
    }
    Ok(Loaded { text, encoding, warnings })
}

fn key_of(path: &str, root: &str, default_ref: Option<&str>) -> String {
    if let Some(d) = default_ref {
        if !root.is_empty() && root == d {
            if path == root {
                return String::new();
            }
            if let Some(rest) = path.strip_prefix(root).and_then(|r| r.strip_prefix('.')) {
                return rest.to_string();
            }
        }
    }
    path.to_string()
}

/// Every translatable string with its dotted key, in order (the LocOS `records()` view).
pub fn records(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    let limits = Limits::default();
    let l = load(bytes, &limits, None)?;
    let parsed = parse::parse(&l.text, &limits)?;
    let mut out = Vec::new();
    for ru in &parsed.units {
        let (inl, _) = ph::unit_inlines(&ru.value);
        if ph::translatable(&inl) {
            out.push(Record {
                key: key_of(&ru.path, &ru.root, parsed.default_ref.as_deref()),
                value: ru.value.clone(),
                unit_index: out.len(),
            });
        }
    }
    Ok(out)
}

/// The text a list of inline pieces writes (tags give back their original placeholder).
fn out_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                if let Some(c) = u.codes().get(id).and_then(|c| c.standalone_markup()) {
                    s.push_str(c);
                }
            }
        }
    }
    s
}

/// The literal for `s` in the given quote style: escaped for that quote (LocOS rules).
fn escape(s: &str, quote: char, multiline: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            '$' if quote == '`' && it.peek() == Some(&'{') => out.push_str("\\$"),
            '\n' => out.push_str(if quote == '`' && multiline {
                "\n"
            } else {
                "\\n"
            }),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

fn hidden(u: &Unit) -> Option<&CodeData> {
    u.codes().get(HIDDEN)
}

/// The literal to write for a unit: `None` when it is unchanged (the original literal stays).
fn new_literal(u: &Unit) -> Option<String> {
    if u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let out = out_text(u, &u.output());
    if out == out_text(u, &u.source()) {
        return None;
    }
    let h = hidden(u)?;
    let quote = h
        .open
        .as_deref()
        .and_then(|q| q.chars().next())
        .unwrap_or('\'');
    Some(escape(&out, quote, h.close_markup() == Some("1")))
}

/// The literal to write for an ICU string split into the `n` units from `at` (bilingua#23): `None` when nobody
/// translated it or it comes out the same (the original literal stays). The units are planned again as the extract
/// planned them (with the document's target's added categories, else without); a unit added for categories the
/// plan does not add means the target changed after extract.
fn icu_literal(doc: &Document, at: usize, n: usize, opts: &MergeOptions) -> Result<Option<String>, Error> {
    let units: Vec<&Unit> = doc.units.get(at..at + n).ok_or_else(|| Error::Write("the units no longer match the skeleton".into()))?.iter().collect();
    if units.iter().all(|u| u.segments.iter().all(|s| !s.has_target())) {
        return Ok(None);
    }
    let first = units[0];
    let value = first.hidden(ICU_V).ok_or_else(|| Error::Write(format!("unit {} lost its message", first.id)))?;
    let target = opts.stamp_lang(doc, true);
    let added = [doc.target_lang.as_deref().map(str::trim).filter(|t| MergeOptions::plausible_lang(t)), None]
        .into_iter()
        .find(|a| icu_units::aligned(value, &units, *a));
    let Some(added) = added else {
        // units added for categories neither plan adds: the document was extracted for another target
        let is_added = |u: &&&Unit| u.has_hidden(bilingua_model::icu::ADDED_HIDDEN);
        if let Some(u) = units.iter().filter(is_added).find(|u| bilingua_model::icu::has_any_target(u)).or_else(|| units.iter().find(is_added)) {
            return Err(bilingua_model::icu::foreign_added_error(u));
        }
        return Err(Error::Write("the units no longer match the skeleton".into()));
    };
    let new = icu_units::rebuild(value, &units, target, added).ok_or_else(|| Error::Write(format!("unit {} lost its message", first.id)))?;
    if new == value {
        return Ok(None);
    }
    let quote = hidden(first).and_then(|h| h.open.as_deref()).and_then(|q| q.chars().next()).unwrap_or('\'');
    Ok(Some(escape(&new, quote, false)))
}

/// The plural slot of a FormatJS (ICU MessageFormat) string, read with the shared parser (unit model step 7): the
/// string stays one unit, with its structure protected as placeholders, so the slot names the message's first
/// plural, selectordinal or select argument (kind and argument name) and no category. `None` for other strings.
fn icu_slot(id: &str, value: &str) -> Option<PluralSlot> {
    if !(value.contains("plural") || value.contains("select")) {
        return None;
    }
    let parts = bilingua_model::icu::parse(value).ok()?;
    let cx = parts.iter().find_map(|p| match p {
        bilingua_model::icu::Part::Complex(cx) => Some(cx),
        _ => None,
    })?;
    let mut slot = PluralSlot::new(id, cx.kind.plural_kind());
    let arg = cx.arg();
    if !arg.is_empty() {
        slot = slot.with_arg(arg);
    }
    Some(slot)
}

impl JsModule {
    /// `extract` with explicit size limits.
    pub fn extract_with(
        &self,
        bytes: &[u8],
        opts: &ExtractOptions,
        limits: &Limits,
    ) -> Result<Document, Error> {
        let l = load(bytes, limits, opts.source_lang.as_deref())?;
        let text = l.text.as_str();
        let parsed = parse::parse(text, limits)?;
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        let (mut line_pos, mut line) = (0usize, 1usize);
        let locos = opts.is_profile(bilingua_model::profile::LOCOS);
        // bilingua#23: outside the LocOS profile, the plural categories the target needs and a message lacks get
        // units of their own
        let added = if locos { None } else { opts.target_lang.as_deref().map(str::trim).filter(|t| MergeOptions::plausible_lang(t)) };
        // key text the branch units of split ICU strings copy, on top of what the parser charged
        let mut context = 0u64;
        for ru in &parsed.units {
            let (inl, codes) = ph::unit_inlines(&ru.value);
            let translatable = ph::translatable(&inl);
            if !(translatable || opts.include_locked && !ru.value.trim().is_empty()) {
                continue;
            }
            if ru.value.len() > limits.max_value_chars
                && ru.value.chars().count() > limits.max_value_chars
            {
                return Err(LimitError::new(
                    LimitKind::UnitChars,
                    format!(
                        "This file has a line or paragraph longer than {} characters (MAX_SEGMENT_CHARS). Break it into shorter lines.",
                        limits.max_value_chars
                    ),
                )
                .into());
            }
            if ru.start > pos {
                parts.push(SkelPart::Raw(text[pos..ru.start].to_string()));
            }
            pos = ru.end;
            line += text.as_bytes()[line_pos..ru.start]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            line_pos = ru.start;
            let key = key_of(&ru.path, &ru.root, parsed.default_ref.as_deref());
            // bilingua#23: an ICU plural or select string in one plain literal is a unit per branch; a template
            // literal or a concatenation stays one unit (it cannot be written back in its own shape)
            let split = if locos || !translatable || ru.concat || ru.quote == b'`' || ru.value.contains("${") { None } else { icu_units::plan(&ru.value, added) };
            if let Some(plan) = split {
                if units.len() + plan.len() > limits.max_units {
                    return Err(ModuleError::Limit(format!("This file has more than {} segments (MAX_DOCUMENT_SEGMENTS). Split it into smaller files.", limits.max_units)).into());
                }
                context = context.saturating_add(((plan.len() - 1) * key.len()) as u64);
                if context > limits.max_context_bytes {
                    return Err(ModuleError::Context(format!("This file carries more key text than the limit of {} bytes allows. Split it into smaller files.", limits.max_context_bytes)).into());
                }
                parts.push(SkelPart::Unit(units.len()));
                let first = format!("u{}", units.len() + 1);
                let n = plan.len();
                for (k, up) in plan.into_iter().enumerate() {
                    let mut u = Unit::new(format!("u{}", units.len() + 1), up.inlines);
                    u.set_codes(up.codes);
                    if k == 0 {
                        u.codes_mut().insert(HIDDEN, CodeData::new().with_open((ru.quote as char).to_string()).with_close("0".to_string()).with_standalone(text[ru.start..ru.end].to_string()));
                        u.set_hidden(ICU_N, n.to_string());
                        u.set_hidden(ICU_V, ru.value.as_str());
                    }
                    u.context.kind = Some("string".into());
                    u.context.style = Some("js".into());
                    u.context.key = Some(key.clone());
                    u.context.location = Some(format!("line {line}"));
                    if let Some(slot) = &up.slot {
                        u.set_plural(bilingua_model::icu::plural_slot(&first, slot));
                        if slot.added {
                            u.set_hidden(bilingua_model::icu::ADDED_HIDDEN, up.path.as_str());
                        }
                    }
                    if !up.path.is_empty() {
                        u.context.extra_mut().insert("icu", up.path);
                    }
                    units.push(u);
                }
                continue;
            }
            let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
            u.set_codes(codes);
            u.codes_mut().insert(
                HIDDEN,
                CodeData::new().with_open((ru.quote as char).to_string()).with_close(if ru.multiline { "1" } else { "0" }.to_string()).with_standalone(text[ru.start..ru.end].to_string()),
            );
            u.context.kind = Some("string".into());
            u.context.style = Some("js".into());
            u.context.key = Some(key);
            u.context.location = Some(format!("line {line}"));
            u.translate = translatable;
            if !locos {
                u.set_plural(icu_slot(&u.id, &ru.value));
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let mut doc = Document::new(ID, units, Skeleton::text(parts, l.encoding, bilingua_model::parser_info!())).with_options(opts);
        doc.warnings.extend(l.warnings);
        Ok(doc)
    }
}

impl Filter for JsModule {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["js", "cjs", "mjs", "ts", "mts", "cts"]
    }

    /// True when the whole file has the locale-module shape. XML (a Qt Linguist `.ts`) is refused, and so is a file
    /// with no export or declaration at all (an empty or prose file).
    fn sniff(&self, bytes: &[u8]) -> bool {
        let limits = Limits::default();
        let Ok(l) = load(bytes, &limits, None) else {
            return false;
        };
        parse::parse(&l.text, &limits).is_ok_and(|p| p.statements > 0)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.extract_with(bytes, opts, &Limits::from_model(&opts.limits()))
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text {
            parts, encoding, ..
        } = &doc.skeleton
        else {
            return Err(Error::Write("js needs a text skeleton".into()));
        };
        let mut out = String::new();
        let mut changed = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc
                        .units
                        .get(*i)
                        .ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    let lit = match u.hidden(ICU_N).and_then(|n| n.parse::<usize>().ok()) {
                        Some(n) => icu_literal(doc, *i, n, opts)?,
                        None => new_literal(u),
                    };
                    match lit {
                        Some(lit) => {
                            changed = true;
                            out.push_str(&lit);
                        }
                        None => {
                            out.push_str(
                                hidden(u).and_then(|h| h.standalone_markup()).ok_or_else(
                                    || Error::Write(format!("unit {} lost its literal", u.id)),
                                )?,
                            )
                        }
                    }
                }
            }
        }
        // Like LocOS: a file with nothing changed comes back exactly as it was read; a changed file is written as
        // UTF-8 (the byte order mark is kept when the source had one).
        if let Some(b) = bilingua_model::enc::utf8_output(&out, encoding, opts) {
            return Ok(b);
        }
        if !changed {
            if let Some(orig) = bilingua_model::enc::original_if_same(encoding, &out) {
                return Ok(orig);
            }
            return enc::encode(&out, bilingua_model::enc::split_original(encoding).0);
        }
        enc::encode(
            &out,
            if bilingua_model::enc::split_original(encoding).0 == enc::UTF8_BOM {
                enc::UTF8_BOM
            } else {
                enc::UTF8
            },
        )
    }
}
