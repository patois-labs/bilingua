<%@ page contentType="text/html; charset=UTF-8" %>
<%@ taglib uri="http://java.sun.com/jsp/jstl/core" prefix="c" %>
<!DOCTYPE html>
<html lang="ar" dir="rtl">
<head><meta charset="utf-8"><title>مرحبا بك ${user.name}</title><meta name="description" content="طلباتك"></head>
<body>
<%-- note: not text --%>
<h1>مرحبا بك, ${user.name}!</h1>
<% if (user != null) { %>
<p>سجل الدخول للمتابعة. إنه سريع. <b>طلباتك</b> ${user.name} <a href="${url}" title="افتح صندوق الوارد">افتح صندوق الوارد</a>.</p>
<% } %>
<img src="logo.png" alt="شعار الشركة">
<ul><li>طلباتك</li><li>${user.name} &amp; مرحبا بك</li></ul>
<script>var user = "${user.name}"; if (a < b) { go(); }</script>
</body>
</html>
