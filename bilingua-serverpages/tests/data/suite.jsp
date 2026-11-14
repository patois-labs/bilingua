<%@ page contentType="text/html; charset=UTF-8" %>
<%-- page comment with <p>markup</p> --%>
<p class="${cls}">Welcome back, ${user.name}! Today is <%= date %>.</p>
<% for (int i = 0; i < 3; i++) { %><li>Item</li><% } %>
<jsp:scriptlet>String s = "<p>no</p>";</jsp:scriptlet>
<p>Price: \${not.el} only</p>
