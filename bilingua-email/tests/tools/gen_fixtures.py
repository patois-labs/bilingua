"""Writes the hand-made message fixtures in tests/data (every person, address and text is made up).

    python gen_fixtures.py ../data

One message per language in UTF-8 (plain quoted-printable plus html base64), one per legacy charset the filter
reads (KOI8-R, Windows-1251, Windows-1256, ISO-2022-JP, Shift_JIS, EUC-JP, GB2312, Big5, EUC-KR, ISO-8859-2/9/15,
Windows-1252), and the structural cases (flowed text, hard wraps, folded headers, nested related/alternative,
forwarded message, inline image, unterminated multipart, quoted-printable edge cases).
"""

import base64
import binascii
import os
import sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "."

TEXT = {
    "en": ["Hello Ann,", "Your order ships on Monday. Please confirm your address before the end of the week.", "Thank you for your patience."],
    "de": ["Hallo Anna,", "Ihre Bestellung wird am Montag versandt. Bitte bestätigen Sie Ihre Adresse bis zum Ende der Woche.", "Vielen Dank für Ihre Geduld. Grüße aus Köln."],
    "fr": ["Bonjour Anne,", "Votre commande sera expédiée lundi. Veuillez confirmer votre adresse avant la fin de la semaine.", "Merci de votre patience. À bientôt !"],
    "es": ["Hola Ana,", "Su pedido se enviará el lunes. Por favor, confirme su dirección antes del fin de semana.", "¡Gracias por su paciencia! Mañana abrimos a las nueve."],
    "ru": ["Здравствуйте, Анна!", "Ваш заказ будет отправлен в понедельник. Пожалуйста, подтвердите адрес.", "Спасибо за терпение."],
    "ar": ["مرحبا آنا،", "سيتم شحن طلبك يوم الاثنين. يرجى تأكيد عنوانك.", "شكرا لصبركم."],
    "ja": ["アン様", "ご注文の商品は月曜日に発送されます。今週中にご住所をご確認ください。", "いつもありがとうございます。"],
    "zh": ["安娜您好，", "您的订单将于星期一发货。请在本周末之前确认您的地址。", "感谢您的耐心等待。"],
    "ko": ["안녕하세요 앤님,", "주문하신 상품은 월요일에 발송됩니다. 이번 주 말까지 주소를 확인해 주세요.", "기다려 주셔서 감사합니다."],
    "zt": ["安娜您好，", "您的訂單將於星期一發貨。請在本週末之前確認您的地址。", "感謝您的耐心等待。"],
    "tr": ["Merhaba Anna,", "Siparişiniz Pazartesi günü gönderilecek. Lütfen adresinizi hafta sonuna kadar onaylayın.", "Sabrınız için teşekkür ederiz."],
    "pl": ["Witaj Anno,", "Twoje zamówienie zostanie wysłane w poniedziałek. Prosimy o potwierdzenie adresu do końca tygodnia.", "Dziękujemy za cierpliwość."],
}
SUBJ = {"en": "Order update", "de": "Grüße aus Köln", "fr": "Café menu", "es": "Mañana abrimos", "ru": "Ваш заказ", "ar": "طلبك", "ja": "ご注文の確認", "zh": "订单确认", "zt": "訂單確認", "ko": "주문 확인", "tr": "Sipariş onayı", "pl": "Potwierdzenie zamówienia"}
SEP = {"ja": "", "zh": ""}


def write(name, data):
    with open(os.path.join(OUT, name), "wb") as fh:
        fh.write(data)


def enc_word(text, cs, how):
    raw = text.encode(cs)
    if how == "b":
        return "=?%s?b?%s?=" % (cs, base64.b64encode(raw).decode())
    q = "".join(chr(c) if (48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122) else ("_" if c == 32 else "=%02X" % c) for c in raw)
    return "=?%s?q?%s?=" % (cs, q)


def body(lang, kind):
    t = TEXT[lang]
    if kind == "plain":
        return "\n\n".join(t) + "\n\n-- \nAnn\n"
    return "<html><head><meta charset=\"utf-8\"></head><body><h1>%s</h1><p>%s</p><p>%s <b>%s</b></p></body></html>\n" % (t[0], t[1], t[2], SUBJ[lang])


