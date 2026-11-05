r"""Checks the crate's encode against Localization OS, end to end (gate 11-05): the bytes a merge writes for the value
`a` + C, for every character C up to U+FFFF and a sample above, against LocOS's `_for_codepage` + codec (read only).

    cargo run --release -p bilingua-htmlhelp --example htmlhelp_encchk -- LABEL OUT.txt
    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe -I check_encode.py LABEL OUT.txt

Prints the number of differences (must be 0)."""
import sys

sys.path.insert(0, r"C:\Users\brent\code\local-tms")
from app.filters.htmlhelp_filter import _decode, _escape_attr, _for_codepage  # noqa: E402

label, path = sys.argv[1], sys.argv[2]
_t, enc = _decode(f'<meta charset={label}><OBJECT type="text/sitemap"><param name="Name" value="a"></OBJECT>'.encode())
codec = enc.codec
diff = n = 0
ex = []
for line in open(path, encoding="ascii"):
    u, got = line.split()
    c = chr(int(u, 16))
    v = _escape_attr("a" + c, '"')
    try:
        want = _for_codepage(v, codec).encode(codec).hex()
    except UnicodeEncodeError:
        want = "ERR"
    n += 1
    if want != got:
        diff += 1
        if len(ex) < 12:
            ex.append((u, want, got))
print(label, codec, n, "chars,", diff, "differences", ex)
