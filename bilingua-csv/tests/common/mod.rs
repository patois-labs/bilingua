#![allow(dead_code)]
use bilingua_csv::{CsvFilter, CsvOptions, TsvFilter};
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

/// One call the way the LocOS filter takes it: format, column mapping, locales, target locale.
#[derive(Clone, Default)]
pub struct Call {
    pub tsv: bool,
    pub columns: Option<String>,
    pub src: Option<String>,
    pub targets: Vec<String>,
    pub locale: Option<String>,
}

impl Call {
    pub fn csv() -> Call {
        Call::default()
    }
    pub fn tsv() -> Call {
        Call { tsv: true, ..Call::default() }
    }
    pub fn cols(mut self, c: &str) -> Call {
        self.columns = Some(c.into());
        self
    }
    pub fn langs(mut self, src: &str, targets: &[&str], locale: Option<&str>) -> Call {
        self.src = (!src.is_empty()).then(|| src.to_string());
        self.targets = targets.iter().map(|s| s.to_string()).collect();
        self.locale = locale.map(String::from);
        self
    }
    pub fn options(&self) -> ExtractOptions {
        ExtractOptions::new().with_source_lang(self.src.clone()).with_target_lang(self.locale.clone())
    }
    pub fn copts(&self) -> CsvOptions {
        { let mut built = CsvOptions::default(); built.columns = self.columns.clone(); built.target_locales = self.targets.clone(); built }
    }
    pub fn extract(&self, data: &[u8]) -> Result<Document, Error> {
        self.extract_with(data, &self.options())
    }
    pub fn extract_with(&self, data: &[u8], o: &ExtractOptions) -> Result<Document, Error> {
        if self.tsv {
            TsvFilter::with_options(self.copts()).extract(data, o)
        } else {
            CsvFilter::with_options(self.copts()).extract(data, o)
        }
    }
    pub fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        if self.tsv {
            TsvFilter::default().merge(doc)
        } else {
            CsvFilter::default().merge(doc)
        }
    }
    pub fn units(&self, data: &[u8]) -> Vec<String> {
        self.extract(data).expect("extract").units.iter().map(|u| plain_text(&u.source())).collect()
    }
    /// Sets targets by unit index (as the LocOS mapping does) and merges.
    pub fn build(&self, data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
        let mut doc = self.extract(data).expect("extract");
        for (i, t) in mapping {
            let id = doc.units[*i].segments[0].id.clone();
            doc.set_target(&id, vec![Inline::text(*t)]).expect("set_target");
        }
        self.merge(&doc).expect("merge")
    }
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    CsvFilter::default().extract(data, &opts())
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/csv")
}

pub fn data_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}
