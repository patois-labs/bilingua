<%@ Page Language="C#" AutoEventWireup="true" %>
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Welcome back <%: Model.Name %></title><meta name="description" content="Your orders"></head>
<body>
<%-- note: not text --%>
<h1>Welcome back, <%: Model.Name %>!</h1>
<% if (User != null) { %>
<p>Sign in to continue. It's quick, and it's free. <b>Your orders</b> <%: Model.Name %> <a href="<%= Url %>" title="Open the inbox">Open the inbox</a>.</p>
<% } %>
<img src="logo.png" alt="Company logo">
<ul><li>Your orders</li><li><%: Model.Name %> &amp; Welcome back</li></ul>
<script>var user = "<%: Model.Name %>"; if (a < b) { go(); }</script>
</body>
</html>
