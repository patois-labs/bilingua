"""Builds tests/data/locos_golden.json: generated email messages and what the LocOS email filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    <LocOS venv python> -B gen_golden.py OUT.json [N]

For each message the golden holds the input bytes (hex), the units LocOS parses (text with its inline tokens, or
the error message), the bytes it builds with four mappings (every unit, every third unit, plain placeholders,
long text) and the localized builds for several target locales. Deterministic (seed 1105). Every address is
made up (example.com).
"""

import base64
import binascii
import json
import os
import quopri
import random
import sys
import tempfile
from email.header import Header

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-email-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

EML = get_filter("eml")

# language -> (sentences, charsets that can hold them)
LANGS = {
    "en": (["The order ships on Monday.", "Please confirm your address before the end of the week.", "Thank you for your patience, we appreciate it.", "Your invoice number is 4711 and it is due in 30 days.", "Open the settings page and choose Notifications."], ["us-ascii", "utf-8", "iso-8859-1", "windows-1252"]),
    "de": (["Ihre Bestellung wird am Montag versandt.", "Bitte best\u00e4tigen Sie Ihre Adresse bis zum Ende der Woche.", "Vielen Dank f\u00fcr Ihre Geduld, wir wissen das zu sch\u00e4tzen.", "Gr\u00fc\u00dfe aus K\u00f6ln \u2013 bis bald!"], ["utf-8", "iso-8859-1", "iso-8859-15", "windows-1252"]),
    "fr": (["Votre commande sera exp\u00e9di\u00e9e lundi.", "Veuillez confirmer votre adresse avant la fin de la semaine.", "Merci de votre patience, nous l\u2019appr\u00e9cions beaucoup.", "\u00c0 bient\u00f4t, l\u2019\u00e9quipe du caf\u00e9."], ["utf-8", "iso-8859-1", "iso-8859-15", "windows-1252"]),
    "es": (["Su pedido se enviar\u00e1 el lunes.", "Por favor, confirme su direcci\u00f3n antes del fin de semana.", "\u00a1Gracias por su paciencia!", "Ma\u00f1ana abrimos a las nueve."], ["utf-8", "iso-8859-1", "windows-1252"]),
    "ru": (["\u0412\u0430\u0448 \u0437\u0430\u043a\u0430\u0437 \u0431\u0443\u0434\u0435\u0442 \u043e\u0442\u043f\u0440\u0430\u0432\u043b\u0435\u043d \u0432 \u043f\u043e\u043d\u0435\u0434\u0435\u043b\u044c\u043d\u0438\u043a.", "\u041f\u043e\u0436\u0430\u043b\u0443\u0439\u0441\u0442\u0430, \u043f\u043e\u0434\u0442\u0432\u0435\u0440\u0434\u0438\u0442\u0435 \u0430\u0434\u0440\u0435\u0441 \u0434\u043e \u043a\u043e\u043d\u0446\u0430 \u043d\u0435\u0434\u0435\u043b\u0438.", "\u0421\u043f\u0430\u0441\u0438\u0431\u043e \u0437\u0430 \u0442\u0435\u0440\u043f\u0435\u043d\u0438\u0435!"], ["utf-8", "koi8-r", "windows-1251", "iso-8859-5"]),
    "ar": (["\u0633\u064a\u062a\u0645 \u0634\u062d\u0646 \u0637\u0644\u0628\u0643 \u064a\u0648\u0645 \u0627\u0644\u0627\u062b\u0646\u064a\u0646.", "\u064a\u0631\u062c\u0649 \u062a\u0623\u0643\u064a\u062f \u0639\u0646\u0648\u0627\u0646\u0643 \u0642\u0628\u0644 \u0646\u0647\u0627\u064a\u0629 \u0627\u0644\u0623\u0633\u0628\u0648\u0639.", "\u0634\u0643\u0631\u0627 \u0644\u0635\u0628\u0631\u0643\u0645."], ["utf-8", "windows-1256", "iso-8859-6"]),
    "ja": (["\u3054\u6ce8\u6587\u306e\u5546\u54c1\u306f\u6708\u66dc\u65e5\u306b\u767a\u9001\u3055\u308c\u307e\u3059\u3002", "\u4eca\u9031\u4e2d\u306b\u3054\u4f4f\u6240\u3092\u3054\u78ba\u8a8d\u304f\u3060\u3055\u3044\u3002", "\u3044\u3064\u3082\u3042\u308a\u304c\u3068\u3046\u3054\u3056\u3044\u307e\u3059\u3002"], ["utf-8", "shift_jis", "euc-jp", "iso-2022-jp"]),
    "zh": (["\u60a8\u7684\u8ba2\u5355\u5c06\u4e8e\u661f\u671f\u4e00\u53d1\u8d27\u3002", "\u8bf7\u5728\u672c\u5468\u672b\u4e4b\u524d\u786e\u8ba4\u60a8\u7684\u5730\u5740\u3002", "\u611f\u8c22\u60a8\u7684\u8010\u5fc3\u7b49\u5f85\u3002"], ["utf-8", "gb2312", "gbk", "big5"]),
    "ko": (["\uc8fc\ubb38\ud558\uc2e0 \uc0c1\ud488\uc740 \uc6d4\uc694\uc77c\uc5d0 \ubc1c\uc1a1\ub429\ub2c8\ub2e4.", "\uc774\ubc88 \uc8fc \ub9d0\uae4c\uc9c0 \uc8fc\uc18c\ub97c \ud655\uc778\ud574 \uc8fc\uc138\uc694.", "\uae30\ub2e4\ub824 \uc8fc\uc154\uc11c \uac10\uc0ac\ud569\ub2c8\ub2e4."], ["utf-8", "euc-kr"]),
    "tr": (["Sipari\u015finiz Pazartesi g\u00fcn\u00fc g\u00f6nderilecek.", "L\u00fctfen adresinizi hafta sonuna kadar onaylay\u0131n.", "Sabr\u0131n\u0131z i\u00e7in te\u015fekk\u00fcr ederiz."], ["utf-8", "iso-8859-9", "windows-1254"]),
    "pl": (["Twoje zam\u00f3wienie zostanie wys\u0142ane w poniedzia\u0142ek.", "Prosimy o potwierdzenie adresu do ko\u0144ca tygodnia.", "Dzi\u0119kujemy za cierpliwo\u015b\u0107."], ["utf-8", "iso-8859-2", "windows-1250"]),
}
CJK = {"ja", "zh", "ko"}
SUBJECTS = {"en": "Weekly update for the team", "de": "Gr\u00fc\u00dfe aus K\u00f6ln", "fr": "Caf\u00e9 menu pour {0} invit\u00e9s", "es": "Ma\u00f1ana abrimos", "ru": "\u0412\u0430\u0448 \u0437\u0430\u043a\u0430\u0437", "ar": "\u0637\u0644\u0628\u0643", "ja": "\u3054\u6ce8\u6587\u306e\u78ba\u8a8d", "zh": "\u8ba2\u5355\u786e\u8ba4", "ko": "\uc8fc\ubb38 \ud655\uc778", "tr": "Sipari\u015f onay\u0131", "pl": "Potwierdzenie zam\u00f3wienia"}
LOCALES = ["de-DE", "ar", "ja-JP", "ru-RU", "ko-KR"]
NAMES = ["Ann", "Bob", "Chen", "Dara", "Eli"]


