<%@ page contentType="text/html; charset=UTF-8" %>
<%@ taglib uri="http://java.sun.com/jsp/jstl/core" prefix="c" %>
<html><body>
<c:if test="${user != null}">
<p>Welcome back, <c:out value="${user.name}"/>!</p>
</c:if>
<aui:fieldset label="details">
<span>Pick a scope</span>
<liferay-ui:message key="select-scope" />
</aui:fieldset>
<s:form action="save"><s:submit value="Save now"/></s:form>
<p>Footer text <img src="logo.png"></p>
</body></html>
