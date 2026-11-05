"""Writes the oracle corpus folder bilingua/oracle/corpus/serverpages/ (made up pages, every dialect and extension,
11 languages, legacy encodings, BOM and line ends).

    python -I gen_corpus.py ..\\..\\..\\oracle\\corpus\\serverpages
"""

import os
import sys

# (output island, statement open, statement close, comment, attribute island)
CODE = {
    "phtml": ("<?= $name ?>", "<?php if ($user): ?>", "<?php endif; ?>", "<?php // note: not text ?>", "<?= $url ?>"),
    "jsp": ("${user.name}", "<% if (user != null) { %>", "<% } %>", "<%-- note: not text --%>", "${url}"),
    "aspx": ("<%: Model.Name %>", "<% if (User != null) { %>", "<% } %>", "<%-- note: not text --%>", "<%= Url %>"),
    "erb": ("<%= @user.name %>", "<% if @user -%>", "<% end -%>", "<%# note: not text %>", "<%= url %>"),
    "shtml": ('<!--#echo var="USER" -->', '<!--#include virtual="/header.html" -->', '<!--#config timefmt="%Y" -->', '<!--#set var="a" value="b" -->', '<!--#echo var="URL" -->'),
}
HEAD = {
    "phtml": "",
    "jsp": '<%@ page contentType="text/html; charset=UTF-8" %>\n<%@ taglib uri="http://java.sun.com/jsp/jstl/core" prefix="c" %>\n',
    "aspx": '<%@ Page Language="C#" AutoEventWireup="true" %>\n',
    "erb": "",
    "shtml": "",
}
TEXT = {
    "en": ("Welcome back", "Your orders", "Sign in to continue. It's quick, and it's free.", "Open the inbox", "Company logo"),
    "ar": ("مرحبا بك", "طلباتك", "سجل الدخول للمتابعة. إنه سريع.", "افتح صندوق الوارد", "شعار الشركة"),
    "ja": ("お帰りなさい", "ご注文", "続行するにはサインインしてください。無料です。", "受信箱を開く", "会社のロゴ"),
    "zh": ("欢迎回来", "您的订单", "请登录以继续。很快，而且免费。", "打开收件箱", "公司标志"),
    "ko": ("다시 오신 것을 환영합니다", "주문 내역", "계속하려면 로그인하세요. 빠르고 무료입니다.", "받은 편지함 열기", "회사 로고"),
    "he": ("ברוך שובך", "ההזמנות שלך", "התחבר כדי להמשיך. זה מהיר.", "פתח את הדואר", "לוגו החברה"),
    "ru": ("С возвращением", "Ваши заказы", "Войдите, чтобы продолжить. Это быстро.", "Открыть почту", "Логотип"),
    "el": ("Καλώς ήρθατε", "Οι παραγγελίες σας", "Συνδεθείτε για να συνεχίσετε.", "Άνοιγμα επιστολών", "Λογότυπο"),
    "hi": ("वापसी पर स्वागत है", "आपके ऑर्डर", "जारी रखने के लिए साइन इन करें।", "इनबॉक्स खोलें", "कंपनी लोगो"),
    "th": ("ยินดีต้อนรับกลับ", "คำสั่งซื้อของคุณ", "เข้าสู่ระบบเพื่อดำเนินการต่อ", "เปิดกล่องจดหมาย", "โลโก้บริษัท"),
    "de": ("Willkommen zurück", "Ihre Bestellungen", "Melden Sie sich an, um fortzufahren. Es geht schnell – und ist kostenlos.", "Posteingang öffnen", "Firmenlogo"),
}


def page(fmt, lang, nl="\n"):
    out, op, cl, com, at = CODE[fmt]
    hello, orders, para, title, alt = TEXT[lang]
    rtl = ' dir="rtl"' if lang in ("ar", "he") else ""
    lines = [
        HEAD[fmt].rstrip("\n"),
        "<!DOCTYPE html>",
        f'<html lang="{lang}"{rtl}>',
        f'<head><meta charset="utf-8"><title>{hello} {out}</title><meta name="description" content="{orders}"></head>',
        "<body>",
        com,
        f"<h1>{hello}, {out}!</h1>",
        op,
        f"<p>{para} <b>{orders}</b> {out} <a href=\"{at}\" title=\"{title}\">{title}</a>.</p>",
        cl,
        f'<img src="logo.png" alt="{alt}">',
        f'<ul><li>{orders}</li><li>{out} &amp; {hello}</li></ul>',
        f'<script>var user = "{out}"; if (a < b) {{ go(); }}</script>',
        "</body>",
        "</html>",
    ]
    return nl.join(x for x in lines if x != "") + nl


