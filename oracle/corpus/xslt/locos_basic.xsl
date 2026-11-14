<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE xsl:stylesheet [
  <!ENTITY nbsp "&#160;">
  <!-- copyright sign -->
  <!ENTITY copy '&#xA9;'>
]>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:output method="html" indent="yes"/>
  <xsl:variable name="mode">compact</xsl:variable>
  <xsl:param name="title" select="'Report'"/>
  <xsl:key name="k" match="item" use="@id"/>
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1 title="Main {@id} heading">Welcome to the <b>shop</b>, <xsl:value-of select="user/name"/>!</h1>
        <p>Price:&nbsp;<xsl:value-of select="price"/> {0} %1$s {{x}}</p>
        <img src="{@src}" alt="Photo of {{the}} product"/>
        <xsl:if test="count(item) &gt; 0">
          You have <xsl:value-of select="count(item)"/> items.
        </xsl:if>
        <xsl:message terminate="no">Debug only</xsl:message>
        <xsl:comment>Generated file</xsl:comment>
        <p translate="no">ACME-42</p>
        <p><xsl:attribute name="title">Tooltip text</xsl:attribute><xsl:attribute name="class">big</xsl:attribute>Body</p>
        <table><tr><td>A cell</td><td>B cell</td></tr></table>
        <xsl:for-each select="x"><li><xsl:value-of select="."/></li></xsl:for-each>
        <xsl:choose>
          <xsl:when test="a">Yes branch</xsl:when>
          <xsl:otherwise><xsl:text>No branch</xsl:text></xsl:otherwise>
        </xsl:choose>
        <xsl:call-template name="footer">
          <xsl:with-param name="note">raw-code</xsl:with-param>
          <xsl:with-param name="html"><em>Thanks for reading</em></xsl:with-param>
        </xsl:call-template>
        <footer>&copy; 2026 Example</footer>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
