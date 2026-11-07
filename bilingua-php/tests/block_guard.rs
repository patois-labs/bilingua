//! The block form of the direct-access guard: `if (!defined('X') || !X) { die('...'); }`.
use bilingua_model::*;
use bilingua_php::Php;

fn open(src: &str) -> Result<Document, Error> {
    Php.extract(src.as_bytes(), &ExtractOptions::new())
}

#[test]
fn block_guard_is_accepted_and_merge_is_exact() {
    for guard in [
        "if (!defined('entry') || !entry) {\n    die('Not A Valid Entry Point');\n}",
        "if (!defined('entry')) die('x');",
        "if ( ! defined('entry') or ! entry ) { exit; }",
        "if (!defined('entry')) {\n exit();\n}",
    ] {
        let src = format!("<?php\n{guard}\n$strings = array(\n    'a' => 'Hello',\n    'b' => array('c' => 'World'),\n);\n");
        let mut doc = open(&src).unwrap_or_else(|e| panic!("{guard}: {e}"));
        assert_eq!(doc.units.len(), 2, "{guard}");
        assert_eq!(Php.merge(&doc).unwrap(), src.as_bytes(), "{guard}");
        for u in doc.units.iter_mut() {
            let mut t = vec![Inline::text("Zq")];
            t.extend(u.segments[0].source.clone());
            t.push(Inline::text("qZ"));
            u.segments[0].target = t;
        }
        let out = String::from_utf8(Php.merge(&doc).unwrap()).unwrap();
        assert!(out.contains("'a' => 'ZqHelloqZ'") && out.contains("'c' => 'ZqWorldqZ'"), "{out}");
        assert!(out.starts_with(&format!("<?php\n{guard}\n")), "guard text unchanged");
    }
}

#[test]
fn block_guard_refuses_anything_else() {
    for guard in [
        "if (!defined('entry') || !entry) { system('id'); }",
        "if (defined('entry')) { die('x'); }",
        "if (!defined('entry') && !entry) { die('x'); }",
        "if (!defined('entry')) { die('x'); } else { echo 'y'; }",
        "if (!defined('entry')) { die('x');",
        "if (!defined('entry')) { die(system('id')); }",
        "if (!defined('entry') || !entry()) { die('x'); }",
        "if (!defined('entry')) { die('x'); echo 'y'; }",
    ] {
        let src = format!("<?php\n{guard}\n$strings = array('a' => 'Hello');\n");
        assert!(open(&src).is_err(), "{guard}");
    }
}
