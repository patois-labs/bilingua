<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>Welcome to the bakery</h1>
        <p>Your order has <b>arrived</b> <xsl:value-of select="count(item)"/> items.</p>
        <img src="{@src}" alt="Fresh bread photo"/>
        <xsl:if test="a"><xsl:text>Save and close</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
