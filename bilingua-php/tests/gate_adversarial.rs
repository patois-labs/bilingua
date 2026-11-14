//! Gate (Opus) adversarial cases: code smuggled into a translatable value, strings hidden inside code, and hostile
//! translations that try to close their literal. A file PHP would run code from must be refused.
mod common;
use bilingua_model::*;
use bilingua_php::Php;
use common::*;

fn is_refused(src: &str) -> bool {
    matches!(
        extract_str(src),
        Err(Error::Parse { .. } | Error::Unsupported(_))
    )
}

#[test]
fn code_after_a_lone_cr_line_comment_is_not_hidden() {
    // PHP ends a `//` or `#` comment at a lone CR; the call after it runs
    assert!(is_refused("<?php\nreturn [\n// note\r'a' => system('id'),\n];\n"));
    assert!(is_refused("<?php\nreturn [\n# note\r'a' => system('id'),\n];\n"));
    // CRLF files are unaffected
    assert_eq!(texts_str("<?php\r\nreturn [\r\n// note\r\n'a' => 'Hi',\r\n];\r\n"), vec!["Hi"]);
}

#[test]
fn heredoc_closing_label_after_a_lone_cr_is_honoured() {
    // PHP closes the heredoc at "\rEOT"; the code between the two heredocs would run
    let src = "<?php\nreturn ['a' => <<<EOT\nhello\rEOT . system('id') . <<<EOT\nx\nEOT\n];\n";
    assert!(is_refused(src));
    assert!(is_refused("<?php\nreturn ['a' => <<<'N'\nhello\rN . system('id') . <<<'N'\nx\nN\n];\n"));
    // CRLF heredocs are fine
    assert_eq!(
        texts_str("<?php\r\nreturn ['a' => <<<EOT\r\n  one\r\n  two\r\n  EOT];\r\n"),
        vec!["one\r\ntwo"]
    );
}

#[test]
fn interpolation_after_any_non_ascii_byte_is_refused() {
    // PHP labels start with any byte 0x80..0xff, so "$" before any non-ASCII character is a variable
    assert!(is_refused("<?php return ['a' => \"Hi $\u{1F600} there\"];"));
    assert!(is_refused("<?php return ['a' => \"Hi $\u{e9}t\u{e9}\"];"));
    assert!(is_refused("<?php return ['a' => \"x {$o->run()} y\"];"));
    assert!(is_refused("<?php return ['a' => \"x ${o} y\"];"));
    assert!(is_refused("<?php return ['a' => \"x \\\\$o y\"];"));
    assert!(is_refused("<?php return ['a' => \"x \\{$o} y\"];"));
    assert!(is_refused("<?php return ['a' => \"x {$o[\"k\"]} y\"];"));
    assert!(is_refused("<?php return ['a' => <<<EOT\n  x {$o} y\n  EOT];"));
    // escaped or not a label start: literal
    assert_eq!(texts_str("<?php return ['a' => \"x \\$o {\\$o} $1 $ y\"];"), vec!["x $o {$o} $1 $ y"]);
    // a nowdoc never interpolates
    assert_eq!(texts_str("<?php return ['a' => <<<'N'\nx {$o} $y\nN];"), vec!["x {$o} $y"]);
}

