<xsl:stylesheet version="3.0" expand-text="yes"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform" xmlns:map="http://www.w3.org/2005/xpath-functions/map">
  <xsl:template match="order">
    <p>Total: {sum(line/@price)} for {map{'a':1}?a} customer, {{literal}}</p>
    <p xsl:expand-text="no">Raw {braces} stay</p>
    <xsl:text>Line {position()} done</xsl:text>
    <span title="{concat('a}', @x)} items">x</span>
  </xsl:template>
</xsl:stylesheet>
