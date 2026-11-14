<%@ Page Language="C#" AutoEventWireup="true" %>
<!DOCTYPE html>
<html lang="ja">
<head><meta charset="utf-8"><title>お帰りなさい <%: Model.Name %></title><meta name="description" content="ご注文"></head>
<body>
<%-- note: not text --%>
<h1>お帰りなさい, <%: Model.Name %>!</h1>
<% if (User != null) { %>
<p>続行するにはサインインしてください。無料です。 <b>ご注文</b> <%: Model.Name %> <a href="<%= Url %>" title="受信箱を開く">受信箱を開く</a>.</p>
<% } %>
<img src="logo.png" alt="会社のロゴ">
<ul><li>ご注文</li><li><%: Model.Name %> &amp; お帰りなさい</li></ul>
<script>var user = "<%: Model.Name %>"; if (a < b) { go(); }</script>
</body>
</html>