def write(path, data):
    with open(path, "wb") as fh:
        fh.write(data)


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    # every extension, English
    for name, fmt in [
        ("lumen.phtml", "phtml"), ("lumen.jsp", "jsp"), ("lumen.jspf", "jsp"), ("lumen.aspx", "aspx"), ("lumen.ascx", "aspx"),
        ("lumen.erb", "erb"), ("lumen.shtml", "shtml"), ("lumen.shtm", "shtml"), ("lumen.stm", "shtml"),
    ]:
        write(os.path.join(out, name), page(fmt, "en").encode("utf-8"))
    # one language each, spread over the dialects
    spread = [("ar", "jsp", "jsp"), ("ja", "aspx", "aspx"), ("zh", "phtml", "phtml"), ("ko", "erb", "erb"), ("he", "shtml", "shtml"),
              ("ru", "aspx", "ascx"), ("el", "jsp", "jspf"), ("hi", "shtml", "shtm"), ("th", "phtml", "phtml"), ("de", "erb", "erb")]
    for lang, fmt, ext in spread:
        write(os.path.join(out, f"{lang}-{fmt}.{ext}"), page(fmt, lang).encode("utf-8"))
    # BOM and CRLF, bare CR
    write(os.path.join(out, "crlf-bom.aspx"), b"\xef\xbb\xbf" + page("aspx", "de", "\r\n").encode("utf-8"))
    write(os.path.join(out, "cr-only.erb"), page("erb", "en", "\r").encode("utf-8"))
    # legacy encodings
    cp1252 = page("jsp", "de").replace("charset=\"utf-8\"", "charset=\"windows-1252\"").replace("–", "-").encode("cp1252")
    write(os.path.join(out, "de-cp1252.jsp"), cp1252)
    cp1251 = page("phtml", "ru").replace("charset=\"utf-8\"", "charset=\"windows-1251\"").encode("cp1251")
    write(os.path.join(out, "ru-cp1251.phtml"), cp1251)
    sjis = page("aspx", "ja").replace("charset=\"utf-8\"", "charset=\"shift_jis\"").encode("shift_jis")
    write(os.path.join(out, "ja-sjis.aspx"), sjis)
    u16 = ("﻿" + page("erb", "zh")).encode("utf-16-le")
    write(os.path.join(out, "zh-utf16.erb"), u16)
    # edge pages
    edge = {
        "edge-translate-no.phtml": '<p>One <?= $a ?> <span translate="no">two <?= $b ?> three</span> four <?php echo $c; ?></p><div class="notranslate">Hidden <b>bold</b></div><p translate="no">Skip</p><p translate="yes">Back</p>\n',
        "edge-attr-code.aspx": '<img alt="<%= a %>" title="Plain title" src="<%= s %>"><a title="T <%: x %> U" href="#">go on</a><input value="Go" type=submit>\n<p>Hi <%= a %> there</p>\n',
        "edge-el-escape.jsp": "<p>Back\\<%= a %> and \\${x} text ${y}</p>\n<p>Price: \\${not.el} only</p>\n",
        "edge-erb-literal.erb": "<p>Use <%= a %%> b %> now, <%% marker and <%== raw %> end</p>\n",
        "edge-heredoc.phtml": "<p>Before</p>\n<?php\n$block = <<<EOT\n<p>Not text ?> here</p>\nEOT;\n$n = <<<'RAW'\n</p> still code\nRAW;\n?>\n<p>After <?= $x ?></p>\n",
        "edge-ssi-attr.shtml": '<a href="<!--#echo var="URL" -->" title="Plain">Last updated <!--#echo var="LAST_MODIFIED" --> by the team.</a>\n',
    }
    for name, text in edge.items():
        write(os.path.join(out, name), text.encode("utf-8"))
    print(len(os.listdir(out)), "files")


main()