def cte_body(raw, cte, eol):
    if cte == "quoted-printable":
        return binascii.b2a_qp(raw.replace(b"\r\n", b"\n"), quotetabs=False, istext=True, header=False).replace(b"\n", eol)
    if cte == "base64":
        return base64.encodebytes(raw).replace(b"\n", eol)
    return raw.replace(b"\r\n", b"\n").replace(b"\n", eol)


def single(lang, cs, cte, subj_how, kind="plain", eol=b"\r\n", subject_cs=None):
    text = body(lang, kind)
    raw = text.encode(cs)
    head = [b"From: Ann <ann@example.com>", b"To: Bob <bob@example.com>"]
    sc = subject_cs or cs
    if subj_how == "raw":
        head.append(b"Subject: " + SUBJ[lang].encode("utf-8"))
    else:
        head.append(b"Subject: " + enc_word(SUBJ[lang], sc, subj_how).encode())
    ct = "text/plain" if kind == "plain" else "text/html"
    head += [b"MIME-Version: 1.0", ('Content-Type: %s; charset="%s"' % (ct, cs)).encode(), b"Content-Transfer-Encoding: " + cte.encode()]
    return eol.join(head) + eol + eol + cte_body(raw, cte, eol)


def alternative(lang, eol=b"\r\n"):
    parts = []
    for kind, cte in (("plain", "quoted-printable"), ("html", "base64")):
        raw = body(lang, kind).encode("utf-8")
        ct = "text/plain" if kind == "plain" else "text/html"
        parts.append(('Content-Type: %s; charset=utf-8' % ct).encode() + eol + b"Content-Transfer-Encoding: " + cte.encode() + eol + eol + cte_body(raw, cte, eol))
    head = eol.join([b"From: Ann <ann@example.com>", b"To: Bob <bob@example.com>", b"Subject: " + enc_word(SUBJ[lang], "utf-8", "b").encode(), b"MIME-Version: 1.0", b'Content-Type: multipart/alternative; boundary="alt-' + lang.encode() + b'"']) + eol + eol
    out = head + b"Preamble text." + eol
    for p in parts:
        out += b"--alt-" + lang.encode() + eol + p + eol
    return out + b"--alt-" + lang.encode() + b"--" + eol