#[test]
fn code_outside_literals_is_refused() {
    for src in [
        "<?php return ['a' => `ls`];",
        "<?php return ['a' => 'x' . FOO];",
        "<?php return ['a' => 'x' . $b];",
        "<?php return ['a' => __('x')];",
        "<?php return ['a' => fn() => 1];",
        "<?php return ['a' => new X()];",
        "<?php return [...$x];",
        "<?php return ['a' => X::Y];",
        "<?php return ['a' => 'b' ?? 'c'];",
        "<?php return ['a' => ('b')];",
        "<?php return ['a' => 'b'] + $x;",
        "<?php $lang['a'] .= 'b';",
        "<?php $lang[FOO] = 'b';",
        "<?php $lang['a'] = 'b'; echo 'x';",
        "<?php defined('B') OR exit(system('id'));",
        "<?php #[Attr]\nreturn ['a' => 'b'];",
        "<?php return ['a' => 'b']; ?>\n<script>x</script>",
        "<?php return ['a' => 'b']; // ?> <b>html</b>",
        "<html><?php return ['a' => 'b'];",
        "<?php return ['a' => 'b'];\n?>\n<?php system('id');",
        "<?php return ['a' => <<<EOT\n  ok\n  EOT . system('id') . 'x'\n  EOT];",
        "<?php return ['a' => \"\\u{D800}\"];",
        "<?php return ['a' => \"\\xff\"];",
        "<?php return ['a' => 'unterminated];",
        "<?php return ['a' => <<<EOT\nno close\n];",
        "<?php return ['a' => 'b' /* open comment ];",
        "<?php ${'x'} = 'y';",
        "<?php $$x = 'y';",
        "<?php return ['a' => 'b', 'c' => [1, 2, [3, include 'x.php']]];",
    ] {
        assert!(is_refused(src), "not refused: {src:?}");
    }
}

#[test]
fn comments_and_close_tags_inside_comments_hide_nothing() {
    // `?>` inside a block comment does not leave PHP mode; strings inside comments are not units
    let src = "<?php\n/* 'x' => 'not a unit' ?> */\nreturn [\n  # 'y' => 'nor this'\n  'a' => 'Hi', // 'z' => 'nor this'\n];\n";
    assert_eq!(texts_str(src), vec!["Hi"]);
    assert_identity(src.as_bytes());
}

#[test]
fn flexible_heredoc_terminators() {
    let src = "<?php\nreturn [\n    'a' => <<<EOT\n        Line one\n          EOTX is not the end\n        EOT,\n    'b' => <<<\"Q\"\n    two\n    Q\n];\n";
    assert_eq!(texts_str(src), vec!["Line one\n  EOTX is not the end", "two"]);
    assert_identity(src.as_bytes());
}

fn hostile_round_trip(src: &str, translation: &str) {
    let mut doc = extract_str(src).unwrap();
    let ids: Vec<String> = doc.units.iter().map(|u| u.segments[0].id.clone()).collect();
    for id in &ids {
        doc.set_target(id, vec![Inline::text(translation)]).unwrap();
    }
    let out = Php.merge(&doc).unwrap();
    let again = extract(&out).unwrap_or_else(|e| panic!("reopen failed ({e}): {}", String::from_utf8_lossy(&out)));
    assert_eq!(again.units.len(), ids.len(), "{}", String::from_utf8_lossy(&out));
    for r in bilingua_php::records(&out).unwrap() {
        assert_eq!(r.value, translation, "{}", String::from_utf8_lossy(&out));
    }
    // the code around the literals is unchanged: same statements, nothing new
    let before = extract_str(src).unwrap();
    assert_eq!(before.units.len(), again.units.len());
}

#[test]
fn hostile_translations_cannot_close_their_literal() {
    let src = "<?php\nreturn [\n  'sq' => 'Single',\n  'dq' => \"Double\",\n  'hd' => <<<EOT\n  Heredoc\n  EOT,\n  'nd' => <<<'N'\nNowdoc\nN,\n  'cc' => 'a' . \"b\",\n];\n$lang['ci'] = \"CI\";\n";
    for t in [
        "'); system('id'); //",
        "\"; system('id'); \"",
        "x\\",
        "x\\'",
        "x\\\"",
        "$x {$x} ${x} \\$x",
        "a ?> <script>b</script> <?php c",
        "line\nbreak\r\nand\ttab\u{0}nul\u{7f}",
        "EOT\nN\n",
        "' . system('id') . '",
        "x /* */ // # \u{2028}",
    ] {
        hostile_round_trip(src, t);
    }
}