def pick_charset(rng, lang):
    return rng.choice(LANGS[lang][1])


def para_text(rng, lang, n):
    sents = LANGS[lang][0]
    sep = "" if lang in CJK else " "
    return sep.join(rng.choice(sents) for _ in range(n))


def hard_wrap(text, width, cjk):
    if cjk:
        return [text[i : i + width // 2] for i in range(0, len(text), width // 2)]
    out, cur = [], ""
    for w in text.split(" "):
        if cur and len(cur) + 1 + len(w) > width:
            out.append(cur)
            cur = w
        else:
            cur = (cur + " " + w) if cur else w
    out.append(cur)
    return out


def plain_body(rng, lang, flowed):
    cjk = lang in CJK
    blocks = []
    for _ in range(rng.randint(1, 4)):
        kind = rng.choice(["line", "wrap", "wrap", "quote", "list", "short"])
        t = para_text(rng, lang, rng.randint(1, 5))
        if kind == "line":
            blocks.append([t])
        elif kind == "wrap":
            blocks.append(hard_wrap(t, rng.choice([64, 70, 76]), cjk) if not flowed else [t])
        elif kind == "quote":
            blocks.append(["> " + para_text(rng, lang, 1)])
        elif kind == "list":
            blocks.append(["- " + para_text(rng, lang, 1), "- " + para_text(rng, lang, 1)])
        else:
            blocks.append([rng.choice(NAMES) + ","])
    if rng.random() < 0.5:
        blocks.append(["-- ", rng.choice(NAMES)])
    lines = []
    for bi, b in enumerate(blocks):
        for l in b:
            if flowed and not l.startswith(">") and l != "-- ":
                for k, w in enumerate(hard_wrap(l, 72, cjk)):
                    lines.append(w + (" " if True else ""))
                lines[-1] = lines[-1].rstrip(" ")
            else:
                lines.append(l)
        if bi < len(blocks) - 1 and rng.random() < 0.8:
            lines.append("")
    return lines


def html_body(rng, lang):
    ps = []
    for _ in range(rng.randint(1, 4)):
        t = para_text(rng, lang, rng.randint(1, 3))
        k = rng.choice(["p", "pb", "pa", "li", "h"])
        if k == "p":
            ps.append(f"<p>{t}</p>")
        elif k == "pb":
            ps.append(f"<p>{t} <b>{para_text(rng, lang, 1)}</b></p>")
        elif k == "pa":
            ps.append(f'<p>{t} <a href="https://example.com/x?a=1&amp;b=2">{para_text(rng, lang, 1)}</a></p>')
        elif k == "li":
            ps.append(f"<ul><li>{t}</li><li>{para_text(rng, lang, 1)}</li></ul>")
        else:
            ps.append(f"<h1>{t}</h1>")
    if rng.random() < 0.4:
        ps.append(f'<img src="cid:logo@example.com" alt="{para_text(rng, lang, 1)}">')
    cs = rng.choice(["", '<meta charset="utf-8">', '<meta http-equiv="Content-Type" content="text/html; charset=iso-8859-1">'])
    lang_attr = ' lang="en"' if rng.random() < 0.5 else ""
    return f"<html{lang_attr}><head>{cs}</head><body>" + "".join(ps) + "</body></html>"


def encode_body(rng, text, charset, cte_pref, eol, force_utf8_if_needed=True):
    """(bytes, cte) with the text in `charset`; charset fallbacks are the generator's job, not LocOS's."""
    text = text.replace("\n", eol)
    raw = text.encode(charset)
    ascii_only = all(b < 128 for b in raw)
    cte = cte_pref
    if cte == "7bit" and not ascii_only:
        cte = "8bit" if charset not in ("iso-2022-jp",) else "7bit"
    if cte == "7bit" or cte == "8bit":
        return raw, cte
    if cte == "quoted-printable":
        qp = binascii.b2a_qp(raw.replace(b"\r\n", b"\n"), quotetabs=False, istext=True, header=False)
        return qp.replace(b"\n", eol.encode()), cte
    b = base64.encodebytes(raw).replace(b"\n", eol.encode())
    return b, cte


def make_subject(rng, lang, eol):
    mode = rng.choice(["ascii", "q", "b", "utf8", "none", "long", "mixed"])
    s = SUBJECTS[lang]
    cs = rng.choice(LANGS[lang][1])
    if mode == "none":
        return b""
    if mode == "ascii" or (all(ord(c) < 128 for c in s) and mode in ("q", "b", "utf8")):
        return f"Subject: {s if all(ord(c) < 128 for c in s) else 'Order update'}{eol}".encode()
    if mode == "long":
        t = (s + " ") * 6
        try:
            h = Header(t.strip(), cs if cs != "us-ascii" else "utf-8", header_name="Subject").encode(linesep=eol, maxlinelen=78)
        except Exception:
            h = Header(t.strip(), "utf-8", header_name="Subject").encode(linesep=eol, maxlinelen=78)
        return f"Subject: {h}{eol}".encode("ascii")
    if mode == "utf8":
        return f"Subject: {s}{eol}".encode("utf-8")
    if mode == "mixed":
        w = Header(s, "utf-8").encode()
        return f"Subject: Re: {w} (final){eol}".encode("ascii")
    try:
        data = s.encode(cs if cs != "us-ascii" else "utf-8")
        cs_used = cs if cs != "us-ascii" else "utf-8"
    except Exception:
        data, cs_used = s.encode("utf-8"), "utf-8"
    if mode == "q":
        enc = "=?%s?q?%s?=" % (cs_used, "".join(chr(c) if (48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122) else ("_" if c == 32 else "=%02X" % c) for c in data))
    else:
        enc = "=?%s?b?%s?=" % (cs_used, base64.b64encode(data).decode())
    return f"Subject: {enc}{eol}".encode("ascii")


def leaf(rng, lang, kind, eol, flowed_ok=True):
    charset = pick_charset(rng, lang)
    cte = rng.choice(["7bit", "8bit", "quoted-printable", "base64"])
    flowed = kind == "plain" and flowed_ok and rng.random() < 0.25
    if kind == "plain":
        lines = plain_body(rng, lang, flowed)
        text = "\n".join(lines) + "\n"
    else:
        text = html_body(rng, lang) + "\n"
        if "iso-8859-1" in text and charset not in ("iso-8859-1", "us-ascii") and False:
            pass
    try:
        body, cte = encode_body(rng, text, charset, cte, eol)
    except (UnicodeEncodeError, LookupError):
        charset = "utf-8"
        body, cte = encode_body(rng, text, charset, cte, eol)
    ct = "text/plain" if kind == "plain" else "text/html"
    hdr = f'Content-Type: {ct}; charset="{charset}"'
    if flowed:
        hdr += "; format=flowed"
        if rng.random() < 0.4:
            hdr += "; delsp=yes"
    if rng.random() < 0.15:
        hdr = hdr.replace(f'charset="{charset}"', "charset=" + charset)
    style = rng.random()
    if style < 0.12:
        hdr = hdr.replace("; charset", ";" + eol + " charset", 1)  # folded Content-Type
    elif style < 0.2:
        hdr = hdr.lower().replace("content-type", "content-type")  # lower-case header and values
    cte_name = "content-transfer-encoding" if rng.random() < 0.15 else "Content-Transfer-Encoding"
    h = hdr + eol + f"{cte_name}: {cte}" + eol
    return h.encode("ascii"), body


def make_message(rng, idx):
    eol = rng.choice(["\n", "\r\n"])
    lang = rng.choice(list(LANGS))
    head = b"From: Ann <ann@example.com>" + eol.encode() + b"To: Bob <bob@example.org>" + eol.encode()
    head += make_subject(rng, lang, eol)
    head += b"MIME-Version: 1.0" + eol.encode()
    if rng.random() < 0.4:
        head += b"Content-Language: en-US" + eol.encode()
    shape = rng.choice(["single_plain", "single_html", "alternative", "mixed", "nested", "bare"])
    e = eol.encode()
    if shape == "bare":
        body = ("Hello there," + eol + eol + para_text(rng, "en", 2) + eol).encode()
        return head + e + body
    if shape in ("single_plain", "single_html"):
        h, b = leaf(rng, lang, "plain" if shape == "single_plain" else "html", eol)
        return head + h + e + b + (e if not b.endswith(e) else b"")
    bnd = "=_b%d" % idx
    parts = []
    if shape == "alternative":
        for k in ("plain", "html"):
            h, b = leaf(rng, lang, k, eol)
            parts.append(h + e + b)
        ct = b'Content-Type: multipart/alternative; boundary="' + bnd.encode() + b'"' + e
    else:
        h1, b1 = leaf(rng, lang, "plain", eol)
        h2, b2 = leaf(rng, lang, "html", eol)
        inner = "alt%d" % idx
        alt = (
            b"Content-Type: multipart/alternative; boundary=" + inner.encode() + e + e
            + b"--" + inner.encode() + e + h1 + e + b1 + e
            + b"--" + inner.encode() + e + h2 + e + b2 + e
            + b"--" + inner.encode() + b"--" + e
        )
        parts.append(alt)
        att = bytes(rng.randrange(256) for _ in range(rng.randint(10, 300)))
        parts.append(b"Content-Type: application/pdf; name=a.pdf" + e + b"Content-Transfer-Encoding: base64" + e + b"Content-Disposition: attachment; filename=a.pdf" + e + e + base64.encodebytes(att).replace(b"\n", e))
        if shape == "nested":
            parts.append(b"Content-Type: text/plain; charset=us-ascii" + e + b"Content-Disposition: attachment; filename=n.txt" + e + e + b"Attachment text." + e)
        ct = b'Content-Type: multipart/mixed; boundary="' + bnd.encode() + b'"' + e
    out = head + ct + e + b"This is a multi-part message in MIME format." + e
    for p in parts:
        out += b"--" + bnd.encode() + e + p + e
    out += b"--" + bnd.encode() + b"--" + e
    if rng.random() < 0.3:
        out += b"Epilogue." + e
    return out


def try_parse(data):
    try:
        res = EML.parse(data)
        return [u.text for u in res.units], None
    except Exception as exc:  # noqa: BLE001
        return None, f"{type(exc).__name__}: {exc}"


def mapping(label, texts, i0=0):
    m = {}
    for i, t in enumerate(texts):
        if label == "all":
            m[str(i)] = "\u00ab" + t + "\u00bb"
        elif label == "third":
            if i % 3 == 0:
                m[str(i)] = "Zeta %d \u65e5\u672c " % i + t
        elif label == "plain":
            m[str(i)] = ("Plain%d" % i) if "\u27e6" not in t else t
        elif label == "long":
            m[str(i)] = " ".join([t] * 6) if "\u27e6" not in t else t
        elif label == "same":
            m[str(i)] = t
    return m


def build(data, m, locale=None):
    try:
        out = EML.build_localized(data, m, locale) if locale else EML.build(data, m)
        return out.hex()
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {exc}"}


def main():
    out_path = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(int(os.environ.get("EMAIL_GOLDEN_SEED", "1105")))
    cases = []
    for idx in range(n):
        data = make_message(rng, idx)
        texts, err = try_parse(data)
        case = {"name": f"gen{idx:04d}", "hex": data.hex()}
        if err:
            case["error"] = err
            cases.append(case)
            continue
        case["units"] = texts
        case["builds"] = {lab: build(data, mapping(lab, texts)) for lab in ("all", "third", "plain", "long")}
        case["localized"] = {f"all:{loc}": build(data, mapping("all", texts), loc) for loc in LOCALES[: 1 + idx % 3]}
        cases.append(case)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    ok = sum(1 for c in cases if "units" in c)
    print(f"{len(cases)} cases, {ok} parsed, {len(cases) - ok} refused")


if __name__ == "__main__":
    main()