def main():
    os.makedirs(OUT, exist_ok=True)
    for lang in TEXT:
        if lang != "zt":
            write("lang_%s.eml" % lang, alternative(lang))
    legacy = [
        ("legacy_ru_koi8r", "ru", "koi8-r", "8bit", "q", "plain"),
        ("legacy_ru_cp1251", "ru", "windows-1251", "quoted-printable", "b", "plain"),
        ("legacy_ar_cp1256", "ar", "windows-1256", "base64", "b", "plain"),
        ("legacy_ja_iso2022jp", "ja", "iso-2022-jp", "7bit", "b", "plain"),
        ("legacy_ja_shift_jis", "ja", "shift_jis", "base64", "b", "plain"),
        ("legacy_ja_euc_jp", "ja", "euc-jp", "8bit", "b", "plain"),
        ("legacy_zh_gb2312", "zh", "gb2312", "quoted-printable", "b", "plain"),
        ("legacy_zh_big5", "zt", "big5", "8bit", "b", "plain"),
        ("legacy_ko_euc_kr", "ko", "euc-kr", "base64", "b", "plain"),
        ("legacy_tr_8859_9", "tr", "iso-8859-9", "quoted-printable", "q", "plain"),
        ("legacy_pl_8859_2", "pl", "iso-8859-2", "quoted-printable", "q", "html"),
        ("legacy_de_latin1", "de", "iso-8859-1", "quoted-printable", "q", "plain"),
        ("legacy_fr_cp1252_html", "fr", "windows-1252", "8bit", "q", "html"),
        ("legacy_es_8859_15", "es", "iso-8859-15", "quoted-printable", "q", "plain"),
    ]
    for name, lang, cs, cte, how, kind in legacy:
        write(name + ".eml", single(lang, cs, cte, how, kind, subject_cs=cs))
    write("utf8_8bit_lf.eml", single("de", "utf-8", "8bit", "raw", "plain", eol=b"\n"))
    write(
        "flowed_delsp.eml",
        b"From: Ann <ann@example.com>\r\nSubject: Flowed text\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8; format=flowed; delsp=yes\r\nContent-Transfer-Encoding: 8bit\r\n\r\n"
        b"This is one long logical line that a mail client wrapped with soft br \r\neaks so it fits in seventy two columns.\r\n\r\n> Quoted text that was \r\n> flowed by the sender.\r\n\r\nRegards \r\n-- \r\nAnn\r\n",
    )
    write(
        "hard_wrapped.eml",
        b"From: Ann <ann@example.com>\nSubject: Hard wrapped\n\nThis paragraph was hard wrapped by an old mail client at sixty-eight\ncolumns, so its lines belong together as one sentence for translation.\n\n"
        b"A short line.\n  An indented line.\n- a list item\n- another list item\n\n> quoted\n> text\n\n-- \nAnn\n",
    )
    write(
        "folded_headers.eml",
        b"Received: from mx.example.com (mx.example.com [192.0.2.1])\r\n\tby mail.example.com with ESMTP id abc123\r\n\tfor <bob@example.com>; Sat, 10 Oct 2026 20:00:00 -0600\r\n"
        b"From: Ann <ann@example.com>\r\nTo: Bob <bob@example.com>,\r\n Carla <carla@example.com>\r\nSubject: A very long subject line that has to be folded by the mail client because\r\n it runs past seventy eight columns\r\n"
        b"MIME-Version: 1.0\r\nContent-Type: text/plain;\r\n charset=\"us-ascii\"\r\nContent-Transfer-Encoding: 7bit\r\n\r\nShort body.\r\n",
    )
    write(
        "subject_words.eml",
        b"From: Ann <ann@example.com>\r\nSubject: =?utf-8?q?Caf=C3=A9?= =?utf-8?q?_menu?= for =?iso-8859-1?q?Z=FCrich?= (final)\r\nMIME-Version: 1.0\r\n\r\nBody text.\r\n",
    )
    write(
        "related_inline_image.eml",
        b"From: Ann <ann@example.com>\r\nSubject: Logo\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=rel\r\n\r\n--rel\r\nContent-Type: text/html; charset=utf-8\r\n\r\n"
        b"<html lang=\"en\"><body><p>See our <img src=\"cid:logo@example.com\" alt=\"Company logo\"> logo.</p></body></html>\r\n--rel\r\nContent-Type: image/png\r\nContent-Transfer-Encoding: base64\r\nContent-ID: <logo@example.com>\r\n\r\n"
        + base64.encodebytes(b"\x89PNG\r\n\x1a\n" + bytes(range(120))).replace(b"\n", b"\r\n") + b"--rel--\r\n",
    )
    write(
        "forwarded_rfc822.eml",
        b"Subject: Fwd: hello\nContent-Type: multipart/mixed; boundary=f\n\n--f\nContent-Type: text/plain\n\nSee below.\n--f\nContent-Type: message/rfc822\n\nSubject: inner\nContent-Type: text/plain\n\nInner body.\n--f--\n",
    )
    write("unterminated.eml", b"Subject: open\nContent-Type: multipart/mixed; boundary=q\n\n--q\nContent-Type: text/plain\n\nbody one\n--q\nContent-Type: text/plain\n\nbody two\n")
    write(
        "qp_edge_cases.eml",
        b"Subject: qp\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: quoted-printable\n\n"
        b"Equals =3D sign and trailing space=20\nA very long line that the encoder had to cut at seventy six characters with a soft break =\nright here in the middle of the text.\n=C3=A9=C3=A8 accents\n\n",
    )
    write("headers_only.eml", b"From: Ann <ann@example.com>\nSubject: Only headers\n")


main()
