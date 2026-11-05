<?xml version="1.0" encoding="ISO-8859-1"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>Willkommen in der Bäckerei</h1>
        <p>Ihre Bestellung hat <b>angekommen</b> <xsl:value-of select="count(item)"/> Artikel.</p>
        <img src="{@src}" alt="Foto von frischem Brot"/>
        <xsl:if test="a"><xsl:text>Speichern und schließen</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
