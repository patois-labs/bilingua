<%@ Page Language="C#" AutoEventWireup="true" %>
<!DOCTYPE html>
<html lang="ru">
<head><meta charset="utf-8"><title>С возвращением <%: Model.Name %></title><meta name="description" content="Ваши заказы"></head>
<body>
<%-- note: not text --%>
<h1>С возвращением, <%: Model.Name %>!</h1>
<% if (User != null) { %>
<p>Войдите, чтобы продолжить. Это быстро. <b>Ваши заказы</b> <%: Model.Name %> <a href="<%= Url %>" title="Открыть почту">Открыть почту</a>.</p>
<% } %>
<img src="logo.png" alt="Логотип">
<ul><li>Ваши заказы</li><li><%: Model.Name %> &amp; С возвращением</li></ul>
<script>var user = "<%: Model.Name %>"; if (a < b) { go(); }</script>
</body>
</html>
