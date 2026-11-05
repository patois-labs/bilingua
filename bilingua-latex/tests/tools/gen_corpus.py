"""Writes the made-up LaTeX corpus into bilingua/oracle/corpus/latex (Lumen Bakery is fictional).

    python gen_corpus.py ..\\..\\..\\oracle\\corpus\\latex
"""

import os
import sys

LANGS = [
    ("en", "english", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es", "spanish", "Bienvenido a Panader\u00eda Lumen. El pan fresco est\u00e1 listo a las seis de cada ma\u00f1ana."),
    ("fr", "french", "Bienvenue \u00e0 la Boulangerie Lumen. Le pain frais est pr\u00eat \u00e0 six heures chaque matin."),
    ("it", "italian", "Benvenuto al Forno Lumen. Il pane fresco \u00e8 pronto ogni mattina alle sei."),
    ("de", "ngerman", "Willkommen in der B\u00e4ckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl", "dutch", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt", "portuguese", "Bem-vindo \u00e0 Padaria Lumen. O p\u00e3o fresco fica pronto \u00e0s seis horas todas as manh\u00e3s."),
    ("ja", None, "\u30eb\u30fc\u30e1\u30f3\u30fb\u30d9\u30fc\u30ab\u30ea\u30fc\u3078\u3088\u3046\u3053\u305d\u3002\u713c\u304d\u305f\u3066\u306e\u30d1\u30f3\u306f\u6bce\u671d\u516d\u6642\u306b\u3054\u7528\u610f\u3057\u3066\u3044\u307e\u3059\u3002"),
    ("zh", None, "\u6b22\u8fce\u5149\u4e34\u6d41\u660e\u9762\u5305\u5e97\u3002\u65b0\u9c9c\u9762\u5305\u6bcf\u5929\u65e9\u4e0a\u516d\u70b9\u51fa\u7089\u3002"),
    ("ko", None, "\ub8e8\uba58 \ubca0\uc774\ucee4\ub9ac\uc5d0 \uc624\uc2e0 \uac83\uc744 \ud658\uc601\ud569\ub2c8\ub2e4. \uac13 \uad6c\uc6b4 \ube75\uc740 \ub9e4\uc77c \uc544\uce68 \uc5ec\uc12f \uc2dc\uc5d0 \uc900\ube44\ub429\ub2c8\ub2e4."),
    ("ar", None, "\u0645\u0631\u062d\u0628\u0627 \u0628\u0643\u0645 \u0641\u064a \u0645\u062e\u0628\u0632 \u0644\u0648\u0645\u0646. \u0627\u0644\u062e\u0628\u0632 \u0627\u0644\u0637\u0627\u0632\u062c \u062c\u0627\u0647\u0632 \u0643\u0644 \u0635\u0628\u0627\u062d \u0641\u064a \u0627\u0644\u0633\u0627\u062f\u0633\u0629."),
]

PRE = {
    "ja": "\\usepackage{xeCJK}\n\\setCJKmainfont{Noto Serif CJK JP}",
    "zh": "\\usepackage{ctex}",
    "ko": "\\usepackage{kotex}",
    "ar": "\\usepackage{polyglossia}\n\\setmainlanguage{arabic}\n\\newfontfamily\\arabicfont{Amiri}",
}

FEATURES = r"""\documentclass[11pt]{article}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage[english]{babel}
\usepackage{graphicx,amsmath,hyperref,booktabs}
% preamble comment with a "quote" and {braces
\newcommand{\pkg}[1]{\texttt{#1}}
\title{Lumen Bakery \emph{Handbook}}
\author{The Lumen Team}
\date{\today}
\begin{document}
\maketitle
\tableofcontents

\begin{abstract}
This handbook explains how the bakery works, from the first oven to the last customer.
\end{abstract}

\section{Welcome}\label{sec:welcome}
Fresh bread is ready at \textbf{six} every morning---see Section~\ref{sec:hours} and \cite[p.~3]{baker99}.
Prices start at 3\,\% of the \$12 loaf; ask about R\&D loaves, a\_b and {\bf grouped} words.
Inline math $E = mc^2$ and \(a+b\) stay, and so does display math:
\[ \sum_{i=1}^n i = \frac{n(n+1)}{2} \]
$$ x^2 + y^2 = z^2 $$

\subsection*{Hours}\label{sec:hours}
\begin{itemize}
  \item Open from six to noon.
  \item[Sundays] Closed\footnote{Except on holidays; call us first.}.
  \item Visit \url{https://lumen.example/hours%20today} or \href{https://lumen.example}{our website}.
\end{itemize}

\begin{enumerate}
  \item Mix the dough. % remove for the demo
  \item Let it rest.
\end{enumerate}

\begin{equation}
  a^2 + b^2 = c^2 \text{ never translated}
\end{equation}
\begin{align*}
  f(x) &= x^2 \\
  g(x) &= x^3
\end{align*}

\begin{verbatim}
printf("Do not translate this: 100%");
\end{verbatim}
Use \verb|code_here| and \texttt{plain text} in a sentence.

\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.5\textwidth]{bread.png}
  \caption[Bread]{A fresh loaf of bread.}
  \label{fig:bread}
\end{figure}

\begin{table}[h]
\centering
\begin{tabular}{|l|r|}
  \hline
  Product & Price \\
  \hline
  Rye bread & 3.50 \\
  Sourdough & 4.20 \\ \hline
\end{tabular}
\caption{Today's prices}
\end{table}

\begin{lstlisting}[language=Python]
print("no")
\end{lstlisting}

Written with \LaTeX\ and plain \TeX. See \S 2 and \ldots{} more.
\input{appendix}
\bibliographystyle{plain}
\bibliography{refs}
\end{document}
Text after the document end is still read.
"""

CHAPTER = r"""\chapter{A chapter pulled in with \texttt{\textbackslash input}}
This file has no document environment, so all of it is body text. Costs are 10\% of R\&D.

\section{Another section}
Second paragraph with \emph{emphasis} and a footnote\footnote{Short note.}.
"""

HOSTILE = {
    "unclosed_group.tex": "\\begin{document}\nOpen { brace and \\emph{unclosed\n\nNext para }}} here.\n\\end{document}\n",
    "unclosed_math.tex": "\\begin{document}\n$ unclosed math\n\nText \\[ unclosed display\n",
    "unclosed_verbatim.tex": "\\begin{document}\nBefore.\n\\begin{verbatim}\nnever closed\n",
    "deep_braces.tex": "\\begin{document}\n\\section{" + "{" * 300 + "deep" + "}" * 300 + "}\nText.\n\\end{document}\n",
    "comments_only.tex": "% only comments\n% another one\n",
}


def w(root, name, data):
    with open(os.path.join(root, name), "wb") as fh:
        fh.write(data)


def main():
    root = sys.argv[1]
    os.makedirs(root, exist_ok=True)
    w(root, "features.tex", FEATURES.encode("utf-8"))
    w(root, "features_crlf.tex", FEATURES.replace("\n", "\r\n").encode("utf-8"))
    w(root, "features_bom.tex", b"\xef\xbb\xbf" + FEATURES.encode("utf-8"))
    w(root, "chapter_fragment.tex", CHAPTER.encode("utf-8"))
    for code, babel, text in LANGS:
        if babel:
            pre = "\\usepackage[%s]{babel}" % babel
            enc = "\\usepackage[utf8]{inputenc}\n"
        else:
            pre = PRE[code]
            enc = ""
        sents = text.split(". ") if code not in ("ja", "zh", "ko", "ar") else [text]
        body = "\n".join(sents)
        doc = (
            "\\documentclass{article}\n%s%s\n\\title{Lumen Bakery}\n\\begin{document}\n\\maketitle\n"
            "\\section{%s}\n%s\n\n"
            "\\begin{itemize}\n\\item %s \\textbf{%s}\n\\end{itemize}\nTotal: $3.50$ and 10\\%%.\n\\end{document}\n"
            % (enc, pre, sents[0], body, sents[-1], sents[0])
        )
        w(root, "lumen_%s.tex" % code, doc.encode("utf-8"))
    # legacy encoding: Latin-1 / Windows-1252 with a matching inputenc
    latin = (
        "\\documentclass{article}\n\\usepackage[latin1]{inputenc}\n\\usepackage[french]{babel}\n"
        "\\begin{document}\n\\section{Caf\u00e9 cr\u00e8me}\nL'\u00e9t\u00e9 \u00e0 la boulangerie : du pain frais \u2013 et des cro\u00eftons.\n\\end{document}\n"
    )
    w(root, "lumen_latin1.tex", latin.encode("cp1252"))
    w(root, "lumen_utf16.tex", b"\xff\xfe" + latin.replace("latin1", "utf8").encode("utf-16-le"))
    for name, text in HOSTILE.items():
        w(root, name, text.encode("utf-8"))


if __name__ == "__main__":
    main()
