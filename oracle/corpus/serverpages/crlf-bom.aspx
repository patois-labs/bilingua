<%@ Page Language="C#" AutoEventWireup="true" %>
<!DOCTYPE html>
<html lang="de">
<head><meta charset="utf-8"><title>Willkommen zurück <%: Model.Name %></title><meta name="description" content="Ihre Bestellungen"></head>
<body>
<%-- note: not text --%>
<h1>Willkommen zurück, <%: Model.Name %>!</h1>
<% if (User != null) { %>
<p>Melden Sie sich an, um fortzufahren. Es geht schnell – und ist kostenlos. <b>Ihre Bestellungen</b> <%: Model.Name %> <a href="<%= Url %>" title="Posteingang öffnen">Posteingang öffnen</a>.</p>
<% } %>
<img src="logo.png" alt="Firmenlogo">
<ul><li>Ihre Bestellungen</li><li><%: Model.Name %> &amp; Willkommen zurück</li></ul>
<script>var user = "<%: Model.Name %>"; if (a < b) { go(); }</script>
</body>
</html>
