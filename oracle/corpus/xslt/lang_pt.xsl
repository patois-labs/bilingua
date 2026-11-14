<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>Bem-vindo à padaria</h1>
        <p>Seu pedido tem <b>chegado</b> <xsl:value-of select="count(item)"/> itens.</p>
        <img src="{@src}" alt="Foto de pão fresco"/>
        <xsl:if test="a"><xsl:text>Salvar e fechar</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
