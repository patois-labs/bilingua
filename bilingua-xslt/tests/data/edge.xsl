<?xml version='1.0' encoding='utf-8' standalone="yes"?>
<?pi data?>
<!DOCTYPE xsl:stylesheet [
  <!ENTITY nbsp "&#160;">
  <!ENTITY reg "&#174;">
]>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform" xmlns:its="http://www.w3.org/2005/11/its">
  <!-- the main page -->
  <xsl:template match="/">
    <div class = 'box'  title = 'It&apos;s a "box"'>
      <p>Click <a href="{@href}" title="Open &amp; read">here</a>&nbsp;now<!-- inline note --> and <![CDATA[CDATA <text> kept]]> then &reg; done.</p>
      <p its:translate="no">DO-NOT-TRANSLATE</p>
      <p>Tab	and   many   spaces
          across lines</p>
      <xsl:attribute name="aria-label">Navigation</xsl:attribute>
      <xsl:attribute name="class">nav</xsl:attribute>
      <button placeholder="{{no}} {$x}">Go</button>
      <xsl:element name="note">Element text</xsl:element>
      <xsl:for-each select="a"><li>Item <xsl:number/></li></xsl:for-each>
      <p>5 &lt; 6 &amp;&amp; 7 &gt; 3 &#169; 2026</p>
      <p>   </p>
      <p>12345</p>
    </div>
  </xsl:template>
  <xsl:template match="b">
    <xsl:apply-templates select="c"><xsl:with-param name="x">Param text</xsl:with-param></xsl:apply-templates>
    <xsl:sequence select="'literal'"/>
    <xsl:message>Never <b>shown</b></xsl:message>
  </xsl:template>
</xsl:stylesheet>
