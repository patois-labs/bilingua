<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <p>First line
       second line</p>
    <pre xml:space="preserve">keep   this
  layout</pre>
    <xsl:text>  padded  </xsl:text>
  </xsl:template>
</xsl:stylesheet>
