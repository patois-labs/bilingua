<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>Bienvenido a la panadería</h1>
        <p>Su pedido tiene <b>llegado</b> <xsl:value-of select="count(item)"/> artículos.</p>
        <img src="{@src}" alt="Foto de pan fresco"/>
        <xsl:if test="a"><xsl:text>Guardar y cerrar</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
