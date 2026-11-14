"""Builds tests/data/locos_golden.json: random LaTeX documents and what the LocOS tex filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the unit texts LocOS parses (or the error class) and the bytes
LocOS builds (length:FNV-1a-64 digests) with several mappings: every unit translated, every third, plain text, the
unit's own text plus a mark (tokens kept). Builds are recorded only for UTF-8 inputs (LocOS exports a legacy file as
UTF-8; Bilingua keeps the file's encoding). Deterministic (seed 1108).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-latex-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Hello", "world", "The", "quick", "brown", "fox", "caf\u00e9", "na\u00efve", "\u65e5\u672c\u8a9e", "\u4e2d\u6587", "\ud55c\uad6d\uc5b4",
    "\u0645\u0631\u062d\u0628\u0627", "\u041f\u0440\u0438\u0432\u0435\u0442", "\u03b3\u03b5\u03b9\u03b1", "123", "10\\%", "R\\&D", "a\\_b",
    "~", "--", "---", "``quoted''", "Section~\\ref{sec:a}", "\\cite{k1,k2}", "\\cite[p.~3]{knuth}", "$x^2$", "$$y$$", "\\(z\\)",
    "\\emph{em}", "\\textbf{bold \\emph{nested}}", "\\textit{it}", "\\footnote{A note.}", "\\href{http://x.org/a%b}{link text}",
    "\\url{http://x.org/a%20b}", "\\verb|a_b|", "\\verb+x+", "\\LaTeX", "\\TeX", "\\ldots", "\\today", "\\textcolor{red}{red}",
    "{\\bf grouped}", "{\\em a {b} c}", "\\textsc{Small}", "\\underline{u}", "\\mbox{box}", "\\label{lab}", "\\ref{lab}", "\\\\", "\\\\[2pt]",
    "\\&", "\\%", "\\$", "\\#", "\\{", "\\}", "% comment here", "\\S 2", "\\textbackslash", "\\foo{bar}", "\\foo[opt]{bar}", "\\keyword{term}",
    "\\(", "\\)", "$", "{", "}", "\\\\*", "\\@", "\\,", "\\ ", "\\-", "\\'e", "\\\"o", "\\c{c}", "\\v{s}", "\\={\\i}", "\\sqrt{2}", "\\mathbb{R}",
]
BLOCKS = [
    lambda r: "\\section{%s}" % para(r, 3),
    lambda r: "\\section*{%s}" % para(r, 3),
    lambda r: "\\section[%s]{%s}" % (para(r, 1), para(r, 3)),
    lambda r: "\\subsection{%s}\\label{s:%d}" % (para(r, 2), r.randint(1, 9)),
    lambda r: "\\chapter{%s}" % para(r, 2),
    lambda r: "\\paragraph{%s} %s" % (para(r, 1), para(r, 6)),
    lambda r: "\\caption{%s}" % para(r, 4),
    lambda r: "\\captionof{figure}{%s}" % para(r, 3),
    lambda r: "\\begin{itemize}\n  \\item %s\n  \\item[%s] %s\n\\end{itemize}" % (para(r, 4), para(r, 1), para(r, 4)),
    lambda r: "\\begin{enumerate}\n\\item %s\n\\item %s\n\\end{enumerate}" % (para(r, 3), para(r, 3)),
    lambda r: "\\begin{description}\n\\item[%s] %s\n\\end{description}" % (para(r, 1), para(r, 4)),
    lambda r: "\\begin{equation}\n  %s\n\\end{equation}" % para(r, 4),
    lambda r: "\\begin{align*}\n a &= b \\\\\n c &= d\n\\end{align*}",
    lambda r: "\\[ x = %s \\]" % r.choice(["y", "\\frac{1}{2}", "a+b"]),
    lambda r: "\\begin{verbatim}\n%s\n\\end{verbatim}" % para(r, 3),
    lambda r: "\\begin{lstlisting}[language=Python]\nprint('%s')\n\\end{lstlisting}" % para(r, 1),
    lambda r: "\\begin{figure}[htbp]\n  \\centering\n  \\includegraphics[width=\\textwidth]{img.png}\n  \\caption{%s}\n  \\label{fig:1}\n\\end{figure}" % para(r, 3),
    lambda r: "\\begin{tabular}{|l|r|}\n\\hline\n%s & %s \\\\\n%s & %s \\\\ \\hline\n\\end{tabular}" % (para(r, 1), para(r, 2), para(r, 2), para(r, 1)),
    lambda r: "\\begin{table}\n\\begin{tabularx}{\\textwidth}{lX}\n%s & %s \\\\\n\\end{tabularx}\n\\end{table}" % (para(r, 1), para(r, 3)),
    lambda r: "\\begin{minipage}{0.5\\textwidth}\n%s\n\\end{minipage}" % para(r, 5),
    lambda r: "\\begin{quote}\n%s\n\\end{quote}" % para(r, 5),
    lambda r: "\\begin{abstract}\n%s\n\\end{abstract}" % para(r, 8),
    lambda r: "\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}",
    lambda r: "\\input{chapter%d}" % r.randint(1, 3),
    lambda r: "\\bibliography{refs}",
    lambda r: "\\newcommand{\\foo}[1]{#1 \\emph{x}}",
    lambda r: "\\def\\bar#1{%s}" % para(r, 2),
    lambda r: "\\hline",
    lambda r: "\\vspace{1em}",
    lambda r: "\\begin{otherlanguage}{german}\n%s\n\\end{otherlanguage}" % para(r, 4),
    lambda r: "\\begin{comment}\n%s\n\\end{comment}" % para(r, 3),
    lambda r: "\\begin{thebibliography}{99}\n\\bibitem{a} %s\n\\end{thebibliography}" % para(r, 4),
    lambda r: "%% %s" % para(r, 3),
]


def para(r, n):
    return " ".join(r.choice(WORDS) for _ in range(r.randint(1, max(1, n))))


def make(r):
    parts = []
    pre = r.random() < 0.85
    if pre:
        parts.append("\\documentclass{article}")
        if r.random() < 0.7:
            parts.append("\\usepackage[utf8]{inputenc}")
        if r.random() < 0.5:
            parts.append("\\usepackage[%s]{babel}" % r.choice(["english", "ngerman", "french"]))
        if r.random() < 0.6:
            parts.append("\\title{%s}" % para(r, 4))
        if r.random() < 0.3:
            parts.append("\\author{%s}" % para(r, 2))
        if r.random() < 0.3:
            parts.append("\\newcommand{\\pkg}[1]{\\texttt{#1}}")
        parts.append("\\begin{document}")
    for _ in range(r.randint(0, 14)):
        if r.random() < 0.55:
            parts.append(r.choice(BLOCKS)(r))
        else:
            parts.append(para(r, r.randint(1, 18)))
        parts.append(r.choice(["", "", "\n", " ", "   ", "%"]))
    if pre:
        parts.append("\\end{document}")
        if r.random() < 0.3:
            parts.append("Trailing text.")
    nl = r.choice(["\n", "\n", "\n", "\r\n"])
    text = "\n".join(parts) + r.choice(["", "\n"])
    if r.random() < 0.4:
        chars = list(text)
        for _ in range(r.randint(1, 4)):
            if not chars:
                break
            p = r.randrange(len(chars))
            op = r.choice(["del", "ins", "dup"])
            if op == "del":
                del chars[p : p + r.randint(1, 6)]
            elif op == "ins":
                chars[p:p] = list(r.choice(["{", "}", "\\", "$", "$$", "%", "\\begin{equation}", "\\end{itemize}", "[", "]", "\n\n", "\\item", "\\[", "\\]", "&", "\\\\", "~", "\\verb", "\\section{", "\\emph{"]))
            else:
                chars[p : p + 0] = chars[p : p + r.randint(1, 30)]
        text = "".join(chars)
    text = text.replace("\n", nl)
    kind = r.choice(["utf8", "utf8", "utf8", "utf8", "bom", "utf16", "cp1252"])
    if kind == "utf8":
        return text.encode("utf-8")
    if kind == "bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "utf16":
        return b"\xff\xfe" + text.encode("utf-16-le", "surrogatepass")
    try:
        return text.encode("cp1252")
    except UnicodeEncodeError:
        return text.encode("utf-8")


MAPS = {
    "all": lambda i, t: "T%d q \u00e9 & 100%% {x} ~ _ ^ #\nline2\t$" % i,
    "third": lambda i, t: "Zeta %d \u65e5" % i if i % 3 == 0 else None,
    "plain": lambda i, t: "Plain%d" % i,
    "echo": lambda i, t: t + "!",
    "spaced": lambda i, t: "  \n" + t + " \n\n ",
}


def digest(b):
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "%d:%016x" % (len(b), h)


def golden(data):
    f = get_filter("tex")
    rec = {"hex": data.hex()}
    try:
        res = f.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    units = [u.text for u in res.units]
    rec["units"] = units
    rec["enc"] = res.skeleton.get("source_encoding", "utf-8")
    try:
        data.decode("utf-8")
        utf8 = True
    except UnicodeDecodeError:
        utf8 = data[:2] in (b"\xff\xfe", b"\xfe\xff")
    rec["builds_compared"] = utf8 and data[:2] not in (b"\xff\xfe", b"\xfe\xff")
    if rec["builds_compared"]:
        builds = {}
        for name, fn in MAPS.items():
            m = {}
            for i, t in enumerate(units):
                v = fn(i, t)
                if v is not None:
                    m[str(i)] = v
            builds[name] = digest(f.build(data, m))
        rec["builds"] = builds
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "5c6265",  # truncated
    "fffe53",  # UTF-16 odd
    "fffe00d8",  # lone surrogate
    "efbbbfff",  # bad UTF-8 after BOM
    "0102030405060708" * 20 + "e9",  # control bytes
    "810a",  # 0x81 undefined in cp1252
)]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    r = random.Random(1108)
    cases = []
    for i in range(600):
        cases.append({"name": f"random-{i}", **golden(make(r))})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h)})
    for path in extra:
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()
