//! Plural-Forms rules for a target language (the table is generated from Localization OS's rules).

use crate::plural_table::RULES;

/// The gettext plural rule of one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub n: usize,
    pub expr: &'static str,
    one: usize,
    catch_all: usize,
}

fn find(key: &str) -> Option<Rule> {
    let i = RULES.binary_search_by(|r| r.0.cmp(key)).ok()?;
    let (_, n, expr, one, catch_all) = RULES[i];
    Some(Rule { n: n as usize, expr, one: one as usize, catch_all: catch_all as usize })
}

/// The rule for `value` (`ru_RU`, `pt_BR`, `id`): the full locale first, then its language.
pub fn lookup(value: &str) -> Option<Rule> {
    find(value).or_else(|| find(value.split('_').next().unwrap_or("")))
}

impl Rule {
    /// `nplurals=3; plural=...;`
    pub fn declaration(&self) -> String {
        format!("nplurals={}; plural={};", self.n, self.expr)
    }

    /// True when msgstr slot `i` takes the singular translation: the slot n = 1 lands on, unless that slot is the
    /// rule's catch-all (a one-form language's only slot).
    pub fn takes_singular(&self, i: usize) -> bool {
        i == self.one && i != self.catch_all
    }

    /// The slot that takes the plural translation when a catalog already written for the target keeps its own
    /// other forms: slot 1, unless slot 1 is where n = 1 lands (Arabic, Welsh), then the catch-all slot.
    pub fn plural_slot(&self) -> usize {
        if self.n <= 1 || self.one != 1 {
            return if self.n <= 1 { 0 } else { 1 };
        }
        if self.catch_all < self.n {
            self.catch_all
        } else {
            1
        }
    }

    /// Slots a catalog written for the target keeps as its own text.
    pub fn owned(&self, i: usize) -> bool {
        i == self.plural_slot() || self.takes_singular(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_rules() {
        let ru = lookup("ru_RU").unwrap();
        assert_eq!(ru.n, 3);
        assert!(ru.takes_singular(0) || ru.takes_singular(1));
        assert_eq!(lookup("id").unwrap().declaration(), "nplurals=1; plural=0;");
        assert_eq!(lookup("pt_BR").unwrap().expr, "(n > 1)");
        assert_eq!(lookup("pt_PT").unwrap().expr, "(n != 1)");
        let ar = lookup("ar").unwrap();
        assert_eq!(ar.n, 6);
        assert!(ar.takes_singular(1) && !ar.takes_singular(0));
        assert_eq!(ar.plural_slot(), 5);
        assert!(lookup("xx").is_none());
        let ja = lookup("ja").unwrap();
        assert!(!ja.takes_singular(0));
        assert_eq!(ja.plural_slot(), 0);
    }

    #[test]
    fn table_is_sorted() {
        assert!(RULES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
