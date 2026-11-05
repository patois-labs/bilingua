"""Builds tests/data/locos_golden.json: random MDX files and what the LocOS mdx filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    python gen_golden.py OUT.json [FILE ...]

For each file the golden holds the input, the units LocOS parses (text and the inline map of each unit) or the
refusal, and the bytes LocOS builds with these mappings: every unit replaced ("all"), every third unit ("third"),
every unit's own text plus " Z" with tokens kept ("tok"), every unit's text doubled ("dup"), every unit as
Japanese-style emphasis text ("cjk"). Deterministic (seed 1105; GOLDEN_SEED and GOLDEN_COUNT change the run).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-mdx-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

INLINE = [
    "Press", "the", "<Kbd>Ctrl</Kbd>", "{count}", '<Icon name="x" />', "`code {x}`", "\\{esc\\}", "<B>", "</B>",
    "**bold**", "_it_", "[link](http://x.y/a_b)", "![alt](i.png)", "https://x.org/p", "\u65e5\u672c\u8a9e",
    "\u0639\u0631\u0628\u0649", "\ud55c\uad6d\uc5b4", "{a +\n b}", '<Abbr title="Tip text">tip</Abbr>',
    "<img alt='A' src=x />", "<Tag", "{ unclosed", "&amp;", "\u00a9 2026", "50%", "<br/>", "x < y", "a > b",
    "{'}' + `{`}", "<Fragment>", "</>", "<>", "``a`b``", "\\<b>", "Caf\u00e9", "1984\\.", "\\#", "<A.B>x</A.B>",
    '<Note title="Warn" label={x}>', "</Note>", "{/* c */}", "<Image alt=\"\" src=\"y.png\"/>", "word", "words",
    "emoji \U0001f600", '<T title="q&quot;">', "{{nested}}", "$", "<$x>", "<_y />",
]
ATTRS = ['alt="A cat"', "title='Hi'", 'label={x}', 'type="info"', 'caption="Cap"', 'placeholder="Type here"', "aria-label='Menu'", 'data="d"', 'description="no"']
ESM = ["import A from './a.mdx'", "import {B} from \"b\"", "export const meta = {\n  title: 'X'\n}", "export default Layout", "import x from 'y'\nstill js text"]
KEYS = ["title", "description", "summary", "other", "Title", "sidebar_label"]
VALS = ["Getting started", '"Quoted: value"', "'single ''q'' value'", "yes", "123", "A title # with comment", "x: y", "\u65e5\u672c\u8a9e", "- item", "&anchor v", "|", "[a, b]", "Caf\u00e9 {x}", '"esc \\" q"', "", "null"]


def inline(rng, n):
    out = []
    for _ in range(n):
        out.append(rng.choice(INLINE))
    return " ".join(out)


def line_text(rng):
    return inline(rng, rng.randint(1, 7))


def flow_tag(rng):
    attrs = " ".join(rng.choice(ATTRS) for _ in range(rng.randint(0, 3)))
    name = rng.choice(["Note", "Chart", "Tabs", "div", "img", "Image", "Callout"])
    kind = rng.random()
    if kind < 0.3:
        return f"<{name} {attrs} />"
    if kind < 0.6:
        return f"<{name} {attrs}>"
    if kind < 0.75:
        return f"</{name}>"
    if kind < 0.9:
        return f"<{name}\n  " + "\n  ".join(attrs.split(" ")) + "\n>"
    return f"<{name} {attrs}></{name}>"


FMX = [
    "title: Plain title", "title: 'It''s'", 'title: "Tab\\there"', 'title: "uni \\u00e9 \\x41"', "title: 2024-01-05", "title: 1.5", "title: 0x1F",
    "title: ~", "title: Null", "title: True", "title: off", "title:", "title: |\n  block text", "title: >\n  folded text",
    "title: [a, b]", "title: {a: b}", "title: *ref", "title: !!str 12", "title: '", 'title: "unterminated', "title: a # comment",
    "title: a #notcomment", "title: \"quoted\" trailing", "title: 'a' # c", "title: 5:30", "title: 1_000", "title: .inf", "title: 12:30:45",
    "description: multi\n  line plain", "description: x\n  y: z", "nested:\n  title: inner\n  description: inner2", "list:\n  - title: x",
    "  indented: 1", "\ttabbed: 1", "key with spaces: v", "'quoted key': v", "\"title\": dq key", "? complex\n: value", "- seq item",
    "title: a: b", "title: a:b", "title: -", "title: - x", "title: ? x", "title: @x", "title: `x`", "title: %x", "title: ,x", "title: }x",
    "summary: Café 日本", "excerpt: <b>x</b>", "subtitle: 100%", "linkTitle: LT", "sidebar_label: &a v", "pagination_label: *a",
    "anchored: &x one", "other: *x", "title: x\ntitle: y", "Title: dup1\ntitle: dup2", "<<: *x", "=: v", "title: 'multi\n  line'",
    "title: \"esc \\\\ \\\" q\"", "title: \"bad \\q esc\"", "# comment line", "", "   ", "title:  spaced   ", "title:\tTabbed", "description: \"\"",
]
TOMLX = [
    'title = "Hello"', "title = 'Lit'", 'title = """multi"""', "title = 5", 'description = "a # b" # c', 'title = "esc \\n \\u00e9"',
    "[table]", 'title = "in table"', 'title="nospace"', "title = ", 'title = "unterminated', "a.b = 1", 'title = "x" "y"', "title = \"dup\"\ntitle = \"dup2\"",
    "other = 'x'", "# comment", "summary = \"S\"", "title = 'it''s'", "title = \"\"\"\nmulti\n\"\"\"",
]


def front_x(rng):
    if rng.random() < 0.8:
        lines = [rng.choice(FMX) for _ in range(rng.randint(0, 6))]
        return "---\n" + "\n".join(lines) + "\n" + rng.choice(["---", "---", "..."]) + "\n"
    lines = [rng.choice(TOMLX) for _ in range(rng.randint(0, 5))]
    return "+++\n" + "\n".join(lines) + "\n+++\n"


def front(rng):
    if rng.random() < 0.8:
        n = rng.randint(0, 4)
        lines = []
        for _ in range(n):
            k = rng.choice(KEYS)
            v = rng.choice(VALS)
            lines.append(f"{k}: {v}" if rng.random() < 0.9 else f"{k}:  {v}  # note")
        if rng.random() < 0.3:
            lines.append("tags:\n  - a\n  - b")
        if rng.random() < 0.1:
            lines.insert(rng.randint(0, len(lines)), "garbage line without colon")
        close = rng.choice(["---", "---", "..."]) if rng.random() < 0.95 else None
        return "---\n" + "\n".join(lines) + ("\n" + close + "\n" if close else "\n")
    lines = []
    for _ in range(rng.randint(0, 3)):
        k = rng.choice(["title", "description", "other"])
        q = rng.choice(['"', "'"])
        lines.append(f"{k} = {q}{rng.choice(['Hello', 'Caf\u00e9 x', 'a \\n b', 'Plain'])}{q}")
    return "+++\n" + "\n".join(lines) + "\n+++\n"


STRUCT = [
    "- item {x}\n  continued line\n\n      indented code {y}\n\n- next",
    "1. one\n   ```\n   fenced in list\n   ```\n2. two",
    "> quote\n> ```\n> code\n> ```",
    "\tTabbed line {z}",
    "  - nested\n    - deeper `c`",
    "[a]: <b c> 'T'\n[d]: e (Paren)\n[f]:\n  g\n  \"T2\"",
    "text\n[notdef]: http://x \"T\"",
    "[^1]: Foot **note**\n    cont line\n\n    para in note",
    "<!-- a -->\n<!-- b -->\ntext after",
    "<!--\nunclosed comment\n\ntext",
    "```` md\n```\nnot closed\n````",
    "~~~\ncode\n``` still\n~~~",
    "Title\n=====\n\nSub\n---",
    "| a \\| b | c |\n|:-:|--|\n| x \\| y | z |",
    "* * *\n___\n- - -",
    "export const a = 1\nsecond line\n\nimport b from 'b'",
    "text line\nexport not esm\nmore",
    "<Foo>\ntext in foo\n</Foo>",
    "<Foo bar=\"{x}\" baz='}'>inline <b>t</b></Foo> tail",
    "{value}\ntext",
    "{\n  multi\n} trailing text",
    "<A>text</A> and <B/>\n<C /> <D>",
    "Line with trailing  \nhard break  \nend",
    "![](empty.png) and ![a](b.png \"t\")",
    "&lt;b&gt; and &#123; and \\\\{x}",
    "title: not front matter\n---\nbody after rule",
    "text with ls\u0085and nel\x0bvt\x0cff",
    "a\\\nb",
    "`` ` `` and ```` ``` ````",
]


def block(rng):
    if os.environ.get("GOLDEN_MODE") == "struct" and rng.random() < 0.7:
        return rng.choice(STRUCT)
    r = rng.random()
    if r < 0.12:
        return "#" * rng.randint(1, 6) + " " + line_text(rng)
    if r < 0.40:
        return "\n".join(line_text(rng) for _ in range(rng.randint(1, 3)))
    if r < 0.48:
        return "\n".join(rng.choice(["- ", "* ", "+ ", "1. ", "2) ", "  - ", "> ", "> - "]) + line_text(rng) for _ in range(rng.randint(1, 3)))
    if r < 0.54:
        return "| a | b |\n|---|---|\n| " + inline(rng, 2) + " | " + inline(rng, 2) + " |"
    if r < 0.60:
        return rng.choice(["```", "~~~", "```js"]) + "\n" + line_text(rng) + "\nconst x = <Hidden>y</Hidden>\n" + rng.choice(["```", "~~~", ""])
    if r < 0.63:
        return rng.choice(["---", "***", "===", "- - -"])
    if r < 0.70:
        return rng.choice(ESM)
    if r < 0.80:
        return flow_tag(rng)
    if r < 0.85:
        return rng.choice(["{/* a comment */}", "{\n  props.items.map(i => <li>{i}</li>)\n}", "{count}", "{ // c }\n x }"])
    if r < 0.89:
        return rng.choice(["<!-- note -->", "<!--\nmulti\n-->", "<!-- a --> <!-- b -->", "<!-- open", "<!-- x --> text"])
    if r < 0.93:
        return rng.choice(["[ref]: http://x.y 'Title text'", "[ref2]: <http://a b> \"T\"", "[r3]: http://z\n  (Paren title)", "[^n]: Footnote text {x}", "[ref]: http://x.y", "text [a]: b"])
    if r < 0.96:
        return "    " + line_text(rng)
    return line_text(rng) + "\n===\n" + line_text(rng)


def make(rng):
    nl = rng.choice(["\n", "\n", "\n", "\r\n", "\r"])
    parts = []
    if os.environ.get("GOLDEN_MODE") == "fm":
        parts.append(front_x(rng))
    elif rng.random() < 0.35:
        parts.append(front(rng))
    for _ in range(rng.randint(1, 14)):
        parts.append(block(rng))
        parts.append(rng.choice(["\n\n", "\n\n", "\n", "\n\n\n"]))
    text = "".join(parts)
    if rng.random() < 0.3:
        text = text.rstrip("\n")
    text = text.replace("\n", nl)
    data = text.encode("utf-8")
    if rng.random() < 0.08:
        data = b"\xef\xbb\xbf" + data
    return data


def main():
    seed, count = int(os.environ.get("GOLDEN_SEED", "1105")), int(os.environ.get("GOLDEN_COUNT", "260"))
    out_path = sys.argv[1]
    files = [(os.path.basename(p), open(p, "rb").read()) for p in sys.argv[2:]]
    rng = random.Random(seed)
    for i in range(count):
        files.append((f"random-{i:03d}", make(rng)))
    f = get_filter("mdx")
    cases = []
    for name, data in files:
        case = {"name": name, "text": data.decode("utf-8")}
        try:
            pr = f.parse(data)
            texts = [u.text for u in pr.units]
            case["units"] = texts
            case["map"] = [u.inline_map for u in pr.units]
            case["styles"] = [u.style for u in pr.units]
            n = len(texts)
            maps = {
                "all": {str(i): 'T%d "q" \u00e9 {b} <c> `{d}` \\{e' % i for i in range(n)},
                "third": {str(i): "Zeta %d \u65e5" % i for i in range(0, n, 3)},
                "tok": {str(i): t + " Z" for i, t in enumerate(texts)},
                "dup": {str(i): t + " " + t for i, t in enumerate(texts)},
                "cjk": {str(i): "__" + t.replace(" ", "") + "__\u30aa\u30d7\u30b7\u30e7\u30f3\u3002 https://x.org/p" for i, t in enumerate(texts)},
            }
            case["builds"] = {k: f.build(data, m).decode("utf-8", "surrogateescape") for k, m in maps.items()}
        except Exception as e:  # noqa: BLE001
            case["error"] = type(e).__name__
        cases.append(case)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, ensure_ascii=False)
    ok = sum(1 for c in cases if "error" not in c)
    print(f"{len(cases)} cases, {ok} parsed, {len(cases) - ok} refused")
    errs = {}
    for c in cases:
        if "error" in c:
            errs[c["error"]] = errs.get(c["error"], 0) + 1
    print(errs)


main()
