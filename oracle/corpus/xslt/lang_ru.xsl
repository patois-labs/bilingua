<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>Добро пожаловать в пекарню</h1>
        <p>Ваш заказ содержит <b>пришёл</b> <xsl:value-of select="count(item)"/> товаров.</p>
        <img src="{@src}" alt="Фото свежего хлеба"/>
        <xsl:if test="a"><xsl:text>Сохранить и закрыть</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
