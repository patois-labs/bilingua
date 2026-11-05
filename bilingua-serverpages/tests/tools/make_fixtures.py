"""Writes the fixture pages of the LocOS suite (tests/verify_server_page_filter.py builds them inline) as files in
tests/data/, so the Rust tests and the golden generator read the same bytes.

    python -I make_fixtures.py tests/data
"""

import os
import sys

PHP = (
    '<!DOCTYPE html>\n<html lang="en">\n<head><title><?= $title ?></title></head>\n<body>\n'
    "<?php if ($user): ?>\n"
    "<p>Hello <?= htmlspecialchars($user) ?>, you have <b><?php echo $n; ?></b> new messages.</p>\n"
    "<?php else: ?>\n<p>Please sign in.</p>\n<?php endif; ?>\n"
    '<a href="<?php echo url("a\\"b>"); ?>" title="Open the inbox">Inbox</a>\n'
    '<img src="logo.png" alt="<?= $alt ?>">\n'
    "<?php\n$block = <<<EOT\n<p>Not text ?> here</p>\nEOT;\n// comment with ' quote\n?>\n"
    "<script>var n = <?= json_encode($n) ?>; if (a <b) {}</script>\n"
    "</body>\n</html>\n"
)
JSP = (
    '<%@ page contentType="text/html; charset=UTF-8" %>\n'
    "<%-- page comment with <p>markup</p> --%>\n"
    '<p class="${cls}">Welcome back, ${user.name}! Today is <%= date %>.</p>\n'
    "<% for (int i = 0; i < 3; i++) { %><li>Item</li><% } %>\n"
    '<jsp:scriptlet>String s = "<p>no</p>";</jsp:scriptlet>\n'
    "<p>Price: \\${not.el} only</p>\n"
)
ASPX = (
    '<%@ Page Language="C#" AutoEventWireup="true" %>\n'
    "<%-- comment --%>\n"
    '<h1>Hi <%: Model.Name %>, and <%# Eval("Title") %> is ready.</h1>\n'
    "<% if (show) { %><p>Shown only sometimes</p><% } %>\n"
    '<asp:Label runat="server" Text="Server label" />\n'
)
ERB = (
    "<h1><%= t('.title') %></h1>\n"
    "<% @items.each do |item| -%>\n"
    "<p>Only <%= item.price %> today</p>\n"
    "<% end -%>\n"
    "<%# a comment %>\n"
    "<p>Literal <%% marker and <%== raw %> end</p>\n"
)
SHTML = (
    '<!--#include virtual="/header.html" -->\n'
    '<!--#config timefmt="%Y" -->\n'
    "<p>Last updated <!--#echo var=\"LAST_MODIFIED\" --> by the team.</p>\n"
)
HTML_JSP = (
    '<?xml version="1.0" encoding="UTF-8" ?>\n<%@ page language="java" %>'
    '<%@ taglib uri="/WEB-INF/struts-i18n.tld" prefix="i18n" %>\n'
    "<html><body><p>Hello <%= user %>, welcome.</p><p>Cost ${price} today.</p></body></html>\n"
)
FILES = {
    "suite.phtml": PHP,
    "suite.jsp": JSP,
    "suite.aspx": ASPX,
    "suite.erb": ERB,
    "suite.shtml": SHTML,
    "suite_html_jsp.html": HTML_JSP,
}

out = sys.argv[1]
for name, text in FILES.items():
    with open(os.path.join(out, name), "wb") as fh:
        fh.write(text.encode("utf-8"))
print(len(FILES), "fixtures")
