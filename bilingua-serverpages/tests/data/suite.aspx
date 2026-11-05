<%@ Page Language="C#" AutoEventWireup="true" %>
<%-- comment --%>
<h1>Hi <%: Model.Name %>, and <%# Eval("Title") %> is ready.</h1>
<% if (show) { %><p>Shown only sometimes</p><% } %>
<asp:Label runat="server" Text="Server label" />
