<?xml version="1.0" encoding="windows-1252"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>Bienvenue à la boulangerie</h1>
        <p>Votre commande a <b>arrivé</b> <xsl:value-of select="count(item)"/> articles.</p>
        <img src="{@src}" alt="Photo de pain frais"/>
        <xsl:if test="a"><xsl:text>Enregistrer et fermer</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
