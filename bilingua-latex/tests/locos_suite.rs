//! The LocOS `verify_latex_filter` suite ported case by case (7 test functions there, 7 groups here, each with the
//! same assertions), plus the byte-level differences this crate makes on purpose (see the README).
mod common;
use bilingua_latex::Latex;
use bilingua_model::*;
use common::*;

const DOC: &str = r#"\documentclass{article}
\usepackage[utf8]{inputenc}
% preamble comment
\newcommand{\pkg}[1]{\texttt{#1}}
\title{A Short \emph{Guide}}
\author{Jane Doe}
\begin{document}
\maketitle

\section[Intro]{Introduction}\label{sec:intro}
This is the first paragraph with \emph{emphasis}, a reference to
Section~\ref{sec:intro} and a citation \cite[p.~3]{knuth}. Costs are 10\% of R\&D.
Math stays $E = mc^2$ inline. % a trailing comment

\subsection*{Details}
\begin{itemize}
  \item First item with \textbf{bold}.
  \item[Label] Second item\footnote{A footnote text.}.
\end{itemize}

\begin{equation}
  a^2 + b^2 = c^2 \text{ never translated}
\end{equation}
\[ x = y \]

\begin{verbatim}
Do not translate this.
\end{verbatim}
\begin{lstlisting}[language=Python]
print("no")
\end{lstlisting}

\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.5\textwidth]{img.png}
  \caption{A nice picture.}
  \label{fig:pic}
\end{figure}

\begin{tabular}{|l|r|}
  \hline
  Name & Value \\
  Apple & Red fruit \\ \hline
\end{tabular}

See \href{https://example.com}{the website} and \url{https://x.org/a%20b}.
Use \verb|code_here| in text.
\input{chapter1}
\end{document}
Trailing text after the document.
"#;

fn doc_units() -> Vec<String> {
    vec![
        "A Short \u{27E6}g1\u{27E7}Guide\u{27E6}/g1\u{27E7}".into(),
        "Intro".into(),
        "Introduction".into(),
        "This is the first paragraph with \u{27E6}g1\u{27E7}emphasis\u{27E6}/g1\u{27E7}, a reference to Section\u{a0}\u{27E6}x2\u{27E7} and a citation \u{27E6}x3\u{27E7}. Costs are 10% of R&D. Math stays \u{27E6}x4\u{27E7} inline.".into(),
        "Details".into(),
        "First item with \u{27E6}g1\u{27E7}bold\u{27E6}/g1\u{27E7}.".into(),
        "Label".into(),
        "Second item\u{27E6}g1\u{27E7}A footnote text.\u{27E6}/g1\u{27E7}.".into(),
        "A nice picture.".into(),
        "Name".into(),
        "Value".into(),
        "Apple".into(),
        "Red fruit".into(),
        "See \u{27E6}g1\u{27E7}the website\u{27E6}/g1\u{27E7} and \u{27E6}x2\u{27E7}. Use \u{27E6}x3\u{27E7} in text.".into(),
    ]
}

fn text_of(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

#[test]
fn units_and_identity() {
    let data = DOC.as_bytes();
    assert_eq!(units(data), doc_units());
    let crlf = DOC.replace('\n', "\r\n");
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(data);
    for body in [data, crlf.as_bytes(), &bom[..]] {
        assert_identity(body);
    }
    // CRLF reads the same units
    assert_eq!(units(crlf.as_bytes()), doc_units());
}

#[test]
fn translated_round_trip() {
    let data = DOC.as_bytes();
    let src = units(data);
    let tr: Vec<String> = src.iter().map(|t| format!("DE {t}")).collect();
    let map: Vec<(usize, &str)> = tr.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    let out = text_of(&build(data, &map));
    assert!(out.contains(r"\title{DE A Short \emph{Guide}}"), "{out}");
    assert!(out.contains(r"\section[DE Intro]{DE Introduction}\label{sec:intro}"), "{out}");
    assert!(
        out.contains(
            "DE This is the first paragraph with \\emph{emphasis}, a reference to Section~\\ref{sec:intro} and a citation \\cite[p.~3]{knuth}. Costs are 10\\% of R\\&D. Math stays $E = mc^2$ inline. % a trailing comment\n"
        ),
        "{out}"
    );
    assert!(out.contains(r"\item[DE Label] DE Second item\footnote{A footnote text.}."), "{out}");
    assert!(out.contains("  DE Name & DE Value \\\\\n  DE Apple & DE Red fruit \\\\ \\hline\n"), "{out}");
    assert!(out.contains(r"\text{ never translated}") && out.contains("Do not translate this."));
    assert!(out.contains(r"DE See \href{https://example.com}{the website} and \url{https://x.org/a%20b}."), "{out}");
    assert!(out.contains("\\input{chapter1}\n\\end{document}\nTrailing text after the document.\n"), "{out}");
    assert_eq!(units(out.as_bytes()), tr);
}

const BODY: &str = "\\begin{document}\n";
const END: &str = "\\end{document}\n";

#[test]
fn escaping() {
    let data = format!("{BODY}Plain text here.\n{END}");
    let out = build(data.as_bytes(), &[(0, "a & b % c $ d # e _ f { g } h ~ i ^ j \\ k\u{a0}l")]);
    assert_eq!(
        text_of(&out),
        "\\begin{document}\na \\& b \\% c \\$ d \\# e \\_ f \\{ g \\} h \\textasciitilde{} i \\textasciicircum{} j \\textbackslash{} k~l\n\\end{document}\n"
    );
    let out = build(data.as_bytes(), &[(0, "zwei\n\nAbs\u{e4}tze")]);
    assert_eq!(text_of(&out), "\\begin{document}\nzwei Abs\u{e4}tze\n\\end{document}\n");
    // a translation that drops or invents tokens keeps the markup balanced
    let data = format!("{BODY}A \\textbf{{bold}} word.\n{END}");
    let out = text_of(&build(data.as_bytes(), &[(0, "Ein \u{27E6}g1\u{27E7}fettes\u{27E6}/g1\u{27E7} Wort \u{27E6}x7\u{27E7}.")]));
    assert!(out.contains("Ein \\textbf{fettes} Wort ."), "{out}");
    // gate: a dropped, missing or out-of-order close never unbalances the braces
    let data = format!("{BODY}A \\textbf{{bold \\emph{{very}} word}} end.\n{END}");
    assert_eq!(units(data.as_bytes()), vec!["A \u{27E6}g1\u{27E7}bold \u{27E6}g2\u{27E7}very\u{27E6}/g2\u{27E7} word\u{27E6}/g1\u{27E7} end."]);
    for (tr, want) in [
        ("Ein \u{27E6}g1\u{27E7}fett Ende.", "Ein \\textbf{fett Ende.}"),
        ("Ein fett\u{27E6}/g1\u{27E7} Ende.", "Ein fett Ende."),
        ("\u{27E6}g1\u{27E7}a \u{27E6}g2\u{27E7}b\u{27E6}/g1\u{27E7} c\u{27E6}/g2\u{27E7} d", "\\textbf{a \\emph{b}} c d"),
    ] {
        let out = text_of(&build(data.as_bytes(), &[(0, tr)]));
        assert!(out.contains(want), "{tr:?} -> {out}");
        let body = out.split("\\begin{document}").nth(1).unwrap();
        assert_eq!(body.matches('{').count(), body.matches('}').count(), "{out}");
    }
}

#[test]
fn config_extra_inline_command() {
    let data = format!("{BODY}A \\keyword{{term}} here.\n{END}");
    assert_eq!(units(data.as_bytes()), vec!["A \u{27E6}x1\u{27E7} here."]);
    let f = Latex::with_inline_commands(["keyword"]);
    assert_eq!(units_with(&f, data.as_bytes()), vec!["A \u{27E6}g1\u{27E7}term\u{27E6}/g1\u{27E7} here."]);
    // names that are not plain letters are ignored
    let f = Latex::with_inline_commands(["key word", "", "a1"]);
    assert!(f.extra_inline.is_empty());
}

fn latin() -> Vec<u8> {
    let s = "\\usepackage[latin1]{inputenc}\n\\begin{document}\nCaf\u{e9} cr\u{e8}me.\n\\end{document}\n";
    s.chars().map(|c| c as u32 as u8).collect()
}

#[test]
fn encoding_windows_1252() {
    let data = latin();
    assert_eq!(units(&data), vec!["Caf\u{e9} cr\u{e8}me."]);
    // Untouched, and a translation that fits the code page: the file keeps its own encoding and its inputenc
    // option (LocOS always exports UTF-8 and rewrites it; Bilingua keeps the file's encoding, B6).
    assert_identity(&data);
    let out = build(&data, &[(0, "Kaffee \u{2013} Sahne \u{e9}")]);
    assert!(out.starts_with(b"\\usepackage[latin1]{inputenc}\n"));
    assert!(out.windows(7).any(|w| w == b"Kaffee "));
    assert!(out.contains(&0x96) && out.contains(&0xE9), "stays Windows-1252");
    // A translation that does not fit: UTF-8, and inputenc says so (the LocOS result)
    let out = build(&data, &[(0, "\u{41a}\u{43e}\u{444}\u{435} \u{2013} Sahne")]);
    let s = text_of(&out);
    assert!(s.starts_with("\\usepackage[utf8]{inputenc}\n"), "{s}");
    assert!(s.contains("\u{41a}\u{43e}\u{444}\u{435} \u{2013} Sahne"));
}

#[test]
fn hostile() {
    let bin = b"\x89PNG\r\n\x1a\n\x00\x00\x81\x8d\x8f\x90binary";
    assert!(matches!(extract(bin), Err(Error::Encoding(_))));
    // segment cap
    let many: String = (0..30).map(|i| format!("Paragraph {i}.\n\n")).collect();
    let o = ExtractOptions { limits: Some(Limits { max_units: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Latex::default().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let deep = format!("\\section{{{}deep{}}}\n", "{".repeat(5000), "}".repeat(5000));
    let bads: Vec<Vec<u8>> = vec![
        b"\\begin{document}\nOpen { brace and \\emph{unclosed\n\nNext para }}} here.\n".to_vec(),
        b"\\begin{verbatim}\nnever closed".to_vec(),
        b"$ unclosed math\n\nText \\[ unclosed display".to_vec(),
        deep.into_bytes(),
        b"\\verb".to_vec(),
        b"\\".to_vec(),
        b"%".repeat(10000),
        b"\\begin{tabular}{".repeat(2000),
        b"\\x{a ".repeat(40000),
        b"$a ".repeat(60000),
        b"\\item[ ".repeat(40000),
    ];
    for bad in &bads {
        let doc = extract(bad).expect("accepted");
        assert_eq!(Latex::default().merge(&doc).unwrap(), *bad);
        let texts = units(bad);
        let map: Vec<(usize, String)> = texts.iter().enumerate().map(|(i, t)| (i, format!("X {t}"))).collect();
        let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
        let _ = build(bad, &m);
    }
}

const CW: &str = "\\begin{document}\nWrite with \\LaTeX\\ and plain \\TeX. See \\S 2.\n\\end{document}\n";

#[test]
fn control_word_glued_to_letters() {
    // #696
    let data = CW.as_bytes();
    assert_eq!(units(data), vec!["Write with \u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7}and plain \u{27E6}x3\u{27E7}. See \u{27E6}x4\u{27E7}2."]);
    let ja = "\u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7}\u{3068}\u{30d7}\u{30ec}\u{30fc}\u{30f3}\u{306a}\u{27E6}x3\u{27E7}\u{3092}\u{4f7f}\u{3046}\u{3002}\u{27E6}x4\u{27E7}2\u{3092}\u{53c2}\u{7167}\u{3002}";
    let out = text_of(&build(data, &[(0, ja)]));
    assert!(out.contains("\\LaTeX\\ \u{3068}\u{30d7}\u{30ec}\u{30fc}\u{30f3}\u{306a}\\TeX{}\u{3092}\u{4f7f}\u{3046}\u{3002}"), "{out}");
    assert!(out.contains("\\S 2"), "{out}");
    let out = text_of(&build(data, &[(0, "Mit \u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7}und \u{27E6}x3\u{27E7}s. \u{27E6}x4\u{27E7}2.")]));
    assert!(out.contains("Mit \\LaTeX\\ und \\TeX{}s. \\S 2."), "{out}");
    let out = text_of(&build(data, &[(0, "Mit \u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7}und \u{27E6}x3\u{27E7}. \u{27E6}x4\u{27E7}2.")]));
    assert!(out.contains("und \\TeX. "), "{out}");
}

#[test]
fn control_word_before_blank() {
    // #742
    let data = b"\\begin{document}\n\\section{Installing \\LaTeX}\nThis chapter explains how to typeset with \\LaTeX.\n\n\
Last updated \\today\n\nSee \\S 2 and plain \\TeX.\n\\end{document}\n";
    let us = units(data);
    assert_eq!(
        us,
        vec![
            "Installing \u{27E6}x1\u{27E7}",
            "This chapter explains how to typeset with \u{27E6}x1\u{27E7}.",
            "Last updated \u{27E6}x1\u{27E7}",
            "See \u{27E6}x1\u{27E7}2 and plain \u{27E6}x2\u{27E7}.",
        ]
    );
    let de = [
        (0, "\u{27E6}x1\u{27E7} installieren"),
        (1, "Dieses Kapitel erkl\u{e4}rt, wie man mit \u{27E6}x1\u{27E7} setzt."),
        (2, "Zuletzt aktualisiert\t\u{27E6}x1\u{27E7}"),
        (3, "Siehe \u{27E6}x1\u{27E7}2 und reines \u{27E6}x2\u{27E7} (mit \u{27E6}x2\u{27E7}\tTab)."),
    ];
    let out = text_of(&build(data, &de));
    assert!(out.contains("\\section{\\LaTeX{} installieren}"), "{out}");
    assert!(out.contains("mit \\LaTeX{} setzt."), "{out}");
    assert!(out.contains("Zuletzt aktualisiert\t\\today\n"), "{out}");
    assert!(out.contains("Siehe \\S 2 und reines \\TeX{} (mit \\TeX{}\tTab)."), "{out}");
    let again = units(out.as_bytes());
    assert_eq!(again[1], "Dieses Kapitel erkl\u{e4}rt, wie man mit \u{27E6}x1\u{27E7} setzt.");
    let out = text_of(&build(data, &[(1, "Mit \u{27E6}x1\u{27E7}.")]));
    assert!(out.contains("Mit \\LaTeX.\n"), "{out}");
    assert_eq!(build(data, &[]), data);
}
